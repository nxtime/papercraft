#![cfg_attr(not(target_os = "android"), allow(dead_code))]
//! The phone's side of the relay: what `web/relay.rs` is to a browser tab. One WebSocket to the
//! owner's `papercraft-relay` brings in tool calls (run by the app like any MCP call) and the
//! chat events of the agents the server hosts, and carries replies and chat frames out.
//!
//! The frame handling and the settings helpers are plain functions so the host tests cover them;
//! the socket itself lives on its own thread (Android only).

use crate::chat::events::Event;
use crate::mcp::relay_proto::{self, AgentInfo, Call, ToTab};
use std::path::Path;

/// What the socket thread hands the app besides tool calls.
#[derive(Debug, PartialEq)]
pub enum Inbound {
    /// The agents the server runs (sent on every connect).
    Agents(Vec<AgentInfo>),
    /// One event of the turn answering the chat with this id.
    Chat(String, Event),
    /// The socket closed or could not open.
    Down,
}

/// A frame from the relay: chat frames carry a `type`, a tool call has none.
#[derive(Debug, PartialEq)]
pub enum Frame {
    Chat(Inbound),
    Call(Call),
}

pub fn classify(text: &str) -> Option<Frame> {
    if let Ok(frame) = serde_json::from_str::<ToTab>(text) {
        return Some(Frame::Chat(match frame {
            ToTab::Hello { agents } => Inbound::Agents(agents),
            ToTab::Chat { chat, event } => Inbound::Chat(chat, event),
        }));
    }
    serde_json::from_str::<Call>(text).ok().map(Frame::Call)
}

/// The WebSocket base for what the person typed: `wss://host` (or `ws://`), with `https://` and
/// `http://` accepted too. None when it is not an address yet.
pub fn base_url(typed: &str) -> Option<String> {
    let t = typed.trim().trim_end_matches('/');
    let ws = t
        .replacen("https://", "wss://", 1)
        .replacen("http://", "ws://", 1);
    let host = ws
        .strip_prefix("wss://")
        .or_else(|| ws.strip_prefix("ws://"))?;
    (!host.is_empty() && !host.contains(char::is_whitespace)).then_some(ws)
}

/// This device's link token: kept in `dir` so the server sees the same tab after a restart.
pub fn token_in(dir: &Path) -> String {
    let path = dir.join("relay_token");
    if let Some(t) = std::fs::read_to_string(&path)
        .ok()
        .map(|t| t.trim().to_owned())
        .filter(|t| relay_proto::valid_token(t))
    {
        return t;
    }
    let mut bytes = [0u8; relay_proto::TOKEN_BYTES];
    getrandom::fill(&mut bytes).expect("the operating system has no random number generator");
    let token = relay_proto::hex(&bytes);
    if let Err(e) = std::fs::create_dir_all(dir).and_then(|()| std::fs::write(&path, &token)) {
        log::warn!("papercraft relay: cannot keep the device token: {e}");
    }
    token
}

#[cfg(target_os = "android")]
pub use client::RelayTab;

#[cfg(target_os = "android")]
mod client {
    use super::{Frame, Inbound, classify};
    use crate::mcp::relay_proto::{self, FromTab, Reply};
    use crate::mcp::{Call, Client};
    use futures_util::{SinkExt, StreamExt};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc::{Receiver, Sender, channel};
    use std::time::Duration;
    use tokio::sync::Notify;
    use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
    use tokio_tungstenite::tungstenite::Message;
    use tokio_tungstenite::{Connector, connect_async_tls_with_config};

    // Same pacing as the browser tab.
    const BACKOFF_MIN: Duration = Duration::from_millis(1_000);
    const BACKOFF_MAX: Duration = Duration::from_millis(15_000);

    /// A running connection; dropping it closes the socket and ends the thread.
    pub struct RelayTab {
        /// Tool calls, ready for `App::run_mcp_calls`.
        pub calls: Receiver<Call>,
        pub inbound: Receiver<Inbound>,
        out: UnboundedSender<String>,
        stop: Arc<Notify>,
        connected: Arc<AtomicBool>,
    }

    struct Ctx {
        url: String,
        calls: Sender<Call>,
        inbound: Sender<Inbound>,
        out: UnboundedSender<String>,
        out_rx: UnboundedReceiver<String>,
        stop: Arc<Notify>,
        connected: Arc<AtomicBool>,
        wake: Arc<dyn Fn() + Send + Sync>,
    }

    impl RelayTab {
        /// Connects to `base` (`wss://host`) as the tab `token`; `wake` runs when something arrives.
        pub fn start(base: &str, token: &str, wake: Arc<dyn Fn() + Send + Sync>) -> RelayTab {
            let (calls_tx, calls) = channel();
            let (inbound_tx, inbound) = channel();
            let (out, out_rx) = unbounded_channel();
            let (stop, connected) = (Arc::new(Notify::new()), Arc::new(AtomicBool::new(false)));
            let ctx = Ctx {
                url: relay_proto::tab_url(base, token),
                calls: calls_tx,
                inbound: inbound_tx,
                out: out.clone(),
                out_rx,
                stop: stop.clone(),
                connected: connected.clone(),
                wake,
            };
            let spawned = std::thread::Builder::new()
                .name("relay-tab".into())
                .spawn(move || {
                    match tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                    {
                        Ok(rt) => rt.block_on(run(ctx)),
                        Err(e) => log::warn!("papercraft relay: no runtime: {e}"),
                    }
                });
            if let Err(e) = spawned {
                log::warn!("papercraft relay: no thread: {e}");
            }
            RelayTab {
                calls,
                inbound,
                out,
                stop,
                connected,
            }
        }

        pub fn connected(&self) -> bool {
            self.connected.load(Ordering::SeqCst)
        }

        pub fn send_chat(&self, frame: &FromTab) {
            match serde_json::to_string(frame) {
                Ok(json) => {
                    let _ = self.out.send(json);
                }
                Err(e) => log::warn!("papercraft relay: {e}"),
            }
        }
    }

    impl Drop for RelayTab {
        fn drop(&mut self) {
            self.stop.notify_one();
        }
    }

    async fn run(mut c: Ctx) {
        let connector = Connector::Rustls(Arc::new(crate::net::tls_config()));
        let stop = c.stop.clone();
        let mut backoff = BACKOFF_MIN;
        loop {
            let opened = tokio::select! {
                _ = stop.notified() => return,
                opened = session(&mut c, connector.clone()) => opened,
            };
            c.connected.store(false, Ordering::SeqCst);
            let _ = c.inbound.send(Inbound::Down);
            (c.wake)();
            if opened {
                backoff = BACKOFF_MIN;
            }
            tokio::select! {
                _ = stop.notified() => return,
                _ = tokio::time::sleep(backoff) => {}
            }
            backoff = (backoff * 2).min(BACKOFF_MAX);
        }
    }

    /// One connection, until it closes. True when it got as far as opening.
    async fn session(c: &mut Ctx, connector: Connector) -> bool {
        // The URL carries the token: neither it nor an error that may quote it is logged.
        let Ok((ws, _)) =
            connect_async_tls_with_config(c.url.as_str(), None, false, Some(connector)).await
        else {
            log::warn!("papercraft relay: cannot connect to the server");
            return false;
        };
        // Replies to calls of a dead connection mean nothing to the new one.
        while c.out_rx.try_recv().is_ok() {}
        c.connected.store(true, Ordering::SeqCst);
        let (mut sink, mut stream) = ws.split();
        loop {
            tokio::select! {
                msg = stream.next() => match msg {
                    Some(Ok(Message::Text(text))) => handle(c, text.as_str()),
                    Some(Ok(Message::Close(_)) | Err(_)) | None => break,
                    Some(Ok(_)) => {}
                },
                out = c.out_rx.recv() => match out {
                    Some(text) => if sink.send(Message::text(text)).await.is_err() { break },
                    None => break,
                },
            }
        }
        true
    }

    fn handle(c: &Ctx, text: &str) {
        match classify(text) {
            Some(Frame::Chat(m)) => {
                let _ = c.inbound.send(m);
            }
            Some(Frame::Call(call)) => {
                let (reply, answer) = tokio::sync::oneshot::channel();
                let id = call.id;
                if c.calls
                    .send(Call {
                        name: call.name,
                        args: call.args,
                        client: Client::default(),
                        reply,
                    })
                    .is_err()
                {
                    return;
                }
                let out = c.out.clone();
                tokio::spawn(async move {
                    let reply = match answer.await {
                        Ok(Ok(blocks)) => Reply::ok(id, blocks),
                        Ok(Err(e)) => Reply::err(id, e),
                        Err(_) => Reply::err(id, "PaperCraft dropped the call"),
                    };
                    if let Ok(json) = serde_json::to_string(&reply) {
                        let _ = out.send(json);
                    }
                });
            }
            None => {
                log::warn!("papercraft relay: ignoring a frame that is not a call");
                return;
            }
        }
        (c.wake)();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn frames_are_told_apart_by_their_type() {
        let hello =
            json!({"type": "hello", "agents": [{"id": "claude-code", "name": "Claude Code"}]})
                .to_string();
        let Some(Frame::Chat(Inbound::Agents(a))) = classify(&hello) else {
            panic!("hello")
        };
        assert_eq!(a[0].id, "claude-code");
        let chat = json!({"type": "chat", "chat": "c1", "event": {"Text": "hi"}}).to_string();
        assert!(
            matches!(classify(&chat), Some(Frame::Chat(Inbound::Chat(c, _))) if c == "c1"),
            "{chat}"
        );
        let call = json!({"id": 7, "name": "list_pages", "args": {}}).to_string();
        assert_eq!(
            classify(&call),
            Some(Frame::Call(Call {
                id: 7,
                name: "list_pages".into(),
                args: json!({})
            }))
        );
        assert_eq!(classify("{\"nonsense\":1}"), None);
        assert_eq!(classify("not json"), None);
    }

    #[test]
    fn the_typed_address_becomes_a_websocket_base() {
        assert_eq!(
            base_url(" wss://pc.example.com/ ").as_deref(),
            Some("wss://pc.example.com")
        );
        assert_eq!(
            base_url("https://pc.example.com").as_deref(),
            Some("wss://pc.example.com")
        );
        assert_eq!(
            base_url("http://10.0.0.2:8080").as_deref(),
            Some("ws://10.0.0.2:8080")
        );
        for bad in ["", "wss://", "pc.example.com", "ftp://x", "wss://a b"] {
            assert_eq!(base_url(bad), None, "{bad}");
        }
    }

    #[test]
    fn the_device_token_is_valid_and_kept() {
        let dir = std::env::temp_dir().join(format!("pc-relay-token-{}", std::process::id()));
        let first = token_in(&dir);
        assert!(relay_proto::valid_token(&first));
        assert_eq!(token_in(&dir), first);
        std::fs::write(dir.join("relay_token"), "short").unwrap();
        assert!(relay_proto::valid_token(&token_in(&dir)));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
