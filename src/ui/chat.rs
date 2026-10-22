//! Chat mode: the chat column on the left, the live canvas on the right: state, event pump,
//! `send`, header, agent picker, composer and the setup state. The messages and cards are drawn
//! by `chat_msgs`.
//!
//! The mode and the column width are per machine (`chat_ui.json` in the config folder, next to the
//! theme choice), not in the project folder, so they do not sync to other people.

use super::chat_msgs::{self, Acts, View};
use crate::chat::events::{Event, ImageAttachment};
use crate::chat::{Backend, BackendKind, Chat, Message, Role, ToolCall, store};
use crate::project::format::{new_id, now_ms};
use crate::theme::{self, Icon};
use crate::ui::{UiState, kit};
use crate::editor::Editor;
use egui::{RichText, Ui};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::time::Duration;

pub const COLUMN_DEFAULT: f32 = 480.0;
pub const COLUMN_MIN: f32 = 420.0;
pub const COLUMN_MAX: f32 = 640.0;
/// Width of the inspector overlay.
const INSPECTOR_W: f32 = 272.0;
/// Context window of the CLIs and of the API models (all three list 200k).
const CONTEXT_WINDOW: u64 = 200_000;
/// The meter turns yellow above this share of the window.
const CONTEXT_WARN: f32 = 0.75;
const COMPOSER_MAX_H: f32 = 120.0;
const SELF_FOLLOW_QUIET: Duration = Duration::from_secs(10);
pub use crate::chat::TMP_DIR;
/// A run that started this long before the turn's first edit call or after its last still counts as its own
/// (events reach the UI a frame after the tool ran).
const RUN_SLACK_MS: u64 = 5_000;
const CLAUDE_INSTALL: &str = "npm install -g @anthropic-ai/claude-code";
const CODEX_INSTALL: &str = "npm install -g @openai/codex";
const REPAINT: Duration = Duration::from_millis(50);
#[cfg(not(target_arch = "wasm32"))]
const TITLE_CHARS: usize = 40;
const MAX_IMAGES: usize = 4;
const NEW_CHAT: &str = "New chat";
const WEB_NOTE: &str = "Chat needs the desktop app";

/// A running turn, whichever backend started it.
pub enum TurnHandle {
    Cli(Box<dyn crate::chat::events::Turn>),
    #[cfg(not(target_arch = "wasm32"))]
    Api(crate::chat::tools::ApiTurn),
}

impl TurnHandle {
    pub fn stop(&mut self) {
        match self {
            TurnHandle::Cli(t) => t.stop(),
            #[cfg(not(target_arch = "wasm32"))]
            TurnHandle::Api(t) => crate::chat::events::Turn::stop(t),
        }
    }

    pub fn is_running(&self) -> bool {
        match self {
            TurnHandle::Cli(t) => t.is_running(),
            #[cfg(not(target_arch = "wasm32"))]
            TurnHandle::Api(t) => crate::chat::events::Turn::is_running(t),
        }
    }

    /// Answer a `NeedsApproval` event (API backend only).
    pub fn approve(&self, id: &str, ok: bool) {
        #[cfg(not(target_arch = "wasm32"))]
        if let TurnHandle::Api(t) = self {
            t.approve(id, ok);
        }
        #[cfg(target_arch = "wasm32")]
        let _ = (id, ok);
    }
}

/// A CLI found on `PATH` (or at the configured path) and its version.
pub type Found = (PathBuf, String);

/// An API endpoint and whether a chat can use it now.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EndpointState {
    pub id: String,
    pub key: bool,
    /// URL and model are set, and a key is stored where one is required.
    pub ready: bool,
}

/// Which backends can run, read once in the background (a CLI is started with `--version`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Detect {
    pub claude: Option<Found>,
    pub codex: Option<Found>,
    pub endpoints: Vec<EndpointState>,
    pub default_endpoint: String,
}

impl Detect {
    pub fn has(&self, kind: BackendKind) -> bool {
        match kind {
            BackendKind::ClaudeCode => self.claude.is_some(),
            BackendKind::Codex => self.codex.is_some(),
            BackendKind::Anthropic => self.endpoints.iter().any(|e| e.ready),
        }
    }

    /// The endpoint with this id (None = the default one) can run.
    pub fn endpoint_ready(&self, id: Option<&str>) -> bool {
        let id = id.unwrap_or(&self.default_endpoint);
        self.endpoints.iter().any(|e| e.id == id && e.ready)
    }

    pub fn has_backend(&self, b: &Backend) -> bool {
        match b.kind {
            BackendKind::Anthropic => self.endpoint_ready(b.endpoint.as_deref()),
            k => self.has(k),
        }
    }

    /// The default endpoint when it can run, else the first that can.
    pub fn pick_endpoint(&self) -> Option<&str> {
        let ready = |e: &&EndpointState| e.ready;
        self.endpoints.iter().filter(ready).find(|e| e.id == self.default_endpoint).or_else(|| self.endpoints.iter().find(ready)).map(|e| e.id.as_str())
    }

    pub fn any(&self) -> bool {
        [BackendKind::ClaudeCode, BackendKind::Codex, BackendKind::Anthropic].iter().any(|k| self.has(*k))
    }

    /// The default backend when available, else the first one that is.
    pub fn pick(&self, preferred: BackendKind) -> BackendKind {
        [preferred, BackendKind::ClaudeCode, BackendKind::Codex, BackendKind::Anthropic]
            .into_iter()
            .find(|k| self.has(*k))
            .unwrap_or(preferred)
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn scan(prefs: &super::settings::ChatPrefs) -> Detect {
    use crate::chat::{claude_code, codex, endpoint};
    use std::path::Path;
    let endpoints = prefs
        .endpoints
        .iter()
        .map(|e| {
            let key = endpoint::get_key(e).is_some();
            EndpointState { id: e.id.clone(), key, ready: e.is_complete() && (key || !e.needs_key()) }
        })
        .collect();
    Detect {
        claude: claude_code::detect(prefs.claude_path.as_deref().map(Path::new)),
        codex: codex::detect(prefs.codex_path.as_deref().map(Path::new)),
        endpoints,
        default_endpoint: prefs.default_endpoint.clone(),
    }
}

/// The current selection as a composer chip.
#[derive(Debug, Clone, PartialEq)]
pub struct SelChip {
    pub label: String,
    pub keys: Vec<String>,
}

/// An image waiting in the composer.
#[derive(Debug, Clone, PartialEq)]
pub struct Attachment {
    pub name: String,
    pub img: ImageAttachment,
}

/// A destructive tool call waiting for the user (API backend).
#[derive(Debug, Clone, PartialEq)]
pub struct Approval {
    pub id: String,
    pub name: String,
    pub args: serde_json::Value,
}

/// What a running (or just ended) turn did, so the run it produced can be told apart from runs
/// other chats or other agents made.
struct TurnWindow {
    chat: String,
    /// Unix ms the turn was sent.
    start: u64,
    /// Unix ms of the first and the latest event of an editing tool call.
    edits: Option<(u64, u64)>,
}

impl TurnWindow {
    fn owns(&self, run: &crate::runs::Run) -> bool {
        let Some((first, last)) = self.edits else { return false };
        run.started >= self.start && run.started + RUN_SLACK_MS >= first && run.started <= last + RUN_SLACK_MS
    }
}

#[derive(Default)]
pub struct ChatState {
    /// The open chat.
    pub chat: Option<Chat>,
    /// Chats of the open project, newest first (refreshed on save).
    pub chats: Vec<Chat>,
    pub turn: Option<TurnHandle>,
    pub rx: Option<Receiver<Event>>,
    /// Chat the running turn belongs to: its events go there, whichever chat is open.
    turn_chat: Option<String>,
    /// Stop was pressed; the turn stays occupied until the backend's last event.
    pub stopping: bool,
    window: Option<TurnWindow>,
    /// Runs the tracker finished while a window was open, waiting for `settle_claims`.
    unclaimed: Vec<crate::runs::Run>,
    /// The turn made edits and ended: the app closes the open agent run now.
    finish_run: bool,
    /// Tokens in the context at the latest model call of the running turn.
    live_context: Option<u64>,
    pub approvals: Vec<Approval>,
    /// Streamed thinking of the last turn (the "Thinking" row; not saved).
    pub thinking: String,
    thinking_since: Option<web_time::Instant>,
    /// Seconds the last turn spent thinking, once it is over ("Thought for 12s").
    pub thought_secs: Option<u64>,
    /// The last turn's error, shown as a row with Retry.
    pub error: Option<String>,
    /// Project the state belongs to; a change resets it.
    pub project: Option<PathBuf>,
    pub project_name: String,
    /// The app's MCP endpoint, copied from `UiState::agent_endpoint`.
    pub mcp_url: String,
    /// Column width; 0 = default.
    pub width: f32,
    /// The inspector overlay is open.
    pub inspector: bool,
    /// What can run; None until the background scan answers.
    pub detect: Option<Detect>,
    /// Copy of the settings' chat prefs (endpoints), refreshed each frame, for `send`.
    pub prefs: super::settings::ChatPrefs,
    detect_rx: Option<Receiver<Detect>>,
    /// The agent picker is open.
    pub picker_open: bool,
    /// Composer text, selection chip and images.
    pub draft: String,
    pub sel: Option<SelChip>,
    pub images: Vec<Attachment>,
    /// Text of the title field while renaming.
    pub renaming: Option<String>,
    /// Chat waiting for the delete confirm.
    pub delete_chat: Option<String>,
    /// API key typed in the setup state.
    pub key_draft: String,
    pub key_error: Option<String>,
    /// Chat the backend defaults were applied to.
    defaults_for: String,
    /// Node the agent just touched; the canvas pans to it.
    follow: Option<String>,
    last_cam: Option<[f64; 3]>,
    cam_moved: Option<web_time::Instant>,
    /// Where API tool calls run; set by the app each frame (`App::tool_dispatch`).
    #[cfg(not(target_arch = "wasm32"))]
    pub tools: Option<crate::chat::events::ToolDispatch>,
    tool_started: HashMap<String, web_time::Instant>,
    /// Images for the next API turn (set by `submit`, taken by `send`).
    #[cfg(not(target_arch = "wasm32"))]
    pending_images: Vec<ImageAttachment>,
    width_dirty: bool,
}

#[derive(Default, Serialize, Deserialize)]
struct Prefs {
    #[serde(default)]
    width: Option<f32>,
    /// Project folders that were in Chat mode when last used.
    #[serde(default)]
    chat: Vec<String>,
}

impl Prefs {
    fn path() -> Option<PathBuf> {
        #[cfg(not(target_arch = "wasm32"))]
        return dirs::config_dir().map(|d| d.join("papercraft").join("chat_ui.json"));
        #[cfg(target_arch = "wasm32")]
        None
    }

    fn load() -> Prefs {
        Prefs::path()
            .and_then(|p| crate::vfs::read(p).ok())
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    fn save(&self) {
        let Some(p) = Prefs::path() else { return };
        if let Some(dir) = p.parent() {
            let _ = crate::vfs::create_dir_all(dir);
        }
        if let Ok(b) = serde_json::to_vec(self) {
            let _ = crate::vfs::write(p, b);
        }
    }
}

/// Remember whether `dir` was left in Chat mode.
pub fn remember_mode(dir: &std::path::Path, on: bool) {
    let mut p = Prefs::load();
    let key = dir.to_string_lossy().into_owned();
    p.chat.retain(|d| *d != key);
    if on {
        p.chat.push(key);
    }
    p.save();
}

/// The assistant message new text goes to: a new one after the user's message, and after tool
/// calls (so text and cards keep the order they arrived in).
fn text_target(chat: &mut Chat, now: u64) -> &mut Message {
    if chat.messages.last().is_some_and(|m| m.role == Role::Assistant && !m.tools.is_empty()) {
        chat.messages.push(Message { role: Role::Assistant, at: now, ..Default::default() });
    }
    last_assistant(chat, now)
}

fn last_assistant(chat: &mut Chat, now: u64) -> &mut Message {
    if chat.messages.last().is_none_or(|m| m.role != Role::Assistant) {
        chat.messages.push(Message { role: Role::Assistant, at: now, ..Default::default() });
    }
    chat.messages.last_mut().expect("just pushed")
}

fn new_chat() -> Chat {
    let now = now_ms();
    Chat { id: new_id(), title: NEW_CHAT.into(), created: now, updated: now, ..Default::default() }
}

impl ChatState {
    pub fn column_width(&self) -> f32 {
        if self.width <= 0.0 { COLUMN_DEFAULT } else { self.width.clamp(COLUMN_MIN, COLUMN_MAX) }
    }

    pub fn is_running(&self) -> bool {
        self.turn.is_some()
    }

    /// The project changed: drop everything, stop a running turn. Returns whether the new
    /// project was last left in Chat mode.
    pub fn switch_project(&mut self, dir: Option<PathBuf>) -> bool {
        self.stop();
        let prefs = Prefs::load();
        #[cfg(not(target_arch = "wasm32"))]
        let tools = self.tools.take();
        let on = !cfg!(target_arch = "wasm32")
            && dir.as_ref().is_some_and(|d| prefs.chat.contains(&d.to_string_lossy().into_owned()));
        let detect = self.detect.take();
        let chat_prefs = std::mem::take(&mut self.prefs);
        *self = ChatState {
            detect,
            prefs: chat_prefs,
            project: dir,
            width: prefs.width.unwrap_or(0.0),
            #[cfg(not(target_arch = "wasm32"))]
            tools,
            ..Default::default()
        };
        on
    }

    /// Asks the backend to stop. The turn stays running (and its chat busy) until the backend
    /// sends its last event; `pump` then folds the rest in, saves and ends it like any turn.
    pub fn stop(&mut self) {
        if let Some(t) = &mut self.turn {
            t.stop();
            self.stopping = true;
        }
    }

    /// Once: whether the app should close the open agent run (the turn that edited is over).
    pub fn take_finish_run(&mut self) -> bool {
        std::mem::take(&mut self.finish_run)
    }

    /// A run just finished (the tracker said so). It is claimed in `pump`, once the turn's own
    /// events are folded in.
    pub fn note_run(&mut self, run: &crate::runs::Run) {
        if self.window.is_some() {
            self.unclaimed.push(run.clone());
        }
    }

    /// Records each noted run on the turn's last assistant message when this turn made it: it
    /// started inside the window of the turn's editing calls. A run that started outside the
    /// turn (another chat, an external agent) is never claimed.
    fn settle_claims(&mut self) {
        let runs = std::mem::take(&mut self.unclaimed);
        if let Some((id, owned)) = self.window.as_ref().map(|w| (w.chat.clone(), runs.into_iter().filter(|r| w.owns(r)).collect::<Vec<_>>())) {
            let now = now_ms();
            for run in &owned {
                let attach = |chat: &mut Chat| {
                    if last_assistant(chat, now).run_id.is_some() {
                        chat.messages.push(Message { role: Role::Assistant, at: now, ..Default::default() });
                    }
                    chat.messages.last_mut().expect("an assistant message").run_id = Some(run.id.clone());
                };
                if let Some(c) = self.chat.as_mut().filter(|c| c.id == id) {
                    attach(c);
                    self.save();
                } else if let Some(dir) = &self.project
                    && let Some(mut c) = store::load(dir, &id)
                {
                    attach(&mut c);
                    let _ = store::save(dir, &c);
                }
            }
        }
        // Over, and the app has had its chance to close the run: forget the window.
        if self.turn.is_none() && !self.finish_run {
            self.window = None;
        }
    }

    /// Opens the newest chat of the project, or starts an empty one (saved on the first send).
    pub fn ensure_chat(&mut self) {
        if self.chat.is_some() {
            return;
        }
        let Some(dir) = &self.project else { return };
        self.chats = store::list(dir);
        self.chat = Some(self.chats.first().cloned().unwrap_or_else(new_chat));
    }

    fn save(&mut self) {
        let (Some(dir), Some(chat)) = (&self.project, &mut self.chat) else { return };
        chat.updated = now_ms();
        if let Err(e) = store::save(dir, chat) {
            log::warn!("could not save chat: {e}");
        }
        self.chats = store::list(dir);
    }

    /// Fold one backend event into the open chat. Returns true when the turn is over.
    pub fn apply(&mut self, ev: Event) -> bool {
        let Some(chat) = self.chat.as_mut() else { return false };
        let now = now_ms();
        match ev {
            Event::Text(d) => text_target(chat, now).text.push_str(&d),
            Event::Thinking(d) => {
                self.thinking_since.get_or_insert_with(web_time::Instant::now);
                self.thinking.push_str(&d);
            }
            Event::ToolStart { id, name, args } => {
                self.tool_started.insert(id.clone(), web_time::Instant::now());
                if crate::mcp::tools::edits_doc(&name, &args)
                    && let Some(w) = &mut self.window
                {
                    w.edits = Some((w.edits.map_or(now, |e| e.0), now));
                }
                if let Some(n) = chat_msgs::follow_node(&args) {
                    self.follow = Some(n);
                }
                let m = last_assistant(chat, now);
                match m.tools.iter_mut().find(|t| t.id == id) {
                    Some(t) => t.args = args,
                    None => m.tools.push(ToolCall { id, name, args, ..Default::default() }),
                }
            }
            Event::ToolEnd { id, result, error } => {
                if let Some((_, last)) = self.window.as_mut().and_then(|w| w.edits.as_mut()) {
                    *last = now;
                }
                let ms = self.tool_started.remove(&id).map(|s| s.elapsed().as_millis() as u64);
                let call = chat.messages.iter_mut().rev().flat_map(|m| m.tools.iter_mut()).find(|t| t.id == id);
                if let Some(t) = call {
                    (t.result, t.error, t.ms) = (result, error, ms);
                }
                self.approvals.retain(|a| a.id != id);
            }
            Event::Session(s) => {
                if chat.backend.kind != BackendKind::Anthropic {
                    chat.backend.cli_session = Some(s);
                }
            }
            Event::NeedsApproval { id, name, args } => self.approvals.push(Approval { id, name, args }),
            Event::Context(n) => self.live_context = Some(n),
            Event::TurnEnd { usage, session, .. } => {
                self.thought_secs = self.thinking_since.take().map(|t| t.elapsed().as_secs());
                if let Some(m) = chat.messages.last_mut().filter(|m| m.role == Role::Assistant) {
                    m.usage = usage;
                    m.context = self.live_context;
                }
                if let Some(s) = session.filter(|_| chat.backend.kind != BackendKind::Anthropic) {
                    chat.backend.cli_session = Some(s);
                }
                return true;
            }
            Event::Error(e) => {
                self.thought_secs = self.thinking_since.take().map(|t| t.elapsed().as_secs());
                self.error = Some(e);
                return true;
            }
        }
        false
    }

    /// Once per frame: drain the running turn's events into the turn's chat; save when it ends.
    pub fn pump(&mut self, ctx: &egui::Context) {
        if self.rx.is_some() {
            self.drain(ctx);
        }
        self.settle_claims();
    }

    fn drain(&mut self, ctx: &egui::Context) {
        let Some(rx) = &self.rx else { return };
        // Read before draining: a worker that stopped has sent everything already.
        let alive = self.turn.as_ref().is_some_and(TurnHandle::is_running);
        let events: Vec<Event> = rx.try_iter().collect();
        // The events belong to the turn's chat, not to whichever chat is open now.
        let parked = self.chat.as_ref().filter(|_| self.turn_chat.is_some()).is_some_and(|c| Some(&c.id) != self.turn_chat.as_ref()).then(|| {
            let turn = self.turn_chat.as_deref().zip(self.project.as_deref()).and_then(|(id, dir)| store::load(dir, id));
            std::mem::replace(&mut self.chat, turn)
        });
        let busy = !events.is_empty();
        let mut done = !alive;
        for ev in events {
            done |= self.apply(ev);
        }
        if done {
            self.finish_turn();
        } else if busy && parked.is_some() {
            self.save();
        }
        if let Some(open) = parked {
            self.chat = open;
        }
        if done {
            ctx.request_repaint();
        } else {
            ctx.request_repaint_after(REPAINT);
        }
    }

    fn finish_turn(&mut self) {
        self.turn = None;
        self.rx = None;
        self.stopping = false;
        self.approvals.clear();
        self.save();
        // The agent run does not wait for its 30 s idle timeout once the turn that made it is over.
        self.finish_run = self.window.as_ref().is_some_and(|w| w.edits.is_some());
        self.turn_chat = None;
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn system_prompt(&self) -> String {
        let skill = include_str!("../../skills/papercraft/SKILL.md");
        let dir = self.project.as_deref().map(|d| d.display().to_string()).unwrap_or_default();
        format!(
            "{skill}\n\nProject context: the open project is \"{}\" (folder {dir}). Read and edit it only through the papercraft tools.",
            self.project_name
        )
    }

    /// Adds `prompt` as a user message and starts a turn on the chat's backend. Errors (no CLI,
    /// no key) come back as text and are also kept in `error` for the error row.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn send(&mut self, ctx: &egui::Context, prompt: &str) -> Result<(), String> {
        use crate::chat::events::TurnConfig;
        use crate::chat::endpoint::ApiFormat;
        use crate::chat::{anthropic, claude_code, codex, openai};
        let prompt = prompt.trim();
        if prompt.is_empty() {
            return Ok(());
        }
        if self.turn.is_some() {
            return Err("A turn is already running".into());
        }
        self.ensure_chat();
        let system = self.system_prompt();
        let (Some(dir), Some(chat)) = (self.project.clone(), self.chat.as_mut()) else {
            return Err("Open a project to chat".into());
        };
        chat.messages.push(Message { role: Role::User, text: prompt.into(), at: now_ms(), ..Default::default() });
        if chat.title == NEW_CHAT {
            let text = chat_msgs::split_context(prompt).1;
            chat.title = text.lines().next().unwrap_or_default().chars().take(TITLE_CHARS).collect();
        }
        let turn_chat = chat.id.clone();
        let Backend { kind, model, effort, endpoint, cli_session } = chat.backend.clone();
        let ep = self.prefs.endpoint(endpoint.as_deref()).clone();
        // Another host's model is whatever the endpoint says now; the Claude host keeps the chat's pick.
        let model = if ep.is_claude_host() { model.or_else(|| Some(ep.model.clone())) } else { Some(ep.model.clone()) };
        let effort = effort.filter(|_| ep.is_claude_host());
        let cfg = TurnConfig {
            endpoint: ep.clone(),
            api_history: Some(dir.join("chats").join(format!("{}.api.json", chat.id))),
            project_dir: dir,
            mcp_url: self.mcp_url.clone(),
            model,
            effort,
            session: cli_session,
            tools: self.tools.clone(),
            images: std::mem::take(&mut self.pending_images),
            system,
            ..Default::default()
        };
        let (tx, rx) = std::sync::mpsc::channel();
        let started = match kind {
            BackendKind::ClaudeCode => claude_code::start(&cfg, prompt, tx).map(TurnHandle::Cli),
            BackendKind::Codex => codex::start(&cfg, prompt, tx).map(TurnHandle::Cli),
            BackendKind::Anthropic => match ep.format {
                ApiFormat::Anthropic => anthropic::start(&cfg, prompt, tx),
                ApiFormat::OpenAi => openai::start(&cfg, prompt, tx),
            }
            .map(TurnHandle::Api),
        };
        self.error = None;
        match started {
            Ok(t) => {
                self.thinking.clear();
                (self.thinking_since, self.thought_secs) = (None, None);
                self.save();
                self.stopping = false;
                self.live_context = None;
                self.window = Some(TurnWindow { chat: turn_chat.clone(), start: now_ms(), edits: None });
                self.turn_chat = Some(turn_chat);
                (self.turn, self.rx) = (Some(t), Some(rx));
                ctx.request_repaint();
                Ok(())
            }
            Err(e) => {
                // The message did not go anywhere: take it back so the user can resend.
                if let Some(c) = self.chat.as_mut() {
                    c.messages.pop();
                }
                self.error = Some(e.clone());
                Err(e)
            }
        }
    }
}

impl ChatState {
    /// Reads the background scan's answer; starts the first scan.
    pub fn poll_detect(&mut self, ctx: &egui::Context, prefs: &super::settings::ChatPrefs) {
        if self.prefs != *prefs {
            self.prefs.clone_from(prefs);
        }
        if let Some(rx) = &self.detect_rx {
            match rx.try_recv() {
                Ok(d) => {
                    self.detect = Some(d);
                    self.detect_rx = None;
                }
                Err(_) => ctx.request_repaint_after(REPAINT),
            }
            return;
        }
        if self.detect.is_none() {
            self.rescan(ctx, prefs);
        }
    }

    /// Looks for the CLIs and the key again, off the UI thread.
    pub fn rescan(&mut self, ctx: &egui::Context, prefs: &super::settings::ChatPrefs) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let (tx, rx) = std::sync::mpsc::channel();
            let (prefs, ctx) = (prefs.clone(), ctx.clone());
            std::thread::spawn(move || {
                let _ = tx.send(scan(&prefs));
                ctx.request_repaint();
            });
            self.detect_rx = Some(rx);
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = (ctx, prefs);
            self.detect = Some(Detect::default());
        }
    }

    /// An empty chat starts on the default backend (or the first one that works).
    pub fn ensure_defaults(&mut self, prefs: &super::settings::ChatPrefs) {
        let (Some(d), Some(c)) = (&self.detect, &mut self.chat) else { return };
        if c.id == self.defaults_for {
            return;
        }
        self.defaults_for.clone_from(&c.id);
        if !c.messages.is_empty() {
            return;
        }
        let kind = d.pick(prefs.default_backend);
        let ep = (kind == BackendKind::Anthropic).then(|| d.pick_endpoint()).flatten().map(|id| prefs.endpoint(Some(id)));
        c.backend = Backend {
            kind,
            model: ep.map(|e| e.model.clone()),
            effort: ep.filter(|e| e.is_claude_host()).map(|_| prefs.api_effort.clone()),
            endpoint: ep.map(|e| e.id.clone()),
            cli_session: None,
        };
    }

    fn reset_view(&mut self) {
        self.thinking.clear();
        (self.thinking_since, self.thought_secs, self.error) = (None, None, None);
        self.renaming = None;
    }

    pub fn rename(&mut self, title: &str) {
        let title = title.trim();
        let Some(c) = self.chat.as_mut().filter(|_| !title.is_empty()) else { return };
        c.title = title.to_owned();
        if !c.messages.is_empty() {
            self.save();
        }
    }

    pub fn start_new_chat(&mut self) {
        if self.turn.is_some() {
            return;
        }
        self.chat = Some(new_chat());
        self.reset_view();
    }

    pub fn switch_to(&mut self, id: &str) {
        if self.turn.is_some() {
            return;
        }
        if let Some(c) = self.chats.iter().find(|c| c.id == id) {
            self.chat = Some(c.clone());
            self.reset_view();
        }
    }

    /// Deletes the chat's files and the images saved for it.
    pub fn remove_chat(&mut self, id: &str) {
        let Some(dir) = self.project.clone() else { return };
        // The running turn still writes to its chat (and the API worker to its history file).
        if self.turn_chat.as_deref() == Some(id) {
            return;
        }
        store::delete(&dir, id);
        store::remove_tmp(&dir, id);
        self.chats = store::list(&dir);
        if self.chat.as_ref().is_some_and(|c| c.id == id) {
            self.chat = None;
            self.reset_view();
            self.ensure_chat();
        }
    }

    pub fn add_image(&mut self, name: String, mime: &str, data: Vec<u8>) {
        if self.images.len() < MAX_IMAGES {
            self.images.push(Attachment { name, img: ImageAttachment { mime: mime.into(), data } });
        }
    }

    /// Sends the composer: context lines (selection, images), then the text. API images go as
    /// blocks; CLI images are saved under the project and referenced by path.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn submit(&mut self, ctx: &egui::Context) -> Result<(), String> {
        let text = self.draft.trim().to_owned();
        if text.is_empty() {
            return Ok(());
        }
        self.ensure_chat();
        let (Some(dir), Some(chat)) = (self.project.clone(), self.chat.as_ref()) else {
            return Err("Open a project to chat".into());
        };
        let (api, chat_id) = (chat.backend.kind == BackendKind::Anthropic, chat.id.clone());
        let mut lines = Vec::new();
        if let Some(s) = &self.sel {
            lines.push(format!("[Selection] {} ({})", s.label, s.keys.join(", ")));
        }
        let mut pending = Vec::new();
        for a in &self.images {
            if api {
                lines.push(format!("[Attached] {}", a.name));
                pending.push(a.img.clone());
                continue;
            }
            let folder = store::tmp_of(&dir, &chat_id).ok_or("invalid chat id")?;
            let path = folder.join(format!("{}-{}", now_ms(), a.name));
            crate::vfs::create_dir_all(&folder).and_then(|()| crate::vfs::write(&path, &a.img.data)).map_err(|e| format!("could not save the image: {e}"))?;
            lines.push(format!("[Attached] {}", path.display()));
        }
        let prompt = if lines.is_empty() { text } else { format!("{}\n\n{text}", lines.join("\n")) };
        self.pending_images = pending;
        let r = self.send(ctx, &prompt);
        self.pending_images.clear();
        if r.is_ok() {
            (self.sel, self.images) = (None, Vec::new());
            self.draft.clear();
        }
        r
    }

    #[cfg(target_arch = "wasm32")]
    pub fn submit(&mut self, _ctx: &egui::Context) -> Result<(), String> {
        Err(WEB_NOTE.into())
    }

    #[cfg(target_arch = "wasm32")]
    pub fn send(&mut self, _ctx: &egui::Context, _prompt: &str) -> Result<(), String> {
        Err(WEB_NOTE.into())
    }

    /// The last user message, to send again.
    pub fn last_prompt(&self) -> Option<String> {
        self.chat.as_ref()?.messages.iter().rev().find(|m| m.role == Role::User).map(|m| m.text.clone())
    }

    /// Context in use at the latest model call and the window it counts against (never above it).
    /// None when the backend reports no per-call usage (Codex): the meter is hidden.
    pub fn context_use(&self) -> Option<(u64, u64)> {
        let saved = self.chat.as_ref().and_then(|c| c.messages.iter().rev().find_map(|m| m.context));
        let used = self.live_context.filter(|_| self.turn.is_some()).or(saved)?;
        Some((used.min(CONTEXT_WINDOW), CONTEXT_WINDOW))
    }
}

/// "Not found at /opt/x/claude" when a path is set in Settings, else "Not found on PATH".
fn not_found(custom: Option<&str>) -> String {
    custom.map_or_else(|| "Not found on PATH".to_owned(), |p| format!("Not found at {p}"))
}

fn model_name(id: &str) -> &str {
    super::settings::API_MODELS.iter().find(|m| m.0 == id).map_or(id, |m| m.1)
}

fn backend_label(b: &Backend, prefs: &super::settings::ChatPrefs) -> String {
    match b.kind {
        BackendKind::ClaudeCode => "Claude Code".into(),
        BackendKind::Codex => "Codex".into(),
        BackendKind::Anthropic => {
            let ep = prefs.endpoint(b.endpoint.as_deref());
            if ep.is_claude_host() {
                let model = b.model.as_deref().unwrap_or(&ep.model);
                format!("{} \u{b7} {} \u{b7} {}", ep.name, model_name(model), b.effort.as_deref().unwrap_or(&prefs.api_effort))
            } else {
                format!("{} \u{b7} {}", ep.name, ep.model)
            }
        }
    }
}

fn fmt_tokens(n: u64) -> String {
    if n >= 1000 { format!("{}k", n / 1000) } else { n.to_string() }
}

/// The current selection becomes a chip (Cmd+L or the @ button).
fn add_selection(ed: &mut Editor, st: &mut ChatState) {
    let ids = ed.top_selection();
    let Some(first) = ids.first() else {
        ed.status = "Select a layer to add it to the chat".into();
        return;
    };
    let page = ed.active_page;
    let keys = ids.iter().map(|id| crate::mcp::exec::node_key(ed, page, *id)).collect();
    let name = ed.doc.get(*first).map(|n| n.name.clone()).unwrap_or_default();
    let label = if ids.len() > 1 { format!("{name} +{}", ids.len() - 1) } else { name };
    st.sel = Some(SelChip { label, keys });
}

/// Pans the canvas to a node the agent touched, unless the user moved the camera lately.
fn follow(ed: &mut Editor, c: &mut ChatState) {
    let cam = |ed: &Editor| [ed.camera.pan.x, ed.camera.pan.y, ed.camera.zoom];
    if c.last_cam.is_some_and(|l| l != cam(ed)) {
        c.cam_moved = Some(web_time::Instant::now());
    }
    if let Some(key) = c.follow.take()
        && c.cam_moved.is_none_or(|t| t.elapsed() >= SELF_FOLLOW_QUIET)
    {
        pan_to(ed, &key);
    }
    c.last_cam = Some(cam(ed));
}

fn pan_to(ed: &mut Editor, key: &str) {
    let Some((prefix, num)) = key.split_once(':') else { return };
    let (Ok(page), Ok(n)) = (crate::mcp::exec::find_page(ed, prefix), num.parse::<u64>()) else { return };
    if page != ed.active_page {
        return;
    }
    let Some(r) = ed.page_doc(page).world_rect(crate::model::NodeId(n)) else { return };
    let v = ed.camera.visible_world(ed.viewport);
    if r.x >= v.x && r.y >= v.y && r.x + r.w <= v.x + v.w && r.y + r.h <= v.y + v.h {
        return;
    }
    ed.camera.pan = r.center() - ed.viewport / (2.0 * ed.camera.zoom);
}

/// The left column: replaces the layers panel in Chat mode.
pub fn panel(ui: &mut Ui, ed: &mut Editor, st: &mut UiState, frame: egui::Frame, floating: bool, fill: egui::Color32) {
    let r = egui::Panel::left("chat")
        .resizable(true)
        .default_size(st.chat.column_width())
        .size_range(COLUMN_MIN..=COLUMN_MAX)
        .frame(frame)
        .show(ui, |ui| {
            if floating {
                super::floating_panel(ui, fill, egui::Margin::symmetric(14, 12), |ui| column(ui, ed, st));
            } else {
                column(ui, ed, st);
            }
        });
    let w = r.response.rect.width();
    if (w - st.chat.column_width()).abs() > 0.5 {
        st.chat.width = w;
        st.chat.width_dirty = true;
    }
    // Write the width once the drag is over.
    if st.chat.width_dirty && !ui.input(|i| i.pointer.any_down()) {
        st.chat.width_dirty = false;
        let mut p = Prefs::load();
        p.width = Some(st.chat.width);
        p.save();
    }
}

/// The inspector as a narrow overlay on the canvas's right edge.
pub fn inspector_overlay(ctx: &egui::Context, ed: &mut Editor, st: &mut UiState, canvas: egui::Rect) {
    let p = theme::pal_from_ctx(ctx);
    let gap = super::PANEL_GAP;
    let size = egui::vec2(INSPECTOR_W, (canvas.height() - 2.0 * gap).max(120.0));
    egui::Area::new("chat-inspector".into())
        .order(egui::Order::Foreground)
        .fixed_pos(canvas.right_top() + egui::vec2(-size.x - gap, gap))
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(p.inspector)
                .stroke(egui::Stroke::new(1.0, p.separator))
                .corner_radius(theme::RADIUS_PANEL)
                .inner_margin(egui::Margin::symmetric(14, 12))
                .show(ui, |ui| {
                    ui.set_min_size(size - egui::vec2(28.0, 24.0));
                    ui.set_max_size(size - egui::vec2(28.0, 24.0));
                    super::inspector::panel(ui, ed, st);
                });
        });
}

/// "Waiting for approval" at the top of the canvas while a tool waits for the user.
pub fn canvas_pill(ctx: &egui::Context, st: &UiState, canvas: egui::Rect) {
    if st.chat.approvals.is_empty() {
        return;
    }
    let p = theme::pal_from_ctx(ctx);
    egui::Area::new("chat-approval-pill".into())
        .order(egui::Order::Foreground)
        .fade_in(false)
        .pivot(egui::Align2::CENTER_TOP)
        .fixed_pos(canvas.center_top() + egui::vec2(0.0, 14.0))
        .interactable(false)
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(p.popover)
                .stroke(egui::Stroke::new(1.0, p.warning))
                .corner_radius(egui::CornerRadius::same(255))
                .inner_margin(egui::Margin::symmetric(12, 6))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let (r, _) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
                        theme::paint_icon(ui.painter(), r, Icon::Lock, p.warning);
                        ui.label(RichText::new("Waiting for approval").font(theme::semibold(theme::BODY)));
                    });
                });
        });
}

fn column(ui: &mut Ui, ed: &mut Editor, st: &mut UiState) {
    let ctx = ui.ctx().clone();
    let column_rect = ui.max_rect();
    st.chat.ensure_chat();
    if std::mem::take(&mut st.settings.chat_changed) {
        // Paths, key or Re-scan changed in Settings: what was detected is stale.
        st.chat.rescan(&ctx, &st.settings.chat);
    }
    st.chat.poll_detect(&ctx, &st.settings.chat);
    st.chat.ensure_defaults(&st.settings.chat);
    follow(ed, &mut st.chat);
    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::L)) {
        add_selection(ed, &mut st.chat);
    }
    ui.spacing_mut().item_spacing.y = 6.0;
    let chip = header(ui, st);
    ui.add_space(6.0);
    let setup = st.chat.detect.as_ref().is_some_and(|d| !d.any());
    let mut acts = Acts::default();
    egui::Panel::bottom("chat-composer").frame(egui::Frame::NONE).resizable(false).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 6.0;
        if let Some(todos) = st.chat.chat.as_ref().map(chat_msgs::latest_plan).filter(|t| !t.is_empty()) {
            chat_msgs::plan_card(ui, &todos);
        }
        composer(ui, ed, st, column_rect, setup);
    });
    if st.review_run.is_some() {
        super::panels::review(ui, ed, st);
    } else if setup {
        setup_view(ui, st);
    } else if st.chat.chat.as_ref().is_none_or(|c| c.messages.is_empty()) && !st.chat.is_running() {
        chat_msgs::empty(ui, &mut acts);
    } else if let Some(chat) = &st.chat.chat {
        let view = View {
            running: st.chat.is_running(),
            thinking: &st.chat.thinking,
            thought_secs: st.chat.thought_secs,
            runs: &st.runs,
            approvals: &st.chat.approvals,
            error: st.chat.error.as_deref(),
        };
        chat_msgs::list(ui, chat, &view, &mut acts);
    }
    if let Some(chip) = chip {
        picker(&ctx, st, chip);
    }
    apply_acts(&ctx, ed, st, acts);
    if let Some(dir) = st.project_dir.clone() {
        super::panels::revert_dialog(&ctx, ed, st, &dir);
        super::panels::restore_dialog(&ctx, ed, st, &dir);
    }
    if let Some(id) = st.chat.delete_chat.clone()
        && let Some(yes) = super::confirm::dialog(
            &ctx,
            "chat-delete",
            "Delete chat?",
            "The conversation and the images saved for it are removed. Agent runs and their changes stay.",
            "Delete",
        )
    {
        st.chat.delete_chat = None;
        if yes {
            st.chat.remove_chat(&id);
        }
    }
}

fn apply_acts(ctx: &egui::Context, ed: &mut Editor, st: &mut UiState, acts: Acts) {
    if let Some((id, ok)) = acts.approve {
        if let Some(t) = &st.chat.turn {
            t.approve(&id, ok);
        }
        st.chat.approvals.retain(|a| a.id != id);
    }
    if let Some(id) = acts.revert {
        st.pending_run_revert = Some(id);
    }
    if let Some(id) = acts.restore {
        st.pending_restore = Some(id);
    }
    if let Some(id) = acts.review {
        st.review_run = Some(id);
    }
    if let Some(id) = acts.accept {
        super::panels::accept_run(ed, st, &id);
    }
    let again = acts.retry_last.then(|| st.chat.last_prompt()).flatten();
    if let Some(text) = acts.send.or(again) {
        let _ = st.chat.send(ctx, &text);
    }
}

/// Title, chat list, settings and the agent chip with the context meter. Returns the chip's rect.
fn header(ui: &mut Ui, st: &mut UiState) -> Option<egui::Rect> {
    let p = theme::pal(ui);
    let title = st.chat.chat.as_ref().map_or(NEW_CHAT, |c| c.title.as_str()).to_owned();
    ui.horizontal(|ui| {
        let right = 64.0;
        if let Some(mut t) = st.chat.renaming.take() {
            let r = ui.add(egui::TextEdit::singleline(&mut t).desired_width(ui.available_width() - right));
            if !r.has_focus() && !r.lost_focus() {
                r.request_focus();
            }
            if !r.lost_focus() {
                st.chat.renaming = Some(t);
            } else if !ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                st.chat.rename(&t);
            }
        } else {
            ui.add(egui::Label::new(RichText::new(&title).font(theme::semibold(theme::BODY))).truncate());
            if theme::icon_button(ui, Icon::Pencil, 22.0, false).on_hover_text("Rename chat").clicked() {
                st.chat.renaming = Some(title.clone());
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            if kit::lottie_button(ui, "settings", "Chat settings", 26.0).clicked() {
                st.settings.open_chat();
            }
            let list = theme::icon_button(ui, Icon::Page, 26.0, false).on_hover_text("Chats");
            chat_menu(&list, st);
        });
    });
    let label = match (&st.chat.chat, &st.chat.detect) {
        (_, Some(d)) if !d.any() => "No agent connected".to_owned(),
        (Some(c), _) => backend_label(&c.backend, &st.settings.chat),
        _ => String::new(),
    };
    let context = st.chat.context_use();
    let mut rect = None;
    ui.horizontal(|ui| {
        let r = kit::chip_ink(ui, &label, st.chat.picker_open.then(theme::accent));
        rect = Some(r.rect);
        if r.clicked() {
            st.chat.picker_open = !st.chat.picker_open;
        }
        let Some((used, window)) = context else { return };
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new(format!("{} / {}", fmt_tokens(used), fmt_tokens(window))).size(theme::SECONDARY).color(p.secondary));
            let (bar, _) = ui.allocate_exact_size(egui::vec2(36.0, 3.0), egui::Sense::hover());
            ui.painter().rect_filled(bar, 2.0, p.separator);
            let frac = (used as f32 / window as f32).clamp(0.0, 1.0);
            let fill = egui::Rect::from_min_size(bar.min, egui::vec2(bar.width() * frac, bar.height()));
            ui.painter().rect_filled(fill, 2.0, if frac > CONTEXT_WARN { p.warning } else { theme::accent() });
        });
    });
    rect
}

/// New chat, the other chats of the project (switch), and delete.
fn chat_menu(anchor: &egui::Response, st: &mut UiState) {
    let running = st.chat.is_running();
    egui::Popup::menu(anchor).width(280.0).show(|ui| {
        let p = theme::pal(ui);
        ui.spacing_mut().item_spacing.y = 2.0;
        if kit::button_wide(ui, kit::ButtonKind::Primary, Some(Icon::Plus), "New chat").clicked() && !running {
            st.chat.start_new_chat();
            ui.close();
        }
        ui.add_space(4.0);
        let current = st.chat.chat.as_ref().map(|c| c.id.clone());
        let now = now_ms();
        for c in st.chat.chats.clone() {
            ui.horizontal(|ui| {
                let on = current.as_deref() == Some(c.id.as_str());
                ui.add_enabled_ui(!running, |ui| {
                    let title = RichText::new(&c.title).font(if on { theme::semibold(theme::BODY) } else { egui::FontId::proportional(theme::BODY) });
                    let w = ui.available_width() - 30.0;
                    let r = ui.add_sized([w, 24.0], egui::Button::selectable(on, title).frame(false));
                    if r.on_hover_text(super::home::relative_time(c.updated, now)).clicked() {
                        st.chat.switch_to(&c.id);
                        ui.close();
                    }
                });
                // The chat of a running turn cannot be deleted: its events and history still land there.
                let busy = running && st.chat.turn_chat.as_deref() == Some(c.id.as_str());
                let del = ui.add_enabled_ui(!busy, |ui| theme::icon_button(ui, Icon::Trash, 24.0, false).on_hover_text("Delete chat").clicked()).inner;
                if del {
                    st.chat.delete_chat = Some(c.id.clone());
                    ui.close();
                }
            });
        }
        if st.chat.chats.is_empty() {
            ui.label(RichText::new("No saved chats yet").size(theme::SECONDARY).color(p.tertiary));
        }
    });
}

fn section_label(ui: &mut Ui, text: &str) {
    ui.add_space(4.0);
    ui.label(RichText::new(text).font(theme::semibold(theme::SECONDARY)).color(theme::pal(ui).tertiary));
}

/// One agent in the picker: name, status line, optional install hint, state chip. Click selects.
fn agent_row(ui: &mut Ui, name: &str, sub: &str, hint: Option<&str>, found: bool, selected: bool) -> egui::Response {
    let p = theme::pal(ui);
    let r = egui::Frame::new()
        .fill(if selected { p.field } else { egui::Color32::TRANSPARENT })
        .corner_radius(theme::RADIUS_LG)
        .inner_margin(egui::Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 1.0;
                    let ink = if found { p.text } else { p.tertiary };
                    ui.label(RichText::new(name).font(theme::semibold(theme::BODY)).color(ink));
                    ui.add(egui::Label::new(RichText::new(sub).size(theme::SECONDARY).color(p.secondary)).truncate());
                    if let Some(h) = hint {
                        ui.label(RichText::new(h).monospace().size(theme::SECONDARY).color(theme::accent()));
                    }
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if found {
                        kit::chip_ink(ui, if selected { "Selected" } else { "Found" }, Some(p.success));
                    } else {
                        kit::chip_ink(ui, "Missing", None);
                    }
                });
            });
        })
        .response;
    ui.interact(r.rect, r.id.with("pick"), egui::Sense::click())
}

fn picker(ctx: &egui::Context, st: &mut UiState, chip: egui::Rect) {
    if !st.chat.picker_open {
        return;
    }
    let p = theme::pal_from_ctx(ctx);
    let running = st.chat.is_running();
    let detect = st.chat.detect.clone().unwrap_or_default();
    let backend = st.chat.chat.as_ref().map(|c| c.backend.clone()).unwrap_or_default();
    let prefs = st.settings.chat.clone();
    let picker_w = st.chat.column_width() - 2.0 * 14.0;
    let mut choose: Option<Backend> = None;
    let mut open_settings = false;
    let mut again = false;
    let area = egui::Area::new("chat-picker".into())
        .order(egui::Order::Foreground)
        .fade_in(false)
        .fixed_pos(chip.left_bottom() + egui::vec2(0.0, 6.0))
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(p.popover)
                .stroke(egui::Stroke::new(1.0, p.separator))
                .corner_radius(theme::RADIUS_PANEL)
                .inner_margin(10)
                .show(ui, |ui| {
                    ui.set_width(picker_w - 20.0);
                    ui.spacing_mut().item_spacing.y = 2.0;
                    section_label(ui, "LOCAL AGENTS");
                    for (kind, name, found, install, custom) in [
                        (BackendKind::ClaudeCode, "Claude Code", &detect.claude, CLAUDE_INSTALL, &prefs.claude_path),
                        (BackendKind::Codex, "Codex", &detect.codex, CODEX_INSTALL, &prefs.codex_path),
                    ] {
                        let (sub, hint) = match found {
                            Some((path, v)) => (format!("Found \u{b7} v{v} \u{b7} {}", path.display()), None),
                            None => (not_found(custom.as_deref()), Some(install)),
                        };
                        let r = agent_row(ui, name, &sub, hint, found.is_some(), backend.kind == kind);
                        if r.clicked() && found.is_some() && !running {
                            choose = Some(Backend { kind, ..Default::default() });
                        }
                        if r.clicked() && found.is_none() {
                            ui.ctx().copy_text(install.to_owned());
                        }
                    }
                    section_label(ui, "API ENDPOINTS");
                    let current = backend.endpoint.clone().unwrap_or_else(|| prefs.default_endpoint.clone());
                    for ep in &prefs.endpoints {
                        let state = detect.endpoints.iter().find(|s| s.id == ep.id);
                        let ready = state.is_some_and(|s| s.ready);
                        let host = crate::chat::endpoint::host(&ep.base_url).unwrap_or_else(|| ep.base_url.clone());
                        let sub = match state {
                            _ if !ep.is_complete() => "Set the URL and model in Settings, Chat".to_string(),
                            Some(s) if s.key => format!("{host} \u{b7} {} \u{b7} key set", ep.model),
                            Some(s) if ep.needs_key() && !s.key => "No key. Add one in Settings, Chat".to_string(),
                            _ => format!("{host} \u{b7} {}", ep.model),
                        };
                        let on = backend.kind == BackendKind::Anthropic && current == ep.id;
                        if agent_row(ui, &ep.name, &sub, None, ready, on).clicked() && ready && !running {
                            choose = Some(Backend {
                                kind: BackendKind::Anthropic,
                                model: Some(ep.model.clone()),
                                effort: ep.is_claude_host().then(|| prefs.api_effort.clone()),
                                endpoint: Some(ep.id.clone()),
                                cli_session: None,
                            });
                        }
                    }
                    let selected = prefs.endpoint(backend.endpoint.as_deref());
                    if backend.kind == BackendKind::Anthropic && selected.is_claude_host() {
                        let model = backend.model.clone().unwrap_or_else(|| selected.model.clone());
                        section_label(ui, "MODEL");
                        for (id, label) in super::settings::API_MODELS {
                            let on = model == id;
                            let r = ui.add(egui::Button::selectable(on, RichText::new(format!("{label}   {id}")).size(theme::SECONDARY + 1.0)).frame(false));
                            if r.clicked() && !running {
                                choose = Some(Backend { model: Some(id.into()), ..backend.clone() });
                            }
                        }
                        section_label(ui, "EFFORT");
                        let effort = backend.effort.clone().unwrap_or_else(|| prefs.api_effort.clone());
                        let items: Vec<(&str, Option<&str>)> = super::settings::API_EFFORTS.iter().map(|(_, l)| (*l, None)).collect();
                        let cur = super::settings::API_EFFORTS.iter().position(|(id, _)| *id == effort).unwrap_or(1);
                        if let Some(i) = kit::segmented(ui, &items, cur)
                            && !running
                        {
                            choose = Some(Backend { effort: Some(super::settings::API_EFFORTS[i].0.into()), ..backend.clone() });
                        }
                    }
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        again = kit::chip(ui, "Detect again", false).clicked();
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            open_settings = kit::chip_ink(ui, "Settings \u{2192} Chat", Some(theme::accent())).clicked();
                        });
                    });
                });
        });
    if let Some(b) = choose
        && let Some(c) = st.chat.chat.as_mut()
    {
        c.backend = Backend { cli_session: if b.kind == c.backend.kind { c.backend.cli_session.clone() } else { None }, ..b };
        if !c.messages.is_empty() {
            st.chat.save();
        }
        st.chat.picker_open = running;
    }
    if again {
        st.chat.rescan(ctx, &prefs);
    }
    if open_settings {
        st.settings.open_chat();
        st.chat.picker_open = false;
    }
    let outside = ctx.input(|i| i.pointer.any_pressed()) && ctx.pointer_interact_pos().is_some_and(|pos| !area.response.rect.contains(pos) && !chip.contains(pos));
    if outside {
        st.chat.picker_open = false;
    }
}

fn setup_view(ui: &mut Ui, st: &mut UiState) {
    let p = theme::pal(ui);
    let ctx = ui.ctx().clone();
    ui.add_space(ui.available_height() * 0.08);
    kit::badge(ui, Icon::Laser, p.warning, 40.0);
    ui.add_space(6.0);
    ui.label(RichText::new("Connect an agent to start").font(theme::semibold(18.0)));
    ui.add(egui::Label::new(RichText::new("Chat needs a local agent CLI or an API key. Pick one, it takes a minute.").color(p.secondary)));
    ui.add_space(8.0);
    let step = |ui: &mut Ui, n: u8, title: &str, body: &str, add: &mut dyn FnMut(&mut Ui)| {
        egui::Frame::new().fill(p.popover).corner_radius(theme::RADIUS_LG).inner_margin(12).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(RichText::new(n.to_string()).font(theme::semibold(theme::BODY)).color(theme::accent()));
                ui.label(RichText::new(title).font(theme::semibold(theme::BODY)));
            });
            ui.label(RichText::new(body).size(theme::SECONDARY).color(p.secondary));
            add(ui);
        });
    };
    step(ui, 1, "Install a local agent", "Uses your own login. PaperCraft passes no keys.", &mut |ui| {
        for cmd in [CLAUDE_INSTALL, CODEX_INSTALL] {
            egui::Frame::new().fill(p.field).corner_radius(theme::RADIUS_MD).inner_margin(egui::Margin::symmetric(10, 6)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(RichText::new(cmd).monospace().size(theme::SECONDARY).color(theme::accent()));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if kit::lottie_button(ui, "copy", "Copy", 22.0).clicked() {
                            ui.ctx().copy_text(cmd.to_owned());
                        }
                    });
                });
            });
        }
    });
    #[cfg(not(target_arch = "wasm32"))]
    step(ui, 2, "Or add an API key", "For the default endpoint. Stored in the system keychain, never in the project. More endpoints are in Settings.", &mut |ui| {
        ui.add(egui::TextEdit::singleline(&mut st.chat.key_draft).password(true).margin(theme::INPUT_MARGIN).hint_text("sk-ant-\u{2026}").desired_width(f32::INFINITY));
        if let Some(e) = &st.chat.key_error {
            ui.label(RichText::new(e).size(theme::SECONDARY).color(p.danger));
        }
        if kit::button(ui, kit::ButtonKind::Primary, None, "Save key").clicked() && !st.chat.key_draft.trim().is_empty() {
            match crate::chat::endpoint::set_key(st.settings.chat.default_endpoint(), &st.chat.key_draft) {
                Ok(()) => {
                    st.chat.key_draft.clear();
                    st.chat.key_error = None;
                    st.chat.rescan(&ctx, &st.settings.chat);
                }
                Err(e) => st.chat.key_error = Some(e),
            }
        }
    });
    step(ui, 3, "Check again", "PaperCraft looks for claude and codex on your PATH.", &mut |ui| {
        ui.horizontal(|ui| {
            if kit::button(ui, kit::ButtonKind::Secondary, Some(Icon::Restore), "Detect again").clicked() {
                st.chat.rescan(&ctx, &st.settings.chat);
            }
            if kit::button(ui, kit::ButtonKind::Secondary, None, "Open settings").clicked() {
                st.settings.open_chat();
            }
        });
    });
}

#[cfg(not(target_arch = "wasm32"))]
fn image_mime(name: &str) -> Option<&'static str> {
    let ext = name.rsplit('.').next()?.to_ascii_lowercase();
    Some(match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        _ => return None,
    })
}

/// Images from the clipboard (a paste with an image on it) and from files dropped on the column.
#[cfg(not(target_arch = "wasm32"))]
fn take_images(ui: &Ui, st: &mut ChatState, column: egui::Rect) {
    let pasted = ui.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Paste(_))));
    if pasted && let Some(png) = clipboard_png() {
        st.add_image(format!("pasted-{}.png", now_ms()), "image/png", png);
    }
    if !ui.rect_contains_pointer(column) {
        return;
    }
    for f in ui.input(|i| i.raw.dropped_files.clone()) {
        let name = f.path().file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if let (Some(mime), Ok(data)) = (image_mime(&name), f.bytes()) {
            st.add_image(name, mime, data);
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn clipboard_png() -> Option<Vec<u8>> {
    let img = arboard::Clipboard::new().ok()?.get_image().ok()?;
    let rgba = image::RgbaImage::from_raw(img.width as u32, img.height as u32, img.bytes.into_owned())?;
    let mut out = std::io::Cursor::new(Vec::new());
    rgba.write_to(&mut out, image::ImageFormat::Png).ok()?;
    Some(out.into_inner())
}

fn composer(ui: &mut Ui, ed: &mut Editor, st: &mut UiState, column: egui::Rect, setup: bool) {
    let p = theme::pal(ui);
    let no_project = st.project_dir.is_none();
    let backend_ok = !setup
        && st.chat.chat.as_ref().is_some_and(|c| st.chat.detect.as_ref().is_none_or(|d| d.has_backend(&c.backend)));
    let running = st.chat.is_running();
    let input_id = egui::Id::new("chat-input");
    let focused = ui.memory(|m| m.has_focus(input_id));
    #[cfg(not(target_arch = "wasm32"))]
    if focused {
        take_images(ui, &mut st.chat, column);
    }
    #[cfg(target_arch = "wasm32")]
    let _ = column;
    let enter = focused && ui.input_mut(|i| !i.modifiers.shift && i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
    let mut submit = false;
    egui::Frame::new().fill(p.popover).corner_radius(theme::RADIUS_PANEL).inner_margin(egui::Margin::symmetric(12, 10)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = 6.0;
        if st.chat.sel.is_some() || !st.chat.images.is_empty() {
            ui.horizontal_wrapped(|ui| {
                if let Some(s) = &st.chat.sel
                    && kit::chip_ink(ui, &format!("@ Selection \u{b7} {} \u{d7}", s.label), Some(theme::accent())).clicked()
                {
                    st.chat.sel = None;
                }
                let mut drop = None;
                for (i, a) in st.chat.images.iter().enumerate() {
                    if kit::chip(ui, &format!("{} \u{d7}", a.name), false).clicked() {
                        drop = Some(i);
                    }
                }
                if let Some(i) = drop {
                    st.chat.images.remove(i);
                }
            });
        }
        let hint = if no_project {
            "Open a project to chat"
        } else if !backend_ok {
            "Connect an agent to start chatting"
        } else if running {
            "Reply, or press Stop to interrupt"
        } else {
            "Ask for a design, a fix or a review"
        };
        egui::ScrollArea::vertical().max_height(COMPOSER_MAX_H).id_salt("chat-input-scroll").show(ui, |ui| {
            ui.add_enabled(
                !no_project && backend_ok,
                egui::TextEdit::multiline(&mut st.chat.draft)
                    .id(input_id)
                    .hint_text(hint)
                    .frame(egui::Frame::NONE)
                    .desired_rows(2)
                    .desired_width(f32::INFINITY)
                    .lock_focus(true),
            );
        });
        ui.horizontal(|ui| {
            if kit::chip(ui, "@ Selection", false).on_hover_text("Add the selection (Cmd+L)").clicked() {
                add_selection(ed, &mut st.chat);
            }
            #[cfg(not(target_arch = "wasm32"))]
            if theme::icon_button(ui, Icon::Image, 24.0, false).on_hover_text("Attach an image (or paste, or drop one)").clicked()
                && let Some(path) = rfd::FileDialog::new().add_filter("Images", &["png", "jpg", "jpeg", "gif", "webp"]).pick_file()
                && let Some(name) = path.file_name().map(|n| n.to_string_lossy().into_owned())
                && let (Some(mime), Ok(data)) = (image_mime(&name), std::fs::read(&path))
            {
                st.chat.add_image(name, mime, data);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if running {
                    let stopping = st.chat.stopping;
                    let label = if stopping { "Stopping\u{2026}" } else { "Stop" };
                    if ui.add_enabled_ui(!stopping, |ui| kit::button(ui, kit::ButtonKind::Danger, None, label).clicked()).inner {
                        st.chat.stop();
                    }
                } else {
                    let can = backend_ok && !no_project && !st.chat.draft.trim().is_empty();
                    ui.add_enabled_ui(can, |ui| {
                        submit = kit::button(ui, kit::ButtonKind::Primary, Some(Icon::ArrowRight), "Send").clicked();
                    });
                    ui.label(RichText::new("Enter sends \u{b7} Shift+Enter newline").size(theme::SECONDARY).color(p.tertiary));
                }
            });
        });
    });
    if (submit || enter) && !running && backend_ok && !no_project {
        let ctx = ui.ctx().clone();
        let _ = st.chat.submit(&ctx);
    }
}

/// The `Design | Chat` switch for the top bar. Returns the new mode when the user picked one.
pub fn mode_switch(ui: &mut Ui, chat_mode: bool) -> Option<bool> {
    let off = cfg!(target_arch = "wasm32").then_some(WEB_NOTE);
    kit::segmented(ui, &[("Design", None), ("Chat", off)], usize::from(chat_mode)).map(|i| i == 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::claude_code::parse_line_partial;

    const TOOL: &str = include_str!("../../tests/fixtures/chat/claude_code_tool.jsonl");

    #[test]
    fn pump_folds_a_recorded_claude_code_turn() {
        let dir = std::env::temp_dir().join(format!("papercraft-chat-pump-{:016x}", fastrand::u64(..)));
        let mut st = ChatState { project: Some(dir.clone()), ..Default::default() };
        st.ensure_chat();
        let (tx, rx) = std::sync::mpsc::channel();
        for ev in TOOL.lines().flat_map(parse_line_partial) {
            tx.send(ev).unwrap();
        }
        st.rx = Some(rx);
        st.pump(&egui::Context::default());

        let chat = st.chat.as_ref().unwrap();
        let tools: Vec<&ToolCall> = chat.messages.iter().flat_map(|m| &m.tools).collect();
        assert!(tools.iter().any(|t| t.name == "get_basic_info" && (t.result.is_some() || t.error.is_some())), "{tools:?}");
        assert!(tools.iter().all(|t| t.result.is_some() || t.error.is_some()), "every call finished");
        assert!(chat.messages.iter().any(|m| !m.text.is_empty()));
        assert!(chat.messages.iter().any(|m| m.usage.is_some()));
        assert_eq!(chat.backend.cli_session.as_deref().map(str::len), Some(36));
        assert!(st.rx.is_none(), "turn over");
        assert_eq!(store::list(&dir).len(), 1, "saved on turn end");
        std::fs::remove_dir_all(dir).ok();
    }

    struct FakeTurn {
        stopped: std::sync::Arc<std::sync::atomic::AtomicBool>,
        running: std::sync::Arc<std::sync::atomic::AtomicBool>,
    }

    impl crate::chat::events::Turn for FakeTurn {
        fn stop(&mut self) {
            self.stopped.store(true, std::sync::atomic::Ordering::SeqCst);
        }
        fn is_running(&self) -> bool {
            self.running.load(std::sync::atomic::Ordering::SeqCst)
        }
    }

    type Running = (ChatState, std::sync::mpsc::Sender<Event>, std::sync::Arc<std::sync::atomic::AtomicBool>, PathBuf);

    /// A state with a saved chat and a fake backend turn running on it.
    fn running_turn() -> Running {
        use std::sync::{Arc, atomic::AtomicBool};
        let dir = std::env::temp_dir().join(format!("papercraft-chat-turn-{:016x}", fastrand::u64(..)));
        let mut st = ChatState { project: Some(dir.clone()), ..Default::default() };
        st.ensure_chat();
        st.chat.as_mut().unwrap().messages.push(Message { role: Role::User, text: "draw".into(), at: now_ms(), ..Default::default() });
        st.save();
        let id = st.chat.as_ref().unwrap().id.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        let running = Arc::new(AtomicBool::new(true));
        st.turn = Some(TurnHandle::Cli(Box::new(FakeTurn { stopped: Arc::new(AtomicBool::new(false)), running: running.clone() })));
        st.rx = Some(rx);
        st.turn_chat = Some(id.clone());
        st.window = Some(TurnWindow { chat: id, start: now_ms(), edits: None });
        (st, tx, running, dir)
    }

    fn write_html() -> Event {
        Event::ToolStart { id: "w".into(), name: "write_html".into(), args: serde_json::json!({"targetNodeId": "root", "html": "<div/>"}) }
    }

    #[test]
    fn stop_keeps_the_turn_until_the_backend_ends_then_saves_and_closes_the_run() {
        use std::sync::atomic::Ordering;
        let (mut st, tx, running, dir) = running_turn();
        let ctx = egui::Context::default();
        st.stop();
        assert!(st.stopping && st.is_running());
        st.pump(&ctx);
        assert!(st.is_running(), "the backend has not said it is over");
        assert!(st.send(&ctx, "again").is_err(), "no second turn while the first one runs");
        tx.send(Event::Text("partial".into())).unwrap();
        tx.send(write_html()).unwrap();
        tx.send(Event::TurnEnd { usage: None, session: None, stop_reason: Some("stopped".into()) }).unwrap();
        running.store(false, Ordering::SeqCst);
        st.pump(&ctx);
        assert!(!st.is_running() && !st.stopping);
        let saved = store::load(&dir, &st.chat.as_ref().unwrap().id).unwrap();
        assert!(saved.messages.iter().any(|m| m.text == "partial"), "what was streamed before the end is kept");
        assert!(st.take_finish_run(), "a turn that edited closes its agent run, like a normal end");
        assert!(!st.take_finish_run());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_running_chat_cannot_be_deleted_and_events_stay_with_it() {
        use std::sync::atomic::Ordering;
        let (mut st, tx, running, dir) = running_turn();
        let id = st.chat.as_ref().unwrap().id.clone();
        st.remove_chat(&id);
        assert!(store::load(&dir, &id).is_some(), "delete is refused mid-turn");
        // Another chat is open when the events arrive: they still go to the turn's chat.
        st.chat = Some(new_chat());
        let other = st.chat.as_ref().unwrap().id.clone();
        tx.send(Event::Text("for the turn chat".into())).unwrap();
        tx.send(Event::TurnEnd { usage: None, session: None, stop_reason: None }).unwrap();
        running.store(false, Ordering::SeqCst);
        st.pump(&egui::Context::default());
        assert_eq!(st.chat.as_ref().unwrap().id, other);
        assert!(st.chat.as_ref().unwrap().messages.is_empty(), "the open chat is untouched");
        assert!(store::load(&dir, &id).unwrap().messages.iter().any(|m| m.text == "for the turn chat"));
        st.remove_chat(&id);
        assert!(store::load(&dir, &id).is_none(), "deletable once the turn is over");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn only_runs_made_during_the_turn_are_claimed() {
        use crate::runs::Run;
        use std::sync::atomic::Ordering;
        let (mut st, tx, running, dir) = running_turn();
        let now = now_ms();
        let run = |id: &str, started| Run { id: id.into(), started, ..Default::default() };
        tx.send(write_html()).unwrap();
        tx.send(Event::TurnEnd { usage: None, session: None, stop_reason: None }).unwrap();
        running.store(false, Ordering::SeqCst);
        st.note_run(&run("mine", now));
        st.note_run(&run("before", now - 60_000));
        st.note_run(&run("after", now + 60_000));
        st.pump(&egui::Context::default());
        let claimed: Vec<&str> = st.chat.as_ref().unwrap().messages.iter().filter_map(|m| m.run_id.as_deref()).collect();
        assert_eq!(claimed, ["mine"]);
        assert!(st.window.is_some(), "kept until the app has closed the run");
        assert!(st.take_finish_run());
        st.pump(&egui::Context::default());
        assert!(st.window.is_none());
        // With no turn, nothing is claimed.
        st.note_run(&run("late", now));
        assert!(st.unclaimed.is_empty());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_turn_without_edits_claims_nothing() {
        use crate::runs::Run;
        use std::sync::atomic::Ordering;
        let (mut st, tx, running, dir) = running_turn();
        tx.send(Event::TurnEnd { usage: None, session: None, stop_reason: None }).unwrap();
        running.store(false, Ordering::SeqCst);
        st.note_run(&Run { id: "ext".into(), started: now_ms(), ..Default::default() });
        st.pump(&egui::Context::default());
        assert!(st.chat.as_ref().unwrap().messages.iter().all(|m| m.run_id.is_none()));
        assert!(!st.take_finish_run(), "nothing to close");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn context_meter_uses_the_last_call_and_never_exceeds_the_window() {
        let mut st = ChatState { chat: Some(new_chat()), ..Default::default() };
        assert_eq!(st.context_use(), None, "Codex reports no per-call usage: hidden");
        st.apply(Event::Context(120_000));
        st.apply(Event::Context(150_000));
        st.apply(Event::TurnEnd { usage: Some(crate::chat::Usage { input: 365_000, ..Default::default() }), session: None, stop_reason: None });
        // The turn summed 365k over many calls; the meter shows the last call.
        st.live_context = None;
        let c = st.chat.as_mut().unwrap();
        c.messages.push(Message { role: Role::Assistant, context: Some(150_000), ..Default::default() });
        assert_eq!(st.context_use(), Some((150_000, CONTEXT_WINDOW)));
        st.chat.as_mut().unwrap().messages.last_mut().unwrap().context = Some(999_999);
        assert_eq!(st.context_use(), Some((CONTEXT_WINDOW, CONTEXT_WINDOW)));
    }

    #[test]
    fn missing_agent_wording_names_the_custom_path() {
        assert_eq!(not_found(None), "Not found on PATH");
        assert_eq!(not_found(Some("/opt/x/claude")), "Not found at /opt/x/claude");
    }

    #[test]
    fn error_ends_the_turn_and_is_kept() {
        let mut st = ChatState { chat: Some(new_chat()), ..Default::default() };
        assert!(!st.apply(Event::Text("hi".into())));
        assert!(st.apply(Event::Error("boom".into())));
        assert_eq!(st.error.as_deref(), Some("boom"));
        assert_eq!(st.chat.unwrap().messages[0].text, "hi");
    }

    #[test]
    fn text_after_tool_calls_starts_a_new_message() {
        let mut st = ChatState { chat: Some(new_chat()), ..Default::default() };
        st.apply(Event::Text("Plan".into()));
        st.apply(Event::ToolStart { id: "1".into(), name: "create_artboard".into(), args: serde_json::json!({"nodeId": "ab12cd:3"}) });
        st.apply(Event::Text("Done".into()));
        let msgs = &st.chat.as_ref().unwrap().messages;
        assert_eq!(msgs.iter().map(|m| (m.text.as_str(), m.tools.len())).collect::<Vec<_>>(), [("Plan", 1), ("Done", 0)]);
        assert_eq!(st.follow.as_deref(), Some("ab12cd:3"));
    }

    #[test]
    fn thinking_time_is_kept_when_the_turn_ends() {
        let mut st = ChatState { chat: Some(new_chat()), ..Default::default() };
        st.apply(Event::Thinking("hm".into()));
        assert!(st.thought_secs.is_none());
        st.apply(Event::TurnEnd { usage: None, session: None, stop_reason: None });
        assert_eq!(st.thought_secs, Some(0));
        assert_eq!(st.thinking, "hm");
    }

    #[test]
    fn detect_picks_the_default_or_the_first_available() {
        let found = Some((PathBuf::from("/bin/codex"), "1".into()));
        let d = Detect { codex: found, ..Default::default() };
        assert_eq!(d.pick(BackendKind::ClaudeCode), BackendKind::Codex);
        assert_eq!(d.pick(BackendKind::Codex), BackendKind::Codex);
        assert!(!Detect::default().any());
        assert!(ready_detect().has(BackendKind::Anthropic));
        assert!(!Detect { endpoints: vec![EndpointState { id: "x".into(), key: false, ready: false }], ..Default::default() }.has(BackendKind::Anthropic));
    }

    fn ready_detect() -> Detect {
        Detect { endpoints: vec![EndpointState { id: "anthropic".into(), key: true, ready: true }], default_endpoint: "anthropic".into(), ..Default::default() }
    }

    #[test]
    fn endpoint_choice_follows_readiness() {
        let mut d = ready_detect();
        d.endpoints.push(EndpointState { id: "local".into(), key: false, ready: true });
        assert_eq!(d.pick_endpoint(), Some("anthropic"));
        d.endpoints[0].ready = false;
        assert_eq!(d.pick_endpoint(), Some("local"));
        let b = |id: Option<&str>| Backend { kind: BackendKind::Anthropic, endpoint: id.map(Into::into), ..Default::default() };
        assert!(d.has_backend(&b(Some("local"))) && !d.has_backend(&b(Some("anthropic"))) && !d.has_backend(&b(None)), "None means the default");
    }

    #[test]
    fn defaults_follow_availability_and_prefs() {
        let prefs = super::super::settings::ChatPrefs { default_backend: BackendKind::Anthropic, ..Default::default() };
        let mut st = ChatState { chat: Some(new_chat()), detect: Some(ready_detect()), ..Default::default() };
        st.ensure_defaults(&prefs);
        let b = &st.chat.as_ref().unwrap().backend;
        assert_eq!((b.kind, b.model.as_deref(), b.effort.as_deref(), b.endpoint.as_deref()), (BackendKind::Anthropic, Some("claude-opus-5-5"), Some("medium"), Some("anthropic")));
    }

    #[test]
    fn deleting_a_chat_removes_its_saved_images() {
        let dir = std::env::temp_dir().join(format!("papercraft-chat-del-{:016x}", fastrand::u64(..)));
        let mut st = ChatState { project: Some(dir.clone()), ..Default::default() };
        st.ensure_chat();
        st.chat.as_mut().unwrap().messages.push(Message { role: Role::User, text: "x".into(), ..Default::default() });
        st.save();
        let id = st.chat.as_ref().unwrap().id.clone();
        let tmp = dir.join(TMP_DIR).join(&id);
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join("a.png"), b"x").unwrap();
        st.remove_chat(&id);
        assert!(!tmp.exists());
        assert!(store::list(&dir).is_empty());
        assert!(st.chat.as_ref().is_some_and(|c| c.id != id), "a fresh chat opens");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn canvas_follows_a_node_unless_the_user_moved_the_camera() {
        use crate::geom::{Rect, V2, v2};
        use crate::model::{Node, NodeId, NodeKind};
        let mut ed = Editor::default();
        ed.viewport = v2(800.0, 600.0);
        let id = ed.add_node(Node::new(NodeId(0), NodeKind::Rect, Rect::new(5000.0, 5000.0, 100.0, 100.0)), None);
        let key = crate::mcp::exec::node_key(&ed, 0, id);
        let mut c = ChatState::default();
        c.follow = Some(key.clone());
        follow(&mut ed, &mut c);
        let v = ed.camera.visible_world(ed.viewport);
        assert!(v.x < 5050.0 && 5050.0 < v.x + v.w && v.y < 5050.0 && 5050.0 < v.y + v.h, "panned to the node");
        // The user moves the camera away; the next touch is not followed for 10 s.
        ed.camera.pan = V2::ZERO;
        c.follow = Some(key);
        follow(&mut ed, &mut c);
        assert_eq!(ed.camera.pan, V2::ZERO);
    }
}
