//! The MCP service (rmcp) and its loopback HTTP server.
//!
//! Like Paper's server it listens on 127.0.0.1 only, accepts loopback `Host` headers only
//! (DNS rebinding guard), rejects browser requests that carry an `Origin`, repairs a missing
//! `Accept` header (some clients omit it and then misread the 406 as an auth failure) and
//! answers unknown routes with a plain JSON hint rather than an OAuth-looking error.

use super::{Call, Client, ENDPOINT, Out, ToolResult, core_tools, tools};
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Implementation,
    ListToolsResult, PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool,
};
use rmcp::service::RequestContext;
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
};
use rmcp::{ErrorData, RoleServer, ServerHandler};
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// Hands a call to the editor thread. Returns false when the app is gone.
pub type Dispatch = Arc<dyn Fn(Call) -> bool + Send + Sync>;

/// Longest a tool call may take on the editor thread.
pub const CALL_TIMEOUT: Duration = Duration::from_secs(120);

/// The tool catalogue as rmcp tools.
pub fn tool_list() -> Vec<Tool> {
    tools::definitions()
        .into_iter()
        .map(|t| serde_json::from_value(t).expect("tool definitions are valid MCP tools"))
        .collect()
}

/// Server identity, capabilities and instructions.
pub fn info() -> ServerConfig {
    ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
        .with_server_info(Implementation::new("PaperCraft", env!("CARGO_PKG_VERSION")))
        .with_instructions(tools::INSTRUCTIONS)
}

/// Tool output as an MCP result (errors become `isError` results the agent can read).
pub fn to_result(r: ToolResult) -> CallToolResult {
    match r {
        Ok(outs) => CallToolResult::success(
            outs.into_iter()
                .map(|o| match o {
                    Out::Text(t) => ContentBlock::text(t),
                    Out::Image { data, mime } => {
                        ContentBlock::image(crate::assets::base64(&data), mime)
                    }
                })
                .collect(),
        ),
        Err(e) => CallToolResult::error(vec![ContentBlock::text(e)]),
    }
}

/// One MCP session's handler; all sessions share the dispatcher.
#[derive(Clone)]
pub struct PaperServer {
    dispatch: Dispatch,
}

impl PaperServer {
    pub fn new(dispatch: Dispatch) -> PaperServer {
        PaperServer { dispatch }
    }
}

impl ServerHandler for PaperServer {
    fn get_info(&self) -> ServerConfig {
        info()
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        if !wants_core(&context) {
            return Ok(ListToolsResult::with_all_items(tool_list()));
        }
        let core = core_tools::definitions(tools::definitions());
        Ok(ListToolsResult::with_all_items(core.into_iter().map(|t| serde_json::from_value(t).expect("core tool definitions are valid MCP tools")).collect()))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let (mut name, mut args) = (request.name.to_string(), serde_json::Value::Object(request.arguments.unwrap_or_default()));
        if name == core_tools::SEARCH {
            let found = core_tools::search(&tools::definitions(), args["query"].as_str().unwrap_or_default(), args["detail"].as_str().unwrap_or("schema"));
            return Ok(to_result(Ok(vec![Out::Text(found.to_string())])).into());
        }
        if name == core_tools::CALL {
            match core_tools::target(&args) {
                Ok(t) => (name, args) = t,
                Err(e) => return Ok(to_result(Err(e)).into()),
            }
        }
        let (reply, rx) = tokio::sync::oneshot::channel();
        let call = Call { name, args, client: client_of(&context), reply };
        let r = if !(self.dispatch)(call) {
            Err("papercraft is shutting down".to_string())
        } else {
            match tokio::time::timeout(CALL_TIMEOUT, rx).await {
                Ok(Ok(r)) => r,
                Ok(Err(_)) => Err("papercraft dropped the call (is it closing?)".into()),
                Err(_) => Err("papercraft did not answer in time (is a modal dialog open?)".into()),
            }
        };
        Ok(to_result(r).into())
    }
}

/// Who is calling: an in-app chat turn when the URL carries `?chat=<id>` (the chat's CLI
/// backends connect that way), else the MCP client's title or name and its session id.
fn client_of(context: &RequestContext<RoleServer>) -> Client {
    let parts = context.extensions.get::<axum::http::request::Parts>();
    let query = parts.and_then(|p| p.uri.query());
    if let Some(id) = query.and_then(chat_of) {
        return Client::chat(id);
    }
    if let Some((name, parent)) = query.and_then(agent_of) {
        return Client::agent(&name, parent);
    }
    let name = context
        .client_info()
        .map(|i| i.title.filter(|t| !t.is_empty()).unwrap_or(i.name))
        .unwrap_or_default();
    let session = parts
        .and_then(|p| p.headers.get("mcp-session-id"))
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    Client { name, session: session.into(), parent: None }
}

/// The session asked for the lean tool list (`tools=core` in the URL query).
fn wants_core(context: &RequestContext<RoleServer>) -> bool {
    let parts = context.extensions.get::<axum::http::request::Parts>();
    parts.and_then(|p| p.uri.query()).is_some_and(|q| q.split('&').any(|kv| kv == "tools=core"))
}

/// The chat id in an MCP URL query (`chat=<id>`).
pub fn chat_of(query: &str) -> Option<&str> {
    query.split('&').find_map(|kv| kv.strip_prefix("chat="))
}

/// Names a person or the app itself uses, which an agent cannot take.
const RESERVED: [&str; 3] = ["You", super::CHAT_CLIENT, super::ACTIONS_CLIENT];
/// Longest agent name (`--as`).
const MAX_AGENT_NAME: usize = 40;

/// The agent name and parent session in an MCP URL query (`agent=<name>&parent=<session>`, from
/// `papercraft-cli --as`). A name outside `[A-Za-z0-9 ._/-]{1,40}` or a reserved one is ignored.
pub fn agent_of(query: &str) -> Option<(String, Option<String>)> {
    let get = |k: &str| query.split('&').find_map(|kv| kv.strip_prefix(k)).map(decode);
    let name = get("agent=")?;
    let ok = (1..=MAX_AGENT_NAME).contains(&name.chars().count())
        && name.chars().all(|c| c.is_ascii_alphanumeric() || " ._/-".contains(c))
        && !RESERVED.iter().any(|r| r.eq_ignore_ascii_case(name.trim()));
    ok.then(|| (name, get("parent=").filter(|p| !p.is_empty())))
}

/// Percent-decoding of a query value (`+` is a space).
fn decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let hex = b.get(i + 1..i + 3).and_then(|h| std::str::from_utf8(h).ok()).and_then(|h| u8::from_str_radix(h, 16).ok());
        match (b[i], hex) {
            (b'%', Some(v)) => {
                out.push(v);
                i += 3;
            }
            (b'+', _) => {
                out.push(b' ');
                i += 1;
            }
            (c, _) => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Add the `Accept` header the streamable HTTP transport requires when a client omits it.
/// Shared with the relay, which serves the same transport to remote agents.
pub(crate) async fn fix_accept(
    mut req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use axum::http::header::{ACCEPT, HeaderValue};
    if req.method() == axum::http::Method::POST {
        let accept = req
            .headers()
            .get(ACCEPT)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if !(accept.contains("application/json") && accept.contains("text/event-stream")) {
            req.headers_mut().insert(
                ACCEPT,
                HeaderValue::from_static("application/json, text/event-stream"),
            );
        }
    }
    next.run(req).await
}

/// The HTTP routes: the MCP endpoint plus a JSON 404 for everything else.
pub fn router(dispatch: Dispatch, ct: CancellationToken) -> axum::Router {
    let config = StreamableHttpServerConfig::default()
        .enforce_origin_validation()
        .with_max_request_body_bytes(64 << 20)
        .with_cancellation_token(ct);
    let service: StreamableHttpService<PaperServer, LocalSessionManager> =
        StreamableHttpService::new(
            move || Ok(PaperServer::new(dispatch.clone())),
            Default::default(),
            config,
        );
    axum::Router::new()
        .nest_service(ENDPOINT, service)
        .fallback(|| async {
            (
                axum::http::StatusCode::NOT_FOUND,
                [(axum::http::header::CONTENT_TYPE, "application/json")],
                format!(
                    "{{\"status\":\"not_found\",\"message\":\"Route not found. The MCP endpoint is {ENDPOINT}.\"}}"
                ),
            )
        })
        .layer(axum::middleware::from_fn(fix_accept))
}

/// A running HTTP server; stops when dropped.
pub struct Running {
    pub port: u16,
    ct: CancellationToken,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Running {
    pub fn url(&self) -> String {
        super::url(self.port)
    }
    pub fn stop(mut self) {
        self.shutdown();
    }
    fn shutdown(&mut self) {
        self.ct.cancel();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Serve MCP on `127.0.0.1:port` (0 = any free port) from a background thread.
pub fn spawn(port: u16, dispatch: Dispatch) -> Result<Running, String> {
    let listener = std::net::TcpListener::bind(("127.0.0.1", port)).map_err(|e| {
        if e.kind() == std::io::ErrorKind::AddrInUse {
            format!(
                "port {port} is in use (another papercraft instance?); set PAPERCRAFT_MCP_PORT to use another"
            )
        } else {
            format!("cannot listen on 127.0.0.1:{port}: {e}")
        }
    })?;
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let ct = CancellationToken::new();
    let thread_ct = ct.clone();
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_name("papercraft-mcp")
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    let thread = std::thread::Builder::new()
        .name("papercraft-mcp-http".into())
        .spawn(move || {
            rt.block_on(async move {
                let listener = match tokio::net::TcpListener::from_std(listener) {
                    Ok(l) => l,
                    Err(e) => {
                        log::error!("mcp: {e}");
                        return;
                    }
                };
                let app = router(dispatch, thread_ct.child_token());
                if let Err(e) = axum::serve(listener, app)
                    .with_graceful_shutdown(thread_ct.cancelled_owned())
                    .await
                {
                    log::error!("mcp: server stopped: {e}");
                }
            });
        })
        .map_err(|e| e.to_string())?;
    Ok(Running {
        port,
        ct,
        thread: Some(thread),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_named_cli_agent_is_read_from_the_query() {
        assert_eq!(agent_of("agent=Codex%20B&parent=s1"), Some(("Codex B".into(), Some("s1".into()))));
        assert_eq!(agent_of("agent=Chat+%2F+build+1"), Some(("Chat / build 1".into(), None)));
        assert_eq!(agent_of("agent=You"), None, "a reserved name");
        assert_eq!(agent_of("agent=a%3Bb"), None, "a character outside the set");
        assert_eq!(agent_of("chat=7"), None);
    }

    #[test]
    fn tools_deserialize_and_names_are_unique() {
        let tools = tool_list();
        assert!(tools.len() >= 30, "{}", tools.len());
        let mut names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();
        names.sort();
        let n = names.len();
        names.dedup();
        assert_eq!(names.len(), n, "duplicate tool names");
        for t in &tools {
            assert_eq!(
                t.input_schema.get("type").and_then(|v| v.as_str()),
                Some("object")
            );
        }
        // Paper's core write and read tools are present under the same names.
        for name in [
            "get_basic_info",
            "get_selection",
            "get_screenshot",
            "write_html",
            "create_artboard",
            "update_styles",
            "set_text_content",
            "delete_nodes",
            "duplicate_nodes",
            "move_nodes",
            "get_jsx",
            "finish_working_on_nodes",
        ] {
            assert!(names.contains(&name), "missing {name}");
        }
    }
}
