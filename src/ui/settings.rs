//! Settings dialog: appearance, canvas, chat agents and keys, updates.

use super::{UiState, kit, motion};
use crate::chat::BackendKind;
use crate::chat::endpoint::{self, Endpoint};
use crate::theme::{self, Icon};
use egui::{RichText, Ui};
use std::collections::BTreeMap;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    #[default]
    Appearance,
    Canvas,
    Chat,
    Plugins,
    Updates,
}

pub const API_MODELS: [(&str, &str); 3] = [
    ("claude-opus-5-5", "Opus 5.5"),
    ("claude-sonnet-5-5", "Sonnet 5.5"),
    ("claude-haiku-5-5", "Haiku 5.5"),
];
pub const API_EFFORTS: [(&str, &str); 5] = [("low", "Low"), ("medium", "Medium"), ("high", "High"), ("xhigh", "X-High"), ("max", "Max")];
/// Fixed size of the dialog, the same on every page; long pages scroll inside it.
const MODAL_W: f32 = 880.0;
const MODAL_H: f32 = 640.0;
/// Top padding, page header and bottom padding above and below a scrolling page.
const PAGE_CHROME: f32 = 112.0;
fn modal_height(ctx: &egui::Context) -> f32 {
    MODAL_H.min(ctx.content_rect().height() - 96.0)
}
const DEFAULT_MODEL: &str = "claude-opus-5-5";
const DEFAULT_EFFORT: &str = "medium";
const ISSUE_REPO: &str = "https://git.marcospaulo.dev.br/masi/papercraft";
/// Generous: a long build (eight screens) read about 5M tokens.
const DEFAULT_TURN_BUDGET: u64 = 10_000_000;
pub const TURN_BUDGETS: [(u64, &str); 5] = [(0, "No limit"), (2_000_000, "2M tokens"), (5_000_000, "5M tokens"), (10_000_000, "10M tokens"), (20_000_000, "20M tokens")];
#[cfg(not(target_arch = "wasm32"))]
const BACKENDS: [(BackendKind, &str); 3] = [
    (BackendKind::ClaudeCode, "Claude Code"),
    (BackendKind::Codex, "Codex"),
    (BackendKind::Anthropic, "API"),
];

/// One registry agent's settings. Native agents are always on; `enabled` gates `npx` ones
/// (running them fetches code, so the user opts in once).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AgentPrefs {
    pub path: Option<String>,
    /// Replaces the registry's launch args, split on whitespace.
    pub args: Option<String>,
    pub enabled: bool,
    /// Native agents only: left out of the chat agent picker.
    pub hidden: bool,
}

/// An ACP agent the user added: `command` runs with `args` (whitespace-split) over stdio.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CustomAgent {
    pub id: String,
    pub name: String,
    pub command: String,
    pub args: String,
}

/// Chat defaults and agent paths, kept in `chat_settings.json` in the config folder.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ChatPrefs {
    /// Old home of `agents["claude-code"].path`: read from old files, mirrored from `agents`, never written.
    #[serde(skip_serializing)]
    pub claude_path: Option<String>,
    /// Old home of `agents["codex"].path`: read from old files, mirrored from `agents`, never written.
    #[serde(skip_serializing)]
    pub codex_path: Option<String>,
    pub default_backend: BackendKind,
    /// Per-agent overrides by registry id. `from_json` moves the old `claude_path`/`codex_path` in.
    pub agents: BTreeMap<String, AgentPrefs>,
    /// User-defined ACP agents (name + command + args).
    pub custom_agents: Vec<CustomAgent>,
    /// API endpoints, at least one. Keys are in the keychain, never here.
    pub endpoints: Vec<Endpoint>,
    /// Id of the endpoint new chats start on.
    pub default_endpoint: String,
    /// One of [`API_EFFORTS`]; sent to `api.anthropic.com` only.
    pub api_effort: String,
    /// Old settings files: read once into the default endpoint's model, never written.
    #[serde(skip_serializing)]
    pub api_model: Option<String>,
    /// Send the design-system summary (`chat::ds_digest`) with chat turns. It costs tokens.
    pub ds_summary: bool,
    /// Claude Code runs without the user's CLAUDE.md, hooks, plugins and skills, with only the
    /// tools a design chat can use: about half the prompt (specs/chat-tokens).
    pub cli_lean: bool,
    /// The agent may search the web and bring back reference images (the composer's Web chip).
    pub research: bool,
    /// Stop a turn once its model calls have read this many tokens in all (0: no limit). A guard
    /// against a runaway loop; one of [`TURN_BUDGETS`].
    pub turn_budget: u64,
    /// Where `generate_image` gets images: "" (off), "codex" or "command".
    pub image_gen: String,
    /// The "command" provider's shell template (`{prompt}`, `{aspect}`, `{out}`).
    pub image_command: String,
    /// The user answered the chat's offer to use Codex for images.
    pub image_offer_seen: bool,
    /// Plugins turned on (`plugins::PLUGINS` ids); all off by default.
    pub plugins: Vec<String>,
    /// Where PaperCraft's own issues go (the form and the agent's drafts).
    pub issue_repo: String,
    /// The helper agent `delegate` runs (`{prompt}`); empty: no helper, the chat agent does it all.
    pub helper_command: String,
    /// Auto mode (the composer's Auto chip): a turn that ends on questions is answered with the
    /// recommended options and the chat goes on, until Stop or the turn budget.
    pub auto: bool,
}

impl Default for ChatPrefs {
    fn default() -> Self {
        Self {
            claude_path: None,
            codex_path: None,
            default_backend: BackendKind::ClaudeCode,
            agents: BTreeMap::new(),
            custom_agents: vec![],
            endpoints: vec![Endpoint::default()],
            default_endpoint: endpoint::DEFAULT_ENDPOINT_ID.into(),
            api_effort: DEFAULT_EFFORT.into(),
            api_model: None,
            ds_summary: true,
            cli_lean: true,
            research: false,
            turn_budget: DEFAULT_TURN_BUDGET,
            image_gen: String::new(),
            image_command: String::new(),
            image_offer_seen: false,
            plugins: Vec::new(),
            issue_repo: ISSUE_REPO.into(),
            helper_command: String::new(),
            auto: false,
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
        for (id, old) in [("claude-code", prefs.claude_path.clone()), ("codex", prefs.codex_path.clone())] {
            let entry = prefs.agents.entry(id.into()).or_default();
            if entry.path.is_none() {
                entry.path = old;
            }
        }
        prefs.tidy();
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

    /// Drops empty agent entries and mirrors the Claude Code and Codex paths into the old fields
    /// that code outside Settings may still read.
    pub fn tidy(&mut self) {
        self.agents.retain(|_, a| *a != AgentPrefs::default());
        self.claude_path = self.agents.get("claude-code").and_then(|a| a.path.clone());
        self.codex_path = self.agents.get("codex").and_then(|a| a.path.clone());
    }

    /// The endpoint with this id; None or an unknown id means the default one.
    pub fn endpoint(&self, id: Option<&str>) -> &Endpoint {
        id.and_then(|id| self.endpoints.iter().find(|e| e.id == id)).unwrap_or_else(|| self.default_endpoint())
    }

    /// The image provider the user turned on (off by default).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn image_provider(&self) -> crate::mcp::imagegen::Provider {
        use crate::mcp::imagegen::Provider;
        match self.image_gen.as_str() {
            "codex" => Provider::Codex,
            "gf" => Provider::Command(crate::mcp::imagegen::GF_COMMAND.into()),
            "command" if !self.image_command.trim().is_empty() => Provider::Command(self.image_command.clone()),
            _ => Provider::Off,
        }
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
    chat_page: native::ChatPage,
    /// A token typed on the Updates page, until saved to the keychain.
    issue_token: String,
    issue_note: Option<String>,
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
            chat_page: native::ChatPage::default(),
            issue_token: String::new(),
            issue_note: None,
        }
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
            ui.set_width(MODAL_W.min(ctx.content_rect().width() - 48.0));
            ui.set_height(modal_height(ctx));
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
                                (Page::Plugins, Icon::Library, "Plugins"),
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

/// The plugin catalogue: a card per plugin, two to a row and of one size, each with its switch.
#[cfg(not(target_arch = "wasm32"))]
fn plugins_page(ui: &mut Ui, prefs: &mut ChatPrefs) {
    let p = theme::pal(ui);
    let mut changed = false;
    let w = (ui.available_width() - PLUGIN_GAP) / 2.0;
    ui.spacing_mut().item_spacing = egui::vec2(PLUGIN_GAP, PLUGIN_GAP);
    for row in crate::plugins::PLUGINS.chunks(2) {
        ui.horizontal(|ui| {
            for plugin in row {
                let mut on = prefs.plugins.iter().any(|id| id == plugin.id);
                ui.allocate_ui_with_layout(egui::vec2(w, PLUGIN_CARD_H), egui::Layout::top_down(egui::Align::Min), |ui| {
                    egui::Frame::new().fill(p.backdrop).corner_radius(theme::RADIUS_PANEL).inner_margin(egui::Margin::same(14)).show(ui, |ui| {
                        ui.set_width(w - 28.0);
                        ui.set_height(PLUGIN_CARD_H - 28.0);
                        ui.spacing_mut().item_spacing.y = 4.0;
                        ui.horizontal(|ui| {
                            kit::badge(ui, plugin_icon(plugin.id), if on { theme::accent() } else { p.secondary }, 32.0);
                            ui.label(RichText::new(plugin.name).font(theme::semibold(theme::BODY + 1.0)));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if kit::toggle(ui, &mut on).changed() {
                                    prefs.plugins.retain(|id| id != plugin.id);
                                    if on {
                                        prefs.plugins.push(plugin.id.into());
                                    }
                                    changed = true;
                                }
                            });
                        });
                        ui.add(egui::Label::new(RichText::new(plugin.about).color(p.text)).wrap());
                        ui.add(egui::Label::new(RichText::new(plugin.license).size(theme::SECONDARY).color(p.tertiary)).wrap());
                    });
                });
            }
        });
    }
    if changed {
        prefs.save();
    }
}

/// Plugin cards: one height for all (three lines of text under the title) and the gap between.
#[cfg(not(target_arch = "wasm32"))]
const PLUGIN_CARD_H: f32 = 132.0;
#[cfg(not(target_arch = "wasm32"))]
const PLUGIN_GAP: f32 = 12.0;

#[cfg(not(target_arch = "wasm32"))]
fn plugin_icon(id: &str) -> Icon {
    match id {
        "avataaars" => Icon::Sketch,
        "dicebear" => Icon::Component,
        "iconify" => Icon::Star,
        "picsum" => Icon::Image,
        "flags" => Icon::Note,
        _ => Icon::Grid,
    }
}

fn content(ui: &mut Ui, ctx: &egui::Context, st: &mut UiState) {
    let p = theme::pal(ui);
    ui.horizontal(|ui| {
        let (title, sub) = match st.settings.page {
            Page::Appearance => ("Appearance", "Theme, scale and motion."),
            Page::Canvas => ("Canvas", "How the canvas looks."),
            Page::Chat => ("Chat", "Agents, API endpoints and defaults for design by conversation."),
            Page::Plugins => ("Plugins", "Assets the agent can place in a design. Fetched from each service once, then cached."),
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
        Page::Chat => {
            // The columns have no fixed height of their own, so the page gets the dialog's.
            let height = modal_height(ctx) - PAGE_CHROME;
            egui::ScrollArea::vertical().auto_shrink([false, false]).max_height(height).min_scrolled_height(height).show(ui, |ui| chat_page(ui, ctx, &mut st.settings));
        }
        Page::Plugins => {
            #[cfg(not(target_arch = "wasm32"))]
            plugins_page(ui, &mut st.settings.chat);
        }
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
            #[cfg(not(target_arch = "wasm32"))]
            {
                ui.add_space(16.0);
                kit::section_header(ui, "Issues", None);
                ui.label(RichText::new("Where \"Report a PaperCraft issue\" and the agent's issue drafts go. Nothing is sent without your Send.").color(theme::pal(ui).secondary));
                ui.add_space(6.0);
                let s = &mut st.settings;
                kit::setting_row(ui, "Repository", "Gitea or GitHub repository URL.", |ui| {
                    if ui.add(egui::TextEdit::singleline(&mut s.chat.issue_repo).desired_width(320.0).margin(theme::INPUT_MARGIN)).lost_focus() {
                        s.chat.save();
                    }
                });
                let saved = crate::issues::token().is_some();
                let desc = if saved { "Saved in the keychain." } else { "Optional: without one, your tea login for this server is used." };
                kit::setting_row(ui, "Access token", desc, |ui| {
                    ui.add(egui::TextEdit::singleline(&mut s.issue_token).password(true).hint_text(if saved { "•••• saved" } else { "token" }).desired_width(200.0).margin(theme::INPUT_MARGIN));
                    if kit::button(ui, kit::ButtonKind::Secondary, None, if s.issue_token.is_empty() && saved { "Remove" } else { "Save" }).clicked() {
                        s.issue_note = Some(match crate::issues::set_token(&s.issue_token) {
                            Ok(()) => if s.issue_token.is_empty() { "Token removed".into() } else { "Token saved".into() },
                            Err(e) => e,
                        });
                        s.issue_token.clear();
                    }
                });
                if let Some(n) = &st.settings.issue_note {
                    ui.label(RichText::new(n).size(theme::SECONDARY).color(theme::pal(ui).secondary));
                }
            }
        }
    }
}

/// The desktop Chat page. Detection runs on a thread; the page only draws what it has.
#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::*;
    use crate::chat::agents::{self, AgentKind, AgentState, AgentStatus};
    use crate::chat::endpoint::ApiFormat;
    use crate::ui::confirm;
    use egui::{Align, Layout, vec2};
    use std::collections::{HashMap, HashSet};
    use std::sync::mpsc::{Receiver, channel};
    use std::time::Duration;

    const KEY_ENV: &str = "ANTHROPIC_API_KEY";
    const KEY_TAIL_CHARS: usize = 4;
    const KEY_MASK: &str = "•••• ";
    const KEY_PLACEHOLDER: &str = "API key (optional for a local server)";
    const CLAUDE_KEY_PLACEHOLDER: &str = "sk-ant-…";
    /// One grid for every field row: a label column, then the field.
    const LABEL_W: f32 = 160.0;
    const CARD_PAD: i8 = 16;
    const ROW_PAD: i8 = 12;
    const SECTION_GAP: f32 = 24.0;
    const ROW_H: f32 = 44.0;
    const ICON_BADGE: f32 = 40.0;
    /// Room for the status chip, toggle and chevron at the right of a row.
    const RIGHT_W: f32 = 270.0;
    const SCROLLBAR_GAP: f32 = 14.0;
    const POLL_EVERY: Duration = Duration::from_millis(100);
    const FORMATS: [(ApiFormat, &str); 2] = [(ApiFormat::Anthropic, "Anthropic Messages"), (ApiFormat::OpenAi, "OpenAI-compatible")];

    /// Chat page state that lives only while the dialog is open.
    #[derive(Default)]
    pub(super) struct ChatPage {
        /// Set when the page is first shown after the dialog opened; cleared when the dialog closes.
        pub(super) scan: Option<Scan>,
        /// Typed keys by endpoint id.
        pub(super) key_drafts: HashMap<String, String>,
        pub(super) key_error: Option<String>,
        /// The endpoint whose key waits for the remove confirmation.
        pub(super) key_remove: Option<String>,
        /// The custom agent that waits for the remove confirmation.
        pub(super) agent_remove: Option<String>,
        /// Expanded rows, as `agent:<id>` or `ep:<id>`.
        pub(super) open: HashSet<String>,
        pub(super) show_more: bool,
    }

    /// What the page shows about the agents and the stored keys. Keys themselves are never kept.
    pub(super) struct Scan {
        /// None until the first detection finishes.
        pub(super) agents: Option<Vec<AgentStatus>>,
        pub(super) probe: Option<Receiver<Vec<AgentStatus>>>,
        /// Last four characters of each endpoint's key (by id).
        pub(super) key_tails: HashMap<String, String>,
        pub(super) env_key: bool,
    }

    impl Scan {
        fn read(prefs: &ChatPrefs, ctx: &egui::Context) -> Scan {
            let mut scan = Scan { agents: None, probe: None, key_tails: HashMap::new(), env_key: false };
            scan.refresh(prefs, ctx);
            scan
        }

        /// Detects the agents again (the old result stays on screen until the new one lands) and
        /// reads the key tails.
        fn refresh(&mut self, prefs: &ChatPrefs, ctx: &egui::Context) {
            let (tx, rx) = channel();
            let (prefs_copy, ctx_copy) = (prefs.clone(), ctx.clone());
            std::thread::spawn(move || {
                let _ = tx.send(agents::detect_all(&prefs_copy));
                ctx_copy.request_repaint();
            });
            self.probe = Some(rx);
            self.key_tails = prefs.endpoints.iter().filter_map(|e| Some((e.id.clone(), key_tail(&endpoint::get_key(e)?)))).collect();
            self.env_key = std::env::var(KEY_ENV).is_ok();
        }

        fn poll(&mut self) {
            let Some(rx) = &self.probe else { return };
            if let Ok(found) = rx.try_recv() {
                self.agents = Some(found);
                self.probe = None;
            }
        }
    }

    fn key_tail(key: &str) -> String {
        let chars: Vec<char> = key.chars().collect();
        chars[chars.len().saturating_sub(KEY_TAIL_CHARS)..].iter().collect()
    }

    /// Where the platform keeps the API key, for the settings note.
    fn keychain_name() -> &'static str {
        if cfg!(target_os = "macos") {
            "macOS Keychain"
        } else if cfg!(windows) {
            "Windows Credential Manager"
        } else {
            "Secret Service"
        }
    }

    /// What the frame changed.
    #[derive(Default)]
    struct Edits {
        save: bool,
        /// Detect again and reread key tails.
        rescan: bool,
        /// Tell the chat column without detecting here (a toggle or a key).
        notify: bool,
    }

    /// One agent as the list shows it.
    struct AgentRow {
        id: String,
        name: String,
        /// None for a custom agent.
        kind: Option<AgentKind>,
        state: AgentState,
        hidden: bool,
    }

    /// Ready and not hidden by the user: shown without the "Show more" click.
    pub(super) fn shown_by_default(state: &AgentState, hidden: bool) -> bool {
        matches!(state, AgentState::Ready { .. }) && !hidden
    }

    /// Splits rows flagged `true` (shown by default) from the rest, keeping their order.
    pub(super) fn split_primary<T>(rows: impl IntoIterator<Item = (T, bool)>) -> (Vec<T>, Vec<T>) {
        let (primary, more): (Vec<_>, Vec<_>) = rows.into_iter().partition(|(_, shown)| *shown);
        (primary.into_iter().map(|(r, _)| r).collect(), more.into_iter().map(|(r, _)| r).collect())
    }

    fn agent_rows(prefs: &ChatPrefs, statuses: &[AgentStatus]) -> Vec<AgentRow> {
        let state_of = |id: &str| statuses.iter().find(|s| s.id == id).map_or(AgentState::NotFound, |s| s.state.clone());
        let hidden_of = |id: &str| prefs.agents.get(id).is_some_and(|a| a.hidden);
        let registry = agents::AGENTS.iter().map(|d| AgentRow { id: d.id.into(), name: d.name.into(), kind: Some(d.kind), state: state_of(d.id), hidden: hidden_of(d.id) });
        let custom = prefs.custom_agents.iter().map(|c| AgentRow { id: c.id.clone(), name: c.name.clone(), kind: None, state: state_of(&c.id), hidden: false });
        registry.chain(custom).collect()
    }

    pub(super) fn chat_page(ui: &mut Ui, ctx: &egui::Context, settings: &mut SettingsUi) {
        let SettingsUi { chat: prefs, chat_page: page, .. } = &mut *settings;
        let mut scan = page.scan.take().unwrap_or_else(|| Scan::read(prefs, ctx));
        scan.poll();
        if scan.probe.is_some() {
            ctx.request_repaint_after(POLL_EVERY);
        }
        let mut ed = Edits::default();
        ui.set_max_width(ui.available_width() - SCROLLBAR_GAP);
        ui.spacing_mut().item_spacing.y = 8.0;

        let mut detect = false;
        section(ui, "Agents", "Found on your login shell PATH, like your terminal does.", |ui| {
            detect = kit::button(ui, kit::ButtonKind::Secondary, Some(Icon::Restore), "Detect again").clicked();
        });
        card(ui, |ui| agents_card(ui, prefs, page, &scan, &mut ed));

        ui.add_space(SECTION_GAP - 8.0);
        let mut add_endpoint = false;
        section(ui, "API endpoints", &format!("Bring your own key. Keys live in the {}.", keychain_name()), |ui| {
            add_endpoint = kit::button(ui, kit::ButtonKind::Secondary, Some(Icon::Plus), "Add endpoint").clicked();
        });
        card(ui, |ui| endpoints_card(ui, prefs, page, &mut scan, &mut ed));
        if add_endpoint {
            let ep = Endpoint::blank();
            page.open.insert(format!("ep:{}", ep.id));
            prefs.endpoints.push(ep);
            ed.save = true;
        }
        if let Some(e) = &page.key_error {
            ui.label(RichText::new(e).color(ui.visuals().error_fg_color));
        }
        if scan.env_key {
            note(ui, &format!("{KEY_ENV} from the environment is used for api.anthropic.com when no key is stored."));
        }

        ui.add_space(SECTION_GAP - 8.0);
        section(ui, "Defaults for new chats", "Each chat can still pick its own.", |_| {});
        card(ui, |ui| defaults_card(ui, prefs, &mut ed));
        ui.add_space(SECTION_GAP);

        confirm_removals(ctx, prefs, page, &mut scan, &mut ed);
        if detect {
            ed.rescan = true;
        }
        if ed.save {
            prefs.tidy();
            prefs.save();
        }
        if ed.rescan {
            scan.refresh(prefs, ctx);
        }
        page.scan = Some(scan);
        settings.chat_changed |= ed.rescan || ed.notify;
    }

    fn confirm_removals(ctx: &egui::Context, prefs: &mut ChatPrefs, page: &mut ChatPage, scan: &mut Scan, ed: &mut Edits) {
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
                                ed.notify = true;
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
        if let Some(id) = page.agent_remove.clone() {
            let name = prefs.custom_agents.iter().find(|c| c.id == id).map_or("this agent", |c| c.name.as_str()).to_owned();
            match confirm::dialog(ctx, "chat-agent-remove", "Remove custom agent?", &format!("{name} is removed from the list. Chats that use it stop until you pick another agent."), "Remove") {
                Some(true) => {
                    page.agent_remove = None;
                    prefs.custom_agents.retain(|c| c.id != id);
                    page.open.remove(&format!("agent:{id}"));
                    ed.save = true;
                    ed.rescan = true;
                }
                Some(false) => page.agent_remove = None,
                None => {}
            }
        }
    }

    // ---- layout pieces ----

    /// Section title and subtitle on the left, `action` at the right.
    fn section(ui: &mut Ui, title: &str, sub: &str, action: impl FnOnce(&mut Ui)) {
        let p = theme::pal(ui);
        ui.allocate_ui_with_layout(vec2(ui.available_width(), 40.0), Layout::left_to_right(Align::Center), |ui| {
            ui.set_min_height(40.0);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                ui.label(RichText::new(title).font(theme::semibold(16.0)));
                ui.label(RichText::new(sub).size(12.0).color(p.secondary));
            });
            ui.with_layout(Layout::right_to_left(Align::Center), action);
        });
    }

    /// The raised surface of a section, 16 px inside.
    fn card(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
        let p = theme::pal(ui);
        egui::Frame::new().fill(p.backdrop).corner_radius(theme::RADIUS_PANEL).inner_margin(CARD_PAD).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 4.0;
            add(ui);
        });
    }

    /// A row inside a card; lighter while expanded.
    fn row_frame(ui: &mut Ui, open: bool, add: impl FnOnce(&mut Ui)) {
        let p = theme::pal(ui);
        let fill = if open { theme::blend(p.backdrop, p.hover) } else { egui::Color32::TRANSPARENT };
        egui::Frame::new().fill(fill).corner_radius(theme::RADIUS_LG).inner_margin(ROW_PAD).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 8.0;
            add(ui);
        });
    }

    /// Icon, name (with version), a muted line, and `right` laid out from the right edge.
    fn row_header(ui: &mut Ui, icon: Icon, title: &str, version: &str, sub: &str, right: impl FnOnce(&mut Ui)) {
        let p = theme::pal(ui);
        ui.allocate_ui_with_layout(vec2(ui.available_width(), ROW_H), Layout::left_to_right(Align::Center), |ui| {
            ui.set_min_height(ROW_H);
            ui.spacing_mut().item_spacing.x = 12.0;
            let left_w = (ui.available_width() - RIGHT_W).max(0.0);
            ui.allocate_ui_with_layout(vec2(left_w, ROW_H), Layout::left_to_right(Align::Center), |ui| {
                ui.set_width(left_w);
                kit::badge(ui, icon, p.secondary, ICON_BADGE);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        ui.label(RichText::new(title).font(theme::semibold(theme::BODY + 1.0)));
                        if !version.is_empty() {
                            ui.add(egui::Label::new(RichText::new(version).size(theme::SECONDARY).color(p.secondary)).truncate());
                        }
                    });
                    ui.add(egui::Label::new(RichText::new(sub).size(theme::SECONDARY).color(p.secondary)).truncate());
                });
            });
            ui.with_layout(Layout::right_to_left(Align::Center), right);
        });
    }

    /// A label in the 160 px column and the field after it.
    fn field_row(ui: &mut Ui, label: &str, add: impl FnOnce(&mut Ui)) {
        let p = theme::pal(ui);
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.allocate_ui_with_layout(vec2(LABEL_W, theme::FIELD_H), Layout::left_to_right(Align::Center), |ui| {
                ui.set_min_size(vec2(LABEL_W, theme::FIELD_H));
                ui.label(RichText::new(label).color(p.secondary));
            });
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 6.0;
                ui.set_width(ui.available_width());
                add(ui);
            });
        });
    }

    fn note(ui: &mut Ui, text: &str) {
        ui.label(RichText::new(text).size(theme::SECONDARY).color(theme::pal(ui).secondary));
    }

    fn text_field(ui: &mut Ui, text: &mut String, hint: &str) -> egui::Response {
        ui.add(egui::TextEdit::singleline(text).margin(theme::INPUT_MARGIN).hint_text(hint).desired_width(ui.available_width()))
    }

    /// A text field for an optional string: edits show at once, `Enter` or leaving the field
    /// commits (save and detect again). An empty field is `None`.
    fn optional_field(ui: &mut Ui, value: &mut Option<String>, hint: &str, ed: &mut Edits) {
        let mut text = value.clone().unwrap_or_default();
        let r = text_field(ui, &mut text, hint);
        if r.changed() {
            *value = (!text.is_empty()).then_some(text);
        }
        if r.lost_focus() {
            *value = value.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned);
            ed.save = true;
            ed.rescan = true;
        }
    }

    /// A plain string field: saves as you type, detects again when it loses focus.
    fn text_edit(ui: &mut Ui, value: &mut String, hint: &str, ed: &mut Edits) {
        let r = text_field(ui, value, hint);
        ed.save |= r.changed();
        ed.rescan |= r.lost_focus();
    }

    fn chevron(ui: &mut Ui, open: bool) -> bool {
        theme::icon_button(ui, if open { Icon::ChevronUp } else { Icon::ChevronDown }, 24.0, false).clicked()
    }

    fn flip(open: &mut HashSet<String>, key: String, clicked: bool) {
        if clicked && !open.remove(&key) {
            open.insert(key);
        }
    }

    /// `~` for the home folder, so paths stay short.
    fn tilde(path: &str) -> String {
        match dirs::home_dir().and_then(|h| path.strip_prefix(h.to_str()?).map(str::to_owned)) {
            Some(rest) => format!("~{rest}"),
            None => path.to_owned(),
        }
    }

    /// `v2.1.307` out of whatever the program prints for `--version`; empty when no number is in it.
    fn short_version(raw: &str) -> String {
        raw.split_whitespace()
            .find(|t| t.trim_start_matches('v').starts_with(|c: char| c.is_ascii_digit()))
            .map_or_else(String::new, |t| format!("v{}", t.trim_start_matches('v')))
    }

    // ---- agents ----

    fn agents_card(ui: &mut Ui, prefs: &mut ChatPrefs, page: &mut ChatPage, scan: &Scan, ed: &mut Edits) {
        let Some(statuses) = &scan.agents else {
            note(ui, "Looking for agents…");
            return;
        };
        let rows = agent_rows(prefs, statuses);
        let (primary, more) = split_primary(rows.into_iter().map(|r| {
            let shown = shown_by_default(&r.state, r.hidden);
            (r, shown)
        }));
        for row in &primary {
            agent_row(ui, row, prefs, page, ed);
        }
        if !more.is_empty() {
            let text = if page.show_more { "Show fewer agents".to_owned() } else { format!("Show {} more agents", more.len()) };
            ui.add_space(4.0);
            let link = ui.add(egui::Label::new(RichText::new(text).color(theme::accent())).sense(egui::Sense::click()));
            if link.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                page.show_more = !page.show_more;
            }
            if page.show_more {
                for row in &more {
                    agent_row(ui, row, prefs, page, ed);
                }
            }
        }
        ui.add_space(8.0);
        if kit::button(ui, kit::ButtonKind::Secondary, Some(Icon::Plus), "Add custom agent").clicked() {
            let id = format!("custom-{:08x}", fastrand::u32(..));
            page.open.insert(format!("agent:{id}"));
            prefs.custom_agents.push(CustomAgent { id, name: "New agent".into(), ..Default::default() });
            page.show_more = true;
            ed.save = true;
        }
    }

    fn agent_row(ui: &mut Ui, row: &AgentRow, prefs: &mut ChatPrefs, page: &mut ChatPage, ed: &mut Edits) {
        let p = theme::pal(ui);
        let key = format!("agent:{}", row.id);
        let open = page.open.contains(&key);
        let custom_command = prefs.custom_agents.iter().find(|c| c.id == row.id).map(|c| c.command.clone());
        let (version, sub) = match &row.state {
            AgentState::Ready { path, version } => (short_version(version), tilde(path)),
            AgentState::NeedsEnable => (String::new(), "Runs via npx · off until enabled".to_owned()),
            AgentState::NotFound => (String::new(), custom_command.filter(|c| !c.is_empty()).unwrap_or_else(|| "Not found on PATH".to_owned())),
            AgentState::WrongProgram { path } => (String::new(), tilde(path)),
        };
        let (label, ink) = match &row.state {
            AgentState::Ready { .. } => ("Ready", Some(p.success)),
            AgentState::NeedsEnable => ("Needs enable", None),
            AgentState::NotFound => ("Not found", None),
            AgentState::WrongProgram { .. } => ("Different program", Some(p.warning)),
        };
        let npx = matches!(row.kind, Some(AgentKind::Npx { .. }));
        let enabled = prefs.agents.get(&row.id).is_some_and(|a| a.enabled);
        let ready = matches!(row.state, AgentState::Ready { .. });
        let mut clicked = false;
        row_frame(ui, open, |ui| {
            row_header(ui, Icon::Component, &row.name, &version, &sub, |ui| {
                clicked = chevron(ui, open);
                if npx && enabled {
                    let mut on = true;
                    if kit::toggle(ui, &mut on).changed() {
                        prefs.agents.entry(row.id.clone()).or_default().enabled = false;
                        ed.save = true;
                        ed.rescan = true;
                    }
                } else if npx {
                    if kit::button(ui, kit::ButtonKind::Secondary, None, "Enable").clicked() {
                        prefs.agents.entry(row.id.clone()).or_default().enabled = true;
                        ed.save = true;
                        ed.rescan = true;
                    }
                } else if row.kind.is_some() && ready {
                    let mut on = !row.hidden;
                    if kit::toggle(ui, &mut on).on_hover_text("Show in the chat agent picker").changed() {
                        prefs.agents.entry(row.id.clone()).or_default().hidden = !on;
                        ed.save = true;
                        ed.notify = true;
                    }
                }
                kit::chip_ink(ui, label, ink);
            });
            if !open {
                return;
            }
            match row.kind {
                Some(_) => registry_body(ui, row, prefs, ed),
                None => custom_body(ui, row, prefs, page, ed),
            }
        });
        flip(&mut page.open, key, clicked);
    }

    fn registry_body(ui: &mut Ui, row: &AgentRow, prefs: &mut ChatPrefs, ed: &mut Edits) {
        let Some(def) = agents::find(&row.id) else { return };
        let direct = matches!(def.kind, AgentKind::Direct);
        field_row(ui, "Path override", |ui| {
            let entry = prefs.agents.entry(row.id.clone()).or_default();
            optional_field(ui, &mut entry.path, "Auto (found on PATH)", ed);
            note(ui, "Empty uses the program found on your login shell PATH. A path here wins.");
        });
        if !direct {
            field_row(ui, "Arguments override", |ui| {
                let hint = if def.launch.is_empty() { "Optional".to_owned() } else { def.launch.join(" ") };
                optional_field(ui, &mut prefs.agents.entry(row.id.clone()).or_default().args, &hint, ed);
            });
        }
        field_row(ui, "Documentation", |ui| {
            ui.hyperlink_to(def.docs_url.trim_start_matches("https://"), def.docs_url);
        });
        if matches!(def.kind, AgentKind::Npx { .. }) {
            field_row(ui, "Security", |ui| {
                ui.label(RichText::new("Runs code fetched by npx").size(theme::SECONDARY).color(theme::pal(ui).warning));
            });
        }
    }

    fn custom_body(ui: &mut Ui, row: &AgentRow, prefs: &mut ChatPrefs, page: &mut ChatPage, ed: &mut Edits) {
        let Some(c) = prefs.custom_agents.iter_mut().find(|c| c.id == row.id) else { return };
        field_row(ui, "Name", |ui| text_edit(ui, &mut c.name, "Name", ed));
        field_row(ui, "Command", |ui| {
            text_edit(ui, &mut c.command, "Program name or full path", ed);
            note(ui, "Started over stdio and spoken to as an ACP agent.");
        });
        field_row(ui, "Arguments", |ui| text_edit(ui, &mut c.args, "Optional, for example acp", ed));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if kit::button(ui, kit::ButtonKind::Danger, Some(Icon::Trash), "Remove").clicked() {
                page.agent_remove = Some(row.id.clone());
            }
        });
    }

    // ---- endpoints ----

    fn endpoints_card(ui: &mut Ui, prefs: &mut ChatPrefs, page: &mut ChatPage, scan: &mut Scan, ed: &mut Edits) {
        let p = theme::pal(ui);
        let count = prefs.endpoints.len();
        let default_id = prefs.default_endpoint.clone();
        let mut remove = None;
        let mut make_default = None;
        for (i, ep) in prefs.endpoints.iter_mut().enumerate() {
            let key = format!("ep:{}", ep.id);
            let open = page.open.contains(&key);
            let is_default = ep.id == default_id;
            let tail = scan.key_tails.get(&ep.id).cloned();
            let (label, ink) = match (&tail, ep.needs_key(), scan.env_key && ep.is_claude_host()) {
                (Some(_), ..) | (None, _, true) => ("Ready", Some(p.success)),
                (None, true, false) => ("Needs key", Some(p.warning)),
                (None, false, _) => ("No key", None),
            };
            let host = endpoint::host(&ep.base_url).unwrap_or_else(|| "no URL yet".to_owned());
            let format_label = FORMATS.iter().find(|(f, _)| *f == ep.format).map_or("", |(_, l)| *l);
            let sub = format!("{format_label} · {host}");
            let mut clicked = false;
            row_frame(ui, open, |ui| {
                row_header(ui, Icon::Library, &ep.name, "", &sub, |ui| {
                    clicked = chevron(ui, open);
                    kit::chip_ink(ui, label, ink);
                    if is_default {
                        kit::chip_ink(ui, "Default", Some(theme::accent()));
                    }
                });
                if !open {
                    return;
                }
                field_row(ui, "Name", |ui| text_edit(ui, &mut ep.name, "Name", ed));
                field_row(ui, "Format", |ui| {
                    let width = ui.available_width();
                    if let Some(f) = kit::select(ui, ep.format, &FORMATS, width) {
                        ep.format = f;
                        ed.save = true;
                        ed.rescan = true;
                    }
                });
                field_row(ui, "Base URL", |ui| text_edit(ui, &mut ep.base_url, "https://api.anthropic.com or http://127.0.0.1:11434/v1", ed));
                field_row(ui, "Model", |ui| text_edit(ui, &mut ep.model, "Model id", ed));
                key_row(ui, ep, tail.as_deref(), page, scan, ed);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if count > 1 && kit::button(ui, kit::ButtonKind::Danger, Some(Icon::Trash), "Remove endpoint").clicked() {
                        remove = Some(i);
                    }
                    if !is_default && kit::button(ui, kit::ButtonKind::Secondary, None, "Make default").clicked() {
                        make_default = Some(ep.id.clone());
                    }
                });
            });
            flip(&mut page.open, key, clicked);
        }
        if let Some(id) = make_default {
            prefs.default_endpoint = id;
            ed.save = true;
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
            page.open.remove(&format!("ep:{}", gone.id));
            if prefs.default_endpoint == gone.id {
                prefs.default_endpoint = prefs.endpoints[0].id.clone();
            }
            ed.save = true;
            ed.rescan = true;
        }
    }

    /// The masked key field with Save and Remove. The key goes to the Keychain only.
    fn key_row(ui: &mut Ui, ep: &Endpoint, tail: Option<&str>, page: &mut ChatPage, scan: &mut Scan, ed: &mut Edits) {
        let hint = match tail {
            Some(t) => format!("{KEY_MASK}{t}"),
            None if ep.is_claude_host() => CLAUDE_KEY_PLACEHOLDER.to_owned(),
            None => KEY_PLACEHOLDER.to_owned(),
        };
        let draft = page.key_drafts.entry(ep.id.clone()).or_default();
        field_row(ui, "API key", |ui| {
            ui.add(egui::TextEdit::singleline(draft).password(true).margin(theme::INPUT_MARGIN).hint_text(hint).desired_width(ui.available_width()));
            note(ui, &format!("Stored in the {}. Never written to the project or logs.", keychain_name()));
        });
        let mut save_clicked = false;
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            save_clicked = ui.add_enabled_ui(!draft.trim().is_empty(), |ui| kit::button(ui, kit::ButtonKind::Primary, None, "Save").clicked()).inner;
            if tail.is_some() && kit::button(ui, kit::ButtonKind::Danger, Some(Icon::Trash), "Remove key").clicked() {
                page.key_remove = Some(ep.id.clone());
            }
        });
        if save_clicked {
            match endpoint::set_key(ep, draft) {
                Ok(()) => {
                    ed.notify = true;
                    scan.key_tails.insert(ep.id.clone(), key_tail(draft.trim()));
                    draft.clear();
                    page.key_error = None;
                }
                Err(e) => page.key_error = Some(format!("Could not save the key: {e}")),
            }
        }
    }

    // ---- defaults ----

    fn defaults_card(ui: &mut Ui, prefs: &mut ChatPrefs, ed: &mut Edits) {
        field_row(ui, "Backend", |ui| {
            let width = ui.available_width();
            if let Some(b) = kit::select(ui, prefs.default_backend, &BACKENDS, width) {
                prefs.default_backend = b;
                ed.save = true;
            }
        });
        field_row(ui, "Endpoint", |ui| {
            let width = ui.available_width();
            let options: Vec<(usize, &str)> = prefs.endpoints.iter().enumerate().map(|(i, e)| (i, e.name.as_str())).collect();
            let current = prefs.endpoints.iter().position(|e| e.id == prefs.default_endpoint).unwrap_or(0);
            if let Some(i) = kit::select(ui, current, &options, width) {
                prefs.default_endpoint = prefs.endpoints[i].id.clone();
                ed.save = true;
            }
        });
        field_row(ui, "Effort", |ui| {
            let width = ui.available_width();
            let effort = API_EFFORTS.iter().map(|(id, _)| *id).find(|id| *id == prefs.api_effort).unwrap_or(DEFAULT_EFFORT);
            if let Some(e) = kit::select(ui, effort, &API_EFFORTS, width) {
                prefs.api_effort = e.to_owned();
                ed.save = true;
            }
            note(ui, "Sent to api.anthropic.com only.");
        });
        field_row(ui, "Design system", |ui| {
            if kit::checkbox(ui, &mut prefs.ds_summary, "Send design-system summary").changed() {
                ed.save = true;
            }
            note(ui, "Variables, components and libraries with each turn, about 1,500 tokens.");
        });
        field_row(ui, "Images", |ui| {
            let width = ui.available_width();
            // Google Flow shows when the user's `gf` CLI is installed (or already chosen).
            let gf = prefs.image_gen == "gf" || crate::chat::path::which("gf").is_some();
            let mut options = vec![("", "Off"), ("codex", "Codex (your ChatGPT plan)")];
            if gf {
                options.push(("gf", "Google Flow (gf)"));
            }
            options.push(("command", "Custom command"));
            let current = options.iter().map(|(id, _)| *id).find(|id| *id == prefs.image_gen).unwrap_or("");
            if let Some(v) = kit::select(ui, current, &options, width) {
                prefs.image_gen = v.to_owned();
                prefs.image_offer_seen = true;
                ed.save = true;
            }
            if prefs.image_gen == "command" {
                text_edit(ui, &mut prefs.image_command, "my-image-cli {prompt} --aspect {aspect} --out {out}", ed);
                note(ui, "Runs once per image. {prompt}, {aspect} (square, portrait, landscape) and {ratio} (1:1, 3:4, 16:9) are passed as text; write the images into the {out} folder.");
            } else if prefs.image_gen == "gf" {
                note(ui, "Uses Google Flow in your signed-in Chrome. Sign in once with `gf auth login`.");
            } else {
                note(ui, "Lets the agent make card art, heroes and reference images. Off by default.");
            }
        });
        #[cfg(not(target_arch = "wasm32"))]
        field_row(ui, "Helper", |ui| {
            use crate::mcp::delegate::{OPENCODE_COMMAND, PI_COMMAND};
            let width = ui.available_width();
            let cmd = prefs.helper_command.trim();
            let current = if cmd.is_empty() {
                ""
            } else if cmd.starts_with("pi ") {
                "pi"
            } else if cmd.starts_with("opencode ") {
                "opencode"
            } else {
                "command"
            };
            let options = [("", "Off"), ("pi", "pi"), ("opencode", "opencode"), ("command", "Custom command")];
            if let Some(v) = kit::select(ui, current, &options, width) {
                prefs.helper_command = match v {
                    "pi" => PI_COMMAND.into(),
                    "opencode" => OPENCODE_COMMAND.into(),
                    "command" => "my-agent {prompt}".into(),
                    _ => String::new(),
                };
                ed.save = true;
            }
            if current.is_empty() {
                note(ui, "A cheaper agent the chat agent hands review, reading and simple builds to. Off by default.");
            } else {
                text_edit(ui, &mut prefs.helper_command, "my-agent {prompt}", ed);
                note(ui, "Any agent CLI with a shell that takes {prompt} and prints its answer; it drives PaperCraft with papercraft-cli. Change --model / -m to pick its model (opencode with ollama/... runs locally).");
            }
        });
        field_row(ui, "Turn budget", |ui| {
            let width = ui.available_width();
            if let Some(b) = kit::select(ui, prefs.turn_budget, &TURN_BUDGETS, width) {
                prefs.turn_budget = b;
                ed.save = true;
            }
            note(ui, "Stops a turn once its steps have read this much in all (each step re-reads the chat). 10M is about $2 on Opus with caching.");
        });
        field_row(ui, "Claude Code", |ui| {
            if kit::checkbox(ui, &mut prefs.cli_lean, "Lean sessions").changed() {
                ed.save = true;
            }
            note(ui, "Leaves out your CLAUDE.md, hooks, plugins and skills: about half the tokens per turn.");
        });
    }
}

#[cfg(not(target_arch = "wasm32"))]
use native::chat_page;

#[cfg(target_arch = "wasm32")]
fn chat_page(ui: &mut Ui, _ctx: &egui::Context, _settings: &mut SettingsUi) {
    let p = theme::pal(ui);
    ui.label(RichText::new("Chat needs the PaperCraft desktop app.").color(p.secondary));
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
            agents: BTreeMap::from([
                ("claude-code".into(), AgentPrefs { path: Some("/opt/homebrew/bin/claude".into()), ..Default::default() }),
                ("codex".into(), AgentPrefs { path: Some("/usr/local/bin/codex".into()), ..Default::default() }),
                ("pi".into(), AgentPrefs { args: Some("--x".into()), enabled: true, ..Default::default() }),
            ]),
            custom_agents: vec![CustomAgent { id: "mine".into(), name: "Mine".into(), command: "my-agent".into(), args: "--stdio".into() }],
            endpoints: vec![
                Endpoint { model: "claude-haiku-5-5".into(), ..Default::default() },
                Endpoint { id: "ep-1".into(), name: "Local".into(), format: ApiFormat::OpenAi, base_url: "http://127.0.0.1:11434/v1".into(), model: "llama".into() },
            ],
            default_endpoint: "ep-1".into(),
            api_effort: "high".into(),
            api_model: None,
            ds_summary: false,
            cli_lean: false,
            research: true,
            turn_budget: 0,
            image_gen: "command".into(),
            image_command: "my-tool {prompt} {out}".into(),
            image_offer_seen: true,
            plugins: vec!["avataaars".into()],
            issue_repo: "https://example.com/me/repo".into(),
            helper_command: "pi -p {prompt}".into(),
            auto: true,
        };
        assert_eq!(ChatPrefs::from_json(&prefs.to_json()), prefs);
        assert_eq!(ChatPrefs::from_json(&ChatPrefs::default().to_json()), ChatPrefs::default());
    }

    #[test]
    fn old_agent_paths_migrate_into_agents() {
        let prefs = ChatPrefs::from_json(r#"{"claude_path":"/a/claude","codex_path":"/a/codex"}"#);
        assert_eq!(prefs.agents["claude-code"].path.as_deref(), Some("/a/claude"));
        assert_eq!(prefs.agents["codex"].path.as_deref(), Some("/a/codex"));
        assert_eq!(prefs.claude_path.as_deref(), Some("/a/claude"));
        let json = prefs.to_json();
        assert!(!json.contains("claude_path") && !json.contains("codex_path"), "legacy fields are never written");
        assert_eq!(ChatPrefs::from_json(&json), prefs);
        let kept = ChatPrefs::from_json(r#"{"claude_path":"/old","agents":{"claude-code":{"path":"/new"}}}"#);
        assert_eq!(kept.agents["claude-code"].path.as_deref(), Some("/new"));
        assert_eq!(kept.claude_path.as_deref(), Some("/new"));
        assert!(ChatPrefs::from_json("{}").agents.is_empty());
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
        let prefs = ChatPrefs::from_json(r#"{"api_model":"gpt-x","api_effort":"turbo"}"#);
        assert_eq!(prefs.endpoints[0].model, DEFAULT_MODEL);
        assert_eq!(prefs.api_effort, DEFAULT_EFFORT);
        assert_eq!(ChatPrefs::from_json(r#"{"endpoints":[]}"#).endpoints, vec![Endpoint::default()]);
        assert_eq!(prefs.default_backend, BackendKind::ClaudeCode);
        assert_eq!(ChatPrefs::from_json("not json"), ChatPrefs::default());
    }

    #[test]
    fn default_group_holds_ready_and_shown_agents_only() {
        use crate::chat::agents::AgentState::{NeedsEnable, NotFound, Ready, WrongProgram};
        let ready = Ready { path: "/bin/a".into(), version: "1".into() };
        assert!(native::shown_by_default(&ready, false));
        assert!(!native::shown_by_default(&ready, true), "hidden by the user");
        for s in [NeedsEnable, NotFound, WrongProgram { path: "/bin/goose".into() }] {
            assert!(!native::shown_by_default(&s, false));
        }
        let rows = [("a", true), ("b", false), ("c", true), ("d", false)];
        assert_eq!(native::split_primary(rows), (vec!["a", "c"], vec!["b", "d"]));
        assert_eq!(native::split_primary(Vec::<(u8, bool)>::new()), (vec![], vec![]));
    }

    #[test]
    fn hidden_flag_and_paths_round_trip_without_legacy_fields() {
        let mut prefs = ChatPrefs::default();
        prefs.agents.insert("gemini".into(), AgentPrefs { hidden: true, ..Default::default() });
        prefs.agents.insert("claude-code".into(), AgentPrefs { path: Some("/x/claude".into()), ..Default::default() });
        prefs.agents.insert("pi".into(), AgentPrefs::default());
        prefs.tidy();
        assert!(!prefs.agents.contains_key("pi"), "empty entries are dropped");
        assert_eq!(prefs.claude_path.as_deref(), Some("/x/claude"));
        let json = prefs.to_json();
        assert!(!json.contains("claude_path"));
        assert_eq!(ChatPrefs::from_json(&json), prefs);
    }

    /// Seeds the scan so the page renders with no CLI on the machine and no key, without
    /// spawning processes or reading the Keychain.
    #[test]
    fn chat_page_draws_with_no_clis_and_no_key() {
        use crate::chat::agents::{AGENTS, AgentState, AgentStatus};
        let ctx = egui::Context::default();
        crate::ui::setup_style(&ctx);
        let mut st = UiState::default();
        let mut chat = ChatPrefs::default();
        chat.custom_agents.push(CustomAgent { id: "mine".into(), name: "Mine".into(), command: "my-agent".into(), args: "acp".into() });
        chat.endpoints.push(Endpoint { id: "ep-1".into(), name: "Local".into(), base_url: "http://127.0.0.1:11434/v1".into(), model: "llama".into(), ..Default::default() });
        let statuses = AGENTS
            .iter()
            .map(|a| AgentStatus { id: a.id.into(), state: if a.id == "goose" { AgentState::WrongProgram { path: "/opt/homebrew/bin/goose".into() } } else { AgentState::NotFound } })
            .chain([AgentStatus { id: "mine".into(), state: AgentState::NotFound }])
            .collect();
        let page = native::ChatPage {
            scan: Some(native::Scan { agents: Some(statuses), probe: None, key_tails: Default::default(), env_key: false }),
            show_more: true,
            open: ["agent:claude-code", "agent:qwen", "agent:mine", "ep:anthropic", "ep:ep-1"].into_iter().map(String::from).collect(),
            ..Default::default()
        };
        st.settings = SettingsUi { open: true, page: Page::Chat, chat, chat_changed: false, chat_page: page, issue_token: String::new(), issue_note: None };
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
        assert!(!st.settings.chat_changed, "drawing alone changes nothing");
    }
}
