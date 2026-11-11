#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
//! Chat in the browser build: the agents the relay runs for this tab (`papercraft-relay
//! --agents`). The tab sends message and cancel frames over its relay socket and folds the
//! streamed events into the chat like any other backend's. The shell (`web.rs`) moves the frames
//! between the socket and this state; nothing here touches the network.

use super::chat_agents::PanelIn;
use super::{ChatState, NEW_CHAT, TITLE_CHARS, TurnHandle, TurnWindow, chat_models, chat_msgs, kit};
use crate::chat::agents::{AgentState, AgentStatus};
use crate::chat::events::{Event, Turn};
use crate::chat::{Backend, BackendKind, Message, Role};
use crate::mcp::relay_proto::{AgentInfo, FromTab};
use crate::project::format::now_ms;
use crate::theme::{self, Icon};
use crate::ui::UiState;
use egui::{RichText, Ui};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex, PoisonError};

const CLAUDE: &str = "claude-code";
const OPENCODE: &str = "opencode";
const LOST: &str = "Lost the connection to the server.";
const NONE_HERE: &str = "This server does not run agents.";
/// The model chip is cut to this many characters; the menu shows the full names.
const MODEL_CHARS: usize = 40;
const MENU_W: f32 = 320.0;
const MENU_MAX_H: f32 = 360.0;

/// Whether the relay advertised any agent: the Design | Chat switch is enabled only then.
static AVAILABLE: AtomicBool = AtomicBool::new(false);

pub fn available() -> bool {
    AVAILABLE.load(Ordering::Relaxed)
}

/// What the relay told this tab, and the frames waiting to go out.
#[derive(Default)]
pub struct Hosted {
    pub agents: Vec<AgentInfo>,
    outbox: Arc<Mutex<Vec<FromTab>>>,
    /// Where the running turn's events go (`ChatState::rx` is the other end).
    tx: Option<Sender<Event>>,
    running: Arc<AtomicBool>,
}

/// The running turn as `ChatState` sees it: stopping sends a cancel frame.
struct HostedTurn {
    chat: String,
    outbox: Arc<Mutex<Vec<FromTab>>>,
    running: Arc<AtomicBool>,
}

impl Turn for HostedTurn {
    fn stop(&mut self) {
        self.outbox.lock().unwrap_or_else(PoisonError::into_inner).push(FromTab::Cancel { chat: self.chat.clone() });
    }

    fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}

/// The relay's id for the agent a chat is set to; None for backends a server cannot run.
fn hosted_id(b: &Backend) -> Option<&str> {
    match (b.kind, b.agent.as_deref()) {
        (BackendKind::ClaudeCode, _) => Some(CLAUDE),
        (BackendKind::Acp, Some(OPENCODE)) => Some(OPENCODE),
        _ => None,
    }
}

fn backend_for(id: &str) -> Backend {
    if id == CLAUDE {
        Backend { kind: BackendKind::ClaudeCode, ..Default::default() }
    } else {
        Backend { kind: BackendKind::Acp, agent: Some(id.to_owned()), ..Default::default() }
    }
}

/// The scan result the chat column expects, for agents that run on the server.
pub(super) fn detect_for(agents: &[AgentInfo]) -> super::Detect {
    let ready = AgentState::Ready { path: "server".into(), version: String::new() };
    super::Detect {
        claude: agents.iter().any(|a| a.id == CLAUDE).then(|| (PathBuf::from("server"), String::new())),
        agents: agents.iter().filter(|a| a.id != CLAUDE).map(|a| AgentStatus { id: a.id.clone(), state: ready.clone() }).collect(),
        ..Default::default()
    }
}

impl ChatState {
    /// The relay's hello: which agents it runs.
    pub fn set_hosted_agents(&mut self, agents: Vec<AgentInfo>) {
        AVAILABLE.store(!agents.is_empty(), Ordering::Relaxed);
        self.detect = Some(detect_for(&agents));
        self.hosted.agents = agents;
        // A chat that picked its default before the agents were known picks again.
        self.defaults_for.clear();
    }

    /// An event of chat `chat` from the relay. Events of a turn that is no longer ours are dropped.
    pub fn hosted_event(&mut self, chat: &str, event: Event) {
        if self.turn_chat.as_deref() != Some(chat) {
            return;
        }
        let ended = matches!(event, Event::TurnEnd { .. } | Event::Error(_));
        if self.hosted.tx.as_ref().is_some_and(|tx| tx.send(event).is_ok()) && ended {
            self.hosted.running.store(false, Ordering::SeqCst);
        }
    }

    /// The socket closed: a running turn will never hear back.
    pub fn hosted_lost(&mut self) {
        if let Some(chat) = self.turn_chat.clone() {
            self.hosted_event(&chat, Event::Error(LOST.into()));
        }
    }

    /// Frames for the shell to send over the relay socket.
    pub fn take_hosted_frames(&self) -> Vec<FromTab> {
        std::mem::take(&mut *self.hosted.outbox.lock().unwrap_or_else(PoisonError::into_inner))
    }

    /// Sends `prompt` to the chat's agent on the server.
    pub(super) fn send_hosted(&mut self, ctx: &egui::Context, prompt: &str) -> Result<(), String> {
        let prompt = prompt.trim();
        if prompt.is_empty() {
            return Ok(());
        }
        if self.turn.is_some() {
            return Err("A turn is already running".into());
        }
        self.ensure_chat();
        let Some(chat) = self.chat.as_mut() else { return Err("Open a project to chat".into()) };
        let agent = hosted_id(&chat.backend).filter(|id| self.hosted.agents.iter().any(|a| a.id == *id)).ok_or(NONE_HERE)?.to_owned();
        chat.messages.push(Message { role: Role::User, text: prompt.into(), at: now_ms(), ..Default::default() });
        if chat.title == NEW_CHAT {
            let text = chat_msgs::split_context(prompt).1;
            chat.title = text.lines().next().unwrap_or_default().chars().take(TITLE_CHARS).collect();
        }
        let id = chat.id.clone();
        let model = chat.backend.model.clone();
        (self.error, self.start_failed, self.stopped, self.stopping) = (None, false, false, false);
        self.thinking.clear();
        (self.thinking_since, self.thought_secs) = (None, None);
        self.save();
        (self.retry, self.last_event) = (None, Some(web_time::Instant::now()));
        (self.live_context, self.turn_read, self.budget_hit) = (None, 0, false);
        self.window = Some(TurnWindow { chat: id.clone(), start: now_ms(), edits: None });
        let (tx, rx) = channel();
        self.hosted.tx = Some(tx);
        self.hosted.running.store(true, Ordering::SeqCst);
        let outbox = self.hosted.outbox.clone();
        outbox.lock().unwrap_or_else(PoisonError::into_inner).push(FromTab::Chat { chat: id.clone(), agent, text: prompt.to_owned(), model });
        let turn = HostedTurn { chat: id.clone(), outbox, running: self.hosted.running.clone() };
        self.turn_chat = Some(id);
        (self.turn, self.rx) = (Some(TurnHandle::Cli(Box::new(turn))), Some(rx));
        ctx.request_repaint();
        Ok(())
    }
}

/// The agent picker under the chat title, then the model of an ACP agent.
pub(super) fn picker(ui: &mut Ui, st: &mut UiState) {
    let running = st.chat.is_running();
    let current = st.chat.chat.as_ref().and_then(|c| hosted_id(&c.backend)).map(str::to_owned);
    let mut chip = None;
    ui.horizontal_wrapped(|ui| {
        for a in st.chat.hosted.agents.clone() {
            let on = current.as_deref() == Some(a.id.as_str());
            if kit::chip(ui, &a.name, on).clicked() && !on && !running {
                st.chat.set_backend(backend_for(&a.id));
            }
        }
        if let Some(b) = st.chat.chat.as_ref().map(|c| &c.backend).filter(|b| b.kind == BackendKind::Acp) {
            let name = b.model.as_deref().map_or_else(|| "Default".into(), |m| super::acp_model_name(b.agent.as_deref(), m, &st.settings.chat));
            let label: String = format!("Model: {name}").chars().take(MODEL_CHARS).collect();
            chip = Some(kit::chip(ui, &label, false).on_hover_text("Pick a model, or type the id of your own"));
        }
    });
    if let Some(chip) = chip {
        model_menu(&chip, st);
    }
}

/// The model list of the desktop agent panel, in a menu under the model chip.
fn model_menu(chip: &egui::Response, st: &mut UiState) {
    let Some(backend) = st.chat.chat.as_ref().map(|c| c.backend.clone()) else { return };
    let mut act = None;
    let shown = egui::Popup::menu(chip).gap(theme::POPUP_GAP).width(MENU_W).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
        theme::popup_backing(ui);
        kit::icon_field(ui, Icon::Search, &mut st.chat.panel.query, "Search, or type a model id");
        ui.add_space(4.0);
        let detect = st.chat.detect.clone().unwrap_or_default();
        let i = PanelIn { detect: &detect, prefs: &st.settings.chat, backend: &backend, running: st.chat.is_running(), column: chip.rect, backdrop: chip.rect, chip: chip.rect, composer_top: chip.rect.bottom() };
        let query = st.chat.panel.query.clone();
        egui::ScrollArea::vertical().max_height(MENU_MAX_H).show(ui, |ui| act = chat_models::list(ui, &i, &query, &mut st.chat.panel.all_models));
    });
    if let Some(a) = act {
        st.chat.panel.query.clear();
        st.chat.act(&chip.ctx, a, &mut st.settings);
        egui::Popup::close_all(&chip.ctx);
    } else if shown.is_none() {
        st.chat.panel.query.clear();
    }
}

/// What the chat column says when the relay runs no agents.
pub(super) fn unavailable(ui: &mut Ui) {
    let p = theme::pal(ui);
    ui.add_space(24.0);
    ui.label(RichText::new(NONE_HERE).font(theme::semibold(theme::BODY)));
    ui.label(RichText::new("Start papercraft-relay with --agents claude,opencode to chat here, or connect your own agent from the Agent menu.").size(theme::SECONDARY).color(p.secondary));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn infos() -> Vec<AgentInfo> {
        vec![AgentInfo { id: CLAUDE.into(), name: "Claude Code".into() }, AgentInfo { id: OPENCODE.into(), name: "OpenCode".into() }]
    }

    #[test]
    fn the_scan_lists_the_hosted_agents_as_ready() {
        let d = detect_for(&infos());
        assert!(d.has_backend(&backend_for(CLAUDE)) && d.has_backend(&backend_for(OPENCODE)));
        assert!(!detect_for(&[]).any());
    }

    #[test]
    fn events_reach_only_the_running_turn_and_the_end_stops_it() {
        let mut st = ChatState::default();
        let (tx, rx) = channel();
        (st.hosted.tx, st.turn_chat) = (Some(tx), Some("c1".into()));
        st.hosted.running.store(true, Ordering::SeqCst);
        st.hosted_event("other", Event::Text("no".into()));
        st.hosted_event("c1", Event::Text("yes".into()));
        assert!(st.hosted.running.load(Ordering::SeqCst));
        st.hosted_lost();
        assert!(!st.hosted.running.load(Ordering::SeqCst));
        let seen: Vec<Event> = rx.try_iter().collect();
        assert_eq!(seen, [Event::Text("yes".into()), Event::Error(LOST.into())]);
    }

    #[test]
    fn stopping_sends_a_cancel_frame() {
        let st = ChatState::default();
        let mut turn = HostedTurn { chat: "c1".into(), outbox: st.hosted.outbox.clone(), running: st.hosted.running.clone() };
        turn.stop();
        assert_eq!(st.take_hosted_frames(), [FromTab::Cancel { chat: "c1".into() }]);
        assert!(st.take_hosted_frames().is_empty());
    }
}
