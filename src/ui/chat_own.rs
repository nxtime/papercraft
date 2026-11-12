#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
//! Chat in the browser with the person's own model key. The endpoints and their keys are kept in
//! this browser's localStorage (`chat::own`); a turn calls the provider straight from the page
//! (`chat::fetch`), so nothing here reaches the relay and the server owner pays nothing.

use super::chat_hosted;
use super::{ChatState, kit};
use crate::chat::endpoint::{self, ApiFormat, Endpoint};
use crate::chat::events::Turn;
use crate::chat::own::{Calls, OwnEndpoint, OwnStore};
use crate::chat::{Backend, BackendKind};
use crate::theme;
use crate::ui::UiState;
use egui::RichText;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

const FORM_W: f32 = 380.0;

/// The saved endpoints and the form that edits them.
#[derive(Default)]
pub struct Own {
    pub store: OwnStore,
    loaded: bool,
    /// Tool calls the running turn waits on; the shell runs them next to the relay's.
    pub calls: Arc<Mutex<Calls>>,
    form: Option<OwnEndpoint>,
}

impl Own {
    /// Reads localStorage once; there is nothing to read outside the browser.
    pub fn ensure(&mut self) {
        if self.loaded {
            return;
        }
        self.loaded = true;
        #[cfg(target_arch = "wasm32")]
        if let Some(s) = crate::web::dom::local_get(crate::chat::own::STORAGE_KEY) {
            self.store = OwnStore::from_json(&s);
        }
    }

    fn save(&self) {
        #[cfg(target_arch = "wasm32")]
        crate::web::dom::local_set(crate::chat::own::STORAGE_KEY, &self.store.to_json());
    }
}

/// A new chat on the endpoint `id`.
fn backend_for(ep: &Endpoint) -> Backend {
    Backend { kind: BackendKind::Anthropic, endpoint: Some(ep.id.clone()), model: Some(ep.model.clone()), ..Default::default() }
}

/// A form for a new endpoint: the Anthropic API, one key away.
fn blank_form() -> OwnEndpoint {
    OwnEndpoint { endpoint: Endpoint { id: Endpoint::blank().id, ..Endpoint::default() }, key: String::new() }
}

impl ChatState {
    /// The scan the chat column reads, after the hosted agents or the saved endpoints changed.
    pub(super) fn refresh_detect(&mut self) {
        self.own.ensure();
        #[cfg(target_arch = "wasm32")]
        {
            self.detect = Some(chat_hosted::detect_for(&self.hosted.agents, &self.own.store));
        }
        // A native scan finds the local agents and the keys; the server's join them.
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(d) = &mut self.detect {
            chat_hosted::merge_hosted(d, &self.hosted.agents);
        }
        self.defaults_for.clear();
    }
}

/// The running turn as `ChatState` sees it. Stopping aborts the request in flight.
struct OwnTurn {
    stop: Arc<AtomicBool>,
    running: Arc<AtomicBool>,
}

impl Turn for OwnTurn {
    fn stop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        #[cfg(target_arch = "wasm32")]
        crate::chat::fetch::abort();
    }

    fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}

#[cfg(target_arch = "wasm32")]
impl ChatState {
    /// Sends `prompt` to the chat's own endpoint, straight from the page.
    pub(super) fn send_own(&mut self, ctx: &egui::Context, prompt: &str) -> Result<(), String> {
        use super::{NEW_CHAT, TITLE_CHARS, TurnHandle, TurnWindow, chat_msgs};
        use crate::chat::events::TurnConfig;
        use crate::chat::{Message, Role, fetch::Fetch, own, tools};
        use crate::project::format::now_ms;
        let prompt = prompt.trim();
        if prompt.is_empty() {
            return Ok(());
        }
        if self.turn.is_some() {
            return Err("A turn is already running".into());
        }
        self.own.ensure();
        self.ensure_chat();
        let digest = self.ds_text().to_owned();
        let (Some(dir), Some(chat)) = (self.project.clone(), self.chat.as_mut()) else { return Err("Open a project to chat".into()) };
        let Some(saved) = chat.backend.endpoint.as_deref().and_then(|id| self.own.store.get(id)).filter(|o| o.is_ready()).cloned() else {
            return Err("Add your model key first: pick Add model above".into());
        };
        let history = dir.join("chats").join(format!("{}.api.json", chat.id));
        let mut messages = tools::load_history(Some(&history))?;
        messages.push(serde_json::json!({"role": "user", "content": prompt}));
        chat.messages.push(Message { role: Role::User, text: prompt.into(), at: now_ms(), ..Default::default() });
        if chat.title == NEW_CHAT {
            let text = chat_msgs::split_context(prompt).1;
            chat.title = text.lines().next().unwrap_or_default().chars().take(TITLE_CHARS).collect();
        }
        let id = chat.id.clone();
        let cfg = TurnConfig {
            model: chat.backend.model.clone().or_else(|| Some(saved.endpoint.model.clone())),
            system: super::hosted_system_prompt(),
            ds_digest: digest,
            endpoint: saved.endpoint.clone(),
            ..Default::default()
        };
        (self.error, self.start_failed, self.stopped, self.stopping) = (None, false, false, false);
        self.thinking.clear();
        (self.thinking_since, self.thought_secs) = (None, None);
        self.save();
        (self.retry, self.last_event) = (None, Some(web_time::Instant::now()));
        (self.live_context, self.turn_read, self.budget_hit) = (None, 0, false);
        self.window = Some(TurnWindow { chat: id.clone(), start: now_ms(), edits: None });
        let (tx, rx) = std::sync::mpsc::channel();
        let (stop, running) = (Arc::new(AtomicBool::new(false)), Arc::new(AtomicBool::new(true)));
        let wake = ctx.clone();
        let fetch = Fetch { stop: stop.clone(), calls: self.own.calls.clone(), wake: Box::new({ let c = ctx.clone(); move || c.request_repaint() }), label: format!("{} API", saved.endpoint.name), history };
        let done = running.clone();
        wasm_bindgen_futures::spawn_local(async move {
            own::run(&fetch, &cfg, &saved.key, messages, &|e| {
                let _ = tx.send(e);
                wake.request_repaint();
            })
            .await;
            // After the last event, which the column reads before it checks this flag.
            done.store(false, Ordering::SeqCst);
            wake.request_repaint();
        });
        self.turn_chat = Some(id);
        (self.turn, self.rx) = (Some(TurnHandle::Cli(Box::new(OwnTurn { stop, running }))), Some(rx));
        ctx.request_repaint();
        Ok(())
    }
}

/// The saved models as chips, an Edit chip for the open chat's, and Add model.
pub(super) fn chips(ui: &mut egui::Ui, st: &mut UiState) {
    st.chat.own.ensure();
    let running = st.chat.is_running();
    let backend = st.chat.chat.as_ref().map(|c| c.backend.clone());
    let current = backend.as_ref().filter(|b| b.kind == BackendKind::Anthropic).and_then(|b| b.endpoint.clone());
    for o in st.chat.own.store.endpoints.clone() {
        let on = current.as_deref() == Some(o.endpoint.id.as_str());
        if kit::chip(ui, &o.endpoint.name, on).clicked() && !on && !running {
            st.chat.set_backend(backend_for(&o.endpoint));
        }
    }
    if let Some(o) = current.as_deref().and_then(|id| st.chat.own.store.get(id)).cloned()
        && kit::chip(ui, "Edit", false).on_hover_text("Change this model or its key").clicked()
    {
        st.chat.own.form = Some(o);
    }
    if kit::chip(ui, "+ Add model", false).on_hover_text("Chat with your own API key, kept in this browser").clicked() {
        st.chat.own.form = Some(blank_form());
    }
}

/// What the person did in the form.
enum Done {
    Save(OwnEndpoint),
    Remove(String),
    Cancel,
}

/// The add/edit form, as a dialog over the page.
pub(super) fn form(ctx: &egui::Context, st: &mut UiState) {
    let Some(mut f) = st.chat.own.form.take() else { return };
    let p = theme::pal_from_ctx(ctx);
    let frame = egui::Frame::new().fill(p.sidebar).corner_radius(theme::r(24.0)).inner_margin(24.0);
    let existing = st.chat.own.store.get(&f.endpoint.id).is_some();
    let modal = egui::Modal::new(egui::Id::new("own-model")).frame(frame).show(ctx, |ui| {
        theme::modal_backing(ui, p.sidebar, theme::ru(24));
        ui.set_width(FORM_W.min(ui.ctx().content_rect().width() - 48.0));
        ui.spacing_mut().item_spacing.y = 8.0;
        ui.label(RichText::new(if existing { "Edit model" } else { "Add your own model" }).font(theme::semibold(18.0)));
        body(ui, &mut f, existing)
    });
    match modal.inner {
        Some(Done::Save(o)) => {
            let ep = o.endpoint.clone();
            st.chat.own.store.upsert(o);
            st.chat.own.save();
            st.chat.refresh_detect();
            st.chat.set_backend(backend_for(&ep));
        }
        Some(Done::Remove(id)) => {
            st.chat.own.store.remove(&id);
            st.chat.own.save();
            st.chat.refresh_detect();
        }
        Some(Done::Cancel) => {}
        None if modal.should_close() => {}
        None => st.chat.own.form = Some(f),
    }
}

fn body(ui: &mut egui::Ui, f: &mut OwnEndpoint, existing: bool) -> Option<Done> {
    let p = theme::pal(ui);
    ui.horizontal_wrapped(|ui| {
        for (fmt, label) in [(ApiFormat::Anthropic, "Anthropic"), (ApiFormat::OpenAi, "OpenAI-compatible")] {
            if kit::chip(ui, label, f.endpoint.format == fmt).clicked() && f.endpoint.format != fmt {
                f.endpoint = Endpoint { format: fmt, ..preset(fmt, &f.endpoint) };
            }
        }
    });
    field(ui, "Name", &mut f.endpoint.name, "My model", false);
    field(ui, "Address", &mut f.endpoint.base_url, "https://api.anthropic.com", false);
    field(ui, "Model", &mut f.endpoint.model, "model id", false);
    field(ui, "API key", &mut f.key, "paste your key", true);
    let host = endpoint::host(&f.endpoint.base_url).unwrap_or_else(|| "the address above".into());
    ui.label(RichText::new(format!("The key stays in this browser and is sent only to {host}. Anyone using this browser profile can read it.")).size(theme::SECONDARY).color(p.secondary));
    let ready = f.is_ready();
    let mut done = None;
    ui.horizontal(|ui| {
        if ui.add_enabled_ui(ready, |ui| kit::button(ui, kit::ButtonKind::Primary, None, "Save")).inner.clicked() {
            done = Some(Done::Save(f.clone()));
        }
        if kit::button(ui, kit::ButtonKind::Secondary, None, "Cancel").clicked() {
            done = Some(Done::Cancel);
        }
        if existing && kit::button(ui, kit::ButtonKind::Danger, None, "Remove").clicked() {
            done = Some(Done::Remove(f.endpoint.id.clone()));
        }
    });
    done
}

/// The address and model to start from when the format changes: the Anthropic API, or blanks to
/// fill for any other OpenAI-compatible server. The name and id stay.
fn preset(fmt: ApiFormat, now: &Endpoint) -> Endpoint {
    let base = Endpoint::default();
    match fmt {
        ApiFormat::Anthropic => Endpoint { id: now.id.clone(), name: now.name.clone(), base_url: base.base_url, model: base.model, ..now.clone() },
        ApiFormat::OpenAi => Endpoint { base_url: String::new(), model: String::new(), ..now.clone() },
    }
}

fn field(ui: &mut egui::Ui, label: &str, text: &mut String, hint: &str, secret: bool) {
    let p = theme::pal(ui);
    ui.label(RichText::new(label).size(theme::SECONDARY).color(p.secondary));
    ui.add(egui::TextEdit::singleline(text).hint_text(hint).password(secret).desired_width(f32::INFINITY));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_endpoint_form_starts_on_the_anthropic_api_with_its_own_id() {
        let (a, b) = (blank_form(), blank_form());
        assert_eq!(a.endpoint.format, ApiFormat::Anthropic);
        assert!(a.endpoint.base_url.starts_with("https://api.anthropic.com") && a.key.is_empty());
        assert_ne!(a.endpoint.id, b.endpoint.id);
        assert!(!a.is_ready());
    }

    #[test]
    fn switching_format_keeps_the_name_and_resets_the_address() {
        let now = Endpoint { id: "x".into(), name: "Mine".into(), format: ApiFormat::Anthropic, base_url: "https://api.anthropic.com".into(), model: "m".into() };
        let o = preset(ApiFormat::OpenAi, &now);
        assert_eq!((o.id.as_str(), o.name.as_str(), o.base_url.as_str()), ("x", "Mine", ""));
    }

    #[test]
    fn an_endpoint_chat_uses_the_api_backend_with_its_model() {
        let b = backend_for(&Endpoint { id: "x".into(), model: "m".into(), ..Default::default() });
        assert_eq!((b.kind, b.endpoint.as_deref(), b.model.as_deref()), (BackendKind::Anthropic, Some("x"), Some("m")));
    }
}
