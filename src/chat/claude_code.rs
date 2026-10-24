//! Parser for `claude -p --output-format stream-json --verbose [--include-partial-messages]` lines.
//! Pure: one JSON line in, zero or more [`Event`]s out.
//!
//! Without partial messages the text arrives in whole `assistant` messages ([`parse_line`]). With
//! `--include-partial-messages` the CLI emits `stream_event` deltas AND the whole `assistant`
//! messages, so [`parse_line_partial`] takes text/thinking from the deltas and only tool calls
//! from the whole messages (their input is complete there).

use super::events::{content_text, str_of, Event};
use super::Usage;
use serde_json::Value;

/// MCP tools reach the model as `mcp__<server>__<tool>`; the UI wants the bare tool name.
const MCP_PREFIX: &str = "mcp__papercraft__";

pub fn parse_line(line: &str) -> Vec<Event> {
    parse(line, false)
}

pub fn parse_line_partial(line: &str) -> Vec<Event> {
    parse(line, true)
}

fn parse(line: &str, partial: bool) -> Vec<Event> {
    let Ok(v) = serde_json::from_str::<Value>(line) else { return vec![] };
    // A sub-agent's own words stay in its card: only its tool calls and results reach the chat.
    let sub = !v["parent_tool_use_id"].is_null();
    match v["type"].as_str() {
        Some("assistant" | "stream_event") if sub => tool_uses(&v),
        Some("system") if v["subtype"] == "init" => v["session_id"].as_str().map(|s| Event::Session(s.into())).into_iter().collect(),
        Some("assistant") => assistant(&v, partial),
        Some("user") => tool_results(&v),
        Some("stream_event") if partial => delta(&v["event"]),
        Some("result") => result(&v),
        _ => vec![],
    }
}

/// Tokens in the context at one model call: input plus cache, from an `assistant` line's
/// `message.usage`. Sub-agent lines (`parent_tool_use_id` set) count against their own context.
pub fn context_of(line: &str) -> Option<u64> {
    let v = serde_json::from_str::<Value>(line).ok()?;
    if v["type"] != "assistant" || !v["parent_tool_use_id"].is_null() {
        return None;
    }
    let u = &v["message"]["usage"];
    u.is_object().then(|| ["input_tokens", "cache_read_input_tokens", "cache_creation_input_tokens"].iter().map(|k| u[*k].as_u64().unwrap_or(0)).sum())
}

fn blocks(v: &Value) -> &[Value] {
    v["message"]["content"].as_array().map_or(&[], Vec::as_slice)
}

fn assistant(v: &Value, partial: bool) -> Vec<Event> {
    let mut out = vec![];
    for b in blocks(v) {
        match b["type"].as_str() {
            Some("text") if !partial => push_text(&mut out, b["text"].as_str(), false),
            Some("thinking") if !partial => push_text(&mut out, b["thinking"].as_str(), true),
            Some("tool_use") => {
                let name = str_of(&b["name"]);
                out.push(Event::ToolStart {
                    id: str_of(&b["id"]),
                    name: name.strip_prefix(MCP_PREFIX).map_or(name.clone(), str::to_string),
                    args: b["input"].clone(),
                });
            }
            _ => {}
        }
    }
    out
}

fn tool_uses(v: &Value) -> Vec<Event> {
    let mut out = assistant(v, true);
    out.retain(|e| matches!(e, Event::ToolStart { .. }));
    out
}

fn push_text(out: &mut Vec<Event>, s: Option<&str>, thinking: bool) {
    let Some(s) = s.filter(|s| !s.is_empty()) else { return };
    out.push(if thinking { Event::Thinking(s.into()) } else { Event::Text(s.into()) });
}

fn delta(e: &Value) -> Vec<Event> {
    let mut out = vec![];
    if e["type"] != "content_block_delta" {
        return out;
    }
    let d = &e["delta"];
    match d["type"].as_str() {
        Some("text_delta") => push_text(&mut out, d["text"].as_str(), false),
        Some("thinking_delta") => push_text(&mut out, d["thinking"].as_str(), true),
        _ => {}
    }
    out
}

fn tool_results(v: &Value) -> Vec<Event> {
    blocks(v)
        .iter()
        .filter(|b| b["type"] == "tool_result")
        .map(|b| {
            let id = str_of(&b["tool_use_id"]);
            if b["is_error"].as_bool() == Some(true) {
                Event::ToolEnd { id, result: None, error: Some(content_text(&b["content"])) }
            } else {
                Event::ToolEnd { id, result: Some(b["content"].clone()), error: None }
            }
        })
        .collect()
}

fn result(v: &Value) -> Vec<Event> {
    let mut out = vec![];
    if v["is_error"].as_bool() == Some(true) {
        let msg = v["result"].as_str().filter(|s| !s.is_empty()).unwrap_or("Claude Code reported an error");
        out.push(Event::Error(msg.into()));
    }
    let u = &v["usage"];
    let usage = u.is_object().then(|| Usage {
        input: u["input_tokens"].as_u64().unwrap_or(0),
        output: u["output_tokens"].as_u64().unwrap_or(0),
        cache_read: u["cache_read_input_tokens"].as_u64().unwrap_or(0),
        cache_write: u["cache_creation_input_tokens"].as_u64().unwrap_or(0),
        cost_usd: v["total_cost_usd"].as_f64(),
    });
    out.push(Event::TurnEnd {
        usage,
        session: v["session_id"].as_str().map(Into::into),
        stop_reason: v["stop_reason"].as_str().map(Into::into),
    });
    out
}

#[cfg(not(target_arch = "wasm32"))]
pub use process::{args, detect, start, user_model};

#[cfg(not(target_arch = "wasm32"))]
mod process {
    use super::*;
    use crate::chat::events::{Turn, TurnConfig};
    use std::collections::HashSet;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::path::{Path, PathBuf};
    use std::process::{Child, Command, Stdio};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{mpsc::Sender, Arc, Mutex};
    use std::time::{Duration, Instant};

    const STOP_GRACE: Duration = Duration::from_secs(3);
    const STOP_POLL: Duration = Duration::from_millis(50);
    /// Internal tool lookup the CLI does before calling a deferred MCP tool; not worth a card.
    const HIDDEN_TOOL: &str = "ToolSearch";

    /// The `claude` binary (override, else first on PATH) and its version, e.g. `2.1.306`.
    pub fn detect(binary: Option<&Path>) -> Option<(PathBuf, String)> {
        let path = match binary {
            Some(p) => p.to_path_buf(),
            None => crate::chat::path::which("claude")?,
        };
        let out = clean_env(&mut Command::new(&path)).arg("--version").output().ok().filter(|o| o.status.success())?;
        let version = String::from_utf8_lossy(&out.stdout).split_whitespace().next()?.to_string();
        Some((path, version))
    }

    /// The model picked in the user's own Claude Code settings. A lean session skips that file
    /// (`--setting-sources project,local`), so without this it silently runs the CLI's default.
    pub fn user_model() -> Option<String> {
        let dir = std::env::var_os("CLAUDE_CONFIG_DIR").map(PathBuf::from).or_else(|| dirs::home_dir().map(|h| h.join(".claude")))?;
        model_in(&dir.join("settings.json"))
    }

    fn model_in(settings: &Path) -> Option<String> {
        let v: Value = serde_json::from_str(&std::fs::read_to_string(settings).ok()?).ok()?;
        v["model"].as_str().filter(|m| !m.is_empty()).map(str::to_owned)
    }

    /// Research runs in a Haiku sub-agent: its searches and fetches stay out of the main
    /// context, which gets only the kept references (a research step cost $0.03 instead of the
    /// $1.71 it cost in a long main session).
    const RESEARCHER: &str = r#"{"researcher": {"description": "Finds interface references on the web and brings the useful ones in. Use it for all reference research.", "prompt": "You research UI references for a designer. Search the web, then call mcp__papercraft__fetch_reference_images once with the best image or page URLs (nicelydone.club/apps/<app> for SaaS web apps, banani.co/references/apps and screenlane.com for mobile, App Store and Google Play listings; Dribbble, Behance and Figma Community block fetching). Look at what came back and keep only images that show an interface like the one asked for: no logos, icons, social cards or illustrations. If fewer than two qualify, search once more. Reply with only the kept images, one line each: the path, then what to take from it (layout, density, color, type).", "tools": ["WebSearch", "WebFetch", "mcp__papercraft__fetch_reference_images"], "model": "haiku"}}"#;

    /// Measured in specs/chat-tokens: 42.7k prompt tokens down to 20.6k. Bash, Edit, Write and
    /// the web tools need a permission `-p` never grants, so they only cost tokens.
    const LEAN_ARGS: [&str; 5] = ["--disable-slash-commands", "--setting-sources", "project,local", "--tools", "Read,TodoWrite,ToolSearch"];

    /// `system_file` holds the system prompt (read by the CLI, so it is not in `ps` output).
    pub fn args(cfg: &TurnConfig, system_file: Option<&Path>) -> Vec<String> {
        let mcp = serde_json::json!({"mcpServers": {"papercraft": {"type": "http", "url": cfg.mcp_url}}});
        let mut a: Vec<String> = [
            "-p", "--input-format", "stream-json", "--output-format", "stream-json", "--verbose",
            "--include-partial-messages", "--mcp-config", &mcp.to_string(), "--strict-mcp-config",
            "--allowedTools", "mcp__papercraft__*",
        ]
        .map(String::from)
        .into();
        if cfg.research {
            let i = a.iter().position(|x| x == "--allowedTools").unwrap() + 1;
            a[i].push_str(",WebSearch,WebFetch,Agent");
            a.extend(["--agents".into(), RESEARCHER.to_string()]);
        }
        if cfg.lean {
            // Without ToolSearch the CLI loads every PaperCraft tool schema up front.
            a.extend(LEAN_ARGS.map(String::from));
            if cfg.research {
                a.last_mut().unwrap().push_str(",WebSearch,WebFetch,Agent");
            }
        }
        if let Some(f) = system_file {
            a.extend(["--append-system-prompt-file".into(), f.display().to_string()]);
        }
        for (flag, v) in [("--model", &cfg.model), ("--effort", &cfg.effort), ("--resume", &cfg.session)] {
            if let Some(v) = v {
                a.extend([flag.into(), v.clone()]);
            }
        }
        a
    }

    /// Keys that would make the CLI bill an API account instead of using its own login; the chat
    /// must run on the user's CLI login (spec, Security), so they are never inherited.
    const CREDENTIAL_ENV: [&str; 4] = ["ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN", "OPENAI_API_KEY", "CODEX_API_KEY"];

    fn clean_env(cmd: &mut Command) -> &mut Command {
        for k in CREDENTIAL_ENV {
            cmd.env_remove(k);
        }
        crate::chat::path::apply(cmd)
    }

    /// Shared by the waiter thread and `stop`; only signalled while unreaped (`try_wait` is `None`),
    /// and always reaped under the lock, so a recycled PID is never signalled.
    struct Proc {
        child: Arc<Mutex<Child>>,
        running: Arc<AtomicBool>,
        /// Set by `stop`: the exit that follows is the user's, not a failure.
        stopped: Arc<AtomicBool>,
    }

    fn interrupt(child: &mut Child) {
        #[cfg(unix)]
        {
            // SAFETY: plain syscall on the PID of a child that is still unreaped (caller holds the lock).
            unsafe { libc::kill(child.id() as libc::pid_t, libc::SIGINT) };
        }
        #[cfg(not(unix))]
        let _ = child.kill();
    }

    fn reap(mut child: Child) {
        let _ = child.kill();
        let _ = child.wait();
    }

    /// Off the UI thread: waits out the grace period, then kills and reaps a child that ignored SIGINT.
    fn escalate(child: &Mutex<Child>) {
        let t = Instant::now();
        loop {
            let mut c = child.lock().unwrap();
            if !matches!(c.try_wait(), Ok(None)) {
                return;
            }
            if t.elapsed() >= STOP_GRACE {
                let _ = c.kill();
                let _ = c.wait();
                return;
            }
            drop(c);
            std::thread::sleep(STOP_POLL);
        }
    }

    impl Turn for Proc {
        fn stop(&mut self) {
            {
                let mut c = self.child.lock().unwrap();
                if !matches!(c.try_wait(), Ok(None)) {
                    return;
                }
                self.stopped.store(true, Ordering::SeqCst);
                interrupt(&mut c);
            }
            let child = self.child.clone();
            std::thread::spawn(move || escalate(&child));
        }
        fn is_running(&self) -> bool {
            self.running.load(Ordering::SeqCst)
        }
    }

    /// The system prompt as a file under the project's tmp folder; the caller deletes it when the
    /// turn is over.
    fn write_system_file(cfg: &TurnConfig) -> Result<Option<PathBuf>, String> {
        if cfg.system.is_empty() {
            return Ok(None);
        }
        let dir = cfg.project_dir.join(crate::chat::TMP_DIR);
        let file = dir.join(format!("system-{:016x}.txt", fastrand::u64(..)));
        std::fs::create_dir_all(&dir).and_then(|()| std::fs::write(&file, &cfg.system)).map_err(|e| format!("could not write the system prompt: {e}"))?;
        Ok(Some(file))
    }

    pub fn start(cfg: &TurnConfig, prompt: &str, tx: Sender<Event>) -> Result<Box<dyn Turn>, String> {
        let (bin, _) = detect(cfg.binary.as_deref()).ok_or("Claude Code (`claude`) was not found")?;
        let system_file = write_system_file(cfg)?;
        let removed = system_file.clone();
        let cleanup = move || {
            if let Some(f) = &removed {
                let _ = std::fs::remove_file(f);
            }
        };
        let mut child = clean_env(&mut Command::new(bin))
            .args(args(cfg, system_file.as_deref()))
            .current_dir(&cfg.project_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| {
                cleanup();
                format!("could not start claude: {e}")
            })?;
        let (Some(mut stdin), Some(stdout), Some(mut stderr)) = (child.stdin.take(), child.stdout.take(), child.stderr.take()) else {
            reap(child);
            cleanup();
            return Err("could not open the claude pipes".into());
        };
        let msg = serde_json::json!({"type": "user", "message": {"role": "user", "content": [{"type": "text", "text": prompt}]}});
        // Dropping stdin after the one message makes the CLI finish the turn and exit.
        if let Err(e) = writeln!(stdin, "{msg}") {
            drop(stdin);
            reap(child);
            cleanup();
            return Err(format!("could not send the message: {e}"));
        }
        drop(stdin);
        let err_thread = std::thread::spawn(move || {
            let mut s = String::new();
            let _ = stderr.read_to_string(&mut s);
            s
        });
        let running = Arc::new(AtomicBool::new(true));
        let child = Arc::new(Mutex::new(child));
        let stopped = Arc::new(AtomicBool::new(false));
        let turn = Proc { child: child.clone(), running: running.clone(), stopped: stopped.clone() };
        std::thread::spawn(move || {
            let (mut ended, mut hidden) = (false, HashSet::new());
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if let Some(n) = context_of(&line) {
                    let _ = tx.send(Event::Context(n));
                }
                for ev in parse_line_partial(&line) {
                    match &ev {
                        Event::ToolStart { id, name, .. } if name == HIDDEN_TOOL => {
                            hidden.insert(id.clone());
                            continue;
                        }
                        Event::ToolEnd { id, .. } if hidden.contains(id) => continue,
                        Event::TurnEnd { .. } | Event::Error(_) => ended = true,
                        _ => {}
                    }
                    let _ = tx.send(ev);
                }
            }
            // Poll under the lock (never block in `wait` holding it) so `stop` can still signal.
            let status = loop {
                let r = child.lock().unwrap().try_wait();
                match r {
                    Ok(None) => std::thread::sleep(STOP_POLL),
                    r => break r,
                }
            };
            let stderr = err_thread.join().unwrap_or_default();
            cleanup();
            if !ended {
                let ok = status.as_ref().is_ok_and(|s| s.as_ref().is_some_and(|s| s.success()));
                let ev = if ok || stopped.load(Ordering::SeqCst) {
                    Event::TurnEnd { usage: None, session: None, stop_reason: Some("stopped".into()).filter(|_| !ok) }
                } else {
                    let msg = stderr.trim();
                    Event::Error(if msg.is_empty() { format!("claude exited: {status:?}") } else { msg.into() })
                };
                let _ = tx.send(ev);
            }
            running.store(false, Ordering::SeqCst);
        });
        Ok(Box::new(turn))
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::sync::mpsc::channel;

        #[test]
    fn a_sub_agent_shows_its_tool_calls_not_its_words() {
        let text = r#"{"type":"stream_event","parent_tool_use_id":"t1","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"thinking aloud"}}}"#;
        assert!(parse_line_partial(text).is_empty());
        let call = r#"{"type":"assistant","parent_tool_use_id":"t1","message":{"content":[{"type":"text","text":"x"},{"type":"tool_use","id":"u1","name":"mcp__papercraft__fetch_reference_images","input":{"urls":["https://a.com"]}}]}}"#;
        let ev = parse_line_partial(call);
        assert!(matches!(ev.as_slice(), [Event::ToolStart { name, .. }] if name == "fetch_reference_images"), "{ev:?}");
    }

    #[test]
        fn args_shape() {
            let cfg = TurnConfig {
                mcp_url: "http://127.0.0.1:1/mcp".into(),
                system: "SYS".into(),
                model: Some("m".into()),
                session: Some("sid".into()),
                ..Default::default()
            };
            let a = args(&cfg, Some(Path::new("/tmp/sys.txt")));
            let at = |f: &str| a[a.iter().position(|x| x == f).unwrap() + 1].as_str();
            assert_eq!(at("--output-format"), "stream-json");
            assert_eq!(at("--allowedTools"), "mcp__papercraft__*");
            assert_eq!(at("--append-system-prompt-file"), "/tmp/sys.txt");
            assert!(!a.iter().any(|x| x == "SYS" || x == "--append-system-prompt"), "the prompt text is not in argv");
            assert_eq!(at("--model"), "m");
            assert_eq!(at("--resume"), "sid");
            assert!(!a.contains(&"--effort".to_string()));
            let mcp: Value = serde_json::from_str(at("--mcp-config")).unwrap();
            assert_eq!(mcp["mcpServers"]["papercraft"]["url"], "http://127.0.0.1:1/mcp");
            assert!(a.contains(&"--strict-mcp-config".to_string()));
            assert!(!a.contains(&"--tools".to_string()), "not lean: the user's own setup");
            let lean = args(&TurnConfig { lean: true, effort: Some("high".into()), ..cfg }, None);
            let at = |f: &str| lean[lean.iter().position(|x| x == f).unwrap() + 1].as_str();
            assert_eq!(at("--tools"), "Read,TodoWrite,ToolSearch", "ToolSearch keeps the PaperCraft schemas deferred");
            assert_eq!(at("--setting-sources"), "project,local");
            assert_eq!(at("--effort"), "high");
            assert!(lean.contains(&"--disable-slash-commands".to_string()));
            let web = args(&TurnConfig { lean: true, research: true, ..Default::default() }, None);
            let at = |f: &str| web[web.iter().position(|x| x == f).unwrap() + 1].as_str();
            assert_eq!(at("--tools"), "Read,TodoWrite,ToolSearch,WebSearch,WebFetch,Agent");
            assert_eq!(at("--allowedTools"), "mcp__papercraft__*,WebSearch,WebFetch,Agent", "allowed, so -p does not deny them");
            let agents: Value = serde_json::from_str(at("--agents")).unwrap();
            assert_eq!(agents["researcher"]["model"], "haiku");
        }

        #[test]
        fn the_users_own_model_is_read_from_their_settings() {
            let p = std::env::temp_dir().join(format!("papercraft-cc-settings-{}.json", std::process::id()));
            std::fs::write(&p, r#"{"model":"opus","hooks":{}}"#).unwrap();
            assert_eq!(model_in(&p).as_deref(), Some("opus"));
            std::fs::write(&p, "{}").unwrap();
            assert_eq!(model_in(&p), None);
            std::fs::remove_file(&p).ok();
            assert_eq!(model_in(&p), None, "no settings file");
        }

        #[test]
        fn credentials_are_not_inherited() {
            let mut c = Command::new("x");
            clean_env(&mut c);
            let removed: Vec<_> = c.get_envs().filter(|(_, v)| v.is_none()).map(|(k, _)| k.to_string_lossy().into_owned()).collect();
            for k in CREDENTIAL_ENV {
                assert!(removed.iter().any(|r| r == k), "{k}");
            }
        }

        #[cfg(unix)]
        fn fake_cli(name: &str, body: &str) -> PathBuf {
            use std::os::unix::fs::PermissionsExt;
            let p = std::env::temp_dir().join(format!("papercraft-fake-claude-{}-{name}", std::process::id()));
            std::fs::write(&p, format!("#!/bin/sh\n[ \"$1\" = --version ] && {{ echo 1.0; exit 0; }}\n{body}\n")).unwrap();
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
            p
        }

        #[cfg(unix)]
        #[test]
        fn stop_interrupts_and_reaps() {
            let cfg = TurnConfig { binary: Some(fake_cli("stop", "exec sleep 30")), project_dir: std::env::temp_dir(), ..Default::default() };
            let (tx, rx) = channel();
            let mut turn = start(&cfg, "hi", tx).unwrap();
            let t = Instant::now();
            turn.stop();
            assert!(t.elapsed() < Duration::from_millis(200), "stop blocked for {:?}", t.elapsed());
            // A stop is the user's choice: it ends the turn, it is not an error.
            assert!(matches!(rx.recv_timeout(Duration::from_secs(5)), Ok(Event::TurnEnd { stop_reason: Some(r), .. }) if r == "stopped"));
            let t = Instant::now();
            while turn.is_running() && t.elapsed() < Duration::from_secs(5) {
                std::thread::sleep(Duration::from_millis(20));
            }
            assert!(!turn.is_running());
            turn.stop(); // after the reap: must be a no-op
        }

        /// SIGINT is ignored, so the waiter thread must kill the child after the grace period.
        #[cfg(unix)]
        #[test]
        fn stop_kills_a_child_that_ignores_sigint() {
            let ready = std::env::temp_dir().join(format!("papercraft-stop-ready-{}", std::process::id()));
            let body = format!("trap '' INT; touch '{}'; exec sleep 30", ready.display());
            let cfg = TurnConfig { binary: Some(fake_cli("ignore", &body)), project_dir: std::env::temp_dir(), ..Default::default() };
            let (tx, rx) = channel();
            let mut turn = start(&cfg, "hi", tx).unwrap();
            let t = Instant::now();
            while !ready.exists() && t.elapsed() < Duration::from_secs(5) {
                std::thread::sleep(Duration::from_millis(20));
            }
            turn.stop();
            assert!(matches!(rx.recv_timeout(Duration::from_secs(8)), Ok(Event::TurnEnd { stop_reason: Some(r), .. }) if r == "stopped"));
            assert!(!turn.is_running());
            std::fs::remove_file(ready).ok();
        }

        /// The system prompt travels in a file under the project's tmp folder, never in argv, and
        /// the file is gone when the turn is over.
        #[cfg(unix)]
        #[test]
        fn system_prompt_goes_through_a_file_that_is_removed_afterwards() {
            let dir = std::env::temp_dir().join(format!("papercraft-claude-sys-{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            let body = r#"for a; do [ "$a" = SECRET-PROMPT ] && exit 3; done; cat >/dev/null; echo '{"type":"result","session_id":"s"}'"#;
            let cfg = TurnConfig { binary: Some(fake_cli("sysfile", body)), project_dir: dir.clone(), system: "SECRET-PROMPT".into(), ..Default::default() };
            let (tx, rx) = channel();
            let turn = start(&cfg, "hi", tx).unwrap();
            let ev = rx.recv_timeout(Duration::from_secs(5)).unwrap();
            assert!(matches!(ev, Event::TurnEnd { .. }), "{ev:?}");
            let t = Instant::now();
            while turn.is_running() && t.elapsed() < Duration::from_secs(5) {
                std::thread::sleep(Duration::from_millis(20));
            }
            let left: Vec<_> = std::fs::read_dir(dir.join(crate::chat::TMP_DIR)).unwrap().flatten().collect();
            assert!(left.is_empty(), "{left:?}");
            std::fs::remove_dir_all(dir).ok();
        }

        #[cfg(unix)]
        #[test]
        fn failed_stdin_write_reaps_child() {
            // Exits at once without reading; the large prompt overflows the pipe and fails the write.
            let cfg = TurnConfig { binary: Some(fake_cli("nostdin", "exit 0")), project_dir: std::env::temp_dir(), ..Default::default() };
            let (tx, _rx) = channel();
            let big = "x".repeat(1 << 20);
            assert!(start(&cfg, &big, tx).is_err());
        }

        /// Needs `claude` on PATH and PAPERCRAFT_TEST_CLAUDE=1; launches its own app instance.
        #[test]
        fn end_to_end() {
            if std::env::var("PAPERCRAFT_TEST_CLAUDE").as_deref() != Ok("1") || detect(None).is_none() {
                return;
            }
            let home = std::env::temp_dir().join(format!("papercraft-chat-e2e-{}", std::process::id()));
            std::fs::create_dir_all(&home).unwrap();
            let exe = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/debug/papercraft");
            let mut app = Command::new(exe)
                .env("PAPERCRAFT_HOME", &home)
                .env("PAPERCRAFT_MCP_PORT", "29984")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .expect("build target/debug/papercraft first");
            std::thread::sleep(Duration::from_secs(4));
            let cfg = TurnConfig {
                project_dir: home.clone(),
                mcp_url: "http://127.0.0.1:29984/mcp".into(),
                ..Default::default()
            };
            let (tx, rx) = channel();
            let r = start(&cfg, "Create a project named T via create_file, open it, then call get_basic_info", tx);
            let mut ev = vec![];
            if r.is_ok() {
                while let Ok(e) = rx.recv_timeout(Duration::from_secs(240)) {
                    let done = matches!(e, Event::TurnEnd { .. } | Event::Error(_));
                    ev.push(e);
                    if done {
                        break;
                    }
                }
            }
            let _ = app.kill();
            let _ = app.wait();
            let _ = std::fs::remove_dir_all(&home);
            r.map(|_| ()).unwrap();
            assert!(matches!(ev.first(), Some(Event::Session(_))), "{ev:?}");
            assert!(ev.iter().any(|e| matches!(e, Event::ToolStart { name, .. } if name == "get_basic_info")), "{ev:?}");
            assert!(ev.iter().any(|e| matches!(e, Event::ToolEnd { .. })), "{ev:?}");
            assert!(matches!(ev.last(), Some(Event::TurnEnd { .. })), "{ev:?}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // Real recordings (claude 2.1, opus-5-5) with SessionStart hook lines, plugin/tool lists,
    // paths and signatures stripped. The tool run used an own PaperCraft instance with no project
    // open, so get_basic_info returns an error result.
    const HI: &str = include_str!("../../tests/fixtures/chat/claude_code_hi.jsonl");
    const TOOL: &str = include_str!("../../tests/fixtures/chat/claude_code_tool.jsonl");

    fn all(f: &str, p: fn(&str) -> Vec<Event>) -> Vec<Event> {
        f.lines().flat_map(p).collect()
    }

    #[test]
    fn hi() {
        let ev = all(HI, parse_line);
        assert_eq!(ev.len(), 3);
        assert!(matches!(&ev[0], Event::Session(s) if s.len() == 36));
        assert_eq!(ev[1], Event::Text("Hi there, friend.".into()));
        let Event::TurnEnd { usage: Some(u), session: Some(s), stop_reason } = &ev[2] else { panic!("{ev:?}") };
        assert_eq!((u.input, u.output, u.cache_read, u.cache_write), (2, 10, 8857, 27039));
        assert!(u.cost_usd.unwrap() > 0.0);
        assert_eq!(s.len(), 36);
        assert_eq!(stop_reason.as_deref(), Some("end_turn"));
    }

    #[test]
    fn tool_run_whole_messages() {
        let ev: Vec<Event> = all(TOOL, parse_line).into_iter().filter(|e| !matches!(e, Event::Session(_))).collect();
        assert_eq!(ev.len(), 6, "{ev:?}");
        let Event::ToolStart { name, args, .. } = &ev[0] else { panic!() };
        assert_eq!((name.as_str(), args), ("ToolSearch", &json!({"query":"select:mcp__papercraft__get_basic_info","max_results":1})));
        assert!(matches!(&ev[1], Event::ToolEnd { result: Some(_), error: None, .. }));
        let Event::ToolStart { id, name, args } = &ev[2] else { panic!() };
        assert_eq!((name.as_str(), args), ("get_basic_info", &json!({})));
        let Event::ToolEnd { id: end_id, result: None, error: Some(e) } = &ev[3] else { panic!("{:?}", ev[3]) };
        assert_eq!(end_id, id);
        assert!(e.starts_with("No project is open"));
        assert!(matches!(&ev[4], Event::Text(t) if t.contains("get_basic_info")));
        assert!(matches!(&ev[5], Event::TurnEnd { stop_reason: Some(r), .. } if r == "end_turn"));
    }

    #[test]
    fn partial_mode_streams_deltas_without_duplicates() {
        let whole = all(TOOL, parse_line);
        assert!(whole.iter().any(|e| matches!(e, Event::Session(_))));
        let part = all(TOOL, parse_line_partial);
        let text: String = part.iter().filter_map(|e| if let Event::Text(t) = e { Some(t.as_str()) } else { None }).collect();
        let full: String = whole.iter().filter_map(|e| if let Event::Text(t) = e { Some(t.as_str()) } else { None }).collect();
        assert_eq!(text, full);
        assert!(part.iter().filter(|e| matches!(e, Event::Text(_))).count() > 1);
        assert_eq!(part.iter().filter(|e| matches!(e, Event::ToolStart { .. })).count(), 2);
    }

    #[test]
    fn context_is_the_last_calls_input_plus_cache_not_the_turn_total() {
        let call = |input, read, write, parent: &str| {
            format!(r#"{{"type":"assistant","parent_tool_use_id":{parent},"message":{{"content":[],"usage":{{"input_tokens":{input},"cache_read_input_tokens":{read},"cache_creation_input_tokens":{write},"output_tokens":9}}}}}}"#)
        };
        assert_eq!(context_of(&call(5, 1000, 200, "null")), Some(1205));
        assert_eq!(context_of(&call(5, 1000, 200, r#""toolu_sub""#)), None, "a sub-agent has its own context");
        assert_eq!(context_of(r#"{"type":"assistant","message":{"content":[]}}"#), None);
        assert_eq!(context_of(r#"{"type":"result","usage":{"input_tokens":365000}}"#), None, "the result line sums the whole turn");
        let from_fixture: Vec<u64> = TOOL.lines().filter_map(context_of).collect();
        assert!(!from_fixture.is_empty());
    }

    #[test]
    fn junk_and_errors() {
        assert!(parse_line("not json").is_empty());
        assert!(parse_line(r#"{"type":"rate_limit_event"}"#).is_empty());
        let ev = parse_line(r#"{"type":"result","is_error":true,"result":"boom","session_id":"s"}"#);
        assert_eq!(ev[0], Event::Error("boom".into()));
        assert!(matches!(&ev[1], Event::TurnEnd { usage: None, session: Some(s), .. } if s == "s"));
    }
}
