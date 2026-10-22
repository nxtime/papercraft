//! Settings dialog: appearance, canvas, chat agents and keys, updates.

use super::{UiState, kit, motion};
#[cfg(not(target_arch = "wasm32"))]
use super::confirm;
use crate::chat::BackendKind;
use crate::chat::endpoint::{self, Endpoint};
#[cfg(not(target_arch = "wasm32"))]
use crate::chat::endpoint::ApiFormat;
#[cfg(not(target_arch = "wasm32"))]
use crate::chat::{claude_code, codex};
use crate::theme::{self, Icon};
use egui::{RichText, Ui};
use std::path::PathBuf;
#[cfg(not(target_arch = "wasm32"))]
use std::path::Path;
use serde::{Deserialize, Serialize};

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    #[default]
    Appearance,
    Canvas,
    Chat,
    Updates,
}

pub const API_MODELS: [(&str, &str); 3] = [
    ("claude-opus-5-5", "Opus 5.5"),
    ("claude-sonnet-5-5", "Sonnet 5.5"),
    ("claude-haiku-5-5", "Haiku 5.5"),
];
pub const API_EFFORTS: [(&str, &str); 3] = [("low", "Low"), ("medium", "Medium"), ("high", "High")];
const DEFAULT_MODEL: &str = "claude-opus-5-5";
const DEFAULT_EFFORT: &str = "medium";
#[cfg(not(target_arch = "wasm32"))]
const BACKENDS: [(BackendKind, &str); 3] = [
    (BackendKind::ClaudeCode, "Claude Code"),
    (BackendKind::Codex, "Codex"),
    (BackendKind::Anthropic, "API"),
];

/// Chat defaults and agent paths, kept in `chat_settings.json` in the config folder.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ChatPrefs {
    /// Override for the `claude` binary; None means the first one on `PATH`.
    pub claude_path: Option<String>,
    /// Override for the `codex` binary; None means the first one on `PATH`.
    pub codex_path: Option<String>,
    pub default_backend: BackendKind,
    /// API endpoints, at least one. Keys are in the keychain, never here.
    pub endpoints: Vec<Endpoint>,
    /// Id of the endpoint new chats start on.
    pub default_endpoint: String,
    /// One of [`API_EFFORTS`]; sent to `api.anthropic.com` only.
    pub api_effort: String,
    /// Old settings files: read once into the default endpoint's model, never written.
    #[serde(skip_serializing)]
    pub api_model: Option<String>,
}

impl Default for ChatPrefs {
    fn default() -> Self {
        Self {
            claude_path: None,
            codex_path: None,
            default_backend: BackendKind::ClaudeCode,
            endpoints: vec![Endpoint::default()],
            default_endpoint: endpoint::DEFAULT_ENDPOINT_ID.into(),
            api_effort: DEFAULT_EFFORT.into(),
            api_model: None,
        }
    }
}

impl ChatPrefs {
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    /// Unreadable input means defaults; an unknown effort falls back to the default. A file from
    /// before endpoints existed (`api_model` and no `endpoints`) moves its model into the default
    /// `Anthropic` endpoint.
    pub fn from_json(s: &str) -> Self {
        let has_endpoints = serde_json::from_str::<serde_json::Value>(s).is_ok_and(|v| v["endpoints"].is_array());
        let mut prefs: Self = serde_json::from_str(s).unwrap_or_default();
        let legacy_model = prefs.api_model.take();
        if prefs.endpoints.is_empty() {
            prefs.endpoints = vec![Endpoint::default()];
        }
        if !has_endpoints {
            let model = legacy_model.filter(|m| API_MODELS.iter().any(|(id, _)| id == m));
            prefs.endpoints[0].model = model.unwrap_or_else(|| DEFAULT_MODEL.into());
        }
        if !prefs.endpoints.iter().any(|e| e.id == prefs.default_endpoint) {
            prefs.default_endpoint = prefs.endpoints[0].id.clone();
        }
        if !API_EFFORTS.iter().any(|(id, _)| *id == prefs.api_effort) {
            prefs.api_effort = DEFAULT_EFFORT.into();
        }
        prefs
    }

    /// The endpoint with this id; None or an unknown id means the default one.
    pub fn endpoint(&self, id: Option<&str>) -> &Endpoint {
        id.and_then(|id| self.endpoints.iter().find(|e| e.id == id)).unwrap_or_else(|| self.default_endpoint())
    }

    pub fn default_endpoint(&self) -> &Endpoint {
        self.endpoints.iter().find(|e| e.id == self.default_endpoint).unwrap_or(&self.endpoints[0])
    }

    fn path() -> Option<PathBuf> {
        #[cfg(not(target_arch = "wasm32"))]
        return dirs::config_dir().map(|d| d.join("papercraft").join("chat_settings.json"));
        #[cfg(target_arch = "wasm32")]
        None
    }

    pub fn load() -> Self {
        Self::path()
            .and_then(|p| crate::vfs::read_to_string(p).ok())
            .map_or_else(Self::default, |s| Self::from_json(&s))
    }

    pub fn save(&self) {
        let Some(p) = Self::path() else { return };
        if let Some(dir) = p.parent() {
            let _ = crate::vfs::create_dir_all(dir);
        }
        let _ = crate::vfs::write(p, self.to_json());
    }
}

pub struct SettingsUi {
    pub open: bool,
    page: Page,
    /// Chat defaults and agent paths. The chat UI reads these, e.g. `st.settings.chat.default_backend`.
    pub chat: ChatPrefs,
    /// Paths, key or Re-scan changed on the Chat page: the chat column detects the agents again.
    pub chat_changed: bool,
    #[cfg(not(target_arch = "wasm32"))]
    chat_page: ChatPage,
}

impl SettingsUi {
    /// Open the dialog on its Chat page.
    pub fn open_chat(&mut self) {
        self.open = true;
        self.page = Page::Chat;
    }
}

impl Default for SettingsUi {
    fn default() -> Self {
        Self {
            open: false,
            page: Page::default(),
            chat: ChatPrefs::load(),
            chat_changed: false,
            #[cfg(not(target_arch = "wasm32"))]
            chat_page: ChatPage::default(),
        }
    }
}

/// Chat page state that lives only while the dialog is open.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Default)]
struct ChatPage {
    /// Set when the page is first shown after the dialog opened; cleared when the dialog closes.
    scan: Option<Scan>,
    /// Typed keys by endpoint id.
    key_drafts: std::collections::HashMap<String, String>,
    key_error: Option<String>,
    /// The endpoint whose key waits for the remove confirmation.
    key_remove: Option<String>,
}

/// What the page shows about the CLIs and the stored key, read once (not every frame).
#[cfg(not(target_arch = "wasm32"))]
struct Scan {
    claude: Option<Found>,
    codex: Option<Found>,
    /// Last four characters of each endpoint's key (by id); the keys themselves are never kept here.
    key_tails: std::collections::HashMap<String, String>,
    env_key: bool,
}

/// Binary path and version.
#[cfg(not(target_arch = "wasm32"))]
type Found = (PathBuf, String);

#[cfg(not(target_arch = "wasm32"))]
impl Scan {
    fn read(prefs: &ChatPrefs) -> Scan {
        Scan {
            claude: claude_code::detect(prefs.claude_path.as_deref().map(Path::new)),
            codex: codex::detect(prefs.codex_path.as_deref().map(Path::new)),
            key_tails: prefs.endpoints.iter().filter_map(|e| Some((e.id.clone(), key_tail(&endpoint::get_key(e)?)))).collect(),
            env_key: std::env::var(KEY_ENV).is_ok(),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
const KEY_ENV: &str = "ANTHROPIC_API_KEY";
#[cfg(not(target_arch = "wasm32"))]
const KEY_TAIL_CHARS: usize = 4;
#[cfg(not(target_arch = "wasm32"))]
const KEY_MASK: &str = "•••• ";
#[cfg(not(target_arch = "wasm32"))]
const KEY_PLACEHOLDER: &str = "API key (optional for a local server)";
#[cfg(not(target_arch = "wasm32"))]
const CLAUDE_KEY_PLACEHOLDER: &str = "sk-ant-…";
#[cfg(not(target_arch = "wasm32"))]
const CLAUDE_INSTALL: &str = "npm i -g @anthropic-ai/claude-code";
#[cfg(not(target_arch = "wasm32"))]
const CODEX_INSTALL: &str = "npm i -g @openai/codex";
#[cfg(not(target_arch = "wasm32"))]
const AGENT_NAME_W: f32 = 150.0;
#[cfg(not(target_arch = "wasm32"))]
const AGENT_CHIP_W: f32 = 84.0;
#[cfg(not(target_arch = "wasm32"))]
const KEY_BUTTONS_W: f32 = 180.0;
#[cfg(not(target_arch = "wasm32"))]
const FORMAT_W: f32 = 130.0;
#[cfg(not(target_arch = "wasm32"))]
const FORMATS: [(ApiFormat, &str); 2] = [(ApiFormat::Anthropic, "Anthropic format"), (ApiFormat::OpenAi, "OpenAI format")];
#[cfg(not(target_arch = "wasm32"))]
const DEFAULT_GAP: f32 = 12.0;

#[cfg(not(target_arch = "wasm32"))]
fn key_tail(key: &str) -> String {
    let chars: Vec<char> = key.chars().collect();
    chars[chars.len().saturating_sub(KEY_TAIL_CHARS)..].iter().collect()
}

/// Where the platform keeps the API key, for the settings note.
#[cfg(not(target_arch = "wasm32"))]
fn keychain_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "macOS Keychain"
    } else if cfg!(windows) {
        "Windows Credential Manager"
    } else {
        "Secret Service"
    }
}

pub fn show(ctx: &egui::Context, st: &mut UiState) {
    let t = motion::progress(
        ctx,
        "settings-open",
        st.settings.open,
        motion::FAST,
        motion::ease_out_cubic,
    );
    if !st.settings.open && t <= 0.0 {
        // Closed: the next opening scans again.
        #[cfg(not(target_arch = "wasm32"))]
        {
            st.settings.chat_page.scan = None;
        }
        return;
    }
    let live = st.settings.open;
    let p = theme::pal_from_ctx(ctx);
    let frame = egui::Frame::new().fill(p.sidebar).corner_radius(24.0);
    let modal = egui::Modal::new(egui::Id::new("settings-modal"))
        .frame(frame)
        .show(ctx, |ui| {
            ui.multiply_opacity(t);
            if !live {
                ui.disable();
            }
            ui.set_width(780.0f32.min(ctx.content_rect().width() - 48.0));
            ui.set_height(520.0f32.min(ctx.content_rect().height() - 96.0));
            ui.horizontal_top(|ui| {
                egui::Frame::new()
                    .fill(p.backdrop)
                    .corner_radius(24.0)
                    .inner_margin(egui::Margin::symmetric(16, 24))
                    .show(ui, |ui| {
                        ui.set_min_width(180.0);
                        ui.set_max_width(180.0);
                        ui.set_min_height(ui.available_height());
                        ui.vertical(|ui| {
                            ui.label(RichText::new("Settings").font(theme::semibold(20.0)));
                            ui.add_space(12.0);
                            for (page, icon, label) in [
                                (Page::Appearance, Icon::Sketch, "Appearance"),
                                (Page::Canvas, Icon::Grid, "Canvas"),
                                (Page::Chat, Icon::Component, "Chat"),
                                (Page::Updates, Icon::Restore, "Updates"),
                            ] {
                                if kit::nav_item(ui, icon, label, st.settings.page == page) {
                                    st.settings.page = page;
                                }
                            }
                        });
                    });
                ui.vertical(|ui| {
                    ui.add_space(24.0);
                    ui.set_min_width(ui.available_width() - 24.0);
                    ui.horizontal(|ui| {
                        ui.add_space(24.0);
                        ui.vertical(|ui| {
                            ui.set_width(ui.available_width() - 8.0);
                            content(ui, ctx, st);
                        });
                    });
                });
            });
        });
    if live && (modal.should_close() || ctx.input(|i| i.key_pressed(egui::Key::Escape))) {
        st.settings.open = false;
    }
}

fn content(ui: &mut Ui, ctx: &egui::Context, st: &mut UiState) {
    let p = theme::pal(ui);
    ui.horizontal(|ui| {
        let (title, sub) = match st.settings.page {
            Page::Appearance => ("Appearance", "Theme, scale and motion."),
            Page::Canvas => ("Canvas", "How the canvas looks."),
            Page::Chat => ("Chat", "Agents and keys for design by conversation."),
            Page::Updates => ("Updates", "Keep PaperCraft current."),
        };
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.label(RichText::new(title).font(theme::semibold(20.0)));
            ui.label(RichText::new(sub).color(p.secondary));
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
            if kit::close_button(ui).clicked() {
                st.settings.open = false;
            }
        });
    });
    ui.add_space(16.0);
    match st.settings.page {
        Page::Appearance => {
            kit::section_header(ui, "Theme", None);
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(6.0, 8.0);
                for c in theme::Choice::ALL {
                    if kit::chip(ui, c.label(), theme::choice() == c).clicked() {
                        theme::apply_choice(ctx, c);
                        c.save();
                    }
                }
            });
            // In a browser the page zoom (Ctrl +/-) already does this.
            #[cfg(not(target_arch = "wasm32"))]
            {
                ui.add_space(12.0);
                kit::section_header(ui, "Interface scale", None);
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(6.0, 8.0);
                    for scale in theme::UI_SCALES {
                        let label = format!("{:.0}%", scale * 100.0);
                        if kit::chip(ui, &label, (theme::ui_scale() - scale).abs() < 1e-3).clicked()
                        {
                            theme::apply_ui_scale(ctx, scale, true);
                        }
                    }
                });
            }
            ui.add_space(12.0);
            let mut animations = motion::enabled(ctx);
            kit::setting_row(
                ui,
                "Animations",
                "Play motion on hover, open and close.",
                |ui| {
                    if kit::toggle(ui, &mut animations).changed() {
                        motion::set_enabled(ctx, animations);
                    }
                },
            );
        }
        Page::Canvas => {
            let mut dots = !st.show_grid_off;
            kit::setting_row(ui, "Dot grid", "Show the dotted canvas background.", |ui| {
                if kit::toggle(ui, &mut dots).changed() {
                    st.show_grid_off = !dots;
                }
            });
            ui.add_space(8.0);
            kit::setting_row(
                ui,
                "Keyboard shortcuts",
                "See every shortcut in one sheet.",
                |ui| {
                    if kit::button(ui, kit::ButtonKind::Secondary, None, "Open").clicked() {
                        st.settings.open = false;
                        st.show_help = true;
                    }
                },
            );
        }
        Page::Chat => chat_page(ui, ctx, &mut st.settings),
        Page::Updates => {
            kit::setting_row(
                ui,
                "Installed version",
                &format!("PaperCraft {}", env!("CARGO_PKG_VERSION")),
                |ui| {
                    #[cfg(not(target_arch = "wasm32"))]
                    if kit::button(ui, kit::ButtonKind::Primary, None, "Check for updates")
                        .clicked()
                    {
                        st.settings.open = false;
                        st.updater.open = true;
                    }
                    #[cfg(target_arch = "wasm32")]
                    ui.label("Reload the page to get the latest version.");
                },
            );
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn chat_page(ui: &mut Ui, _ctx: &egui::Context, _settings: &mut SettingsUi) {
    let p = theme::pal(ui);
    ui.label(RichText::new("Chat needs the PaperCraft desktop app.").color(p.secondary));
}

#[cfg(not(target_arch = "wasm32"))]
fn chat_page(ui: &mut Ui, ctx: &egui::Context, settings: &mut SettingsUi) {
    let p = theme::pal(ui);
    let SettingsUi { chat: prefs, chat_page: page, .. } = &mut *settings;
    let mut key_changed = false;
    let mut scan = page.scan.take().unwrap_or_else(|| Scan::read(prefs));
    let mut save = false;
    let mut rescan = false;

    kit::section_header(ui, "Local agents", None);
    ui.add_space(4.0);
    let claude_commit = agent_row(
        ui,
        ("Claude Code", CLAUDE_INSTALL, "claude"),
        scan.claude.as_ref(),
        &mut prefs.claude_path,
    );
    ui.add_space(6.0);
    let codex_commit = agent_row(
        ui,
        ("Codex", CODEX_INSTALL, "codex"),
        scan.codex.as_ref(),
        &mut prefs.codex_path,
    );
    if claude_commit || codex_commit {
        save = true;
        rescan = true;
    }
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new("Detected on PATH. Set a path to override.").size(theme::SECONDARY).color(p.secondary));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if kit::button(ui, kit::ButtonKind::Secondary, None, "Re-scan").clicked() {
                rescan = true;
            }
        });
    });

    ui.add_space(16.0);
    kit::section_header(ui, "API endpoints", None);
    ui.add_space(4.0);
    let note = |ui: &mut Ui, text: &str| {
        ui.label(RichText::new(text).size(theme::SECONDARY).color(p.secondary));
    };
    note(ui, "Any server that speaks the Anthropic Messages or the OpenAI Chat Completions format. A key is optional for local servers.");
    ui.add_space(6.0);
    let edits = endpoint_cards(ui, prefs, page, &mut scan);
    save |= edits.save;
    rescan |= edits.rescan;
    key_changed |= edits.key_changed;
    ui.add_space(6.0);
    if kit::button(ui, kit::ButtonKind::Secondary, Some(Icon::Plus), "Add endpoint").clicked() {
        prefs.endpoints.push(Endpoint::blank());
        save = true;
    }
    if let Some(id) = page.key_remove.clone() {
        let answer = confirm::dialog(
            ctx,
            "chat-key-remove",
            "Remove API key?",
            "The key is deleted from the Keychain. Chats on this endpoint stop until a key is saved again.",
            "Remove",
        );
        match answer {
            Some(true) => {
                page.key_remove = None;
                if let Some(ep) = prefs.endpoints.iter().find(|e| e.id == id) {
                    match endpoint::delete_key(ep) {
                        Ok(()) => {
                            key_changed = true;
                            scan.key_tails.remove(&id);
                            page.key_error = None;
                        }
                        Err(e) => page.key_error = Some(format!("Could not remove the key: {e}")),
                    }
                }
            }
            Some(false) => page.key_remove = None,
            None => {}
        }
    }
    if let Some(e) = &page.key_error {
        ui.label(RichText::new(e).color(ui.visuals().error_fg_color));
    }
    if !scan.key_tails.is_empty() {
        note(ui, &format!("Keys are stored in the {} and never written to the project or logs.", keychain_name()));
    }
    if scan.env_key {
        note(ui, &format!("{KEY_ENV} from the environment is used for api.anthropic.com when no key is stored."));
    }

    ui.add_space(16.0);
    kit::section_header(ui, "Defaults for new chats", None);
    ui.add_space(4.0);
    let w = ((ui.available_width() - 2.0 * DEFAULT_GAP) / 3.0).max(0.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = DEFAULT_GAP;
        ui.vertical(|ui| {
            ui.set_width(w);
            note(ui, "Backend");
            if let Some(b) = kit::select(ui, prefs.default_backend, &BACKENDS, w) {
                prefs.default_backend = b;
                save = true;
            }
        });
        ui.vertical(|ui| {
            ui.set_width(w);
            note(ui, "Endpoint (API)");
            let options: Vec<(usize, &str)> = prefs.endpoints.iter().enumerate().map(|(i, e)| (i, e.name.as_str())).collect();
            let current = prefs.endpoints.iter().position(|e| e.id == prefs.default_endpoint).unwrap_or(0);
            if let Some(i) = kit::select(ui, current, &options, w) {
                prefs.default_endpoint = prefs.endpoints[i].id.clone();
                save = true;
            }
        });
        ui.vertical(|ui| {
            ui.set_width(w);
            note(ui, "Effort (api.anthropic.com)");
            let effort = API_EFFORTS.iter().map(|(id, _)| *id).find(|id| *id == prefs.api_effort).unwrap_or(DEFAULT_EFFORT);
            if let Some(e) = kit::select(ui, effort, &API_EFFORTS, w) {
                prefs.api_effort = e.to_owned();
                save = true;
            }
        });
    });

    if save {
        prefs.save();
    }
    if rescan {
        scan = Scan::read(prefs);
    }
    page.scan = Some(scan);
    settings.chat_changed |= rescan || key_changed;
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Default)]
struct EndpointEdits {
    save: bool,
    rescan: bool,
    key_changed: bool,
}

#[cfg(not(target_arch = "wasm32"))]
fn edit(ui: &mut Ui, text: &mut String, hint: &str, width: f32, out: &mut EndpointEdits) {
    let r = ui.add(egui::TextEdit::singleline(text).margin(theme::INPUT_MARGIN).hint_text(hint).desired_width(width));
    out.save |= r.changed();
    out.rescan |= r.lost_focus();
}

/// One card per endpoint: name, default chip, remove; format and base URL; model; key.
/// Text edits save as you type and re-scan when the field loses focus.
#[cfg(not(target_arch = "wasm32"))]
fn endpoint_cards(ui: &mut Ui, prefs: &mut ChatPrefs, page: &mut ChatPage, scan: &mut Scan) -> EndpointEdits {
    let p = theme::pal(ui);
    let mut out = EndpointEdits::default();
    let mut remove = None;
    let mut make_default = None;
    let count = prefs.endpoints.len();
    for (i, ep) in prefs.endpoints.iter_mut().enumerate() {
        let is_default = ep.id == prefs.default_endpoint;
        let claude = ep.is_claude_host();
        egui::Frame::new().fill(p.backdrop).corner_radius(theme::RADIUS_LG).inner_margin(12).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 6.0;
            ui.horizontal(|ui| {
                let w = (ui.available_width() - 130.0).max(0.0);
                edit(ui, &mut ep.name, "Name", w, &mut out);
                if kit::chip(ui, if is_default { "Default" } else { "Make default" }, is_default).clicked() {
                    make_default = Some(ep.id.clone());
                }
                if count > 1 && theme::icon_button(ui, Icon::Trash, 22.0, false).on_hover_text("Remove endpoint").clicked() {
                    remove = Some(i);
                }
            });
            ui.horizontal(|ui| {
                if let Some(f) = kit::select(ui, ep.format, &FORMATS, FORMAT_W) {
                    ep.format = f;
                    out.save = true;
                    out.rescan = true;
                }
                let w = ui.available_width().max(0.0);
                edit(ui, &mut ep.base_url, "https://api.anthropic.com or http://127.0.0.1:11434/v1", w, &mut out);
            });
            let w = ui.available_width().max(0.0);
            edit(ui, &mut ep.model, "Model id", w, &mut out);
            let tail = scan.key_tails.get(&ep.id).cloned();
            let draft = page.key_drafts.entry(ep.id.clone()).or_default();
            let hint = match &tail {
                Some(t) => format!("{KEY_MASK}{t}"),
                None if claude => CLAUDE_KEY_PLACEHOLDER.to_string(),
                None => KEY_PLACEHOLDER.to_string(),
            };
            let mut save_clicked = false;
            ui.horizontal(|ui| {
                let w = (ui.available_width() - KEY_BUTTONS_W).max(0.0);
                ui.add(egui::TextEdit::singleline(draft).password(true).margin(theme::INPUT_MARGIN).hint_text(hint).desired_width(w));
                let can_save = !draft.trim().is_empty();
                save_clicked = ui.add_enabled_ui(can_save, |ui| kit::button(ui, kit::ButtonKind::Primary, None, "Save").clicked()).inner;
                if tail.is_some() && kit::button(ui, kit::ButtonKind::Danger, Some(Icon::Trash), "Remove").clicked() {
                    page.key_remove = Some(ep.id.clone());
                }
            });
            if save_clicked {
                match endpoint::set_key(ep, draft) {
                    Ok(()) => {
                        out.key_changed = true;
                        scan.key_tails.insert(ep.id.clone(), key_tail(draft.trim()));
                        draft.clear();
                        page.key_error = None;
                    }
                    Err(e) => page.key_error = Some(format!("Could not save the key: {e}")),
                }
            }
        });
        ui.add_space(6.0);
    }
    if let Some(id) = make_default {
        prefs.default_endpoint = id;
        out.save = true;
    }
    if let Some(i) = remove {
        let gone = prefs.endpoints.remove(i);
        // The key goes too, unless another endpoint uses the same base URL.
        let shared = prefs.endpoints.iter().any(|e| endpoint::normalize_url(&e.base_url) == endpoint::normalize_url(&gone.base_url));
        if !shared {
            let _ = endpoint::delete_key(&gone);
        }
        scan.key_tails.remove(&gone.id);
        page.key_drafts.remove(&gone.id);
        if prefs.default_endpoint == gone.id {
            prefs.default_endpoint = prefs.endpoints[0].id.clone();
        }
        out.save = true;
        out.rescan = true;
    }
    out
}

/// One CLI: name and version, the path field (empty means PATH), and a found or missing chip.
/// Returns true when the path was committed (the field lost focus), so the caller saves and rescans.
#[cfg(not(target_arch = "wasm32"))]
fn agent_row(ui: &mut Ui, (name, install, bin): (&str, &str, &str), found: Option<&Found>, path: &mut Option<String>) -> bool {
    let p = theme::pal(ui);
    let mut committed = false;
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.set_width(AGENT_NAME_W);
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.label(RichText::new(name).font(theme::semibold(theme::BODY)));
            if let Some((_, version)) = found {
                ui.label(RichText::new(format!("v{version}")).size(theme::SECONDARY).color(p.secondary));
            }
        });
        let hint = found.map_or_else(|| bin.to_string(), |(bin, _)| bin.display().to_string());
        let mut text = path.clone().unwrap_or_default();
        let field = ui.add(
            egui::TextEdit::singleline(&mut text)
                .hint_text(hint)
                .desired_width((ui.available_width() - AGENT_CHIP_W).max(0.0)),
        );
        if field.changed() {
            *path = (!text.is_empty()).then_some(text);
        }
        if field.lost_focus() {
            *path = path.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned);
            committed = true;
        }
        if found.is_some() {
            kit::chip_ink(ui, "Found", Some(p.success));
        } else {
            kit::chip_ink(ui, "Not found", None);
        }
    });
    if found.is_none() {
        ui.label(RichText::new(format!("Not found. Install: {install}")).size(theme::SECONDARY).color(p.secondary));
    }
    committed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::endpoint::ApiFormat;

    #[test]
    fn chat_prefs_round_trip_with_new_fields() {
        let prefs = ChatPrefs {
            claude_path: Some("/opt/homebrew/bin/claude".into()),
            codex_path: Some("/usr/local/bin/codex".into()),
            default_backend: BackendKind::Anthropic,
            endpoints: vec![
                Endpoint { model: "claude-haiku-5-5".into(), ..Default::default() },
                Endpoint { id: "ep-1".into(), name: "Local".into(), format: ApiFormat::OpenAi, base_url: "http://127.0.0.1:11434/v1".into(), model: "llama".into() },
            ],
            default_endpoint: "ep-1".into(),
            api_effort: "high".into(),
            api_model: None,
        };
        assert_eq!(ChatPrefs::from_json(&prefs.to_json()), prefs);
        assert_eq!(ChatPrefs::from_json(&ChatPrefs::default().to_json()), ChatPrefs::default());
    }

    #[test]
    fn old_model_and_effort_move_into_the_default_endpoint() {
        let prefs = ChatPrefs::from_json(r#"{"default_backend":"anthropic","api_model":"claude-haiku-5-5","api_effort":"high"}"#);
        assert_eq!(prefs.endpoints, vec![Endpoint { model: "claude-haiku-5-5".into(), ..Default::default() }]);
        assert_eq!((prefs.default_endpoint.as_str(), prefs.api_effort.as_str()), ("anthropic", "high"));
        assert_eq!(prefs.api_model, None);
        assert!(!prefs.to_json().contains("api_model"), "migrated once, not written back");
        // Saved and loaded again it is stable.
        assert_eq!(ChatPrefs::from_json(&prefs.to_json()), prefs);
        // With endpoints present the old model is ignored.
        let both = ChatPrefs::from_json(r#"{"api_model":"claude-haiku-5-5","endpoints":[{"id":"x","name":"X","base_url":"http://h","model":"m"}],"default_endpoint":"gone"}"#);
        assert_eq!((both.endpoints[0].model.as_str(), both.default_endpoint.as_str()), ("m", "x"));
        assert_eq!(both.endpoint(Some("nope")).id, "x");
    }

    #[test]
    fn chat_prefs_fall_back_to_defaults() {
        let prefs = ChatPrefs::from_json(r#"{"api_model":"gpt-x","api_effort":"max"}"#);
        assert_eq!(prefs.endpoints[0].model, DEFAULT_MODEL);
        assert_eq!(prefs.api_effort, DEFAULT_EFFORT);
        assert_eq!(ChatPrefs::from_json(r#"{"endpoints":[]}"#).endpoints, vec![Endpoint::default()]);
        assert_eq!(prefs.default_backend, BackendKind::ClaudeCode);
        assert_eq!(ChatPrefs::from_json("not json"), ChatPrefs::default());
    }

    /// Seeds the scan so the page renders with no CLI on the machine and no key, without
    /// spawning processes or reading the Keychain.
    #[test]
    fn chat_page_draws_with_no_clis_and_no_key() {
        let ctx = egui::Context::default();
        crate::ui::setup_style(&ctx);
        let mut st = UiState::default();
        st.settings = SettingsUi {
            open: true,
            page: Page::Chat,
            chat: ChatPrefs::default(),
            chat_changed: false,
            chat_page: ChatPage {
                scan: Some(Scan { claude: None, codex: None, key_tails: Default::default(), env_key: false }),
                ..ChatPage::default()
            },
        };
        let mut ed = crate::editor::Editor::default();
        for _ in 0..3 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 800.0))),
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| {
                crate::ui::draw(ui, &mut ed, &mut st);
            });
            out.textures_delta.clear();
        }
        assert!(st.settings.chat_page.scan.is_some());
    }
}
