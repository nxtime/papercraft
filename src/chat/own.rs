//! Your own model key in the browser: the API-endpoint chat, run from the page itself. The key
//! lives in this browser's localStorage and goes only to the provider's URL, never to the relay,
//! so the server owner pays nothing. The loop is platform neutral (the browser supplies a
//! [`Transport`]); the native tests drive it with a fake one.

use super::anthropic::{self, SseParser};
use super::endpoint::{ApiFormat, Endpoint};
use super::events::{Event, TurnConfig};
use super::openai::{self, ChatParser};
use super::tools::{self, CONTINUE_NOTE, CUT_OFF, MAX_TOOL_ROUNDS, STOPPED_BY_USER};
use super::Usage;
use crate::mcp::{Out, relay_proto::Reply};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{HashMap, VecDeque};

pub const STORAGE_KEY: &str = "papercraft.chat.endpoints";
/// Ids of tool calls made here start high so a reply can never be mistaken for a relay call's.
const OWN_ID_BASE: u64 = 1 << 62;
const NEEDS_APPROVAL: &str = "this action needs the user's approval, which a browser-key chat cannot ask for; do not retry it";

type ToolResult = Result<Vec<Out>, String>;

// ------------------------------------------------------------------ stored endpoints

/// One endpoint with the key the person typed for it.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct OwnEndpoint {
    pub endpoint: Endpoint,
    pub key: String,
}

impl std::fmt::Debug for OwnEndpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OwnEndpoint").field("endpoint", &self.endpoint).field("key", &"[redacted]").finish()
    }
}

impl OwnEndpoint {
    pub fn is_ready(&self) -> bool {
        self.endpoint.is_complete() && (!self.endpoint.needs_key() || !self.key.trim().is_empty())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct OwnStore {
    pub endpoints: Vec<OwnEndpoint>,
}

impl OwnStore {
    /// Garbage in storage means no endpoints, never a crash.
    pub fn from_json(s: &str) -> Self {
        serde_json::from_str(s).unwrap_or_default()
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    pub fn get(&self, id: &str) -> Option<&OwnEndpoint> {
        self.endpoints.iter().find(|e| e.endpoint.id == id)
    }

    pub fn upsert(&mut self, e: OwnEndpoint) {
        match self.endpoints.iter_mut().find(|x| x.endpoint.id == e.endpoint.id) {
            Some(slot) => *slot = e,
            None => self.endpoints.push(e),
        }
    }

    pub fn remove(&mut self, id: &str) {
        self.endpoints.retain(|e| e.endpoint.id != id);
    }
}

// ------------------------------------------------------------------ one request

/// A request ready for `fetch`.
#[derive(Debug, Clone, PartialEq)]
pub struct Prepared {
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Value,
}

fn tools_for(ep: &Endpoint) -> Vec<Value> {
    match ep.format {
        ApiFormat::Anthropic => anthropic::tools_json(ep.is_claude_host()),
        ApiFormat::OpenAi => openai::tools_json(),
    }
}

pub fn prepare(cfg: &TurnConfig, key: &str, messages: &[Value]) -> Prepared {
    let ep = &cfg.endpoint;
    let tools = tools_for(ep);
    let model = cfg.model.as_deref().unwrap_or(&ep.model);
    let (mut headers, body): (Vec<(String, String)>, Value) = match ep.format {
        ApiFormat::Anthropic => {
            let mut h: Vec<_> = anthropic::build_headers(model, key, ep.is_claude_host()).into_iter().map(|(k, v)| (k.to_string(), v)).collect();
            // Without this the API refuses a request that comes from a web page.
            h.push(("anthropic-dangerous-direct-browser-access".into(), "true".into()));
            (h, anthropic::build_body(cfg, &tools, messages))
        }
        ApiFormat::OpenAi => (openai::build_headers(key).into_iter().map(|(k, v)| (k.to_string(), v)).collect(), openai::build_body(cfg, &tools, messages)),
    };
    headers.sort();
    Prepared { url: ep.url(), headers, body }
}

/// The text of the bytes read so far; an incomplete character at the end stays in `buf` for the
/// next chunk, and bytes that are not UTF-8 at all become U+FFFD.
pub fn take_utf8(buf: &mut Vec<u8>) -> String {
    match std::str::from_utf8(buf) {
        Ok(s) => {
            let s = s.to_string();
            buf.clear();
            s
        }
        Err(e) if e.error_len().is_none() => {
            let tail = buf.split_off(e.valid_up_to());
            let s = String::from_utf8_lossy(buf).into_owned();
            *buf = tail;
            s
        }
        Err(_) => {
            let s = String::from_utf8_lossy(buf).into_owned();
            buf.clear();
            s
        }
    }
}

// ------------------------------------------------------------------ the loop

/// Why a request ended early.
#[derive(Debug, PartialEq)]
pub enum Halt {
    Stopped,
    /// An error event was already emitted.
    Reported,
    Fail(String),
}

/// What the browser (or a test) provides.
#[allow(async_fn_in_trait)]
pub trait Transport {
    fn stopped(&self) -> bool;
    /// POSTs `p`, calling `feed` with each decoded chunk of the response until it returns true.
    async fn post(&self, p: &Prepared, feed: &mut dyn FnMut(&str) -> bool) -> Result<(), Halt>;
    /// Runs one PaperCraft tool; `None` = stopped while waiting.
    async fn call(&self, name: &str, args: &Value) -> Option<ToolResult>;
    fn persist(&self, messages: &[Value]);
}

enum Content {
    Anthropic(Vec<Value>),
    OpenAi(openai::Round),
}

enum Kind {
    Tools,
    /// Output limit hit.
    Cut,
    Pause,
    Done(String),
}

struct Use {
    id: String,
    name: String,
    input: Result<Value, String>,
}

struct Step {
    kind: Kind,
    uses: Vec<Use>,
    usage: Option<Usage>,
    context: u64,
    content: Content,
}

impl Step {
    /// Appends the assistant turn and the answers (one per tool use, in order) to the history.
    fn commit(&self, messages: &mut Vec<Value>, results: &[ToolResult], note: Option<&str>) {
        match &self.content {
            Content::Anthropic(content) => {
                if !content.is_empty() {
                    messages.push(json!({"role": "assistant", "content": content}));
                }
                let mut follow: Vec<Value> = self.uses.iter().zip(results).map(|(u, r)| anthropic::tool_result(&u.id, r)).collect();
                if let Some(n) = note {
                    follow.push(json!({"type": "text", "text": n}));
                }
                if !follow.is_empty() {
                    messages.push(json!({"role": "user", "content": follow}));
                }
            }
            Content::OpenAi(round) => {
                openai::push_round(messages, round, results);
                if let Some(n) = note {
                    messages.push(json!({"role": "user", "content": n}));
                }
            }
        }
    }
}

fn cut_results(step: &Step) -> Vec<ToolResult> {
    step.uses.iter().map(|_| Err(CUT_OFF.to_string())).collect()
}

async fn stream<T: Transport>(t: &T, p: &Prepared, format: ApiFormat, emit: &dyn Fn(Event)) -> Result<Step, Halt> {
    let mut errored = false;
    match format {
        ApiFormat::Anthropic => {
            let (mut parser, mut done) = (SseParser::default(), false);
            t.post(p, &mut |text| {
                for ev in parser.push(text) {
                    match ev {
                        Event::TurnEnd { .. } => done = true,
                        Event::Error(m) => {
                            errored = true;
                            emit(Event::Error(m));
                        }
                        other => emit(other),
                    }
                }
                done
            })
            .await?;
            if !done {
                return Err(unfinished(errored));
            }
            let content = parser.final_content();
            let uses = content
                .iter()
                .filter(|b| b["type"] == "tool_use")
                .map(|b| Use { id: str_of(&b["id"]), name: str_of(&b["name"]), input: Ok(b["input"].clone()) })
                .collect();
            let u = parser.usage().clone();
            let kind = match parser.stop_reason() {
                Some("refusal") => {
                    let msg = match parser.stop_details().and_then(|d| d["category"].as_str()) {
                        Some(c) => format!("The model declined this request (category: {c})."),
                        None => "The model declined this request.".to_string(),
                    };
                    emit(Event::Error(msg));
                    return Err(Halt::Reported);
                }
                Some("tool_use") => Kind::Tools,
                Some("max_tokens") => Kind::Cut,
                Some("pause_turn") => Kind::Pause,
                other => Kind::Done(other.unwrap_or("end_turn").into()),
            };
            let context = u.input + u.cache_read + u.cache_write;
            Ok(Step { kind, uses, usage: (u != Usage::default()).then_some(u), context, content: Content::Anthropic(content) })
        }
        ApiFormat::OpenAi => {
            let mut parser = ChatParser::default();
            t.post(p, &mut |text| {
                for ev in parser.push(text) {
                    errored |= matches!(ev, Event::Error(_));
                    emit(ev);
                }
                parser.done()
            })
            .await?;
            // Some servers close the stream after the last chunk without `[DONE]`.
            let _ = parser.push("\n");
            let Some(finish) = parser.finish_reason().map(str::to_string) else { return Err(unfinished(errored)) };
            let round = openai::Round {
                text: parser.text().to_string(),
                uses: parser.tool_uses().to_vec(),
                finish: Some(finish.clone()),
                usage: parser.usage().cloned(),
                prompt: parser.prompt_tokens(),
            };
            let uses = round.uses.iter().map(|u| Use { id: u.id.clone(), name: u.name.clone(), input: u.input.clone() }).collect();
            let kind = match finish.as_str() {
                "content_filter" => {
                    emit(Event::Error("The server's content filter blocked the reply.".into()));
                    return Err(Halt::Reported);
                }
                "length" => Kind::Cut,
                _ if !round.uses.is_empty() => Kind::Tools,
                other => Kind::Done(other.into()),
            };
            Ok(Step { kind, uses, usage: round.usage.clone(), context: round.prompt, content: Content::OpenAi(round) })
        }
    }
}

fn unfinished(errored: bool) -> Halt {
    if errored { Halt::Reported } else { Halt::Fail("The stream ended before the reply was complete.".into()) }
}

fn str_of(v: &Value) -> String {
    v.as_str().unwrap_or_default().to_string()
}

/// Runs the tool uses in order, one result each. Once stopped, the rest get an error result.
async fn run_tools<T: Transport>(t: &T, specs: &[Value], uses: &[Use], emit: &dyn Fn(Event)) -> (Vec<ToolResult>, bool) {
    let (mut results, mut stopped) = (vec![], false);
    for u in uses {
        let r = match (&u.input, stopped) {
            (_, true) => Err(STOPPED_BY_USER.to_string()),
            (Err(e), false) => Err(format!("invalid arguments for `{}`: {e}", u.name)),
            (Ok(args), false) => match check(specs, &u.name, args) {
                Err(e) => Err(e),
                Ok(()) => match t.call(&u.name, args).await {
                    Some(r) => r,
                    None => {
                        stopped = true;
                        Err(STOPPED_BY_USER.to_string())
                    }
                },
            },
        };
        emit(tools::end_event(&u.id, &r));
        results.push(r);
    }
    (results, stopped)
}

fn check(specs: &[Value], name: &str, args: &Value) -> Result<(), String> {
    let Some(tool) = specs.iter().find(|s| s["name"] == name) else { return Err(format!("unknown tool `{name}`")) };
    tools::validate_input(&tool["inputSchema"], args).map_err(|e| format!("invalid input for `{name}`: {e}"))?;
    if tools::needs_approval(name, args) {
        return Err(NEEDS_APPROVAL.into());
    }
    Ok(())
}

/// One whole turn. `messages` already ends with the new user message; the history is persisted
/// before the terminal event is last emitted, as the app reopens the chat on seeing it.
pub async fn run<T: Transport>(t: &T, cfg: &TurnConfig, key: &str, mut messages: Vec<Value>, emit: &dyn Fn(Event)) {
    let model = cfg.model.clone().unwrap_or_else(|| cfg.endpoint.model.clone());
    let finish = |total: &mut Usage, reported: bool, reason: &str| {
        let usage = reported.then(|| {
            total.cost_usd = super::endpoint::cost(&model, total);
            total.clone()
        });
        Event::TurnEnd { usage, session: None, stop_reason: Some(reason.into()) }
    };
    let end = turn(t, cfg, key, &mut messages, emit, &finish).await;
    t.persist(&messages);
    if let Some(e) = end {
        emit(e);
    }
}

/// The terminal event of the turn (an error, or the end with usage); `None` when an error
/// event was already emitted.
async fn turn<T: Transport>(t: &T, cfg: &TurnConfig, key: &str, messages: &mut Vec<Value>, emit: &dyn Fn(Event), finish: &dyn Fn(&mut Usage, bool, &str) -> Event) -> Option<Event> {
    let specs = tools::tool_specs();
    let (mut total, mut reported) = (Usage::default(), false);
    let (mut rounds, mut continued) = (0, false);
    loop {
        if t.stopped() {
            return Some(finish(&mut total, reported, "stopped"));
        }
        let prepared = prepare(cfg, key, messages);
        let step = match stream(t, &prepared, cfg.endpoint.format, emit).await {
            Ok(s) => s,
            Err(Halt::Stopped) => return Some(finish(&mut total, reported, "stopped")),
            Err(Halt::Reported) => return None,
            Err(Halt::Fail(m)) => return Some(Event::Error(tools::redact(&m, key))),
        };
        if let Some(u) = &step.usage {
            reported = true;
            emit(Event::Context(step.context));
            total.input += u.input;
            total.output += u.output;
            total.cache_read += u.cache_read;
            total.cache_write += u.cache_write;
        }
        match &step.kind {
            Kind::Tools => {
                if rounds >= MAX_TOOL_ROUNDS {
                    return Some(Event::Error(format!("Stopped after {MAX_TOOL_ROUNDS} tool rounds; send a message to continue.")));
                }
                // A stop mid-round still records what ran, so the next turn matches the document.
                let (results, stopped) = run_tools(t, &specs, &step.uses, emit).await;
                step.commit(messages, &results, None);
                rounds += 1;
                t.persist(messages);
                if stopped {
                    return Some(finish(&mut total, reported, "stopped"));
                }
            }
            Kind::Cut if !continued => {
                continued = true;
                step.commit(messages, &cut_results(&step), Some(CONTINUE_NOTE));
                t.persist(messages);
            }
            Kind::Cut => {
                step.commit(messages, &cut_results(&step), None);
                return Some(finish(&mut total, reported, "length"));
            }
            Kind::Pause => {
                if rounds >= MAX_TOOL_ROUNDS {
                    return Some(Event::Error(format!("Stopped after {MAX_TOOL_ROUNDS} rounds; send a message to continue.")));
                }
                rounds += 1;
                step.commit(messages, &[], None);
                t.persist(messages);
            }
            Kind::Done(reason) => {
                step.commit(messages, &[], None);
                return Some(finish(&mut total, reported, reason));
            }
        }
    }
}

// ------------------------------------------------------------------ tool calls from the page

/// Tool calls the chat wants run, and the senders waiting for their answer. The web shell drains
/// `take` next to the relay's calls and hands every reply it sees to `answer`.
#[derive(Default)]
pub struct Calls {
    next: u64,
    queue: VecDeque<crate::mcp::relay_proto::Call>,
    waiting: HashMap<u64, tokio::sync::oneshot::Sender<ToolResult>>,
}

impl Calls {
    pub fn submit(&mut self, name: &str, args: &Value) -> tokio::sync::oneshot::Receiver<ToolResult> {
        let id = OWN_ID_BASE + self.next;
        self.next += 1;
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.waiting.insert(id, tx);
        self.queue.push_back(crate::mcp::relay_proto::Call { id, name: name.into(), args: args.clone() });
        rx
    }

    pub fn take(&mut self) -> Option<crate::mcp::relay_proto::Call> {
        self.queue.pop_front()
    }

    pub fn is_own(id: u64) -> bool {
        id >= OWN_ID_BASE
    }

    /// Gives the reply to whoever waits for it; a stale id is ignored.
    pub fn answer(&mut self, reply: Reply) {
        if let Some(tx) = self.waiting.remove(&reply.id) {
            let _ = tx.send(reply.into_result());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::endpoint::Endpoint;
    use std::cell::{Cell, RefCell};

    const KEY: &str = "sk-fake-test-key-123456";

    fn anthropic_cfg() -> TurnConfig {
        TurnConfig { endpoint: Endpoint::default(), system: "sys".into(), ..Default::default() }
    }

    fn openai_cfg() -> TurnConfig {
        let endpoint = Endpoint { id: "o".into(), name: "Local".into(), format: ApiFormat::OpenAi, base_url: "https://llm.example/v1".into(), model: "m".into() };
        TurnConfig { endpoint, ..Default::default() }
    }

    fn fixture(name: &str) -> String {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/chat").join(name);
        std::fs::read_to_string(p).unwrap().lines().filter(|l| !l.starts_with(": ")).collect::<Vec<_>>().join("\n") + "\n"
    }

    /// Serves canned responses in order and answers tool calls with "ok".
    struct Fake {
        responses: RefCell<VecDeque<Result<String, Halt>>>,
        sent: RefCell<Vec<Prepared>>,
        called: RefCell<Vec<String>>,
        stop: Cell<bool>,
        saved: RefCell<Vec<Value>>,
    }

    impl Fake {
        fn new(r: Vec<Result<String, Halt>>) -> Self {
            Self { responses: RefCell::new(r.into()), sent: Default::default(), called: Default::default(), stop: Cell::new(false), saved: Default::default() }
        }
    }

    impl Transport for Fake {
        fn stopped(&self) -> bool {
            self.stop.get()
        }
        async fn post(&self, p: &Prepared, feed: &mut dyn FnMut(&str) -> bool) -> Result<(), Halt> {
            self.sent.borrow_mut().push(p.clone());
            let body = self.responses.borrow_mut().pop_front().expect("unexpected request")?;
            feed(&body);
            Ok(())
        }
        async fn call(&self, name: &str, _: &Value) -> Option<ToolResult> {
            self.called.borrow_mut().push(name.into());
            Some(Ok(vec![Out::Text("ok".into())]))
        }
        fn persist(&self, m: &[Value]) {
            *self.saved.borrow_mut() = m.to_vec();
        }
    }

    fn drive(t: &Fake, cfg: &TurnConfig) -> Vec<Event> {
        let events = RefCell::new(vec![]);
        let msgs = vec![json!({"role": "user", "content": "hi"})];
        pollster::block_on(run(t, cfg, KEY, msgs, &|e| events.borrow_mut().push(e)));
        events.into_inner()
    }

    #[test]
    fn store_round_trips_and_survives_garbage() {
        let mut s = OwnStore::default();
        s.upsert(OwnEndpoint { endpoint: Endpoint::default(), key: KEY.into() });
        assert_eq!(OwnStore::from_json(&s.to_json()), s);
        s.upsert(OwnEndpoint { endpoint: Endpoint::default(), key: "other".into() });
        assert_eq!(s.endpoints.len(), 1);
        assert_eq!(s.get("anthropic").map(|e| e.key.as_str()), Some("other"));
        s.remove("anthropic");
        assert!(s.endpoints.is_empty());
        assert_eq!(OwnStore::from_json("not json"), OwnStore::default());
    }

    #[test]
    fn debug_hides_the_key_and_a_keyless_claude_endpoint_is_not_ready() {
        let e = OwnEndpoint { endpoint: Endpoint::default(), key: KEY.into() };
        assert!(!format!("{e:?}").contains(KEY));
        assert!(e.is_ready());
        assert!(!OwnEndpoint { key: " ".into(), ..e }.is_ready());
    }

    #[test]
    fn requests_go_to_the_provider_with_the_right_headers() {
        let a = prepare(&anthropic_cfg(), KEY, &[]);
        assert_eq!(a.url, "https://api.anthropic.com/v1/messages");
        let get = |p: &Prepared, k: &str| p.headers.iter().find(|(h, _)| h == k).map(|(_, v)| v.clone());
        assert_eq!(get(&a, "x-api-key").as_deref(), Some(KEY));
        assert_eq!(get(&a, "anthropic-dangerous-direct-browser-access").as_deref(), Some("true"));
        let o = prepare(&openai_cfg(), KEY, &[]);
        assert_eq!(o.url, "https://llm.example/v1/chat/completions");
        assert_eq!(get(&o, "authorization").or(get(&o, "Authorization")).as_deref(), Some(format!("Bearer {KEY}").as_str()));
        assert!(get(&o, "anthropic-dangerous-direct-browser-access").is_none());
    }

    #[test]
    fn anthropic_tool_round_runs_the_tool_then_ends() {
        let t = Fake::new(vec![Ok(fixture("anthropic_round1_tools.sse")), Ok(fixture("anthropic_round2_end.sse"))]);
        let events = drive(&t, &anthropic_cfg());
        assert!(!t.called.borrow().is_empty());
        assert!(matches!(events.last(), Some(Event::TurnEnd { .. })), "{events:?}");
        let msgs = t.saved.borrow();
        assert_eq!(msgs[1]["role"], "assistant");
        assert_eq!(msgs[2]["content"][0]["type"], "tool_result");
        assert_eq!(t.sent.borrow().len(), 2);
    }

    #[test]
    fn openai_tool_round_runs_the_tool_then_ends() {
        let t = Fake::new(vec![Ok(fixture("openai_round1_tools.sse")), Ok(fixture("openai_round2_end.sse"))]);
        let events = drive(&t, &openai_cfg());
        assert!(!t.called.borrow().is_empty());
        assert!(matches!(events.last(), Some(Event::TurnEnd { .. })), "{events:?}");
        assert!(t.saved.borrow().iter().any(|m| m["role"] == "tool"));
    }

    #[test]
    fn stop_before_the_request_ends_the_turn_without_calling_out() {
        let t = Fake::new(vec![]);
        t.stop.set(true);
        let events = drive(&t, &anthropic_cfg());
        assert!(t.sent.borrow().is_empty());
        assert!(matches!(events.last(), Some(Event::TurnEnd { stop_reason: Some(r), .. }) if r == "stopped"));
    }

    #[test]
    fn a_failure_is_reported_without_the_key() {
        let t = Fake::new(vec![Err(Halt::Fail(format!("Anthropic API error 401: bad key {KEY}")))]);
        let events = drive(&t, &anthropic_cfg());
        let Some(Event::Error(m)) = events.last() else { panic!("{events:?}") };
        assert!(m.contains("401") && !m.contains(KEY), "{m}");
    }

    #[test]
    fn a_tool_that_needs_approval_is_refused() {
        let specs = tools::tool_specs();
        let args = json!({"nodeIds": (0..40).map(|i| format!("n{i}")).collect::<Vec<_>>()});
        assert!(tools::needs_approval("delete_nodes", &args));
        assert_eq!(check(&specs, "delete_nodes", &args).unwrap_err(), NEEDS_APPROVAL);
        assert!(check(&specs, "nope", &json!({})).unwrap_err().contains("unknown tool"));
    }

    #[test]
    fn a_character_split_across_chunks_is_kept_whole() {
        let bytes = "a\u{e9}".as_bytes();
        let mut buf = bytes[..2].to_vec();
        assert_eq!(take_utf8(&mut buf), "a");
        buf.extend(&bytes[2..]);
        assert_eq!(take_utf8(&mut buf), "\u{e9}");
        let mut bad = vec![0xff, b'x'];
        assert_eq!(take_utf8(&mut bad), "\u{fffd}x");
    }

    #[test]
    fn calls_route_a_reply_to_its_waiting_turn() {
        let mut c = Calls::default();
        let mut rx = c.submit("get_basic_info", &json!({}));
        let call = c.take().unwrap();
        assert!(Calls::is_own(call.id) && !Calls::is_own(7));
        c.answer(Reply::err(call.id, "boom"));
        assert_eq!(rx.try_recv().unwrap().unwrap_err(), "boom");
        c.answer(Reply::err(call.id, "late"));
    }
}
