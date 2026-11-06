//! Agents hosted by the relay, end to end: a fake tab on a real WebSocket sends chat frames and
//! the relay starts a fake `claude`. The fake is this test program itself (`harness = false`):
//! called the way the relay calls Claude Code it plays the agent, so no shell scripts are needed
//! and Windows runs the same test.

use papercraft::chat::events::Event;
use papercraft::mcp::relay::{self, Running};
use papercraft::mcp::relay_agents::AgentsConfig;
use papercraft::mcp::relay_proto::{AgentInfo, FromTab, ToTab};
use std::io::{BufRead, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tokio_tungstenite::tungstenite::{Message as Ws, WebSocket, connect, stream::MaybeTlsStream};

const WAIT: Duration = Duration::from_secs(20);
const HANG: &str = "hang";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|a| a == "--version") {
        println!("1.0.0");
        return;
    }
    if args.iter().any(|a| a == "--output-format") {
        return fake_claude(&args);
    }
    let tests: [(&str, fn()); 6] = [
        ("a_message_starts_the_agent_and_its_events_stream_back", message_streams_back),
        ("a_follow_up_continues_the_session", follow_up_resumes),
        ("cancel_stops_the_agent", cancel_stops),
        ("the_cap_on_running_agents_is_enforced", cap_is_enforced),
        ("a_silent_agent_is_stopped", idle_is_stopped),
        ("without_agents_the_relay_refuses", refuses_without_agents),
    ];
    for (name, test) in tests {
        test();
        println!("test {name} ... ok");
    }
    println!("test result: ok. {} passed; 0 failed", tests.len());
}

/// What the relay's Claude Code would do: read the one message, answer with what it was started
/// with, then (for "hang") wait to be killed.
fn fake_claude(args: &[String]) {
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line).expect("the message arrives on stdin");
    let msg: serde_json::Value = serde_json::from_str(&line).expect("the message is JSON");
    let text = msg["message"]["content"][0]["text"].as_str().unwrap_or_default();
    let cwd = std::env::current_dir().expect("a working directory");
    let entries = std::fs::read_dir(&cwd).map_or(99, |d| d.flatten().filter(|e| e.file_name() != papercraft::chat::TMP_DIR).count());
    let reply = format!("ARGS {} CWD {} ENTRIES {entries}", args.join(" "), cwd.display());
    let out = |v: serde_json::Value| {
        println!("{v}");
        std::io::stdout().flush().expect("stdout");
    };
    out(serde_json::json!({"type": "system", "subtype": "init", "session_id": "fake-session"}));
    // The relay asks for partial messages, so the words arrive as stream deltas.
    out(serde_json::json!({"type": "stream_event", "event": {"type": "content_block_delta", "delta": {"type": "text_delta", "text": reply}}}));
    if text == HANG {
        std::thread::sleep(Duration::from_secs(600));
    }
    out(serde_json::json!({"type": "result", "session_id": "fake-session"}));
}

struct Env {
    relay: Running,
    root: PathBuf,
}

impl Drop for Env {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.root).ok();
    }
}

fn start(agents: bool, max_turns: usize, idle: Duration) -> Env {
    let root = std::env::temp_dir().join(format!("papercraft-relay-agents-{:x}", fastrand::u64(..)));
    let cfg = agents.then(|| {
        let mut c = AgentsConfig::new("claude", root.clone()).expect("a valid list");
        c.claude_bin = Some(std::env::current_exe().expect("this program"));
        c.max_turns = max_turns;
        c.idle = idle;
        c
    });
    let relay = relay::spawn_full("127.0.0.1:0".parse().expect("address"), None, None, None, cfg).expect("the relay starts");
    Env { relay, root }
}

struct Tab(WebSocket<MaybeTlsStream<TcpStream>>);

impl Tab {
    /// Connects and returns the tab with the agents the relay said it hosts.
    fn connect(env: &Env) -> (Tab, Vec<AgentInfo>) {
        let token = format!("{:032x}", fastrand::u64(..));
        let (ws, _) = connect(format!("ws://127.0.0.1:{}/tab/{token}", env.relay.port())).expect("the tab connects");
        let mut tab = Tab(ws);
        loop {
            if let Some(ToTab::Hello { agents }) = tab.next() {
                return (tab, agents);
            }
        }
    }

    fn next(&mut self) -> Option<ToTab> {
        match self.0.read().expect("the socket stays open") {
            Ws::Text(t) => serde_json::from_str(&t).ok(),
            _ => None,
        }
    }

    fn say(&mut self, chat: &str, text: &str) {
        let frame = FromTab::Chat { chat: chat.into(), agent: "claude-code".into(), text: text.into(), model: None };
        self.0.send(Ws::Text(serde_json::to_string(&frame).expect("encodes").into())).expect("sends");
    }

    fn cancel(&mut self, chat: &str) {
        self.0.send(Ws::Text(serde_json::to_string(&FromTab::Cancel { chat: chat.into() }).expect("encodes").into())).expect("sends");
    }

    /// Events of `chat` up to and including the first one for which `stop` is true.
    fn until(&mut self, chat: &str, stop: impl Fn(&Event) -> bool) -> Vec<Event> {
        let (mut seen, start) = (Vec::new(), Instant::now());
        while start.elapsed() < WAIT {
            if let Some(ToTab::Chat { chat: c, event }) = self.next()
                && c == chat
            {
                let done = stop(&event);
                seen.push(event);
                if done {
                    return seen;
                }
            }
        }
        panic!("no end of the wanted events, saw {seen:?}");
    }

    /// The events of one whole turn.
    fn turn(&mut self, chat: &str) -> Vec<Event> {
        self.until(chat, |e| matches!(e, Event::TurnEnd { .. } | Event::Error(_)))
    }
}

fn text_of(events: &[Event]) -> String {
    events.iter().filter_map(|e| if let Event::Text(t) = e { Some(t.as_str()) } else { None }).collect()
}

fn error_of(events: &[Event]) -> String {
    events.iter().find_map(|e| if let Event::Error(m) = e { Some(m.clone()) } else { None }).expect("an error event")
}

fn message_streams_back() {
    let env = start(true, 4, Duration::from_secs(60));
    let (mut tab, agents) = Tab::connect(&env);
    assert_eq!(agents.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(), ["claude-code"]);
    tab.say("c1", "hello");
    let events = tab.turn("c1");
    assert!(matches!(events.last(), Some(Event::TurnEnd { session: Some(s), .. }) if s == "fake-session"), "{events:?}");
    let text = text_of(&events);
    assert!(text.contains("--allowedTools mcp__papercraft__*"), "{text}");
    assert!(text.contains("--tools  --disallowedTools Bash"), "no built-in tools: {text}");
    assert!(text.contains("/mcp"), "the agent is pointed at the link's MCP endpoint: {text}");
    assert!(text.contains("ENTRIES 0"), "an empty working folder: {text}");
    assert!(text.contains("c1"), "its own folder: {text}");
}

fn follow_up_resumes() {
    let env = start(true, 4, Duration::from_secs(60));
    let (mut tab, _) = Tab::connect(&env);
    tab.say("c1", "one");
    assert!(!text_of(&tab.turn("c1")).contains("--resume"));
    tab.say("c1", "two");
    assert!(text_of(&tab.turn("c1")).contains("--resume fake-session"));
}

fn cancel_stops() {
    let env = start(true, 4, Duration::from_secs(60));
    let (mut tab, _) = Tab::connect(&env);
    tab.say("c1", HANG);
    tab.until("c1", |e| matches!(e, Event::Text(_)));
    tab.cancel("c1");
    let events = tab.turn("c1");
    assert!(matches!(events.last(), Some(Event::TurnEnd { stop_reason: Some(r), .. }) if r == "stopped"), "{events:?}");
}

fn cap_is_enforced() {
    let env = start(true, 1, Duration::from_secs(60));
    let (mut tab, _) = Tab::connect(&env);
    tab.say("a", HANG);
    tab.until("a", |e| matches!(e, Event::Text(_)));
    tab.say("b", "hello");
    assert!(error_of(&tab.turn("b")).contains("as many agents"));
    tab.cancel("a");
    tab.turn("a");
    // The slot is free again.
    tab.say("b", "hello");
    assert!(matches!(tab.turn("b").last(), Some(Event::TurnEnd { .. })));
}

fn idle_is_stopped() {
    let env = start(true, 4, Duration::from_millis(1500));
    let (mut tab, _) = Tab::connect(&env);
    tab.say("c1", HANG);
    assert!(error_of(&tab.turn("c1")).contains("stopped answering"));
}

fn refuses_without_agents() {
    let off = start(false, 4, Duration::from_secs(60));
    let (mut tab, agents) = Tab::connect(&off);
    assert!(agents.is_empty());
    tab.say("c1", "hello");
    assert!(error_of(&tab.turn("c1")).contains("does not run agents"));

    // An agent that is not on the list is refused too, and a bad chat id never reaches the disk.
    let on = start(true, 4, Duration::from_secs(60));
    let (mut tab, _) = Tab::connect(&on);
    let frame = FromTab::Chat { chat: "c1".into(), agent: "opencode".into(), text: "hi".into(), model: None };
    tab.0.send(Ws::Text(serde_json::to_string(&frame).expect("encodes").into())).expect("sends");
    assert!(error_of(&tab.turn("c1")).contains("does not run the agent"));
    tab.say("../x", "hello");
    assert!(error_of(&tab.turn("../x")).contains("not valid"));
}
