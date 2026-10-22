//! Chat Completions streaming (OpenAI-compatible endpoints): an SSE accumulator that turns the
//! response into [`Event`]s, and the tool loop. Same event stream, approvals, schema validation,
//! round cap and persist-then-emit rule as `anthropic`. No provider-specific request fields.
//!
//! History (`chats/<id>.api.json`) is the OpenAI `messages` array without the system message:
//! `user`, `assistant` (with `tool_calls`) and `tool` messages. An OpenAI `tool` message carries
//! text only, so a tool result with an image goes back as the text `[image omitted]` and the image
//! follows as an extra `user` message right after the tool messages of that round.

use super::events::Event;
use super::{Usage, endpoint};
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// One tool call of a finished reply.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolUse {
    pub id: String,
    pub name: String,
    /// The parsed arguments, or why they could not be parsed.
    pub input: Result<Value, String>,
}

#[derive(Debug, Default)]
struct Call {
    id: String,
    name: String,
    args: String,
}

#[derive(Debug, Default)]
pub struct ChatParser {
    buf: String,
    text: String,
    calls: BTreeMap<usize, Call>,
    uses: Vec<ToolUse>,
    ended: bool,
    finish: Option<String>,
    usage: Option<Usage>,
    /// Prompt tokens of the call (input and cache), for the context meter.
    prompt: u64,
    done: bool,
}

impl ChatParser {
    /// Feed any chunk of the response body; incomplete lines wait for the next call.
    pub fn push(&mut self, chunk: &str) -> Vec<Event> {
        self.buf.push_str(chunk);
        let mut out = vec![];
        while let Some(nl) = self.buf.find('\n') {
            let line: String = self.buf.drain(..=nl).collect();
            let Some(data) = line.trim_end().strip_prefix("data:") else { continue };
            let data = data.trim();
            if data == "[DONE]" {
                self.done = true;
                self.end_round(&mut out);
            } else if let Ok(v) = serde_json::from_str::<Value>(data) {
                self.chunk(&v, &mut out);
            }
        }
        out
    }

    /// `[DONE]` seen: the reply is complete.
    pub fn done(&self) -> bool {
        self.done
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn finish_reason(&self) -> Option<&str> {
        self.finish.as_deref()
    }

    pub fn tool_uses(&self) -> &[ToolUse] {
        &self.uses
    }

    /// None when the server sent no usage.
    pub fn usage(&self) -> Option<&Usage> {
        self.usage.as_ref()
    }

    pub fn prompt_tokens(&self) -> u64 {
        self.prompt
    }

    fn chunk(&mut self, v: &Value, out: &mut Vec<Event>) {
        if v["error"].is_object() || v["error"].is_string() {
            let e = &v["error"];
            let msg = e["message"].as_str().or_else(|| e.as_str()).unwrap_or("the server reported an error");
            return out.push(Event::Error(msg.to_string()));
        }
        if v["usage"].is_object() {
            self.merge_usage(&v["usage"]);
        }
        let choice = &v["choices"][0];
        let delta = &choice["delta"];
        if let Some(s) = delta["content"].as_str().filter(|s| !s.is_empty()) {
            self.text.push_str(s);
            out.push(Event::Text(s.to_string()));
        }
        // Reasoning text, when the server streams it under either common name.
        if let Some(s) = ["reasoning_content", "reasoning"].iter().find_map(|k| delta[*k].as_str().filter(|s| !s.is_empty())) {
            out.push(Event::Thinking(s.to_string()));
        }
        for (pos, tc) in delta["tool_calls"].as_array().into_iter().flatten().enumerate() {
            let idx = tc["index"].as_u64().map_or(pos, |i| i as usize);
            let c = self.calls.entry(idx).or_default();
            if c.id.is_empty() {
                c.id = tc["id"].as_str().unwrap_or_default().to_string();
            }
            let f = &tc["function"];
            if c.name.is_empty() {
                c.name = f["name"].as_str().unwrap_or_default().to_string();
            }
            match &f["arguments"] {
                Value::String(s) => c.args.push_str(s),
                Value::Null => {}
                other => c.args.push_str(&other.to_string()),
            }
        }
        if let Some(r) = choice["finish_reason"].as_str() {
            self.finish = Some(r.to_string());
            self.end_round(out);
        }
    }

    /// Parses the accumulated tool calls once and announces them.
    fn end_round(&mut self, out: &mut Vec<Event>) {
        if self.ended {
            return;
        }
        self.ended = true;
        self.finish.get_or_insert_with(|| "stop".into());
        for c in std::mem::take(&mut self.calls).into_values() {
            let id = if c.id.is_empty() { format!("call_{:08x}", fastrand::u32(..)) } else { c.id };
            let input = if c.args.trim().is_empty() {
                Ok(json!({}))
            } else {
                serde_json::from_str::<Value>(&c.args).map_err(|e| format!("invalid tool arguments JSON: {e}"))
            };
            out.push(Event::ToolStart { id: id.clone(), name: c.name.clone(), args: input.clone().unwrap_or_else(|_| json!({})) });
            self.uses.push(ToolUse { id, name: c.name, input });
        }
    }

    /// Usage can arrive in the last chunk only, or in every chunk; the latest wins.
    fn merge_usage(&mut self, u: &Value) {
        let n = |v: &Value| v.as_u64().unwrap_or(0);
        let prompt = n(&u["prompt_tokens"]);
        let cached = n(&u["prompt_tokens_details"]["cached_tokens"]);
        self.prompt = prompt;
        self.usage = Some(Usage { input: prompt.saturating_sub(cached), output: n(&u["completion_tokens"]), cache_read: cached, cache_write: 0, cost_usd: None });
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use api::{build_body, build_headers, start, tools_json};

/// The Chat Completions backend: request building and the streaming tool loop.
#[cfg(not(target_arch = "wasm32"))]
mod api {
    use super::*;
    use crate::chat::events::TurnConfig;
    use crate::chat::tools::{self, ApiTurn, Ctl, Halt, Sink, CONTINUE_NOTE, CUT_OFF, MAX_TOOL_ROUNDS, STOPPED_BY_USER};
    use crate::mcp::Out;
    use base64::{Engine, engine::general_purpose::STANDARD};
    use std::sync::mpsc::{Receiver, Sender};

    const IMAGE_OMITTED: &str = "[image omitted]";
    const IMAGE_NOTE: &str = "Image result of the tool call(s) above (a tool message cannot carry an image):";

    // ------------------------------------------------------------------ request

    /// No key header for a keyless local server.
    pub fn build_headers(key: &str) -> Vec<(&'static str, String)> {
        let mut h = vec![("content-type", "application/json".to_string())];
        if !key.is_empty() {
            h.push(("authorization", format!("Bearer {key}")));
        }
        h
    }

    /// The app's tools as `{"type":"function","function":{name,description,parameters}}`.
    pub fn tools_json() -> Vec<Value> {
        tools::tool_specs()
            .into_iter()
            .map(|t| json!({"type": "function", "function": {"name": t["name"], "description": t["description"], "parameters": t["inputSchema"]}}))
            .collect()
    }

    /// Only the portable fields: no `tool_choice`, no max-token or provider-specific settings.
    pub fn build_body(cfg: &TurnConfig, tools: &[Value], messages: &[Value]) -> Value {
        let model = cfg.model.as_deref().unwrap_or(&cfg.endpoint.model);
        let mut all = vec![];
        if !cfg.system.is_empty() {
            all.push(json!({"role": "system", "content": cfg.system}));
        }
        all.extend(messages.iter().cloned());
        json!({
            "model": model,
            "stream": true,
            "stream_options": {"include_usage": true},
            "tools": tools,
            "messages": all,
        })
    }

    fn data_uri(mime: &str, data: &[u8]) -> String {
        format!("data:{mime};base64,{}", STANDARD.encode(data))
    }

    /// A plain string, or the images then the text when images are attached.
    fn user_content(prompt: &str, images: &[crate::chat::events::ImageAttachment]) -> Value {
        if images.is_empty() {
            return json!(prompt);
        }
        let mut parts: Vec<Value> = images.iter().map(|i| json!({"type": "image_url", "image_url": {"url": data_uri(&i.mime, &i.data)}})).collect();
        parts.push(json!({"type": "text", "text": prompt}));
        Value::Array(parts)
    }

    /// The `tool` message content: the texts, with `[image omitted]` for each image.
    fn tool_text(r: &Result<Vec<Out>, String>) -> String {
        match r {
            Err(e) => format!("Error: {e}"),
            Ok(outs) => {
                let parts: Vec<&str> = outs
                    .iter()
                    .map(|o| match o {
                        Out::Text(t) => t.as_str(),
                        Out::Image { .. } => IMAGE_OMITTED,
                    })
                    .collect();
                if parts.is_empty() { "OK".into() } else { parts.join("\n") }
            }
        }
    }

    pub fn start(cfg: &TurnConfig, prompt: &str, tx: Sender<Event>) -> Result<ApiTurn, String> {
        let key = endpoint::get_key(&cfg.endpoint).unwrap_or_default();
        start_with_key(cfg, prompt, key, tx)
    }

    pub(super) fn start_with_key(cfg: &TurnConfig, prompt: &str, key: String, tx: Sender<Event>) -> Result<ApiTurn, String> {
        let mut messages = tools::load_history(cfg.api_history.as_ref())?;
        messages.push(json!({"role": "user", "content": user_content(prompt, &cfg.images)}));
        let cfg = cfg.clone();
        ApiTurn::spawn("chat-openai", move |stop, approvals| {
            Worker { cfg, key, sink: Sink::new(tx, stop), messages, approvals }.run();
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
        text: String,
        uses: Vec<ToolUse>,
        finish: Option<String>,
        usage: Option<Usage>,
        prompt: u64,
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

        fn emit(&self, e: Event) {
            self.sink.emit(e);
        }

        fn persist(&self) {
            tools::persist(self.cfg.api_history.as_ref(), &self.messages);
        }

        fn model(&self) -> &str {
            self.cfg.model.as_deref().unwrap_or(&self.cfg.endpoint.model)
        }

        fn fail(&self, m: String) {
            self.emit(Event::Error(tools::redact(&m, &self.key)));
        }

        /// `reported`: some call returned usage; otherwise the turn has no usage row.
        fn finish(&self, total: &mut Usage, reported: bool, stop_reason: &str) {
            let usage = reported.then(|| {
                total.cost_usd = endpoint::cost(self.model(), total);
                total.clone()
            });
            self.emit(Event::TurnEnd { usage, session: None, stop_reason: Some(stop_reason.into()) });
        }

        /// The assistant message, then one `tool` message per call, then the images (if any) as a
        /// user message. `results` has one entry per call, in order.
        fn push_round(&mut self, round: &Round, results: &[Result<Vec<Out>, String>]) {
            if round.text.is_empty() && round.uses.is_empty() {
                return;
            }
            let mut msg = json!({"role": "assistant", "content": if round.text.is_empty() { Value::Null } else { json!(round.text) }});
            if !round.uses.is_empty() {
                msg["tool_calls"] = round
                    .uses
                    .iter()
                    .map(|u| json!({"type": "function", "id": u.id, "function": {"name": u.name, "arguments": u.input.clone().unwrap_or_else(|_| json!({})).to_string()}}))
                    .collect();
            }
            self.messages.push(msg);
            let mut images: Vec<Value> = vec![];
            for (u, r) in round.uses.iter().zip(results) {
                self.messages.push(json!({"role": "tool", "tool_call_id": u.id, "content": tool_text(r)}));
                for o in r.iter().flatten() {
                    if let Out::Image { data, mime } = o {
                        images.push(json!({"type": "image_url", "image_url": {"url": data_uri(mime, data)}}));
                    }
                }
            }
            if !images.is_empty() {
                images.insert(0, json!({"type": "text", "text": IMAGE_NOTE}));
                self.messages.push(json!({"role": "user", "content": images}));
            }
        }

        async fn turn(&mut self) {
            let client = match tools::client() {
                Ok(c) => c,
                Err(e) => return self.emit(Event::Error(e)),
            };
            let url = match &self.cfg.api_base_url {
                Some(base) => format!("{}/chat/completions", base.trim_end_matches('/')),
                None => self.cfg.endpoint.url(),
            };
            let tools_body = tools_json();
            let specs = tools::tool_specs();
            let mut total = Usage::default();
            let (mut reported, mut rounds, mut continued) = (false, 0, false);
            loop {
                if self.sink.stopped() {
                    return self.finish(&mut total, reported, "stopped");
                }
                let body = build_body(&self.cfg, &tools_body, &self.messages);
                let round = match self.stream(&client, &url, &body).await {
                    Ok(r) => r,
                    Err(Halt::Stopped) => return self.finish(&mut total, reported, "stopped"),
                    Err(Halt::Reported) => return,
                    Err(Halt::Fail(m)) => return self.fail(m),
                };
                if let Some(u) = &round.usage {
                    reported = true;
                    self.emit(Event::Context(round.prompt));
                    total.input += u.input;
                    total.output += u.output;
                    total.cache_read += u.cache_read;
                }
                let finish = round.finish.as_deref().unwrap_or("stop");
                match finish {
                    "content_filter" => return self.emit(Event::Error("The server's content filter blocked the reply.".into())),
                    "length" if !continued => {
                        continued = true;
                        // Cut-off tool arguments are never run; answer them so the history stays valid.
                        let results: Vec<_> = round.uses.iter().map(|_| Err(CUT_OFF.to_string())).collect();
                        self.push_round(&round, &results);
                        self.messages.push(json!({"role": "user", "content": CONTINUE_NOTE}));
                        self.persist();
                    }
                    "length" => {
                        let results: Vec<_> = round.uses.iter().map(|_| Err(CUT_OFF.to_string())).collect();
                        self.push_round(&round, &results);
                        self.persist();
                        return self.finish(&mut total, reported, "length");
                    }
                    _ if !round.uses.is_empty() => {
                        if rounds >= MAX_TOOL_ROUNDS {
                            return self.emit(Event::Error(format!("Stopped after {MAX_TOOL_ROUNDS} tool rounds; send a message to continue.")));
                        }
                        // A stop mid-round still records what ran, so the next turn matches the document.
                        let (results, stopped) = self.run_tools(&specs, &round.uses).await;
                        self.push_round(&round, &results);
                        rounds += 1;
                        self.persist();
                        if stopped {
                            return self.finish(&mut total, reported, "stopped");
                        }
                    }
                    other => {
                        self.push_round(&round, &[]);
                        self.persist();
                        return self.finish(&mut total, reported, other);
                    }
                }
            }
        }

        /// One request; forwards text, reasoning and tool starts as they stream.
        async fn stream(&self, client: &reqwest::Client, url: &str, body: &Value) -> Result<Round, Halt> {
            let label = format!("{} API", self.cfg.endpoint.name);
            let resp = tools::post(&self.sink.stop, client, url, build_headers(&self.key), body, &label).await?;
            let mut parser = ChatParser::default();
            let mut errored = false;
            tools::read_stream(&self.sink.stop, resp, |text| {
                for ev in parser.push(text) {
                    if matches!(ev, Event::Error(_)) {
                        errored = true;
                    }
                    self.emit(ev);
                }
                parser.done()
            })
            .await?;
            // Some servers close the stream after the last chunk without `[DONE]`.
            let _ = parser.push("\n");
            if parser.finish_reason().is_none() {
                return Err(if errored { Halt::Reported } else { Halt::Fail("The stream ended before the reply was complete.".into()) });
            }
            Ok(Round {
                text: parser.text().to_string(),
                uses: parser.tool_uses().to_vec(),
                finish: parser.finish_reason().map(str::to_string),
                usage: parser.usage().cloned(),
                prompt: parser.prompt_tokens(),
            })
        }

        /// Runs the calls in order, one result each; once stopped, the rest (and the interrupted
        /// one) get an error result. The flag says whether a stop happened.
        async fn run_tools(&self, specs: &[Value], uses: &[ToolUse]) -> (Vec<Result<Vec<Out>, String>>, bool) {
            let emit = |e| self.emit(e);
            let ctl = Ctl { stop: &self.sink.stop, approvals: &self.approvals, dispatch: self.cfg.tools.as_ref(), emit: &emit };
            let (mut results, mut stopped) = (vec![], false);
            for u in uses {
                let r = match (&u.input, stopped) {
                    (_, true) => Err(STOPPED_BY_USER.to_string()),
                    (Err(e), false) => Err(format!("invalid arguments for `{}`: {e}", u.name)),
                    (Ok(args), false) => ctl.run_one(specs, &u.id, &u.name, args).await.unwrap_or_else(|| {
                        stopped = true;
                        Err(STOPPED_BY_USER.to_string())
                    }),
                };
                self.emit(Ctl::end_event(&u.id, &r));
                results.push(r);
            }
            (results, stopped)
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::chat::endpoint::{ApiFormat, Endpoint};
        use crate::chat::events::ToolDispatch;
        use crate::chat::tools::test_support::{serve, serve_raw};
        use std::sync::mpsc::channel;
        use std::sync::{Arc, Mutex};
        use std::time::Duration;

        const ROUND1: &str = include_str!("../../tests/fixtures/chat/openai_round1_tools.sse");
        const ROUND2: &str = include_str!("../../tests/fixtures/chat/openai_round2_end.sse");
        const TEST_KEY: &str = "sk-test-secret-456";

        fn endpoint() -> Endpoint {
            Endpoint { id: "local".into(), name: "Local".into(), format: ApiFormat::OpenAi, base_url: "http://127.0.0.1:1/v1".into(), model: "llama-x".into() }
        }

        fn cfg(name: &str, url: String, tools: Option<ToolDispatch>) -> TurnConfig {
            let dir = std::env::temp_dir().join(format!("papercraft-openai-{}-{name}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            TurnConfig {
                system: "sys".into(),
                api_history: Some(dir.join("chats").join("c.api.json")),
                api_base_url: Some(url),
                endpoint: endpoint(),
                tools,
                ..Default::default()
            }
        }

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

        #[test]
        fn request_shape_has_only_portable_fields() {
            let tools = tools_json();
            let c = TurnConfig { system: "SYS".into(), model: Some("m".into()), effort: Some("high".into()), endpoint: endpoint(), ..Default::default() };
            let b = build_body(&c, &tools, &[json!({"role": "user", "content": "hi"})]);
            let mut keys: Vec<&str> = b.as_object().unwrap().keys().map(String::as_str).collect();
            keys.sort_unstable();
            assert_eq!(keys, ["messages", "model", "stream", "stream_options", "tools"]);
            assert_eq!((b["model"].as_str(), b["stream"].as_bool()), (Some("m"), Some(true)));
            assert_eq!(b["stream_options"], json!({"include_usage": true}));
            assert_eq!(b["messages"], json!([{"role": "system", "content": "SYS"}, {"role": "user", "content": "hi"}]));
            let ts = b["tools"].as_array().unwrap();
            let names: Vec<String> = crate::mcp::server::tool_list().iter().map(|t| t.name.to_string()).collect();
            assert_eq!(ts.iter().map(|t| t["function"]["name"].as_str().unwrap().to_string()).collect::<Vec<_>>(), names);
            assert!(ts.iter().all(|t| t["type"] == "function" && t["function"]["parameters"]["type"] == "object" && t["function"]["description"].is_string()));
            assert_eq!(tools_json(), tools, "stable across calls");
            // The model falls back to the endpoint's.
            assert_eq!(build_body(&TurnConfig { endpoint: endpoint(), ..Default::default() }, &[], &[])["model"], "llama-x");
            // Even on the Claude host: nothing Claude-only in this format.
            let on_claude = TurnConfig { endpoint: Endpoint { format: ApiFormat::OpenAi, ..Default::default() }, ..Default::default() };
            let b = build_body(&on_claude, &tools, &[]);
            assert!(b.get("output_config").is_none() && b.get("thinking").is_none() && b.get("fallbacks").is_none());
            assert!(!b.to_string().contains("cache_control") && !b.to_string().contains("eager_input_streaming"));
        }

        #[test]
        fn headers_bearer_only_with_a_key() {
            assert!(build_headers("K").contains(&("authorization", "Bearer K".into())));
            assert!(build_headers("").iter().all(|(k, _)| *k != "authorization"));
        }

        #[test]
        fn attachments_become_image_url_data_uris_before_the_text() {
            assert_eq!(user_content("hi", &[]), json!("hi"));
            let img = crate::chat::events::ImageAttachment { mime: "image/png".into(), data: vec![1, 2, 3] };
            let v = user_content("hi", &[img]);
            assert_eq!(v[0], json!({"type": "image_url", "image_url": {"url": "data:image/png;base64,AQID"}}));
            assert_eq!(v[1], json!({"type": "text", "text": "hi"}));
        }

        #[test]
        fn tool_loop_two_rounds() {
            let (url, seen) = serve(vec![ROUND1, ROUND2]);
            let calls: Arc<Mutex<Vec<String>>> = Arc::default();
            let c = cfg("loop", url, Some(fake_tools(calls.clone())));
            let ev = run(&c, |_, _| {});
            let contexts: Vec<u64> = ev.iter().filter_map(|e| if let Event::Context(n) = e { Some(*n) } else { None }).collect();
            assert_eq!(contexts, [100, 160], "{ev:?}");
            let kinds: Vec<&str> = ev
                .iter()
                .filter(|e| !matches!(e, Event::Context(_)))
                .map(|e| match e {
                    Event::Text(_) => "text",
                    Event::ToolStart { .. } => "start",
                    Event::ToolEnd { .. } => "end",
                    Event::TurnEnd { .. } => "turn",
                    _ => "other",
                })
                .collect();
            assert_eq!(kinds, ["text", "start", "start", "end", "end", "text", "text", "turn"], "{ev:?}");
            assert_eq!(*calls.lock().unwrap(), ["create_artboard", "get_basic_info"]);
            let Event::TurnEnd { usage: Some(u), stop_reason, .. } = ev.last().unwrap() else { panic!() };
            assert_eq!(stop_reason.as_deref(), Some("stop"));
            // Round 1: 100 prompt (20 cached), 30 out. Round 2: 160 prompt, 12 out.
            assert_eq!((u.input, u.output, u.cache_read, u.cache_write), (80 + 160, 42, 20, 0));
            assert_eq!(u.cost_usd, None, "no price for a non-Claude model");

            let seen = seen.lock().unwrap();
            let (h1, b1) = &seen[0];
            assert!(h1.contains(&format!("authorization: bearer {}", TEST_KEY.to_lowercase())));
            assert!(!h1.iter().any(|h| h.starts_with("x-api-key") || h.starts_with("anthropic")));
            assert_eq!(b1["messages"].as_array().unwrap().len(), 2, "system + user");
            let m = seen[1].1["messages"].as_array().unwrap();
            assert_eq!(m.len(), 6, "system, user, assistant, tool, tool, image user message");
            assert_eq!(m[2]["role"], "assistant");
            assert_eq!(m[2]["content"], "Creating it.");
            assert_eq!(m[2]["tool_calls"][0]["id"], "call_A");
            assert_eq!(m[2]["tool_calls"][0]["function"]["name"], "create_artboard");
            let args: Value = serde_json::from_str(m[2]["tool_calls"][0]["function"]["arguments"].as_str().unwrap()).unwrap();
            assert_eq!(args, json!({"name": "Home", "styles": {"width": "1440px", "height": "900px"}}));
            assert_eq!(m[3], json!({"role": "tool", "tool_call_id": "call_A", "content": "created"}));
            assert_eq!(m[4], json!({"role": "tool", "tool_call_id": "call_B", "content": "info\n[image omitted]"}));
            assert_eq!(m[5]["role"], "user");
            assert_eq!(m[5]["content"][1], json!({"type": "image_url", "image_url": {"url": "data:image/png;base64,AQID"}}));

            let h = history(&c);
            assert_eq!(h.len(), 6, "user, assistant, tool, tool, image, final assistant");
            assert_eq!(h[..5], m[1..]);
            assert_eq!(h[5], json!({"role": "assistant", "content": "Done: board ready."}));
            assert!(!serde_json::to_string(&h).unwrap().contains(TEST_KEY), "key must never reach the history file");
        }

        fn delete_round() -> &'static str {
            let call = r#"{"id":"c","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_D","type":"function","function":{"name":"delete_pages","arguments":"{\"pageIds\":[\"p1\"]}"}}]}}]}"#;
            let fin = r#"{"id":"c","choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}]}"#;
            Box::leak(format!("data: {call}\n\ndata: {fin}\n\ndata: [DONE]\n\n").into_boxed_str())
        }

        #[test]
        fn destructive_tool_waits_for_approval() {
            for (ok, name) in [(false, "deny"), (true, "allow")] {
                let (url, seen) = serve(vec![delete_round(), ROUND2]);
                let calls: Arc<Mutex<Vec<String>>> = Arc::default();
                let c = cfg(name, url, Some(fake_tools(calls.clone())));
                let ev = run(&c, |t, e| {
                    if let Event::NeedsApproval { id, .. } = e {
                        t.approve(id, ok);
                    }
                });
                assert!(ev.iter().any(|e| matches!(e, Event::NeedsApproval { name, .. } if name == "delete_pages")), "{ev:?}");
                assert!(matches!(ev.last(), Some(Event::TurnEnd { .. })), "{ev:?}");
                assert_eq!(calls.lock().unwrap().len(), usize::from(ok));
                if !ok {
                    let tool_msg = &seen.lock().unwrap()[1].1["messages"][3];
                    assert!(tool_msg["content"].as_str().unwrap().contains("denied"), "{tool_msg}");
                }
            }
        }

        #[test]
        fn invalid_input_and_bad_json_go_back_as_errors() {
            let bad = r#"{"id":"c","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"c1","function":{"name":"create_artboard","arguments":"{\"name\": 5}"}},{"index":1,"id":"c2","function":{"name":"get_basic_info","arguments":"{oops"}}]}}]}"#;
            let fin = r#"{"id":"c","choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}]}"#;
            let round: &'static str = Box::leak(format!("data: {bad}\n\ndata: {fin}\n\ndata: [DONE]\n\n").into_boxed_str());
            let (url, seen) = serve(vec![round, ROUND2]);
            let calls: Arc<Mutex<Vec<String>>> = Arc::default();
            let ev = run(&cfg("invalid", url, Some(fake_tools(calls.clone()))), |_, _| {});
            assert!(matches!(ev.last(), Some(Event::TurnEnd { .. })), "{ev:?}");
            assert!(calls.lock().unwrap().is_empty());
            let m = seen.lock().unwrap()[1].1["messages"].clone();
            assert!(m[3]["content"].as_str().unwrap().starts_with("Error: invalid input for `create_artboard`"), "{}", m[3]);
            assert!(m[4]["content"].as_str().unwrap().contains("invalid arguments"), "{}", m[4]);
        }

        #[test]
        fn http_error_shows_message_and_redacts_the_key() {
            let b = format!(r#"{{"error":{{"message":"bad key {TEST_KEY}"}}}}"#);
            let (url, _) = serve_raw(vec![format!("HTTP/1.1 401 Unauthorized\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{b}", b.len())]);
            let ev = run(&cfg("401", url, None), |_, _| {});
            let Some(Event::Error(m)) = ev.last() else { panic!("{ev:?}") };
            assert!(m.contains("401") && m.contains("bad key") && !m.contains(TEST_KEY), "{m}");
        }

        #[test]
        fn round_cap_is_forty() {
            let (url, seen) = serve(vec![delete_round_plain(); MAX_TOOL_ROUNDS + 1]);
            let calls: Arc<Mutex<Vec<String>>> = Arc::default();
            let ev = run(&cfg("cap", url, Some(fake_tools(calls))), |_, _| {});
            let Some(Event::Error(m)) = ev.last() else { panic!("{ev:?}") };
            assert!(m.contains("Stopped after"), "{m}");
            assert_eq!(seen.lock().unwrap().len(), MAX_TOOL_ROUNDS + 1);
        }

        fn delete_round_plain() -> &'static str {
            let call = r#"{"id":"c","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"c1","function":{"name":"get_basic_info","arguments":"{}"}}]}}]}"#;
            let fin = r#"{"id":"c","choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}]}"#;
            Box::leak(format!("data: {call}\n\ndata: {fin}\n\ndata: [DONE]\n\n").into_boxed_str())
        }

        #[test]
        fn history_is_on_disk_before_the_end_event() {
            let (url, _) = serve(vec![ROUND2]);
            let c = cfg("persist_first", url, None);
            let path = c.api_history.clone().unwrap();
            let mut on_disk = None;
            run(&c, |_, e| {
                if matches!(e, Event::TurnEnd { .. }) {
                    on_disk = Some(path.exists());
                }
            });
            assert_eq!(on_disk, Some(true));
        }

        #[test]
        fn stop_ends_a_hanging_stream_fast() {
            use std::io::{Read, Write};
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
            use crate::chat::events::Turn;
            let mut turn = start_with_key(&cfg("stop", url, None), "x", TEST_KEY.into(), tx).unwrap();
            std::thread::sleep(Duration::from_millis(400));
            turn.stop();
            let e = rx.recv_timeout(Duration::from_secs(3)).unwrap();
            assert!(matches!(e, Event::TurnEnd { stop_reason: Some(ref r), .. } if r == "stopped"), "{e:?}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // SYNTHETIC fixture, hand-written from the documented Chat Completions streaming shape.
    const TOOLS: &str = include_str!("../../tests/fixtures/chat/openai_tools.sse");

    /// Feed in awkward 7-byte chunks (fixtures are ASCII) to prove line buffering.
    fn run(src: &str) -> (ChatParser, Vec<Event>) {
        let mut p = ChatParser::default();
        let ev = src.as_bytes().chunks(7).flat_map(|c| p.push(std::str::from_utf8(c).unwrap())).collect();
        (p, ev)
    }

    #[test]
    fn two_parallel_tool_calls_split_across_chunks() {
        let (p, ev) = run(TOOLS);
        assert_eq!(
            ev,
            vec![
                Event::Thinking("Need an artboard.".into()),
                Event::Text("Creating ".into()),
                Event::Text("the home screen.".into()),
                Event::ToolStart { id: "call_A".into(), name: "create_artboard".into(), args: json!({"name": "Home", "width": 1440}) },
                Event::ToolStart { id: "call_B".into(), name: "get_basic_info".into(), args: json!({}) },
            ]
        );
        assert_eq!(p.text(), "Creating the home screen.");
        assert_eq!(p.finish_reason(), Some("tool_calls"));
        assert_eq!(p.tool_uses().len(), 2);
        let u = p.usage().unwrap();
        assert_eq!((u.input, u.output, u.cache_read), (120, 87, 900));
        assert_eq!(p.prompt_tokens(), 1020);
        assert!(p.done());
    }

    #[test]
    fn no_usage_missing_ids_and_object_arguments_are_tolerated() {
        let mut p = ChatParser::default();
        let ev = p.push(
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"function\":{\"name\":\"get_basic_info\",\"arguments\":{\"a\":1}}}]},\"finish_reason\":\"stop\"}]}\n\n",
        );
        let Event::ToolStart { id, name, args } = &ev[0] else { panic!("{ev:?}") };
        assert!(id.starts_with("call_") && id.len() > 5);
        assert_eq!((name.as_str(), args), ("get_basic_info", &json!({"a": 1})));
        assert!(p.usage().is_none());
        assert!(!p.done(), "no [DONE] yet");
    }

    #[test]
    fn bad_arguments_json_is_kept_as_an_error_not_a_panic() {
        let mut p = ChatParser::default();
        p.push("data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"t\",\"function\":{\"name\":\"x\",\"arguments\":\"{\\\"a\\\"\"}}]},\"finish_reason\":\"tool_calls\"}]}\n");
        assert!(p.tool_uses()[0].input.as_ref().unwrap_err().contains("invalid tool arguments"));
    }

    #[test]
    fn in_stream_error_object() {
        let mut p = ChatParser::default();
        let ev = p.push("data: {\"error\":{\"message\":\"rate limited\"}}\n");
        assert_eq!(ev, vec![Event::Error("rate limited".into())]);
    }
}
