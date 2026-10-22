//! Parser for `codex exec --json` JSONL events. Stateful only because the thread id arrives in
//! `thread.started` and belongs on the `TurnEnd`.

use super::events::{content_text, str_of, Event};
use super::Usage;
use serde_json::{json, Value};

#[derive(Debug, Default)]
pub struct Parser {
    thread: Option<String>,
}

impl Parser {
    pub fn parse_line(&mut self, line: &str) -> Vec<Event> {
        let Ok(v) = serde_json::from_str::<Value>(line) else { return vec![] };
        match v["type"].as_str() {
            Some("thread.started") => {
                self.thread = v["thread_id"].as_str().map(Into::into);
                vec![]
            }
            Some("item.started") => item(&v["item"], false),
            Some("item.completed") => item(&v["item"], true),
            Some("turn.completed") => vec![Event::TurnEnd {
                usage: usage(&v["usage"]),
                session: self.thread.clone(),
                stop_reason: None,
            }],
            Some("turn.failed") => vec![Event::Error(error_text(&v["error"], "turn failed"))],
            Some("error") => vec![Event::Error(error_text(&v, "codex error"))],
            _ => vec![],
        }
    }
}

fn error_text(v: &Value, fallback: &str) -> String {
    let m = v["message"].as_str().or_else(|| v.as_str());
    m.unwrap_or(fallback).into()
}

fn usage(u: &Value) -> Option<Usage> {
    if !u.is_object() {
        return None;
    }
    let get = |k: &str| u[k].as_u64().unwrap_or(0);
    let cached = get("cached_input_tokens");
    Some(Usage {
        // Codex counts cached tokens inside input_tokens; Usage keeps them apart.
        input: get("input_tokens").saturating_sub(cached),
        output: get("output_tokens"),
        cache_read: cached,
        cache_write: get("cache_write_input_tokens"),
        cost_usd: None,
    })
}

fn item(i: &Value, done: bool) -> Vec<Event> {
    let id = str_of(&i["id"]);
    match (i["type"].as_str(), done) {
        (Some("agent_message"), true) => text(i, false),
        (Some("reasoning"), true) => text(i, true),
        (Some("mcp_tool_call"), false) => {
            vec![Event::ToolStart { id, name: str_of(&i["tool"]), args: i["arguments"].clone() }]
        }
        (Some("mcp_tool_call"), true) => {
            let failed = i["status"] == "failed" || !i["error"].is_null();
            let ev = if failed {
                let msg = match &i["error"] {
                    Value::Null => content_text(&i["result"]["content"]),
                    e => error_text(e, "tool failed"),
                };
                Event::ToolEnd { id, result: None, error: Some(msg) }
            } else {
                Event::ToolEnd { id, result: Some(i["result"].clone()), error: None }
            };
            vec![ev]
        }
        (Some("command_execution"), false) => {
            vec![Event::ToolStart { id, name: "command".into(), args: json!({ "command": i["command"] }) }]
        }
        (Some("command_execution"), true) => {
            let ok = i["exit_code"].as_i64() == Some(0);
            let out = str_of(&i["aggregated_output"]);
            vec![if ok {
                Event::ToolEnd { id, result: Some(json!(out)), error: None }
            } else {
                Event::ToolEnd { id, result: None, error: Some(out) }
            }]
        }
        (Some("error"), true) => vec![Event::Error(error_text(i, "codex error"))],
        _ => vec![],
    }
}

fn text(i: &Value, thinking: bool) -> Vec<Event> {
    let Some(t) = i["text"].as_str().filter(|t| !t.is_empty()) else { return vec![] };
    vec![if thinking { Event::Thinking(t.into()) } else { Event::Text(t.into()) }]
}

#[cfg(not(target_arch = "wasm32"))]
pub use process::{detect, start};
#[cfg(all(test, not(target_arch = "wasm32")))]
use process::build_args;

#[cfg(not(target_arch = "wasm32"))]
mod process {
    use super::*;
    use super::super::events::{Turn, TurnConfig};
    use std::{ffi::OsString, io::{BufRead, BufReader, Read}, path::{Path, PathBuf},
        process::{Child, Command, Stdio}, sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}, mpsc::Sender},
        thread, time::{Duration, Instant}};

    /// Keys that would make the CLI bill an API account instead of using its own login; the chat
    /// must run on the user's CLI login (spec, Security), so they are never inherited.
    const CREDENTIAL_ENV: [&str; 4] = ["ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN", "OPENAI_API_KEY", "CODEX_API_KEY"];

    pub(super) fn clean_env(cmd: &mut Command) -> &mut Command {
        for k in CREDENTIAL_ENV { cmd.env_remove(k); }
        cmd
    }

    pub fn detect(binary: Option<&Path>) -> Option<(PathBuf, String)> {
        let candidates = match binary {
            Some(path) => vec![path.to_path_buf()],
            None => std::env::split_paths(&std::env::var_os("PATH")?)
                .map(|dir| dir.join(if cfg!(windows) { "codex.exe" } else { "codex" })).collect(),
        };
        candidates.into_iter().find_map(|path| {
            let output = clean_env(&mut Command::new(&path)).arg("--version").output().ok()?;
            if !output.status.success() { return None; }
            let version = String::from_utf8(output.stdout).ok()?.trim().to_owned();
            if version.is_empty() { return None; }
            Some((path, version))
        })
    }

    pub(super) fn build_args(cfg: &TurnConfig, prompt: &str) -> Vec<OsString> {
        let mut args: Vec<OsString> = vec!["exec".into(), "--json".into(),
            "--skip-git-repo-check".into(), "-C".into(), cfg.project_dir.as_os_str().into(),
            "-c".into(), format!("mcp_servers.papercraft.url={}", json!(cfg.mcp_url)).into(),
            // `exec` never asks, so a tool that needs approval is refused: pre-approve ours only.
            "-c".into(), "mcp_servers.papercraft.default_tools_approval_mode=\"approve\"".into()];
        if let Some(model) = &cfg.model { args.extend(["-m".into(), model.into()]); }
        if let Some(session) = &cfg.session {
            args.extend(["resume".into(), "--".into(), session.into(), prompt.into()]);
        } else {
            let prompt = if cfg.system.is_empty() { prompt.to_owned() }
                else { format!("{}\n\n{prompt}", cfg.system) };
            args.extend(["--".into(), prompt.into()]);
        }
        args
    }

    struct ProcessTurn {
        child: Arc<Mutex<Child>>,
        running: Arc<AtomicBool>,
        /// Set by `stop`: the exit that follows is the user's, not a failure.
        stopped: Arc<AtomicBool>,
    }

    impl Turn for ProcessTurn {
        fn stop(&mut self) {
            const STOP_TIMEOUT: Duration = Duration::from_secs(3);
            const POLL: Duration = Duration::from_millis(20);
            {
                let mut child = self.child.lock().unwrap();
                if !matches!(child.try_wait(), Ok(None)) { return; }
                self.stopped.store(true, Ordering::SeqCst);
                #[cfg(unix)]
                {
                    // The child is unreaped while this lock is held, so its PID cannot be reused.
                    unsafe { libc::kill(child.id() as libc::pid_t, libc::SIGINT); }
                }
                #[cfg(not(unix))]
                { let _ = child.kill(); }
            }
            // Off the UI thread: kill and reap if the child is still running after the grace period.
            // `running` stays set until the reader thread has sent the turn's last event.
            let shared = self.child.clone();
            thread::spawn(move || {
                let deadline = Instant::now() + STOP_TIMEOUT;
                loop {
                    {
                        let mut child = shared.lock().unwrap();
                        if !matches!(child.try_wait(), Ok(None)) { break; }
                        if Instant::now() >= deadline {
                            let _ = child.kill();
                            let _ = child.wait();
                            break;
                        }
                    }
                    thread::sleep(POLL);
                }
            });
        }

        fn is_running(&self) -> bool { self.running.load(Ordering::Acquire) }
    }

    pub fn start(cfg: &TurnConfig, prompt: &str, tx: Sender<Event>) -> Result<Box<dyn Turn>, String> {
        let mut command = Command::new(cfg.binary.as_deref().unwrap_or(Path::new("codex")));
        clean_env(&mut command).args(build_args(cfg, prompt)).current_dir(&cfg.project_dir);
        spawn_turn(command, tx)
    }

    pub(super) fn spawn_turn(mut command: Command, tx: Sender<Event>) -> Result<Box<dyn Turn>, String> {
        let mut child = command.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped())
            .spawn().map_err(|e| format!("Could not start Codex: {e}"))?;
        let stdout = child.stdout.take().unwrap();
        let mut stderr = child.stderr.take().unwrap();
        let child = Arc::new(Mutex::new(child));
        let running = Arc::new(AtomicBool::new(true));
        let stopped = Arc::new(AtomicBool::new(false));
        let turn = ProcessTurn { child: child.clone(), running: running.clone(), stopped: stopped.clone() };
        thread::spawn(move || {
            let stderr = thread::spawn(move || {
                let mut text = String::new();
                stderr.read_to_string(&mut text).map(|_| text)
            });
            let mut parser = Parser::default();
            let mut terminal = None;
            for line in BufReader::new(stdout).lines() {
                let line = match line {
                    Ok(line) => line,
                    Err(e) => { terminal = Some(Event::Error(format!("Codex stdout: {e}"))); break; }
                };
                let previous = parser.thread.clone();
                let events = parser.parse_line(&line);
                if parser.thread != previous {
                    if let Some(id) = &parser.thread { let _ = tx.send(Event::Session(id.clone())); }
                }
                for event in events {
                    if matches!(event, Event::TurnEnd { .. } | Event::Error(_)) {
                        terminal = Some(event);
                    } else { let _ = tx.send(event); }
                }
            }
            let status = loop {
                let result = child.lock().unwrap().try_wait();
                match result {
                    Ok(None) => thread::sleep(Duration::from_millis(20)),
                    result => break result,
                }
            };
            let stderr = stderr.join().ok().and_then(Result::ok).unwrap_or_default();
            let end = match status {
                _ if stopped.load(Ordering::SeqCst) && !matches!(terminal, Some(Event::TurnEnd { .. })) =>
                    Event::TurnEnd { usage: None, session: parser.thread.clone(), stop_reason: Some("stopped".into()) },
                Ok(Some(status)) if status.success() => terminal.unwrap_or_else(||
                    Event::Error("Codex exited without a turn result".into())),
                Ok(Some(status)) => Event::Error(format!("Codex exited with {status}: {}", stderr.trim())),
                Err(e) => Event::Error(format!("Could not wait for Codex: {e}")),
                Ok(None) => unreachable!(),
            };
            // Send first: the app treats "not running" as "everything is in the channel".
            let _ = tx.send(end);
            running.store(false, Ordering::Release);
        });
        Ok(Box::new(turn))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn arguments_initial_and_resume() {
        let mut cfg = super::super::events::TurnConfig {
            project_dir: "/tmp/project space".into(),
            mcp_url: "http://127.0.0.1:29980/mcp".into(),
            system: "PaperCraft instructions".into(),
            model: Some("test-model".into()),
            ..Default::default()
        };
        let args = build_args(&cfg, "-prompt");
        assert_eq!(args, vec!["exec", "--json", "--skip-git-repo-check", "-C",
            "/tmp/project space", "-c", "mcp_servers.papercraft.url=\"http://127.0.0.1:29980/mcp\"",
            "-c", "mcp_servers.papercraft.default_tools_approval_mode=\"approve\"",
            "-m", "test-model", "--", "PaperCraft instructions\n\n-prompt"]);
        cfg.session = Some("session-id".into());
        let args = build_args(&cfg, "next");
        assert_eq!(&args[args.len() - 4..], ["resume", "--", "session-id", "next"].map(std::ffi::OsString::from));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn credentials_are_not_inherited() {
        let mut c = std::process::Command::new("x");
        process::clean_env(&mut c);
        let removed: Vec<_> = c.get_envs().filter(|(_, v)| v.is_none()).map(|(k, _)| k.to_string_lossy().into_owned()).collect();
        for k in ["ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN", "OPENAI_API_KEY", "CODEX_API_KEY"] {
            assert!(removed.iter().any(|r| r == k), "{k}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn stop_interrupts_and_reaps() {
        let mut cmd = std::process::Command::new("sleep");
        cmd.arg("30");
        let (tx, rx) = std::sync::mpsc::channel();
        let mut turn = process::spawn_turn(cmd, tx).unwrap();
        let t = std::time::Instant::now();
        turn.stop();
        assert!(t.elapsed() < std::time::Duration::from_millis(200), "stop blocked for {:?}", t.elapsed());
        // A stop is the user's choice: it ends the turn, it is not an error.
        assert!(matches!(rx.recv_timeout(std::time::Duration::from_secs(5)), Ok(Event::TurnEnd { stop_reason: Some(r), .. }) if r == "stopped"));
        turn.stop(); // already reaped: no-op
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn codex_integration() {
        if std::env::var("PAPERCRAFT_TEST_CODEX").as_deref() != Ok("1") || detect(None).is_none() {
            return;
        }
        let cfg = super::super::events::TurnConfig {
            project_dir: std::env::temp_dir(),
            mcp_url: "http://127.0.0.1:9/mcp".into(),
            ..Default::default()
        };
        let (tx, rx) = std::sync::mpsc::channel();
        let mut turn = start(&cfg, "Reply with exactly: hello. Do not use tools.", tx).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(90);
        let mut session = false;
        let mut text = false;
        loop {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            match rx.recv_timeout(remaining) {
                Ok(Event::Session(_)) => session = true,
                Ok(Event::Text(_)) => text = true,
                Ok(Event::TurnEnd { .. }) => break,
                Ok(Event::Error(e)) => panic!("{e}"),
                Ok(_) => {},
                Err(e) => { turn.stop(); panic!("Codex timed out: {e}"); }
            }
        }
        assert!(session && text);
        assert!(!turn.is_running());
    }

    #[cfg(unix)]
    #[test]
    fn process_events_exit_and_stop() {
        use std::time::Duration;
        fn launch(body: &str, tx: std::sync::mpsc::Sender<Event>) -> Box<dyn super::super::events::Turn> {
            let mut command = std::process::Command::new("/bin/sh");
            command.args(["-c", body]);
            process::spawn_turn(command, tx).unwrap()
        }
        for (body, fails) in [
            (format!("printf '%s\\n' '{}'", HI.trim().replace('\n', "' '")), false),
            ("printf '%s\\n' '{\"type\":\"thread.started\",\"thread_id\":\"early\"}'; echo diagnostic >&2; exit 7".into(), true),
        ] {
            let (tx, rx) = std::sync::mpsc::channel();
            let turn = launch(&body, tx);
            assert!(matches!(rx.recv_timeout(Duration::from_secs(5)).unwrap(), Event::Session(_)));
            let events: Vec<_> = rx.iter().collect();
            if fails {
                assert!(matches!(events.last(), Some(Event::Error(e)) if e.contains("diagnostic")));
            } else {
                assert!(events.iter().any(|e| matches!(e, Event::Text(_))));
                assert!(matches!(events.last(), Some(Event::TurnEnd { .. })));
            }
            assert!(!turn.is_running());
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let mut turn = launch("trap '' INT; printf '%s\\n' '{\"type\":\"thread.started\",\"thread_id\":\"stop\"}'; while :; do :; done", tx);
        assert!(matches!(rx.recv_timeout(Duration::from_secs(5)).unwrap(), Event::Session(_)));
        assert!(turn.is_running());
        let before = std::time::Instant::now();
        turn.stop();
        assert!(before.elapsed() < Duration::from_secs(4));
        // The user's stop ends the turn, it is not an error; "not running" comes after that event.
        assert!(matches!(rx.recv_timeout(Duration::from_secs(5)).unwrap(), Event::TurnEnd { stop_reason: Some(r), .. } if r == "stopped"));
        let t = std::time::Instant::now();
        while turn.is_running() && t.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(!turn.is_running());
    }

    // Real recordings from codex exec --json (--ephemeral, scratch dir, own PaperCraft instance
    // with no project open). codex_synthetic.jsonl is SYNTHETIC, written from the documented event
    // shapes to cover reasoning, command_execution, error and turn.failed.
    const HI: &str = include_str!("../../tests/fixtures/chat/codex_hi.jsonl");
    const TOOL: &str = include_str!("../../tests/fixtures/chat/codex_tool.jsonl");
    const SYNTH: &str = include_str!("../../tests/fixtures/chat/codex_synthetic.jsonl");

    fn all(f: &str) -> Vec<Event> {
        let mut p = Parser::default();
        f.lines().flat_map(|l| p.parse_line(l)).collect()
    }

    #[test]
    fn hi() {
        let ev = all(HI);
        assert_eq!(ev[0], Event::Text("Hi there, friend!".into()));
        let Event::TurnEnd { usage: Some(u), session: Some(s), .. } = &ev[1] else { panic!("{ev:?}") };
        assert_eq!((u.input, u.output, u.cache_read, u.cache_write), (26064 - 12288, 9, 12288, 0));
        assert_eq!(s, "01a16076-15a4-71c0-8cfa-df664e87bdd2");
        assert_eq!(ev.len(), 2);
    }

    #[test]
    fn tool_run() {
        let ev = all(TOOL);
        assert_eq!(ev.len(), 5, "{ev:?}");
        assert!(matches!(&ev[0], Event::Text(_)));
        let Event::ToolStart { id, name, args } = &ev[1] else { panic!() };
        assert_eq!((id.as_str(), name.as_str(), args), ("item_1", "get_basic_info", &json!({})));
        let Event::ToolEnd { id: e, result: None, error: Some(m) } = &ev[2] else { panic!("{:?}", ev[2]) };
        assert_eq!(e, id);
        assert!(m.starts_with("No project is open"));
        assert_eq!(ev[3], Event::Text("No file open in PaperCraft.".into()));
        assert!(matches!(&ev[4], Event::TurnEnd { session: Some(_), .. }));
    }

    #[test]
    fn synthetic() {
        let ev = all(SYNTH);
        assert_eq!(ev[0], Event::Thinking("Plan the frame first.".into()));
        assert!(matches!(&ev[1], Event::ToolStart { name, .. } if name == "command"));
        assert!(matches!(&ev[2], Event::ToolEnd { result: Some(r), .. } if r == "a.txt\n"));
        assert!(matches!(&ev[3], Event::ToolStart { name, args, .. } if name == "create_artboard" && args["name"] == "Home"));
        assert!(matches!(&ev[4], Event::ToolEnd { error: Some(e), .. } if e == "denied"));
        assert_eq!(ev[5], Event::Error("stream disconnected, retrying".into()));
        assert_eq!(ev[6], Event::Error("rate limited".into()));
    }
}
