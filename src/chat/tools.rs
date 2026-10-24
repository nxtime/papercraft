//! Pieces shared by the two API loops (`anthropic`, `openai`): the tool list, the input validator,
//! approvals, running a tool on the UI thread, the turn handle, the event sink and the HTTP
//! request with its stop polling.

use super::events::{Event, ToolDispatch, Turn};
use crate::mcp::Out;
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::time::Duration;

pub const MAX_TOOL_ROUNDS: usize = 40;
/// How often blocking waits look at the stop flag (stop must end the turn within 3 s).
pub const POLL: Duration = Duration::from_millis(100);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const READ_TIMEOUT: Duration = Duration::from_secs(120);
/// An HTTP error body is a short JSON message; never buffer more than this.
const ERROR_BODY_MAX: usize = 64 * 1024;
pub const STOPPED_BY_USER: &str = "stopped by user; this call was not completed or its outcome is unknown";
pub const DENIED: &str = "The user denied this action.";
/// Tool results above this many layers in one `delete_nodes` need the user's approval.
const DELETE_NODES_CONFIRM_ABOVE: usize = 20;
pub const CONTINUE_NOTE: &str = "Your previous reply was cut off by the output limit. Continue where you stopped.";
pub const CUT_OFF: &str = "output limit reached before the call finished; not run";

/// The app's tools as `{name, description, inputSchema}`, in `tool_list()` order.
pub fn tool_specs() -> Vec<Value> {
    crate::mcp::server::tool_list()
        .iter()
        .filter_map(|t| serde_json::to_value(t).ok())
        .map(|t| json!({"name": t["name"], "description": t["description"], "inputSchema": t["inputSchema"]}))
        .collect()
}

// ---------------------------------------------------------------------- validation

/// Minimal JSON-schema check: object shape, required fields, and the JSON type of each
/// declared property (recursing into properties and array items).
pub fn validate_input(schema: &Value, input: &Value) -> Result<(), String> {
    check(schema, input, "input")
}

fn type_ok(t: &str, v: &Value) -> bool {
    match t {
        "string" => v.is_string(),
        "boolean" => v.is_boolean(),
        "number" => v.is_number(),
        "integer" => v.is_i64() || v.is_u64() || v.as_f64().is_some_and(|f| f.fract() == 0.0),
        "array" => v.is_array(),
        "object" => v.is_object(),
        "null" => v.is_null(),
        _ => true,
    }
}

fn check(schema: &Value, v: &Value, path: &str) -> Result<(), String> {
    let ok = match schema.get("type") {
        Some(Value::String(t)) => type_ok(t, v),
        Some(Value::Array(ts)) => ts.iter().filter_map(Value::as_str).any(|t| type_ok(t, v)),
        _ => true,
    };
    if !ok {
        return Err(format!("{path}: expected {}, got {v}", schema["type"]));
    }
    if let Some(e) = schema["enum"].as_array() {
        if !e.contains(v) {
            return Err(format!("{path}: {v} is not one of {}", schema["enum"]));
        }
    }
    if let Some(n) = v.as_f64() {
        if schema["minimum"].as_f64().is_some_and(|m| n < m) {
            return Err(format!("{path}: {n} is below the minimum {}", schema["minimum"]));
        }
        if schema["maximum"].as_f64().is_some_and(|m| n > m) {
            return Err(format!("{path}: {n} is above the maximum {}", schema["maximum"]));
        }
    }
    if let Some(s) = v.as_str() {
        if schema["minLength"].as_u64().is_some_and(|m| (s.chars().count() as u64) < m) {
            return Err(format!("{path}: shorter than {} characters", schema["minLength"]));
        }
    }
    // anyOf / oneOf: at least one branch must accept (oneOf's "exactly one" is not enforced).
    for key in ["anyOf", "oneOf"] {
        if let Some(branches) = schema[key].as_array() {
            if !branches.iter().any(|b| check(b, v, path).is_ok()) {
                return Err(format!("{path}: {v} matches none of the allowed {key} shapes"));
            }
        }
    }
    if let Some(o) = v.as_object() {
        for r in schema["required"].as_array().into_iter().flatten().filter_map(Value::as_str) {
            if !o.contains_key(r) {
                return Err(format!("{path}: missing required field `{r}`"));
            }
        }
        let props = schema["properties"].as_object();
        for (k, val) in o {
            let child = format!("{path}.{k}");
            match (props.and_then(|p| p.get(k)), &schema["additionalProperties"]) {
                (Some(s), _) => check(s, val, &child)?,
                (None, Value::Bool(false)) => return Err(format!("{path}: unexpected field `{k}`")),
                (None, extra @ Value::Object(_)) => check(extra, val, &child)?,
                _ => {}
            }
        }
    }
    if let Some(a) = v.as_array() {
        if schema["minItems"].as_u64().is_some_and(|m| (a.len() as u64) < m) {
            return Err(format!("{path}: needs at least {} items", schema["minItems"]));
        }
        if let Some(items) = schema.get("items") {
            for (i, x) in a.iter().enumerate() {
                check(items, x, &format!("{path}[{i}]"))?;
            }
        }
    }
    Ok(())
}

/// Destructive tools wait for the user (spec, Confirmation).
pub fn needs_approval(name: &str, args: &Value) -> bool {
    match name {
        "delete_pages" | "disable_library" | "import_design_system" => true,
        "delete_nodes" => args["nodeIds"].as_array().is_some_and(|a| a.len() > DELETE_NODES_CONFIRM_ABOVE),
        _ => false,
    }
}

// ---------------------------------------------------------------------- running a tool

/// Tool output as content blocks (text and base64 images) for the `ToolEnd` event and the
/// Messages API; never empty.
pub fn out_blocks(outs: &[Out]) -> Vec<Value> {
    let mut v: Vec<Value> = outs
        .iter()
        .map(|o| match o {
            Out::Text(t) => json!({"type": "text", "text": t}),
            Out::Image { data, mime } => json!({"type": "image", "source": {"type": "base64", "media_type": mime, "data": STANDARD.encode(data)}}),
        })
        .collect();
    if v.is_empty() {
        v.push(json!({"type": "text", "text": "OK"}));
    }
    v
}

/// What a tool run needs from its worker.
pub struct Ctl<'a> {
    pub stop: &'a AtomicBool,
    pub approvals: &'a Receiver<(String, bool)>,
    pub dispatch: Option<&'a ToolDispatch>,
    pub emit: &'a dyn Fn(Event),
}

impl Ctl<'_> {
    fn stopped(&self) -> bool {
        self.stop.load(Ordering::SeqCst)
    }

    /// Validates, waits for approval when needed, runs on the UI thread. `None` = stopped.
    pub async fn run_one(&self, specs: &[Value], id: &str, name: &str, args: &Value) -> Option<Result<Vec<Out>, String>> {
        let Some(tool) = specs.iter().find(|t| t["name"] == name) else {
            return Some(Err(format!("unknown tool `{name}`")));
        };
        if let Err(e) = validate_input(&tool["inputSchema"], args) {
            return Some(Err(format!("invalid input for `{name}`: {e}")));
        }
        if needs_approval(name, args) {
            (self.emit)(Event::NeedsApproval { id: id.into(), name: name.into(), args: args.clone() });
            loop {
                if self.stopped() {
                    return None;
                }
                match self.approvals.recv_timeout(POLL) {
                    Ok((i, ok)) if i == id => {
                        if ok {
                            break;
                        }
                        return Some(Err(DENIED.into()));
                    }
                    Ok(_) | Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => return Some(Err(DENIED.into())),
                }
            }
        }
        let Some(d) = self.dispatch else { return Some(Err("tools are not available".into())) };
        let (reply, mut rx) = tokio::sync::oneshot::channel();
        if !(d.0)(crate::mcp::Call { name: name.into(), args: args.clone(), client: Default::default(), reply }) {
            return Some(Err("the app is not accepting tool calls".into()));
        }
        loop {
            match tokio::time::timeout(POLL, &mut rx).await {
                Ok(Ok(r)) => return Some(r),
                Ok(Err(_)) => return Some(Err("the app dropped the tool call".into())),
                Err(_) if self.stopped() => return None,
                Err(_) => {}
            }
        }
    }

    /// The `ToolEnd` event for a finished call.
    pub fn end_event(id: &str, r: &Result<Vec<Out>, String>) -> Event {
        match r {
            Ok(outs) => Event::ToolEnd { id: id.into(), result: Some(Value::Array(out_blocks(outs))), error: None },
            Err(e) => Event::ToolEnd { id: id.into(), result: None, error: Some(e.clone()) },
        }
    }
}

// ---------------------------------------------------------------------- turn handle, sink

/// A running API turn. `approve` answers a `NeedsApproval` event.
pub struct ApiTurn {
    pub(crate) stop: Arc<AtomicBool>,
    running: Arc<AtomicBool>,
    approvals: Sender<(String, bool)>,
}

impl ApiTurn {
    pub fn approve(&self, id: &str, ok: bool) {
        let _ = self.approvals.send((id.to_string(), ok));
    }

    /// Runs `work(stop, approvals)` on a named thread; the turn counts as running until it returns.
    pub fn spawn(name: &str, work: impl FnOnce(Arc<AtomicBool>, Receiver<(String, bool)>) + Send + 'static) -> Result<ApiTurn, String> {
        let (approvals, approvals_rx) = channel();
        let turn = ApiTurn { stop: Arc::new(AtomicBool::new(false)), running: Arc::new(AtomicBool::new(true)), approvals };
        let (stop, running) = (turn.stop.clone(), turn.running.clone());
        std::thread::Builder::new()
            .name(name.into())
            .spawn(move || {
                work(stop, approvals_rx);
                running.store(false, Ordering::SeqCst);
            })
            .map_err(|e| e.to_string())?;
        Ok(turn)
    }
}

impl Turn for ApiTurn {
    /// The worker polls the flag every 100 ms and drops the connection.
    fn stop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
    fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}

/// Sends events; the turn's `TurnEnd` or `Error` is held back until `flush`, so the history is on
/// disk before the app sees the end (it deletes or reopens the chat right then).
pub struct Sink {
    tx: Sender<Event>,
    pub stop: Arc<AtomicBool>,
    terminal: std::cell::RefCell<Option<Event>>,
}

impl Sink {
    pub fn new(tx: Sender<Event>, stop: Arc<AtomicBool>) -> Self {
        Self { tx, stop, terminal: Default::default() }
    }

    pub fn stopped(&self) -> bool {
        self.stop.load(Ordering::SeqCst)
    }

    pub fn emit(&self, e: Event) {
        if matches!(e, Event::TurnEnd { .. } | Event::Error(_)) {
            self.terminal.borrow_mut().get_or_insert(e);
            return;
        }
        if self.tx.send(e).is_err() {
            self.stop.store(true, Ordering::SeqCst);
        }
    }

    pub fn flush(&self) {
        if let Some(e) = self.terminal.take() {
            let _ = self.tx.send(e);
        }
    }
}

/// Messages only change in consistent steps, so this file is always replayable.
pub fn persist(path: Option<&std::path::PathBuf>, messages: &[Value]) {
    let Some(p) = path else { return };
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let r = serde_json::to_vec(messages).map_err(|e| e.to_string()).and_then(|b| std::fs::write(p, b).map_err(|e| e.to_string()));
    if let Err(e) = r {
        log::warn!("chat history not saved: {e}");
    }
}

/// Reads `chats/<id>.api.json`; a missing file is an empty history.
pub fn load_history(path: Option<&std::path::PathBuf>) -> Result<Vec<Value>, String> {
    match path.map(std::fs::read) {
        Some(Ok(b)) => serde_json::from_slice(&b).map_err(|e| format!("chat history is unreadable: {e}")),
        _ => Ok(vec![]),
    }
}

// ---------------------------------------------------------------------- HTTP

pub enum Halt {
    Stopped,
    /// An error event was already sent.
    Reported,
    Fail(String),
}

pub fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder().connect_timeout(CONNECT_TIMEOUT).read_timeout(READ_TIMEOUT).build().map_err(|e| format!("http client: {e}"))
}

/// Header names that carry the key; marked sensitive so they never show up in debug output.
const SENSITIVE: [&str; 2] = ["x-api-key", "authorization"];

/// POSTs `body`; on a non-2xx answer returns `Fail("<label> error <status>: <message>")`.
/// Stop is polled every 100 ms, also while the error body is read.
pub async fn post(stop: &AtomicBool, client: &reqwest::Client, url: &str, headers: Vec<(&'static str, String)>, body: &Value, label: &str) -> Result<reqwest::Response, Halt> {
    let mut req = client.post(url).body(body.to_string());
    for (k, v) in headers {
        let mut hv = reqwest::header::HeaderValue::from_str(&v).map_err(|_| Halt::Fail(format!("invalid {k} header value")))?;
        hv.set_sensitive(SENSITIVE.contains(&k));
        req = req.header(k, hv);
    }
    let send = req.send();
    tokio::pin!(send);
    let mut resp = loop {
        match tokio::time::timeout(POLL, &mut send).await {
            Ok(r) => break r.map_err(|e| Halt::Fail(format!("request failed: {}", e.without_url())))?,
            Err(_) if stop.load(Ordering::SeqCst) => return Err(Halt::Stopped),
            Err(_) => {}
        }
    };
    if resp.status().is_success() {
        return Ok(resp);
    }
    let status = resp.status();
    let mut bytes: Vec<u8> = vec![];
    while bytes.len() < ERROR_BODY_MAX {
        if stop.load(Ordering::SeqCst) {
            return Err(Halt::Stopped);
        }
        match tokio::time::timeout(POLL, resp.chunk()).await {
            Err(_) => {}
            Ok(Ok(Some(c))) => bytes.extend_from_slice(&c),
            Ok(_) => break,
        }
    }
    let text = String::from_utf8_lossy(&bytes).into_owned();
    let v = serde_json::from_str::<Value>(&text).ok();
    // `{"error":{"message":..}}` (both formats), `{"error":"..."}` or `{"message":..}`.
    let msg = v.as_ref().and_then(|v| v["error"]["message"].as_str().or_else(|| v["error"].as_str()).or_else(|| v["message"].as_str()).map(str::to_string));
    Err(Halt::Fail(format!("{label} error {status}: {}", msg.unwrap_or(text))))
}

/// Feeds the response body to `feed` as UTF-8 text until it returns true (the reply is complete) or
/// the body ends. Stop drops the connection.
pub async fn read_stream(stop: &AtomicBool, mut resp: reqwest::Response, mut feed: impl FnMut(&str) -> bool) -> Result<(), Halt> {
    let mut pending: Vec<u8> = vec![];
    loop {
        if stop.load(Ordering::SeqCst) {
            return Err(Halt::Stopped);
        }
        let chunk = match tokio::time::timeout(POLL, resp.chunk()).await {
            Err(_) => continue,
            Ok(r) => r.map_err(|e| Halt::Fail(format!("stream error: {}", e.without_url())))?,
        };
        let Some(chunk) = chunk else { return Ok(()) };
        pending.extend_from_slice(&chunk);
        let valid = std::str::from_utf8(&pending).map_or_else(|e| e.valid_up_to(), str::len);
        let text = String::from_utf8_lossy(&pending[..valid]).into_owned();
        pending.drain(..valid);
        if feed(&text) {
            return Ok(());
        }
    }
}

/// Runs `fut` on a fresh current-thread runtime, as the workers do.
pub fn block_on<F: std::future::Future<Output = ()>>(fut: F) -> Result<(), String> {
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|e| format!("could not start the network runtime: {e}"))?;
    rt.block_on(fut);
    Ok(())
}

/// Replaces anything that looks like the key in an error message (servers sometimes echo it).
pub fn redact(msg: &str, key: &str) -> String {
    if key.len() < 8 { msg.to_string() } else { msg.replace(key, "[redacted]") }
}

/// A fake HTTP server for the loop tests.
#[cfg(test)]
pub mod test_support {
    use serde_json::Value;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::sync::{Arc, Mutex};

    /// `(lowercased request headers, JSON body)` per request.
    pub type Seen = Arc<Mutex<Vec<(Vec<String>, Value)>>>;

    /// Serves one canned SSE response per connection, in order; records headers and body.
    pub fn serve(responses: Vec<&'static str>) -> (String, Seen) {
        let ok = "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n";
        serve_raw(responses.into_iter().map(|r| format!("{ok}{r}")).collect())
    }

    pub fn serve_raw(responses: Vec<String>) -> (String, Seen) {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", l.local_addr().unwrap());
        let seen: Seen = Arc::default();
        let s = seen.clone();
        std::thread::spawn(move || {
            for resp in responses {
                let (mut sock, _) = l.accept().unwrap();
                let mut rd = BufReader::new(sock.try_clone().unwrap());
                let (mut headers, mut len) = (vec![], 0usize);
                loop {
                    let mut line = String::new();
                    rd.read_line(&mut line).unwrap();
                    let line = line.trim_end().to_lowercase();
                    if line.is_empty() {
                        break;
                    }
                    if let Some(n) = line.strip_prefix("content-length:") {
                        len = n.trim().parse().unwrap();
                    }
                    headers.push(line);
                }
                let mut body = vec![0; len];
                rd.read_exact(&mut body).unwrap();
                s.lock().unwrap().push((headers, serde_json::from_slice(&body).unwrap()));
                let _ = sock.write_all(resp.as_bytes());
            }
        });
        (url, seen)
    }
}
