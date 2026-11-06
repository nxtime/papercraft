//! Agents the relay runs for a tab (`--agents`): the tab chats, the relay starts Claude Code or
//! opencode with the link's own MCP endpoint and streams the turn's events back.
//!
//! Safety is layered: the CLIs are started in hosted mode (only the PaperCraft MCP tools, see
//! `TurnConfig::hosted`), each chat gets its own empty working directory, the number of running
//! agents is capped and a silent agent is stopped. Credentials come from the relay's environment
//! (`CLAUDE_CODE_OAUTH_TOKEN`, `ANTHROPIC_API_KEY`, opencode's own provider setup); the tab never
//! sends any.

use super::relay_proto::{AgentInfo, FromTab, ToTab};
use crate::chat::events::{Event, Turn, TurnConfig};
use crate::chat::{acp, agents, claude_code, valid_id};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

pub const CLAUDE: &str = "claude-code";
pub const OPENCODE: &str = "opencode";
/// Agents running at once, over every link.
pub const DEFAULT_MAX_TURNS: usize = 2;
/// A turn that says nothing for this long is stopped.
pub const DEFAULT_IDLE: Duration = Duration::from_secs(10 * 60);
/// Chats one link may keep a working directory for.
const MAX_CHATS: usize = 32;
/// Longest message a tab may send an agent.
const MAX_MESSAGE_BYTES: usize = 64 << 10;

/// What `--agents` turns on, and the limits.
#[derive(Clone, Debug)]
pub struct AgentsConfig {
    /// Agent ids (`claude-code`, `opencode`) the tab may pick.
    pub enabled: Vec<String>,
    /// Holds one empty working directory per chat; wiped when the relay starts.
    pub root: PathBuf,
    pub max_turns: usize,
    pub idle: Duration,
    /// Binary overrides (tests run a fake agent); None = found on PATH.
    pub claude_bin: Option<PathBuf>,
    pub opencode_bin: Option<PathBuf>,
}

impl AgentsConfig {
    /// `list` is the `--agents` value, e.g. `claude,opencode`.
    pub fn new(list: &str, root: PathBuf) -> Result<AgentsConfig, String> {
        let mut enabled = Vec::new();
        for name in list.split(',').map(str::trim).filter(|n| !n.is_empty()) {
            let id = match name {
                "claude" | CLAUDE => CLAUDE,
                OPENCODE => OPENCODE,
                other => return Err(format!("unknown agent '{other}' (use claude or opencode)")),
            };
            if !enabled.iter().any(|e| e == id) {
                enabled.push(id.to_string());
            }
        }
        if enabled.is_empty() {
            return Err("--agents needs claude, opencode or both".into());
        }
        Ok(AgentsConfig { enabled, root, max_turns: DEFAULT_MAX_TURNS, idle: DEFAULT_IDLE, claude_bin: None, opencode_bin: None })
    }
}

/// Sends one frame to the tab; false once the tab is gone.
pub type Push = Arc<dyn Fn(ToTab) -> bool + Send + Sync>;

/// The relay-wide part: config, the running-agent count and the link numbering.
pub struct Agents {
    cfg: AgentsConfig,
    active: Arc<AtomicUsize>,
    links: AtomicU64,
}

impl Agents {
    /// Starts from an empty `cfg.root`: what an earlier run left in it is not trusted.
    pub fn new(cfg: AgentsConfig) -> Result<Arc<Agents>, String> {
        if cfg.root.exists() {
            std::fs::remove_dir_all(&cfg.root).map_err(|e| format!("cannot clear {}: {e}", cfg.root.display()))?;
        }
        std::fs::create_dir_all(&cfg.root).map_err(|e| format!("cannot create {}: {e}", cfg.root.display()))?;
        Ok(Arc::new(Agents { cfg, active: Arc::new(AtomicUsize::new(0)), links: AtomicU64::new(1) }))
    }

    /// What the tab's picker lists.
    pub fn infos(&self) -> Vec<AgentInfo> {
        self.cfg
            .enabled
            .iter()
            .filter_map(|id| agents::find(id).map(|d| AgentInfo { id: id.clone(), name: d.name.to_string() }))
            .collect()
    }

    /// The chats of one link; `mcp_url` is that link's endpoint, token included.
    pub fn session(self: &Arc<Agents>, mcp_url: String) -> Arc<Session> {
        let dir = self.cfg.root.join(format!("link-{}", self.links.fetch_add(1, Ordering::Relaxed)));
        Arc::new(Session { agents: self.clone(), mcp_url, dir, chats: Arc::new(Mutex::new(HashMap::new())) })
    }
}

#[derive(Default)]
struct Chat {
    agent: String,
    /// The backend's session id, so the next message continues the conversation.
    session: Option<String>,
    turn: Option<Box<dyn Turn>>,
    busy: bool,
}

type Chats = Arc<Mutex<HashMap<String, Chat>>>;

fn lock(chats: &Chats) -> MutexGuard<'_, HashMap<String, Chat>> {
    chats.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Counts a running agent against the cap until dropped.
struct Slot(Arc<AtomicUsize>);

impl Drop for Slot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// The chats of one link.
pub struct Session {
    agents: Arc<Agents>,
    mcp_url: String,
    dir: PathBuf,
    chats: Chats,
}

impl Session {
    /// Handles a chat frame from the tab; every answer goes out through `push`.
    pub fn on_frame(&self, frame: FromTab, push: Push) {
        match frame {
            FromTab::Cancel { chat } => stop_turn(&self.chats, &chat),
            FromTab::Chat { chat, agent, text, model } => {
                if let Err(e) = self.start_turn(&chat, &agent, text, model, &push) {
                    push(ToTab::Chat { chat, event: Event::Error(e) });
                }
            }
        }
    }

    pub fn infos(&self) -> Vec<AgentInfo> {
        self.agents.infos()
    }

    /// Stops every turn of this link (the tab left).
    pub fn stop_all(&self) {
        for c in lock(&self.chats).values_mut() {
            if let Some(t) = c.turn.as_mut() {
                t.stop();
            }
        }
    }

    fn start_turn(&self, chat: &str, agent: &str, text: String, model: Option<String>, push: &Push) -> Result<(), String> {
        let cfg = &self.agents.cfg;
        if !valid_id(chat) {
            return Err("that chat id is not valid".into());
        }
        if !cfg.enabled.iter().any(|a| a == agent) {
            return Err(format!("this server does not run the agent '{agent}'"));
        }
        if text.trim().is_empty() || text.len() > MAX_MESSAGE_BYTES {
            return Err("the message is empty or too long".into());
        }
        let slot = self.reserve()?;
        let session = {
            let mut chats = lock(&self.chats);
            if !chats.contains_key(chat) && chats.len() >= MAX_CHATS {
                return Err("too many chats on this link".into());
            }
            let c = chats.entry(chat.to_string()).or_default();
            if c.busy {
                return Err("the agent is still answering this chat".into());
            }
            if c.agent != agent {
                // Another agent cannot resume this one's session.
                c.session = None;
                c.agent = agent.to_string();
            }
            c.busy = true;
            c.session.clone()
        };
        let dir = self.dir.join(chat);
        let turn_cfg = TurnConfig {
            project_dir: dir.clone(),
            mcp_url: self.mcp_url.clone(),
            system: crate::ui::chat::hosted_system_prompt(),
            model,
            session,
            hosted: true,
            binary: cfg.claude_bin.clone(),
            ..Default::default()
        };
        let (chats, idle, agent, chat) = (self.chats.clone(), cfg.idle, agent.to_string(), chat.to_string());
        let (opencode_bin, push) = (cfg.opencode_bin.clone(), push.clone());
        std::thread::spawn(move || {
            let mut slot = Some(slot);
            let (tx, rx) = channel();
            let started = std::fs::create_dir_all(&dir)
                .map_err(|e| format!("cannot make the working folder: {e}"))
                .and_then(|()| launch(&agent, &turn_cfg, opencode_bin, &text, tx));
            match started {
                Ok(turn) => lock(&chats).entry(chat.clone()).or_default().turn = Some(turn),
                Err(e) => {
                    push(ToTab::Chat { chat: chat.clone(), event: Event::Error(e) });
                    finish_chat(&chats, &chat);
                    return;
                }
            }
            pump(&chats, &chat, &rx, idle, &push, &mut slot);
            finish_chat(&chats, &chat);
        });
        Ok(())
    }

    fn reserve(&self) -> Result<Slot, String> {
        let active = &self.agents.active;
        if active.fetch_add(1, Ordering::SeqCst) >= self.agents.cfg.max_turns {
            active.fetch_sub(1, Ordering::SeqCst);
            return Err("The server is already running as many agents as it allows. Try again in a moment.".into());
        }
        Ok(Slot(active.clone()))
    }
}

fn stop_turn(chats: &Chats, chat: &str) {
    if let Some(t) = lock(chats).get_mut(chat).and_then(|c| c.turn.as_mut()) {
        t.stop();
    }
}

fn finish_chat(chats: &Chats, chat: &str) {
    if let Some(c) = lock(chats).get_mut(chat) {
        c.busy = false;
        c.turn = None;
    }
}

/// Starts the agent's process for one message.
fn launch(agent: &str, cfg: &TurnConfig, opencode_bin: Option<PathBuf>, text: &str, tx: Sender<Event>) -> Result<Box<dyn Turn>, String> {
    if agent == CLAUDE {
        return claude_code::start(cfg, text, tx);
    }
    let program = opencode_bin.or_else(|| crate::chat::path::which("opencode")).ok_or("opencode was not found on the server")?;
    let args: Vec<String> = agents::find(OPENCODE).map(|d| d.launch.iter().map(|a| (*a).to_string()).collect()).unwrap_or_default();
    Ok(Box::new(acp::start(cfg, &program, &args, &agents::hosted_env(), text, tx)?))
}

/// Forwards events until the turn ends, the tab leaves or the agent goes quiet.
fn pump(chats: &Chats, chat: &str, rx: &Receiver<Event>, idle: Duration, push: &Push, slot: &mut Option<Slot>) {
    let say = |event| push(ToTab::Chat { chat: chat.to_string(), event });
    loop {
        let event = match rx.recv_timeout(idle) {
            Ok(e) => e,
            Err(RecvTimeoutError::Timeout) => {
                stop_turn(chats, chat);
                say(Event::Error("The agent stopped answering and was stopped.".into()));
                return;
            }
            Err(RecvTimeoutError::Disconnected) => {
                say(Event::Error("The agent ended without an answer.".into()));
                return;
            }
        };
        let ended = matches!(event, Event::TurnEnd { .. } | Event::Error(_));
        if let Event::Session(s) | Event::TurnEnd { session: Some(s), .. } = &event
            && let Some(c) = lock(chats).get_mut(chat)
        {
            c.session = Some(s.clone());
        }
        if ended {
            // Before the tab sees the end, so its next message is never refused as busy or capped.
            finish_chat(chats, chat);
            slot.take();
        }
        if !say(event) {
            stop_turn(chats, chat);
            return;
        }
        if ended {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_agents_list_is_checked() {
        let cfg = AgentsConfig::new("claude, opencode,claude", PathBuf::from("x")).unwrap();
        assert_eq!(cfg.enabled, [CLAUDE, OPENCODE]);
        assert!(AgentsConfig::new("claude,bash", PathBuf::from("x")).is_err());
        assert!(AgentsConfig::new(" ", PathBuf::from("x")).is_err());
    }
}
