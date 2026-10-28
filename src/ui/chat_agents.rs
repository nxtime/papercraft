//! The agent panel (opened from the agent chip), the setup card and the sign-in state of the chat
//! column. `group_agents`, `backend_for`, `current_pick` and `auth_error` are plain functions so
//! they are tested without a frame; the drawing only calls them.

use crate::theme::Surface as _;
use super::{Detect, EndpointState};
use crate::chat::agents::{self, AgentKind, AgentState};
use crate::chat::{Backend, BackendKind};
use crate::theme::{self, Icon};
use crate::ui::settings::ChatPrefs;
use crate::ui::{chat_msgs, kit};
use egui::{Align, Color32, Id, Layout, Rect, RichText, Sense, Ui, UiBuilder, pos2, vec2};

const ROW_H: f32 = 46.0;
const BADGE: f32 = 28.0;
const PAD: f32 = 8.0;
const PANEL_PAD: f32 = 16.0;
const SEARCH_H: f32 = 34.0;
const FOOTER_H: f32 = 30.0;
const TITLE_H: f32 = 30.0;
const BACKDROP_ALPHA: u8 = 110;
const SHADOW_BLUR: u8 = 40;
const SHADOW_ALPHA: u8 = 150;
/// Room right of a row's text for its chip or button.
/// Agents listed in the setup card; "Show all agents" opens the panel for the rest.
const SETUP_ROWS: usize = 4;
const RIGHT_W: f32 = 104.0;

/// What a row stands for.
#[derive(Debug, Clone, PartialEq)]
pub enum Pick {
    /// A registry or custom agent id (`claude-code` and `codex` keep their direct backends).
    Agent(String),
    Endpoint(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Status {
    Ready,
    NeedsEnable,
    NotFound,
    WrongProgram(String),
    NoKey,
    Incomplete,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub pick: Pick,
    pub name: String,
    pub version: String,
    pub sub: String,
    pub status: Status,
    pub docs: Option<&'static str>,
}

impl Row {
    pub fn is_selectable(&self) -> bool {
        self.status == Status::Ready
    }
}

#[derive(Debug, Default, PartialEq)]
pub struct Groups {
    /// Ready agents, the direct ones included.
    pub detected: Vec<Row>,
    /// `npx` agents that are off until the user enables them.
    pub npx: Vec<Row>,
    /// Not found, or a different program with the agent's name.
    pub missing: Vec<Row>,
    pub endpoints: Vec<Row>,
    pub custom: Vec<Row>,
}

impl Groups {
    pub fn is_empty(&self) -> bool {
        self.detected.is_empty() && self.npx.is_empty() && self.missing.is_empty() && self.endpoints.is_empty() && self.custom.is_empty()
    }
}

/// "gemini 0.44.1" -> "v0.44.1"; "" when no token looks like a version.
fn short_version(v: &str) -> String {
    let starts_digit = |t: &str| t.trim_start_matches('v').chars().next().is_some_and(|c| c.is_ascii_digit());
    v.split_whitespace().find(|t| starts_digit(t)).map(|t| format!("v{}", t.trim_start_matches('v'))).unwrap_or_default()
}

/// `/Users/me/.local/bin/x` -> `~/.local/bin/x`.
fn tidy(path: &str) -> String {
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(rest) = dirs::home_dir().and_then(|h| path.strip_prefix(h.to_string_lossy().as_ref()).map(str::to_owned)) {
        return format!("~{rest}");
    }
    path.to_owned()
}

fn matches_query(q: &str, parts: &[&str]) -> bool {
    q.is_empty() || parts.iter().any(|p| p.to_lowercase().contains(q))
}

/// The panel's sections for `query` (case-insensitive, on name and id).
pub fn group_agents(d: &Detect, prefs: &ChatPrefs, query: &str) -> Groups {
    let q = query.trim().to_lowercase();
    let mut g = Groups::default();
    for def in agents::AGENTS {
        let hidden = prefs.agents.get(def.id).is_some_and(|a| a.hidden);
        if hidden || !matches_query(&q, &[def.name, def.id]) {
            continue;
        }
        let mut row = Row { pick: Pick::Agent(def.id.into()), name: def.name.into(), version: String::new(), sub: String::new(), status: Status::NotFound, docs: Some(def.docs_url) };
        match d.agent_state(def.id) {
            AgentState::Ready { path, version } => {
                let npx = matches!(def.kind, AgentKind::Npx { .. });
                (row.status, row.version, row.sub) = (Status::Ready, short_version(&version), if npx { "Runs via npx".into() } else { tidy(&path) });
                g.detected.push(row);
            }
            AgentState::NeedsEnable => {
                (row.status, row.sub) = (Status::NeedsEnable, "Needs enable \u{b7} runs via npx".into());
                g.npx.push(row);
            }
            AgentState::NotFound => {
                row.sub = "Not found".into();
                g.missing.push(row);
            }
            AgentState::WrongProgram { path } => {
                (row.sub, row.status) = (format!("Different program at {}", tidy(&path)), Status::WrongProgram(path));
                g.missing.push(row);
            }
        }
    }
    for ep in &prefs.endpoints {
        if !matches_query(&q, &[&ep.name, &ep.id]) {
            continue;
        }
        let state = d.endpoints.iter().find(|s: &&EndpointState| s.id == ep.id);
        let host = crate::chat::endpoint::host(&ep.base_url).unwrap_or_else(|| ep.base_url.clone());
        let (status, sub) = match state {
            _ if !ep.is_complete() => (Status::Incomplete, "Set the URL and model in Settings, Chat".to_owned()),
            Some(s) if s.ready && s.key => (Status::Ready, format!("{host} \u{b7} {} \u{b7} key set", ep.model)),
            Some(s) if s.ready => (Status::Ready, format!("{host} \u{b7} {}", ep.model)),
            _ => (Status::NoKey, "No key. Add one in Settings, Chat".to_owned()),
        };
        g.endpoints.push(Row { pick: Pick::Endpoint(ep.id.clone()), name: ep.name.clone(), version: String::new(), sub, status, docs: None });
    }
    for c in &prefs.custom_agents {
        if !matches_query(&q, &[&c.name, &c.id]) {
            continue;
        }
        let (status, sub) = match d.agent_state(&c.id) {
            AgentState::Ready { .. } => (Status::Ready, format!("{} {}", c.command, c.args).trim().to_owned()),
            _ => (Status::NotFound, format!("Not found: {}", c.command)),
        };
        g.custom.push(Row { pick: Pick::Agent(c.id.clone()), name: c.name.clone(), version: String::new(), sub, status, docs: None });
    }
    g
}

/// What arrows and Enter walk through, in the order the panel draws it.
pub fn selectable(g: &Groups) -> Vec<Pick> {
    [&g.detected, &g.endpoints, &g.custom].into_iter().flatten().filter(|r| r.is_selectable()).map(|r| r.pick.clone()).collect()
}

/// The backend a chat gets when this row is chosen.
pub fn backend_for(pick: &Pick, prefs: &ChatPrefs) -> Backend {
    match pick {
        Pick::Agent(id) => match id.as_str() {
            "claude-code" => Backend { kind: BackendKind::ClaudeCode, ..Default::default() },
            "codex" => Backend { kind: BackendKind::Codex, ..Default::default() },
            _ => Backend { kind: BackendKind::Acp, agent: Some(id.clone()), ..Default::default() },
        },
        Pick::Endpoint(id) => {
            let ep = prefs.endpoint(Some(id));
            Backend {
                kind: BackendKind::Anthropic,
                model: Some(ep.model.clone()),
                effort: ep.is_claude_host().then(|| prefs.api_effort.clone()),
                endpoint: Some(ep.id.clone()),
                cli_session: None,
                agent: None,
            }
        }
    }
}

/// The row a chat's backend belongs to (the checkmark).
pub fn current_pick(b: &Backend, prefs: &ChatPrefs) -> Pick {
    match b.kind {
        BackendKind::ClaudeCode => Pick::Agent("claude-code".into()),
        BackendKind::Codex => Pick::Agent("codex".into()),
        BackendKind::Acp => Pick::Agent(b.agent.clone().unwrap_or_default()),
        BackendKind::Anthropic => Pick::Endpoint(b.endpoint.clone().unwrap_or_else(|| prefs.default_endpoint.clone())),
    }
}

/// Another agent or endpoint is another conversation: the old session id does not carry over.
pub fn same_target(a: &Backend, b: &Backend) -> bool {
    (a.kind, &a.agent, &a.endpoint) == (b.kind, &b.agent, &b.endpoint)
}

// ---- sign-in state ----

/// A turn failed because the agent is signed out.
#[derive(Debug, Clone, PartialEq)]
pub struct SignIn {
    pub title: String,
    pub hint: String,
}

/// The agent's own error says it needs a login (ACP error code -32000 reads "Authentication
/// required"; a few agents word it differently).
pub fn auth_error(e: &str) -> bool {
    const PHRASES: [&str; 8] =
        ["authentication required", "auth_required", "not authenticated", "not logged in", "login required", "please log in", "please sign in", "unauthenticated"];
    let e = e.to_lowercase();
    PHRASES.iter().any(|p| e.contains(p))
}

/// The command from each agent's own docs that signs it in. None: the card tells to use its own app.
fn login_command(id: &str) -> Option<&'static str> {
    Some(match id {
        "cursor" => "cursor-agent login",
        "opencode" => "opencode auth login",
        "codex" | "codex-acp" => "codex login",
        "gemini" => "gemini",
        "goose" => "goose configure",
        "claude-acp" => "claude /login",
        _ => return None,
    })
}

pub fn signin_for(id: &str, name: &str, error: &str) -> Option<SignIn> {
    if !auth_error(error) {
        return None;
    }
    let hint = login_command(id).map_or_else(|| "Sign in with the agent's own app, then retry.".to_owned(), |c| format!("Run `{c}` in a terminal, then retry."));
    Some(SignIn { title: format!("{name} needs you to sign in"), hint })
}

// ---- drawing ----

pub enum Act {
    Select(Pick),
    /// Model or effort of an Anthropic endpoint changed.
    SetBackend(Backend),
    Enable(String),
    /// Settings, Chat: set a path, add an endpoint or a custom agent.
    Settings,
    DetectAgain,
    OpenPanel,
    Close,
}

#[derive(Default)]
pub struct PanelState {
    pub query: String,
    /// Row the arrow keys are on (index into [`selectable`]).
    pub sel: usize,
    /// Focus the search field on the next frame.
    pub fresh: bool,
    pub missing_open: bool,
}

pub struct PanelIn<'a> {
    pub detect: &'a Detect,
    pub prefs: &'a ChatPrefs,
    pub backend: &'a Backend,
    pub running: bool,
    /// The chat column, the agent chip, and the top of the composer.
    pub column: Rect,
    pub chip: Rect,
    pub composer_top: f32,
}

enum Badge<'a> {
    Letter(&'a str),
    Icon(Icon),
    /// A brand mark: an SVG drawn in `currentColor`, tinted with the row's ink.
    Logo(&'static str, &'static [u8]),
}

/// The vendor's mark for an agent (LobeHub icons, MIT); agents without one keep their letter.
fn logo(agent: &str) -> Option<(&'static str, &'static [u8])> {
    macro_rules! svg {
        ($f:literal) => {
            ($f, include_bytes!(concat!("../../assets/agents/", $f, ".svg")).as_slice())
        };
    }
    Some(match agent {
        "claude-code" | "claude-acp" => svg!("claude"),
        "codex" | "codex-acp" => svg!("openai"),
        "gemini" => svg!("gemini"),
        "cursor" => svg!("cursor"),
        "goose" => svg!("goose"),
        "kimi" => svg!("kimi"),
        "vibe" => svg!("mistral"),
        "hermes" => svg!("nousresearch"),
        "qwen" => svg!("qwen"),
        "cline" => svg!("cline"),
        "copilot" => svg!("githubcopilot"),
        "codebuddy" => svg!("tencent"),
        _ => return None,
    })
}

/// The mark of the agent behind `b`: its registry logo, Claude's for a Claude API endpoint.
pub fn backend_logo(b: &Backend, prefs: &ChatPrefs) -> Option<(&'static str, &'static [u8])> {
    match current_pick(b, prefs) {
        Pick::Agent(id) => logo(&id),
        Pick::Endpoint(id) => prefs.endpoint(Some(&id)).is_claude_host().then(|| logo("claude-code")).flatten(),
    }
}

/// Paints `b`'s logo into `rect` tinted `ink`; false when it has none.
pub fn paint_backend_logo(ui: &Ui, rect: Rect, b: &Backend, prefs: &ChatPrefs, ink: Color32) -> bool {
    let Some((name, svg)) = backend_logo(b, prefs) else { return false };
    let Some(t) = logo_texture(ui.ctx(), name, svg) else { return false };
    let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
    ui.painter().image(t.id(), rect, uv, ink);
    true
}

/// Paints agent `id`'s logo into `rect` tinted `ink`, or the first letter of `name` when it has
/// none.
pub fn paint_agent_mark(ui: &Ui, rect: Rect, id: &str, name: &str, ink: Color32) {
    if let Some(t) = logo(id).and_then(|(n, svg)| logo_texture(ui.ctx(), n, svg)) {
        let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
        ui.painter().image(t.id(), rect, uv, ink);
        return;
    }
    let letter = name.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_default();
    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, letter, theme::semibold(theme::BODY + 2.0), ink);
}

/// Pixels a logo is rasterised at (white, so the tint gives its colour).
const LOGO_PX: u32 = 64;

fn logo_texture(ctx: &egui::Context, name: &str, svg: &[u8]) -> Option<egui::TextureHandle> {
    svg_texture(ctx, &format!("agent-logo-{name}"), svg)
}

/// A monochrome SVG as a white texture (its shape in alpha), so a tint gives its colour.
/// Cached per `key` for the context's life.
pub(crate) fn svg_texture(ctx: &egui::Context, key: &str, svg: &[u8]) -> Option<egui::TextureHandle> {
    let id = Id::new(("svg-texture", key));
    if let Some(t) = ctx.data(|d| d.get_temp::<egui::TextureHandle>(id)) {
        return Some(t);
    }
    let tree = usvg::Tree::from_data(svg, &usvg::Options::default()).ok()?;
    let scale = LOGO_PX as f32 / tree.size().width().max(tree.size().height());
    let mut pixmap = resvg::tiny_skia::Pixmap::new(LOGO_PX, LOGO_PX)?;
    resvg::render(&tree, resvg::tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
    let pixels: Vec<egui::Color32> = pixmap.pixels().iter().map(|p| egui::Color32::from_white_alpha(p.alpha())).collect();
    let img = egui::ColorImage { size: [LOGO_PX as usize; 2], source_size: egui::vec2(LOGO_PX as f32, LOGO_PX as f32), pixels };
    let t = ctx.load_texture(key, img, egui::TextureOptions::LINEAR);
    ctx.data_mut(|d| d.insert_temp(id, t.clone()));
    Some(t)
}

struct Spec<'a> {
    id: Id,
    badge: Badge<'a>,
    name: &'a str,
    version: &'a str,
    sub: &'a str,
    ink: Color32,
    fill: Option<Color32>,
    click: bool,
}

/// One row's frame: hover, badge, name, version and sub line. Returns the row rect and its click.
fn shell(ui: &mut Ui, s: &Spec) -> (Rect, bool) {
    let p = theme::pal(ui);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::hover());
    let resp = ui.interact(rect, s.id, Sense::click());
    let hover = s.click && resp.hovered();
    if let Some(f) = s.fill.or(hover.then_some(p.hover)) {
        ui.painter().rect_filled(rect, theme::radius_lg(), f);
    }
    if hover {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let badge = Rect::from_min_size(pos2(rect.left() + PAD, rect.center().y - BADGE / 2.0), vec2(BADGE, BADGE));
    theme::fill_control(ui, badge, theme::radius_lg(), p.field);
    match s.badge {
        Badge::Letter(l) => {
            ui.painter().text(badge.center(), egui::Align2::CENTER_CENTER, l, theme::semibold(theme::BODY), s.ink);
        }
        Badge::Icon(i) => theme::paint_icon(ui.painter(), badge.shrink(7.0), i, s.ink),
        Badge::Logo(name, svg) => {
            if let Some(t) = logo_texture(ui.ctx(), name, svg) {
                let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
                ui.painter().image(t.id(), badge.shrink(7.0), uv, s.ink);
            }
        }
    }
    let x = badge.right() + PAD + 2.0;
    let text = ui.painter().with_clip_rect(Rect::from_min_max(rect.min, pos2(rect.right() - RIGHT_W, rect.bottom())));
    let g = text.layout_no_wrap(s.name.to_owned(), theme::semibold(theme::BODY), s.ink);
    let w = g.size().x;
    text.galley(pos2(x, rect.top() + 7.0), g, s.ink);
    if !s.version.is_empty() {
        text.text(pos2(x + w + 6.0, rect.top() + 9.0), egui::Align2::LEFT_TOP, s.version, egui::FontId::proportional(theme::SECONDARY), p.tertiary);
    }
    if !s.sub.is_empty() {
        text.text(pos2(x, rect.top() + 25.0), egui::Align2::LEFT_TOP, s.sub, egui::FontId::proportional(theme::SECONDARY), p.secondary);
    }
    (rect, resp.clicked())
}

/// Widgets laid right to left inside a row.
fn right<R>(ui: &mut Ui, rect: Rect, f: impl FnOnce(&mut Ui) -> R) -> R {
    let area = Rect::from_min_max(pos2(rect.left(), rect.top()), pos2(rect.right() - PAD, rect.bottom()));
    ui.scope_builder(UiBuilder::new().max_rect(area).layout(Layout::right_to_left(Align::Center)), |ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        f(ui)
    })
    .inner
}

fn check(ui: &mut Ui) {
    let (r, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
    let c = theme::accent();
    let pt = |x: f32, y: f32| pos2(r.left() + x, r.top() + y);
    ui.painter().add(egui::Shape::line(vec![pt(3.0, 8.5), pt(6.5, 12.0), pt(13.0, 4.5)], egui::Stroke::new(1.6, c)));
}

fn agent_row(ui: &mut Ui, scope: &str, r: &Row, selected: bool, focused: bool) -> Option<Act> {
    let p = theme::pal(ui);
    let ready = r.is_selectable();
    let missing = matches!(r.status, Status::NotFound | Status::WrongProgram(_));
    let ink = if missing { p.tertiary } else { p.text };
    let id = Id::new(("chat-agent-row", scope, format!("{:?}", r.pick)));
    let letter = r.name.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_default();
    let spec = Spec {
        id,
        badge: match &r.pick {
            Pick::Agent(a) => logo(a).map_or(Badge::Letter(&letter), |(n, svg)| Badge::Logo(n, svg)),
            Pick::Endpoint(_) => Badge::Letter(&letter),
        },
        name: &r.name,
        version: &r.version,
        // A missing agent's sub line is drawn below, with its links.
        sub: if missing { "" } else { &r.sub },
        ink,
        fill: (selected || focused).then_some(p.hover),
        click: ready,
    };
    let (rect, clicked) = shell(ui, &spec);
    let mut act = None;
    let mut chip_click = false;
    if missing {
        let line = Rect::from_min_max(pos2(rect.left() + PAD + BADGE + PAD + 2.0, rect.top() + 23.0), pos2(rect.right() - RIGHT_W, rect.bottom()));
        ui.scope_builder(UiBuilder::new().max_rect(line).layout(Layout::left_to_right(Align::Center)), |ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            if ui.link(RichText::new("Set path\u{2026}").size(theme::SECONDARY)).clicked() {
                act = Some(Act::Settings);
            }
            if let Some(url) = r.docs {
                ui.hyperlink_to(RichText::new("Docs").size(theme::SECONDARY), url);
            }
            if matches!(r.status, Status::WrongProgram(_)) {
                ui.add(egui::Label::new(RichText::new(&r.sub).size(theme::SECONDARY).color(p.tertiary)).truncate());
            }
        });
    }
    right(ui, rect, |ui| {
        if selected {
            check(ui);
        }
        match &r.status {
            Status::Ready => chip_click = kit::chip_ink(ui, "Ready", Some(p.success)).clicked(),
            Status::NeedsEnable => {
                if kit::chip_ink(ui, "Enable", Some(theme::accent())).clicked() {
                    act = Some(Act::Enable(match &r.pick {
                        Pick::Agent(id) => id.clone(),
                        Pick::Endpoint(id) => id.clone(),
                    }));
                }
            }
            Status::NotFound => drop(kit::chip_ink(ui, "Not found", None)),
            Status::WrongProgram(_) => drop(kit::chip_ink(ui, "Wrong program", Some(p.warning))),
            Status::NoKey => drop(kit::chip_ink(ui, "No key", Some(p.warning))),
            Status::Incomplete => drop(kit::chip_ink(ui, "Set up", Some(p.warning))),
        }
    });
    if (clicked || chip_click) && ready {
        act = Some(Act::Select(r.pick.clone()));
    }
    act
}

/// A card-like row that leads to Settings: "Add an API endpoint", "Add custom agent".
fn nav_row(ui: &mut Ui, scope: &str, icon: Icon, name: &str, sub: &str, card: bool) -> bool {
    let p = theme::pal(ui);
    let spec = Spec { id: Id::new(("chat-nav-row", scope, name)), badge: Badge::Icon(icon), name, version: "", sub, ink: p.text, fill: card.then_some(p.popover), click: true };
    let (rect, clicked) = shell(ui, &spec);
    if card {
        let c = rect.right_center() - vec2(PAD + 6.0, 0.0);
        theme::paint_icon(ui.painter(), Rect::from_center_size(c, vec2(14.0, 14.0)), Icon::ChevronRight, p.secondary);
    }
    clicked
}

enum Fold {
    None,
    Action(Icon),
    Open(bool),
}

/// "DETECTED  7" with an action (a + icon) or a fold chevron on the right. True when clicked.
fn section(ui: &mut Ui, title: &str, count: Option<usize>, fold: Fold) -> bool {
    let p = theme::pal(ui);
    ui.add_space(6.0);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 22.0), Sense::hover());
    let resp = ui.interact(rect, Id::new(("chat-agent-section", title)), if matches!(fold, Fold::None) { Sense::hover() } else { Sense::click() });
    let font = theme::semibold(theme::SECONDARY);
    let x = rect.left() + PAD;
    let g = ui.painter().layout_no_wrap(title.to_uppercase(), font.clone(), p.tertiary);
    let w = g.size().x;
    ui.painter().galley(pos2(x, rect.center().y - g.size().y / 2.0), g, p.tertiary);
    let edge = rect.right() - PAD;
    match fold {
        Fold::None => {
            if let Some(n) = count {
                ui.painter().text(pos2(edge, rect.center().y), egui::Align2::RIGHT_CENTER, n.to_string(), font, p.tertiary);
            }
        }
        Fold::Action(icon) => {
            theme::paint_icon(ui.painter(), Rect::from_center_size(pos2(edge - 7.0, rect.center().y), vec2(14.0, 14.0)), icon, theme::accent());
        }
        Fold::Open(open) => {
            let icon = if open { Icon::ChevronDown } else { Icon::ChevronRight };
            theme::paint_icon(ui.painter(), Rect::from_center_size(pos2(edge - 6.0, rect.center().y), vec2(12.0, 12.0)), icon, p.tertiary);
            if let Some(n) = count {
                ui.painter().text(pos2(x + w + 8.0, rect.center().y), egui::Align2::LEFT_CENTER, n.to_string(), font, p.tertiary);
            }
        }
    }
    resp.clicked()
}

/// The panel over the chat column. Returns what the user did this frame.
pub fn panel(ctx: &egui::Context, ps: &mut PanelState, i: &PanelIn) -> Option<Act> {
    let p = theme::pal_from_ctx(ctx);
    let g = group_agents(i.detect, i.prefs, &ps.query);
    let flat = selectable(&g);
    let (down, up, enter, esc) = ctx.input_mut(|inp| {
        use egui::{Key, Modifiers as M};
        (inp.consume_key(M::NONE, Key::ArrowDown), inp.consume_key(M::NONE, Key::ArrowUp), inp.consume_key(M::NONE, Key::Enter), inp.consume_key(M::NONE, Key::Escape))
    });
    if esc {
        return Some(Act::Close);
    }
    if down {
        ps.sel += 1;
    }
    if up {
        ps.sel = ps.sel.saturating_sub(1);
    }
    ps.sel = ps.sel.min(flat.len().saturating_sub(1));
    let mut act = None;
    if enter && !i.running {
        act = flat.get(ps.sel).cloned().map(Act::Select);
    }

    // Dimmed backdrop over the column: a click outside the panel closes it.
    egui::Area::new("chat-agent-backdrop".into()).order(egui::Order::Foreground).fade_in(false).fixed_pos(i.column.min).show(ctx, |ui| {
        let (r, resp) = ui.allocate_exact_size(i.column.size(), Sense::click());
        ui.painter().rect_filled(r, theme::radius_panel(), Color32::from_black_alpha(BACKDROP_ALPHA));
        if resp.clicked() {
            act = Some(Act::Close);
        }
    });

    let top = i.chip.bottom() + 6.0;
    let height = (i.composer_top - 10.0 - top).max(240.0);
    let list_h = height - 2.0 * PANEL_PAD - TITLE_H - SEARCH_H - FOOTER_H - 24.0;
    egui::Area::new("chat-agent-panel".into()).order(egui::Order::Foreground).fade_in(false).fixed_pos(pos2(i.column.left(), top)).show(ctx, |ui| {
        egui::Frame::new()
            .fill(p.popover)
            .stroke(egui::Stroke::new(1.0, p.separator))
            .corner_radius(theme::radius_panel())
            .inner_margin(PANEL_PAD)
            .shadow(egui::Shadow { offset: [0, 12], blur: SHADOW_BLUR, spread: 0, color: Color32::from_black_alpha(SHADOW_ALPHA) })
            .show_surface(ui, |ui| {
                ui.set_width(i.column.width() - 2.0 * PANEL_PAD);
                ui.spacing_mut().item_spacing.y = 2.0;
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Choose an agent").font(theme::semibold(15.0)));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if kit::close_button(ui).clicked() {
                            act = Some(Act::Close);
                        }
                    });
                });
                ui.add_space(4.0);
                let search = kit::icon_field(ui, Icon::Search, &mut ps.query, "Search agents");
                if std::mem::take(&mut ps.fresh) {
                    search.request_focus();
                }
                if search.changed() {
                    ps.sel = 0;
                }
                let selected = current_pick(i.backend, i.prefs);
                let mut n = 0usize;
                egui::ScrollArea::vertical().max_height(list_h).auto_shrink([false, true]).show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    if !g.detected.is_empty() {
                        section(ui, "Detected", Some(g.detected.len()), Fold::None);
                        draw_rows(ui, &g.detected, &selected, ps.sel, &mut n, i.running, &mut act);
                    }
                    if !g.npx.is_empty() {
                        section(ui, "Enable via npx", Some(g.npx.len()), Fold::None);
                        draw_rows(ui, &g.npx, &selected, ps.sel, &mut n, i.running, &mut act);
                        ui.label(RichText::new("Runs code fetched by npx").size(theme::SECONDARY).color(p.tertiary));
                    }
                    if !g.missing.is_empty() && section(ui, "Not found", Some(g.missing.len()), Fold::Open(ps.missing_open)) {
                        ps.missing_open = !ps.missing_open;
                    }
                    if !g.missing.is_empty() && ps.missing_open {
                        draw_rows(ui, &g.missing, &selected, ps.sel, &mut n, i.running, &mut act);
                    }
                    if !g.endpoints.is_empty() && section(ui, "API endpoints", None, Fold::Action(Icon::Plus)) {
                        act = Some(Act::Settings);
                    }
                    draw_rows(ui, &g.endpoints, &selected, ps.sel, &mut n, i.running, &mut act);
                    section(ui, "Custom", None, Fold::None);
                    draw_rows(ui, &g.custom, &selected, ps.sel, &mut n, i.running, &mut act);
                    if nav_row(ui, "panel", Icon::Plus, "Add custom agent", "Command and arguments (ACP)", false) {
                        act = Some(Act::Settings);
                    }
                    if g.is_empty() {
                        ui.label(RichText::new("No agent matches the search").size(theme::SECONDARY).color(p.tertiary));
                    }
                    if let Some(a) = model_block(ui, i) {
                        act = Some(a);
                    }
                });
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if kit::chip(ui, "Detect again", false).clicked() {
                        act = Some(Act::DetectAgain);
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if kit::chip_ink(ui, "Settings \u{2192} Chat", Some(theme::accent())).clicked() {
                            act = Some(Act::Settings);
                        }
                    });
                });
            });
    });
    act
}

fn draw_rows(ui: &mut Ui, rows: &[Row], selected: &Pick, sel: usize, n: &mut usize, running: bool, act: &mut Option<Act>) {
    for r in rows {
        let focused = r.is_selectable() && *n == sel;
        *n += usize::from(r.is_selectable());
        let a = ui.add_enabled_ui(!(running && r.is_selectable()), |ui| agent_row(ui, "panel", r, r.pick == *selected, focused)).inner;
        *act = a.or(act.take());
    }
}

/// Model and effort of a Claude API endpoint, or of Claude Code (where no pick, "Default", leaves
/// the CLI's own choice).
fn model_block(ui: &mut Ui, i: &PanelIn) -> Option<Act> {
    use crate::ui::settings::{API_EFFORTS, API_MODELS};
    let b = i.backend;
    let ep = i.prefs.endpoint(b.endpoint.as_deref());
    let api = b.kind == BackendKind::Anthropic && ep.is_claude_host();
    if !api && b.kind != BackendKind::ClaudeCode {
        return None;
    }
    let mut out = None;
    let model = if api { Some(b.model.clone().unwrap_or_else(|| ep.model.clone())) } else { b.model.clone() };
    section(ui, "Model", None, Fold::None);
    let default = (!api).then_some((None, "Default", "Claude Code's own choice"));
    for (id, label, detail) in default.into_iter().chain(API_MODELS.iter().map(|(id, l)| (Some(*id), *l, *id))) {
        let r = ui.add(egui::Button::selectable(model.as_deref() == id, RichText::new(format!("{label}   {detail}")).size(theme::SECONDARY + 1.0)).frame(false));
        if r.clicked() && !i.running {
            out = Some(Act::SetBackend(Backend { model: id.map(Into::into), ..b.clone() }));
        }
    }
    section(ui, "Effort", None, Fold::None);
    let effort = if api { Some(b.effort.clone().unwrap_or_else(|| i.prefs.api_effort.clone())) } else { b.effort.clone() };
    let offset = usize::from(!api);
    let items: Vec<(&str, Option<&str>)> = (!api).then_some(("Default", None)).into_iter().chain(API_EFFORTS.iter().map(|(_, l)| (*l, None))).collect();
    let cur = effort.and_then(|e| API_EFFORTS.iter().position(|(id, _)| *id == e)).map_or(if api { 1 } else { 0 }, |n| n + offset);
    if let Some(n) = kit::segmented(ui, &items, cur)
        && !i.running
    {
        let effort = n.checked_sub(offset).map(|n| API_EFFORTS[n].0.to_owned());
        out = Some(Act::SetBackend(Backend { effort, ..b.clone() }));
    }
    out
}

/// "Connect an agent": detected agents to pick, plus API endpoint and custom agent.
pub fn setup(ui: &mut Ui, d: &Detect, prefs: &ChatPrefs) -> Option<Act> {
    let p = theme::pal(ui);
    let g = group_agents(d, prefs, "");
    let ready: Vec<&Row> = g.detected.iter().chain(g.custom.iter().filter(|r| r.is_selectable())).collect();
    let mut act = None;
    ui.add_space(ui.available_height() * 0.04);
    egui::Frame::new().fill(p.popover).corner_radius(theme::radius_panel()).inner_margin(PANEL_PAD).show_surface(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = 4.0;
        kit::badge(ui, Icon::Laser, theme::accent(), 40.0);
        ui.add_space(4.0);
        ui.label(RichText::new("Connect an agent").font(theme::semibold(18.0)));
        let blurb = match ready.len() {
            0 => "PaperCraft did not find an agent on this computer. Add an API endpoint or a custom agent.".to_owned(),
            1 => "PaperCraft found 1 agent on this computer. Pick it to start designing by conversation.".to_owned(),
            n => format!("PaperCraft found {n} agents on this computer. Pick one to start designing by conversation."),
        };
        ui.add(egui::Label::new(RichText::new(blurb).color(p.secondary)));
        if !ready.is_empty() {
            section(ui, "Detected on this computer", Some(ready.len()), Fold::None);
            for r in ready.into_iter().take(SETUP_ROWS) {
                act = agent_row(ui, "setup", r, false, false).or(act.take());
            }
        }
        if ui.link(RichText::new("Show all agents").size(theme::SECONDARY)).clicked() {
            act = Some(Act::OpenPanel);
        }
    });
    ui.add_space(6.0);
    if nav_row(ui, "setup", Icon::Plus, "Add an API endpoint", "Anthropic, OpenAI-compatible or MiniMax-style", true) {
        act = Some(Act::Settings);
    }
    if nav_row(ui, "setup", Icon::Plus, "Add custom agent", "Any ACP agent: name, command, arguments", true) {
        act = Some(Act::Settings);
    }
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        if kit::chip(ui, "Detect again", false).clicked() {
            act = Some(Act::DetectAgain);
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if kit::chip_ink(ui, "Settings \u{2192} Chat", Some(theme::accent())).clicked() {
                act = Some(Act::Settings);
            }
        });
    });
    act
}

/// The sign-in card: what to run, and Retry (through `acts`).
pub fn signin_card(ui: &mut Ui, s: &SignIn, acts: &mut chat_msgs::Acts) {
    chat_msgs::notice_card(ui, &s.title, &s.hint, acts);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::agents::AgentStatus;
    use crate::ui::settings::CustomAgent;

    fn ready(id: &str, v: &str) -> AgentStatus {
        AgentStatus { id: id.into(), state: AgentState::Ready { path: format!("/bin/{id}"), version: v.into() } }
    }

    fn detect() -> Detect {
        Detect {
            claude: Some(("/bin/claude".into(), "2.1.0 (Claude Code)".into())),
            agents: vec![
                ready("gemini", "gemini 0.44.1"),
                AgentStatus { id: "pi".into(), state: AgentState::NeedsEnable },
                AgentStatus { id: "goose".into(), state: AgentState::WrongProgram { path: "/opt/goose".into() } },
                ready("mine", ""),
            ],
            endpoints: vec![EndpointState { id: "anthropic".into(), key: true, ready: true }],
            default_endpoint: "anthropic".into(),
            ..Default::default()
        }
    }

    fn prefs() -> ChatPrefs {
        let mut p = ChatPrefs::default();
        p.custom_agents.push(CustomAgent { id: "mine".into(), name: "Mine".into(), command: "my-agent".into(), args: "--stdio".into() });
        p
    }

    fn names(rows: &[Row]) -> Vec<&str> {
        rows.iter().map(|r| r.name.as_str()).collect()
    }

    #[test]
    fn groups_split_ready_npx_missing_endpoints_and_custom() {
        let g = group_agents(&detect(), &prefs(), "");
        assert_eq!(names(&g.detected), ["Claude Code", "Gemini CLI"]);
        assert_eq!(g.detected[1].version, "v0.44.1");
        assert!(g.npx.iter().any(|r| r.name == "Pi" && r.status == Status::NeedsEnable));
        assert!(g.missing.iter().any(|r| r.name == "Codex" && r.status == Status::NotFound && r.docs.is_some()));
        let goose = g.missing.iter().find(|r| r.name == "Goose").unwrap();
        assert_eq!(goose.sub, "Different program at /opt/goose");
        assert_eq!(names(&g.endpoints), ["Anthropic"]);
        assert_eq!((g.custom[0].name.as_str(), g.custom[0].status.clone()), ("Mine", Status::Ready));
        assert!(!g.detected.iter().any(|r| r.name == "Mine"), "custom agents stay in their own section");
    }

    #[test]
    fn hidden_agents_stay_out_of_the_panel() {
        let mut p = prefs();
        p.agents.insert("gemini".into(), crate::ui::settings::AgentPrefs { hidden: true, ..Default::default() });
        let g = group_agents(&detect(), &p, "");
        assert_eq!(names(&g.detected), ["Claude Code"]);
    }

    #[test]
    fn query_filters_every_section_case_insensitively() {
        let g = group_agents(&detect(), &prefs(), "  GEM ");
        assert_eq!(names(&g.detected), ["Gemini CLI"]);
        assert!(g.npx.is_empty() && g.missing.is_empty() && g.endpoints.is_empty() && g.custom.is_empty());
        assert!(group_agents(&detect(), &prefs(), "zzz").is_empty());
        assert_eq!(names(&group_agents(&detect(), &prefs(), "mine").custom), ["Mine"]);
    }

    #[test]
    fn only_ready_rows_are_selectable_in_draw_order() {
        let picks = selectable(&group_agents(&detect(), &prefs(), ""));
        assert_eq!(picks, [Pick::Agent("claude-code".into()), Pick::Agent("gemini".into()), Pick::Endpoint("anthropic".into()), Pick::Agent("mine".into())]);
    }

    #[test]
    fn a_row_maps_to_its_backend_and_back() {
        let prefs = prefs();
        let claude = backend_for(&Pick::Agent("claude-code".into()), &prefs);
        assert_eq!(claude.kind, BackendKind::ClaudeCode);
        assert_eq!(backend_for(&Pick::Agent("codex".into()), &prefs).kind, BackendKind::Codex);
        let acp = backend_for(&Pick::Agent("gemini".into()), &prefs);
        assert_eq!((acp.kind, acp.agent.as_deref()), (BackendKind::Acp, Some("gemini")));
        let ep = backend_for(&Pick::Endpoint("anthropic".into()), &prefs);
        assert_eq!((ep.kind, ep.endpoint.as_deref()), (BackendKind::Anthropic, Some("anthropic")));
        assert!(ep.model.is_some());
        for pick in [Pick::Agent("claude-code".into()), Pick::Agent("gemini".into()), Pick::Endpoint("anthropic".into())] {
            assert_eq!(current_pick(&backend_for(&pick, &prefs), &prefs), pick);
        }
        // No endpoint stored means the default one.
        assert_eq!(current_pick(&Backend { kind: BackendKind::Anthropic, ..Default::default() }, &prefs), Pick::Endpoint(prefs.default_endpoint.clone()));
        assert!(!same_target(&acp, &claude) && !same_target(&acp, &backend_for(&Pick::Agent("opencode".into()), &prefs)));
        assert!(same_target(&acp, &Backend { cli_session: Some("s".into()), ..acp.clone() }));
    }

    #[test]
    fn auth_errors_are_told_apart_from_server_errors() {
        assert!(auth_error("/bin/cursor-agent: session/new: Authentication required"));
        assert!(auth_error("Not logged in. Please log in"));
        assert!(!auth_error("/opt/homebrew/bin/gemini: session/prompt: Internal error (503 service unavailable)"));
        assert!(!auth_error("Could not start /x: No such file"));
        let s = signin_for("cursor", "Cursor", "x: Authentication required").unwrap();
        assert_eq!(s.title, "Cursor needs you to sign in");
        assert_eq!(s.hint, "Run `cursor-agent login` in a terminal, then retry.");
        assert!(signin_for("kimi", "Kimi", "Authentication required").unwrap().hint.contains("own app"));
        assert_eq!(signin_for("cursor", "Cursor", "boom"), None);
    }

    #[test]
    fn version_and_home_are_shortened() {
        assert_eq!(short_version("opencode 1.18.30"), "v1.18.30");
        assert_eq!(short_version("v2.1"), "v2.1");
        assert_eq!(short_version("Cursor Agent"), "");
    }

    fn run(ctx: &egui::Context, events: Vec<egui::Event>, f: &mut dyn FnMut(&mut Ui)) {
        for n in 0..3 {
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(1280.0, 800.0))),
                events: if n == 2 { events.clone() } else { vec![] },
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| f(ui));
            out.textures_delta.clear();
        }
    }

    fn key(k: egui::Key) -> egui::Event {
        egui::Event::Key { key: k, physical_key: None, pressed: true, repeat: false, modifiers: Default::default() }
    }

    #[test]
    fn panel_and_setup_draw_and_keys_select() {
        let ctx = egui::Context::default();
        crate::ui::setup_style(&ctx);
        let (d, prefs) = (detect(), prefs());
        let backend = Backend::default();
        let col = Rect::from_min_size(pos2(10.0, 60.0), vec2(450.0, 700.0));
        let chip = Rect::from_min_size(pos2(10.0, 90.0), vec2(120.0, 22.0));
        let drive = |events: Vec<egui::Event>, ps: &mut PanelState| {
            let mut last = None;
            run(&ctx, events, &mut |_| last = panel(&ctx, ps, &PanelIn { detect: &d, prefs: &prefs, backend: &backend, running: false, column: col, chip, composer_top: 700.0 }));
            last
        };
        let mut ps = PanelState { fresh: true, ..Default::default() };
        assert!(drive(vec![], &mut ps).is_none(), "nothing happens without input");
        let two = drive(vec![key(egui::Key::ArrowDown), key(egui::Key::Enter)], &mut ps);
        assert!(matches!(two, Some(Act::Select(Pick::Agent(id))) if id == "gemini"), "second ready row");
        assert!(matches!(drive(vec![key(egui::Key::Escape)], &mut ps), Some(Act::Close)));
        run(&ctx, vec![], &mut |ui| {
            assert!(setup(ui, &Detect::default(), &prefs).is_none());
            assert!(setup(ui, &d, &prefs).is_none());
        });
    }
}
