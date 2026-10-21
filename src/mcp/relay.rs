//! The relay: lets a remote agent drive a PaperCraft tab running in someone's browser.
//!
//! ```text
//! agent --(MCP streamable HTTP)--> relay <--(WebSocket)-- browser tab
//!                                 POST /s/{token}/mcp     GET /tab/{token}
//! ```
//!
//! The relay holds no document state: it serves the same tool list and instructions as the
//! desktop server and forwards each call to the tab as a [`relay_proto::Call`] frame. The token
//! in the path is the only credential, so it is never logged and never appears in an error.

use super::relay_proto::{self, Call as CallFrame, Reply};
use super::server::{CALL_TIMEOUT, Dispatch, PaperServer};
use super::{Call, ToolResult};
use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade};
use axum::extract::{FromRequestParts, Path, Request, State};
use axum::http::{HeaderMap, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use axum::routing::any;
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
};
use serde_json::Value;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::{Component, Path as FsPath, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

/// Biggest frame we accept from (or send to) a tab.
pub const MAX_WS_MESSAGE_BYTES: usize = 16 << 20;
/// How often we ping a tab; a socket that stops answering is dropped.
pub const PING_INTERVAL: Duration = Duration::from_secs(20);
/// A socket that has not ponged within this many ping intervals is considered dead.
const PONG_GRACE_INTERVALS: u32 = 3;
/// Tool calls allowed to be in flight for one link; the rest wait their turn.
pub const MAX_INFLIGHT_CALLS: usize = 8;
/// Cap on an MCP request body (screenshots and assets are big).
const MAX_MCP_BODY_BYTES: usize = 64 << 20;
/// What an agent sees when nothing is connected to the link.
pub const NO_TAB_ERROR: &str =
    "No PaperCraft tab is connected for this link. Open PaperCraft and use Agent > Connect.";
/// Close reason sent to the previous socket when a newer tab claims the link.
const TAB_REPLACED: &str = "replaced by a newer tab";
/// Shown for any request the token rules out; deliberately identical for bad tokens and
/// unknown links so a wrong token cannot be told apart from a wrong path.
const NOT_FOUND: &str = "not found";
/// Answered for a route the relay does not serve.
const UNKNOWN_ROUTE: &str = "not found: this relay serves /tab/{token} and /s/{token}/mcp";

type Service = StreamableHttpService<PaperServer, LocalSessionManager>;

/// Frames the relay pushes to a tab.
enum ToTab {
    /// A tool call to run.
    Call(String),
    /// Close the socket with this reason.
    Close(&'static str),
}

/// The live connection of one tab, plus the calls waiting for its answers.
struct Tab {
    out: mpsc::UnboundedSender<ToTab>,
    pending: Mutex<HashMap<u64, oneshot::Sender<Reply>>>,
}

impl Tab {
    /// Shorthand for a poisoned lock: a panic cannot leave the map half-written, and a stale
    /// entry only costs one abandoned wait.
    fn pending(&self) -> std::sync::MutexGuard<'_, HashMap<u64, oneshot::Sender<Reply>>> {
        self.pending.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Answer every call still waiting with `error`, so agents stop waiting on a dead socket.
    fn fail_all(&self, error: &str) {
        for (id, reply) in self.pending().drain() {
            let _ = reply.send(Reply::err(id, error));
        }
    }
}

/// Per-link state shared by the MCP service and the socket: the connected tab and the
/// in-flight limit.
struct Shared {
    tab: tokio::sync::Mutex<Option<Arc<Tab>>>,
    calls: Arc<tokio::sync::Semaphore>,
    next_id: AtomicU64,
}

impl Shared {
    fn new() -> Shared {
        Shared {
            tab: tokio::sync::Mutex::new(None),
            calls: Arc::new(tokio::sync::Semaphore::new(MAX_INFLIGHT_CALLS)),
            next_id: AtomicU64::new(1),
        }
    }

    /// Hand a tool call to the tab and wait for its answer.
    async fn forward(&self, name: String, args: Value) -> ToolResult {
        let Some(tab) = self.tab.lock().await.clone() else {
            return Err(NO_TAB_ERROR.to_string());
        };
        // Acquired before the frame goes out, so a link never asks the tab to answer more
        // calls at once than it is allowed to hold.
        let Ok(_permit) = self.calls.clone().acquire_owned().await else {
            return Err("the relay is shutting down".into());
        };
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let frame = CallFrame { id, name, args };
        let json = match serde_json::to_string(&frame) {
            Ok(j) => j,
            Err(e) => return Err(format!("cannot encode the call: {e}")),
        };
        let (reply_tx, reply_rx) = oneshot::channel();
        // Registered before the frame goes out: a tab may answer before we get there.
        tab.pending().insert(id, reply_tx);
        if tab.out.send(ToTab::Call(json)).is_err() {
            tab.pending().remove(&id);
            return Err(NO_TAB_ERROR.to_string());
        }
        let answered = tokio::time::timeout(CALL_TIMEOUT, reply_rx).await;
        tab.pending().remove(&id);
        match answered {
            Ok(Ok(reply)) => reply.into_result(),
            Ok(Err(_)) => Err(NO_TAB_ERROR.to_string()),
            Err(_) => Err("the PaperCraft tab did not answer in time".into()),
        }
    }
}

/// Everything one link owns: the cached MCP service plus its [`Shared`].
struct Link {
    service: Service,
    shared: Arc<Shared>,
}

impl Link {
    fn new(shared: Arc<Shared>, ct: CancellationToken) -> Link {
        let dispatch = dispatch(shared.clone());
        // Served under a public hostname; the URL token is the credential, not the Host header.
        let config = StreamableHttpServerConfig::default()
            .disable_allowed_hosts()
            .enforce_origin_validation()
            .with_max_request_body_bytes(MAX_MCP_BODY_BYTES)
            .with_cancellation_token(ct);
        let service: Service = StreamableHttpService::new(
            move || Ok(PaperServer::new(dispatch.clone())),
            Default::default(),
            config,
        );
        Link { service, shared }
    }
}

/// The relay holds no editor thread, so a call runs on its own task and the caller's oneshot
/// gets the answer that arrives over the WebSocket.
fn dispatch(shared: Arc<Shared>) -> Dispatch {
    Arc::new(move |call: Call| {
        let shared = shared.clone();
        tokio::spawn(async move {
            let r = shared.forward(call.name, call.args).await;
            let _ = call.reply.send(r);
        });
        true
    })
}

struct Relay {
    links: tokio::sync::Mutex<HashMap<String, Arc<Link>>>,
    allow_origin: Option<String>,
    static_dir: Option<PathBuf>,
    ct: CancellationToken,
}

impl Relay {
    /// The link for `token`, created on first use. An invalid token never gets one.
    async fn link(&self, token: &str) -> Option<Arc<Link>> {
        if !relay_proto::valid_token(token) {
            return None;
        }
        let mut links = self.links.lock().await;
        if let Some(link) = links.get(token) {
            return Some(link.clone());
        }
        let link = Arc::new(Link::new(Arc::new(Shared::new()), self.ct.child_token()));
        links.insert(token.to_string(), link.clone());
        Some(link)
    }

    /// `/tab/*` is a browser route: with `--allow-origin` only that exact origin may connect.
    fn origin_ok(&self, headers: &HeaderMap) -> bool {
        let Some(want) = &self.allow_origin else {
            return true;
        };
        match headers.get(header::ORIGIN) {
            Some(v) => v.to_str().map(|o| o == want).unwrap_or(false),
            None => true,
        }
    }
}
fn not_found(body: &'static str) -> Response {
    (StatusCode::NOT_FOUND, body).into_response()
}

fn forbidden(body: &'static str) -> Response {
    (StatusCode::FORBIDDEN, body).into_response()
}

/// `POST|GET|DELETE /s/{token}/mcp`: the agent's streamable-HTTP endpoint for one link.
async fn mcp_endpoint(
    State(relay): State<Arc<Relay>>,
    Path(token): Path<String>,
    req: Request,
) -> Response {
    // rmcp's `enforce_origin_validation` already turns browsers away; checking here keeps the
    // policy ours, and it runs before the link lookup so a browser cannot create one.
    if req.headers().contains_key(header::ORIGIN) {
        return forbidden("this endpoint does not accept browser requests");
    }
    let Some(link) = relay.link(&token).await else {
        return not_found(NOT_FOUND);
    };
    // axum can only nest a service under a path known when the router is built, and tokens
    // arrive at runtime; the service only needs the request, so it is called directly.
    link.service.handle(req).await.into_response()
}

/// `GET /tab/{token}`: the browser's socket for one link.
async fn tab_socket(
    State(relay): State<Arc<Relay>>,
    Path(token): Path<String>,
    req: Request,
) -> Response {
    let Some(link) = relay.link(&token).await else {
        return not_found(NOT_FOUND);
    };
    if !relay.origin_ok(req.headers()) {
        return forbidden("this origin may not connect a tab");
    }
    // The token is checked before the upgrade: a `WebSocketUpgrade` extractor would answer a
    // plain request with 400 before we could refuse a bad token.
    let (mut parts, _) = req.into_parts();
    let upgrade = match WebSocketUpgrade::from_request_parts(&mut parts, &()).await {
        Ok(u) => u,
        Err(rejection) => return rejection.into_response(),
    };
    upgrade
        .max_message_size(MAX_WS_MESSAGE_BYTES)
        .on_upgrade(move |socket| serve_tab(socket, link))
        .into_response()
}

/// What the socket loop is waiting for. Only one branch touches the socket, so the borrow
/// from `recv()` is gone by the time we write to it.
enum Event {
    Frame(Result<Message, axum::Error>),
    Outgoing(Option<ToTab>),
    Ping,
    Closed,
}

async fn next_event(
    socket: &mut WebSocket,
    out: &mut mpsc::UnboundedReceiver<ToTab>,
    ping: &mut tokio::time::Interval,
) -> Event {
    tokio::select! {
        m = socket.recv() => match m {
            Some(m) => Event::Frame(m),
            None => Event::Closed,
        },
        o = out.recv() => Event::Outgoing(o),
        _ = ping.tick() => Event::Ping,
    }
}

/// Answer the call waiting on this frame's id; frames for no known call are ignored.
fn handle_reply(tab: &Tab, text: &str) {
    let reply = match serde_json::from_str::<Reply>(text) {
        Ok(r) => r,
        Err(_) => {
            log::debug!("relay: ignoring a frame that is not a reply");
            return;
        }
    };
    match tab.pending().remove(&reply.id) {
        Some(waiter) => {
            let _ = waiter.send(reply);
        }
        None => log::debug!("relay: ignoring a reply with no pending call"),
    }
}

/// One tab's socket: forwards tool calls out, routes replies back to their calls, pings to
/// keep the connection (and any proxy in front of it) alive.
async fn serve_tab(mut socket: WebSocket, link: Arc<Link>) {
    let (out, mut out_rx) = mpsc::unbounded_channel::<ToTab>();
    let tab = Arc::new(Tab {
        out,
        pending: Mutex::new(HashMap::new()),
    });
    // One tab per link: a newer connection takes over and the older socket is closed.
    let previous = link.shared.tab.lock().await.replace(tab.clone());
    if let Some(old) = previous {
        let _ = old.out.send(ToTab::Close(TAB_REPLACED));
    }
    // The tab is live as of here: say so with a ping, so a client that waits for one never
    // races this registration.
    let _ = socket.send(Message::Ping(axum::body::Bytes::new())).await;
    let mut ping = tokio::time::interval_at(Instant::now() + PING_INTERVAL, PING_INTERVAL);
    let mut last_pong = Instant::now();
    loop {
        match next_event(&mut socket, &mut out_rx, &mut ping).await {
            Event::Closed | Event::Frame(Err(_)) => break,
            Event::Frame(Ok(Message::Close(_))) => break,
            Event::Frame(Ok(Message::Text(t))) => handle_reply(&tab, &t),
            Event::Frame(Ok(Message::Pong(_))) => last_pong = Instant::now(),
            // Ping replies are sent by the protocol layer; other frame types are not ours.
            Event::Frame(Ok(_)) => {}
            Event::Outgoing(None) => break,
            Event::Outgoing(Some(ToTab::Call(json))) => {
                if socket.send(Message::Text(json.into())).await.is_err() {
                    break;
                }
            }
            Event::Outgoing(Some(ToTab::Close(reason))) => {
                let frame = CloseFrame {
                    code: axum::extract::ws::close_code::AWAY,
                    reason: reason.into(),
                };
                let _ = socket.send(Message::Close(Some(frame))).await;
                break;
            }
            Event::Ping => {
                // No pong for several intervals means the socket is gone even if TCP has not
                // noticed yet.
                if last_pong.elapsed() > PING_INTERVAL * PONG_GRACE_INTERVALS {
                    break;
                }
                if socket
                    .send(Message::Ping(axum::body::Bytes::new()))
                    .await
                    .is_err()
                {
                    break;
                }
            }
        }
    }
    let mut current = link.shared.tab.lock().await;
    if let Some(t) = current.as_ref()
        && Arc::ptr_eq(t, &tab)
    {
        *current = None;
    }
    drop(current);
    tab.fail_all(NO_TAB_ERROR);
}

/// The MIME types the built web app needs; anything else is an opaque download.
const MIME_TYPES: &[(&str, &str)] = &[
    ("wasm", "application/wasm"),
    ("js", "text/javascript"),
    ("html", "text/html"),
    ("css", "text/css"),
    ("png", "image/png"),
    ("svg", "image/svg+xml"),
];
/// Fallback for anything not in [`MIME_TYPES`].
const DEFAULT_MIME: &str = "application/octet-stream";

fn mime_of(path: &FsPath) -> &'static str {
    let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
        return DEFAULT_MIME;
    };
    MIME_TYPES
        .iter()
        .find(|(e, _)| e.eq_ignore_ascii_case(ext))
        .map_or(DEFAULT_MIME, |(_, m)| *m)
}

/// `<dir>/index.html` for `/`, else `<dir>/<path>`; `None` for anything that would read
/// outside `root` (`..`, absolute paths, `.`).
fn static_path(root: &FsPath, request_path: &str) -> Option<PathBuf> {
    let rel = request_path.trim_start_matches('/');
    let rel = FsPath::new(if rel.is_empty() { "index.html" } else { rel });
    if rel.components().any(|c| !matches!(c, Component::Normal(_))) {
        return None;
    }
    Some(root.join(rel))
}

/// The `--static` tree, and the answer for every route the relay does not serve.
async fn serve_static(State(relay): State<Arc<Relay>>, uri: Uri) -> Response {
    let Some(dir) = relay.static_dir.as_deref() else {
        return not_found(UNKNOWN_ROUTE);
    };
    let Some(path) = static_path(dir, uri.path()) else {
        return not_found(NOT_FOUND);
    };
    // Read whole: the files are a wasm bundle, and a streaming body would need another
    // dependency for no benefit here.
    match tokio::fs::read(&path).await {
        // `no-cache` revalidates instead of storing: the web app is replaced under the same
        // URL, and a stale bundle in the browser is worse than a round trip.
        Ok(bytes) => (
            [
                (header::CONTENT_TYPE, mime_of(&path)),
                (header::CACHE_CONTROL, "no-cache"),
            ],
            bytes,
        )
            .into_response(),
        Err(_) => not_found(NOT_FOUND),
    }
}

fn router(relay: Arc<Relay>, store: Option<Arc<super::store::Store>>) -> axum::Router {
    let app = axum::Router::new()
        .route("/tab/{token}", any(tab_socket))
        .route("/s/{token}/mcp", any(mcp_endpoint))
        .fallback(serve_static)
        .layer(axum::middleware::from_fn(super::server::fix_accept))
        .with_state(relay);
    match store {
        Some(store) => app.merge(super::store::router(store)),
        None => app,
    }
}

/// A running relay; stops when dropped.
pub struct Running {
    port: u16,
    ct: CancellationToken,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Running {
    /// The port actually bound (0 in the request means "any free port").
    pub fn port(&self) -> u16 {
        self.port
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

/// Serve the relay from a background thread. `static_dir` serves the built web app from the
/// same origin; `allow_origin` restricts which browser may connect a tab.
pub fn spawn(
    listen: SocketAddr,
    static_dir: Option<PathBuf>,
    allow_origin: Option<String>,
) -> Result<Running, String> {
    spawn_with_data(listen, static_dir, allow_origin, None)
}

/// [`spawn`], plus `data`: where shared projects are kept (the `/p/` routes are off without it).
pub fn spawn_with_data(
    listen: SocketAddr,
    static_dir: Option<PathBuf>,
    allow_origin: Option<String>,
    data: Option<super::store::Data>,
) -> Result<Running, String> {
    let listener = std::net::TcpListener::bind(listen)
        .map_err(|e| format!("cannot listen on {listen}: {e}"))?;
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let ct = CancellationToken::new();
    let thread_ct = ct.clone();
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_name("papercraft-relay")
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    let thread = std::thread::Builder::new()
        .name("papercraft-relay-http".into())
        .spawn(move || {
            rt.block_on(async move {
                let listener = match tokio::net::TcpListener::from_std(listener) {
                    Ok(l) => l,
                    Err(e) => {
                        log::error!("relay: {e}");
                        return;
                    }
                };
                let relay = Arc::new(Relay {
                    links: tokio::sync::Mutex::new(HashMap::new()),
                    allow_origin,
                    static_dir,
                    ct: thread_ct.child_token(),
                });
                if let Err(e) =
                    axum::serve(listener, router(relay, data.map(super::store::Store::new)))
                        .with_graceful_shutdown(thread_ct.cancelled_owned())
                        .await
                {
                    log::error!("relay: server stopped: {e}");
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
    fn static_paths_stay_inside_the_root() {
        let root = FsPath::new("/srv/web");
        assert_eq!(
            static_path(root, "/").as_deref(),
            Some(FsPath::new("/srv/web/index.html"))
        );
        assert_eq!(
            static_path(root, "/pkg/papercraft_bg.wasm").as_deref(),
            Some(FsPath::new("/srv/web/pkg/papercraft_bg.wasm"))
        );
        for bad in [
            "/../Cargo.toml",
            "/pkg/../../etc/passwd",
            "/./index.html",
            "..",
        ] {
            assert_eq!(static_path(root, bad), None, "{bad}");
        }
        // Extra leading slashes are stripped, not resolved: still inside the root.
        assert_eq!(
            static_path(root, "//etc/passwd").as_deref(),
            Some(FsPath::new("/srv/web/etc/passwd"))
        );
    }

    #[test]
    fn mime_types_cover_the_built_web_app() {
        assert_eq!(mime_of(FsPath::new("a/b.wasm")), "application/wasm");
        assert_eq!(mime_of(FsPath::new("a/b.js")), "text/javascript");
        assert_eq!(mime_of(FsPath::new("index.html")), "text/html");
        assert_eq!(mime_of(FsPath::new("style.css")), "text/css");
        assert_eq!(mime_of(FsPath::new("icon.png")), "image/png");
        assert_eq!(mime_of(FsPath::new("logo.svg")), "image/svg+xml");
        assert_eq!(mime_of(FsPath::new("LICENSE")), DEFAULT_MIME);
    }
}
