//! MCP server: AI agents read and edit the canvas of the running app.
//!
//! Modeled on Paper's MCP server (behaviour notes in `analysis/paper-mcp.md`): the app serves
//! MCP over streamable HTTP on loopback, and `papercraft-mcp` bridges stdio clients to it. Tool
//! calls are handed to the thread that owns the editor, so every change goes through the
//! normal transaction / undo path and repaints the window.
//!
//! * [`server`]: the rmcp HTTP service (runs on its own Tokio thread).
//! * [`exec`]: runs one tool call against a [`exec::Host`] (the app, or a headless session).
//! * [`tools`]: tool names, descriptions and schemas; [`css`] and [`html`]: the style and
//!   markup mapping used by `write_html` / `update_styles`.
//! * [`relay_proto`]: JSON frames between the relay and a browser tab; [`relay`]: the
//!   relay server itself (native only).

pub mod bounds;
pub mod codegen;
pub mod css;
pub mod diff;
pub mod enforcer;
pub mod exec;
pub mod html;
pub mod lint;
pub mod lint_overflow;
#[cfg(not(target_arch = "wasm32"))]
pub mod imagegen;
#[cfg(not(target_arch = "wasm32"))]
pub mod delegate;
#[cfg(not(target_arch = "wasm32"))]
pub mod refs;
#[cfg(not(target_arch = "wasm32"))]
pub mod relay;
pub mod relay_proto;
#[cfg(not(target_arch = "wasm32"))]
pub mod server;
pub mod steer;
#[cfg(not(target_arch = "wasm32"))]
pub mod store;
pub mod tools;

/// Paper uses 29979; papercraft listens next door so both apps can run side by side.
pub const DEFAULT_PORT: u16 = 29_980;
pub const ENDPOINT: &str = "/mcp";

/// The port to serve on / connect to: `$PAPERCRAFT_MCP_PORT`, else [`DEFAULT_PORT`].
pub fn port() -> u16 {
    std::env::var("PAPERCRAFT_MCP_PORT")
        .ok()
        .and_then(|p| p.trim().parse().ok())
        .unwrap_or(DEFAULT_PORT)
}

pub fn url(port: u16) -> String {
    format!("http://127.0.0.1:{port}{ENDPOINT}")
}

/// One piece of tool output.
#[derive(Debug, Clone, PartialEq)]
pub enum Out {
    Text(String),
    Image { data: Vec<u8>, mime: String },
}

pub type ToolResult = Result<Vec<Out>, String>;

/// Name of the client for calls made by an in-app chat turn; its session is the chat id.
pub const CHAT_CLIENT: &str = "Chat";
/// Client of the Actions menu's in-process actions (Bind to tokens, Use components, ...).
pub const ACTIONS_CLIENT: &str = "Actions";

/// Who made a tool call: each (name, session) pair gets its own agent run.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Client {
    /// The MCP `clientInfo` title or name ("Claude Code"), or [`CHAT_CLIENT`].
    pub name: String,
    /// The MCP session id, or the chat id.
    pub session: String,
}

impl Client {
    pub fn chat(id: &str) -> Client {
        Client { name: CHAT_CLIENT.into(), session: id.into() }
    }
}

/// A tool call travelling from the HTTP thread to the editor thread.
pub struct Call {
    pub name: String,
    pub args: serde_json::Value,
    pub client: Client,
    pub reply: tokio::sync::oneshot::Sender<ToolResult>,
}

/// Run calls arriving on `rx` against a host until every sender is gone. For hosts that
/// own their editor on a plain thread (tests, headless use); the app drains its channel
/// from the event loop instead.
pub fn serve_host(host: &mut dyn exec::Host, rx: std::sync::mpsc::Receiver<Call>) {
    while let Ok(call) = rx.recv() {
        let r = exec::call(host, &call.name, &call.args);
        let _ = call.reply.send(r);
    }
}
