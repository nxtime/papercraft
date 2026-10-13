//! Tab side of the relay: one WebSocket to `papercraft-relay` that carries agent tool calls in
//! and results out (frames in `mcp::relay_proto`). The tab holds the document and the executor;
//! the relay holds nothing, so reconnecting just resumes.
//!
//! The socket callbacks only queue work into an [`Inbox`]; the shell drains it at the start of
//! every frame (`Web::pump_calls`) so edits and repaint stay on one code path.

use super::dom;
use crate::mcp::relay_proto::{self, Call, Reply, TOKEN_BYTES};
use crate::ui::agent::LinkState;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use web_sys::{MessageEvent, WebSocket};

const TOKEN_KEY: &str = "papercraft.relay.token";
const BACKOFF_MIN_MS: f64 = 1_000.0;
const BACKOFF_MAX_MS: f64 = 15_000.0;
/// WebSocket close codes: normal closure, and "going away" for a replaced socket.
const CLOSE_NORMAL: u16 = 1000;

#[derive(Default)]
struct Inbox {
    calls: VecDeque<Call>,
    state: LinkState,
    /// Set by the callbacks for the shell's `tick` to act on.
    opened: bool,
    closed: bool,
    /// Which `connect` the callbacks belong to; stale sockets are ignored.
    generation: u32,
    /// Called when a call arrives (the shell uses it to keep working in a hidden tab, where
    /// `requestAnimationFrame` is paused).
    wake: Option<Rc<dyn Fn()>>,
}

type Callbacks = (
    Closure<dyn FnMut()>,
    Closure<dyn FnMut(MessageEvent)>,
    Closure<dyn FnMut()>,
);

pub struct Relay {
    token: String,
    base: String,
    inbox: Rc<RefCell<Inbox>>,
    socket: Option<WebSocket>,
    _callbacks: Option<Callbacks>,
    backoff_ms: f64,
    retry_at: Option<f64>,
}

fn new_token() -> String {
    let mut bytes = [0u8; TOKEN_BYTES];
    let filled = super::window()
        .crypto()
        .is_ok_and(|c| c.get_random_values_with_u8_array(&mut bytes).is_ok());
    if !filled {
        // No Web Crypto (very old browser or insecure context): fall back to the seeded PRNG.
        bytes.iter_mut().for_each(|b| *b = fastrand::u8(..));
    }
    relay_proto::hex(&bytes)
}

impl Relay {
    pub fn new() -> Relay {
        let loc = super::window().location();
        let base = relay_proto::relay_base(
            &loc.search().unwrap_or_default(),
            dom::relay_override().as_deref(),
            &loc.origin().unwrap_or_default(),
        );
        let token = dom::local_get(TOKEN_KEY)
            .filter(|t| relay_proto::valid_token(t))
            .unwrap_or_else(|| {
                let t = new_token();
                dom::local_set(TOKEN_KEY, &t);
                t
            });
        Relay {
            token,
            base,
            inbox: Rc::default(),
            socket: None,
            _callbacks: None,
            backoff_ms: BACKOFF_MIN_MS,
            retry_at: Some(0.0),
        }
    }

    pub fn set_wake(&self, wake: Rc<dyn Fn()>) {
        self.inbox.borrow_mut().wake = Some(wake);
    }

    pub fn state(&self) -> LinkState {
        self.inbox.borrow().state
    }

    pub fn next_call(&mut self) -> Option<Call> {
        self.inbox.borrow_mut().calls.pop_front()
    }

    /// The URL to hand to an agent. Contains the secret token.
    pub fn agent_url(&self) -> String {
        relay_proto::agent_url(&relay_proto::http_origin(&self.base), &self.token)
    }

    /// New token (the old link stops working) and a fresh connection.
    pub fn regenerate(&mut self, now: f64) {
        self.token = new_token();
        dom::local_set(TOKEN_KEY, &self.token);
        self.backoff_ms = BACKOFF_MIN_MS;
        self.connect(now);
    }

    /// Answer a call. Dropped silently when the socket is gone: the relay already told the agent.
    pub fn send(&self, reply: &Reply) {
        let Some(ws) = &self.socket else { return };
        match serde_json::to_string(reply) {
            Ok(json) => {
                if ws.send_with_str(&json).is_err() {
                    log::warn!("papercraft relay: could not send a reply");
                }
            }
            Err(e) => log::warn!("papercraft relay: {e}"),
        }
    }

    /// Drive reconnection; call every frame. True when something changed that needs a repaint.
    pub fn tick(&mut self, now: f64) -> bool {
        let (opened, closed) = {
            let mut i = self.inbox.borrow_mut();
            (std::mem::take(&mut i.opened), std::mem::take(&mut i.closed))
        };
        if opened {
            self.backoff_ms = BACKOFF_MIN_MS;
        }
        if closed {
            self.retry_at = Some(now + self.backoff_ms);
            self.backoff_ms = (self.backoff_ms * 2.0).min(BACKOFF_MAX_MS);
        }
        if self.retry_at.is_some_and(|t| now >= t) {
            self.connect(now);
        }
        opened || closed
    }

    fn connect(&mut self, now: f64) {
        self.retry_at = None;
        self.detach();
        let generation = {
            let mut i = self.inbox.borrow_mut();
            i.generation += 1;
            i.state = LinkState::Connecting;
            i.generation
        };
        let url = relay_proto::tab_url(&self.base, &self.token);
        let ws = match WebSocket::new(&url) {
            Ok(ws) => ws,
            Err(_) => {
                // Never log `url`: it carries the token.
                log::warn!(
                    "papercraft relay: cannot open a connection to {}",
                    self.base
                );
                self.inbox.borrow_mut().state = LinkState::Offline;
                self.retry_at = Some(now + self.backoff_ms);
                self.backoff_ms = (self.backoff_ms * 2.0).min(BACKOFF_MAX_MS);
                return;
            }
        };
        let current = move |inbox: &Rc<RefCell<Inbox>>| inbox.borrow().generation == generation;

        let inbox = self.inbox.clone();
        let onopen = Closure::<dyn FnMut()>::new(move || {
            if current(&inbox) {
                let mut i = inbox.borrow_mut();
                i.state = LinkState::Connected;
                i.opened = true;
            }
        });
        let inbox = self.inbox.clone();
        let onmessage = Closure::<dyn FnMut(MessageEvent)>::new(move |e: MessageEvent| {
            if !current(&inbox) {
                return;
            }
            let Some(text) = e.data().as_string() else {
                return;
            };
            let Ok(call) = serde_json::from_str::<Call>(&text) else {
                log::warn!("papercraft relay: ignoring a frame that is not a call");
                return;
            };
            let wake = {
                let mut i = inbox.borrow_mut();
                i.calls.push_back(call);
                i.wake.clone()
            };
            if let Some(wake) = wake {
                wake();
            }
        });
        let inbox = self.inbox.clone();
        let onclose = Closure::<dyn FnMut()>::new(move || {
            if current(&inbox) {
                let mut i = inbox.borrow_mut();
                i.state = LinkState::Offline;
                i.closed = true;
            }
        });
        ws.set_onopen(Some(onopen.as_ref().unchecked_ref()));
        ws.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
        // An error is always followed by a close event, which handles the retry.
        ws.set_onclose(Some(onclose.as_ref().unchecked_ref()));
        self.socket = Some(ws);
        self._callbacks = Some((onopen, onmessage, onclose));
    }

    fn detach(&mut self) {
        if let Some(ws) = self.socket.take() {
            ws.set_onopen(None);
            ws.set_onmessage(None);
            ws.set_onclose(None);
            let _ = ws.close_with_code_and_reason(CLOSE_NORMAL, "reconnecting");
        }
        self._callbacks = None;
    }
}
