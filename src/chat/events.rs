//! The one event stream every chat backend (Claude Code, Codex, API endpoints) is turned into.

use super::Usage;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Text(String),
    Thinking(String),
    ToolStart { id: String, name: String, args: Value },
    /// The model began writing a tool call: its input streams in as [`Event::ToolInput`], and
    /// the canvas shows a skeleton where it will land until the call ends.
    ToolDraft { id: String, name: String },
    /// A piece of the input of the call being drafted (raw JSON text).
    ToolInput(String),
    /// `result` on success, `error` on failure; exactly one is set.
    ToolEnd { id: String, result: Option<Value>, error: Option<String> },
    TurnEnd { usage: Option<Usage>, session: Option<String>, stop_reason: Option<String> },
    /// The backend's session id, as soon as it is known (so a stopped turn can still resume).
    Session(String),
    /// API backend: a destructive tool waits for `ApiTurn::approve(id, ..)` before it runs.
    NeedsApproval { id: String, name: String, args: Value },
    Error(String),
    /// Tokens in the context at the backend's latest model call (input and cache), not summed
    /// over the turn: the header meter.
    Context(u64),
    /// The model the agent's session runs on and the ones it offers, as `(id, name)`.
    Models { current: String, options: Vec<(String, String)> },
    /// The selected ACP model's available reasoning levels.
    ModelEfforts { model: String, options: Vec<(String, String)> },
    /// The turn moved from model `from` to `to` because `from` failed with `reason` before doing
    /// anything ("Free (auto)").
    ModelSwitch { from: String, to: String, reason: String },
}

/// Sends a tool call to the UI thread (the app's MCP dispatch), so runs, undo and the agent log
/// behave exactly as for an MCP request. Returns false when the app no longer takes calls.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone)]
pub struct ToolDispatch(pub std::sync::Arc<dyn Fn(crate::mcp::Call) -> bool + Send + Sync>);

#[cfg(not(target_arch = "wasm32"))]
impl ToolDispatch {
    /// Tags every call with the chat, so the turn's edits land in that chat's own run.
    pub fn for_chat(self, chat: &str) -> ToolDispatch {
        let client = crate::mcp::Client::chat(chat);
        ToolDispatch(std::sync::Arc::new(move |mut c: crate::mcp::Call| {
            c.client = client.clone();
            (self.0)(c)
        }))
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl std::fmt::Debug for ToolDispatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ToolDispatch")
    }
}

/// An image the user attached to a message (API backend: sent as an image block).
#[derive(Debug, Clone, PartialEq)]
pub struct ImageAttachment {
    /// `image/png`, `image/jpeg`, ...
    pub mime: String,
    pub data: Vec<u8>,
}

/// What a turn needs from the app, the same for every backend.
#[derive(Debug, Clone, Default)]
pub struct TurnConfig {
    /// The project folder (working directory for CLI agents).
    pub project_dir: std::path::PathBuf,
    /// The app's MCP endpoint, `http://127.0.0.1:<port>/mcp`.
    pub mcp_url: String,
    /// PaperCraft skill text and project context, appended to the system prompt.
    pub system: String,
    /// API backend: the design-system summary, sent after `system` as its own block. CLI and ACP
    /// backends get it inside `system` or in front of the prompt instead.
    pub ds_digest: String,
    /// Model id; None = the backend's default (API: the endpoint's model).
    pub model: Option<String>,
    /// ACP: models to move to, in order, when `model` fails before any tool ran (rate limit,
    /// quota, gone). Filled for "Free (auto)".
    pub fallbacks: Vec<String>,
    /// Reasoning level; None = the backend's default.
    pub effort: Option<String>,
    /// Resume this backend session.
    pub session: Option<String>,
    /// Claude Code: leave out the user's CLAUDE.md, hooks, plugins, skills and unusable tools.
    pub lean: bool,
    /// Web research is on: web search and fetch tools for the agent.
    pub research: bool,
    /// Override for the CLI binary path.
    pub binary: Option<std::path::PathBuf>,
    /// API backend: where tool calls run (see [`ToolDispatch`]).
    #[cfg(not(target_arch = "wasm32"))]
    pub tools: Option<ToolDispatch>,
    /// API backend: the raw messages array, `chats/<id>.api.json`.
    pub api_history: Option<std::path::PathBuf>,
    /// API backend: images attached to the new user message.
    pub images: Vec<ImageAttachment>,
    /// API backend: the endpoint (format, base URL, model).
    pub endpoint: super::endpoint::Endpoint,
    /// API backend: base URL override for the request only (tests); Claude-only fields still follow
    /// `endpoint`. Anthropic format also reads `PAPERCRAFT_ANTHROPIC_BASE_URL`.
    pub api_base_url: Option<String>,
}

/// A running turn. Events arrive on the receiver given to `start`; the last one is `TurnEnd`
/// or `Error`.
pub trait Turn: Send {
    /// Ask the backend to stop; it must end within 3 s (kill after that).
    fn stop(&mut self);
    fn is_running(&self) -> bool;
}

/// Text of a tool result: a plain string, or the `text` of each content block joined.
pub(crate) fn content_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Array(a) => a.iter().filter_map(|b| b["text"].as_str()).collect::<Vec<_>>().join("\n"),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

pub(crate) fn str_of(v: &Value) -> String {
    v.as_str().unwrap_or_default().to_string()
}
