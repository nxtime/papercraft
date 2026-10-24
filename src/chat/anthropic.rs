//! Messages API streaming, parsing half: an SSE accumulator that turns raw response text into
//! [`Event`]s and keeps the raw content blocks so the tool loop can append `response.content`
//! unchanged (thinking signatures and redacted blocks included).

use super::events::{str_of, Event};
use super::Usage;
use serde_json::{json, Value};
use std::collections::BTreeMap;

#[derive(Debug, Default)]
pub struct SseParser {
    buf: String,
    blocks: BTreeMap<usize, Block>,
    usage: Usage,
    stop_reason: Option<String>,
    stop_details: Option<Value>,
}

#[derive(Debug)]
struct Block {
    raw: Value,
    /// Accumulated `input_json_delta` fragments of a `tool_use` block.
    json: String,
}

impl SseParser {
    /// Feed any chunk of the response body; incomplete lines wait for the next call.
    pub fn push(&mut self, chunk: &str) -> Vec<Event> {
        self.buf.push_str(chunk);
        let mut out = vec![];
        while let Some(nl) = self.buf.find('\n') {
            let line: String = self.buf.drain(..=nl).collect();
            if let Some(data) = line.trim_end().strip_prefix("data:") {
                if let Ok(v) = serde_json::from_str::<Value>(data.trim()) {
                    self.event(&v, &mut out);
                }
            }
        }
        out
    }

    /// The assistant `content` array so far, in index order, ready to resend as-is.
    pub fn final_content(&self) -> Vec<Value> {
        self.blocks.values().map(|b| b.raw.clone()).collect()
    }

    pub fn stop_reason(&self) -> Option<&str> {
        self.stop_reason.as_deref()
    }

    /// `message_delta.delta.stop_details` (e.g. the refusal category), when present.
    pub fn stop_details(&self) -> Option<&Value> {
        self.stop_details.as_ref()
    }

    pub fn usage(&self) -> &Usage {
        &self.usage
    }

    fn event(&mut self, v: &Value, out: &mut Vec<Event>) {
        let idx = v["index"].as_u64().unwrap_or(0) as usize;
        match v["type"].as_str() {
            Some("message_start") => {
                self.blocks.clear();
                self.stop_reason = None;
                self.usage = Usage::default();
                self.merge_usage(&v["message"]["usage"]);
            }
            Some("content_block_start") => {
                let raw = v["content_block"].clone();
                if !raw.is_object() {
                    return out.push(Event::Error(format!("unexpected content_block at index {idx}: {raw}")));
                }
                self.blocks.insert(idx, Block { raw, json: String::new() });
            }
            Some("content_block_delta") => self.delta(idx, &v["delta"], out),
            Some("content_block_stop") => self.stop(idx, out),
            Some("message_delta") => {
                if let Some(r) = v["delta"]["stop_reason"].as_str() {
                    self.stop_reason = Some(r.into());
                }
                if !v["delta"]["stop_details"].is_null() {
                    self.stop_details = Some(v["delta"]["stop_details"].clone());
                }
                self.merge_usage(&v["usage"]);
            }
            Some("message_stop") => out.push(Event::TurnEnd {
                usage: Some(self.usage.clone()),
                session: None,
                stop_reason: self.stop_reason.clone(),
            }),
            Some("error") => {
                let e = &v["error"];
                let msg = format!("{}: {}", str_of(&e["type"]), str_of(&e["message"]));
                out.push(Event::Error(msg));
            }
            _ => {} // ping and unknown events
        }
    }

    fn delta(&mut self, idx: usize, d: &Value, out: &mut Vec<Event>) {
        let Some(b) = self.blocks.get_mut(&idx) else { return };
        // Only mutate a block of the matching type; anything else is reported and ignored.
        let want: &[&str] = match d["type"].as_str() {
            Some("text_delta") => &["text"],
            Some("thinking_delta" | "signature_delta") => &["thinking"],
            Some("input_json_delta") => &["tool_use", "server_tool_use"],
            _ => return,
        };
        if !b.raw.is_object() || !want.iter().any(|w| b.raw["type"] == *w) {
            return out.push(Event::Error(format!("{} for a {} block at index {idx}", str_of(&d["type"]), b.raw["type"])));
        }
        let append = |raw: &mut Value, key: &str, s: &str| {
            let cur = raw[key].as_str().unwrap_or_default();
            raw[key] = Value::String(format!("{cur}{s}"));
        };
        match d["type"].as_str() {
            Some("text_delta") => {
                let s = str_of(&d["text"]);
                append(&mut b.raw, "text", &s);
                out.push(Event::Text(s));
            }
            Some("thinking_delta") => {
                let s = str_of(&d["thinking"]);
                append(&mut b.raw, "thinking", &s);
                out.push(Event::Thinking(s));
            }
            Some("signature_delta") => append(&mut b.raw, "signature", d["signature"].as_str().unwrap_or("")),
            Some("input_json_delta") => b.json.push_str(d["partial_json"].as_str().unwrap_or("")),
            _ => {}
        }
    }

    fn stop(&mut self, idx: usize, out: &mut Vec<Event>) {
        let Some(b) = self.blocks.get_mut(&idx) else { return };
        if b.raw["type"] != "tool_use" {
            return;
        }
        // Some compatible servers send the whole input in `content_block_start` and no deltas.
        let args = if b.json.trim().is_empty() {
            Ok(if b.raw["input"].is_object() { b.raw["input"].clone() } else { json!({}) })
        } else {
            serde_json::from_str::<Value>(&b.json)
        };
        let (id, name) = (str_of(&b.raw["id"]), str_of(&b.raw["name"]));
        match args {
            Ok(args) => {
                b.raw["input"] = args.clone();
                out.push(Event::ToolStart { id, name, args });
            }
            // The loop must still answer this tool_use id, so keep the block with empty input.
            Err(e) => {
                b.raw["input"] = json!({});
                out.push(Event::Error(format!("invalid tool input JSON for {name}: {e}")));
            }
        }
    }

    fn merge_usage(&mut self, u: &Value) {
        let set = |k: &str, f: &mut u64| {
            if let Some(n) = u[k].as_u64() {
                *f = n;
            }
        };
        set("input_tokens", &mut self.usage.input);
        set("output_tokens", &mut self.usage.output);
        set("cache_read_input_tokens", &mut self.usage.cache_read);
        set("cache_creation_input_tokens", &mut self.usage.cache_write);
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use api::{build_body, build_headers, start, tools_json};

/// The Messages API backend: request building and the streaming tool loop. The same loop serves
/// `api.anthropic.com` and any Anthropic-compatible URL; Claude-only fields go to the former only.
#[cfg(not(target_arch = "wasm32"))]
mod api {
    use super::*;
    use crate::chat::endpoint;
    use crate::chat::events::TurnConfig;
    use crate::chat::tools::{self, ApiTurn, Ctl, Halt, Sink, CONTINUE_NOTE, CUT_OFF, MAX_TOOL_ROUNDS, STOPPED_BY_USER};
    use crate::mcp::Out;
    use base64::{engine::general_purpose::STANDARD, Engine};
    use std::sync::mpsc::{Receiver, Sender};

    const BASE_URL_ENV: &str = "PAPERCRAFT_ANTHROPIC_BASE_URL";
    const API_VERSION: &str = "2023-06-01";
    const BETA_FALLBACK: &str = "server-side-fallback-2026-07-01";
    const DEFAULT_EFFORT: &str = "medium";
    const MAX_TOKENS: u64 = 64_000;
    /// Other hosts have unknown output limits; stay well under common ones.
    const OTHER_MAX_TOKENS: u64 = 16_384;

    // ------------------------------------------------------------------ request

    /// Server-side refusal fallback exists for Opus and Sonnet; Haiku shows the refusal.
    fn has_fallbacks(model: &str) -> bool {
        model.contains("opus") || model.contains("sonnet")
    }

    /// `claude_only`: the host is `api.anthropic.com`, so the beta header may be sent. An empty key
    /// sends no key header (local servers).
    pub fn build_headers(model: &str, key: &str, claude_only: bool) -> Vec<(&'static str, String)> {
        let mut h = vec![("anthropic-version", API_VERSION.to_string()), ("content-type", "application/json".to_string())];
        if !key.is_empty() {
            h.insert(0, ("x-api-key", key.to_string()));
        }
        if claude_only && has_fallbacks(model) {
            h.push(("anthropic-beta", BETA_FALLBACK.to_string()));
        }
        h
    }

    /// The app's tools as Messages API tools, in `tool_list()` order. `claude_only` adds
    /// `eager_input_streaming` and a cache breakpoint on the last tool.
    pub fn tools_json(claude_only: bool) -> Vec<Value> {
        let mut out: Vec<Value> = tools::tool_specs()
            .into_iter()
            .map(|t| {
                let mut tool = json!({"name": t["name"], "description": t["description"], "input_schema": t["inputSchema"]});
                if claude_only {
                    tool["eager_input_streaming"] = json!(true);
                }
                tool
            })
            .collect();
        if let Some(last) = out.last_mut().filter(|_| claude_only) {
            last["cache_control"] = json!({"type": "ephemeral"});
        }
        out
    }

    /// No `tool_choice`: forced tool use is rejected on these models. Effort, thinking display,
    /// fallbacks and cache breakpoints only for `api.anthropic.com`; other hosts get a plain request.
    pub fn build_body(cfg: &TurnConfig, tools: &[Value], messages: &[Value]) -> Value {
        let model = cfg.model.as_deref().unwrap_or(&cfg.endpoint.model);
        let claude = cfg.endpoint.is_claude_host();
        let mut body = json!({
            "model": model,
            "max_tokens": if claude { MAX_TOKENS } else { OTHER_MAX_TOKENS },
            "stream": true,
            "tools": tools,
            "messages": messages,
        });
        if !cfg.system.is_empty() {
            // The digest changes more often than the skill: its own breakpoint keeps the skill cached.
            body["system"] = match (claude, cfg.ds_digest.is_empty()) {
                (true, true) => json!([{"type": "text", "text": cfg.system, "cache_control": {"type": "ephemeral"}}]),
                (true, false) => json!([
                    {"type": "text", "text": cfg.system, "cache_control": {"type": "ephemeral"}},
                    {"type": "text", "text": cfg.ds_digest, "cache_control": {"type": "ephemeral"}},
                ]),
                (false, true) => json!(cfg.system),
                (false, false) => json!(format!("{}\n\n{}", cfg.system, cfg.ds_digest)),
            };
        }
        if claude {
            body["thinking"] = json!({"type": "adaptive", "display": "summarized"});
            body["output_config"] = json!({"effort": cfg.effort.as_deref().unwrap_or(DEFAULT_EFFORT)});
            if has_fallbacks(model) {
                body["fallbacks"] = json!("default");
            }
        }
        body
    }

    fn tool_result(id: &str, r: &Result<Vec<Out>, String>) -> Value {
        match r {
            Ok(outs) => json!({"type": "tool_result", "tool_use_id": id, "content": tools::out_blocks(outs)}),
            Err(e) => json!({"type": "tool_result", "tool_use_id": id, "is_error": true, "content": [{"type": "text", "text": e}]}),
        }
    }

    /// A plain string, or image blocks followed by the text when images are attached.
    fn user_content(prompt: &str, images: &[crate::chat::events::ImageAttachment]) -> Value {
        if images.is_empty() {
            return json!(prompt);
        }
        let mut blocks: Vec<Value> = images
            .iter()
            .map(|i| json!({"type": "image", "source": {"type": "base64", "media_type": i.mime, "data": STANDARD.encode(&i.data)}}))
            .collect();
        blocks.push(json!({"type": "text", "text": prompt}));
        Value::Array(blocks)
    }

    pub fn start(cfg: &TurnConfig, prompt: &str, tx: Sender<Event>) -> Result<ApiTurn, String> {
        let key = endpoint::get_key(&cfg.endpoint);
        if key.is_none() && cfg.endpoint.needs_key() {
            return Err(format!("No API key for {}. Add one in Settings > Chat or set ANTHROPIC_API_KEY.", cfg.endpoint.name));
        }
        start_with_key(cfg, prompt, key.unwrap_or_default(), tx)
    }

    pub(super) fn start_with_key(cfg: &TurnConfig, prompt: &str, key: String, tx: Sender<Event>) -> Result<ApiTurn, String> {
        let mut messages = tools::load_history(cfg.api_history.as_ref())?;
        messages.push(json!({"role": "user", "content": user_content(prompt, &cfg.images)}));
        let cfg = cfg.clone();
        ApiTurn::spawn("chat-anthropic", move |stop, approvals| {
            let w = Worker { cfg, key, sink: Sink::new(tx, stop), messages, approvals };
            w.run();
        })
    }

    // ------------------------------------------------------------------ worker

    struct Worker {
        cfg: TurnConfig,
        key: String,
        sink: Sink,
        messages: Vec<Value>,
        approvals: Receiver<(String, bool)>,
    }

    struct Round {
        content: Vec<Value>,
        stop_reason: Option<String>,
        details: Option<Value>,
        usage: Usage,
    }

    impl Worker {
        fn run(mut self) {
            if let Err(e) = tools::block_on(self.turn()) {
                self.emit(Event::Error(e));
            }
            tools::persist(self.cfg.api_history.as_ref(), &self.messages);
            // Persist first: the app deletes or reopens the chat as soon as it sees the end.
            self.sink.flush();
        }

        fn stopped(&self) -> bool {
            self.sink.stopped()
        }

        fn emit(&self, e: Event) {
            self.sink.emit(e);
        }

        fn model(&self) -> &str {
            self.cfg.model.as_deref().unwrap_or(&self.cfg.endpoint.model)
        }

        fn persist(&self) {
            tools::persist(self.cfg.api_history.as_ref(), &self.messages);
        }

        /// A server that reports no usage gets no usage row.
        fn finish(&self, total: &mut Usage, stop_reason: &str) {
            let usage = (*total != Usage::default()).then(|| {
                total.cost_usd = endpoint::cost(self.model(), total);
                total.clone()
            });
            self.emit(Event::TurnEnd { usage, session: None, stop_reason: Some(stop_reason.into()) });
        }

        fn push_assistant(&mut self, content: Vec<Value>) {
            if !content.is_empty() {
                self.messages.push(json!({"role": "assistant", "content": content}));
            }
        }

        fn fail(&self, m: String) {
            self.emit(Event::Error(tools::redact(&m, &self.key)));
        }

        async fn turn(&mut self) {
            let client = match tools::client() {
                Ok(c) => c,
                Err(e) => return self.emit(Event::Error(e)),
            };
            let url = match self.cfg.api_base_url.clone().or_else(|| std::env::var(BASE_URL_ENV).ok()) {
                Some(base) => format!("{}/v1/messages", base.trim_end_matches('/')),
                None => self.cfg.endpoint.url(),
            };
            let tools = tools_json(self.cfg.endpoint.is_claude_host());
            let specs = crate::chat::tools::tool_specs();
            let mut total = Usage::default();
            let (mut rounds, mut continued) = (0, false);
            loop {
                if self.stopped() {
                    return self.finish(&mut total, "stopped");
                }
                let body = build_body(&self.cfg, &tools, &self.messages);
                let round = match self.stream(&client, &url, &body).await {
                    Ok(r) => r,
                    Err(Halt::Stopped) => return self.finish(&mut total, "stopped"),
                    Err(Halt::Reported) => return,
                    Err(Halt::Fail(m)) => return self.fail(m),
                };
                self.emit(Event::Context(round.usage.input + round.usage.cache_read + round.usage.cache_write));
                total.input += round.usage.input;
                total.output += round.usage.output;
                total.cache_read += round.usage.cache_read;
                total.cache_write += round.usage.cache_write;
                let uses: Vec<&Value> = round.content.iter().filter(|b| b["type"] == "tool_use").collect();
                match round.stop_reason.as_deref() {
                    Some("refusal") => {
                        let cat = round.details.as_ref().and_then(|d| d["category"].as_str());
                        let msg = match cat {
                            Some(c) => format!("The model declined this request (category: {c})."),
                            None => "The model declined this request.".to_string(),
                        };
                        return self.emit(Event::Error(msg));
                    }
                    Some("tool_use") => {
                        if rounds >= MAX_TOOL_ROUNDS {
                            return self.emit(Event::Error(format!("Stopped after {MAX_TOOL_ROUNDS} tool rounds; send a message to continue.")));
                        }
                        // A stop mid-round still records what ran, so the next turn matches the document.
                        let (results, stopped) = self.run_tools(&specs, &uses).await;
                        self.push_assistant(round.content.clone());
                        self.messages.push(json!({"role": "user", "content": results}));
                        rounds += 1;
                        self.persist();
                        if stopped {
                            return self.finish(&mut total, "stopped");
                        }
                    }
                    Some("max_tokens") if !continued => {
                        continued = true;
                        // Cut-off tool inputs are never run; answer them so the history stays valid.
                        let mut follow: Vec<Value> = uses.iter().map(|u| tool_result(u["id"].as_str().unwrap_or_default(), &Err(CUT_OFF.into()))).collect();
                        follow.push(json!({"type": "text", "text": CONTINUE_NOTE}));
                        self.push_assistant(round.content.clone());
                        self.messages.push(json!({"role": "user", "content": follow}));
                        self.persist();
                    }
                    Some("pause_turn") => {
                        if rounds >= MAX_TOOL_ROUNDS {
                            return self.emit(Event::Error(format!("Stopped after {MAX_TOOL_ROUNDS} rounds; send a message to continue.")));
                        }
                        rounds += 1;
                        self.push_assistant(round.content.clone());
                        self.persist();
                    }
                    other => {
                        self.push_assistant(round.content.clone());
                        // A second max_tokens can leave unanswered tool_use blocks; answer them so replay is valid.
                        if !uses.is_empty() {
                            let results: Vec<Value> = uses.iter().map(|u| tool_result(u["id"].as_str().unwrap_or_default(), &Err(CUT_OFF.into()))).collect();
                            self.messages.push(json!({"role": "user", "content": results}));
                        }
                        self.persist();
                        return self.finish(&mut total, other.unwrap_or("end_turn"));
                    }
                }
            }
        }

        /// One request; forwards text, thinking and tool starts as they stream.
        async fn stream(&self, client: &reqwest::Client, url: &str, body: &Value) -> Result<Round, Halt> {
            let headers = build_headers(self.model(), &self.key, self.cfg.endpoint.is_claude_host());
            let label = format!("{} API", self.cfg.endpoint.name);
            let resp = tools::post(&self.sink.stop, client, url, headers, body, &label).await?;
            let mut parser = SseParser::default();
            let (mut done, mut errored) = (false, false);
            tools::read_stream(&self.sink.stop, resp, |text| {
                for ev in parser.push(text) {
                    match ev {
                        Event::TurnEnd { .. } => done = true,
                        Event::Error(m) => {
                            errored = true;
                            self.emit(Event::Error(m));
                        }
                        other => self.emit(other),
                    }
                }
                done
            })
            .await?;
            if !done {
                return Err(if errored { Halt::Reported } else { Halt::Fail("The stream ended before the reply was complete.".into()) });
            }
            Ok(Round {
                content: parser.final_content(),
                stop_reason: parser.stop_reason().map(str::to_string),
                details: parser.stop_details().cloned(),
                usage: parser.usage().clone(),
            })
        }

        /// Runs the `tool_use` blocks in order. Returns one result per block; once stopped, the rest
        /// (and the interrupted one) get an error result. The flag says whether a stop happened.
        async fn run_tools(&self, specs: &[Value], uses: &[&Value]) -> (Vec<Value>, bool) {
            let emit = |e| self.emit(e);
            let ctl = Ctl { stop: &self.sink.stop, approvals: &self.approvals, dispatch: self.cfg.tools.as_ref(), emit: &emit };
            let (mut results, mut stopped) = (vec![], false);
            for u in uses {
                let (id, name, args) = (u["id"].as_str().unwrap_or_default(), u["name"].as_str().unwrap_or_default(), &u["input"]);
                let r = if stopped {
                    Err(STOPPED_BY_USER.to_string())
                } else {
                    ctl.run_one(specs, id, name, args).await.unwrap_or_else(|| {
                        stopped = true;
                        Err(STOPPED_BY_USER.to_string())
                    })
                };
                self.emit(Ctl::end_event(id, &r));
                results.push(tool_result(id, &r));
            }
            (results, stopped)
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::chat::events::ToolDispatch;
        use crate::chat::tools::test_support::{serve, serve_raw};
        use crate::chat::events::Turn;
        use std::io::{Read, Write};
        use std::sync::atomic::Ordering;
        use std::sync::mpsc::channel;
        use std::sync::{Arc, Mutex};
        use std::time::Duration;

        #[test]
        fn attachments_become_image_blocks_before_the_text() {
            assert_eq!(user_content("hi", &[]), json!("hi"));
            let img = crate::chat::events::ImageAttachment { mime: "image/png".into(), data: vec![1, 2, 3] };
            let v = user_content("hi", &[img]);
            assert_eq!(v[0]["source"], json!({"type": "base64", "media_type": "image/png", "data": "AQID"}));
            assert_eq!(v[1], json!({"type": "text", "text": "hi"}));
        }

        const ROUND1: &str = include_str!("../../tests/fixtures/chat/anthropic_round1_tools.sse");
        const ROUND2: &str = include_str!("../../tests/fixtures/chat/anthropic_round2_end.sse");
        const DELETE: &str = include_str!("../../tests/fixtures/chat/anthropic_delete_pages.sse");
        const REFUSAL: &str = include_str!("../../tests/fixtures/chat/anthropic_refusal.sse");
        const TEST_KEY: &str = "sk-ant-test-secret-123";

        fn fake_tools(calls: Arc<Mutex<Vec<String>>>) -> ToolDispatch {
            ToolDispatch(Arc::new(move |c| {
                calls.lock().unwrap().push(c.name.clone());
                let out = match c.name.as_str() {
                    "get_basic_info" => vec![Out::Text("info".into()), Out::Image { data: vec![1, 2, 3], mime: "image/png".into() }],
                    _ => vec![Out::Text("created".into())],
                };
                let _ = c.reply.send(Ok(out));
                true
            }))
        }

        fn cfg(name: &str, url: String, tools: Option<ToolDispatch>) -> TurnConfig {
            let dir = std::env::temp_dir().join(format!("papercraft-api-{}-{name}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            TurnConfig { system: "sys".into(), api_history: Some(dir.join("chats").join("c.api.json")), api_base_url: Some(url), tools, ..Default::default() }
        }

        /// Runs a turn; `on_event` can react (approve) before the next event is read.
        fn run(cfg: &TurnConfig, mut on_event: impl FnMut(&ApiTurn, &Event)) -> Vec<Event> {
            let (tx, rx) = channel();
            let turn = start_with_key(cfg, "make a board", TEST_KEY.into(), tx).unwrap();
            let mut out = vec![];
            while let Ok(e) = rx.recv_timeout(Duration::from_secs(10)) {
                on_event(&turn, &e);
                let end = matches!(e, Event::TurnEnd { .. } | Event::Error(_));
                out.push(e);
                if end {
                    break;
                }
            }
            out
        }

        fn history(cfg: &TurnConfig) -> Vec<Value> {
            serde_json::from_slice(&std::fs::read(cfg.api_history.as_ref().unwrap()).unwrap()).unwrap()
        }

        #[test]
        fn request_shape() {
            let tools = tools_json(true);
            let c = TurnConfig { system: "SYS".into(), ..Default::default() };
            let b = build_body(&c, &tools, &[json!({"role": "user", "content": "hi"})]);
            assert_eq!((b["model"].as_str(), b["max_tokens"].as_u64(), b["stream"].as_bool()), (Some("claude-opus-5-5"), Some(64_000), Some(true)));
            assert_eq!(b["thinking"], json!({"type": "adaptive", "display": "summarized"}));
            assert_eq!(b["output_config"], json!({"effort": "medium"}));
            assert_eq!(b["fallbacks"], "default");
            assert!(b.get("tool_choice").is_none());
            assert_eq!(b["system"], json!([{"type": "text", "text": "SYS", "cache_control": {"type": "ephemeral"}}]));
            assert_eq!(b["messages"], json!([{"role": "user", "content": "hi"}]));
            let ts = b["tools"].as_array().unwrap();
            let names: Vec<String> = crate::mcp::server::tool_list().iter().map(|t| t.name.to_string()).collect();
            assert_eq!(ts.iter().map(|t| t["name"].as_str().unwrap().to_string()).collect::<Vec<_>>(), names);
            assert!(ts.iter().all(|t| t["eager_input_streaming"] == true && t["input_schema"]["type"] == "object" && t["description"].is_string()));
            let with_cc: Vec<_> = ts.iter().filter(|t| t.get("cache_control").is_some()).collect();
            assert_eq!(with_cc.len(), 1);
            assert_eq!(ts.last().unwrap()["cache_control"], json!({"type": "ephemeral"}));
            assert_eq!(tools_json(true), tools, "stable across calls");
        }

        #[test]
        fn other_hosts_get_a_plain_request() {
            use crate::chat::endpoint::Endpoint;
            let ep = Endpoint { base_url: "http://127.0.0.1:9/anthropic".into(), model: "other-model".into(), ..Default::default() };
            let c = TurnConfig { system: "SYS".into(), effort: Some("high".into()), endpoint: ep, ..Default::default() };
            let tools = tools_json(false);
            let b = build_body(&c, &tools, &[json!({"role": "user", "content": "hi"})]);
            let mut keys: Vec<&str> = b.as_object().unwrap().keys().map(String::as_str).collect();
            keys.sort_unstable();
            assert_eq!(keys, ["max_tokens", "messages", "model", "stream", "system", "tools"]);
            assert_eq!((b["model"].as_str(), b["system"].as_str()), (Some("other-model"), Some("SYS")));
            assert_eq!(b["max_tokens"], 16_384);
            let text = b.to_string();
            for claude_only in ["effort", "thinking", "fallbacks", "cache_control", "eager_input_streaming"] {
                assert!(!text.contains(claude_only), "{claude_only} sent to another host");
            }
            let names: Vec<String> = crate::mcp::server::tool_list().iter().map(|t| t.name.to_string()).collect();
            assert_eq!(b["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap().to_string()).collect::<Vec<_>>(), names);
            // A Claude model name on another host (a proxy) still gets no beta header.
            let h = build_headers("claude-opus-5-5", "K", false);
            assert!(h.iter().all(|(k, _)| *k != "anthropic-beta") && h.contains(&("x-api-key", "K".into())));
            assert!(build_headers("m", "", false).iter().all(|(k, _)| *k != "x-api-key"), "no key, no key header");
        }

        #[test]
        fn claude_host_keeps_every_claude_field() {
            let c = TurnConfig { system: "S".into(), ..Default::default() };
            assert!(c.endpoint.is_claude_host());
            let b = build_body(&c, &tools_json(true), &[]);
            for k in ["thinking", "output_config", "fallbacks"] {
                assert!(b.get(k).is_some(), "{k}");
            }
            assert_eq!(b["system"][0]["cache_control"], json!({"type": "ephemeral"}));
            let c = TurnConfig { system: "S".into(), ds_digest: "DS".into(), ..Default::default() };
            let b = build_body(&c, &tools_json(true), &[]);
            assert_eq!(b["system"][1], json!({"type": "text", "text": "DS", "cache_control": {"type": "ephemeral"}}), "the digest is its own block");
            let other = TurnConfig { endpoint: crate::chat::endpoint::Endpoint { base_url: "http://h".into(), ..Default::default() }, ..c };
            assert_eq!(build_body(&other, &[], &[])["system"], "S\n\nDS");
        }

        #[test]
        fn fallbacks_and_beta_per_model() {
            for (model, fb) in [("claude-opus-5-5", true), ("claude-sonnet-5-5", true), ("claude-haiku-5-5", false)] {
                let c = TurnConfig { model: Some(model.into()), effort: Some("high".into()), ..Default::default() };
                let b = build_body(&c, &[], &[]);
                assert_eq!(b.get("fallbacks").is_some(), fb, "{model}");
                assert_eq!(b["output_config"]["effort"], "high");
                let h = build_headers(model, "K", true);
                assert_eq!(h.iter().any(|(k, v)| *k == "anthropic-beta" && v == "server-side-fallback-2026-07-01"), fb, "{model}");
                assert!(h.contains(&("x-api-key", "K".into())) && h.contains(&("anthropic-version", "2023-06-01".into())));
            }
        }

        #[test]
        fn validator() {
            let t = tools::tool_specs();
            let schema = &t.iter().find(|t| t["name"] == "create_artboard").unwrap()["inputSchema"];
            assert!(tools::validate_input(schema, &json!({"name": "A", "styles": {"width": "1px", "height": "2px"}})).is_ok());
            assert!(tools::validate_input(schema, &json!({"styles": {"width": "1px", "height": "2px"}})).unwrap_err().contains("name"));
            assert!(tools::validate_input(schema, &json!({"name": 5, "styles": {"width": "1px", "height": "2px"}})).unwrap_err().contains("string"));
            assert!(tools::validate_input(schema, &json!({"name": "A", "styles": {"width": "1px"}})).unwrap_err().contains("height"));
            assert!(tools::validate_input(schema, &json!("nope")).is_err());
        }

        #[test]
        fn validator_keywords() {
            let schema = json!({"type": "object", "additionalProperties": false, "required": ["m"], "properties": {
                "m": {"type": "string", "enum": ["a", "b"]},
                "n": {"type": "integer", "minimum": 1, "maximum": 5},
                "v": {"anyOf": [{"type": "string"}, {"type": "number"}]},
                "x": {"type": "object", "additionalProperties": {"anyOf": [{"type": "string"}, {"type": "number"}]}},
                "l": {"type": "array", "minItems": 1}}});
            assert!(tools::validate_input(&schema, &json!({"m": "a", "n": 3, "v": 1.5, "x": {"k": "s", "j": 2}, "l": [1]})).is_ok());
            for (bad, word) in [
                (json!({"m": "c"}), "one of"),
                (json!({"m": "a", "n": 0}), "minimum"),
                (json!({"m": "a", "n": 6}), "maximum"),
                (json!({"m": "a", "v": true}), "anyOf"),
                (json!({"m": "a", "x": {"k": true}}), "anyOf"),
                (json!({"m": "a", "extra": 1}), "unexpected field"),
                (json!({"m": "a", "l": []}), "at least"),
            ] {
                assert!(tools::validate_input(&schema, &bad).unwrap_err().contains(word), "{bad}");
            }
        }

        /// One tool_use-style SSE round per call: `stop` is the stop_reason, each id a `get_basic_info` call.
        fn round(stop: &str, ids: &[&str]) -> &'static str {
            let mut s = String::from("event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":1}}}\n\n");
            for (i, id) in ids.iter().enumerate() {
                s += &format!("data: {{\"type\":\"content_block_start\",\"index\":{i},\"content_block\":{{\"type\":\"tool_use\",\"id\":\"{id}\",\"name\":\"get_basic_info\",\"input\":{{}}}}}}\n\n");
                s += &format!("data: {{\"type\":\"content_block_stop\",\"index\":{i}}}\n\n");
            }
            s += &format!("data: {{\"type\":\"message_delta\",\"delta\":{{\"stop_reason\":\"{stop}\"}},\"usage\":{{\"output_tokens\":1}}}}\n\n");
            s += "data: {\"type\":\"message_stop\"}\n\n";
            Box::leak(s.into_boxed_str())
        }

        /// Every assistant tool_use id is answered by a tool_result in the next user message.
        fn assert_replayable(h: &[Value]) {
            for (i, m) in h.iter().enumerate().filter(|(_, m)| m["role"] == "assistant") {
                for u in m["content"].as_array().unwrap().iter().filter(|b| b["type"] == "tool_use") {
                    let next = h[i + 1]["content"].as_array().unwrap();
                    assert!(next.iter().any(|b| b["type"] == "tool_result" && b["tool_use_id"] == u["id"]), "unanswered {}", u["id"]);
                }
            }
        }

        #[test]
        fn second_max_tokens_answers_tool_uses() {
            let (url, _) = serve(vec![round("max_tokens", &["t1"]), round("max_tokens", &["t2"])]);
            let c = cfg("max2", url, None);
            let ev = run(&c, |_, _| {});
            assert!(matches!(ev.last(), Some(Event::TurnEnd { .. })), "{ev:?}");
            let h = history(&c);
            assert_replayable(&h);
            assert_eq!(h.last().unwrap()["content"][0]["tool_use_id"], "t2");
        }

        #[test]
        fn pause_turn_counts_toward_round_cap() {
            let (url, seen) = serve(vec![round("pause_turn", &[]); MAX_TOOL_ROUNDS + 1]);
            let ev = run(&cfg("pause", url, None), |_, _| {});
            let Some(Event::Error(m)) = ev.last() else { panic!("{ev:?}") };
            assert!(m.contains("Stopped after"), "{m}");
            assert_eq!(seen.lock().unwrap().len(), MAX_TOOL_ROUNDS + 1);
        }

        #[test]
        fn stop_mid_round_persists_results() {
            let (url, _) = serve(vec![round("tool_use", &["t1", "t2", "t3"])]);
            let held: Arc<Mutex<Vec<Box<dyn std::any::Any + Send>>>> = Arc::default();
            let n = Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let tools = ToolDispatch(Arc::new(move |c| {
                if n.fetch_add(1, Ordering::SeqCst) == 0 {
                    let _ = c.reply.send(Ok(vec![Out::Text("done".into())]));
                } else {
                    held.lock().unwrap().push(Box::new(c.reply)); // never answered: the tool "hangs"
                }
                true
            }));
            let c = cfg("midstop", url, Some(tools));
            let ev = run(&c, |t, e| {
                if matches!(e, Event::ToolEnd { id, .. } if id == "t1") {
                    t.stop.store(true, Ordering::SeqCst);
                }
            });
            assert!(matches!(ev.last(), Some(Event::TurnEnd { stop_reason: Some(r), .. }) if r == "stopped"), "{ev:?}");
            let h = history(&c);
            assert_replayable(&h);
            let r = h.last().unwrap()["content"].as_array().unwrap().clone();
            assert_eq!(r.len(), 3);
            assert!(r[0]["is_error"].is_null());
            assert!(r[1]["is_error"] == true && r[2]["is_error"] == true);
            assert!(r[1]["content"][0]["text"].as_str().unwrap().contains("stopped by user"));
        }

        #[test]
        fn stop_interrupts_a_stalled_error_body() {
            let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let url = format!("http://{}", l.local_addr().unwrap());
            std::thread::spawn(move || {
                let (mut s, _) = l.accept().unwrap();
                let mut buf = [0; 8192];
                let _ = s.read(&mut buf);
                let _ = write!(s, "HTTP/1.1 500 Internal Server Error\r\ncontent-length: 1000\r\n\r\npartial");
                std::thread::sleep(Duration::from_secs(20));
            });
            let (tx, rx) = channel();
            let mut turn = start_with_key(&cfg("errstop", url, None), "x", TEST_KEY.into(), tx).unwrap();
            std::thread::sleep(Duration::from_millis(400));
            turn.stop();
            let e = rx.recv_timeout(Duration::from_secs(3)).unwrap();
            assert!(matches!(e, Event::TurnEnd { stop_reason: Some(ref r), .. } if r == "stopped"), "{e:?}");
        }

        #[test]
        fn tool_loop_two_rounds() {
            let (url, seen) = serve(vec![ROUND1, ROUND2]);
            let calls: Arc<Mutex<Vec<String>>> = Arc::default();
            let c = cfg("loop", url, Some(fake_tools(calls.clone())));
            let ev = run(&c, |_, _| {});
            // One Context event per request: the latest call's input and cache tokens (120, then 160).
            let contexts: Vec<u64> = ev.iter().filter_map(|e| if let Event::Context(n) = e { Some(*n) } else { None }).collect();
            assert_eq!(contexts, [120, 160], "{ev:?}");
            let ev: Vec<Event> = ev.into_iter().filter(|e| !matches!(e, Event::Context(_))).collect();
            let kinds: Vec<&str> = ev
                .iter()
                .map(|e| match e {
                    Event::Thinking(_) => "think",
                    Event::Text(_) => "text",
                    Event::ToolStart { .. } => "start",
                    Event::ToolEnd { .. } => "end",
                    Event::TurnEnd { .. } => "turn",
                    _ => "other",
                })
                .collect();
            assert_eq!(kinds, ["think", "text", "start", "start", "end", "end", "text", "text", "turn"], "{ev:?}");
            assert_eq!(*calls.lock().unwrap(), ["create_artboard", "get_basic_info"]);
            let Event::ToolEnd { id, result: Some(r), error: None } = &ev[5] else { panic!() };
            assert_eq!(id, "toolu_B");
            assert_eq!(r[1]["type"], "image");
            let Event::TurnEnd { usage: Some(u), stop_reason, .. } = ev.last().unwrap() else { panic!() };
            assert_eq!(stop_reason.as_deref(), Some("end_turn"));
            assert_eq!((u.input, u.output, u.cache_read, u.cache_write), (140, 62, 120, 20));
            assert!(u.cost_usd.unwrap() > 0.0);

            let seen = seen.lock().unwrap();
            let (h1, b1) = &seen[0];
            assert!(h1.contains(&format!("x-api-key: {}", TEST_KEY.to_lowercase())));
            assert!(h1.iter().any(|h| h.starts_with("anthropic-beta: server-side-fallback")));
            assert_eq!(b1["messages"].as_array().unwrap().len(), 1);
            let m = seen[1].1["messages"].as_array().unwrap();
            assert_eq!(m.len(), 3);
            assert_eq!(m[1]["role"], "assistant");
            assert_eq!(
                m[1]["content"],
                json!([
                    {"type": "thinking", "thinking": "Plan the board.", "signature": "SIG1"},
                    {"type": "text", "text": "Creating it."},
                    {"type": "tool_use", "id": "toolu_A", "name": "create_artboard", "input": {"name": "Home", "styles": {"width": "1440px", "height": "900px"}}},
                    {"type": "tool_use", "id": "toolu_B", "name": "get_basic_info", "input": {}},
                ])
            );
            assert_eq!(m[2]["role"], "user");
            let tr = m[2]["content"].as_array().unwrap();
            assert_eq!(tr.len(), 2);
            assert_eq!((tr[0]["tool_use_id"].as_str(), tr[1]["tool_use_id"].as_str()), (Some("toolu_A"), Some("toolu_B")));
            assert_eq!(tr[0]["content"], json!([{"type": "text", "text": "created"}]));
            assert_eq!(tr[1]["content"][1]["source"], json!({"type": "base64", "media_type": "image/png", "data": "AQID"}));
            assert!(tr.iter().all(|t| t.get("is_error").is_none()));

            let h = history(&c);
            assert_eq!(h.len(), 4);
            assert_eq!(h[..3], m[..]);
            assert_eq!(h[3]["content"][0]["text"], "Done: board ready.");
            assert!(!serde_json::to_string(&h).unwrap().contains(TEST_KEY), "key must never reach the history file");
        }

        #[test]
        fn denied_destructive_tool_is_not_run() {
            let (url, seen) = serve(vec![DELETE, ROUND2]);
            let calls: Arc<Mutex<Vec<String>>> = Arc::default();
            let c = cfg("deny", url, Some(fake_tools(calls.clone())));
            let ev = run(&c, |t, e| {
                if let Event::NeedsApproval { id, .. } = e {
                    t.approve(id, false);
                }
            });
            assert!(ev.iter().any(|e| matches!(e, Event::NeedsApproval { name, .. } if name == "delete_pages")));
            assert!(calls.lock().unwrap().is_empty());
            assert!(matches!(ev.last(), Some(Event::TurnEnd { .. })));
            let tr = &seen.lock().unwrap()[1].1["messages"][2]["content"][0];
            assert_eq!(tr["is_error"], true);
            assert!(tr["content"][0]["text"].as_str().unwrap().contains("denied"));
        }

        #[test]
        fn approved_destructive_tool_runs() {
            let (url, _) = serve(vec![DELETE, ROUND2]);
            let calls: Arc<Mutex<Vec<String>>> = Arc::default();
            let c = cfg("allow", url, Some(fake_tools(calls.clone())));
            run(&c, |t, e| {
                if let Event::NeedsApproval { id, .. } = e {
                    t.approve(id, true);
                }
            });
            assert_eq!(*calls.lock().unwrap(), ["delete_pages"]);
        }

        #[test]
        fn refusal_reports_category() {
            let (url, _) = serve(vec![REFUSAL]);
            let c = cfg("refusal", url, None);
            let ev = run(&c, |_, _| {});
            let Some(Event::Error(m)) = ev.last() else { panic!("{ev:?}") };
            assert!(m.contains("cyber"), "{m}");
            assert!(!ev.iter().any(|e| matches!(e, Event::TurnEnd { .. })));
        }

        #[test]
        fn http_error_shows_api_message() {
            let b = r#"{"type":"error","error":{"type":"authentication_error","message":"invalid x-api-key"}}"#;
            let (url, _) = serve_raw(vec![format!("HTTP/1.1 401 Unauthorized\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{b}", b.len())]);
            let ev = run(&cfg("401", url, None), |_, _| {});
            let Some(Event::Error(m)) = ev.last() else { panic!("{ev:?}") };
            assert!(m.contains("401") && m.contains("invalid x-api-key") && !m.contains(TEST_KEY), "{m}");
        }

        #[test]
        fn history_is_on_disk_before_the_end_event() {
            // The app reopens or deletes the chat the moment it sees the end: the worker must
            // have written (and be done with) `chats/<id>.api.json` by then.
            let (url, _) = serve(vec![ROUND2]);
            let c = cfg("persist_first_ok", url, None);
            let path = c.api_history.clone().unwrap();
            let mut on_disk = None;
            let ev = run(&c, |_, e| {
                if matches!(e, Event::TurnEnd { .. }) {
                    on_disk = Some(path.exists());
                }
            });
            assert!(matches!(ev.last(), Some(Event::TurnEnd { .. })));
            assert_eq!(on_disk, Some(true));

            let b = r#"{"type":"error","error":{"message":"nope"}}"#;
            let (url, _) = serve_raw(vec![format!("HTTP/1.1 500 Internal Server Error\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{b}", b.len())]);
            let c = cfg("persist_first_err", url, None);
            let path = c.api_history.clone().unwrap();
            let mut on_disk = None;
            let ev = run(&c, |_, e| {
                if matches!(e, Event::Error(_)) {
                    on_disk = Some(path.exists());
                }
            });
            assert!(matches!(ev.last(), Some(Event::Error(_))));
            assert_eq!(on_disk, Some(true));
        }

        #[test]
        fn stop_ends_a_hanging_stream_fast() {
            let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let url = format!("http://{}", l.local_addr().unwrap());
            std::thread::spawn(move || {
                let (mut s, _) = l.accept().unwrap();
                let mut buf = [0; 8192];
                let _ = s.read(&mut buf);
                let _ = write!(s, "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\n\r\n: hold\n\n");
                std::thread::sleep(Duration::from_secs(20));
            });
            let (tx, rx) = channel();
            let mut turn = start_with_key(&cfg("stop", url, None), "x", TEST_KEY.into(), tx).unwrap();
            std::thread::sleep(Duration::from_millis(400));
            let t = std::time::Instant::now();
            turn.stop();
            let e = rx.recv_timeout(Duration::from_secs(3)).unwrap();
            assert!(matches!(e, Event::TurnEnd { stop_reason: Some(ref r), .. } if r == "stopped"), "{e:?}");
            while turn.is_running() && t.elapsed() < Duration::from_secs(3) {
                std::thread::sleep(Duration::from_millis(20));
            }
            assert!(!turn.is_running() && t.elapsed() < Duration::from_secs(3));
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    // SYNTHETIC fixtures, hand-written from the documented Messages API streaming shapes.
    const TOOLS: &str = include_str!("../../tests/fixtures/chat/anthropic_tools.sse");
    const REFUSAL: &str = include_str!("../../tests/fixtures/chat/anthropic_refusal.sse");
    const MAX: &str = include_str!("../../tests/fixtures/chat/anthropic_max_tokens.sse");

    /// Feed in awkward 7-byte chunks (fixtures are ASCII) to prove line buffering.
    fn run(src: &str) -> (SseParser, Vec<Event>) {
        let mut p = SseParser::default();
        let ev = src.as_bytes().chunks(7).flat_map(|c| p.push(std::str::from_utf8(c).unwrap())).collect();
        (p, ev)
    }

    #[test]
    fn tools_turn() {
        let (p, ev) = run(TOOLS);
        assert_eq!(ev[0], Event::Thinking("Need an artboard".into()));
        assert_eq!(ev[1], Event::Thinking(" then a frame.".into()));
        assert_eq!(ev[2], Event::Text("Creating ".into()));
        assert_eq!(ev[3], Event::Text("the home screen.".into()));
        assert_eq!(ev[4], Event::ToolStart { id: "toolu_A".into(), name: "create_artboard".into(), args: json!({"name":"Home","width":1440}) });
        assert_eq!(ev[5], Event::ToolStart { id: "toolu_B".into(), name: "get_basic_info".into(), args: json!({}) });
        let Event::TurnEnd { usage: Some(u), stop_reason, .. } = &ev[6] else { panic!("{ev:?}") };
        assert_eq!((u.input, u.output, u.cache_read, u.cache_write), (120, 87, 900, 30));
        assert_eq!(stop_reason.as_deref(), Some("tool_use"));
        assert_eq!(ev.len(), 7);
        assert_eq!(
            p.final_content(),
            vec![
                json!({"type":"thinking","thinking":"Need an artboard then a frame.","signature":"EqQBCkYIBxgC"}),
                json!({"type":"text","text":"Creating the home screen."}),
                json!({"type":"tool_use","id":"toolu_A","name":"create_artboard","input":{"name":"Home","width":1440}}),
                json!({"type":"tool_use","id":"toolu_B","name":"get_basic_info","input":{}}),
            ]
        );
    }

    #[test]
    fn missing_usage_thinking_and_cache_fields_are_tolerated() {
        // A compatible server: no usage anywhere, no thinking block, the tool input whole in the start event.
        let sse = "data: {\"type\":\"message_start\",\"message\":{\"id\":\"m\"}}\n\
                   data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\
                   data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hi\"}}\n\
                   data: {\"type\":\"content_block_stop\",\"index\":0}\n\
                   data: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"tool_use\",\"id\":\"t\",\"name\":\"get_basic_info\",\"input\":{\"a\":1}}}\n\
                   data: {\"type\":\"content_block_stop\",\"index\":1}\n\
                   data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"tool_use\"}}\n\
                   data: {\"type\":\"message_stop\"}\n";
        let (p, ev) = run(sse);
        assert_eq!(ev[1], Event::ToolStart { id: "t".into(), name: "get_basic_info".into(), args: json!({"a": 1}) });
        assert!(matches!(ev.last(), Some(Event::TurnEnd { stop_reason: Some(r), .. }) if r == "tool_use"));
        assert_eq!(p.usage(), &Usage::default());
    }

    #[test]
    fn refusal_carries_details() {
        let (p, ev) = run(REFUSAL);
        assert!(matches!(ev.last(), Some(Event::TurnEnd { stop_reason: Some(r), .. }) if r == "refusal"));
        assert_eq!(p.stop_details().unwrap()["category"], "cyber");
    }

    #[test]
    fn max_tokens_keeps_redacted_block() {
        let (p, ev) = run(MAX);
        assert_eq!(ev[0], Event::Text("A long answer that gets cut".into()));
        assert!(matches!(&ev[1], Event::TurnEnd { stop_reason: Some(r), .. } if r == "max_tokens"));
        assert_eq!(p.final_content()[0], json!({"type":"redacted_thinking","data":"ENC"}));
        assert_eq!(p.usage().output, 64);
    }

    #[test]
    fn odd_blocks_and_deltas_do_not_panic() {
        let mut p = SseParser::default();
        let ev = p.push("data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":\"oops\"}\n");
        assert!(matches!(&ev[..], [Event::Error(_)]));
        assert!(p.final_content().is_empty());
        p.push("data: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"tool_use\",\"id\":\"t\",\"name\":\"x\"}}\n");
        let ev = p.push("data: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"text_delta\",\"text\":\"hi\"}}\n");
        assert!(matches!(&ev[..], [Event::Error(_)]));
        assert_eq!(p.final_content()[0], json!({"type":"tool_use","id":"t","name":"x"}));
    }

    #[test]
    fn error_event_and_bad_tool_json() {
        let mut p = SseParser::default();
        let ev = p.push("data: {\"type\":\"error\",\"error\":{\"type\":\"overloaded_error\",\"message\":\"Overloaded\"}}\r\n");
        assert_eq!(ev, vec![Event::Error("overloaded_error: Overloaded".into())]);
        p.push("data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"tool_use\",\"id\":\"t\",\"name\":\"x\",\"input\":{}}}\n");
        p.push("data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"a\\\"\"}}\n");
        let ev = p.push("data: {\"type\":\"content_block_stop\",\"index\":0}\n");
        assert!(matches!(&ev[0], Event::Error(m) if m.contains("invalid tool input")));
        assert_eq!(p.final_content()[0]["input"], json!({}));
    }
}
