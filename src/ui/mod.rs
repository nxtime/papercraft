//! egui chrome: menu bar, layers panel, design (properties) panel, floating toolbar, and the
//! projects home screen.

pub mod agent;
pub mod confirm;
pub mod design;
pub mod home;
pub mod inspector;
pub mod kit;
pub mod library;
pub mod lottie_icon;
pub mod motion;
pub mod pages;
pub mod palette;
pub mod panels;
pub mod settings;
#[cfg(not(target_arch = "wasm32"))]
pub mod update;

use crate::editor::shortcuts::{self, Action};
use crate::editor::{Editor, Tool};
use crate::model::{Color, NodeId, NodeKind};
use crate::theme::{self, Icon};
use egui::{Align2, Color32, RichText, Ui};
use std::collections::HashSet;

#[derive(Clone, Copy)]
pub enum WinCmd {
    Drag,
    Minimize,
    ToggleMaximize,
    Close,
}

#[derive(Default)]
pub struct UiState {
    pub collapsed: HashSet<NodeId>,
    pub renaming: Option<(NodeId, String)>,
    pub show_help: bool,
    pub show_grid_off: bool,
    /// Export settings (inspector Export section).
    pub export: crate::io::export::Spec,
    /// Export settings the user kept with "+", applied with one click (right-click removes).
    pub export_presets: Vec<crate::io::export::Spec>,
    /// The window has native chrome: a system menu bar (so no egui menu) and traffic lights
    /// inset into the toolbar / sidebar.
    pub native_chrome: bool,
    /// Windows: native decorations are off and `title_bar` draws the menu, drag area and
    /// window buttons.
    pub custom_titlebar: bool,
    /// Window action requested by the title bar, run by the app after the frame.
    pub win_cmd: Option<WinCmd>,
    /// The sidebar sits over an NSVisualEffectView, so paint it transparent.
    pub translucent_sidebar: bool,
    /// When set, the projects home screen is shown instead of the editor.
    pub home: Option<home::HomeView>,
    /// Name of the open project (shown in the toolbar).
    pub project_name: String,
    /// Folder of the open project (saved versions live under it).
    pub project_dir: Option<std::path::PathBuf>,
    /// Version being renamed in the History view `(id, draft name)`.
    pub version_renaming: Option<(String, String)>,
    /// Which comments the Comments view lists.
    pub comment_filter: panels::CommentFilter,
    /// Name typed into the History view's "Save version" box.
    pub version_draft: String,
    /// Saved versions of the current page, refreshed when the page or project changes.
    pub versions: Vec<crate::versions::Version>,
    pub versions_key: Option<(std::path::PathBuf, String)>,
    /// Autosave state for the toolbar subtitle, e.g. "Saved" or "Saving…".
    pub save_state: String,
    pub pages: pages::PagesUi,
    pub pending_removal: Option<confirm::PendingRemoval>,
    #[cfg(not(target_arch = "wasm32"))]
    pub updater: update::UpdaterUi,
    pub library: library::LibraryUi,
    /// Left sidebar shows the library instead of the layers.
    pub show_library: bool,
    /// ...or the design tab (variables, modes, components).
    pub show_design: bool,
    pub palette: palette::PaletteUi,
    pub settings: settings::SettingsUi,
    /// All panels hidden (⌘\\): just the canvas.
    pub hide_ui: bool,
    /// Present mode: only the canvas, with a presenter bar to step through the page's frames.
    pub present: bool,
    /// Frame shown by the presenter (index into the page's top-level frames).
    pub present_index: usize,
    /// Inspector shows the Prototype tab instead of Design.
    pub proto_tab: bool,
    /// Present mode's camera glide `(from, to, started)`, if one is running.
    pub present_glide: Option<(crate::geom::Camera, crate::geom::Camera, web_time::Instant)>,
    /// Present mode has shown a frame already, so the next one glides.
    pub present_glide_ready: bool,
    /// Layer being dragged in the layers panel, and where it would drop.
    pub layer_drag: Option<(NodeId, Option<LayerDrop>)>,
    /// Row rects of the last layers-panel frame (for drop targeting).
    pub layer_rows: Vec<(NodeId, egui::Rect, usize)>,
    /// Selection seen by the last layers-panel frame, to notice changes made on the canvas.
    pub last_selection: Vec<NodeId>,
    /// A layer the layers panel should scroll into view (and unfold its parents for).
    pub reveal: Option<NodeId>,
    /// Text typed into the inspector's feedback box.
    pub feedback_draft: String,
    /// Text in the layers panel's search box; only matching layers (and their parents) show.
    pub layer_filter: String,
    /// Name typed into the Design tab's new-variable box.
    pub variable_draft: String,
    /// Name typed into the Design tab's new-mode box.
    pub mode_draft: String,
    /// Name typed into the Design tab's new-token name box.
    pub token_name_draft: String,
    /// Value typed into the Design tab's new-token value box.
    pub token_val_draft: String,
    /// Filter text for the Design tab's components list.
    pub component_filter: String,
    /// Variable currently being renamed inline `(old_name, draft_new_name)`.
    pub var_renaming: Option<(String, String)>,
    /// The last tool an agent ran and when, for the toolbar's activity hint.
    pub agent: Option<(String, web_time::Instant)>,
    /// Recent agent tool calls, oldest first (the Agent view).
    pub agent_log: Vec<AgentCall>,
    /// Where an agent connects (MCP URL natively, the relay URL in the browser); empty until known.
    pub agent_endpoint: String,
    /// An extra sidebar view: history, comments or the agent log. None shows layers, library or design.
    pub side_extra: Option<panels::SideView>,
    /// The browser's remote-agent link (Agent menu); None everywhere but the web shell.
    pub agent_link: Option<agent::AgentUi>,
    /// A message shown over the home screen and the canvas (text, is_error); the shell that
    /// sets it also clears it. Only the web shell uses it: native shows errors in dialogs.
    pub toast: Option<(String, bool)>,
    /// Selected codegen target in the inspector's Code & AI Handoff section.
    pub code_target: String,
}

/// Where a dragged layer lands relative to a row.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LayerDrop {
    /// Above the row (in front of it in paint order).
    Above(NodeId),
    Below(NodeId),
    /// Inside a frame or group (on top of its children).
    Into(NodeId),
}

pub fn setup_style(ctx: &egui::Context) {
    theme::apply(ctx);
}

pub(crate) fn c32(c: Color) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r, c.g, c.b, c.a)
}
pub(crate) fn from32(c: Color32) -> Color {
    let [r, g, b, a] = c.to_srgba_unmultiplied();
    Color::rgba(r, g, b, a)
}

/// Section header, as in a macOS sidebar or inspector: 11pt semibold, secondary colour.
pub(crate) fn section(ui: &mut Ui, title: &str) {
    ui.add_space(2.0);
    ui.label(
        RichText::new(title.to_uppercase())
            .font(theme::semibold(theme::SECONDARY))
            .color(theme::pal(ui).tertiary),
    );
}

pub(crate) fn title_text(text: impl Into<String>) -> RichText {
    RichText::new(text).font(theme::semibold(theme::BODY))
}

/// The sidebar footer: the project, its save state, and the settings button.
fn account_footer(ui: &mut Ui, st: &mut UiState) {
    let p = theme::pal(ui);
    let name = if st.project_name.is_empty() {
        "Untitled".to_string()
    } else {
        st.project_name.clone()
    };
    let initial: String = name
        .chars()
        .next()
        .map(|c| c.to_uppercase().collect())
        .unwrap_or_default();
    ui.horizontal_centered(|ui| {
        let (r, _) = ui.allocate_exact_size(egui::vec2(30.0, 30.0), egui::Sense::hover());
        ui.painter().circle_filled(r.center(), 15.0, p.field);
        ui.painter().text(
            r.center(),
            Align2::CENTER_CENTER,
            initial,
            theme::semibold(theme::SECONDARY + 1.0),
            theme::accent(),
        );
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.set_max_width((ui.available_width() - 44.0).max(40.0));
            ui.add(
                egui::Label::new(RichText::new(name).font(theme::semibold(theme::BODY))).truncate(),
            );
            let sub = if st.save_state.is_empty() {
                "Local project".to_string()
            } else {
                st.save_state.clone()
            };
            ui.label(RichText::new(sub).size(theme::SECONDARY).color(p.secondary));
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if kit::lottie_button(ui, "settings", "Settings", 28.0).clicked() {
                st.settings.open = true;
            }
        });
    });
}

/// Most calls the Agent view keeps.
const AGENT_LOG_MAX: usize = 60;

/// One tool call an agent made.
pub struct AgentCall {
    pub name: String,
    pub at: web_time::Instant,
    /// The error text when the call failed.
    pub error: Option<String>,
}

impl UiState {
    /// Records an agent's tool call for the toolbar hint and the Agent view.
    pub fn log_agent(&mut self, name: &str, error: Option<String>) {
        let at = web_time::Instant::now();
        self.agent = Some((name.to_owned(), at));
        self.agent_log.push(AgentCall {
            name: name.to_owned(),
            at,
            error,
        });
        if self.agent_log.len() > AGENT_LOG_MAX {
            self.agent_log.remove(0);
        }
    }
}

/// How long after its last tool call an agent still counts as working.
const AGENT_WORKING_SECS: u64 = 6;

/// What an agent's tool call means in plain words, for the activity pill.
fn agent_verb(tool: &str) -> &'static str {
    match tool {
        "write_html" | "create_artboard" | "create_shape" => "Drawing layout",
        "update_styles" | "set_text_content" | "rename_nodes" => "Styling",
        "get_screenshot" | "get_jsx" | "get_design_spec" | "get_computed_styles" => {
            "Looking at the design"
        }
        "lint_design" | "verify_code" | "diff_frames" => "Checking the design",
        "set_variables" | "set_tokens" | "set_variable_mode" | "import_design_system" => {
            "Setting up tokens"
        }
        "create_component" | "create_instance" | "import_components" | "swap_instance" => {
            "Using components"
        }
        "delete_nodes" | "move_nodes" | "duplicate_nodes" => "Rearranging",
        "add_feedback" | "resolve_feedback" | "get_feedback" => "Reading notes",
        _ => "Working",
    }
}

impl UiState {
    /// What the agent is doing right now, if it called a tool within [`AGENT_WORKING_SECS`].
    pub fn agent_working(&self) -> Option<&'static str> {
        self.agent
            .as_ref()
            .filter(|(_, at)| at.elapsed().as_secs() < AGENT_WORKING_SECS)
            .map(|(tool, _)| agent_verb(tool))
    }
}

/// A pill with three bouncing dots and what the agent is doing; nothing when it is idle.
fn agent_pill(ui: &mut Ui, st: &UiState, with_text: bool) {
    let Some(verb) = st.agent_working() else {
        return;
    };
    ui.ctx().request_repaint();
    let p = theme::pal(ui);
    let t = ui.ctx().input(|i| i.time) as f32;
    let accent = theme::accent();
    egui::Frame::new()
        .fill(accent.gamma_multiply(0.16))
        .corner_radius(egui::CornerRadius::same(255))
        .inner_margin(egui::Margin::symmetric(10, 4))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                let (r, _) = ui.allocate_exact_size(egui::vec2(24.0, 14.0), egui::Sense::hover());
                for i in 0..3 {
                    let phase = t * 7.0 - i as f32 * 0.9;
                    let hop = phase.sin().max(0.0) * 4.0;
                    let x = r.left() + 4.0 + i as f32 * 8.0;
                    ui.painter()
                        .circle_filled(egui::pos2(x, r.center().y - hop), 2.5, accent);
                }
                if with_text {
                    ui.label(
                        RichText::new(format!("{verb}…"))
                            .size(theme::SECONDARY)
                            .color(p.text),
                    );
                }
            });
        });
}

/// A soft accent ring around the canvas while an agent works.
fn agent_glow(ctx: &egui::Context, st: &UiState, canvas: egui::Rect) {
    if st.agent_working().is_none() {
        return;
    }
    let t = ctx.input(|i| i.time) as f32;
    let pulse = 0.35 + 0.25 * (t * 3.0).sin();
    ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        "agent-glow".into(),
    ))
    .rect_stroke(
        canvas.shrink(1.5),
        f32::from(theme::RADIUS_PANEL),
        egui::Stroke::new(2.0, theme::accent().gamma_multiply(pulse)),
        egui::StrokeKind::Inside,
    );
}

/// Gap between the floating panels and around the canvas.
pub(crate) const PANEL_GAP: f32 = 12.0;

/// Canvas width under which the top bar drops the zoom pill (it is also on the keyboard).
const TIGHT_BAR_W: f32 = 520.0;

/// Canvas width under which the egui menu bar folds into a single "Menu" button.
const MENU_BAR_W: f32 = 1100.0;

/// Largest share of the window one side panel may take.
const SIDE_MAX_SHARE: f32 = 0.3;

/// Bottom strip of the canvas taken by the presenter bar; clicks there never follow links.
const PRESENT_BAR_H: f32 = 80.0;

/// The page's top-level frames, in layer order: what Present mode steps through.
fn present_frames(ed: &Editor) -> Vec<NodeId> {
    ed.doc
        .root
        .iter()
        .copied()
        .filter(|id| {
            ed.doc
                .get(*id)
                .is_some_and(|n| n.is_frame() && n.visible && !n.component)
        })
        .collect()
}

/// Starts Present mode on the first frame (or the selected one).
pub fn start_present(ed: &mut Editor, st: &mut UiState) {
    let frames = present_frames(ed);
    st.present_index = ed
        .selection
        .first()
        .and_then(|s| frames.iter().position(|f| f == s))
        .unwrap_or(0);
    st.present = true;
    st.present_glide = None;
    st.present_glide_ready = false;
    show_present_frame(ed, st);
}

/// How long the camera takes to glide from one presented frame to the next.
const PRESENT_GLIDE_SECS: f32 = 0.45;

fn show_present_frame(ed: &mut Editor, st: &mut UiState) {
    let frames = present_frames(ed);
    let Some(&id) = frames.get(st.present_index) else {
        return;
    };
    let from = ed.camera;
    ed.selection = vec![id];
    ed.run(Action::ZoomSelection);
    ed.selection.clear();
    let to = ed.camera;
    // Glide instead of cutting (the first frame of a presentation still snaps).
    st.present_glide = (st.present_glide_ready && from != to).then(|| {
        ed.camera = from;
        (from, to, web_time::Instant::now())
    });
    st.present_glide_ready = true;
}

/// Moves the camera along the glide started by [`show_present_frame`].
fn step_present_glide(ctx: &egui::Context, ed: &mut Editor, st: &mut UiState) {
    let Some((from, to, start)) = st.present_glide else {
        return;
    };
    let t = (start.elapsed().as_secs_f32() / PRESENT_GLIDE_SECS).min(1.0);
    let e = f64::from(motion::ease_in_out_cubic(t));
    if t >= 1.0 || !motion::enabled(ctx) {
        ed.camera = to;
        st.present_glide = None;
        return;
    }
    ed.camera.pan = crate::geom::v2(
        from.pan.x + (to.pan.x - from.pan.x) * e,
        from.pan.y + (to.pan.y - from.pan.y) * e,
    );
    // Zoom glides in log space so it feels even.
    ed.camera.zoom = (from.zoom.ln() + (to.zoom.ln() - from.zoom.ln()) * e).exp();
    ctx.request_repaint();
}

/// The presenter bar: previous and next frame, a counter and Exit; Esc and the arrow keys work too.
fn presenter(ctx: &egui::Context, ed: &mut Editor, st: &mut UiState, canvas: egui::Rect) {
    step_present_glide(ctx, ed, st);
    let frames = present_frames(ed);
    let total = frames.len();
    let (left, right, esc) = ctx.input(|i| {
        (
            i.key_pressed(egui::Key::ArrowLeft),
            i.key_pressed(egui::Key::ArrowRight),
            i.key_pressed(egui::Key::Escape),
        )
    });
    let mut step: i32 = i32::from(right) - i32::from(left);
    let mut exit = esc;
    // A click on a layer with a prototype link jumps to its frame.
    let clicked = ctx.input(|i| i.pointer.primary_clicked())
        && ctx
            .pointer_interact_pos()
            .is_some_and(|p| canvas.contains(p) && p.y < canvas.bottom() - PRESENT_BAR_H);
    if clicked {
        let at = ed.screen_to_world(ed.pointer);
        let hit = crate::editor::hit::hit_test(&ed.doc, at, ed.camera.zoom);
        let target = hit.and_then(|h| {
            std::iter::once(h)
                .chain(ed.doc.ancestors(h))
                .find_map(|id| ed.doc.get(id).and_then(|n| n.link))
        });
        if let Some(i) = target.and_then(|t| frames.iter().position(|f| *f == t)) {
            st.present_index = i;
            show_present_frame(ed, st);
        }
    }
    let p = theme::pal_from_ctx(ctx);
    egui::Area::new("presenter".into())
        .order(egui::Order::Foreground)
        .pivot(Align2::CENTER_BOTTOM)
        .fixed_pos(egui::pos2(canvas.center().x, canvas.bottom() - 24.0))
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(p.sidebar)
                .corner_radius(egui::CornerRadius::same(255))
                .inner_margin(egui::Margin::symmetric(8, 6))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        if kit::lottie_button(ui, "chevron-left", "Previous frame (←)", 32.0)
                            .clicked()
                        {
                            step = -1;
                        }
                        ui.label(
                            RichText::new(format!(
                                "{} / {}",
                                (st.present_index + 1).min(total.max(1)),
                                total.max(1)
                            ))
                            .font(theme::semibold(theme::BODY)),
                        );
                        if kit::lottie_button(ui, "chevron-right", "Next frame (→)", 32.0).clicked()
                        {
                            step = 1;
                        }
                        if kit::button(ui, kit::ButtonKind::Primary, None, "Exit").clicked() {
                            exit = true;
                        }
                    });
                });
        });
    egui::Area::new("presenting".into())
        .order(egui::Order::Foreground)
        .anchor(
            Align2::LEFT_TOP,
            egui::vec2(canvas.left() + 24.0, canvas.top() + 24.0),
        )
        .interactable(false)
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(p.sidebar)
                .corner_radius(egui::CornerRadius::same(255))
                .inner_margin(egui::Margin::symmetric(14, 6))
                .show(ui, |ui| {
                    ui.label(
                        RichText::new(format!("Presenting · {}", ed.page_name()))
                            .size(theme::SECONDARY)
                            .color(p.secondary),
                    );
                });
        });
    if step != 0 && total > 0 {
        st.present_index = (st.present_index as i32 + step).rem_euclid(total as i32) as usize;
        show_present_frame(ed, st);
    }
    if exit {
        st.present = false;
    }
}

/// Whether the chrome uses the floating rounded look (everything except the macOS translucent sidebar).
pub(crate) fn is_floating(translucent: bool) -> bool {
    !translucent || matches!(theme::choice(), theme::Choice::Catppuccin(_))
}

/// A rounded panel that floats on the backdrop and fills the space it is given.
fn floating_panel(ui: &mut Ui, fill: Color32, margin: egui::Margin, add: impl FnOnce(&mut Ui)) {
    egui::Frame::new()
        .fill(fill)
        .corner_radius(theme::RADIUS_PANEL)
        .inner_margin(margin)
        .show(ui, |ui| {
            ui.set_min_height(ui.available_height());
            ui.set_width(ui.available_width());
            add(ui);
        });
}

/// Paints the four corners of `rect` outside a rounded rectangle of radius `r`, so the canvas
/// (drawn by the renderer underneath the UI) shows with rounded corners.
fn round_corners(painter: &egui::Painter, rect: egui::Rect, r: f32, colour: Color32) {
    const SEGMENTS: usize = 10;
    let mut mesh = egui::Mesh::default();
    let corners = [
        (rect.left_top(), egui::vec2(1.0, 1.0)),
        (rect.right_top(), egui::vec2(-1.0, 1.0)),
        (rect.right_bottom(), egui::vec2(-1.0, -1.0)),
        (rect.left_bottom(), egui::vec2(1.0, -1.0)),
    ];
    for (corner, dir) in corners {
        let centre = corner + dir * r;
        let base = mesh.vertices.len() as u32;
        mesh.colored_vertex(corner, colour);
        for i in 0..=SEGMENTS {
            let a = std::f32::consts::FRAC_PI_2 * i as f32 / SEGMENTS as f32;
            // From the point on one edge to the point on the other edge, around `centre`.
            let v = egui::vec2(-dir.x * a.cos(), -dir.y * a.sin());
            mesh.colored_vertex(centre + v * r, colour);
        }
        for i in 0..SEGMENTS as u32 {
            mesh.add_triangle(base, base + 1 + i, base + 2 + i);
        }
    }
    painter.add(egui::Shape::mesh(mesh));
}

/// Lay out the chrome and return the remaining canvas rectangle (logical px).
pub fn draw(ui: &mut Ui, ed: &mut Editor, st: &mut UiState) -> egui::Rect {
    if st.custom_titlebar {
        let p = theme::pal(ui);
        egui::Panel::top("titlebar")
            .exact_size(TITLEBAR_H)
            .frame(egui::Frame::NONE.fill(p.window))
            .show(ui, |ui| title_bar(ui, ed, st));
    }
    if let Some(a) = &mut st.agent_link {
        a.window(ui.ctx());
    }
    if let Some((text, error)) = &st.toast {
        toast(ui.ctx(), text, *error);
    }
    if let Some(hv) = &mut st.home {
        home::draw(ui, hv, st.native_chrome, st.translucent_sidebar);
        #[cfg(not(target_arch = "wasm32"))]
        {
            st.updater.open |= std::mem::take(&mut hv.show_updates);
            st.updater.show(ui.ctx());
        }
        return egui::Rect::NOTHING;
    }
    for ev in std::mem::take(&mut ed.ui_events) {
        use crate::editor::commands::UiEvent;
        match ev {
            UiEvent::TogglePalette => st.palette.toggle(),
            UiEvent::Rename(id) => {
                if let Some(n) = ed.doc.get(id) {
                    st.renaming = Some((id, n.name.clone()));
                    st.show_library = false;
                    st.show_design = false;
                    st.hide_ui = false;
                }
            }
            UiEvent::ToggleUi => st.hide_ui = !st.hide_ui,
            UiEvent::ToggleHelp => st.show_help = !st.show_help,
            UiEvent::ToggleGrid => st.show_grid_off = !st.show_grid_off,
        }
    }
    if st.hide_ui || st.present {
        #[cfg(not(target_arch = "wasm32"))]
        st.updater.show(ui.ctx());
        confirm::show(ui.ctx(), ed, st);
        let canvas = ui.available_rect_before_wrap();
        if st.present {
            presenter(ui.ctx(), ed, st, canvas);
        }
        palette::draw(ui.ctx(), ed, st, canvas);
        return canvas;
    }
    let p = theme::pal(ui);
    let sidebar_fill = theme::sidebar_fill(st.translucent_sidebar, p);
    let floating = is_floating(st.translucent_sidebar);
    let gap = PANEL_GAP;
    let side_frame = |left: i8, right: i8, top: i8, bottom: i8| {
        if floating {
            egui::Frame::NONE
                .fill(p.backdrop)
                .inner_margin(egui::Margin {
                    left,
                    right,
                    top,
                    bottom,
                })
        } else {
            egui::Frame::NONE
        }
    };
    // Narrow windows keep the canvas usable: the side panels never take more than 30% each.
    let window_w = ui.max_rect().width();
    let side_max = |min: f32| (window_w * SIDE_MAX_SHARE).clamp(min, 430.0);
    egui::Panel::left("layers")
        .resizable(true)
        .default_size(252.0 + 2.0 * gap)
        .size_range(190.0..=side_max(190.0))
        .frame(if floating {
            side_frame(gap as i8, gap as i8, gap as i8, gap as i8)
        } else {
            egui::Frame::NONE
                .fill(sidebar_fill)
                .inner_margin(egui::Margin::symmetric(10, 8))
        })
        .show(ui, |ui| {
            let mut body = |ui: &mut Ui| {
                if floating {
                    egui::Panel::bottom("side-footer")
                        .exact_size(48.0)
                        .frame(egui::Frame::NONE)
                        .show(ui, |ui| account_footer(ui, st));
                    ui.horizontal(|ui| {
                        kit::badge(ui, Icon::Pen, theme::accent(), 28.0);
                        ui.label(RichText::new("PaperCraft").font(theme::semibold(16.0)));
                    });
                    ui.add_space(10.0);
                }
                layers(ui, ed, st)
            };
            if floating {
                floating_panel(ui, sidebar_fill, egui::Margin::symmetric(14, 12), body);
            } else {
                body(ui);
            }
        });
    egui::Panel::right("design")
        .resizable(true)
        .default_size(272.0 + 2.0 * gap)
        .size_range(230.0..=side_max(230.0))
        .frame(if floating {
            side_frame(gap as i8, gap as i8, gap as i8, gap as i8)
        } else {
            egui::Frame::NONE
                .fill(p.inspector)
                .inner_margin(egui::Margin::symmetric(14, 10))
        })
        .show(ui, |ui| {
            if floating {
                floating_panel(ui, p.inspector, egui::Margin::symmetric(14, 12), |ui| {
                    inspector::panel(ui, ed, st)
                });
            } else {
                inspector::panel(ui, ed, st)
            }
        });
    egui::Panel::top("toolbar")
        .exact_size(if floating {
            theme::TOOLBAR_H + gap * 2.0
        } else if st.native_chrome {
            theme::TOOLBAR_H
        } else {
            34.0
        })
        .frame(if floating {
            side_frame(0, 0, gap as i8, gap as i8)
        } else {
            egui::Frame::NONE
                .fill(p.window)
                .inner_margin(egui::Margin::symmetric(12, 0))
        })
        .show(ui, |ui| {
            if floating {
                floating_panel(ui, p.sidebar, egui::Margin::symmetric(14, 0), |ui| {
                    ui.set_height(theme::TOOLBAR_H);
                    top_bar(ui, ed, st)
                });
            } else {
                top_bar(ui, ed, st)
            }
        });
    if floating {
        egui::Panel::bottom("canvas-gap")
            .exact_size(gap)
            .frame(egui::Frame::NONE.fill(p.backdrop))
            .show(ui, |_| {});
    }
    let canvas = ui.available_rect_before_wrap();
    if floating {
        round_corners(
            ui.painter(),
            canvas,
            f32::from(theme::RADIUS_PANEL),
            p.backdrop,
        );
    }
    toolbar(ui.ctx(), ed, canvas);
    agent_glow(ui.ctx(), st, canvas);
    status(ui.ctx(), ed, canvas);
    palette::draw(ui.ctx(), ed, st, canvas);
    if st.show_help {
        help(ui.ctx(), st, canvas);
    }
    settings::show(ui.ctx(), st);
    #[cfg(not(target_arch = "wasm32"))]
    st.updater.show(ui.ctx());
    confirm::show(ui.ctx(), ed, st);
    canvas
}

fn menu_item(ui: &mut Ui, ed: &mut Editor, label: &str, action: Action) {
    if ui
        .add(egui::Button::new(label).shortcut_text(shortcuts::chord_text(action)))
        .clicked()
    {
        ed.run(action);
        ui.close();
    }
}

fn menu_entries(ui: &mut Ui, ed: &mut Editor, entries: &[shortcuts::MenuEntry]) {
    for e in entries {
        match e {
            shortcuts::MenuEntry::Item(label, a) => menu_item(ui, ed, label, *a),
            shortcuts::MenuEntry::Separator => {
                ui.separator();
            }
            shortcuts::MenuEntry::Submenu(label, items) => {
                ui.menu_button(*label, |ui| menu_entries(ui, ed, items));
            }
        }
    }
}

/// The unified toolbar: document title on the left, zoom and help on the right. Without native
/// chrome (Linux) it also carries an egui menu bar.
fn top_bar(ui: &mut Ui, ed: &mut Editor, st: &mut UiState) {
    let p = theme::pal(ui);
    // In a narrow window only the essentials stay: zoom, present and share.
    let compact = ui.max_rect().width() < 720.0;
    let tight = ui.max_rect().width() < TIGHT_BAR_W;
    ui.horizontal_centered(|ui| {
        if !st.native_chrome && !st.custom_titlebar {
            if ui.max_rect().width() < MENU_BAR_W {
                // Not enough room for every menu: they fold into one.
                ui.scope(|ui| {
                    flat_menus(ui);
                    let menu = ui.menu_button("Menu", |ui| flat_menu(ui, ed));
                    let _ = menu;
                });
            } else {
                ui.scope(|ui| menu_bar(ui, ed, &mut st.agent_link));
            }
            ui.add_space(6.0);
            ui.separator();
            ui.add_space(6.0);
        }
        if theme::icon_button(ui, theme::Icon::Grid, 26.0, false)
            .on_hover_text("All projects")
            .clicked()
        {
            ed.requests.push(crate::editor::AppRequest::Home);
        }
        ui.add_space(4.0);
        let title = if st.project_name.is_empty() {
            ed.doc.name.clone()
        } else {
            st.project_name.clone()
        };
        ui.vertical(|ui| {
            // Leave room for the zoom pill and icons on the right; long titles truncate.
            ui.set_max_width(
                (ui.available_width()
                    - if tight {
                        150.0
                    } else if compact {
                        300.0
                    } else {
                        420.0
                    })
                .max(40.0),
            );
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.add_space(if st.native_chrome { 6.0 } else { 1.0 });
            let state = if !st.save_state.is_empty() {
                st.save_state.clone()
            } else if ed.is_dirty() {
                "Edited".into()
            } else {
                String::new()
            };
            ui.horizontal(|ui| {
                ui.add(egui::Label::new(title_text(title)).truncate());
                if !state.is_empty() {
                    kit::chip(ui, &state, state == "Saved");
                }
            });
            let sub = ed.page_name().to_string();
            ui.add(
                egui::Label::new(RichText::new(sub).size(theme::SECONDARY).color(p.secondary))
                    .truncate(),
            );
        });
        agent_pill(ui, st, !tight);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            let share = kit::button(ui, kit::ButtonKind::Primary, Some(Icon::Export), "Share");
            egui::Popup::menu(&share).show(|ui| {
                ui.set_min_width(190.0);
                for (icon, label, action) in [
                    (Icon::Image, "Export PNG…", Action::ExportPng),
                    (Icon::Pen, "Export SVG…", Action::ExportSvg),
                    (Icon::Image, "Copy as PNG", Action::CopyAsPng),
                    (Icon::Pen, "Copy as SVG", Action::CopyAsSvg),
                ] {
                    if icon_menu_item(ui, icon, label).clicked() {
                        ed.run(action);
                        ui.close();
                    }
                }
            });
            ui.add_space(6.0);
            if let Some(a) = &st.agent_link {
                a.dot(ui);
            }
            #[cfg(not(target_arch = "wasm32"))]
            if !compact
                && theme::icon_button(ui, theme::Icon::Restore, 26.0, st.updater.open)
                    .on_hover_text("Check for updates")
                    .clicked()
            {
                st.updater.open = true;
            }
            if kit::play_button(ui, "Present").clicked() {
                start_present(ed, st);
            }
            if !compact {
                if kit::history_button(ui, true, ed.history.redo_len() > 0).clicked() {
                    ed.run(Action::Redo);
                }
                if kit::history_button(ui, false, ed.history.can_undo()).clicked() {
                    ed.run(Action::Undo);
                }
                ui.add_space(6.0);
                // The floating layout has Settings in the sidebar footer already.
                if !is_floating(st.translucent_sidebar)
                    && kit::lottie_button(ui, "settings", "Settings", 26.0).clicked()
                {
                    st.settings.open = true;
                }
            }
            if !compact
                && theme::icon_button(ui, theme::Icon::Question, 26.0, st.show_help)
                    .on_hover_text("Keyboard shortcuts")
                    .clicked()
            {
                st.show_help = !st.show_help;
            }
            ui.add_space(8.0);
            if !tight {
                egui::Frame::new()
                    .fill(p.field)
                    .corner_radius(egui::CornerRadius::same(255))
                    .inner_margin(egui::Margin::symmetric(6, 2))
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        ui.horizontal(|ui| {
                            if theme::icon_button(ui, theme::Icon::Minus, 26.0, false)
                                .on_hover_text("Zoom out")
                                .clicked()
                            {
                                ed.run(Action::ZoomOut);
                            }
                            let z = ui.add(
                                egui::Button::new(
                                    RichText::new(format!("{:.0}%", ed.camera.zoom * 100.0))
                                        .font(theme::semibold(theme::SECONDARY + 1.0)),
                                )
                                .frame(false)
                                .min_size(egui::vec2(48.0, 22.0)),
                            );
                            if z.on_hover_text("Zoom to fit").clicked() {
                                ed.run(Action::ZoomFit);
                            }
                            if theme::icon_button(ui, theme::Icon::Plus, 26.0, false)
                                .on_hover_text("Zoom in")
                                .clicked()
                            {
                                ed.run(Action::ZoomIn);
                            }
                        });
                    });
            }
        });
    });
}

/// egui menus for platforms without a native menu bar (inline, so the toolbar row keeps
/// the title and zoom controls).
fn menu_bar(ui: &mut Ui, ed: &mut Editor, agent: &mut Option<agent::AgentUi>) {
    flat_menus(ui);
    for (title, entries) in shortcuts::menus() {
        ui.menu_button(title, |ui| {
            menu_entries(ui, ed, &entries);
        });
    }
    if let Some(a) = agent {
        a.menu(ui);
    }
    theme_menu(ui);
}

/// Width of a popup menu row with an icon.
const ICON_MENU_W: f32 = 200.0;

/// A menu row with a leading icon, hover tint and the pointing-hand cursor.
pub(super) fn icon_menu_item(ui: &mut Ui, icon: Icon, label: &str) -> egui::Response {
    let p = theme::pal(ui);
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(ICON_MENU_W, 32.0), egui::Sense::click());
    let hover = motion::hover(ui.ctx(), resp.id.with("menu-item"), &resp);
    if hover > 0.0 {
        ui.painter()
            .rect_filled(rect, theme::RADIUS_MD, p.hover.gamma_multiply(hover));
    }
    let at = egui::Rect::from_center_size(
        egui::pos2(rect.left() + 18.0, rect.center().y),
        egui::vec2(16.0, 16.0),
    );
    theme::paint_icon(ui.painter(), at, icon, p.secondary);
    ui.painter().text(
        egui::pos2(rect.left() + 34.0, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(theme::BODY),
        p.text,
    );
    resp
}

/// Every menu in one popup, one after the other under its title: no submenus to open.
fn flat_menu(ui: &mut Ui, ed: &mut Editor) {
    let p = theme::pal(ui);
    ui.set_min_width(220.0);
    egui::ScrollArea::vertical()
        .max_height((ui.ctx().content_rect().height() - 120.0).max(160.0))
        .show(ui, |ui| {
            for (title, entries) in shortcuts::menus() {
                ui.add_space(4.0);
                ui.label(
                    RichText::new(title.to_uppercase())
                        .font(theme::semibold(theme::SECONDARY))
                        .color(p.tertiary),
                );
                menu_entries(ui, ed, &entries);
            }
        });
}

/// Menu titles read as plain text: no fill or border at rest, a tint on hover/press.
fn flat_menus(ui: &mut Ui) {
    let p = theme::pal(ui);
    ui.spacing_mut().button_padding = egui::vec2(8.0, 4.0);
    ui.spacing_mut().item_spacing.x = 0.0;
    let w = &mut ui.visuals_mut().widgets;
    w.inactive.weak_bg_fill = Color32::TRANSPARENT;
    w.hovered.weak_bg_fill = p.hover;
    w.active.weak_bg_fill = p.pressed;
    w.open.weak_bg_fill = p.pressed;
    for s in [&mut w.inactive, &mut w.hovered, &mut w.active, &mut w.open] {
        s.bg_stroke = egui::Stroke::NONE;
    }
}

fn theme_menu(ui: &mut Ui) {
    flat_menus(ui);
    ui.menu_button("Theme", |ui| {
        for c in theme::Choice::ALL {
            if ui
                .selectable_label(theme::choice() == c, c.label())
                .clicked()
            {
                theme::apply_choice(ui.ctx(), c);
                c.save();
                ui.close();
            }
        }
        ui.separator();
        let mut animations = motion::enabled(ui.ctx());
        if kit::checkbox(ui, &mut animations, "Animations").changed() {
            motion::set_enabled(ui.ctx(), animations);
        }
    });
}

const TITLEBAR_H: f32 = 32.0;
const WIN_BTN_W: f32 = 46.0;

/// Custom window chrome: menus on the left, empty space drags the window, buttons on the right.
fn title_bar(ui: &mut Ui, ed: &mut Editor, st: &mut UiState) {
    let p = theme::pal(ui);
    let rect = ui.max_rect();
    let drag = ui.interact(rect, ui.id().with("drag"), egui::Sense::click_and_drag());
    if drag.double_clicked() {
        st.win_cmd = Some(WinCmd::ToggleMaximize);
    } else if drag.drag_started() {
        st.win_cmd = Some(WinCmd::Drag);
    }
    ui.horizontal_centered(|ui| {
        ui.add_space(8.0);
        if st.home.is_some() {
            theme_menu(ui);
        } else {
            menu_bar(ui, ed, &mut st.agent_link);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            for (cmd, hover) in [
                (WinCmd::Close, Color32::from_rgb(0xe8, 0x11, 0x23)),
                (WinCmd::ToggleMaximize, p.hover),
                (WinCmd::Minimize, p.hover),
            ] {
                let (r, resp) =
                    ui.allocate_exact_size(egui::vec2(WIN_BTN_W, TITLEBAR_H), egui::Sense::click());
                let hot = resp.hovered();
                if hot {
                    ui.painter().rect_filled(r, 0.0, hover);
                }
                let ink = if hot && matches!(cmd, WinCmd::Close) {
                    Color32::WHITE
                } else {
                    p.text
                };
                let stroke = egui::Stroke::new(1.0, ink);
                let c = r.center();
                let h = 5.0;
                match cmd {
                    WinCmd::Minimize => {
                        ui.painter().hline(c.x - h..=c.x + h, c.y, stroke);
                    }
                    WinCmd::ToggleMaximize => {
                        ui.painter().rect_stroke(
                            egui::Rect::from_center_size(c, egui::Vec2::splat(h * 2.0)),
                            0.0,
                            stroke,
                            egui::StrokeKind::Inside,
                        );
                    }
                    _ => {
                        let d = egui::vec2(h, h);
                        let e = egui::vec2(h, -h);
                        ui.painter().line_segment([c - d, c + d], stroke);
                        ui.painter().line_segment([c - e, c + e], stroke);
                    }
                }
                if resp.clicked() {
                    st.win_cmd = Some(cmd);
                }
            }
        });
    });
}

/// The sidebar views as a strip that fills the sidebar: layers, library, design (variables, modes,
/// components), history, comments and the agent log. Each tab grows with the width; when there is
/// room it also shows its name.
fn side_tabs(ui: &mut Ui, st: &mut UiState) {
    const TABS: [(i32, &str); 6] = [
        (0, "Layers"),
        (1, "Library"),
        (2, "Design"),
        (3, "History"),
        (4, "Notes"),
        (5, "Agent"),
    ];
    let p = theme::pal(ui);
    let current = match st.side_extra {
        Some(panels::SideView::History) => 3,
        Some(panels::SideView::Comments) => 4,
        Some(panels::SideView::Agent) => 5,
        None if st.show_design => 2,
        None => i32::from(st.show_library),
    };
    let pad = 3.0;
    let gap = 2.0;
    egui::Frame::new()
        .fill(p.field)
        .corner_radius(theme::RADIUS_LG)
        .inner_margin(pad as i8)
        .show(ui, |ui| {
            let total = ui.available_width();
            let w = ((total - gap * (TABS.len() as f32 - 1.0)) / TABS.len() as f32).floor();
            // With room for it, the active tab shows its name and takes the extra width.
            let wide = w >= 62.0;
            ui.spacing_mut().item_spacing.x = gap;
            ui.horizontal(|ui| {
                for (tab, tip) in TABS {
                    let (rect, resp) =
                        ui.allocate_exact_size(egui::vec2(w, 30.0), egui::Sense::click());
                    let on = current == tab;
                    let hover = motion::hover(ui.ctx(), resp.id.with("side-tab"), &resp);
                    let fill = if on {
                        p.sidebar
                    } else {
                        p.hover.gamma_multiply(hover)
                    };
                    ui.painter().rect_filled(rect, 9.0, fill);
                    let tint = if on { theme::accent() } else { p.secondary };
                    let hovered = resp.hovered();
                    let icon_box = if wide {
                        egui::Rect::from_center_size(
                            egui::pos2(rect.left() + 18.0, rect.center().y),
                            egui::vec2(16.0, 16.0),
                        )
                    } else {
                        egui::Rect::from_center_size(rect.center(), egui::vec2(16.0, 16.0))
                    };
                    match tab {
                        0 => {
                            lottie_icon::paint_hover(ui, resp.id, "layers", icon_box, tint, hovered)
                        }
                        1 => theme::paint_icon(ui.painter(), icon_box, Icon::Library, tint),
                        2 => lottie_icon::paint_hover(
                            ui,
                            resp.id,
                            "layout-dashboard",
                            icon_box,
                            tint,
                            hovered,
                        ),
                        3 => theme::paint_icon(ui.painter(), icon_box, Icon::Restore, tint),
                        4 => lottie_icon::paint_hover(
                            ui,
                            resp.id,
                            "message-square-quote",
                            icon_box,
                            tint,
                            hovered,
                        ),
                        _ => lottie_icon::paint_hover(
                            ui, resp.id, "sparkles", icon_box, tint, hovered,
                        ),
                    }
                    if wide {
                        ui.painter().with_clip_rect(rect).text(
                            egui::pos2(rect.left() + 32.0, rect.center().y),
                            egui::Align2::LEFT_CENTER,
                            tip,
                            if on {
                                theme::semibold(theme::SECONDARY + 1.0)
                            } else {
                                egui::FontId::proportional(theme::SECONDARY + 1.0)
                            },
                            if on { p.text } else { p.secondary },
                        );
                    }
                    let tooltip = match tab {
                        4 => "Comments",
                        _ => tip,
                    };
                    if resp.on_hover_text(tooltip).clicked() {
                        st.show_library = tab == 1;
                        st.show_design = tab == 2;
                        st.side_extra = match tab {
                            3 => Some(panels::SideView::History),
                            4 => Some(panels::SideView::Comments),
                            5 => Some(panels::SideView::Agent),
                            _ => None,
                        };
                    }
                }
            });
        });
}

fn layers(ui: &mut Ui, ed: &mut Editor, st: &mut UiState) {
    theme::figma_fields(ui);
    pages::panel(ui, ed, st);
    ui.add_space(8.0);
    side_tabs(ui, st);
    if !(st.show_library || st.show_design || st.side_extra.is_some()) {
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            section(ui, "Layers");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let some = !ed.selection.is_empty();
                ui.add_enabled_ui(some, |ui| {
                    if theme::icon_button(ui, Icon::ChevronDown, 20.0, false)
                        .on_hover_text("Send backward")
                        .clicked()
                    {
                        ed.run(Action::Backward);
                    }
                    if theme::icon_button(ui, Icon::ChevronUp, 20.0, false)
                        .on_hover_text("Bring forward")
                        .clicked()
                    {
                        ed.run(Action::Forward);
                    }
                });
            });
        });
    }
    ui.add_space(2.0);
    if let Some(view) = st.side_extra {
        match view {
            panels::SideView::History => panels::history(ui, ed, st),
            panels::SideView::Comments => panels::comments(ui, ed, st),
            panels::SideView::Agent => panels::agent(ui, st),
        }
        return;
    }
    if st.show_library {
        library::panel(ui, ed, st);
        return;
    }
    if st.show_design {
        design::panel(ui, ed, st);
        return;
    }
    // Selecting on the canvas brings the layer into view and unfolds what hides it.
    if st.last_selection != ed.selection {
        st.last_selection = ed.selection.clone();
        if let Some(&id) = ed.selection.last() {
            for a in ed.doc.ancestors(id) {
                st.collapsed.remove(&a);
            }
            st.reveal = Some(id);
        }
    }
    if ed.doc.nodes.len() > 12 || !st.layer_filter.is_empty() {
        ui.add(
            egui::TextEdit::singleline(&mut st.layer_filter)
                .hint_text("Search layers")
                .desired_width(f32::INFINITY)
                .margin(crate::theme::INPUT_MARGIN),
        );
        ui.add_space(2.0);
    }
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            let roots = ed.doc.root.clone();
            if roots.is_empty() {
                ui.label(
                    RichText::new("Nothing here yet. Pick a tool below\nand draw on the canvas.")
                        .size(theme::SECONDARY)
                        .color(theme::pal(ui).secondary),
                );
            }
            st.layer_rows.clear();
            for id in roots.iter().rev() {
                let index = st.layer_rows.len();
                motion::pop_in(ui, ("layer-pop", *id), index, |ui| {
                    layer_row(ui, ed, st, *id, 0)
                });
            }
            layer_drag(ui, ed, st);
        });
}

/// Drag-and-drop reordering in the layers tree: above / below a row, or into a frame/group.
fn layer_drag(ui: &mut Ui, ed: &mut Editor, st: &mut UiState) {
    let Some((dragged, _)) = st.layer_drag else {
        return;
    };
    let Some(pos) = ui.input(|i| i.pointer.interact_pos()) else {
        return;
    };
    let target = st
        .layer_rows
        .iter()
        .find(|(_, r, _)| r.y_range().contains(pos.y))
        .copied();
    let drop = target.and_then(|(id, r, _)| {
        if id == dragged || ed.doc.is_ancestor(dragged, id) {
            return None;
        }
        let n = ed.doc.get(id)?;
        let t = (pos.y - r.top()) / r.height();
        Some(if n.is_container() && (0.3..0.7).contains(&t) {
            LayerDrop::Into(id)
        } else if t < 0.5 {
            LayerDrop::Above(id)
        } else {
            LayerDrop::Below(id)
        })
    });
    st.layer_drag = Some((dragged, drop));
    // Indicator.
    if let (Some(d), Some((_, r, depth))) = (drop, target) {
        let x0 = r.left() + 4.0 + depth as f32 * 14.0 + 16.0;
        let stroke = egui::Stroke::new(2.0, theme::accent());
        match d {
            LayerDrop::Above(_) => {
                ui.painter().hline(x0..=r.right(), r.top(), stroke);
            }
            LayerDrop::Below(_) => {
                ui.painter().hline(x0..=r.right(), r.bottom(), stroke);
            }
            LayerDrop::Into(_) => {
                ui.painter()
                    .rect_stroke(r, 6.0, stroke, egui::StrokeKind::Inside);
            }
        }
    }
    if ui.input(|i| i.pointer.any_released()) {
        st.layer_drag = None;
        if let Some(d) = drop {
            drop_layer(ed, dragged, d);
        }
    }
}

/// Move a layer in the tree (keeping its position on the canvas).
pub fn drop_layer(ed: &mut Editor, id: NodeId, d: LayerDrop) {
    let (parent, index) = match d {
        LayerDrop::Into(c) => (Some(c), None),
        LayerDrop::Above(t) | LayerDrop::Below(t) => {
            let parent = ed.doc.get(t).and_then(|n| n.parent);
            let list: Vec<NodeId> = ed
                .doc
                .children(parent)
                .iter()
                .copied()
                .filter(|x| *x != id)
                .collect();
            let i = list.iter().position(|x| *x == t).unwrap_or(0);
            (
                parent,
                Some(if matches!(d, LayerDrop::Above(_)) {
                    i + 1
                } else {
                    i
                }),
            )
        }
    };
    if parent.is_some_and(|p| p == id || ed.doc.is_ancestor(id, p)) {
        return;
    }
    ed.edit("Move layer", |tx| {
        let same = tx.doc().get(id).and_then(|n| n.parent) == parent;
        if same {
            if let Some(i) = index {
                tx.move_to_index(id, i);
            } else {
                let len = tx.doc().children(parent).len();
                tx.move_to_index(id, len);
            }
        } else {
            tx.reparent(id, parent, index);
        }
    });
    ed.selection = vec![id];
}

fn node_icon(n: &crate::model::Node) -> Icon {
    if n.component {
        return Icon::Component;
    }
    if n.instance.is_some() && n.parent.is_none_or(|_| n.is_frame()) && n.is_frame() {
        return Icon::Instance;
    }
    if n.style
        .fills
        .iter()
        .any(|p| matches!(p, crate::model::Paint::Image { .. }))
        && matches!(n.kind, NodeKind::Rect)
    {
        return Icon::Image;
    }
    kind_icon(&n.kind)
}

fn kind_icon(k: &NodeKind) -> Icon {
    match k {
        NodeKind::Group => Icon::Group,
        NodeKind::Connector { .. } => Icon::Arrow,
        NodeKind::Sticky { .. } => Icon::Sticky,
        NodeKind::Path { .. } => Icon::Pen,
        NodeKind::Polygon { .. } => Icon::Polygon,
        NodeKind::Star { .. } => Icon::Star,
        NodeKind::Frame { .. } => Icon::Frame,
        NodeKind::Rect => Icon::Rect,
        NodeKind::Ellipse => Icon::Ellipse,
        NodeKind::Line { .. } => Icon::Line,
        NodeKind::Text { .. } => Icon::Text,
        NodeKind::Stroke { .. } => Icon::Ink,
    }
}

/// One sidebar row, drawn like an NSOutlineView row: 24pt tall, 6pt radius selection in the
/// accent colour, subtle hover, disclosure chevron, symbol, name, and eye/lock on hover.
/// Whether `id` or anything below it has a name containing `needle` (lowercase).
fn layer_matches(doc: &crate::model::Document, id: NodeId, needle: &str) -> bool {
    doc.get(id).is_some_and(|n| {
        n.name.to_lowercase().contains(needle)
            || n.kind
                .text()
                .is_some_and(|t| t.to_lowercase().contains(needle))
            || n.children.iter().any(|c| layer_matches(doc, *c, needle))
    })
}

fn layer_row(ui: &mut Ui, ed: &mut Editor, st: &mut UiState, id: NodeId, depth: usize) {
    let Some(n) = ed.doc.get(id).cloned() else {
        return;
    };
    let needle = st.layer_filter.trim().to_lowercase();
    if !needle.is_empty() && !layer_matches(&ed.doc, id, &needle) {
        return;
    }
    let p = theme::pal(ui);
    let selected = ed.selection.contains(&id);
    let width = ui.available_width();
    let (row, resp) =
        ui.allocate_exact_size(egui::vec2(width, 30.0), egui::Sense::click_and_drag());
    st.layer_rows.push((id, row, depth));
    if st.reveal == Some(id) {
        ui.scroll_to_rect(row, None);
        st.reveal = None;
    }
    if resp.drag_started() && !n.locked {
        st.layer_drag = Some((id, None));
    }
    let hovered = ui.rect_contains_pointer(row);
    if selected {
        ui.painter().rect_filled(row, 10.0, p.field);
    } else if hovered {
        ui.painter().rect_filled(row, 10.0, p.hover);
    }
    let fg = if n.visible { p.text } else { p.secondary };
    let fg2 = p.secondary;
    let mut x = row.left() + 4.0 + depth as f32 * 14.0;
    let cy = row.center().y;
    let sq = |x: f32, s: f32| {
        egui::Rect::from_center_size(egui::pos2(x + s / 2.0, cy), egui::vec2(s, s))
    };

    // Disclosure chevron.
    if !n.children.is_empty() {
        let open = !st.collapsed.contains(&id);
        let r = sq(x, 14.0);
        let cr = ui.interact(
            r.expand(2.0),
            ui.id().with(("disclose", id)),
            egui::Sense::click(),
        );
        lottie_icon::toggle(
            ui,
            ("disclose-icon", id),
            "chevron-right",
            r.shrink(2.0),
            fg2,
            open,
        );
        if cr.clicked() {
            if open {
                st.collapsed.insert(id);
            } else {
                st.collapsed.remove(&id);
            }
        }
    }
    x += 16.0;
    theme::paint_icon(ui.painter(), sq(x, 14.0), node_icon(&n), theme::accent());
    if n.open_comments() > 0 {
        let at = egui::pos2(x + 14.0, cy - 6.0);
        ui.painter().circle_filled(at, 4.5, Color32::WHITE);
        ui.painter()
            .circle_filled(at, 3.0, Color32::from_rgb(250, 189, 47));
    }
    x += 20.0;

    // Trailing eye / lock buttons: always shown when hidden or locked, otherwise on hover.
    let mut right = row.right() - 4.0;
    let mut trailing =
        |ui: &mut Ui, paint: &dyn Fn(&Ui, egui::Rect), tip: &str, show: bool| -> bool {
            let r =
                egui::Rect::from_center_size(egui::pos2(right - 9.0, cy), egui::vec2(18.0, 18.0));
            right -= 20.0;
            if !show {
                return false;
            }
            let b = ui.interact(r, ui.id().with((tip, id)), egui::Sense::click());
            let t = motion::hover(ui.ctx(), ui.id().with(("trail-hover", tip, id)), &b);
            if t > 0.0 {
                let tint = p.hover;
                ui.painter().rect_filled(r, 5.0, tint.gamma_multiply(t));
            }
            paint(ui, r.shrink(3.0));
            b.on_hover_text(tip).clicked()
        };
    let eye = if n.visible { Icon::Eye } else { Icon::EyeSlash };
    if trailing(
        ui,
        &|ui, r| theme::paint_icon(ui.painter(), r, eye, fg2),
        if n.visible { "Hide" } else { "Show" },
        hovered || !n.visible,
    ) {
        ed.edit(if n.visible { "Hide" } else { "Show" }, |tx| {
            tx.node_mut(id).unwrap().visible = !n.visible
        });
    }
    if trailing(
        ui,
        &|ui, r| lottie_icon::toggle(ui, ("lock-icon", id), "lock", r, fg2, n.locked),
        if n.locked { "Unlock" } else { "Lock" },
        hovered || n.locked,
    ) {
        ed.edit(if n.locked { "Unlock" } else { "Lock" }, |tx| {
            tx.node_mut(id).unwrap().locked = !n.locked
        });
    }

    // Name, or an inline rename field.
    let name_rect =
        egui::Rect::from_min_max(egui::pos2(x, row.top()), egui::pos2(right, row.bottom()));
    if let Some((rid, name)) = &mut st.renaming
        && *rid == id
    {
        let r = theme::rename_field(
            ui,
            name_rect.shrink2(egui::vec2(0.0, 2.0)),
            name,
            ("layer", format!("{id:?}")),
        );
        if r.lost_focus() {
            let new = name.trim().to_string();
            st.renaming = None;
            if !new.is_empty() && new != n.name {
                ed.edit("Rename", |tx| tx.node_mut(id).unwrap().name = new);
            }
        }
    } else {
        let font = if n.is_frame() {
            theme::medium(theme::BODY)
        } else {
            egui::FontId::proportional(theme::BODY)
        };
        let galley = ui.painter().layout(n.name.clone(), font, fg, f32::INFINITY);
        let clip = ui
            .painter()
            .with_clip_rect(name_rect.intersect(ui.clip_rect()));
        clip.galley(egui::pos2(x, cy - galley.size().y / 2.0), galley, fg);
    }

    if resp.double_clicked() {
        st.renaming = Some((id, n.name.clone()));
    } else if resp.clicked() {
        ed.end_text_edit();
        if ui.input(|i| i.modifiers.shift || i.modifiers.command) {
            if selected {
                ed.selection.retain(|s| *s != id);
            } else {
                ed.selection.push(id);
            }
        } else {
            ed.selection = vec![id];
        }
    }
    if hovered {
        ed.hover = Some(id);
    }
    if !needle.is_empty() || !st.collapsed.contains(&id) {
        for c in n.children.iter().rev() {
            let index = st.layer_rows.len();
            motion::pop_in(ui, ("layer-pop", *c), index, |ui| {
                layer_row(ui, ed, st, *c, depth + 1)
            });
        }
    }
}

fn tool_icon(t: Tool) -> Icon {
    match t {
        Tool::Select => Icon::Pointer,
        Tool::Hand => Icon::Hand,
        Tool::Frame => Icon::Frame,
        Tool::Rect => Icon::Rect,
        Tool::Ellipse => Icon::Ellipse,
        Tool::Polygon => Icon::Polygon,
        Tool::Diamond => Icon::Diamond,
        Tool::Arrow => Icon::Arrow,
        Tool::Sticky => Icon::Sticky,
        Tool::Laser => Icon::Laser,
        Tool::Star => Icon::Star,
        Tool::Pen => Icon::Pen,
        Tool::Line => Icon::Line,
        Tool::Text => Icon::Text,
        Tool::Pencil => Icon::Pencil,
        Tool::Eraser => Icon::Eraser,
    }
}

/// Floating tool palette at the bottom of the canvas, styled as a macOS popover.
fn toolbar(ctx: &egui::Context, ed: &mut Editor, canvas: egui::Rect) {
    egui::Area::new("toolbar".into())
        .constrain_to(canvas)
        .pivot(Align2::CENTER_BOTTOM)
        .fixed_pos(egui::pos2(canvas.center().x, canvas.bottom() - 16.0))
        .show(ctx, |ui| {
            let p = theme::pal(ui);
            egui::Frame::popup(ui.style())
                .fill(p.popover)
                .corner_radius(theme::RADIUS_PANEL)
                .inner_margin(4.0)
                .stroke(egui::Stroke::NONE)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        for t in Tool::ALL {
                            if matches!(t, Tool::Frame | Tool::Line | Tool::Text | Tool::Pencil) {
                                ui.add_space(6.0);
                            }
                            if theme::icon_button(ui, tool_icon(t), 34.0, ed.tool == t)
                                .on_hover_text(format!("{} ({})", t.label(), t.shortcut()))
                                .clicked()
                            {
                                ed.run(Action::Tool(t));
                            }
                        }
                    });
                });
        });
}

/// Colour of an error toast's background.
const TOAST_ERROR: Color32 = Color32::from_rgb(0xb4, 0x2b, 0x2b);

fn toast(ctx: &egui::Context, text: &str, error: bool) {
    egui::Area::new("toast".into())
        .order(egui::Order::Tooltip)
        .anchor(Align2::CENTER_TOP, egui::vec2(0.0, 44.0))
        .interactable(false)
        .show(ctx, |ui| {
            ui.multiply_opacity(motion::appear(
                ctx,
                "toast-appear",
                motion::MEDIUM,
                motion::ease_out_cubic,
            ));
            let p = theme::pal(ui);
            let (fill, ink) = if error {
                (TOAST_ERROR, Color32::WHITE)
            } else {
                (p.popover, p.secondary)
            };
            egui::Frame::popup(ui.style())
                .fill(fill)
                .corner_radius(16.0)
                .inner_margin(egui::Margin::symmetric(16, 10))
                .show(ui, |ui| {
                    ui.set_max_width(520.0);
                    ui.label(RichText::new(text).color(ink));
                });
        });
}

fn status(ctx: &egui::Context, ed: &Editor, canvas: egui::Rect) {
    if ed.status.is_empty() {
        return;
    }
    egui::Area::new("status".into())
        .pivot(Align2::LEFT_BOTTOM)
        .fixed_pos(egui::pos2(canvas.left() + 12.0, canvas.bottom() - 12.0))
        .interactable(false)
        .show(ctx, |ui| {
            let p = theme::pal(ui);
            egui::Frame::new()
                .fill(p.sidebar)
                .corner_radius(egui::CornerRadius::same(255))
                .inner_margin(egui::Margin::symmetric(14, 6))
                .show(ui, |ui| {
                    ui.label(
                        RichText::new(&ed.status)
                            .color(p.secondary)
                            .size(theme::SECONDARY),
                    );
                });
        });
}

fn help(ctx: &egui::Context, st: &mut UiState, canvas: egui::Rect) {
    let width = 820.0f32.min(canvas.width() - 40.0);
    egui::Area::new("help".into())
        .order(egui::Order::Foreground)
        .fade_in(false)
        .pivot(Align2::CENTER_CENTER)
        .fixed_pos(canvas.center())
        .show(ctx, |ui| {
            let p = theme::pal(ui);
            egui::Frame::popup(ui.style())
                .fill(p.sidebar)
                .corner_radius(24.0)
                .inner_margin(24.0)
                .stroke(egui::Stroke::NONE)
                .show(ui, |ui| {
                    ui.set_width(width);
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 2.0;
                            ui.label(RichText::new("Keyboard shortcuts").font(theme::semibold(20.0)));
                            ui.label(RichText::new("Press ? anytime to open this sheet.").color(p.secondary));
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if kit::button(ui, kit::ButtonKind::Secondary, None, "Done")
                                .on_hover_text("Close (Esc)")
                                .clicked()
                            {
                                st.show_help = false;
                            }
                        });
                    });
                    ui.add_space(12.0);
                    let sections = shortcuts::help_sections();
                    egui::ScrollArea::vertical()
                        .max_height((canvas.height() - 160.0).max(200.0))
                        .show(ui, |ui| {
                            ui.columns(3, |cols| {
                                // Balance columns by row count.
                                let total: usize = sections.iter().map(|(_, r)| r.len() + 2).sum();
                                let mut acc = 0;
                                for (title, rows) in sections.iter() {
                                    let col = (acc * 3 / total.max(1)).min(2);
                                    acc += rows.len() + 2;
                                    let ui = &mut cols[col];
                                    kit::section_header(ui, title, None);
                                    ui.add_space(4.0);
                                    for (k, d) in rows {
                                        ui.horizontal(|ui| {
                                            ui.set_height(30.0);
                                            ui.label(RichText::new(*d).size(theme::BODY));
                                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                                ui.spacing_mut().item_spacing.x = 4.0;
                                                kit::key_caps_rtl(ui, k);
                                            });
                                        });
                                    }
                                    ui.add_space(14.0);
                                }
                            });
                        });
                    ui.label(
                        RichText::new("Space-drag pans · wheel pans · ⌘/Ctrl+wheel or pinch zooms · ⌥-drag duplicates · hold ⌥ to measure")
                            .size(theme::SECONDARY)
                            .color(p.secondary),
                    );
                });
        });
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        st.show_help = false;
    }
}
