//! `delegate`: the chat agent hands review, reading or a well-specified build to a helper agent
//! on a cheaper model (pi, opencode, an Ollama-backed agent: any CLI that takes a prompt and has
//! a shell). The helper drives PaperCraft through `papercraft-cli`, which reaches this app (it
//! inherits `PAPERCRAFT_MCP_PORT`), and only its final answer comes back into the main context.

use super::{Out, ToolResult};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const TOOL: &str = "delegate";
pub const ROLES: [&str; 3] = ["qa", "read", "build"];
/// The user's pi with MiniMax M3.1 Flash: fast, cheap, sees images.
pub const PI_COMMAND: &str = "pi -p --no-session -nc -ns --model minimax/MiniMax-M3.1-Flash {prompt}";
/// opencode with its default model; add `-m provider/model` (an `ollama/...` one runs locally).
pub const OPENCODE_COMMAND: &str = "opencode run {prompt}";
const TIMEOUT: Duration = Duration::from_secs(900);
const POLL: Duration = Duration::from_millis(250);
/// The tail of the helper's output that comes back: its answer, not its tool log.
const MAX_ANSWER: usize = 6000;

/// Helpers running for a chat, each with its stop flag; the chat's Stop button sets them.
static RUNNING: Mutex<Vec<(String, Arc<AtomicBool>)>> = Mutex::new(Vec::new());

/// Stop the helpers the chat `chat` started (its Stop button).
pub fn stop(chat: &str) {
    for (c, flag) in RUNNING.lock().unwrap().iter() {
        if c == chat {
            flag.store(true, Ordering::Relaxed);
        }
    }
}

/// Runs the helper `command` (`{prompt}` is passed as `$PAPERCRAFT_PROMPT`) for one task.
/// `chat`: the chat that asked, so its Stop button ends the helper too.
/// `agent`: the helper's name and the session of the agent that delegated it, passed to its
/// `papercraft-cli` calls so they are attributed to it and its claims.
pub fn run(command: &str, chat: Option<&str>, agent: (&str, &str), args: &serde_json::Value) -> ToolResult {
    if command.trim().is_empty() {
        return Err("No helper agent is set up. The user can choose one in Settings, Chat, Helper; do the work yourself.".into());
    }
    let role = args["role"].as_str().filter(|r| ROLES.contains(r)).ok_or("role must be qa, read or build")?;
    let task = args["task"].as_str().map(str::trim).filter(|t| !t.is_empty()).ok_or("give the task with everything the helper needs")?;
    let cli = crate::chat::path::which("papercraft-cli").ok_or("papercraft-cli is not installed, so a helper cannot reach PaperCraft")?;
    let prompt = format!("{}\n\n{}\n\nTask:\n{task}", preamble(&cli.display().to_string()), role_rules(role));
    let mut cmd = if cfg!(windows) {
        let mut c = Command::new("cmd.exe");
        c.args(["/C", &command.replace("{prompt}", "%PAPERCRAFT_PROMPT%")]);
        c
    } else {
        let mut c = Command::new("/bin/sh");
        c.args(["-c", &command.replace("{prompt}", "\"$PAPERCRAFT_PROMPT\"")]);
        c
    };
    // Its own process group, so stopping it also ends the agent the shell started.
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut cmd, 0);
    let mut child = cmd
        .env("PAPERCRAFT_PROMPT", &prompt)
        .env("PAPERCRAFT_AGENT", agent.0)
        .env("PAPERCRAFT_PARENT", agent.1)
        // An app opened from Finder has a bare PATH: give the helper the user's shell PATH.
        .env("PATH", crate::chat::path::resolved_path())
        .current_dir(std::env::temp_dir())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("could not start the helper: {e}"))?;
    let flag = Arc::new(AtomicBool::new(false));
    let key = chat.unwrap_or_default().to_owned();
    RUNNING.lock().unwrap().push((key, flag.clone()));
    let started = Instant::now();
    let ended = loop {
        match child.try_wait() {
            Err(e) => break Err(e.to_string()),
            Ok(Some(_)) => break Ok(()),
            Ok(None) => {}
        }
        let late = started.elapsed() > TIMEOUT;
        if late || flag.load(Ordering::Relaxed) {
            kill_group(&mut child);
            break Err(if late { format!("the helper took longer than {} minutes", TIMEOUT.as_secs() / 60) } else { "stopped by the user".into() });
        }
        std::thread::sleep(POLL);
    };
    RUNNING.lock().unwrap().retain(|(_, f)| !Arc::ptr_eq(f, &flag));
    ended?;
    let done = child.wait_with_output().map_err(|e| e.to_string())?;
    let out = String::from_utf8_lossy(&done.stdout);
    if !done.status.success() || out.trim().is_empty() {
        let err = String::from_utf8_lossy(&done.stderr);
        return Err(format!("the helper failed: {}", err.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("no output")));
    }
    Ok(vec![Out::Text(tail(out.trim(), MAX_ANSWER).to_owned())])
}

/// End the helper and everything it started.
fn kill_group(child: &mut std::process::Child) {
    #[cfg(unix)]
    let _ = Command::new("kill").args(["-TERM", &format!("-{}", child.id())]).status();
    let _ = child.kill();
    let _ = child.wait();
}

fn preamble(cli: &str) -> String {
    format!(
        "You are a helper agent for a design agent working in PaperCraft, a design app. You do not see its \
         chat: everything you need is below. Drive PaperCraft only with its CLI from your shell: \
         `{cli} call <tool> '<json args>'` (tool names: `{cli} list`; a tool's arguments: `{cli} schema <tool>`). \
         For an image result add `-o <file>.png` and look at the file if you can view images. Work only on \
         what the task names. End with only your answer in plain text, no preamble."
    )
}

fn role_rules(role: &str) -> &'static str {
    match role {
        "qa" => "Role: QA reviewer. Do not edit. Run lint_design without fix, then check each frame the task names \
                 (get_screenshot if you can view images, else get_tree_summary and get_computed_styles): overflow and \
                 clipped text, alignment and spacing, literal or off-palette colors, contrast, target sizes, placeholder \
                 content, look-alikes that should be instances. Reply with at most 12 findings, most severe first, one \
                 line each: node id, problem, fix. Or 'No findings'.",
        "read" => "Role: reader. Do not edit. Read what the task names (pages, frames, specs, files, URLs) and return only \
                   the key points it asks for, at most 25 lines, with node ids where they matter.",
        _ => "Role: builder. Make exactly the edits the task specifies with the variables, tokens and components it names \
              (write_html with var(--name) and <x-paper-instance>). Do not redesign or add anything else. Then run \
              lint_design on what you changed. Reply in at most 10 lines: what you made (node ids) and lint warnings left.",
    }
}

/// The last `max` bytes of `s`, cut at a character boundary.
fn tail(s: &str, max: usize) -> &str {
    let mut i = s.len().saturating_sub(max);
    while !s.is_char_boundary(i) {
        i += 1;
    }
    &s[i..]
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_task_reaches_the_helper_as_data_and_only_its_answer_returns() {
        if crate::chat::path::which("papercraft-cli").is_none() {
            return;
        }
        // The task tries to run a command; it must arrive as plain text.
        let task = "check Home\"; touch pwned; echo \"";
        let out = run("printf '%s' {prompt} | tail -c 40", None, ("Helper", ""), &json!({"role": "qa", "task": task})).unwrap();
        let Out::Text(t) = &out[0] else { panic!() };
        assert!(t.ends_with(task), "{t}");
        assert!(!std::env::temp_dir().join("pwned").exists());
        assert!(run("", None, ("Helper", ""), &json!({"role": "qa", "task": "x"})).unwrap_err().contains("Settings"));
        assert!(run("echo", None, ("Helper", ""), &json!({"role": "design", "task": "x"})).is_err());
        assert!(run("exit 3", None, ("Helper", ""), &json!({"role": "read", "task": "x"})).is_err());
        let who = run("printf '%s|%s' \"$PAPERCRAFT_AGENT\" \"$PAPERCRAFT_PARENT\"", None, ("Chat / build 1", "s1"), &json!({"role": "qa", "task": "x"})).unwrap();
        let Out::Text(t) = &who[0] else { panic!() };
        assert_eq!(t, "Chat / build 1|s1", "the helper's CLI calls carry its identity");
        assert_eq!(tail("héllo", 4), "llo");
        // Stop ends a running helper, and the process it started, at once.
        let t = std::thread::spawn(|| run("sleep 30", Some("chat-stop-test"), ("Helper", ""), &json!({"role": "qa", "task": "x"})));
        std::thread::sleep(Duration::from_millis(500));
        let begun = Instant::now();
        stop("chat-stop-test");
        assert_eq!(t.join().unwrap().unwrap_err(), "stopped by the user");
        assert!(begun.elapsed() < Duration::from_secs(5));
    }
}
