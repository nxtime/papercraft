//! Chat: a conversation with a coding agent (Claude Code, Codex or an API endpoint). Each chat
//! is one JSON file under the project's `chats/` folder, see [`store`].

pub mod agents;
#[cfg(not(target_arch = "wasm32"))]
pub mod acp;
pub mod anthropic;
pub mod claude_code;
pub mod codex;
pub mod endpoint;
pub mod events;
#[cfg(not(target_arch = "wasm32"))]
pub mod openai;
#[cfg(not(target_arch = "wasm32"))]
pub mod path;
pub mod store;
#[cfg(not(target_arch = "wasm32"))]
pub mod tools;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Where images for CLI backends and the system prompt file are saved, under the project folder.
pub const TMP_DIR: &str = ".papercraft-chat-tmp";
/// Each agent's own setup page, linked from a missing agent's row.
pub const CLAUDE_DOCS: &str = "https://docs.anthropic.com/en/docs/claude-code/setup";
pub const CODEX_DOCS: &str = "https://github.com/openai/codex";
const ID_MAX: usize = 64;

/// Chat ids become file and folder names, so only ascii letters, digits, `-` and `_` pass: no
/// path separators, no `.`.
pub fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= ID_MAX && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackendKind {
    #[default]
    ClaudeCode,
    Codex,
    /// An API endpoint (any format); the name is kept for saved chats.
    Anthropic,
    /// Any agent that speaks the Agent Client Protocol; [`Backend::agent`] says which.
    Acp,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Backend {
    pub kind: BackendKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    /// API chats: the endpoint's id; None = the default endpoint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    /// The CLI's session id, so a follow-up message resumes the same session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cli_session: Option<String>,
    /// `Acp` chats: the registry id (`agents::AGENTS`) or custom agent id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Usage {
    #[serde(default)]
    pub input: u64,
    #[serde(default)]
    pub output: u64,
    #[serde(default)]
    pub cache_read: u64,
    #[serde(default)]
    pub cache_write: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_usd: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ToolCall {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub args: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Milliseconds the tool took.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ms: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    #[default]
    User,
    Assistant,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Message {
    #[serde(default)]
    pub role: Role,
    #[serde(default)]
    pub text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<ToolCall>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
    /// Tokens in the context at the turn's last model call (input and cache), for the header meter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<u64>,
    /// Unix milliseconds.
    #[serde(default)]
    pub at: u64,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Chat {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    /// Unix milliseconds.
    #[serde(default)]
    pub created: u64,
    /// Unix milliseconds; `list` sorts on it.
    #[serde(default)]
    pub updated: u64,
    #[serde(default)]
    pub backend: Backend,
    #[serde(default)]
    pub messages: Vec<Message>,
}
