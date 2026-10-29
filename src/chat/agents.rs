//! The agent registry: every coding agent PaperCraft knows how to start, as data, plus detection
//! on the resolved PATH and the command that launches one over ACP.
//!
//! Launch args were checked against the agents installed on the dev machine (2026-10-21):
//! * OpenCode 1.18.30: `opencode acp` ("start ACP (Agent Client Protocol) server").
//! * Cursor Agent 2026.03.20: the binary is `cursor-agent` (its help says `agent`); `cursor-agent acp`
//!   ("Start the Cursor Agent as an ACP server").
//! * Pi 0.80.3 has no ACP mode of its own; the `pi-acp` adapter is run with `npx`.
//! * Goose: `/opt/homebrew/bin/goose` here is pressly/goose, the SQL migration tool. Block's agent
//!   has an `acp` subcommand; the migration tool's `--help` never says `acp`, so that is the identity.
//!
//! Gemini CLI is left out: Google retired it for the Antigravity CLI (June 2026).
//! The rest keep their documented command and fail with the agent's own error.

#[cfg(not(target_arch = "wasm32"))]
use crate::ui::settings::ChatPrefs;
#[cfg(not(target_arch = "wasm32"))]
use crate::ui::settings::CustomAgent;
#[cfg(not(target_arch = "wasm32"))]
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentKind {
    /// A program found on PATH that speaks ACP.
    Native,
    /// Run with `npx -y <package>`; off until the user enables it, because that runs fetched code.
    Npx { package: &'static str },
    /// Claude Code and Codex: proven direct backends, not started through ACP.
    Direct,
}

#[derive(Debug, Clone, Copy)]
pub struct AgentDef {
    pub id: &'static str,
    pub name: &'static str,
    pub binary: &'static str,
    /// Args after the binary (or after the package for `Npx`) that start ACP mode.
    pub launch: &'static [&'static str],
    pub kind: AgentKind,
    pub docs_url: &'static str,
    /// Text (case-insensitive) that `--version` or `--help` of the real program contains.
    pub identity: Option<&'static str>,
}

const fn native(id: &'static str, name: &'static str, binary: &'static str, launch: &'static [&'static str], docs_url: &'static str, identity: Option<&'static str>) -> AgentDef {
    AgentDef { id, name, binary, launch, kind: AgentKind::Native, docs_url, identity }
}

const fn npx(id: &'static str, name: &'static str, package: &'static str, launch: &'static [&'static str], docs_url: &'static str) -> AgentDef {
    AgentDef { id, name, binary: "npx", launch, kind: AgentKind::Npx { package }, docs_url, identity: None }
}

pub const AGENTS: &[AgentDef] = &[
    AgentDef { id: "claude-code", name: "Claude Code", binary: "claude", launch: &[], kind: AgentKind::Direct, docs_url: super::CLAUDE_DOCS, identity: Some("claude code") },
    AgentDef { id: "codex", name: "Codex", binary: "codex", launch: &[], kind: AgentKind::Direct, docs_url: super::CODEX_DOCS, identity: Some("codex") },
    native("cursor", "Cursor", "cursor-agent", &["acp"], "https://cursor.com/cli", Some("cursor agent")),
    native("opencode", "OpenCode", "opencode", &["acp"], "https://opencode.ai/docs", Some("opencode")),
    native("goose", "Goose", "goose", &["acp"], "https://block.github.io/goose", Some("acp")),
    native("kimi", "Kimi", "kimi", &["acp"], "https://github.com/MoonshotAI/kimi-cli", None),
    native("devin", "Devin", "devin", &["acp"], "https://docs.devin.ai", None),
    native("junie", "Junie", "junie", &["--acp=true"], "https://www.jetbrains.com/junie", None),
    native("vibe", "Mistral Vibe", "vibe-acp", &[], "https://github.com/mistralai/mistral-vibe", None),
    native("hermes", "Hermes", "hermes", &["acp"], "https://github.com/NousResearch/hermes-agent", None),
    native("openhands", "OpenHands", "openhands", &["acp"], "https://docs.all-hands.dev", None),
    native("stakpak", "Stakpak", "stakpak", &["acp"], "https://github.com/stakpak/agent", None),
    native("vtcode", "VT Code", "vtcode", &["acp"], "https://github.com/vinhnx/vtcode", None),
    native("poolside", "Poolside", "pool", &["acp"], "https://poolside.ai", None),
    native("droid", "Droid", "droid", &["exec", "--output-format", "acp-daemon"], "https://docs.factory.ai", None),
    npx("qwen", "Qwen Code", "@qwen-code/qwen-code", &["--acp"], "https://github.com/QwenLM/qwen-code"),
    npx("kilo", "Kilo", "@kilocode/cli", &["acp"], "https://kilo.ai/docs"),
    npx("cline", "Cline", "cline", &["--acp"], "https://docs.cline.bot"),
    npx("auggie", "Auggie", "@augmentcode/auggie", &["--acp"], "https://docs.augmentcode.com"),
    npx("copilot", "Copilot CLI", "@github/copilot", &["--acp"], "https://docs.github.com/copilot"),
    npx("qoder", "Qoder", "@qoder-ai/qodercli", &["--acp"], "https://qoder.com"),
    npx("codebuddy", "CodeBuddy", "@tencent-ai/codebuddy-code", &["--acp"], "https://www.codebuddy.ai"),
    npx("pi", "Pi", "pi-acp", &[], "https://github.com/svkozak/pi-acp"),
    npx("claude-acp", "Claude Code (ACP adapter)", "@zed-industries/claude-code-acp", &[], "https://github.com/zed-industries/claude-code-acp"),
    npx("codex-acp", "Codex (ACP adapter)", "@zed-industries/codex-acp", &[], "https://github.com/zed-industries/codex-acp"),
];

pub fn find(id: &str) -> Option<&'static AgentDef> {
    AGENTS.iter().find(|a| a.id == id)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentState {
    Ready { path: String, version: String },
    /// An `npx` agent the user has not turned on.
    NeedsEnable,
    NotFound,
    /// A program with the right name that is not the agent (e.g. the `goose` migration tool).
    WrongProgram { path: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentStatus {
    pub id: String,
    pub state: AgentState,
}

/// Registry agents then custom ones.
#[cfg(not(target_arch = "wasm32"))]
pub fn detect_all(prefs: &ChatPrefs) -> Vec<AgentStatus> {
    detect_all_in(prefs, super::path::resolved_path().as_ref())
}

#[cfg(not(target_arch = "wasm32"))]
fn detect_all_in(prefs: &ChatPrefs, path: &std::ffi::OsStr) -> Vec<AgentStatus> {
    // One thread per agent: each probe can wait out its timeout.
    std::thread::scope(|s| {
        let jobs: Vec<_> = AGENTS
            .iter()
            .map(|d| (d.id.to_owned(), s.spawn(move || detect_def(d, prefs, path))))
            .chain(prefs.custom_agents.iter().map(|c| (c.id.clone(), s.spawn(move || detect_custom(c, path)))))
            .collect();
        jobs.into_iter().map(|(id, j)| AgentStatus { id, state: j.join().unwrap_or(AgentState::NotFound) }).collect()
    })
}

#[cfg(not(target_arch = "wasm32"))]
const PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(4); // macOS scans a program on its first run, which can take seconds under load
/// `--help` of a node CLI can take a moment longer than `--version`.
#[cfg(not(target_arch = "wasm32"))]
const HELP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);
#[cfg(not(target_arch = "wasm32"))]
const PROBE_POLL: std::time::Duration = std::time::Duration::from_millis(10);
/// How long a probe's output may lag behind the program's exit.
#[cfg(not(target_arch = "wasm32"))]
const PROBE_DRAIN: std::time::Duration = std::time::Duration::from_millis(200);
#[cfg(not(target_arch = "wasm32"))]
const PROBE_MAX_BYTES: u64 = 64 * 1024;

/// The override from Settings, if any. The old `claude_path`/`codex_path` fields win for their
/// agent until Settings writes `agents` instead.
#[cfg(not(target_arch = "wasm32"))]
fn path_override<'a>(id: &str, prefs: &'a ChatPrefs) -> Option<&'a str> {
    let legacy = match id {
        "claude-code" => prefs.claude_path.as_deref(),
        "codex" => prefs.codex_path.as_deref(),
        _ => None,
    };
    legacy.or_else(|| prefs.agents.get(id)?.path.as_deref()).map(str::trim).filter(|p| !p.is_empty())
}

#[cfg(not(target_arch = "wasm32"))]
fn args_override<'a>(id: &str, prefs: &'a ChatPrefs) -> Option<&'a str> {
    prefs.agents.get(id)?.args.as_deref().map(str::trim).filter(|a| !a.is_empty())
}

#[cfg(not(target_arch = "wasm32"))]
fn is_enabled(def: &AgentDef, prefs: &ChatPrefs) -> bool {
    !matches!(def.kind, AgentKind::Npx { .. }) || prefs.agents.get(def.id).is_some_and(|a| a.enabled)
}

#[cfg(not(target_arch = "wasm32"))]
fn locate(binary: &str, over: Option<&str>, path: &std::ffi::OsStr) -> Option<PathBuf> {
    match over {
        Some(p) => PathBuf::from(p).is_file().then(|| PathBuf::from(p)),
        None => super::path::which_in(binary, path),
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn detect_def(def: &AgentDef, prefs: &ChatPrefs, path: &std::ffi::OsStr) -> AgentState {
    let over = path_override(def.id, prefs);
    if !is_enabled(def, prefs) {
        return AgentState::NeedsEnable;
    }
    // An npx agent needs npx itself; a path override points at that program instead.
    let Some(bin) = locate(def.binary, over, path) else { return AgentState::NotFound };
    let shown = bin.display().to_string();
    if matches!(def.kind, AgentKind::Npx { .. }) {
        return AgentState::Ready { path: shown, version: String::new() };
    }
    let version = probe(&bin, "--version", path, PROBE_TIMEOUT);
    if let Some(want) = def.identity {
        let hit = |o: &Option<String>| o.as_ref().is_some_and(|o| o.to_lowercase().contains(want));
        if !hit(&version) && !hit(&probe(&bin, "--help", path, HELP_TIMEOUT)) {
            return AgentState::WrongProgram { path: shown };
        }
    }
    AgentState::Ready { path: shown, version: version.as_deref().and_then(|v| v.lines().find(|l| !l.trim().is_empty())).unwrap_or_default().trim().to_owned() }
}

#[cfg(not(target_arch = "wasm32"))]
fn detect_custom(c: &CustomAgent, path: &std::ffi::OsStr) -> AgentState {
    match custom_program(c, path) {
        Some(p) => AgentState::Ready { path: p.display().to_string(), version: String::new() },
        None => AgentState::NotFound,
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn custom_program(c: &CustomAgent, path: &std::ffi::OsStr) -> Option<PathBuf> {
    let cmd = c.command.trim();
    if cmd.is_empty() {
        return None;
    }
    if cmd.contains(['/', '\\']) {
        return PathBuf::from(cmd).is_file().then(|| PathBuf::from(cmd));
    }
    super::path::which_in(cmd, path)
}

/// Runs `bin arg` and returns stdout + stderr; None when it fails to start or times out. No
/// credentials in the environment, PATH as the agents get it.
#[cfg(not(target_arch = "wasm32"))]
fn probe(bin: &std::path::Path, arg: &str, path: &std::ffi::OsStr, timeout: std::time::Duration) -> Option<String> {
    use std::io::Read;
    use std::process::{Command, Stdio};
    let mut cmd = Command::new(bin);
    cmd.arg(arg).env("PATH", path).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    for k in super::path::CREDENTIAL_ENV {
        cmd.env_remove(k);
    }
    // ETXTBSY (26): a binary just written (installer, test) is briefly still open for writing.
    let mut child = (0..5).find_map(|_| match cmd.spawn() {
        Err(e) if e.raw_os_error() == Some(26) => {
            std::thread::sleep(std::time::Duration::from_millis(30));
            None
        }
        r => Some(r),
    })?.ok()?;
    let (tx, rx) = std::sync::mpsc::channel();
    for pipe in [child.stdout.take().map(|p| Box::new(p) as Box<dyn Read + Send>), child.stderr.take().map(|p| Box::new(p) as Box<dyn Read + Send>)].into_iter().flatten() {
        let tx = tx.clone();
        std::thread::spawn(move || {
            // Chunks, not one read to EOF: a pipe end can leak into an unrelated process
            // spawned at the same moment (macOS has no atomic pipe+CLOEXEC) and then never
            // reaches EOF until that process exits.
            let (mut pipe, mut buf) = (pipe.take(PROBE_MAX_BYTES), [0u8; 4096]);
            while let Ok(n @ 1..) = pipe.read(&mut buf) {
                if tx.send(String::from_utf8_lossy(&buf[..n]).into_owned()).is_err() {
                    break;
                }
            }
        });
    }
    drop(tx);
    // Done when the program exits (its output then gets a short grace to arrive) or when both
    // pipes close; a hung program hits the deadline and is killed.
    let deadline = std::time::Instant::now() + timeout;
    let mut exited_at = None;
    let mut out = String::new();
    let done = loop {
        let now = std::time::Instant::now();
        if exited_at.is_none() && child.try_wait().ok().flatten().is_some() {
            exited_at = Some(now);
        }
        if exited_at.is_some_and(|t| now >= t + PROBE_DRAIN) {
            break true;
        }
        if now >= deadline {
            break false;
        }
        match rx.recv_timeout(PROBE_POLL.min(deadline - now)) {
            Ok(s) => out.push_str(&s),
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break true,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
        }
    };
    let _ = child.kill();
    let _ = child.wait();
    done.then_some(out)
}

/// Environment agent `id` always runs with. opencode waits 60 s for an MCP reply (the SDK's
/// default), less than PaperCraft lets a call run (`CALL_TIMEOUT`): a slow `generate_image` would
/// finish with nobody left to read it.
#[cfg(not(target_arch = "wasm32"))]
pub fn base_env(id: &str) -> Vec<(String, String)> {
    const MARGIN_MS: u128 = 30_000;
    match id {
        "opencode" => {
            let ms = crate::mcp::server::CALL_TIMEOUT.as_millis() + MARGIN_MS;
            vec![("OPENCODE_CONFIG_CONTENT".into(), serde_json::json!({"experimental": {"mcp_timeout": ms}}).to_string())]
        }
        _ => vec![],
    }
}

/// Environment that starts agent `id` without the user's own setup (config, plugins, rules,
/// skills), so a lean turn carries only PaperCraft's instructions. `home` is an empty folder
/// PaperCraft owns. Empty for agents whose sign-in would not survive it, and where `real_home` is
/// unknown (Windows).
pub fn pure_env(id: &str, home: &std::path::Path, real_home: Option<&std::path::Path>) -> Vec<(String, String)> {
    let Some(real) = real_home else { return vec![] };
    let xdg = |k: &str, under: &str| (k.to_owned(), std::env::var(k).unwrap_or_else(|_| real.join(under).display().to_string()));
    match id {
        // Skills are found under HOME (~/.agents, ~/.claude) and config under XDG_CONFIG_HOME;
        // sign-ins and sessions (so `session/load` still works) stay in the real data folder.
        "opencode" => vec![
            ("HOME".into(), home.display().to_string()),
            ("XDG_CONFIG_HOME".into(), home.join("config").display().to_string()),
            xdg("XDG_DATA_HOME", ".local/share"),
            xdg("XDG_CACHE_HOME", ".cache"),
            xdg("XDG_STATE_HOME", ".local/state"),
            ("OPENCODE_PURE".into(), "1".into()),
            ("OPENCODE_DISABLE_PROJECT_CONFIG".into(), "1".into()),
        ],
        // Pi finds skills under HOME (~/.agents/skills) and its config, packages and sessions in
        // PI_CODING_AGENT_DIR; npm (npx pi-acp, Pi's package installs) keeps the user's cache.
        "pi" => vec![
            ("HOME".into(), home.display().to_string()),
            ("PI_CODING_AGENT_DIR".into(), home.join(PI_AGENT_DIR).display().to_string()),
            ("npm_config_cache".into(), std::env::var("npm_config_cache").unwrap_or_else(|_| real.join(".npm").display().to_string())),
        ],
        _ => vec![],
    }
}

/// Pi's folder inside its lean home (`PI_CODING_AGENT_DIR`).
pub const PI_AGENT_DIR: &str = "agent";

/// Writes the lean Pi folder under `home`: only `pi-mcp-adapter` as a package, connected to
/// PaperCraft at `mcp_url` with its tools listed one by one (not behind the adapter's `mcp`
/// proxy) and a call timeout above PaperCraft's own.
#[cfg(not(target_arch = "wasm32"))]
pub fn write_pi_home(home: &std::path::Path, mcp_url: &str) -> Result<(), String> {
    const MARGIN_MS: u128 = 30_000;
    let dir = home.join(PI_AGENT_DIR);
    crate::vfs::create_dir_all(&dir).map_err(|e| format!("Could not make {}: {e}", dir.display()))?;
    let settings = serde_json::json!({"packages": ["npm:pi-mcp-adapter"], "quietStartup": true});
    let mcp = serde_json::json!({
        "mcpServers": {"papercraft": {"url": mcp_url, "directTools": true}},
        "settings": {"requestTimeoutMs": crate::mcp::server::CALL_TIMEOUT.as_millis() + MARGIN_MS},
    });
    for (name, v) in [("settings.json", settings), ("mcp.json", mcp)] {
        let path = dir.join(name);
        let text = serde_json::to_string_pretty(&v).map_err(|e| e.to_string())?;
        crate::io::atomic::write(&path, text.as_bytes()).map_err(|e| format!("Could not write {}: {e}", path.display()))?;
    }
    Ok(())
}

/// What the ACP backend runs for this agent: program and args. None for an unknown id, a
/// `Direct` agent, an `npx` agent that is not enabled, or a program that is not on PATH.
#[cfg(not(target_arch = "wasm32"))]
pub fn launch_command(id: &str, prefs: &ChatPrefs) -> Option<(PathBuf, Vec<String>)> {
    launch_command_in(id, prefs, super::path::resolved_path().as_ref())
}

#[cfg(not(target_arch = "wasm32"))]
fn launch_command_in(id: &str, prefs: &ChatPrefs, path: &std::ffi::OsStr) -> Option<(PathBuf, Vec<String>)> {
    if let Some(c) = prefs.custom_agents.iter().find(|c| c.id == id) {
        return Some((custom_program(c, path)?, split(&c.args)));
    }
    let def = find(id)?;
    if !is_enabled(def, prefs) {
        return None;
    }
    let program = locate(def.binary, path_override(id, prefs), path)?;
    let args = args_override(id, prefs).map_or_else(|| def.launch.iter().map(|a| (*a).to_owned()).collect(), split);
    match def.kind {
        AgentKind::Native => Some((program, args)),
        AgentKind::Npx { package } => Some((program, ["-y".to_owned(), package.to_owned()].into_iter().chain(args).collect())),
        AgentKind::Direct => None,
    }
}

// ponytail: whitespace split, no quoting; add shlex if a path with spaces is ever needed in args.
#[cfg(not(target_arch = "wasm32"))]
fn split(args: &str) -> Vec<String> {
    args.split_whitespace().map(String::from).collect()
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::ui::settings::AgentPrefs;
    use std::ffi::OsString;
    use std::os::unix::fs::PermissionsExt;

    fn fake_dir(scripts: &[(&str, &str)]) -> (PathBuf, OsString) {
        let dir = std::env::temp_dir().join(format!("pc-agents-{}", fastrand::u64(..)));
        std::fs::create_dir_all(&dir).unwrap();
        for (name, body) in scripts {
            let p = dir.join(name);
            std::fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let path = dir.clone().into_os_string();
        (dir, path)
    }

    fn state(all: &[AgentStatus], id: &str) -> AgentState {
        all.iter().find(|s| s.id == id).unwrap().state.clone()
    }

    #[test]
    fn table_ids_are_unique_and_launchable() {
        let mut ids: Vec<_> = AGENTS.iter().map(|a| a.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), AGENTS.len());
        assert!(AGENTS.iter().all(|a| !a.name.is_empty() && !a.binary.is_empty() && a.docs_url.starts_with("https://")));
        assert!(AGENTS.iter().all(|a| !matches!(a.kind, AgentKind::Native) || !a.launch.is_empty() || a.id == "vibe"));
    }

    #[test]
    fn detects_ready_wrong_missing_and_npx_states() {
        let (dir, path) = fake_dir(&[
            ("cursor-agent", r#"if [ "$1" = --version ]; then echo 0.1.0; else echo "Cursor Agent help"; fi"#),
            // The migration tool: version string and help with no "acp".
            ("goose", r#"if [ "$1" = --version ]; then echo "goose version: v3.27.1"; else echo "Usage: goose DRIVER DBSTRING"; fi"#),
            ("opencode", r#"echo "opencode 1.0""#),
            ("npx", "echo npx"),
        ]);
        let mut prefs = ChatPrefs::default();
        let all = detect_all_in(&prefs, &path);
        assert_eq!(all.len(), AGENTS.len());
        let shown = |n: &str| dir.join(n).display().to_string();
        assert_eq!(state(&all, "cursor"), AgentState::Ready { path: shown("cursor-agent"), version: "0.1.0".into() });
        assert_eq!(state(&all, "opencode"), AgentState::Ready { path: shown("opencode"), version: "opencode 1.0".into() });
        assert_eq!(state(&all, "goose"), AgentState::WrongProgram { path: shown("goose") });
        assert_eq!(state(&all, "kimi"), AgentState::NotFound);
        assert_eq!(state(&all, "pi"), AgentState::NeedsEnable);
        prefs.agents.insert("pi".into(), AgentPrefs { enabled: true, ..Default::default() });
        assert_eq!(state(&detect_all_in(&prefs, &path), "pi"), AgentState::Ready { path: shown("npx"), version: String::new() });
        // Enabled but npx missing.
        assert_eq!(state(&detect_all_in(&prefs, OsStr::new("/nonexistent")), "pi"), AgentState::NotFound);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    use std::ffi::OsStr;

    #[test]
    fn path_override_wins_and_hung_program_times_out() {
        let (dir, path) = fake_dir(&[("opencode", "echo opencode"), ("hang", "/bin/sleep 30")]);
        let mut prefs = ChatPrefs::default();
        prefs.agents.insert("opencode".into(), AgentPrefs { path: Some("/nonexistent/opencode".into()), ..Default::default() });
        assert_eq!(state(&detect_all_in(&prefs, &path), "opencode"), AgentState::NotFound);
        let t = std::time::Instant::now();
        assert_eq!(probe(&dir.join("hang"), "--version", &path, std::time::Duration::from_millis(200)), None);
        assert!(t.elapsed() < std::time::Duration::from_secs(5));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn opencode_waits_longer_for_a_tool_than_papercraft_lets_it_run() {
        let env = base_env("opencode");
        let config: serde_json::Value = serde_json::from_str(&env[0].1).unwrap();
        let ms = config["experimental"]["mcp_timeout"].as_u64().unwrap();
        assert!(u128::from(ms) > crate::mcp::server::CALL_TIMEOUT.as_millis());
        assert!(base_env("cursor").is_empty());
    }

    #[test]
    fn pure_opencode_hides_the_users_home_but_keeps_its_sign_ins() {
        let env = pure_env("opencode", std::path::Path::new("/pc/home"), Some(std::path::Path::new("/Users/me")));
        let get = |k: &str| env.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str());
        assert_eq!((get("HOME"), get("XDG_CONFIG_HOME"), get("OPENCODE_PURE")), (Some("/pc/home"), Some("/pc/home/config"), Some("1")));
        let data = std::env::var("XDG_DATA_HOME").unwrap_or_else(|_| "/Users/me/.local/share".into());
        assert_eq!(get("XDG_DATA_HOME"), Some(data.as_str()), "auth.json and sessions live there");
        assert!(pure_env("cursor", std::path::Path::new("/pc/home"), Some(std::path::Path::new("/Users/me"))).is_empty(), "its sign-in is under HOME");
        assert!(pure_env("opencode", std::path::Path::new("/pc/home"), None).is_empty());
        let pi = pure_env("pi", std::path::Path::new("/pc/home"), Some(std::path::Path::new("/Users/me")));
        assert!(pi.contains(&("HOME".into(), "/pc/home".into())) && pi.contains(&("PI_CODING_AGENT_DIR".into(), "/pc/home/agent".into())), "{pi:?}");
    }

    #[test]
    fn the_lean_pi_folder_has_only_the_mcp_adapter_pointed_at_papercraft() {
        let home = std::env::temp_dir().join(format!("pc-pi-home-{}", std::process::id()));
        write_pi_home(&home, "http://127.0.0.1:1/mcp?chat=c1&tools=core").unwrap();
        let read = |f: &str| serde_json::from_str::<serde_json::Value>(&std::fs::read_to_string(home.join("agent").join(f)).unwrap()).unwrap();
        assert_eq!(read("settings.json")["packages"], serde_json::json!(["npm:pi-mcp-adapter"]));
        let mcp = read("mcp.json");
        assert_eq!(mcp["mcpServers"]["papercraft"]["url"], "http://127.0.0.1:1/mcp?chat=c1&tools=core");
        assert!(mcp["settings"]["requestTimeoutMs"].as_u64().unwrap() > crate::mcp::server::CALL_TIMEOUT.as_millis() as u64);
        std::fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn launch_commands_for_native_npx_custom_and_direct() {
        let (dir, path) = fake_dir(&[("cursor-agent", ""), ("npx", ""), ("my-agent", "")]);
        let mut prefs = ChatPrefs::default();
        prefs.custom_agents.push(CustomAgent { id: "mine".into(), name: "Mine".into(), command: "my-agent".into(), args: "serve  --stdio".into() });
        let at = |n: &str| dir.join(n);
        assert_eq!(launch_command_in("cursor", &prefs, &path), Some((at("cursor-agent"), vec!["acp".into()])));
        assert_eq!(launch_command_in("mine", &prefs, &path), Some((at("my-agent"), vec!["serve".into(), "--stdio".into()])));
        assert_eq!(launch_command_in("pi", &prefs, &path), None);
        prefs.agents.insert("pi".into(), AgentPrefs { enabled: true, ..Default::default() });
        assert_eq!(launch_command_in("pi", &prefs, &path), Some((at("npx"), vec!["-y".into(), "pi-acp".into()])));
        prefs.agents.insert("cursor".into(), AgentPrefs { args: Some("acp --debug".into()), ..Default::default() });
        assert_eq!(launch_command_in("cursor", &prefs, &path).unwrap().1, ["acp", "--debug"]);
        assert_eq!(launch_command_in("claude-code", &prefs, &path), None);
        assert_eq!(launch_command_in("nope", &prefs, &path), None);
        assert_eq!(launch_command_in("kimi", &prefs, &path), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
