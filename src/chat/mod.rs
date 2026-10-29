//! Chat: a conversation with a coding agent (Claude Code, Codex or an API endpoint). Each chat
//! is one JSON file under the project's `chats/` folder, see [`store`].

pub mod agents;
#[cfg(not(target_arch = "wasm32"))]
pub mod acp;
pub mod anthropic;
pub mod claude_code;
pub mod codex;
pub mod ds_digest;
pub mod endpoint;
pub mod events;
pub mod install;
pub mod keys;
pub mod models;
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
/// Long edge of a `zoom_image` crop, the same cap as a sent attachment.
pub const ZOOM_MAX_EDGE: u32 = 1568;

/// Attachments are sent at most this big (about 1.6k image tokens; full size costs up to ~4.8k).
/// The original is kept for `zoom_image` when it was bigger.
const IMAGE_MAX_EDGE: f64 = ZOOM_MAX_EDGE as f64;
const IMAGE_MAX_PIXELS: f64 = 1_150_000.0;
/// Only used when a JPEG had to be scaled down: high enough that re-encoding is invisible.
const IMAGE_JPEG_QUALITY: u8 = 95;
/// An opaque PNG (a screenshot, a photo) is also tried as a JPEG this good; text stays legible.
const IMAGE_LOSSY_QUALITY: u8 = 82;

/// Scales an image down to the send cap, then re-encodes it: PNG at the best lossless
/// compression, JPEG stays JPEG (a PNG of a photo is often bigger than the original), and an
/// opaque PNG becomes a JPEG when that is smaller. Keeps the original bytes when they are
/// already smaller. The third value is the original when scaled.
pub fn shrink_image(name: String, img: events::ImageAttachment) -> (String, events::ImageAttachment, Option<Vec<u8>>) {
    use image::codecs::{jpeg::JpegEncoder, png};
    let Ok(decoded) = image::load_from_memory(&img.data) else { return (name, img, None) };
    let (w, h) = (f64::from(decoded.width()), f64::from(decoded.height()));
    let k = (IMAGE_MAX_EDGE / w.max(h)).min((IMAGE_MAX_PIXELS / (w * h)).sqrt());
    if k >= 1.0 && img.mime == "image/jpeg" {
        return (name, img, None); // re-encoding a JPEG loses detail for little gain
    }
    let decoded = if k < 1.0 { decoded.resize((w * k) as u32, (h * k) as u32, image::imageops::FilterType::Lanczos3) } else { decoded };
    let mut jpeg = img.mime == "image/jpeg";
    let mut out = Vec::new();
    let mut encoded = if jpeg {
        decoded.to_rgb8().write_with_encoder(JpegEncoder::new_with_quality(&mut out, IMAGE_JPEG_QUALITY))
    } else {
        decoded.write_with_encoder(png::PngEncoder::new_with_quality(&mut out, png::CompressionType::Best, png::FilterType::Adaptive))
    };
    let opaque = || !decoded.color().has_alpha() || decoded.to_rgba8().pixels().all(|px| px[3] == u8::MAX);
    if !jpeg && opaque() {
        let mut lossy = Vec::new();
        if decoded.to_rgb8().write_with_encoder(JpegEncoder::new_with_quality(&mut lossy, IMAGE_LOSSY_QUALITY)).is_ok() && (encoded.is_err() || lossy.len() < out.len()) {
            (out, encoded, jpeg) = (lossy, Ok(()), true);
        }
    }
    if encoded.is_err() || (k >= 1.0 && out.len() >= img.data.len()) {
        return (name, img, None);
    }
    let ext = if jpeg { "jpg" } else { "png" };
    let name = std::path::Path::new(&name).with_extension(ext).to_string_lossy().into_owned();
    let full = (k < 1.0).then_some(img.data);
    (name, events::ImageAttachment { mime: if jpeg { "image/jpeg" } else { "image/png" }.into(), data: out }, full)
}

/// Where the original of a scaled-down attachment is kept: `shot.png` -> `shot.full.png`.
pub fn full_image_path(sent: &std::path::Path) -> std::path::PathBuf {
    let ext = sent.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default();
    sent.with_extension(format!("full.{ext}"))
}
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
    /// Claude Code: the session's running cost as the CLI reported it (`total_cost_usd` adds up
    /// over a resumed session); `usage.cost_usd` holds this turn's share.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_cost: Option<f64>,
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
    /// What became of the issues drafted in this chat, by the drafting tool call's id.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub issues: std::collections::BTreeMap<String, IssueOutcome>,
}

/// A chat as the history list shows it, without its messages: a long chat's screenshots run to
/// hundreds of MB, and the list must not hold a copy of every chat.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
pub struct ChatSummary {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub updated: u64,
    #[serde(default)]
    pub backend: Backend,
}

impl ChatSummary {
    pub fn of(c: &Chat) -> Self {
        Self { id: c.id.clone(), title: c.title.clone(), updated: c.updated, backend: c.backend.clone() }
    }
}

/// A drafted issue the user sent or dropped.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "lowercase")]
pub enum IssueOutcome {
    /// Filed as #number; `existing` when an issue with a matching title was there already.
    Filed { number: u64, url: String, existing: bool },
    Discarded,
}
