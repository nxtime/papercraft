//! Pausing an external MCP client from the canvas.
//!
//! While a client is paused its editing calls wait here, untouched, in arrival order; reads go
//! on. A call is only ever held *before* it starts, so none is half applied: on resume it runs
//! as it would have, and when it has waited [`PAUSE_WAIT`] it is answered with a clear error
//! (before the MCP server's own [`crate::mcp::server::CALL_TIMEOUT`] gives up on its own) so the
//! agent can say so and retry later. The in-app chat is never held: it has its own stop.

use crate::mcp::{Call, CHAT_CLIENT, Client, ToolResult};
use std::collections::BTreeMap;
use std::time::Duration;
use web_time::Instant;

/// How long a paused client's call waits before it is refused. Under the server's call timeout,
/// so the agent gets this message and not a generic "did not answer in time".
pub const PAUSE_WAIT: Duration = Duration::from_secs(100);
/// A client that made no call for this long no longer shows a pause control (unless paused).
const RECENT: Duration = Duration::from_secs(120);

/// What the canvas shows for one client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chip {
    pub key: String,
    pub label: String,
    pub paused: bool,
    /// Editing calls waiting for a resume.
    pub waiting: usize,
}

struct Known {
    label: String,
    last_call: Instant,
    paused: bool,
}

/// Which clients are paused, and the calls waiting for them.
#[derive(Default)]
pub struct Pauses {
    clients: BTreeMap<String, Known>,
    waiting: Vec<(Instant, Call)>,
}

/// The error a refused call carries.
pub fn refusal() -> String {
    format!(
        "paused by the user: the person using PaperCraft paused this agent, and the call waited {}s without a resume. \
         Nothing was changed. Tell them, and retry once they resume you; reads still work meanwhile.",
        PAUSE_WAIT.as_secs()
    )
}

fn answer(call: Call, result: ToolResult) {
    if call.reply.send(result).is_err() {
        // The agent gave up on its own timeout first; nothing is left to tell.
        log::debug!("mcp {}: caller gone before the reply", call.name);
    }
}

impl Pauses {
    /// Pauses `key`, or resumes it when it is paused. The chat client cannot be paused.
    pub fn toggle(&mut self, key: &str) {
        if let Some(k) = self.clients.get_mut(key) {
            k.paused = !k.paused;
        }
    }

    pub fn is_paused(&self, key: &str) -> bool {
        self.clients.get(key).is_some_and(|k| k.paused)
    }

    pub fn has_waiting(&self) -> bool {
        !self.waiting.is_empty()
    }

    /// The clients to offer a control for: paused ones, and ones that called recently.
    pub fn chips(&self, now: Instant) -> Vec<Chip> {
        self.clients
            .iter()
            .filter(|(_, k)| k.paused || now.duration_since(k.last_call) < RECENT)
            .map(|(key, k)| Chip {
                key: key.clone(),
                label: k.label.clone(),
                paused: k.paused,
                waiting: self.waiting.iter().filter(|(_, c)| &c.client.key() == key).count(),
            })
            .collect()
    }

    /// Takes the calls that arrived and returns the ones to run now, in order: first those that
    /// waited for a client since resumed, then the new ones that are not held. Calls that waited
    /// too long are answered with [`refusal`]. `label` names a client for its control.
    pub fn admit(&mut self, calls: Vec<Call>, now: Instant, mut label: impl FnMut(&Client) -> String) -> Vec<Call> {
        let mut run = Vec::new();
        for (since, call) in std::mem::take(&mut self.waiting) {
            if self.is_paused(&call.client.key()) {
                if now.duration_since(since) >= PAUSE_WAIT {
                    answer(call, Err(refusal()));
                } else {
                    self.waiting.push((since, call));
                }
            } else {
                run.push(call);
            }
        }
        for call in calls {
            if call.client.name == CHAT_CLIENT {
                run.push(call);
                continue;
            }
            let key = call.client.key();
            let known = self.clients.entry(key.clone()).or_insert_with(|| Known { label: label(&call.client), last_call: now, paused: false });
            known.last_call = now;
            if known.paused && crate::mcp::tools::edits_doc(&call.name, &call.args) {
                self.waiting.push((now, call));
            } else {
                run.push(call);
            }
        }
        self.clients.retain(|_, k| k.paused || now.duration_since(k.last_call) < RECENT * 5);
        run
    }

    /// Answers every waiting call with `error` (the server is going away).
    pub fn fail_all(&mut self, error: &str) {
        for (_, call) in std::mem::take(&mut self.waiting) {
            answer(call, Err(error.into()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tokio::sync::oneshot::{Receiver, channel};

    fn call(client: &str, name: &str) -> (Call, Receiver<ToolResult>) {
        let (reply, rx) = channel();
        let client = Client { name: client.into(), session: "s1".into(), parent: None };
        (Call { name: name.into(), args: json!({}), client, reply }, rx)
    }

    fn names(calls: &[Call]) -> Vec<&str> {
        calls.iter().map(|c| c.name.as_str()).collect()
    }

    fn admit(p: &mut Pauses, calls: Vec<Call>, now: Instant) -> Vec<Call> {
        p.admit(calls, now, |c| c.name.clone())
    }

    /// A paused Claude Code with one edit waiting; returns its key.
    fn paused_with_edit(p: &mut Pauses, t0: Instant) -> (String, Receiver<ToolResult>) {
        let (hello, _) = call("Claude Code", "get_basic_info");
        let key = hello.client.key();
        admit(p, vec![hello], t0);
        p.toggle(&key);
        let (edit, rx) = call("Claude Code", "update_styles");
        assert!(admit(p, vec![edit], t0).is_empty());
        (key, rx)
    }

    #[test]
    fn a_paused_client_waits_on_edits_but_reads_go_on() {
        let (mut p, t0) = (Pauses::default(), Instant::now());
        let (first, _) = call("Claude Code", "get_basic_info");
        let key = first.client.key();
        admit(&mut p, vec![first], t0);
        p.toggle(&key);
        let (a, _ra) = call("Claude Code", "update_styles");
        let (b, _rb) = call("Claude Code", "get_node_info");
        let (c, _rc) = call("Claude Code", "move_nodes");
        let (other, _ro) = call("Cursor", "update_styles");
        let ran = admit(&mut p, vec![a, b, c, other], t0);
        assert_eq!(names(&ran), ["get_node_info", "update_styles"], "the read and another client's edit run");
        assert!(p.has_waiting());
        assert_eq!(p.chips(t0).iter().find(|c| c.key == key).map(|c| (c.paused, c.waiting)), Some((true, 2)));
    }

    #[test]
    fn resuming_runs_the_waiting_edits_first_and_in_order() {
        let (mut p, t0) = (Pauses::default(), Instant::now());
        let (key, _rx) = paused_with_edit(&mut p, t0);
        let (second, _r2) = call("Claude Code", "move_nodes");
        admit(&mut p, vec![second], t0);
        p.toggle(&key);
        let (fresh, _r3) = call("Claude Code", "delete_nodes");
        let ran = admit(&mut p, vec![fresh], t0 + Duration::from_secs(5));
        assert_eq!(names(&ran), ["update_styles", "move_nodes", "delete_nodes"]);
        assert!(!p.has_waiting());
    }

    #[test]
    fn a_call_that_waits_too_long_gets_the_paused_error_and_is_not_run() {
        let (mut p, t0) = (Pauses::default(), Instant::now());
        let (_key, mut rx) = paused_with_edit(&mut p, t0);
        assert!(admit(&mut p, Vec::new(), t0 + PAUSE_WAIT - Duration::from_secs(1)).is_empty());
        assert!(rx.try_recv().is_err(), "still waiting");
        assert!(admit(&mut p, Vec::new(), t0 + PAUSE_WAIT).is_empty());
        let err = rx.try_recv().unwrap().unwrap_err();
        assert!(err.starts_with("paused by the user") && err.contains("Nothing was changed"), "{err}");
        assert!(!p.has_waiting(), "answered once, not kept");
        assert!(PAUSE_WAIT < crate::mcp::server::CALL_TIMEOUT);
    }

    #[test]
    fn the_chat_is_never_held_and_the_server_going_away_answers_the_waiting() {
        let (mut p, t0) = (Pauses::default(), Instant::now());
        let (chat, _r) = call(CHAT_CLIENT, "update_styles");
        let key = chat.client.key();
        let ran = admit(&mut p, vec![chat], t0);
        p.toggle(&key);
        assert_eq!((ran.len(), p.is_paused(&key)), (1, false));
        let (_key, mut rx) = paused_with_edit(&mut p, t0);
        p.fail_all("papercraft is shutting down");
        assert_eq!(rx.try_recv().unwrap().unwrap_err(), "papercraft is shutting down");
    }

    #[test]
    fn idle_clients_lose_their_control_unless_paused() {
        let (mut p, t0) = (Pauses::default(), Instant::now());
        let (a, _ra) = call("Claude Code", "get_basic_info");
        let (b, _rb) = call("Cursor", "get_basic_info");
        let key_b = b.client.key();
        admit(&mut p, vec![a, b], t0);
        p.toggle(&key_b);
        let later = t0 + RECENT + Duration::from_secs(1);
        let chips = p.chips(later);
        assert_eq!(chips.iter().map(|c| c.label.as_str()).collect::<Vec<_>>(), ["Cursor"]);
    }
}
