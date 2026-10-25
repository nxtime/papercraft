//! Chat mode: the chat column on the left, the live canvas on the right: state, event pump,
//! `send`, header, agent picker, composer and the setup state. The messages and cards are drawn
//! by `chat_msgs`.
//!
//! The mode and the column width are per machine (`chat_ui.json` in the config folder, next to the
//! theme choice), not in the project folder, so they do not sync to other people.

use super::chat_msgs::{self, Acts, View};
#[path = "chat_agents.rs"]
pub mod chat_agents;
#[path = "chat_pick.rs"]
pub mod chat_pick;
pub use chat_pick::{layer_chip, open_ask};
use chat_agents::{Act, PanelIn};
use crate::chat::events::{Event, ImageAttachment};
use crate::chat::agents::{self, AgentKind, AgentState, AgentStatus};
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
const CONTEXT_WINDOW: u64 = 1_000_000;
/// Past this a CLI chat's next message starts a fresh agent session with a recap: every step
/// re-reads the whole context, and answers get worse well before the window is full.
const FRESH_SESSION_AT: u64 = 120_000;
/// The recap a fresh session starts with: this many recent requests, each and the last report cut.
const RECAP_ASKS: usize = 8;
const RECAP_ASK_CHARS: usize = 300;
const RECAP_REPORT_CHARS: usize = 1500;
/// The meter turns yellow above this share of the window.
const CONTEXT_WARN: f32 = 0.75;
const COMPOSER_MAX_H: f32 = 120.0;
/// Added to the chat rules when an image provider is on (Settings, Chat, Images).
#[cfg(not(target_arch = "wasm32"))]
const ART_RULE: &str = "\n\nArt: image generation is on. Never leave an icon on a glow where art belongs (card art, a hero, thumbnails, backgrounds): call generate_image once with up to four prompts written as art direction in the agreed palette, then place each with <img src=\"path\">. It takes a minute or more, so batch what a screen needs.";
/// First line of a message sent with web research on (a `chat_msgs::CONTEXT_TAGS` line).
#[cfg(not(target_arch = "wasm32"))]
const RESEARCH_LINE: &str = "[Research] Web research is on";
/// How the in-app chat differs from an agent in a terminal: the user is right here, and every
/// token is theirs (specs/chat-tokens).
#[cfg(not(target_arch = "wasm32"))]
/// Added when the design-playbook CLI is installed: the agent reaches it through `design_rules`.
const PLAYBOOK_RULE: &str = "\n\nDesign playbook: the design-playbook rules are available through design_rules. Add a seventh question to the brief: follow the playbook (recommended) or not. If yes, before building look up the rules for what you design (for example the screen's patterns, forms, states, accessibility, motion) with one or two design_rules calls, follow them, and cite the rule ids you applied in the finish report. Thresholds (contrast, target sizes, durations) come from the playbook, not memory.";

/// Added in auto mode: nobody is there to answer, the first option of every question is taken.
const AUTO_RULE: &str = "\n\nAuto mode is on: the user is away. Every ask_user call is answered at once with the first option of each question, so always put the option you recommend first, and never ask for what only the user can know (brand assets, real data, credentials): choose a sensible default and say which. End every turn with ask_user next steps, so the work goes on.";

/// Added when the user set up a helper agent (Settings, Chat, Helper): the chat agent orchestrates.
const HELPER_RULE: &str = "\n\nHelper. A helper agent on a cheaper model is available through delegate; you orchestrate. Keep the brief, design decisions and anything that needs taste. Hand it: qa after each built screen; read when the work needs many pages, frames, specs or references read (it returns key points; do not reread what it read); build for repetitive or fully specified edits (the same row in ten places, filling a table, applying given markup), with the variables, components and exact structure written into the task. It does not see this chat, so each task is self-contained. Check a build with one narrow read or a qa pass, not by redoing it.";

/// A tool call being written by the model, from its first streamed byte until it ends.
#[derive(Debug, Clone)]
pub struct Drafting {
    pub id: String,
    pub name: String,
    /// The start of its input, enough to find the target.
    json: String,
    /// The node it writes into or changes (`page:id`), once its input names one.
    pub target: Option<String>,
    pub since: web_time::Instant,
}

/// Input kept from a drafted call: the target is named in its first few keys.
const DRAFT_JSON_MAX: usize = 2048;

/// The node a partly written tool input names (`targetNodeId`, `parentId` or `nodeId`).
fn draft_target(json: &str) -> Option<String> {
    ["targetNodeId", "parentId", "nodeId"].iter().find_map(|k| {
        let rest = &json[json.find(&format!("\"{k}\""))? + k.len() + 2..];
        let rest = rest.trim_start().strip_prefix(':')?.trim_start().strip_prefix('"')?;
        let v = &rest[..rest.find('"')?];
        v.contains(':').then(|| v.to_owned())
    })
}

/// Characters of an added message shown in the composer's queue.
const ADDED_PREVIEW: usize = 80;

const CHAT_RULES: &str = "In-app chat: the user is watching and can answer at once.

Brief. Before the first design in a project or on an empty page, ask first, even when the request looks specific, unless the user said to go ahead:
* Call ask_user once, up to six questions, your recommended option first: platform and frame size (desktop 1440x900, laptop 1280x800, 1440p 2560x1440, tablet 768x1024, phone 390x844); density (compact, 32px controls for a desktop pointer UI; comfortable, 40px; touch, 44px and up); palette (two or three palettes that suit the request, with their hex colors, plus \"my brand colors\"); type (a pairing that suits it, or Inter only); scope (one screen to judge the look, or a flow); code library (shadcn/ui, Ant Design, React Bits on top of either, egui for a Rust app, or none).
* In the same message invite references: they can paste or drop screenshots and images in the chat. Then end your turn. Never restate a card in text: one short sentence at most next to it.

Build.
* A code library chosen: call apply_ui_kit with it first and build screens from its components (<x-paper-instance>), restyling its variables to the agreed palette, so the code handoff writes the library's components. A component of your own that maps to a library one: set_code_link it.
* Variables first: colors (light and dark modes) with set_variables, spacing, radius and type sizes with set_tokens, then write every style with var(--name), no literal colors or off-scale sizes. For more than one screen put them and the components on a Design System page first.
* Anything that repeats (a card, a row, a button, a slot) is a component, built once and placed as instances, with variants for its states. Once a component exists, place it inside write_html with <x-paper-instance component=\"Name\" overrides='{\"Label\":\"Text\"}'/> instead of writing its markup again: rewritten markup is most of what a turn costs.
* Sizes follow the agreed density. WCAG 2.2 AA asks 24px targets for a pointer, so compact desktop controls are fine; touch screens need 44px.
* Few, large steps: one write_html per region or per set of components, not one per element. Read narrowly: get_tree_summary or find_nodes on the frame you change, not the page; get_jsx only for what you will copy. One screenshot per screen you changed, at the end, after one lint: every image stays in the context and is re-read on each later step.
* Review. When a qa agent (Agent tool) or delegate with role qa is available, hand the end-of-turn lint and screenshot check to it instead (delegate first when on: it is cheaper), naming the frames and the brief; fix what it reports with narrow reads, and screenshot yourself only what a finding leaves unclear.

Finish. Report in at most 80 words, flat bullets: what you built, what is a component, what the user should confirm. Then call ask_user with two to four next steps (for example: turn the repeats into components, add the light mode, a mobile layout, the next screen).

Research. When a message carries the [Research] line, web search is on. If a researcher agent is available, hand all reference research to it (one Agent call with what the design is and the brief): it searches and fetches on a cheaper model and returns only the kept images; then skip to asking which to follow. Otherwise, after the brief is answered (or when asked), find three to six references that show an interface, not marketing. Sources that work: nicelydone.club/apps/<app> (real screenshots of 500+ SaaS web apps: linear, notion, slack, jira...), banani.co/references/apps and screenlane.com (mobile screens), App Store and Google Play listings (app screenshots), godly.website and awwwards.com/websites (marketing sites). Dribbble, Behance, Figma Community, Mobbin, Land-book and Lapa block automated fetching; a homepage's image is marketing art. Bring them in with one fetch_reference_images call, then look at what came back: keep only images that show an interface like the one you are designing (a screen, its layout or its components). A social card, a logo, an app icon or an illustration is not a reference. If fewer than two qualify, search again once with better sources (the product name plus \"screenshot\", its App Store listing, a help-center article) before building. Say in one line each what you take from a kept one (layout, density, color, type), then ask_user which to follow with only the kept ones as options, each with its `image` path, even when the user took your defaults. Take ideas, never a brand's logo or artwork.

* When PaperCraft itself gets in the way (a tool misbehaves, its docs disagree with what it does, or you had to work around a missing capability), call report_issue once per problem with a minimal synthetic repro, never the user's content, then go on. The user decides whether it is sent.
* Less common tools (annotations, pages, files, export, libraries, design-system and component edits) may not be loaded yet: find them with tool search.
* Attached images are scaled down to send. To read small text or detail in one, call zoom_image with its attached path and a region instead of guessing.";
const SELF_FOLLOW_QUIET: Duration = Duration::from_secs(10);
pub use crate::chat::TMP_DIR;
/// A run that started this long before the turn's first edit call or after its last still counts as its own
/// (events reach the UI a frame after the tool ran).
const RUN_SLACK_MS: u64 = 5_000;
const REPAINT: Duration = Duration::from_millis(50);
/// How often the column checks the design system for changes (the summary's key).
const DS_CHECK: Duration = Duration::from_secs(1);
#[cfg(not(target_arch = "wasm32"))]
const TITLE_CHARS: usize = 40;
const MAX_IMAGES: usize = 4;
const NEW_CHAT: &str = "New chat";
const WEB_NOTE: &str = "Chat needs the desktop app";

/// A running turn, whichever backend started it.
pub enum TurnHandle {
    Cli(Box<dyn crate::chat::events::Turn>),
    #[cfg(not(target_arch = "wasm32"))]
    Acp(crate::chat::acp::AcpTurn),
    #[cfg(not(target_arch = "wasm32"))]
    Api(crate::chat::tools::ApiTurn),
}

impl TurnHandle {
    pub fn stop(&mut self) {
        match self {
            TurnHandle::Cli(t) => t.stop(),
            #[cfg(not(target_arch = "wasm32"))]
            TurnHandle::Acp(t) => crate::chat::events::Turn::stop(t),
            #[cfg(not(target_arch = "wasm32"))]
            TurnHandle::Api(t) => crate::chat::events::Turn::stop(t),
        }
    }

    pub fn is_running(&self) -> bool {
        match self {
            TurnHandle::Cli(t) => t.is_running(),
            #[cfg(not(target_arch = "wasm32"))]
            TurnHandle::Acp(t) => crate::chat::events::Turn::is_running(t),
            #[cfg(not(target_arch = "wasm32"))]
            TurnHandle::Api(t) => crate::chat::events::Turn::is_running(t),
        }
    }

    /// Answer a `NeedsApproval` event (API and ACP backends).
    pub fn approve(&self, id: &str, ok: bool) {
        #[cfg(not(target_arch = "wasm32"))]
        match self {
            TurnHandle::Api(t) => t.approve(id, ok),
            TurnHandle::Acp(t) => t.approve(id, ok),
            TurnHandle::Cli(_) => {}
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
    /// Codex is signed in with a ChatGPT plan (`codex login status`), so it can make images.
    pub codex_plan: bool,
    pub endpoints: Vec<EndpointState>,
    pub default_endpoint: String,
    /// Every registry and custom agent (`agents::detect_all`).
    pub agents: Vec<AgentStatus>,
}

impl Detect {
    pub fn has(&self, kind: BackendKind) -> bool {
        match kind {
            BackendKind::ClaudeCode => self.claude.is_some(),
            BackendKind::Codex => self.codex.is_some(),
            BackendKind::Anthropic => self.endpoints.iter().any(|e| e.ready),
            BackendKind::Acp => self.first_acp().is_some(),
        }
    }

    /// The state of one agent. Claude Code and Codex use their own detection, which is what
    /// `has` and the turn use too.
    pub fn agent_state(&self, id: &str) -> AgentState {
        let direct = |f: &Option<Found>| f.as_ref().map_or(AgentState::NotFound, |(p, v)| AgentState::Ready { path: p.display().to_string(), version: v.clone() });
        match id {
            "claude-code" => direct(&self.claude),
            "codex" => direct(&self.codex),
            _ => self.agents.iter().find(|s| s.id == id).map_or(AgentState::NotFound, |s| s.state.clone()),
        }
    }

    pub fn agent_ready(&self, id: &str) -> bool {
        matches!(self.agent_state(id), AgentState::Ready { .. })
    }

    /// The first ready agent that runs over ACP.
    pub fn first_acp(&self) -> Option<&str> {
        let acp = |s: &&AgentStatus| matches!(s.state, AgentState::Ready { .. }) && agents::find(&s.id).is_none_or(|d| d.kind != AgentKind::Direct);
        self.agents.iter().find(acp).map(|s| s.id.as_str())
    }

    /// The endpoint with this id (None = the default one) can run.
    pub fn endpoint_ready(&self, id: Option<&str>) -> bool {
        let id = id.unwrap_or(&self.default_endpoint);
        self.endpoints.iter().any(|e| e.id == id && e.ready)
    }

    pub fn has_backend(&self, b: &Backend) -> bool {
        match b.kind {
            BackendKind::Anthropic => self.endpoint_ready(b.endpoint.as_deref()),
            BackendKind::Acp => b.agent.as_deref().is_some_and(|id| self.agent_ready(id)),
            k => self.has(k),
        }
    }

    /// The default endpoint when it can run, else the first that can.
    pub fn pick_endpoint(&self) -> Option<&str> {
        let ready = |e: &&EndpointState| e.ready;
        self.endpoints.iter().filter(ready).find(|e| e.id == self.default_endpoint).or_else(|| self.endpoints.iter().find(ready)).map(|e| e.id.as_str())
    }

    pub fn any(&self) -> bool {
        [BackendKind::ClaudeCode, BackendKind::Codex, BackendKind::Anthropic, BackendKind::Acp].iter().any(|k| self.has(*k))
    }

    /// The default backend when available, else the first one that is.
    pub fn pick(&self, preferred: BackendKind) -> BackendKind {
        [preferred, BackendKind::ClaudeCode, BackendKind::Codex, BackendKind::Anthropic, BackendKind::Acp]
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
    let codex = codex::detect(cli_path(prefs, "codex").map(Path::new));
    // `codex login status` prints "Logged in using ChatGPT" (to stderr in 0.162).
    let codex_plan = codex.as_ref().is_some_and(|(bin, _)| {
        std::process::Command::new(bin).args(["login", "status"]).env_remove("OPENAI_API_KEY").env_remove("CODEX_API_KEY").output().is_ok_and(|o| [&o.stdout, &o.stderr].iter().any(|b| String::from_utf8_lossy(b).contains("ChatGPT")))
    });
    Detect {
        claude: claude_code::detect(cli_path(prefs, "claude-code").map(Path::new)),
        codex,
        codex_plan,
        endpoints,
        default_endpoint: prefs.default_endpoint.clone(),
        agents: agents::detect_all(prefs),
    }
}

/// The current selection as a composer chip.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SelChip {
    pub label: String,
    pub keys: Vec<String>,
    /// An area chip (Pick, Shift+drag): its world rect `[x, y, w, h]`; `keys` are the layers inside.
    pub rect: Option<[f64; 4]>,
    /// Name of the page an area chip is on.
    pub page: String,
    /// PNG crop of the layers (or of the area), sent with the message like an attached image.
    pub image: Option<Vec<u8>>,
}

/// An issue for PaperCraft: kind (`bug`, `docs`, `feature`), title and body.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct IssueDraft {
    pub kind: String,
    pub title: String,
    pub body: String,
}

/// Where an issue draft stands.
#[derive(Debug, Clone, PartialEq)]
pub enum IssueState {
    Sending,
    /// Filed as #number; `existing` when an issue with a matching title was there already.
    Filed { number: u64, url: String, existing: bool },
    Failed(String),
    Discarded,
}

/// The key of the form's issue in `ChatState::issues`.
pub const FORM_ISSUE: &str = "form";
const ISSUE_TEMPLATE: &str = "**Expected:** \n**Actual:** \n**Repro:** \n**Workaround:** \n**Impact:** ";

/// An image waiting in the composer.
#[derive(Debug, Clone, PartialEq)]
pub struct Attachment {
    pub name: String,
    pub img: ImageAttachment,
    /// The original when `img` was scaled down, saved next to it for `zoom_image`.
    pub full: Option<Vec<u8>>,
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
    /// Messages added during the running turn (`mcp::steer`), shown until it ends.
    pub added: Vec<String>,
    /// Added messages no tool call picked up: sent as the next message once the turn ends.
    followup: Option<String>,
    /// Stop was pressed; the turn stays occupied until the backend's last event.
    pub stopping: bool,
    /// The last turn ended because the user stopped it: the list says "Stopped", not an error.
    pub stopped: bool,
    window: Option<TurnWindow>,
    /// Runs the tracker finished while a window was open, waiting for `settle_claims`.
    unclaimed: Vec<crate::runs::Run>,
    /// The turn made edits and ended: the app closes the open agent run now.
    finish_run: bool,
    /// Tokens in the context at the latest model call of the running turn.
    live_context: Option<u64>,
    /// The user asked to compact (the meter): the next message starts a fresh CLI session.
    pub fresh_next: bool,
    /// Tokens the running turn has read so far (every model call's context), for the budget.
    turn_read: u64,
    /// The last turn was stopped by the turn budget (`ChatPrefs::turn_budget`).
    pub budget_hit: bool,
    /// Issues being filed or filed, by key (a `report_issue` call's id, or `FORM_ISSUE`).
    pub issues: std::collections::HashMap<String, IssueState>,
    #[cfg(not(target_arch = "wasm32"))]
    issue_rx: Vec<(String, std::sync::mpsc::Receiver<Result<crate::issues::Filed, String>>)>,
    /// The "Report a PaperCraft issue" form, while open.
    pub issue_form: Option<IssueDraft>,
    pub approvals: Vec<Approval>,
    /// Streamed thinking of the last turn (the "Thinking" row; not saved).
    pub thinking: String,
    thinking_since: Option<web_time::Instant>,
    /// Seconds the last turn spent thinking, once it is over ("Thought for 12s").
    pub thought_secs: Option<u64>,
    /// The last turn's error, shown as a row with Retry.
    pub error: Option<String>,
    /// `error` is a failed start (nothing ran): Retry sends again what was typed.
    pub start_failed: bool,
    /// The prompt of the failed start, for Retry when the composer was cleared (suggestion chips).
    #[cfg(not(target_arch = "wasm32"))]
    failed_prompt: Option<String>,
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
    /// The agent panel is open.
    pub picker_open: bool,
    panel: chat_agents::PanelState,
    /// Composer text, selection chip and images.
    pub draft: String,
    /// One chip per `@ Selection` click, oldest first.
    pub sels: Vec<SelChip>,
    /// Annotation lines of the chips' layers, refreshed from the editor just before a send.
    pub sel_notes: Vec<String>,
    pub images: Vec<Attachment>,
    /// An Actions-menu prompt is in the composer: the column sends it once the agents are known.
    pub send_draft: bool,
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
    pub follow: Option<String>,
    /// The tool call the model is writing now: the canvas shows a skeleton where it lands.
    pub drafting: Option<Drafting>,
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
    /// The design-system summary, rebuilt when the design system changes (`refresh_ds`).
    pub ds: crate::chat::ds_digest::Cached,
    ds_checked: Option<web_time::Instant>,
    /// CLI/ACP chats: the summary key their agent session last got, so a change is sent once.
    ds_sent: HashMap<String, u64>,
    /// Offscreen renderer for chip crops; set by the app each frame (None in tests and the web).
    pub gpu: Option<crate::render::gpu::Headless>,
    /// Pick mode: the next canvas click (or Shift+drag) becomes a chip.
    pub picking: bool,
    /// The on-canvas Ask box (⌘K on a selection).
    pub ask: Option<chat_pick::Ask>,
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
        return crate::project::config_dir().map(|d| d.join("chat_ui.json"));
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
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(chat) = &self.turn_chat {
            crate::mcp::delegate::stop(chat);
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
                    self.follow = Some(n.clone());
                    if let Some(d) = self.drafting.as_mut().filter(|d| d.id == id) {
                        d.target = Some(n);
                    }
                }
                let m = last_assistant(chat, now);
                match m.tools.iter_mut().find(|t| t.id == id) {
                    Some(t) => t.args = args,
                    None => m.tools.push(ToolCall { id, name, args, ..Default::default() }),
                }
            }
            Event::ToolDraft { id, name } => {
                self.drafting = Some(Drafting { id, name, json: String::new(), target: None, since: web_time::Instant::now() });
            }
            Event::ToolInput(piece) => {
                if let Some(d) = self.drafting.as_mut().filter(|d| d.target.is_none() && d.json.len() < DRAFT_JSON_MAX) {
                    d.json.push_str(&piece);
                    d.target = draft_target(&d.json);
                    if let Some(t) = &d.target {
                        self.follow = Some(t.clone());
                    }
                }
            }
            Event::ToolEnd { id, result, error } => {
                if self.drafting.as_ref().is_some_and(|d| d.id == id) {
                    self.drafting = None;
                }
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
            Event::Context(n) => {
                self.live_context = Some(n);
                self.turn_read += n;
            }
            Event::TurnEnd { usage, session, .. } => {
                self.thought_secs = self.thinking_since.take().map(|t| t.elapsed().as_secs());
                // Claude Code reports the session's running cost; a resumed session's turn costs
                // the difference from the last total.
                let cli = chat.backend.kind == BackendKind::ClaudeCode;
                let resumed = cli && session.is_some() && session == chat.backend.cli_session;
                let before = chat.messages.iter().rev().find_map(|m| m.session_cost).filter(|_| resumed);
                if let Some(m) = chat.messages.last_mut().filter(|m| m.role == Role::Assistant) {
                    let total = usage.as_ref().and_then(|u| u.cost_usd).filter(|_| cli);
                    m.usage = usage.map(|mut u| {
                        if let (Some(t), Some(b)) = (total, before) {
                            u.cost_usd = Some((t - b).max(0.0));
                        }
                        u
                    });
                    m.session_cost = total;
                    m.context = self.live_context;
                }
                if let Some(s) = session.filter(|_| chat.backend.kind != BackendKind::Anthropic) {
                    chat.backend.cli_session = Some(s);
                }
                return true;
            }
            Event::Error(e) => {
                self.thought_secs = self.thinking_since.take().map(|t| t.elapsed().as_secs());
                // A backend killed by Stop reports how it died; that is the stop, not a failure.
                if !self.stopping {
                    self.error = Some(e);
                }
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
        // A turn that keeps going past the budget is stopped, like the Stop button; what it made
        // so far stays.
        let budget = self.prefs.turn_budget;
        if !done && budget > 0 && self.turn_read > budget && !self.stopping {
            self.budget_hit = true;
            self.stop();
        }
        if done {
            let turn = self.turn_chat.clone();
            self.finish_turn();
            // Auto mode: questions the turn ended on get the recommended answers, and it goes on.
            if self.followup.is_none()
                && self.prefs.auto
                && !self.stopped
                && !self.budget_hit
                && let Some(chat) = self.chat.as_ref().filter(|c| turn.as_deref() == Some(c.id.as_str()))
            {
                self.followup = chat_msgs::auto_answers(&chat.messages);
            }
            if let Some(f) = self.followup.take() {
                let _ = self.send(ctx, &f);
            }
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

    /// Add the draft to the running turn: the agent reads it with its next PaperCraft tool result.
    pub fn add_to_turn(&mut self) {
        let text = self.draft.trim().to_owned();
        let Some(chat) = self.turn_chat.as_deref().filter(|_| !text.is_empty()) else { return };
        crate::mcp::steer::add(chat, &text);
        self.added.push(text);
        self.draft.clear();
    }

    fn finish_turn(&mut self) {
        // What the agent never picked up: sent next, or back in the box after a Stop.
        let left = self.turn_chat.as_deref().map(crate::mcp::steer::take).unwrap_or_default().join("\n\n");
        if !left.is_empty() {
            if self.stopping {
                self.draft = if self.draft.is_empty() { left } else { format!("{left}\n\n{}", self.draft) };
            } else {
                self.followup = Some(left);
            }
        }
        self.added.clear();
        self.drafting = None;
        self.turn = None;
        self.rx = None;
        self.stopped = std::mem::take(&mut self.stopping);
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
        let art = if self.prefs.image_provider() == crate::mcp::imagegen::Provider::Off { "" } else { ART_RULE };
        let playbook = if crate::chat::path::which("playbook").is_some() { PLAYBOOK_RULE } else { "" };
        let helper = if self.prefs.helper_command.trim().is_empty() { "" } else { HELPER_RULE };
        let auto = if self.prefs.auto { AUTO_RULE } else { "" };
        // Plugins the user turned on, one usage line each.
        let on: Vec<&str> = crate::plugins::PLUGINS.iter().filter(|p| self.prefs.plugins.iter().any(|id| id == p.id)).map(|p| p.hint).collect();
        let plugins = if on.is_empty() { String::new() } else { format!("\n\nPlugins on, placed inside write_html (prefer them to drawn placeholders: avatars for people, real icons):\n- {}", on.join("\n- ")) };
        format!(
            "{skill}\n\n{CHAT_RULES}{art}{playbook}{helper}{auto}{plugins}\n\nProject context: the open project is \"{}\" (folder {dir}). Read and edit it only through the papercraft tools.",
            self.project_name
        )
    }

    /// The summary to send with turns: on in Settings and not empty.
    fn ds_text(&self) -> &str {
        if self.prefs.ds_summary { &self.ds.text } else { "" }
    }

    /// Rebuilds the design-system summary when the project's design system changed. `force`
    /// skips the once-a-second throttle (a send).
    pub fn refresh_ds(&mut self, ed: &Editor, force: bool) {
        if !force && self.ds_checked.is_some_and(|t| t.elapsed() < DS_CHECK) {
            return;
        }
        self.ds_checked = Some(web_time::Instant::now());
        let dir = self.project.clone();
        self.ds.refresh(ed, || {
            dir.as_deref()
                .and_then(|d| Some(crate::libraries::status(&crate::project::Store::open(d.parent()?).ok()?, d, ed)))
                .unwrap_or_default()
        });
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
        let mut system = self.system_prompt();
        let digest = self.ds_text().to_owned();
        let compact = std::mem::take(&mut self.fresh_next);
        let (Some(dir), Some(chat)) = (self.project.clone(), self.chat.as_mut()) else {
            return Err("Open a project to chat".into());
        };
        // A CLI session that grew long is not resumed: a fresh one with a recap re-reads far less on
        // every step, and the canvas holds the work.
        let long = chat.messages.iter().rev().find_map(|m| m.context).is_some_and(|c| c > FRESH_SESSION_AT);
        let recap = (chat.backend.kind != BackendKind::Anthropic && chat.backend.cli_session.is_some() && (long || compact)).then(|| recap(&chat.messages));
        if recap.is_some() {
            chat.backend.cli_session = None;
        }
        chat.messages.push(Message { role: Role::User, text: prompt.into(), at: now_ms(), ..Default::default() });
        if chat.title == NEW_CHAT {
            let text = chat_msgs::split_context(prompt).1;
            chat.title = text.lines().next().unwrap_or_default().chars().take(TITLE_CHARS).collect();
        }
        let turn_chat = chat.id.clone();
        let Backend { kind, model, effort, endpoint, cli_session, agent } = chat.backend.clone();
        // API: the summary is its own system block, current every request. CLI/ACP read the system
        // prompt when a session starts; a resumed one hears of a change in front of the prompt.
        let api = kind == BackendKind::Anthropic;
        let ds_stale = !api && !digest.is_empty() && cli_session.is_some() && self.ds_sent.get(&turn_chat) != Some(&self.ds.key);
        let sent = if ds_stale { format!("[Design system changed] {digest}\n\n{prompt}") } else { prompt.to_owned() };
        let sent = match &recap {
            Some(r) => format!("{r}\n\n{sent}"),
            None => sent,
        };
        if !api && !digest.is_empty() {
            system = format!("{system}\n\n{digest}");
        }
        let ep = self.prefs.endpoint(endpoint.as_deref()).clone();
        // Another host's model is whatever the endpoint says now; the Claude host keeps the chat's pick.
        // CLI agents get only their own pick, never an API endpoint's model.
        let (model, effort) = match kind {
            BackendKind::Anthropic if ep.is_claude_host() => (model.or_else(|| Some(ep.model.clone())), effort),
            BackendKind::Anthropic => (Some(ep.model.clone()), None),
            // Lean sessions skip the user's Claude Code settings: keep the model they picked there.
            BackendKind::ClaudeCode if self.prefs.cli_lean => (model.or_else(claude_code::user_model), effort),
            _ => (model, effort),
        };
        let cfg = TurnConfig {
            endpoint: ep.clone(),
            api_history: Some(dir.join("chats").join(format!("{}.api.json", chat.id))),
            project_dir: dir,
            // Calls from a CLI backend name the chat, so they land in this chat's run.
            mcp_url: format!("{}?chat={turn_chat}", self.mcp_url),
            model,
            effort,
            session: cli_session,
            tools: self.tools.clone().map(|d| d.for_chat(&turn_chat)),
            images: std::mem::take(&mut self.pending_images),
            system,
            ds_digest: if api { digest.clone() } else { String::new() },
            binary: cli_binary(&self.prefs, kind),
            lean: self.prefs.cli_lean,
            research: self.prefs.research,
            ..Default::default()
        };
        let (tx, rx) = std::sync::mpsc::channel();
        let started = match kind {
            BackendKind::ClaudeCode => claude_code::start(&cfg, &sent, tx).map(TurnHandle::Cli),
            BackendKind::Codex => codex::start(&cfg, &sent, tx).map(TurnHandle::Cli),
            BackendKind::Anthropic => match ep.format {
                ApiFormat::Anthropic => anthropic::start(&cfg, &sent, tx),
                ApiFormat::OpenAi => openai::start(&cfg, &sent, tx),
            }
            .map(TurnHandle::Api),
            BackendKind::Acp => start_acp(&cfg, &self.prefs, agent.as_deref(), &sent, tx),
        };
        (self.error, self.start_failed, self.failed_prompt, self.stopped) = (None, false, None, false);
        match started {
            Ok(t) => {
                self.thinking.clear();
                (self.thinking_since, self.thought_secs) = (None, None);
                self.save();
                self.stopping = false;
                self.live_context = None;
                (self.turn_read, self.budget_hit) = (0, false);
                self.window = Some(TurnWindow { chat: turn_chat.clone(), start: now_ms(), edits: None });
                if !digest.is_empty() {
                    self.ds_sent.insert(turn_chat.clone(), self.ds.key);
                }
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
                (self.error, self.start_failed, self.failed_prompt) = (Some(e.clone()), true, Some(prompt.to_owned()));
                Err(e)
            }
        }
    }
}

impl ChatState {
    /// Files an issue on a worker thread; `issues[key]` follows it.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn file_issue(&mut self, key: String, d: IssueDraft, repo: String) {
        self.issues.insert(key.clone(), IssueState::Sending);
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let draft = crate::issues::Draft { kind: d.kind, title: d.title, body: d.body };
            let _ = tx.send(crate::issues::file(&repo, &draft));
        });
        self.issue_rx.push((key, rx));
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn poll_issues(&mut self, ctx: &egui::Context) {
        use crate::issues::Filed;
        let issues = &mut self.issues;
        self.issue_rx.retain(|(key, rx)| match rx.try_recv() {
            Ok(r) => {
                issues.insert(key.clone(), match r {
                    Ok(Filed::New { number, url }) => IssueState::Filed { number, url, existing: false },
                    Ok(Filed::Duplicate { number, url, .. }) => IssueState::Filed { number, url, existing: true },
                    Err(e) => IssueState::Failed(e),
                });
                false
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => true,
            Err(_) => false,
        });
        if !self.issue_rx.is_empty() {
            ctx.request_repaint_after(REPAINT);
        }
    }

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
            agent: (kind == BackendKind::Acp).then(|| d.first_acp().map(str::to_owned)).flatten(),
        };
    }

    /// Signed-out agent: the error is its authentication error.
    pub fn signin(&self) -> Option<chat_agents::SignIn> {
        let e = self.error.as_deref()?;
        let b = &self.chat.as_ref()?.backend;
        let id = b.agent.as_deref().filter(|_| b.kind == BackendKind::Acp)?;
        chat_agents::signin_for(id, &agent_name(id, &self.prefs), e)
    }

    /// Gives the open chat the backend of a panel row or a model change; another target starts a
    /// new agent session.
    fn set_backend(&mut self, b: Backend) {
        let running = self.is_running();
        let Some(c) = self.chat.as_mut().filter(|_| !running) else { return };
        let keep = chat_agents::same_target(&c.backend, &b);
        c.backend = Backend { cli_session: if keep { c.backend.cli_session.clone() } else { None }, ..b };
        if !c.messages.is_empty() {
            self.save();
        }
    }

    /// What the user did in the panel or the setup card.
    fn act(&mut self, ctx: &egui::Context, act: Act, settings: &mut super::settings::SettingsUi) {
        match act {
            Act::Select(pick) => {
                self.set_backend(chat_agents::backend_for(&pick, &settings.chat));
                self.picker_open = self.is_running();
            }
            Act::SetBackend(b) => self.set_backend(b),
            Act::Enable(id) => {
                settings.chat.agents.entry(id).or_default().enabled = true;
                settings.chat.save();
                self.rescan(ctx, &settings.chat);
            }
            Act::Settings => {
                settings.open_chat();
                self.picker_open = false;
            }
            Act::DetectAgain => self.rescan(ctx, &settings.chat),
            Act::OpenPanel => (self.picker_open, self.panel.fresh) = (true, true),
            Act::Close => self.picker_open = false,
        }
    }

    fn reset_view(&mut self) {
        self.thinking.clear();
        (self.thinking_since, self.thought_secs, self.error, self.start_failed) = (None, None, None, false);
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
        self.fresh_next = false;
        self.chat = Some(new_chat());
        self.reset_view();
    }

    pub fn switch_to(&mut self, id: &str) {
        if self.turn.is_some() {
            return;
        }
        self.fresh_next = false;
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
            let (name, img, full) = crate::chat::shrink_image(name, ImageAttachment { mime: mime.into(), data });
            self.images.push(Attachment { name, img, full });
        }
    }

    /// Sends the composer: context lines (selection, images), then the text. API images go as
    /// blocks; CLI images are saved under the project and referenced by path. Any failure is
    /// kept in `error` (an error row) and leaves the composer as it was.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn submit(&mut self, ctx: &egui::Context) -> Result<(), String> {
        let r = self.submit_inner(ctx);
        if let Err(e) = &r {
            (self.error, self.start_failed) = (Some(e.clone()), true);
        }
        r
    }

    /// Retry on a failed start: what is typed, else the prompt that failed.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn retry_start(&mut self, ctx: &egui::Context) {
        if !self.draft.trim().is_empty() {
            let _ = self.submit(ctx);
        } else if let Some(p) = self.failed_prompt.clone() {
            let _ = self.send(ctx, &p);
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn submit_inner(&mut self, ctx: &egui::Context) -> Result<(), String> {
        let text = self.draft.trim().to_owned();
        if text.is_empty() && self.images.is_empty() {
            return Ok(());
        }
        let (sels, notes, images) = (self.sels.clone(), self.sel_notes.clone(), self.images.clone());
        let r = self.send_with(ctx, &sels, &notes, &images, &text);
        if r.is_ok() {
            (self.sels, self.images) = (Vec::new(), Vec::new());
            self.draft.clear();
        }
        r
    }

    /// Sends `text` with the chips (context lines and crops) and the images. The composer and the
    /// Ask box both send through here, so they give the same message.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn send_with(&mut self, ctx: &egui::Context, chips: &[SelChip], notes: &[String], images: &[Attachment], text: &str) -> Result<(), String> {
        let prompt = self.compose(chips, notes, images, text)?;
        let r = self.send(ctx, &prompt);
        self.pending_images.clear();
        r
    }

    /// The message: the chips' context lines, the images, then the text. API images (crops too)
    /// go to `pending_images` as blocks; CLI images are saved under the project and named by path.
    #[cfg(not(target_arch = "wasm32"))]
    fn compose(&mut self, chips: &[SelChip], notes: &[String], images: &[Attachment], text: &str) -> Result<String, String> {
        self.ensure_chat();
        let (Some(dir), Some(chat)) = (self.project.clone(), self.chat.as_ref()) else {
            return Err("Open a project to chat".into());
        };
        let (api, chat_id) = (chat.backend.kind == BackendKind::Anthropic, chat.id.clone());
        let mut lines = selection_lines(chips, notes);
        if self.prefs.research {
            lines.insert(0, RESEARCH_LINE.to_owned());
        }
        let mut pending = Vec::new();
        let crops = chips.iter().filter_map(|c| {
            let data = c.image.clone()?;
            let what = if c.rect.is_some() { "area".to_owned() } else { c.keys.first().map_or_else(String::new, |k| k.replace(':', "-")) };
            Some(Attachment { name: format!("crop-{what}.png"), img: ImageAttachment { mime: "image/png".into(), data }, full: None })
        });
        // Every image is saved, so the sent message can show it; API images also go as blocks.
        for a in images.iter().cloned().chain(crops) {
            let folder = store::tmp_of(&dir, &chat_id).ok_or("invalid chat id")?;
            let path = folder.join(format!("{}-{}", now_ms(), a.name));
            crate::vfs::create_dir_all(&folder)
                .and_then(|()| crate::vfs::write(&path, &a.img.data))
                .and_then(|()| a.full.as_ref().map_or(Ok(()), |full| crate::vfs::write(crate::chat::full_image_path(&path), full)))
                .map_err(|e| format!("could not save the image: {e}"))?;
            lines.push(format!("[Attached] {}", path.display()));
            if api {
                pending.push(a.img);
            }
        }
        self.pending_images = pending;
        Ok(if lines.is_empty() { text.to_owned() } else { format!("{}\n\n{text}", lines.join("\n")) })
    }

    #[cfg(target_arch = "wasm32")]
    pub fn send_with(&mut self, _ctx: &egui::Context, _chips: &[SelChip], _notes: &[String], _images: &[Attachment], _text: &str) -> Result<(), String> {
        Err(WEB_NOTE.into())
    }

    #[cfg(target_arch = "wasm32")]
    pub fn submit(&mut self, _ctx: &egui::Context) -> Result<(), String> {
        Err(WEB_NOTE.into())
    }

    #[cfg(target_arch = "wasm32")]
    pub fn retry_start(&mut self, _ctx: &egui::Context) {}

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
    /// Before the first turn it is the design-system summary that the first turn will carry.
    pub fn context_use(&self) -> Option<(u64, u64)> {
        let saved = self.chat.as_ref().and_then(|c| c.messages.iter().rev().find_map(|m| m.context));
        let fresh = self.chat.as_ref().is_some_and(|c| c.messages.is_empty()) && !self.ds_text().is_empty();
        let first = fresh.then(|| self.ds.tokens());
        let used = self.live_context.filter(|_| self.turn.is_some()).or(saved).or(first)?;
        Some((used.min(CONTEXT_WINDOW), CONTEXT_WINDOW))
    }
}

/// Starts the chat's ACP agent: its launch command from the registry (or a custom agent).
#[cfg(not(target_arch = "wasm32"))]
fn start_acp(cfg: &crate::chat::events::TurnConfig, prefs: &super::settings::ChatPrefs, agent: Option<&str>, prompt: &str, tx: std::sync::mpsc::Sender<Event>) -> Result<TurnHandle, String> {
    let id = agent.ok_or("No agent is chosen for this chat. Pick one from the agent chip.")?;
    let name = agent_name(id, prefs);
    let (program, args) = agents::launch_command(id, prefs).ok_or_else(|| format!("{name} is not available. Check it in the agent panel (Not found, or off until enabled)."))?;
    crate::chat::acp::start(cfg, &program, &args, prompt, tx).map(TurnHandle::Acp)
}

/// The name an agent shows under: the registry's, a custom agent's, else its id.
fn agent_name(id: &str, prefs: &super::settings::ChatPrefs) -> String {
    prefs.custom_agents.iter().find(|c| c.id == id).map(|c| c.name.clone()).or_else(|| agents::find(id).map(|a| a.name.to_owned())).unwrap_or_else(|| id.to_owned())
}

/// The path Settings holds for a direct agent: its `agents` entry, else the old field.
#[cfg(not(target_arch = "wasm32"))]
fn cli_path<'a>(prefs: &'a super::settings::ChatPrefs, id: &str) -> Option<&'a str> {
    let old = if id == "codex" { prefs.codex_path.as_deref() } else { prefs.claude_path.as_deref() };
    prefs.agents.get(id).and_then(|a| a.path.as_deref()).or(old)
}

/// The CLI path set in Settings; detection uses it, so a turn must too (None = look on PATH).
#[cfg(not(target_arch = "wasm32"))]
fn cli_binary(prefs: &super::settings::ChatPrefs, kind: BackendKind) -> Option<PathBuf> {
    match kind {
        BackendKind::ClaudeCode => cli_path(prefs, "claude-code"),
        BackendKind::Codex => cli_path(prefs, "codex"),
        BackendKind::Anthropic | BackendKind::Acp => None,
    }
    .filter(|p| !p.trim().is_empty())
    .map(PathBuf::from)
}

fn model_name(id: &str) -> &str {
    super::settings::API_MODELS.iter().find(|m| m.0 == id).map_or(id, |m| m.1)
}

/// Once, when Codex is signed in with a ChatGPT plan and no image provider is set: offer to use
/// it for images. Either answer is kept; Settings, Chat, Images changes it later.
fn image_offer(ui: &mut Ui, st: &mut UiState) {
    let prefs = &st.settings.chat;
    let codex_plan = st.chat.detect.as_ref().is_some_and(|d| d.codex_plan);
    if !codex_plan || !prefs.image_gen.is_empty() || prefs.image_offer_seen {
        return;
    }
    let p = theme::pal(ui);
    let mut answer = None;
    egui::Frame::new().fill(p.popover).corner_radius(theme::RADIUS_PANEL).inner_margin(egui::Margin::symmetric(12, 10)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = 6.0;
        ui.label(RichText::new("Generate images with Codex?").font(theme::semibold(theme::BODY)));
        ui.add(egui::Label::new(RichText::new("Codex is signed in with your ChatGPT plan. The agent could use it to make card art, heroes and reference images instead of placeholders. Each batch takes a minute or two of your Codex usage.").size(theme::SECONDARY).color(p.secondary)).wrap());
        ui.horizontal(|ui| {
            if kit::button(ui, kit::ButtonKind::Primary, None, "Turn on").clicked() {
                answer = Some(true);
            }
            if kit::chip(ui, "Not now", false).clicked() {
                answer = Some(false);
            }
        });
    });
    if let Some(on) = answer {
        let prefs = &mut st.settings.chat;
        prefs.image_offer_seen = true;
        if on {
            prefs.image_gen = "codex".into();
        }
        prefs.save();
    }
}

/// The "Report a PaperCraft issue" form: kind, title, description, then Send (filed on the
/// repository in Settings, Updates) and the link to what was filed.
#[cfg(not(target_arch = "wasm32"))]
fn issue_form(ctx: &egui::Context, st: &mut UiState) {
    let Some(draft) = st.chat.issue_form.as_mut() else { return };
    let mut close = false;
    let mut send = false;
    let state = st.chat.issues.get(FORM_ISSUE).cloned();
    egui::Modal::new(egui::Id::new("issue-form")).show(ctx, |ui| {
        ui.set_width(ISSUE_FORM_W);
        ui.spacing_mut().item_spacing.y = 8.0;
        ui.label(RichText::new("Report a PaperCraft issue").font(theme::semibold(theme::BODY + 3.0)));
        ui.label(RichText::new(format!("Goes to {}. Leave out private content.", st.settings.chat.issue_repo)).size(theme::SECONDARY).color(theme::pal(ui).secondary));
        let kinds = [("bug", "Bug"), ("docs", "Docs"), ("feature", "Feature")];
        if let Some(k) = kit::segmented(ui, &kinds.map(|(_, l)| (l, None)), kinds.iter().position(|(k, _)| *k == draft.kind).unwrap_or(0)) {
            draft.kind = kinds[k].0.into();
        }
        ui.add(egui::TextEdit::singleline(&mut draft.title).hint_text("What goes wrong, in a line").desired_width(f32::INFINITY).margin(theme::INPUT_MARGIN));
        ui.add(egui::TextEdit::multiline(&mut draft.body).desired_rows(7).desired_width(f32::INFINITY).margin(theme::INPUT_MARGIN));
        match &state {
            Some(IssueState::Sending) => {
                ui.label(RichText::new("Sending\u{2026}").color(theme::pal(ui).secondary));
            }
            Some(IssueState::Filed { number, url, existing }) => {
                ui.hyperlink_to(if *existing { format!("Already reported as #{number}") } else { format!("Filed as #{number}") }, url);
            }
            Some(IssueState::Failed(e)) => {
                ui.label(RichText::new(e).color(theme::pal(ui).danger));
            }
            _ => {}
        }
        ui.horizontal(|ui| {
            let filed = matches!(state, Some(IssueState::Filed { .. }));
            if !filed {
                let ready = !draft.title.trim().is_empty() && state != Some(IssueState::Sending);
                if ui.add_enabled_ui(ready, |ui| kit::button(ui, kit::ButtonKind::Primary, None, "Send").clicked()).inner {
                    send = true;
                }
            }
            if kit::button(ui, kit::ButtonKind::Secondary, None, if filed { "Done" } else { "Cancel" }).clicked() {
                close = true;
            }
        });
    });
    if send {
        let d = draft.clone();
        let repo = st.settings.chat.issue_repo.clone();
        st.chat.file_issue(FORM_ISSUE.into(), d, repo);
    }
    if close {
        st.chat.issue_form = None;
    }
}

#[cfg(not(target_arch = "wasm32"))]
const ISSUE_FORM_W: f32 = 520.0;

/// What a fresh session needs from a chat that grew long: the user's recent requests and the
/// agent's last report. The canvas itself is read with the tools.
fn recap(messages: &[Message]) -> String {
    let asks: Vec<String> = messages
        .iter()
        .filter(|m| m.role == Role::User)
        .rev()
        .take(RECAP_ASKS)
        .map(|m| chat_msgs::split_context(&m.text).1.chars().take(RECAP_ASK_CHARS).collect::<String>())
        .collect();
    let last = messages.iter().rev().find(|m| m.role == Role::Assistant && !m.text.trim().is_empty()).map(|m| m.text.chars().take(RECAP_REPORT_CHARS).collect::<String>()).unwrap_or_default();
    let asks = asks.iter().rev().map(|a| format!("- {}", a.replace('\n', " "))).collect::<Vec<_>>().join("\n");
    format!(
        "[Fresh session] This chat continues from an earlier session that grew long. The design is on the canvas: read what you need with the tools (get_basic_info, get_tree_summary) instead of assuming.\nRecent requests:\n{asks}\nYour last report:\n{last}"
    )
}

fn backend_label(b: &Backend, prefs: &super::settings::ChatPrefs) -> String {
    match b.kind {
        BackendKind::ClaudeCode => {
            ["Claude Code", b.model.as_deref().map_or("", model_name), b.effort.as_deref().unwrap_or_default()].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" \u{b7} ")
        }
        BackendKind::Codex => "Codex".into(),
        BackendKind::Acp => b.agent.as_deref().map_or("Agent".into(), |id| agent_name(id, prefs)),
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

/// "Card" for one layer, "Card \u{b7} 3 layers" for several.
fn chip_label(first: &str, count: usize) -> String {
    if count > 1 { format!("{first} \u{b7} {count} layers") } else { first.to_owned() }
}

/// Adds a chip unless one with the same layers (and area) is already there.
fn push_chip(chips: &mut Vec<SelChip>, chip: SelChip) {
    if !chips.iter().any(|c| c.keys == chip.keys && c.rect == chip.rect) {
        chips.push(chip);
    }
}

/// The `[Selection]` context lines of the chips, one each, with node ids.
#[cfg(not(target_arch = "wasm32"))]
fn selection_lines(chips: &[SelChip], notes: &[String]) -> Vec<String> {
    chips.iter().map(chip_line).chain(notes.iter().map(|n| format!("[Selection] {n}"))).collect()
}

#[cfg(not(target_arch = "wasm32"))]
/// `[Selection] Card (a:1)`, or for an area `[Area] page P, rect x,y,w,h, contains: a:1, a:2`.
fn chip_line(c: &SelChip) -> String {
    let Some([x, y, w, h]) = c.rect else { return format!("[Selection] {} ({})", c.label, c.keys.join(", ")) };
    let ids = if c.keys.is_empty() { "nothing".to_owned() } else { c.keys.join(", ") };
    format!("[Area] page {}, rect {x:.0},{y:.0},{w:.0},{h:.0}, contains: {ids}", c.page)
}

/// One line per annotation on the chips' layers (and their children): number, type key, label,
/// first line of the body.
pub fn selection_notes(ed: &Editor, chips: &[SelChip]) -> Vec<String> {
    let cats = ed.design.categories();
    let mut seen = Vec::new();
    let mut out = Vec::new();
    for key in chips.iter().flat_map(|c| &c.keys) {
        let Some((prefix, num)) = key.split_once(':') else { continue };
        let (Ok(page), Ok(n)) = (crate::mcp::exec::find_page(ed, prefix), num.parse::<u64>()) else { continue };
        let doc = ed.page_doc(page);
        let id = crate::model::NodeId(n);
        if doc.get(id).is_none() {
            continue;
        }
        let (subtree, numbers) = (doc.subtree(id), crate::model::annotation::numbers(doc));
        for a in doc.annotations.iter().filter(|a| a.target.is_some_and(|t| subtree.contains(&t))) {
            if seen.contains(&(page, a.id)) {
                continue;
            }
            seen.push((page, a.id));
            let type_key = crate::model::annotation::category_by_key(&cats, &a.category).or_else(|| cats.iter().find(|c| c.id == a.category)).map_or(a.category.as_str(), |c| c.key());
            let line = a.body.lines().next().unwrap_or_default().trim();
            let sep = if line.is_empty() { "" } else { " \u{2014} " };
            out.push(format!("note #{} [{type_key}] on {key}: {}{sep}{line}", numbers.get(&a.id).map_or(0, |n| n.1), a.label));
        }
    }
    out
}

/// The current selection becomes a chip (Cmd+L or the @ button); each click adds one.
pub(super) fn add_selection(ed: &mut Editor, st: &mut ChatState) {
    let ids = ed.top_selection();
    if ids.is_empty() {
        ed.status = "Select a layer to add it to the chat".into();
        return;
    }
    push_chip(&mut st.sels, layer_chip(ed, st.gpu.as_ref(), ed.active_page, &ids));
}

/// Sends the composer an Actions-menu intent filled in, once detection is done (so the chat has
/// its backend); a failed start shows the usual error row.
pub(super) fn send_draft(ctx: &egui::Context, ed: &Editor, c: &mut ChatState) {
    if !c.send_draft || c.detect.is_none() || c.is_running() {
        return;
    }
    c.send_draft = false;
    c.sel_notes = selection_notes(ed, &c.sels);
    c.refresh_ds(ed, true);
    let _ = c.submit(ctx);
}

/// Pans the canvas to a node the agent touched, unless the user moved the camera lately.
/// Once a frame, in both modes: show the layer an agent is working on (Settings, Chat, Follow
/// agents), unless the user moved the canvas a moment ago.
pub fn follow(ed: &mut Editor, c: &mut ChatState) {
    let cam = |ed: &Editor| [ed.camera.pan.x, ed.camera.pan.y, ed.camera.zoom];
    if c.last_cam.is_some_and(|l| l != cam(ed)) {
        c.cam_moved = Some(web_time::Instant::now());
    }
    if let Some(key) = c.follow.take()
        && c.prefs.follow_agents
        && c.cam_moved.is_none_or(|t| t.elapsed() >= SELF_FOLLOW_QUIET)
    {
        pan_to(ed, &key);
    }
    c.last_cam = Some(cam(ed));
}

fn pan_to(ed: &mut Editor, key: &str) {
    let Some((prefix, num)) = key.trim_start_matches('#').split_once(':') else { return };
    let (Ok(page), Ok(n)) = (crate::mcp::exec::find_page(ed, prefix), num.parse::<u64>()) else { return };
    // Following goes to the page the agent works on.
    if page != ed.active_page {
        ed.switch_page(page);
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
    chat_msgs::lightbox(&ctx);
    #[cfg(not(target_arch = "wasm32"))]
    {
        st.chat.poll_issues(&ctx);
        issue_form(&ctx, st);
    }
    st.chat.ensure_chat();
    if std::mem::take(&mut st.settings.chat_changed) {
        // Paths, key or Re-scan changed in Settings: what was detected is stale.
        st.chat.rescan(&ctx, &st.settings.chat);
    }
    st.chat.poll_detect(&ctx, &st.settings.chat);
    st.chat.ensure_defaults(&st.settings.chat);
    send_draft(&ctx, ed, &mut st.chat);
    st.chat.refresh_ds(ed, false);
    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::L)) {
        add_selection(ed, &mut st.chat);
    }
    ui.spacing_mut().item_spacing.y = 6.0;
    let chip = header(ui, st);
    ui.add_space(6.0);
    // Nothing to talk to yet: no agent is ready, or this empty chat's agent is gone.
    let setup = st.chat.detect.as_ref().zip(st.chat.chat.as_ref()).is_some_and(|(d, c)| c.messages.is_empty() && !d.has_backend(&c.backend));
    let mut acts = Acts::default();
    let composer_top = egui::Panel::bottom("chat-composer").frame(egui::Frame::NONE).resizable(false).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 6.0;
        if let Some(todos) = st.chat.chat.as_ref().map(chat_msgs::latest_plan).filter(|t| !t.is_empty()) {
            chat_msgs::plan_card(ui, &todos);
        }
        image_offer(ui, st);
        composer(ui, ed, st, column_rect, setup);
    })
    .response
    .rect
    .top();
    if st.review_run.is_some() {
        super::panels::review(ui, ed, st);
    } else if setup {
        let act = egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| chat_agents::setup(ui, st.chat.detect.as_ref().expect("setup needs a scan"), &st.settings.chat)).inner;
        if let Some(a) = act {
            st.chat.act(&ctx, a, &mut st.settings);
        }
    } else if st.chat.chat.as_ref().is_none_or(|c| c.messages.is_empty()) && !st.chat.is_running() {
        chat_msgs::empty(ui, &mut acts);
        if let Some(e) = st.chat.error.as_deref() {
            chat_msgs::error_row(ui, e, st.chat.signin().as_ref(), &mut acts);
        }
    } else if let Some(chat) = &st.chat.chat {
        let signin = st.chat.signin();
        let view = View {
            running: st.chat.is_running(),
            thinking: &st.chat.thinking,
            thought_secs: st.chat.thought_secs,
            runs: &st.runs,
            approvals: &st.chat.approvals,
            error: st.chat.error.as_deref(),
            stopped: st.chat.stopped,
            budget: st.chat.budget_hit.then_some(st.settings.chat.turn_budget),
            issues: &st.chat.issues,
            signin: signin.as_ref(),
            live_run: st.run_open.iter().find(|r| r.client.split(" \u{b7} ").next() == Some(crate::mcp::CHAT_CLIENT)),
        };
        chat_msgs::list(ui, chat, &view, &mut acts);
    }
    if let Some(chip) = chip.filter(|_| st.chat.picker_open) {
        let (d, backend) = (st.chat.detect.clone().unwrap_or_default(), st.chat.chat.as_ref().map(|c| c.backend.clone()).unwrap_or_default());
        let input = PanelIn { detect: &d, prefs: &st.settings.chat, backend: &backend, running: st.chat.is_running(), column: column_rect, chip, composer_top };
        if let Some(a) = chat_agents::panel(&ctx, &mut st.chat.panel, &input) {
            st.chat.act(&ctx, a, &mut st.settings);
        }
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
    #[cfg(not(target_arch = "wasm32"))]
    if let Some((key, d)) = acts.send_issue {
        let repo = st.settings.chat.issue_repo.clone();
        st.chat.file_issue(key, d, repo);
    }
    if let Some(key) = acts.discard_issue {
        st.chat.issues.insert(key, IssueState::Discarded);
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
    if acts.retry_last && st.chat.start_failed {
        st.chat.sel_notes = selection_notes(ed, &st.chat.sels);
        st.chat.refresh_ds(ed, true);
        st.chat.retry_start(ctx);
        return;
    }
    let again = acts.retry_last.then(|| st.chat.last_prompt()).flatten();
    if let Some(text) = acts.send.or(again) {
        st.chat.refresh_ds(ed, true);
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
        (Some(c), Some(d)) if !d.has_backend(&c.backend) => "No agent connected".to_owned(),
        (Some(c), _) => backend_label(&c.backend, &st.settings.chat),
        _ => String::new(),
    };
    let context = st.chat.context_use();
    let ds_tokens = if st.chat.ds_text().is_empty() { 0 } else { st.chat.ds.tokens() };
    let mut rect = None;
    ui.horizontal(|ui| {
        let r = kit::chip_ink(ui, &label, st.chat.picker_open.then(theme::accent));
        rect = Some(r.rect);
        if r.clicked() {
            st.chat.picker_open = !st.chat.picker_open;
            st.chat.panel.fresh = st.chat.picker_open;
        }
        let Some((used, window)) = context else { return };
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let long = used > FRESH_SESSION_AT;
            let mut tip = String::new();
            if ds_tokens > 0 {
                tip = format!("Includes the design-system summary, about {} tokens.", chat_msgs::fmt_k(ds_tokens));
            }
            // A CLI session can be compacted: the next message starts fresh with a recap.
            let can_compact = st.chat.chat.as_ref().is_some_and(|c| c.backend.kind != BackendKind::Anthropic && c.backend.cli_session.is_some());
            let queued = can_compact && (long || st.chat.fresh_next);
            if queued {
                tip.push_str("\nYour next message starts a fresh agent session with a recap of this chat: each step re-reads far less. The canvas keeps the work.");
            } else if can_compact {
                tip.push_str("\nClick to compact: your next message starts a fresh agent session with a recap.");
            }
            let text = format!("{} / {}{}", chat_msgs::fmt_k(used), chat_msgs::fmt_k(window), if queued { " \u{b7} fresh next" } else { "" });
            let r = ui.add(egui::Label::new(RichText::new(text).size(theme::SECONDARY).color(if long || queued { p.warning } else { p.secondary })).sense(egui::Sense::click()));
            if let Some(total) = st.chat.chat.as_ref().and_then(chat_msgs::chat_cost) {
                ui.label(RichText::new(format!("${total:.2} \u{b7}")).size(theme::SECONDARY).color(p.secondary))
                    .on_hover_text("What this chat has cost so far, at API prices. A Claude or ChatGPT plan is not billed per token.");
            }
            let r = if tip.is_empty() { r } else { r.on_hover_text(tip.trim()) };
            if can_compact && !long && r.clicked() {
                st.chat.fresh_next = !st.chat.fresh_next;
            }
            let (bar, _) = ui.allocate_exact_size(egui::vec2(36.0, 3.0), egui::Sense::hover());
            ui.painter().rect_filled(bar, 2.0, p.separator);
            let frac = (used as f32 / window as f32).clamp(0.0, 1.0);
            let fill = egui::Rect::from_min_size(bar.min, egui::vec2(bar.width() * frac, bar.height()));
            ui.painter().rect_filled(fill, 2.0, if frac > CONTEXT_WARN || long { p.warning } else { theme::accent() });
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
        if ui.add(egui::Button::new(RichText::new("Report a PaperCraft issue\u{2026}").size(theme::SECONDARY + 1.0)).frame(false)).clicked() {
            st.chat.issues.remove(FORM_ISSUE);
            st.chat.issue_form = Some(IssueDraft { kind: "bug".into(), title: String::new(), body: ISSUE_TEMPLATE.into() });
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

/// One selection chip: "@ Name  3 layers  x". True when clicked (removes it).
fn sel_chip(ui: &mut Ui, label: &str) -> bool {
    let p = theme::pal(ui);
    let (name, rest) = label.split_once(" \u{b7} ").unwrap_or((label, ""));
    let paint = |text: &str, font: egui::FontId, color| ui.painter().layout_no_wrap(text.to_owned(), font, color);
    let parts = [
        paint("@", theme::semibold(theme::SECONDARY), theme::accent()),
        paint(name, theme::semibold(theme::SECONDARY), p.text),
        paint(rest, egui::FontId::proportional(theme::SECONDARY), p.secondary),
        paint("\u{d7}", egui::FontId::proportional(theme::BODY), p.secondary),
    ];
    const GAP: f32 = 6.0;
    let shown = if rest.is_empty() { &parts[..2] } else { &parts[..3] };
    let w = 2.0 * GAP + shown.iter().chain([&parts[3]]).map(|g| g.size().x + GAP).sum::<f32>();
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, kit::CHIP_H), egui::Sense::click());
    ui.painter().rect_filled(rect, egui::CornerRadius::same(255), theme::blend(p.field, p.hover.gamma_multiply(f32::from(u8::from(resp.hovered())))));
    let mut x = rect.left() + GAP + 2.0;
    for g in shown.iter().chain([&parts[3]]) {
        let pos = egui::pos2(x, rect.center().y - g.size().y / 2.0);
        x += g.size().x + GAP;
        ui.painter().galley(pos, g.clone(), p.text);
    }
    resp.on_hover_text("Remove from the chat").clicked()
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
        // Messages added to the running turn: waiting for the agent's next step, or seen.
        if !st.chat.added.is_empty() {
            let waiting = st.chat.turn_chat.as_deref().map(crate::mcp::steer::pending).unwrap_or_default();
            for a in &st.chat.added {
                let (tag, color) = if waiting.contains(a) { ("Waiting", p.tertiary) } else { ("Seen", p.secondary) };
                let line: String = a.lines().next().unwrap_or_default().chars().take(ADDED_PREVIEW).collect();
                ui.label(RichText::new(format!("{tag} \u{b7} {line}")).size(theme::SECONDARY).color(color));
            }
        }
        if !st.chat.sels.is_empty() || !st.chat.images.is_empty() {
            ui.horizontal_wrapped(|ui| {
                let clicked = st.chat.sels.iter().position(|s| sel_chip(ui, &s.label));
                if let Some(i) = clicked {
                    st.chat.sels.remove(i);
                }
                let mut drop = None;
                for (i, a) in st.chat.images.iter().enumerate() {
                    let key = format!("draft-{}-{}", a.name, a.img.data.len());
                    let Some(thumb) = chat_msgs::image_thumb(ui, &key, chat_msgs::ATTACHED_THUMB_H, || Some(a.img.data.clone())) else {
                        if kit::chip(ui, &format!("{} \u{d7}", a.name), false).clicked() {
                            drop = Some(i);
                        }
                        continue;
                    };
                    let corner = thumb.rect.right_top();
                    thumb.on_hover_text(format!("{} \u{b7} {} KB", a.name, (a.img.data.len() / 1024).max(1)));
                    // The remove button sits on the thumbnail's top-right corner.
                    let x = egui::Rect::from_center_size(corner + egui::vec2(-11.0, 11.0), egui::vec2(18.0, 18.0));
                    let resp = ui.interact(x, egui::Id::new(("chat-image-remove", &key)), egui::Sense::click());
                    let p = theme::pal(ui);
                    ui.painter().circle_filled(x.center(), 9.0, if resp.hovered() { p.danger } else { p.popover });
                    ui.painter().text(x.center(), egui::Align2::CENTER_CENTER, "\u{d7}", egui::FontId::proportional(theme::BODY), p.text);
                    if resp.on_hover_text("Remove the image").clicked() {
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
            "Add to what it is doing (Enter), or press Stop to interrupt"
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
        // Buttons on the right; the chips wrap in what is left instead of running under them.
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if running {
                let stopping = st.chat.stopping;
                let label = if stopping { "Stopping\u{2026}" } else { "Stop" };
                if ui.add_enabled_ui(!stopping, |ui| kit::button(ui, kit::ButtonKind::Danger, None, label).clicked()).inner {
                    st.chat.stop();
                }
                let can = !stopping && !st.chat.draft.trim().is_empty();
                let add = ui.add_enabled_ui(can, |ui| kit::button(ui, kit::ButtonKind::Secondary, Some(Icon::ArrowRight), "Add").clicked()).inner;
                if add {
                    st.chat.add_to_turn();
                }
            } else {
                let can = backend_ok && !no_project && (!st.chat.draft.trim().is_empty() || !st.chat.images.is_empty());
                ui.add_enabled_ui(can, |ui| {
                    submit = kit::button(ui, kit::ButtonKind::Primary, Some(Icon::ArrowRight), "Send").clicked();
                });
            }
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(true), |ui| {
                if kit::chip(ui, "@ Selection", false).on_hover_text("Add the selection (Cmd+L)").clicked() {
                    add_selection(ed, &mut st.chat);
                }
                let tip = "Point at the canvas: click a layer or Shift+drag an area (Esc cancels). Option+click adds a layer too.";
                if kit::chip(ui, "Pick", st.chat.picking).on_hover_text(tip).clicked() {
                    st.chat.picking = !st.chat.picking;
                }
                super::actions::chip(ui, ed, st);
                let tip = "Let the agent search the web and bring back reference images";
                if kit::chip(ui, "Web", st.settings.chat.research).on_hover_text(tip).clicked() {
                    st.settings.chat.research = !st.settings.chat.research;
                    st.settings.chat.save();
                }
                let tip = "Keep going on its own: questions get the recommended answer and the next step starts, until you press Stop or a turn hits its budget";
                if kit::chip(ui, "Auto", st.settings.chat.auto).on_hover_text(tip).clicked() {
                    st.settings.chat.auto = !st.settings.chat.auto;
                    st.settings.chat.save();
                }
                #[cfg(not(target_arch = "wasm32"))]
                if theme::icon_button(ui, Icon::Image, 24.0, false).on_hover_text("Attach an image (or paste, or drop one)").clicked()
                    && let Some(path) = rfd::FileDialog::new().add_filter("Images", &["png", "jpg", "jpeg", "gif", "webp"]).pick_file()
                    && let Some(name) = path.file_name().map(|n| n.to_string_lossy().into_owned())
                    && let (Some(mime), Ok(data)) = (image_mime(&name), std::fs::read(&path))
                {
                    st.chat.add_image(name, mime, data);
                }
            });
        });
    });
    if enter && running && !st.chat.stopping {
        st.chat.add_to_turn();
    }
    if (submit || enter) && !running && backend_ok && !no_project {
        let ctx = ui.ctx().clone();
        st.chat.sel_notes = selection_notes(ed, &st.chat.sels);
        st.chat.refresh_ds(ed, true);
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

    #[test]
    fn a_drafted_call_finds_its_target_in_partial_input() {
        assert_eq!(draft_target(r#"{"mode": "replace", "targetNodeId" : "8f98b5:12", "ht"#).as_deref(), Some("8f98b5:12"));
        assert_eq!(draft_target(r#"{"targetNodeId": "8f98b5:1"#), None, "not closed yet");
        assert_eq!(draft_target(r#"{"targetNodeId": "root", "nodeId": "a1:2"}"#).as_deref(), Some("a1:2"), "root is not a node");
    }

    #[test]
    fn messages_added_during_a_turn_wait_for_the_agent_then_follow_up() {
        let mut st = ChatState { turn_chat: Some("added-test".into()), ..Default::default() };
        st.draft = "use the amber badge".into();
        st.add_to_turn();
        assert!(st.draft.is_empty());
        assert_eq!(crate::mcp::steer::pending("added-test"), vec!["use the amber badge".to_string()]);
        st.finish_turn();
        assert_eq!(st.followup.take().as_deref(), Some("use the amber badge"), "not picked up: sent next");
        assert!(st.added.is_empty());
        // After Stop it goes back into the box instead.
        st.turn_chat = Some("added-test".into());
        st.draft = "and a dark mode".into();
        st.add_to_turn();
        st.stopping = true;
        st.finish_turn();
        assert_eq!((st.draft.as_str(), st.followup.is_none()), ("and a dark mode", true));
    }
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

    #[test]
    fn a_turn_past_its_budget_is_stopped() {
        let (mut st, tx, _running, dir) = running_turn();
        st.prefs.turn_budget = 250_000;
        let ctx = egui::Context::default();
        tx.send(Event::Context(120_000)).unwrap();
        tx.send(Event::Context(125_000)).unwrap();
        st.pump(&ctx);
        assert!(!st.stopping && !st.budget_hit, "245k read: under the budget");
        tx.send(Event::Context(130_000)).unwrap();
        st.pump(&ctx);
        assert!(st.stopping && st.budget_hit, "375k read: stopped like the Stop button");
        std::fs::remove_dir_all(dir).ok();
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
    fn a_stopped_backend_that_dies_shows_stopped_not_an_error() {
        use std::sync::atomic::Ordering;
        let (mut st, tx, running, dir) = running_turn();
        let ctx = egui::Context::default();
        st.stop();
        tx.send(Event::Error("Codex exited with signal: 15".into())).unwrap();
        running.store(false, Ordering::SeqCst);
        st.pump(&ctx);
        assert!(!st.is_running());
        assert_eq!(st.error, None);
        assert!(st.stopped);
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
        st.chat.as_mut().unwrap().messages.last_mut().unwrap().context = Some(CONTEXT_WINDOW + 1);
        assert_eq!(st.context_use(), Some((CONTEXT_WINDOW, CONTEXT_WINDOW)));
    }

    #[test]
    fn a_long_cli_session_is_not_resumed_and_the_new_one_gets_a_recap() {
        let msgs = vec![
            Message { role: Role::User, text: "[Selection] Card (a:1)\n\nDesign a sprint board".into(), ..Default::default() },
            Message { role: Role::Assistant, text: "Built the board with 4 columns.".into(), context: Some(FRESH_SESSION_AT + 1), ..Default::default() },
        ];
        let r = recap(&msgs);
        assert!(r.starts_with("[Fresh session]") && r.contains("- Design a sprint board") && r.contains("Built the board with 4 columns."), "{r}");
        assert!(!r.contains("[Selection]"), "context lines are not repeated");
    }

    #[test]
    fn a_resumed_claude_code_turn_costs_its_share_of_the_session_total() {
        let mut chat = new_chat();
        chat.backend = Backend { kind: BackendKind::ClaudeCode, cli_session: Some("s".into()), ..Default::default() };
        chat.messages.push(Message { role: Role::Assistant, session_cost: Some(6.25), ..Default::default() });
        chat.messages.push(Message { role: Role::User, text: "annotate".into(), ..Default::default() });
        chat.messages.push(Message { role: Role::Assistant, text: "done".into(), ..Default::default() });
        let mut st = ChatState { chat: Some(chat), ..Default::default() };
        let usage = crate::chat::Usage { cost_usd: Some(6.72), ..Default::default() };
        st.apply(Event::TurnEnd { usage: Some(usage), session: Some("s".into()), stop_reason: None });
        let m = st.chat.as_ref().unwrap().messages.last().unwrap();
        assert!((m.usage.as_ref().unwrap().cost_usd.unwrap() - 0.47).abs() < 1e-9, "{:?}", m.usage);
        assert_eq!(m.session_cost, Some(6.72), "the running total is kept for the next turn");
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

    fn chip(label: &str, keys: &[&str]) -> SelChip {
        SelChip { label: label.into(), keys: keys.iter().map(|k| (*k).into()).collect(), ..Default::default() }
    }

    #[test]
    fn selections_stack_dedupe_and_become_context_lines() {
        assert_eq!(chip_label("Card", 1), "Card");
        assert_eq!(chip_label("Card", 3), "Card \u{b7} 3 layers");
        let mut chips = vec![];
        push_chip(&mut chips, chip("Card", &["a:1"]));
        push_chip(&mut chips, chip("Hero \u{b7} 2 layers", &["a:2", "a:3"]));
        push_chip(&mut chips, chip("Card", &["a:1"]));
        assert_eq!(chips.len(), 2, "an identical selection is not added twice");
        chips.remove(0);
        push_chip(&mut chips, chip("Card", &["a:1"]));
        assert_eq!(selection_lines(&chips, &[]), ["[Selection] Hero \u{b7} 2 layers (a:2, a:3)", "[Selection] Card (a:1)"]);
        let prompt = format!("{}\n\nMake it blue", selection_lines(&chips, &[]).join("\n"));
        assert_eq!(chat_msgs::split_context(&prompt).0.len(), 2);
    }

    #[test]
    fn selection_notes_list_the_annotations_of_selected_layers() {
        use crate::geom::Rect;
        use crate::model::{annotation::Annotation, Node, NodeId, NodeKind};
        let mut ed = Editor::default();
        let page = ed.active_page;
        let id = ed.add_node(Node::new(NodeId(0), NodeKind::Rect, Rect::new(0.0, 0.0, 10.0, 10.0)), None);
        let mut a = Annotation::new(Some(id), "Use 8pt grid", "rule");
        a.id = 1;
        a.body = "first line\nsecond".into();
        ed.doc.annotations.push(a);
        let chips = [SelChip { label: "x".into(), keys: vec![crate::mcp::exec::node_key(&ed, page, id)], ..Default::default() }];
        let notes = selection_notes(&ed, &chips);
        assert_eq!(notes.len(), 1, "{notes:?}");
        assert!(notes[0].contains("[rule]") && notes[0].contains("Use 8pt grid \u{2014} first line") && !notes[0].contains("second"), "{notes:?}");
        let lines = selection_lines(&chips, &notes);
        assert_eq!(chat_msgs::split_context(&format!("{}\n\nhi", lines.join("\n"))).0.len(), 2);
        assert_eq!(selection_notes(&ed, &[]), Vec::<String>::new());
    }

    #[cfg(unix)]
    fn fake_claude(dir: &std::path::Path) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        std::fs::create_dir_all(dir).unwrap();
        let bin = dir.join("claude");
        std::fs::write(
            &bin,
            "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo 9.9.9; exit 0; fi\ncat >/dev/null\n\
             echo '{\"type\":\"stream_event\",\"event\":{\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"fake-ran\"}}}'\n\
             echo '{\"type\":\"result\",\"session_id\":\"s\"}'\n",
        )
        .unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        bin
    }

    /// Root cause of "Send does nothing": detection used the path set in Settings, `send` did not,
    /// so with the Dock's bare PATH the turn failed with "not found" although the row said Ready.
    #[test]
    fn cli_paths_come_from_the_agents_map_first() {
        use super::super::settings::{AgentPrefs, ChatPrefs};
        let mut prefs = ChatPrefs { codex_path: Some("/old/codex".into()), ..Default::default() };
        assert_eq!(cli_binary(&prefs, BackendKind::Codex), Some(PathBuf::from("/old/codex")));
        prefs.agents.insert("codex".into(), AgentPrefs { path: Some("/new/codex".into()), ..Default::default() });
        assert_eq!(cli_binary(&prefs, BackendKind::Codex), Some(PathBuf::from("/new/codex")));
        assert_eq!(cli_binary(&prefs, BackendKind::ClaudeCode), None);
    }

    #[cfg(unix)]
    #[test]
    fn send_runs_the_claude_path_set_in_settings() {
        let dir = std::env::temp_dir().join(format!("papercraft-chat-bin-{:016x}", fastrand::u64(..)));
        let bin = fake_claude(&dir.join("bin"));
        let prefs = super::super::settings::ChatPrefs { claude_path: Some(bin.display().to_string()), ..Default::default() };
        assert_eq!(cli_binary(&prefs, BackendKind::ClaudeCode), Some(bin));
        assert_eq!(cli_binary(&prefs, BackendKind::Codex), None);
        let mut st = ChatState { project: Some(dir.clone()), prefs, mcp_url: "http://127.0.0.1:1/mcp".into(), ..Default::default() };
        st.draft = "hello".into();
        let ctx = egui::Context::default();
        st.submit(&ctx).expect("starts with the configured binary");
        assert!(st.draft.is_empty() && st.error.is_none());
        let t = std::time::Instant::now();
        while st.is_running() && t.elapsed() < Duration::from_secs(10) {
            st.pump(&ctx);
            std::thread::sleep(Duration::from_millis(20));
        }
        st.pump(&ctx);
        let chat = st.chat.as_ref().unwrap();
        assert!(chat.messages.iter().any(|m| m.text == "fake-ran"), "{:?}", chat.messages);
        assert_eq!(store::list(&dir).len(), 1, "the chat is saved");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_failed_start_keeps_the_text_and_shows_an_error() {
        let dir = std::env::temp_dir().join(format!("papercraft-chat-fail-{:016x}", fastrand::u64(..)));
        let prefs = super::super::settings::ChatPrefs { claude_path: Some("/nonexistent/claude".into()), ..Default::default() };
        let mut st = ChatState { project: Some(dir.clone()), prefs, ..Default::default() };
        st.draft = "draw a card".into();
        st.sels = vec![chip("Card", &["a:1"])];
        let ctx = egui::Context::default();
        assert!(st.submit(&ctx).is_err());
        assert_eq!(st.draft, "draw a card", "the typed text stays in the composer");
        assert_eq!(st.sels.len(), 1);
        assert!(st.error.is_some() && st.start_failed, "an error row, never a silent no-op");
        assert!(st.chat.as_ref().unwrap().messages.is_empty(), "no message that never went anywhere");
        assert!(!st.is_running());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn attached_images_are_capped_to_what_the_model_sees() {
        let png = |w, h| crate::io::export::encode_png(&image::RgbaImage::from_pixel(w, h, image::Rgba([40, 90, 200, 255]))).unwrap();
        let (name, big, full) = crate::chat::shrink_image("shot.png".into(), ImageAttachment { mime: "image/png".into(), data: png(4000, 1000) });
        let size = image::load_from_memory(&big.data).map(|i| (i.width(), i.height())).unwrap();
        assert_eq!((name.as_str(), size), ("shot.png", (1568, 392)), "long edge capped, aspect kept");
        assert_eq!(image::load_from_memory(&full.expect("the original is kept")).unwrap().width(), 4000);
        let small = png(300, 200);
        let (_, kept, full) = crate::chat::shrink_image("a.png".into(), ImageAttachment { mime: "image/png".into(), data: small.clone() });
        assert!(kept.data.len() <= small.len() && full.is_none(), "never bigger than the original, nothing to zoom");
        let jpeg = ImageAttachment { mime: "image/jpeg".into(), data: vec![0xff, 0xd8] };
        assert_eq!(crate::chat::shrink_image("p.jpg".into(), jpeg.clone()).1, jpeg, "unreadable or small JPEGs pass through");
    }
}
