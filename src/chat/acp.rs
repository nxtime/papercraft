//! A generic Agent Client Protocol client (https://agentclientprotocol.com): JSON-RPC 2.0, one
//! message per line, over the agent's stdio. One process per turn: `initialize`, `session/new`
//! (or `session/load`) with PaperCraft's MCP server, `session/prompt`, then the agent's
//! `session/update` notifications are mapped to [`Event`]s until the prompt response arrives.

use super::events::{Event, Turn, TurnConfig};
use super::{tools, Usage};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const PROTOCOL_VERSION: u64 = 1;
const STOP_TIMEOUT: Duration = Duration::from_secs(3);
const POLL: Duration = Duration::from_millis(20);
const TICK: Duration = Duration::from_millis(50);
/// Agents may start slowly (node, login); a prompt itself has no limit.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(120);
/// After the turn the agent gets this long to exit on its own (flush its session), then dies.
const EXIT_GRACE: Duration = Duration::from_secs(1);
const STDERR_MAX: usize = 16 * 1024;
const METHOD_NOT_FOUND: i64 = -32601;
/// The Plan card is the existing `TodoWrite` card.
const PLAN_TOOL: &str = "TodoWrite";
const SERVER_NAME: &str = "papercraft";
const BRIDGE: &str = "papercraft-mcp";
/// How agents prefix MCP tool names; longest first.
const NAME_PREFIXES: [&str; 4] = ["mcp__papercraft__", "mcp_papercraft_", "papercraft__", "papercraft_"];
/// Same keys the other backends drop: the agent must run on its own login.
const CREDENTIAL_ENV: [&str; 4] = ["ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN", "OPENAI_API_KEY", "CODEX_API_KEY"];

type Stdin = Arc<Mutex<Option<ChildStdin>>>;

fn send(stdin: &Stdin, v: &Value) -> bool {
    let mut g = stdin.lock().unwrap();
    let Some(w) = g.as_mut() else { return false };
    writeln!(w, "{v}").and_then(|_| w.flush()).is_ok()
}

/// SIGKILL to the agent's whole process group (it is its own group), so its helpers die too.
fn kill_tree(child: &mut Child) {
    #[cfg(unix)]
    unsafe {
        libc::kill(-(child.id() as libc::pid_t), libc::SIGKILL);
    }
    let _ = child.kill();
    let _ = child.wait();
}

pub struct AcpTurn {
    child: Arc<Mutex<Child>>,
    stdin: Stdin,
    session: Arc<Mutex<Option<String>>>,
    running: Arc<AtomicBool>,
    stopped: Arc<AtomicBool>,
    approvals: Sender<(String, bool)>,
}

impl AcpTurn {
    /// Answers a `NeedsApproval` event (`id` = the event's id).
    pub fn approve(&self, id: &str, ok: bool) {
        let _ = self.approvals.send((id.to_string(), ok));
    }
}

impl Turn for AcpTurn {
    /// Never blocks: the cancel write and the kill timer run on a thread.
    fn stop(&mut self) {
        if self.stopped.swap(true, Ordering::SeqCst) {
            return;
        }
        let (child, stdin, session) = (self.child.clone(), self.stdin.clone(), self.session.clone());
        // The kill timer starts first: a stalled agent can block the cancel write forever.
        let deadline = Instant::now() + STOP_TIMEOUT;
        thread::spawn(move || loop {
            let mut c = child.lock().unwrap();
            if !matches!(c.try_wait(), Ok(None)) {
                return;
            }
            if Instant::now() >= deadline {
                kill_tree(&mut c);
                return;
            }
            drop(c);
            thread::sleep(POLL);
        });
        thread::spawn(move || {
            let sid = session.lock().unwrap().clone();
            if let Some(sid) = sid {
                send(&stdin, &json!({"jsonrpc": "2.0", "method": "session/cancel", "params": {"sessionId": sid}}));
            }
        });
    }

    fn is_running(&self) -> bool {
        self.running.load(Ordering::Acquire)
    }
}

/// Starts `program args…` and runs one prompt. Events arrive on `tx`; the last is `TurnEnd` or
/// `Error`.
/// `env` is added to the agent's environment (a lean turn's `agents::pure_env`).
pub fn start(cfg: &TurnConfig, program: &Path, args: &[String], env: &[(String, String)], prompt: &str, tx: Sender<Event>) -> Result<AcpTurn, String> {
    spawn(cfg, program, args, env, prompt, tx, None)
}

fn spawn(cfg: &TurnConfig, program: &Path, args: &[String], env: &[(String, String)], prompt: &str, tx: Sender<Event>, bridge: Option<PathBuf>) -> Result<AcpTurn, String> {
    let mut cmd = Command::new(program);
    cmd.args(args).envs(env.iter().cloned()).current_dir(&cfg.project_dir).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    for k in CREDENTIAL_ENV {
        cmd.env_remove(k);
    }
    super::path::apply(&mut cmd);
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut cmd, 0);
    let mut child = cmd.spawn().map_err(|e| format!("Could not start {}: {e}", program.display()))?;
    let (stdout, mut stderr) = (child.stdout.take().unwrap(), child.stderr.take().unwrap());
    let stdin: Stdin = Arc::new(Mutex::new(child.stdin.take()));
    let child = Arc::new(Mutex::new(child));
    let (approvals, approvals_rx) = channel();
    let turn = AcpTurn {
        child: child.clone(),
        stdin: stdin.clone(),
        session: Arc::new(Mutex::new(None)),
        running: Arc::new(AtomicBool::new(true)),
        stopped: Arc::new(AtomicBool::new(false)),
        approvals,
    };
    let (session, running, stopped) = (turn.session.clone(), turn.running.clone(), turn.stopped.clone());

    let (line_tx, rx) = channel();
    thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            match serde_json::from_str::<Value>(&line) {
                Ok(v) if v.is_object() => {
                    if line_tx.send(v).is_err() {
                        return;
                    }
                }
                _ => log::debug!("acp: skipped non-JSON line: {line}"),
            }
        }
    });
    let err_text = Arc::new(Mutex::new(String::new()));
    let err_sink = err_text.clone();
    thread::spawn(move || {
        let mut buf = [0u8; 1024];
        while let Ok(n @ 1..) = stderr.read(&mut buf) {
            let mut t = err_sink.lock().unwrap();
            if t.len() < STDERR_MAX {
                t.push_str(&String::from_utf8_lossy(&buf[..n]));
            }
        }
    });

    let (cfg, prompt, name) = (cfg.clone(), prompt.to_string(), program.display().to_string());
    thread::spawn(move || {
        let mut c = Client {
            rx,
            stdin: stdin.clone(),
            approvals: approvals_rx,
            stopped: stopped.clone(),
            tx: tx.clone(),
            next_id: 0,
            replay: false,
            pending: HashMap::new(),
            tools: HashMap::new(),
            usage: None,
            cost: None,
            plans: 0,
            sid: None,
        };
        let result = c.run(&cfg, &prompt, bridge, &session);
        c.close_open_tools();
        let end = match result {
            Ok(stop) => Event::TurnEnd {
                usage: c.usage.take(),
                session: c.sid.clone(),
                stop_reason: Some(if stop == "cancelled" { "stopped".into() } else { stop }),
            },
            Err(_) if stopped.load(Ordering::SeqCst) => Event::TurnEnd { usage: None, session: c.sid.clone(), stop_reason: Some("stopped".into()) },
            Err(e) => {
                // Give a dying agent a moment to flush its error text.
                thread::sleep(POLL * 5);
                let err = err_text.lock().unwrap().trim().to_string();
                Event::Error(if err.is_empty() { format!("{name}: {e}") } else { format!("{name}: {e}: {}", tail(&err)) })
            }
        };
        // Send first: the app treats "not running" as "everything is in the channel".
        let _ = tx.send(end);
        stdin.lock().unwrap().take(); // EOF: a well-behaved agent exits
        let deadline = Instant::now() + EXIT_GRACE;
        loop {
            let mut ch = child.lock().unwrap();
            if !matches!(ch.try_wait(), Ok(None)) {
                break;
            }
            if Instant::now() >= deadline {
                kill_tree(&mut ch);
                break;
            }
            drop(ch);
            thread::sleep(POLL);
        }
        running.store(false, Ordering::Release);
    });
    Ok(turn)
}

/// An error that says the model, not the request, is the problem: rate limited, out of quota,
/// overloaded or gone. ponytail: agents do not document these, so it matches their wording.
fn is_retryable(e: &str) -> bool {
    const SIGNS: [&str; 10] = ["429", "rate limit", "rate_limit", "too many requests", "quota", "overloaded", "unavailable", "model not found", "no longer available", "insufficient"];
    let e = e.to_lowercase();
    SIGNS.iter().any(|s| e.contains(s))
}

fn tail(s: &str) -> &str {
    const KEEP: usize = 600;
    let mut from = s.len().saturating_sub(KEEP);
    while !s.is_char_boundary(from) {
        from += 1;
    }
    &s[from..]
}

struct Tool {
    name: String,
    args: Value,
    ended: bool,
    /// Announced as one of PaperCraft's own MCP tools (never a shell, edit or fetch).
    ours: bool,
}

struct Client {
    rx: Receiver<Value>,
    stdin: Stdin,
    approvals: Receiver<(String, bool)>,
    stopped: Arc<AtomicBool>,
    tx: Sender<Event>,
    next_id: u64,
    /// `session/load` replays the history as updates; those are not new output.
    replay: bool,
    /// Permission requests waiting for the user: tool call id -> (JSON-RPC id, options).
    pending: HashMap<String, (Value, Vec<Value>)>,
    tools: HashMap<String, Tool>,
    usage: Option<Usage>,
    cost: Option<f64>,
    plans: u32,
    sid: Option<String>,
}

impl Client {
    fn emit(&self, e: Event) {
        let _ = self.tx.send(e);
    }

    /// Switches the session to the chat's model when the agent offers it. A refusal keeps the
    /// agent's model, which is what the chat then shows.
    fn pick_model(&mut self, sid: &str, m: &mut Models, want: Option<&str>) -> Option<Value> {
        let want = want.filter(|w| *w != m.current && m.options.iter().any(|(id, _)| id == w))?;
        let r = match &m.config_id {
            Some(config) => self.call("session/set_config_option", json!({"sessionId": sid, "configId": config, "value": want}), Some(HANDSHAKE_TIMEOUT)),
            None => self.call("session/set_model", json!({"sessionId": sid, "modelId": want}), Some(HANDSHAKE_TIMEOUT)),
        };
        match r {
            Ok(r) => { want.clone_into(&mut m.current); Some(r) },
            Err(e) => { log::warn!("acp: could not switch the model to {want}: {e}"); None },
        }
    }

    fn run(&mut self, cfg: &TurnConfig, prompt: &str, bridge: Option<PathBuf>, slot: &Mutex<Option<String>>) -> Result<String, String> {
        let init = self.call(
            "initialize",
            json!({
                "protocolVersion": PROTOCOL_VERSION,
                "clientCapabilities": {"fs": {"readTextFile": false, "writeTextFile": false}, "terminal": false},
                "clientInfo": {"name": "papercraft", "title": "PaperCraft", "version": env!("CARGO_PKG_VERSION")},
            }),
            Some(HANDSHAKE_TIMEOUT),
        )?;
        let caps = &init["agentCapabilities"];
        let http = caps["mcpCapabilities"]["http"] == true;
        let image = caps["promptCapabilities"]["image"] == true;
        let bridge = if http { None } else { bridge.or_else(find_bridge) };
        let session_params = |extra: Value| -> Result<Value, String> {
            let mut p = json!({"cwd": cfg.project_dir, "mcpServers": mcp_servers(&session_url(cfg), http, bridge.as_deref())?});
            if let (Some(p), Some(extra)) = (p.as_object_mut(), extra.as_object()) {
                p.extend(extra.clone());
            }
            Ok(p)
        };

        let mut loaded = None;
        if let Some(id) = cfg.session.as_ref().filter(|_| caps["loadSession"] == true) {
            self.replay = true;
            let r = self.call("session/load", session_params(json!({"sessionId": id}))?, Some(HANDSHAKE_TIMEOUT));
            self.replay = false;
            match r {
                Ok(r) => loaded = Some((id.clone(), r)),
                Err(e) => log::warn!("acp: session/load failed ({e}); starting a new session"),
            }
        }
        let first = loaded.is_none();
        let (sid, initial) = match loaded {
            Some((id, r)) => (id, r),
            None => {
                let r = self.call("session/new", session_params(json!({}))?, Some(HANDSHAKE_TIMEOUT))?;
                (r["sessionId"].as_str().ok_or("session/new returned no sessionId")?.to_string(), r)
            }
        };
        let mut models = session_models(&initial);
        self.sid = Some(sid.clone());
        *slot.lock().unwrap() = Some(sid.clone());
        self.emit(Event::Session(sid.clone()));
        if let Some(m) = models.as_mut() {
            let updated = self.pick_model(&sid, m, cfg.model.as_deref());
            self.emit(Event::Models { current: m.current.clone(), options: m.options.clone() });
            if cfg.effort.is_some() && cfg.model.as_deref().is_some_and(|want| want != m.current) {
                return Err("Could not select the model for the requested effort".into());
            }
            let configs = updated.as_ref().unwrap_or(&initial);
            if let Some(option) = effort_option(configs) {
                self.emit(Event::ModelEfforts { model: m.current.clone(), options: option.options.clone() });
                if let Some(want) = cfg.effort.as_deref() {
                    if !option.options.iter().any(|(id, _)| id == want) {
                        return Err(format!("The selected model does not offer effort {want}"));
                    }
                    if want != option.current {
                        self.call("session/set_config_option", json!({"sessionId": sid, "configId": option.id, "value": want}), Some(HANDSHAKE_TIMEOUT))
                            .map_err(|e| format!("Could not set effort to {want}: {e}"))?;
                    }
                }
            } else if cfg.effort.is_some() {
                return Err("This agent did not offer an effort setting for the selected model".into());
            }
        } else if cfg.effort.is_some() {
            return Err("This agent did not offer an effort setting for the selected model".into());
        }

        // ACP has no system prompt: the PaperCraft text leads the first prompt of a session.
        let mut blocks = vec![];
        if first && !cfg.system.is_empty() {
            blocks.push(json!({"type": "text", "text": cfg.system}));
        }
        blocks.push(json!({"type": "text", "text": prompt}));
        if image {
            blocks.extend(cfg.images.iter().map(|i| json!({"type": "image", "mimeType": i.mime, "data": STANDARD.encode(&i.data)})));
        }
        let mut fallbacks = cfg.fallbacks.iter();
        let r = loop {
            let e = match self.call("session/prompt", json!({"sessionId": sid, "prompt": blocks}), None) {
                Ok(r) => break r,
                Err(e) => e,
            };
            // A failure after a tool ran is not retried: the design may already have changed.
            let retry = is_retryable(&e) && self.tools.is_empty() && !self.stopped.load(Ordering::SeqCst);
            let Some(m) = models.as_mut().filter(|_| retry) else { return Err(e) };
            let Some(next) = fallbacks.find(|f| m.options.iter().any(|(id, _)| id == *f)) else { return Err(e) };
            let from = m.current.clone();
            self.pick_model(&sid, m, Some(next));
            if m.current != *next {
                return Err(e);
            }
            self.emit(Event::ModelSwitch { from, to: next.clone(), reason: e });
            self.emit(Event::Models { current: m.current.clone(), options: m.options.clone() });
        };
        self.usage = usage_of(&r["usage"]).map(|mut u| {
            u.cost_usd = self.cost;
            u
        });
        if self.usage.is_none() && self.cost.is_some() {
            self.usage = Some(Usage { cost_usd: self.cost, ..Default::default() });
        }
        Ok(r["stopReason"].as_str().unwrap_or("end_turn").to_string())
    }

    /// Sends a request and serves the agent's messages until its response arrives.
    fn call(&mut self, method: &str, params: Value, timeout: Option<Duration>) -> Result<Value, String> {
        let id = self.next_id;
        self.next_id += 1;
        if !send(&self.stdin, &json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})) {
            return Err("the agent closed its input".into());
        }
        let deadline = timeout.map(|t| Instant::now() + t);
        loop {
            while let Ok((tool, ok)) = self.approvals.try_recv() {
                if let Some((rpc, options)) = self.pending.remove(&tool) {
                    self.answer(&rpc, &options, ok);
                }
            }
            if self.stopped.load(Ordering::SeqCst) {
                // The spec: every pending permission request gets the `cancelled` outcome.
                for (_, (rpc, _)) in self.pending.drain() {
                    send(&self.stdin, &reply(&rpc, json!({"outcome": {"outcome": "cancelled"}})));
                }
            }
            match self.rx.recv_timeout(TICK) {
                Ok(msg) if msg.get("method").is_some() => self.incoming(&msg),
                Ok(msg) if msg["id"] == json!(id) => {
                    return match msg.get("error") {
                        Some(e) => Err(rpc_error(e)),
                        None => Ok(msg["result"].clone()),
                    };
                }
                Ok(_) => {}
                Err(RecvTimeoutError::Timeout) => {
                    if deadline.is_some_and(|d| Instant::now() >= d) {
                        return Err(format!("{method} timed out"));
                    }
                }
                Err(RecvTimeoutError::Disconnected) => return Err("the agent exited".into()),
            }
        }
    }

    fn incoming(&mut self, msg: &Value) {
        let method = msg["method"].as_str().unwrap_or_default();
        match (method, msg.get("id")) {
            ("session/update", _) => self.update(&msg["params"]["update"]),
            ("session/request_permission", Some(id)) => self.permission(id.clone(), &msg["params"]),
            (m, Some(id)) => {
                log::debug!("acp: unsupported client method {m}");
                send(&self.stdin, &json!({"jsonrpc": "2.0", "id": id, "error": {"code": METHOD_NOT_FOUND, "message": format!("method not found: {m}")}}));
            }
            (m, None) => log::debug!("acp: ignored notification {m}"),
        }
    }

    fn update(&mut self, u: &Value) {
        if self.replay {
            return;
        }
        let text = |v: &Value| v["content"]["text"].as_str().filter(|t| !t.is_empty()).map(String::from);
        match u["sessionUpdate"].as_str().unwrap_or_default() {
            "agent_message_chunk" => text(u).into_iter().for_each(|t| self.emit(Event::Text(t))),
            "agent_thought_chunk" => text(u).into_iter().for_each(|t| self.emit(Event::Thinking(t))),
            "tool_call" => self.tool_call(u),
            "tool_call_update" => {
                if !self.tools.contains_key(&call_id(u)) {
                    self.tool_call(u);
                } else {
                    self.tool_status(u);
                }
            }
            "plan" => self.plan(u),
            "usage_update" => {
                if let Some(used) = u["used"].as_u64() {
                    self.emit(Event::Context(used));
                }
                if u["cost"]["currency"] == "USD" {
                    self.cost = u["cost"]["amount"].as_f64();
                }
            }
            other => log::debug!("acp: skipped session update {other:?}"),
        }
    }

    fn tool_call(&mut self, u: &Value) {
        let id = call_id(u);
        if id.is_empty() {
            return;
        }
        let args = if u["rawInput"].is_null() { json!({}) } else { u["rawInput"].clone() };
        let name = canonical_name(&tool_name(u));
        let ours = is_ours(&name, u["kind"].as_str());
        self.tools.insert(id.clone(), Tool { name: name.clone(), args: args.clone(), ended: false, ours });
        self.emit(Event::ToolStart { id, name, args });
        self.tool_status(u);
    }

    fn tool_status(&mut self, u: &Value) {
        let id = call_id(u);
        let Some(t) = self.tools.get_mut(&id) else { return };
        if t.ended || !["completed", "failed"].contains(&u["status"].as_str().unwrap_or_default()) {
            return;
        }
        t.ended = true;
        let text = content_text(&u["content"]);
        let ev = if u["status"] == "failed" {
            let msg = if !text.is_empty() { text } else if u["rawOutput"].is_null() { "tool failed".into() } else { u["rawOutput"].to_string() };
            Event::ToolEnd { id, result: None, error: Some(msg) }
        } else {
            let result = if !text.is_empty() { json!(text) } else if u["rawOutput"].is_null() { json!("") } else { u["rawOutput"].clone() };
            Event::ToolEnd { id, result: Some(result), error: None }
        };
        self.emit(ev);
    }

    /// A stopped turn leaves tools open; close them so no card spins forever.
    fn close_open_tools(&mut self) {
        let open: Vec<String> = self.tools.iter().filter(|(_, t)| !t.ended).map(|(id, _)| id.clone()).collect();
        for id in open {
            if let Some(t) = self.tools.get_mut(&id) {
                t.ended = true;
            }
            self.emit(Event::ToolEnd { id, result: None, error: Some(tools::STOPPED_BY_USER.into()) });
        }
    }

    fn plan(&mut self, u: &Value) {
        let Some(entries) = u["entries"].as_array() else { return };
        let todos: Vec<Value> = entries
            .iter()
            .filter_map(|e| {
                let status = match e["status"].as_str() {
                    Some(s @ ("in_progress" | "completed")) => s,
                    _ => "pending",
                };
                Some(json!({"content": e["content"].as_str()?, "status": status}))
            })
            .collect();
        self.plans += 1;
        let id = format!("acp-plan-{}", self.plans);
        self.emit(Event::ToolStart { id: id.clone(), name: PLAN_TOOL.into(), args: json!({"todos": todos}) });
        self.emit(Event::ToolEnd { id, result: Some(json!("ok")), error: None });
    }

    fn permission(&mut self, rpc: Value, params: &Value) {
        let options = params["options"].as_array().cloned().unwrap_or_default();
        let tc = &params["toolCall"];
        let id = call_id(tc);
        let known = self.tools.get(&id);
        let name = known.map_or_else(|| canonical_name(&tool_name(tc)), |t| t.name.clone());
        let args = if !tc["rawInput"].is_null() { tc["rawInput"].clone() } else { known.map_or(json!({}), |t| t.args.clone()) };
        if known.is_some_and(|t| t.ours) && is_ours(&name, tc["kind"].as_str()) && !tools::needs_approval(&name, &args) {
            return self.answer(&rpc, &options, true);
        }
        self.pending.insert(id.clone(), (rpc, options));
        self.emit(Event::NeedsApproval { id, name, args });
    }

    fn answer(&self, rpc: &Value, options: &[Value], allow: bool) {
        let prefix = if allow { "allow" } else { "reject" };
        let pick = ["_once", "_always"]
            .iter()
            .find_map(|s| options.iter().find(|o| o["kind"] == format!("{prefix}{s}")))
            .and_then(|o| o["optionId"].as_str());
        let outcome = match pick {
            Some(id) => json!({"outcome": "selected", "optionId": id}),
            None => json!({"outcome": "cancelled"}),
        };
        send(&self.stdin, &reply(rpc, json!({"outcome": outcome})));
    }
}

fn reply(id: &Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

fn rpc_error(e: &Value) -> String {
    let mut m = e["message"].as_str().unwrap_or("request failed").to_string();
    if let Some(d) = e["data"].as_str().or_else(|| e["data"]["details"].as_str()) {
        m = format!("{m} ({d})");
    }
    m
}

fn call_id(u: &Value) -> String {
    u["toolCallId"].as_str().unwrap_or_default().to_string()
}

/// `name` when the agent sends one, else the human `title`.
fn tool_name(u: &Value) -> String {
    u["name"].as_str().or_else(|| u["title"].as_str()).unwrap_or("tool").to_string()
}

/// Name without the agent's MCP prefix or a trailing ` (papercraft MCP Server)` suffix.
fn canonical_name(name: &str) -> String {
    let name = name.trim();
    let name = match name.rfind(" (") {
        Some(i) if name.ends_with(')') && name.to_ascii_lowercase()[i..].contains("mcp server") => &name[..i],
        _ => name,
    };
    NAME_PREFIXES.iter().find_map(|p| name.strip_prefix(p)).unwrap_or(name).trim().to_string()
}

/// A tool of PaperCraft's own MCP server: the canonical name is one of our tools and the kind is
/// not a built-in agent kind (execute, edit, fetch, delete...).
fn is_ours(canonical: &str, kind: Option<&str>) -> bool {
    matches!(kind, None | Some("other")) && tools::tool_specs().iter().any(|t| t["name"] == canonical)
}

/// Text of a `ToolCallContent` array: `{type:"content", content:{type:"text", text}}` entries.
fn content_text(v: &Value) -> String {
    v.as_array()
        .map(|a| a.iter().filter_map(|c| c["content"]["text"].as_str()).collect::<Vec<_>>().join("\n"))
        .unwrap_or_default()
}

/// The models of a session: its current one and the ones it offers.
struct Models {
    current: String,
    options: Vec<(String, String)>,
    /// The `model` config option's id; None when the agent only has the older `models` field.
    config_id: Option<String>,
}

struct EffortOption {
    id: String,
    current: String,
    options: Vec<(String, String)>,
}

fn effort_option(r: &Value) -> Option<EffortOption> {
    let o = r["configOptions"].as_array()?.iter().find(|o| o["category"] == "thought_level" || ["effort", "reasoning_effort"].contains(&o["id"].as_str().unwrap_or_default()))?;
    let options = o["options"].as_array()?.iter().filter_map(|v| {
        let id = v["value"].as_str()?;
        Some((id.to_owned(), v["name"].as_str().unwrap_or(id).to_owned()))
    }).collect();
    Some(EffortOption { id: o["id"].as_str()?.to_owned(), current: o["currentValue"].as_str()?.to_owned(), options })
}

/// The models in a `session/new` or `session/load` result: the `model` config option, else the
/// older (unstable) `models` field.
fn session_models(r: &Value) -> Option<Models> {
    let pairs = |list: &Value, id: &str| -> Vec<(String, String)> {
        let list = list.as_array().into_iter().flatten();
        list.filter_map(|o| Some((o[id].as_str()?.to_owned(), o["name"].as_str().or(o[id].as_str())?.to_owned()))).collect()
    };
    let opts = r["configOptions"].as_array().into_iter().flatten();
    if let Some(o) = opts.into_iter().find(|o| o["category"] == "model" || o["id"] == "model") {
        let current = o["currentValue"].as_str()?.to_owned();
        return Some(Models { current, options: pairs(&o["options"], "value"), config_id: o["id"].as_str().map(String::from) });
    }
    let m = &r["models"];
    Some(Models { current: m["currentModelId"].as_str()?.to_owned(), options: pairs(&m["availableModels"], "modelId"), config_id: None })
}

/// `{inputTokens, outputTokens, cachedReadTokens, cachedWriteTokens}` of a prompt response.
fn usage_of(u: &Value) -> Option<Usage> {
    if !u.is_object() {
        return None;
    }
    let get = |k: &str| u[k].as_u64().unwrap_or(0);
    Some(Usage { input: get("inputTokens"), output: get("outputTokens"), cache_read: get("cachedReadTokens"), cache_write: get("cachedWriteTokens"), cost_usd: None })
}

/// The MCP URL for the session. ACP agents load every tool schema up front, so a lean turn asks
/// for the core list (`mcp::core_tools`); the rest stay reachable through `call_tool`.
pub fn session_url(cfg: &TurnConfig) -> String {
    if !cfg.lean {
        return cfg.mcp_url.clone();
    }
    let join = if cfg.mcp_url.contains('?') { '&' } else { '?' };
    format!("{}{join}tools=core", cfg.mcp_url)
}

/// PaperCraft's MCP server as an ACP `mcpServers` list: HTTP when the agent supports it, else
/// the stdio `papercraft-mcp` bridge aimed at the same port.
fn mcp_servers(url: &str, http: bool, bridge: Option<&Path>) -> Result<Value, String> {
    if http {
        return Ok(json!([{"type": "http", "name": SERVER_NAME, "url": url, "headers": []}]));
    }
    let bridge = bridge.ok_or("This agent cannot use HTTP MCP servers and the stdio bridge (papercraft-mcp) was not found")?;
    let port = url.rsplit_once(':').map(|(_, r)| r.split('/').next().unwrap_or_default()).filter(|p| p.parse::<u16>().is_ok());
    let env: Vec<Value> = port.into_iter().map(|p| json!({"name": "PAPERCRAFT_MCP_PORT", "value": p})).collect();
    Ok(json!([{"name": SERVER_NAME, "command": bridge, "args": [], "env": env}]))
}

/// `papercraft-mcp` next to the running executable, else on PATH.
fn find_bridge() -> Option<PathBuf> {
    let file = format!("{BRIDGE}{}", std::env::consts::EXE_SUFFIX);
    std::env::current_exe().ok().and_then(|e| e.parent().map(|d| d.join(&file))).filter(|p| p.is_file()).or_else(|| super::path::which(BRIDGE))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    const AGENT: &str = include_str!("../../tests/fixtures/chat/acp_fake_agent.sh");
    const HTTP: &str = include_str!("../../tests/fixtures/chat/acp_http.jsonl");
    const MODEL: &str = include_str!("../../tests/fixtures/chat/acp_model.jsonl");
    const EFFORT: &str = include_str!("../../tests/fixtures/chat/acp_effort.jsonl");
    const ROTATE: &str = include_str!("../../tests/fixtures/chat/acp_rotate.jsonl");
    const ROTATE_LATE: &str = include_str!("../../tests/fixtures/chat/acp_rotate_late.jsonl");
    const STDIO: &str = include_str!("../../tests/fixtures/chat/acp_stdio.jsonl");
    const PERMISSION: &str = include_str!("../../tests/fixtures/chat/acp_permission.jsonl");
    const CANCEL: &str = include_str!("../../tests/fixtures/chat/acp_cancel.jsonl");
    const LOAD: &str = include_str!("../../tests/fixtures/chat/acp_load.jsonl");
    const LOAD_FAILS: &str = include_str!("../../tests/fixtures/chat/acp_load_fails.jsonl");
    const ERROR: &str = include_str!("../../tests/fixtures/chat/acp_error.jsonl");

    fn cfg() -> TurnConfig {
        TurnConfig { project_dir: std::env::temp_dir(), mcp_url: "http://127.0.0.1:29991/mcp".into(), system: "SYS".into(), ..Default::default() }
    }

    /// Runs the fake agent on `fixture`; `on` sees every event and may approve or stop. Returns
    /// the events and the JSON lines the client wrote.
    fn run(fixture: &str, cfg: TurnConfig, bridge: Option<PathBuf>, mut on: impl FnMut(&mut AcpTurn, &Event)) -> (Vec<Event>, Vec<Value>) {
        let dir = std::env::temp_dir().join(format!("pc-acp-{}", fastrand::u64(..)));
        std::fs::create_dir_all(&dir).unwrap();
        let (script, fx, log) = (dir.join("agent.sh"), dir.join("fixture.jsonl"), dir.join("client.log"));
        std::fs::write(&script, AGENT).unwrap();
        std::fs::write(&fx, fixture).unwrap();
        let args: Vec<String> = [&script, &fx, &log].iter().map(|p| p.display().to_string()).collect();
        let (tx, rx) = channel();
        let mut turn = spawn(&cfg, Path::new("/bin/sh"), &args, &[], "hello", tx, bridge).unwrap();
        let mut events = vec![];
        loop {
            let e = rx.recv_timeout(Duration::from_secs(15)).expect("turn timed out");
            on(&mut turn, &e);
            let done = matches!(e, Event::TurnEnd { .. } | Event::Error(_));
            events.push(e);
            if done {
                break;
            }
        }
        let t = Instant::now();
        while turn.is_running() && t.elapsed() < Duration::from_secs(5) {
            thread::sleep(POLL);
        }
        assert!(!turn.is_running());
        let sent = std::fs::read_to_string(&log).unwrap_or_default().lines().map(|l| serde_json::from_str(l).unwrap()).collect();
        std::fs::remove_dir_all(&dir).unwrap();
        (events, sent)
    }

    fn method<'a>(sent: &'a [Value], m: &str) -> &'a Value {
        sent.iter().find(|v| v["method"] == m).unwrap_or_else(|| panic!("no {m} in {sent:?}"))
    }

    #[test]
    fn http_handshake_streaming_tools_plan() {
        let mut c = cfg();
        c.images = vec![super::super::events::ImageAttachment { mime: "image/png".into(), data: vec![1, 2, 3] }];
        let (ev, sent) = run(HTTP, c, None, |_, _| {});
        let init = &method(&sent, "initialize")["params"];
        assert_eq!(init["protocolVersion"], 1);
        assert_eq!(init["clientCapabilities"], json!({"fs": {"readTextFile": false, "writeTextFile": false}, "terminal": false}));
        let new = &method(&sent, "session/new")["params"];
        assert_eq!(new["mcpServers"], json!([{"type": "http", "name": "papercraft", "url": "http://127.0.0.1:29991/mcp", "headers": []}]));
        assert_eq!(new["cwd"], json!(std::env::temp_dir()));
        let prompt = &method(&sent, "session/prompt")["params"];
        assert_eq!(prompt["sessionId"], "sess-1");
        assert_eq!(
            prompt["prompt"],
            json!([{"type": "text", "text": "SYS"}, {"type": "text", "text": "hello"}, {"type": "image", "mimeType": "image/png", "data": "AQID"}])
        );
        // unknown client request answered with -32601
        assert_eq!(sent.iter().find(|v| v["id"] == 50).unwrap()["error"]["code"], METHOD_NOT_FOUND);

        assert_eq!(ev[0], Event::Session("sess-1".into()));
        assert_eq!(ev[1], Event::Models { current: "opencode/big-pickle".into(), options: vec![] }, "the model option, not the mode");
        assert_eq!(ev[2], Event::Thinking("hmm".into()));
        assert_eq!(ev[3], Event::Text("Hel".into()));
        assert_eq!(ev[4], Event::Text("lo".into()));
        assert_eq!(ev[5], Event::ToolStart { id: "t1".into(), name: "get_basic_info".into(), args: json!({}) });
        assert_eq!(ev[6], Event::ToolEnd { id: "t1".into(), result: Some(json!("{\"ok\":true}")), error: None });
        assert_eq!(ev[7], Event::ToolStart { id: "t2".into(), name: "ls".into(), args: json!({"path": "."}) });
        assert_eq!(ev[8], Event::ToolEnd { id: "t2".into(), result: None, error: Some("denied".into()) });
        assert_eq!(ev[9], Event::ToolStart { id: "acp-plan-1".into(), name: "TodoWrite".into(), args: json!({"todos": [{"content": "A", "status": "completed"}, {"content": "B", "status": "pending"}]}) });
        assert_eq!(ev[10], Event::ToolEnd { id: "acp-plan-1".into(), result: Some(json!("ok")), error: None });
        assert_eq!(ev[11], Event::Context(5000));
        let Event::TurnEnd { usage: Some(u), session: Some(s), stop_reason: Some(r) } = &ev[12] else { panic!("{:?}", ev[12]) };
        assert_eq!((u.input, u.output, u.cost_usd, s.as_str(), r.as_str()), (7, 3, Some(0.5), "sess-1", "end_turn"));
        assert_eq!(ev.len(), 13);
    }

    #[test]
    fn switches_to_the_chats_model_when_offered() {
        let mut c = cfg();
        c.model = Some("paid/b".into());
        let (ev, sent) = run(MODEL, c, None, |_, _| {});
        assert_eq!(method(&sent, "session/set_config_option")["params"], json!({"sessionId": "sess-1", "configId": "model", "value": "paid/b"}));
        let options = vec![("free/a".into(), "A".into()), ("paid/b".into(), "B".into())];
        assert_eq!(ev[1], Event::Models { current: "paid/b".into(), options });
    }

    #[test]
    fn selects_effort_after_switching_model() {
        let mut c = cfg();
        c.model = Some("paid/b".into());
        c.effort = Some("high".into());
        let (ev, sent) = run(EFFORT, c, None, |_, _| {});
        let settings: Vec<_> = sent.iter().filter(|v| v["method"] == "session/set_config_option").collect();
        assert_eq!(settings[0]["params"]["value"], "paid/b");
        assert_eq!(settings[1]["params"], json!({"sessionId": "sess-1", "configId": "effort", "value": "high"}));
        assert!(ev.iter().any(|e| matches!(e, Event::ModelEfforts { model, options } if model == "paid/b" && options.iter().any(|(id, _)| id == "high"))));
        assert!(matches!(ev.last(), Some(Event::TurnEnd { .. })));
    }

    #[test]
    fn model_switch_without_refreshed_options_never_uses_old_efforts() {
        let fixture = r#"> "method":"initialize"
< {"jsonrpc":"2.0","id":0,"result":{"protocolVersion":1,"agentCapabilities":{}}}
> "method":"session/new"
< {"jsonrpc":"2.0","id":1,"result":{"sessionId":"sess-1","configOptions":[{"id":"model","category":"model","currentValue":"free/a","options":[{"value":"free/a"},{"value":"paid/b"}]},{"id":"effort","category":"thought_level","currentValue":"low","options":[{"value":"low"},{"value":"high"}]}]}}
> "method":"session/set_config_option"
< {"jsonrpc":"2.0","id":2,"result":{}}
"#;
        let mut c = cfg();
        c.model = Some("paid/b".into());
        c.effort = Some("high".into());
        let (ev, sent) = run(fixture, c, None, |_, _| {});
        assert!(matches!(ev.last(), Some(Event::Error(e)) if e.contains("effort setting")), "{ev:?}");
        assert_eq!(sent.iter().filter(|v| v["method"] == "session/set_config_option").count(), 1);
        assert!(sent.iter().all(|v| v["method"] != "session/prompt"));

        let rejected = fixture.replace(r#"{"jsonrpc":"2.0","id":2,"result":{}}"#, r#"{"jsonrpc":"2.0","id":2,"error":{"code":-32602,"message":"invalid model"}}"#);
        let mut c = cfg();
        c.model = Some("paid/b".into());
        c.effort = Some("high".into());
        let (ev, sent) = run(&rejected, c, None, |_, _| {});
        assert!(matches!(ev.last(), Some(Event::Error(e)) if e.contains("select the model")), "{ev:?}");
        assert_eq!(sent.iter().filter(|v| v["method"] == "session/set_config_option").count(), 1);
    }

    #[test]
    fn effort_without_session_model_is_not_silently_ignored() {
        let fixture = r#"> "method":"initialize"
< {"jsonrpc":"2.0","id":0,"result":{"protocolVersion":1,"agentCapabilities":{}}}
> "method":"session/new"
< {"jsonrpc":"2.0","id":1,"result":{"sessionId":"sess-1"}}
"#;
        let mut c = cfg();
        c.effort = Some("high".into());
        let (ev, sent) = run(fixture, c, None, |_, _| {});
        assert!(matches!(ev.last(), Some(Event::Error(e)) if e.contains("effort setting")), "{ev:?}");
        assert!(sent.iter().all(|v| v["method"] != "session/prompt"));
    }

    #[test]
    fn a_rate_limited_model_hands_the_turn_to_the_next_free_one() {
        let mut c = cfg();
        c.fallbacks = vec!["gone/x".into(), "paid/b".into()];
        let (ev, sent) = run(ROTATE, c, None, |_, _| {});
        assert_eq!(sent.iter().filter(|v| v["method"] == "session/prompt").count(), 2);
        assert_eq!(method(&sent, "session/set_config_option")["params"]["value"], "paid/b");
        assert!(ev.iter().any(|e| matches!(e, Event::ModelSwitch { from, to, reason } if from == "free/a" && to == "paid/b" && reason.contains("Rate limit"))), "{ev:?}");
        assert!(matches!(ev.last(), Some(Event::TurnEnd { .. })), "{ev:?}");
    }

    #[test]
    fn a_failure_after_a_tool_ran_is_not_retried() {
        let mut c = cfg();
        c.fallbacks = vec!["paid/b".into()];
        let (ev, sent) = run(ROTATE_LATE, c, None, |_, _| {});
        assert_eq!(sent.iter().filter(|v| v["method"] == "session/prompt").count(), 1);
        assert!(matches!(ev.last(), Some(Event::Error(e)) if e.contains("Rate limit")), "{ev:?}");
        assert!(!is_retryable("invalid params") && is_retryable("HTTP 429 Too Many Requests"));
    }

    #[test]
    fn a_model_the_agent_does_not_offer_is_not_sent() {
        let mut c = cfg();
        c.model = Some("gone/x".into());
        let (ev, sent) = run(HTTP, c, None, |_, _| {});
        assert!(sent.iter().all(|v| v["method"] != "session/set_config_option" && v["method"] != "session/set_model"));
        assert!(matches!(ev.last(), Some(Event::TurnEnd { .. })), "{ev:?}");
    }

    #[test]
    fn a_lean_turn_asks_for_the_core_tools() {
        let mut c = cfg();
        assert_eq!(session_url(&c), c.mcp_url);
        c.lean = true;
        c.mcp_url = "http://127.0.0.1:1/mcp?chat=c1".into();
        assert_eq!(session_url(&c), "http://127.0.0.1:1/mcp?chat=c1&tools=core");
    }

    #[test]
    fn stdio_bridge_when_no_http() {
        let (ev, sent) = run(STDIO, cfg(), Some("/fake/papercraft-mcp".into()), |_, _| {});
        assert_eq!(
            method(&sent, "session/new")["params"]["mcpServers"],
            json!([{"name": "papercraft", "command": "/fake/papercraft-mcp", "args": [], "env": [{"name": "PAPERCRAFT_MCP_PORT", "value": "29991"}]}])
        );
        assert_eq!(method(&sent, "session/prompt")["params"]["prompt"].as_array().unwrap().len(), 2);
        assert!(matches!(ev.last(), Some(Event::TurnEnd { stop_reason: Some(r), .. }) if r == "max_tokens"));
    }

    #[test]
    fn permissions_auto_and_asked() {
        let (ev, sent) = run(PERMISSION, cfg(), None, |turn, e| {
            if let Event::NeedsApproval { id, .. } = e {
                turn.approve(id, id == "t-del");
            }
        });
        let asked: Vec<_> = ev.iter().filter_map(|e| if let Event::NeedsApproval { id, name, .. } = e { Some((id.as_str(), name.as_str())) } else { None }).collect();
        assert_eq!(asked, [("t-del", "delete_pages"), ("t-sh", "Run ls")]);
        let answer = |rpc: u64| sent.iter().find(|v| v["id"] == rpc).unwrap()["result"]["outcome"].clone();
        assert_eq!(answer(100), json!({"outcome": "selected", "optionId": "allow-once"}));
        assert_eq!(answer(101), json!({"outcome": "selected", "optionId": "allow-once"}));
        assert_eq!(answer(102), json!({"outcome": "selected", "optionId": "reject-once"}));
        assert!(matches!(ev.last(), Some(Event::TurnEnd { .. })));
    }

    #[test]
    fn cancel_sends_notification_and_reports_stopped() {
        let t = Instant::now();
        let (ev, sent) = run(CANCEL, cfg(), None, |turn, e| {
            if matches!(e, Event::Text(_)) {
                turn.stop();
            }
        });
        assert_eq!(method(&sent, "session/cancel")["params"]["sessionId"], "sess-1");
        assert!(sent.iter().find(|v| v["method"] == "session/cancel").unwrap().get("id").is_none());
        // the open tool is closed, the pending permission got `cancelled`
        assert!(ev.iter().any(|e| matches!(e, Event::ToolEnd { id, error: Some(_), .. } if id == "t1")));
        assert_eq!(sent.iter().find(|v| v["id"] == 100).unwrap()["result"]["outcome"], json!({"outcome": "cancelled"}));
        assert!(matches!(ev.last(), Some(Event::TurnEnd { stop_reason: Some(r), .. }) if r == "stopped"));
        assert!(t.elapsed() < Duration::from_secs(3));
    }

    #[test]
    fn stop_kills_an_agent_that_ignores_cancel() {
        let (tx, rx) = channel();
        let args = ["-c".to_string(), "trap '' INT TERM; while :; do sleep 1; done".to_string()];
        let mut turn = spawn(&cfg(), Path::new("/bin/sh"), &args, &[], "x", tx, None).unwrap();
        thread::sleep(Duration::from_millis(100));
        let t = Instant::now();
        turn.stop();
        assert!(t.elapsed() < Duration::from_millis(200), "stop blocked");
        assert!(matches!(rx.recv_timeout(Duration::from_secs(8)), Ok(Event::TurnEnd { stop_reason: Some(r), .. }) if r == "stopped"));
        let t = Instant::now();
        while turn.is_running() && t.elapsed() < Duration::from_secs(5) {
            thread::sleep(POLL);
        }
        assert!(!turn.is_running());
    }

    #[test]
    fn load_replays_silently_and_skips_the_system_text() {
        let mut c = cfg();
        c.session = Some("old".into());
        let (ev, sent) = run(LOAD, c, None, |_, _| {});
        let load = &method(&sent, "session/load")["params"];
        assert_eq!(load["sessionId"], "old");
        assert!(load["mcpServers"].is_array());
        assert!(sent.iter().all(|v| v["method"] != "session/new"));
        assert_eq!(method(&sent, "session/prompt")["params"]["prompt"], json!([{"type": "text", "text": "hello"}]));
        assert_eq!(ev[0], Event::Session("old".into()));
        assert_eq!(ev[1], Event::Models { current: "gemini-3-pro".into(), options: vec![] });
        assert_eq!(ev[2], Event::Text("fresh".into()), "replayed history must not surface: {ev:?}");
        assert!(matches!(&ev[3], Event::TurnEnd { session: Some(s), .. } if s == "old"));
    }

    #[test]
    fn failed_load_falls_back_to_new_session() {
        let mut c = cfg();
        c.session = Some("gone".into());
        let (ev, sent) = run(LOAD_FAILS, c, None, |_, _| {});
        assert_eq!(method(&sent, "session/prompt")["params"]["prompt"][0]["text"], "SYS");
        assert_eq!(ev[0], Event::Session("sess-new".into()));
    }

    #[test]
    fn rpc_error_becomes_error_event() {
        let (ev, _) = run(ERROR, cfg(), None, |_, _| {});
        assert!(matches!(ev.last(), Some(Event::Error(e)) if e.contains("Authentication required")), "{ev:?}");
    }

    #[test]
    fn missing_program_is_an_error() {
        let (tx, _rx) = channel();
        assert!(spawn(&cfg(), Path::new("/nonexistent/agent"), &[], &[], "x", tx, None).is_err());
    }

    #[test]
    fn tool_names_and_papercraft_detection() {
        assert_eq!(canonical_name("mcp__papercraft__get_basic_info"), "get_basic_info");
        assert_eq!(canonical_name("papercraft_x"), "x");
        assert_eq!(canonical_name("Read file"), "Read file");
        assert_eq!(canonical_name("delete_pages (papercraft MCP Server)"), "delete_pages");
        assert!(is_ours("get_basic_info", Some("other")));
        assert!(!is_ours("get_basic_info", Some("execute")));
        assert!(!is_ours("rm -rf papercraft", None));
        assert!(!is_ours("bogus", None));
        assert_eq!(mcp_servers("http://x:1/mcp", false, None).unwrap_err().contains("papercraft-mcp"), true);
    }

    #[test]
    fn permission_trust_needs_an_announced_papercraft_tool() {
        // title-only differences, unannounced ids, execute kind and suffixed destructive names all ask
        let fx = [
            r#"{"sessionUpdate":"tool_call","toolCallId":"a","title":"get_basic_info (papercraft MCP Server)","kind":"other","rawInput":{}}"#,
            r#"{"sessionUpdate":"tool_call","toolCallId":"b","title":"Run ls","kind":"execute","rawInput":{}}"#,
            r#"{"sessionUpdate":"tool_call","toolCallId":"d","title":"delete_pages (papercraft MCP Server)","kind":"other","rawInput":{}}"#,
        ];
        let (tx, rx) = channel();
        let (stdin, _) = (Arc::new(Mutex::new(None)), ());
        let mut c = Client { rx: channel().1, stdin, approvals: channel().1, stopped: Arc::new(AtomicBool::new(false)), tx, next_id: 0, replay: false, pending: HashMap::new(), tools: HashMap::new(), usage: None, cost: None, plans: 0, sid: None };
        for u in fx {
            c.update(&serde_json::from_str(u).unwrap());
        }
        for (id, title, asked) in [("a", "get_basic_info (papercraft MCP Server)", false), ("b", "Run ls", true), ("d", "delete_pages (papercraft MCP Server)", true), ("zz", "get_basic_info", true), ("b", "get_basic_info", true)] {
            c.pending.clear();
            c.permission(json!(1), &json!({"toolCall": {"toolCallId": id, "title": title}, "options": []}));
            assert_eq!(c.pending.contains_key(id), asked, "{id} {title}");
        }
        drop(rx);
    }

    #[test]
    fn stop_kills_even_when_the_cancel_write_blocks() {
        let (tx, rx) = channel();
        // never reads stdin; a big write fills the pipe and blocks
        let args = ["-c".to_string(), "trap '' INT TERM; while :; do sleep 1; done".to_string()];
        let mut turn = spawn(&cfg(), Path::new("/bin/sh"), &args, &[], "x", tx, None).unwrap();
        *turn.session.lock().unwrap() = Some("s".repeat(1 << 20));
        thread::sleep(Duration::from_millis(100));
        turn.stop();
        assert!(matches!(rx.recv_timeout(Duration::from_secs(8)), Ok(Event::TurnEnd { stop_reason: Some(r), .. }) if r == "stopped"));
    }
}
