//! egui chrome: menu bar, layers panel, design (properties) panel, floating toolbar, and the
//! projects home screen.

pub mod actions;
pub mod agent;
pub mod annotations;
pub mod annotations_canvas;
pub mod bind;
pub mod chat;
pub mod chat_md;
pub mod chat_msgs;
pub mod confirm;
pub mod design;
pub mod home;
pub mod inspector;
pub mod props_modal;
pub mod kit;
pub mod library;
pub mod lottie_icon;
pub mod crisp;
pub mod menu_toggle;
pub mod menus;
pub mod panel_handle;
pub mod motion;
pub mod pages;
pub mod perf;
pub mod palette;
pub mod panels;
pub mod libraries;
pub mod settings;
pub mod variables;
#[cfg(not(target_arch = "wasm32"))]
pub mod update;

use crate::theme::Surface as _;
use crate::editor::shortcuts::{self, Action};
use crate::editor::{Editor, Tool};
use crate::model::{Color, NodeId, NodeKind};
use crate::theme::{self, Icon};
use egui::{Align2, Color32, RichText, Ui};
use std::collections::{HashMap, HashSet};

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
    /// The File / Edit / ... strip is folded away (the menu button in the toolbar toggles it).
    pub menu_bar_hidden: bool,
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
    pub variables: variables::VariablesUi,
    pub libraries: libraries::LibrariesUi,
    /// All panels hidden (⌘\\): just the canvas.
    pub hide_ui: bool,
    /// The side panels are tucked away (`/`); the toolbar and canvas stay.
    pub panels_hidden: bool,
    /// Dragged widths of the left and right panels; `None` keeps the default.
    pub panel_w: [Option<f32>; 2],
    /// Narrowest width each side panel's content needs, learned from how far it overflowed.
    pub panel_need: [f32; 2],
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
    /// Scroll offset of the layers list at the last frame, and whether rows may pop in now.
    pub layer_offset: f32,
    pub layer_animate: bool,
    /// `layer_subtree_height` of the rows, so each subtree is measured once; kept while the
    /// document and the folded rows stay the same (`layer_heights_key`).
    pub layer_heights: HashMap<NodeId, f32>,
    pub layer_heights_key: (u64, usize),
    /// Selection seen by the last layers-panel frame, to notice changes made on the canvas.
    pub last_selection: Vec<NodeId>,
    /// A layer the layers panel should scroll into view (and unfold its parents for).
    pub reveal: Option<NodeId>,
    /// Text typed into the inspector's feedback box.
    pub feedback_draft: String,
    /// Text in the layers panel's search box; only matching layers (and their parents) show.
    pub layer_filter: String,
    /// The Libraries modal is open (Assets header button).
    pub libraries_open: bool,
    /// The last tool an agent ran and when, for the toolbar's activity hint.
    pub agent: Option<(String, web_time::Instant)>,
    /// When the pill stops counting the agent as working: the latest call's window, never
    /// shortened by a later, shorter one.
    pub agent_until: Option<web_time::Instant>,
    /// The last read-only call that reached the canvas, to tell a lone lookup from a sweep.
    pub last_read: Option<web_time::Instant>,
    /// Recent agent tool calls, oldest first (the Agent view).
    pub agent_log: Vec<AgentCall>,
    /// Layers agents touched in the last [`AGENT_MARK_SECS`], oldest first: the live outlines and
    /// the agent cursor.
    pub agent_marks: Vec<AgentMark>,
    /// Saved agent runs of the open project, newest first; re-read when `runs_key` stops matching
    /// the project folder (run finished, project changed).
    pub runs: Vec<crate::runs::Run>,
    pub runs_key: Option<std::path::PathBuf>,
    /// The runs being recorded now (one per agent client), copied from the tracker each frame.
    pub run_open: Vec<crate::runs::Run>,
    /// (active page, history generation) when the review runs were last recounted.
    pub runs_gen: Option<(usize, u64)>,
    /// A recount running on another thread: (run id, counts) per run when done.
    pub recount: Option<std::sync::mpsc::Receiver<Vec<(String, crate::runs::Counts)>>>,
    /// The run that just ended with changes: the Agent view's "Agent finished" banner.
    pub run_banner: Option<crate::runs::Run>,
    /// Canvas toast for the same run: (run id, text, shown at).
    pub run_toast: Option<(String, String, web_time::Instant)>,
    /// An Actions-menu action and the layers it runs on, for the host to run as one run.
    pub pending_action: Option<(actions::Act, Vec<String>)>,
    /// Run (id) the user chose to review; the review view draws it.
    pub review_run: Option<String>,
    /// Diff of the open review, cached until the document changes; the canvas overlay reads it.
    pub review_diff: Option<panels::ReviewDiff>,
    /// Run (id) waiting for the "Revert agent run?" confirm.
    pub pending_run_revert: Option<String>,
    /// Run (id) waiting for the "Restore to here" confirm: it and every later run get reverted.
    pub pending_restore: Option<String>,
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
    /// Chat mode: the chat column replaces the layers and inspector.
    pub chat_mode: bool,
    pub chat: chat::ChatState,
    /// Annotation view state (mode, hidden categories, collapsed notes); not saved.
    pub annotations: annotations_canvas::AnnotationsView,
    /// The "Edit properties" modal of a main component.
    pub props_modal: props_modal::PropsModal,
    /// Annotation picked on the canvas, for the side view and inspector.
    pub selected_annotation: Option<u64>,
    /// Annotations UI state (popover, Customize modal, inline edit), not saved.
    pub ann_ui: annotations::AnnotationsUi,
    /// Frame times, RAM, CPU and GPU for Settings > Performance and its overlay.
    pub perf: perf::Perf,
    /// The Design tab's lint and feedback, kept until the document changes.
    pub design_cache: design::Cache,
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
            // A child column starts at the top of the row while the avatar is centred in it;
            // pad the column so its two lines centre on the avatar.
            let block = ui.fonts_mut(|f| {
                f.row_height(&theme::semibold(theme::BODY))
                    + f.row_height(&egui::FontId::proportional(theme::SECONDARY))
            });
            ui.add_space((r.center().y - block / 2.0 - ui.cursor().top()).max(0.0));
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

/// Review runs recounted after an edit (newest first).
const RECOUNT_MAX: usize = 3;
/// Most calls the Agent view keeps.
const AGENT_LOG_MAX: usize = 60;

/// One tool call an agent made.
pub struct AgentCall {
    pub name: String,
    pub at: web_time::Instant,
    /// The error text when the call failed.
    pub error: Option<String>,
}

/// A layer an agent just touched, outlined on the canvas while it fades.
pub struct AgentMark {
    pub page_id: String,
    pub node: NodeId,
    pub verb: &'static str,
    /// The run's client label ("Claude Code · 2", "Chat"); empty for unnamed clients.
    pub client: String,
    pub at: web_time::Instant,
}

/// How long an agent's outline stays on the canvas, fading out.
const AGENT_MARK_SECS: f32 = 3.0;
/// Outlines kept from one call (its topmost changed layers) and in all.
const AGENT_MARKS_PER_CALL: usize = 12;
const AGENT_MARKS_MAX: usize = 48;
/// Reads closer together than this are one sweep and draw no outline after the first.
const AGENT_READ_GAP_SECS: f32 = 2.0;

impl UiState {
    /// Records the layers a successful agent call pointed at (`follow_node`) or changed
    /// (`touched`, the tracker's per-call log) for the live outlines.
    pub fn mark_agent(
        &mut self,
        ed: &Editor,
        client: String,
        name: &str,
        args: &serde_json::Value,
        touched: &[(String, NodeId)],
    ) {
        let key = chat_msgs::follow_node(args).and_then(|k| {
            let (prefix, num) = k.trim_start_matches('#').split_once(':')?;
            let page = crate::mcp::exec::find_page(ed, prefix).ok()?;
            Some((ed.pages[page].id.clone(), NodeId(num.parse().ok()?)))
        });
        // Topmost changed layers only: a write_html of 500 nodes is one outline, not 500.
        let roots = touched.iter().filter(|(page, id)| {
            let Some(i) = ed.pages.iter().position(|p| &p.id == page) else { return false };
            let parent = ed.page_doc(i).get(*id).and_then(|n| n.parent);
            parent.is_none_or(|par| !touched.iter().any(|(pg, t)| pg == page && *t == par))
        });
        let at = web_time::Instant::now();
        // A sweep of reads (get_node_info over every layer) must not flash an outline on each:
        // only a lookup with no other read just before it shows one.
        if !crate::mcp::tools::edits_doc(name, args) {
            let sweeping = self.last_read.is_some_and(|t| t.elapsed().as_secs_f32() < AGENT_READ_GAP_SECS);
            self.last_read = Some(at);
            if sweeping {
                return;
            }
        }
        let verb = agent_verb(name);
        for (page_id, node) in key.into_iter().chain(roots.cloned()).take(AGENT_MARKS_PER_CALL) {
            self.agent_marks.retain(|m| (&m.page_id, m.node) != (&page_id, node));
            self.agent_marks.push(AgentMark { page_id, node, verb, client: client.clone(), at });
        }
        let over = self.agent_marks.len().saturating_sub(AGENT_MARKS_MAX);
        self.agent_marks.drain(..over);
        // Any agent, not only the chat: the canvas follows the layer it just touched.
        if let Some(m) = self.agent_marks.last().filter(|m| m.at == at) {
            self.chat.follow = Some(format!("{}:{}", m.page_id, m.node.0));
        }
    }

    /// Drops faded marks; true while any is still showing.
    fn live_marks(&mut self) -> bool {
        self.agent_marks.retain(|m| m.at.elapsed().as_secs_f32() < AGENT_MARK_SECS);
        !self.agent_marks.is_empty()
    }

    /// Per frame, after `Tracker::tick`: mirrors the open runs and picks up a finished one.
    /// Returns true when a run just ended with changes.
    pub fn sync_runs(&mut self, ctx: &egui::Context, t: &mut crate::runs::Tracker, ed: &Editor) -> bool {
        self.run_open = t.open_runs().cloned().collect();
        let finished = t.take_finished();
        if let Some(run) = &finished {
            self.chat.note_run(run);
        }
        if finished.is_some() {
            self.runs_key = None;
        }
        if let Some(dir) = self.project_dir.clone()
            && self.runs_key.as_ref() != Some(&dir)
        {
            self.runs = crate::runs::list(&dir);
            self.runs_key = Some(dir);
        }
        self.recount_runs(ctx, ed);
        let Some(run) = finished else {
            return false;
        };
        let changed = run.added + run.changed + run.removed > 0;
        if changed {
            let text = if run.client == crate::mcp::ACTIONS_CLIENT {
                run.title.clone()
            } else {
                format!("Agent finished \u{b7} {}", panels::count(run.added + run.changed + run.removed, "change"))
            };
            self.run_toast = Some((run.id.clone(), text, web_time::Instant::now()));
            self.run_banner = Some(run);
        }
        changed
    }

    /// After undo, redo or an edit on the page, the review runs on it get their counts again, so
    /// their cards do not keep showing changes that are gone. Saved, so a reload agrees. The
    /// count reads each run's snapshot and diffs whole pages (~100 ms each on a large page), so it
    /// runs on another thread against copies of the pages, one recount at a time.
    fn recount_runs(&mut self, ctx: &egui::Context, ed: &Editor) {
        if let Some(rx) = &self.recount {
            match rx.try_recv() {
                Ok(counts) => {
                    self.recount = None;
                    self.apply_counts(counts);
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => return,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => self.recount = None,
            }
        }
        let key = (ed.active_page, ed.history.generation());
        if !self.run_open.is_empty() || self.runs_gen == Some(key) {
            return;
        }
        self.runs_gen = Some(key);
        let Some(dir) = self.project_dir.clone() else { return };
        let page = ed.active_page_id();
        // ponytail: newest few only, each recount reads its snapshots from disk.
        let review: Vec<crate::runs::Run> = self
            .runs
            .iter()
            .filter(|r| r.state == crate::runs::RunState::Review && r.pages.iter().any(|p| p.page_id == page))
            .take(RECOUNT_MAX)
            .cloned()
            .collect();
        if review.is_empty() {
            return;
        }
        // ponytail: copies the pages the runs touched; shared documents would avoid the copy.
        let pages: Vec<(String, String, crate::model::Document)> = ed
            .pages
            .iter()
            .enumerate()
            .filter(|(_, s)| review.iter().any(|r| r.pages.iter().any(|p| p.page_id == s.id)))
            .map(|(i, s)| (s.id.clone(), s.name.clone(), ed.page_doc(i).clone()))
            .collect();
        let work = move || -> Vec<(String, crate::runs::Counts)> {
            review
                .iter()
                .map(|r| {
                    let c = crate::runs::count_in(&dir, r, |id| {
                        pages.iter().find(|p| p.0 == id).map(|p| (&p.2, p.1.as_str()))
                    });
                    (r.id.clone(), c)
                })
                .collect()
        };
        #[cfg(target_arch = "wasm32")]
        {
            let _ = ctx;
            self.apply_counts(work());
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let (tx, rx) = std::sync::mpsc::channel();
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                let _ = tx.send(work());
                ctx.request_repaint();
            });
            self.recount = Some(rx);
        }
    }

    fn apply_counts(&mut self, counts: Vec<(String, crate::runs::Counts)>) {
        let Some(dir) = &self.project_dir else { return };
        for (id, (added, changed, removed, _)) in counts {
            let Some(r) = self.runs.iter_mut().find(|r| r.id == id) else { continue };
            if (added, changed, removed) != (r.added, r.changed, r.removed) {
                (r.added, r.changed, r.removed) = (added, changed, removed);
                let _ = crate::runs::save(dir, r);
            }
        }
    }

    /// Records an agent's tool call for the toolbar hint and the Agent view.
    pub fn log_agent(&mut self, name: &str, args: &serde_json::Value, error: Option<String>) {
        let at = web_time::Instant::now();
        self.agent = Some((name.to_owned(), at));
        let window = if crate::mcp::tools::edits_doc(name, args) { AGENT_WORKING_SECS as f32 } else { AGENT_READ_WORKING_SECS };
        let until = at + std::time::Duration::from_secs_f32(window);
        self.agent_until = Some(self.agent_until.map_or(until, |u| u.max(until)));
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
/// The same for a read-only call: a sweep of lookups lights the pill only while it runs.
const AGENT_READ_WORKING_SECS: f32 = 1.5;
/// Repaint interval of the activity pill's dots and the canvas glow while an agent works. Each
/// tick redraws the whole window, so 15 FPS: the pulse still reads smooth.
const AGENT_PILL_FRAME_MS: u64 = 66;

/// What an agent's tool call means in plain words, for the activity pill.
pub(crate) fn agent_verb(tool: &str) -> &'static str {
    match tool {
        "write_html" | "create_artboard" | "create_shape" => "Drawing layout",
        "update_styles" | "set_text_content" | "rename_nodes" => "Styling",
        "get_screenshot" | "get_jsx" | "get_design_spec" | "get_computed_styles" | "get_node_info"
        | "get_children" | "get_tree_summary" | "get_bounds" | "get_selection" | "get_basic_info"
        | "get_inventory" | "get_variables" | "get_components" | "find_nodes" | "get_annotations"
        | "get_annotation_template" | "get_font_family_info" | "get_fill_image" | "search_icons"
        | "list_resources" | "list_libraries" => "Looking at the design",
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
    /// What the agent is doing right now, while its latest call's window
    /// ([`AGENT_WORKING_SECS`] for an edit, [`AGENT_READ_WORKING_SECS`] for a read) is open.
    pub fn agent_working(&self) -> Option<&'static str> {
        let open = self.agent_until.is_some_and(|u| web_time::Instant::now() < u);
        self.agent.as_ref().filter(|_| open).map(|(tool, _)| agent_verb(tool))
    }
}

/// A pill with three bouncing dots and what the agent is doing; nothing when it is idle.
fn agent_pill(ui: &mut Ui, st: &UiState, with_text: bool, keep_free: f32) {
    let Some(verb) = st.agent_working() else {
        return;
    };
    // The dots bounce at 30 fps: a full-canvas frame per vsync for this alone burns CPU.
    ui.ctx().request_repaint_after(std::time::Duration::from_millis(AGENT_PILL_FRAME_MS));
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
                let text = match run_progress(&st.run_open) {
                    Some(n) => format!("{verb} \u{b7} {n}"),
                    None => format!("{verb}…"),
                };
                let text = RichText::new(text).size(theme::SECONDARY).color(p.text);
                // Only the dots when the text would run under the buttons on the right.
                let w = egui::WidgetText::from(text.clone()).into_galley(ui, Some(egui::TextWrapMode::Extend), f32::INFINITY, egui::TextStyle::Body).size().x;
                if with_text && ui.available_width() - keep_free > w + AGENT_PILL_PAD {
                    ui.label(text);
                }
            });
        });
}

/// `1340` as `1,340`.
pub(crate) fn grouped(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// Live size of the open runs, all clients together: "212 calls · 1,340 changes". None when no
/// run is open.
pub(crate) fn run_progress(open: &[crate::runs::Run]) -> Option<String> {
    if open.is_empty() {
        return None;
    }
    let calls: usize = open.iter().map(|r| r.calls as usize).sum();
    let changes: usize = open.iter().map(crate::runs::Run::touched).sum();
    let unit = |n: usize, u: &str| format!("{} {u}{}", grouped(n), if n == 1 { "" } else { "s" });
    Some(format!("{} \u{b7} {}", unit(calls, "call"), unit(changes, "change")))
}

/// A soft accent ring around the canvas while an agent works.
fn agent_glow(ctx: &egui::Context, st: &UiState, canvas: egui::Rect) {
    if st.agent_working().is_none() {
        return;
    }
    let t = ctx.input(|i| i.time) as f32;
    let pulse = 0.35 + 0.25 * (t * 3.0).sin();
    ctx.layer_painter(egui::LayerId::new(
        egui::Order::Middle,
        "agent-glow".into(),
    ))
    .rect_stroke(
        canvas.shrink(1.5),
        f32::from(theme::radius_panel()),
        egui::Stroke::new(2.0, theme::accent().gamma_multiply(pulse)),
        egui::StrokeKind::Inside,
    );
}

/// Room the title leaves beside the top bar's right-hand buttons: the agent pill's dots and gaps.
const TITLE_ROOM: f32 = 72.0;

/// The agent pill's own padding and gap, kept clear beside its text.
const AGENT_PILL_PAD: f32 = 32.0;

/// Gap between the floating panels and around the canvas.
pub(crate) const PANEL_GAP: f32 = 12.0;

/// Canvas width under which the top bar drops the zoom pill (it is also on the keyboard).
const TIGHT_BAR_W: f32 = 520.0;
/// Room the hamburger, home and mode buttons take on top of the bar's old budget.
const TIGHT_EXTRA: f32 = 80.0;
/// Tallest the top bar's page menu grows before it scrolls.
const PAGE_MENU_MAX_H: f32 = 360.0;

/// Canvas width under which the egui menu bar folds into a single "Menu" button.
const MENU_BAR_W: f32 = 1100.0;

/// Largest share of the window one side panel may take.
const SIDE_MAX_SHARE: f32 = 0.3;
/// The canvas, with the toolbar above it, needs at least this much room; below it the side
/// panels step aside.
const MIN_CANVAS_W: f32 = 600.0;

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
        .fixed_pos(egui::pos2(canvas.center().x, canvas.bottom() - RUN_TOAST_LIFT))
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(p.sidebar)
                .corner_radius(egui::CornerRadius::same(255))
                .inner_margin(egui::Margin::symmetric(8, 6))
                .show_surface(ui, |ui| {
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
                .show_surface(ui, |ui| {
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
/// The panel width after a grip: `None` (the default) on a double click, else `current` moved by
/// the drag. Unmoved panels stay `None` so they keep following the default.
/// How much wider than the `room` it was given a panel's content came out this frame. (Widgets
/// that overflow grow `max_rect` too, so it is the width that has to be compared.)
fn overflow(ui: &Ui, room: f32) -> f32 {
    (ui.min_rect().width() - room).max(0.0)
}

/// The width a panel has to be to hold its content: what it was plus what overflowed. Kept once
/// learned, so the panel does not shrink back and overflow again.
fn grown_need(need: f32, width: f32, overflow: f32) -> f32 {
    if overflow > 0.5 { need.max(width + overflow + PANEL_BREATH) } else { need }
}

/// Spare room added to a learned panel width, so the content does not end on the card's edge.
const PANEL_BREATH: f32 = 8.0;

#[cfg(test)]
mod panel_need_tests {
    use super::*;

    #[test]
    fn a_panel_learns_the_width_its_content_needs_and_keeps_it() {
        let need = grown_need(0.0, 296.0, 7.0);
        assert_eq!(need, 296.0 + 7.0 + PANEL_BREATH);
        assert_eq!(grown_need(need, 320.0, 0.0), need, "no overflow: unchanged");
        assert_eq!(grown_need(need, 296.0, 3.0), need, "never shrinks");
    }
}

fn grip_width(saved: Option<f32>, grip: panel_handle::Grip, current: f32, range: std::ops::RangeInclusive<f32>) -> Option<f32> {
    if grip.reset {
        return None;
    }
    if grip.delta == 0.0 {
        return saved;
    }
    Some(panel_handle::resized(current, grip.delta, range))
}

/// Returns how far the content ran past the card's right edge (0 when it fits).
fn floating_panel(ui: &mut Ui, fill: Color32, margin: egui::Margin, add: impl FnOnce(&mut Ui)) -> f32 {
    let fill = if theme::is_acrylic() {
        let dark = ui.visuals().dark_mode;
        let rect = ui.available_rect_before_wrap();
        theme::paint_acrylic_panel(ui.painter(), rect, theme::radius_panel(), fill, dark);
        Color32::TRANSPARENT
    } else {
        fill
    };
    egui::Frame::new()
        .fill(fill)
        .corner_radius(theme::radius_panel())
        .inner_margin(margin)
        .show(ui, |ui| {
            ui.set_min_height(ui.available_height());
            ui.set_width(ui.available_width());
            let room = ui.available_width();
            add(ui);
            overflow(ui, room)
        })
        .inner
}

/// The acrylic backdrop around the canvas (which the renderer draws underneath the UI, so it
/// must stay unpainted).
fn acrylic_backdrop(p: theme::Palette, screen: egui::Rect, canvas: egui::Rect) -> egui::Shape {
    let bands = [
        (screen.min, egui::pos2(screen.right(), canvas.top())),
        (egui::pos2(screen.left(), canvas.bottom()), screen.max),
        (
            egui::pos2(screen.left(), canvas.top()),
            canvas.left_bottom(),
        ),
        (
            canvas.right_top(),
            egui::pos2(screen.right(), canvas.bottom()),
        ),
    ];
    egui::Shape::Vec(
        bands
            .into_iter()
            .map(|(min, max)| egui::Rect::from_min_max(min, max))
            .filter(|r| r.is_positive())
            .map(|r| theme::backdrop_mesh(p, screen, r).into())
            .collect(),
    )
}

/// Paints the four corners of `rect` outside a rounded rectangle of radius `r`, so the canvas
/// (drawn by the renderer underneath the UI) shows with rounded corners.
fn round_corners(
    painter: &egui::Painter,
    rect: egui::Rect,
    r: f32,
    colour_at: impl Fn(egui::Pos2) -> Color32,
) {
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
        let colour = colour_at(corner);
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
impl UiState {
    /// Switch Design | Chat and remember it for the open project. Chat needs the desktop app
    /// and an open project.
    pub fn set_chat_mode(&mut self, on: bool) {
        let on = on && !cfg!(target_arch = "wasm32") && self.project_dir.is_some();
        self.chat_mode = on;
        if let Some(dir) = &self.project_dir {
            chat::remember_mode(dir, on);
        }
    }
}

pub fn draw(ui: &mut Ui, ed: &mut Editor, st: &mut UiState) -> egui::Rect {
    if st.chat.project != st.project_dir {
        st.chat_mode = st.chat.switch_project(st.project_dir.clone());
    }
    if st.chat.mcp_url != st.agent_endpoint {
        st.chat.mcp_url.clone_from(&st.agent_endpoint);
    }
    st.chat.project_name.clone_from(&st.project_name);
    let turn = st.chat.turn.is_some();
    st.chat.pump(ui.ctx());
    if turn && st.chat.turn.is_none() {
        // The turn is over: the "Working" pill goes with it, not 6 s after its last tool call.
        st.agent = None;
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
        if st.custom_titlebar {
            home_window_chrome(ui, st);
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
            UiEvent::TogglePanels => st.panels_hidden = !st.panels_hidden,
            UiEvent::ToggleChat => st.set_chat_mode(!st.chat_mode),
            UiEvent::ToggleHelp => st.show_help = !st.show_help,
            UiEvent::ToggleGrid => st.show_grid_off = !st.show_grid_off,
            UiEvent::ToggleAnnotations => {
                let d = ed.active_template(ed.active_page).display;
                st.annotations.cycle(d);
            }
            UiEvent::OpenVariables => st.variables.open = !st.variables.open,
            UiEvent::Annotate(id) => annotations::open_popover(ed, &mut st.ann_ui, id),
            UiEvent::Ask => chat::open_ask(ed, st),
        }
    }
    if !st.run_open.is_empty() {
        // So the tracker's idle timeout fires even when nothing else repaints.
        ui.ctx().request_repaint_after(std::time::Duration::from_secs(1));
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
    // The acrylic backdrop goes under the panels, but needs the canvas rect from after them.
    let backdrop_slot = ui.painter().add(egui::Shape::Noop);
    let gap = PANEL_GAP;
    // Empty margin between a floating panel's card and the canvas; none when they are flush.
    let grip_gap = if floating { gap } else { 0.0 };
    let side_frame = |left: i8, right: i8, top: i8, bottom: i8| {
        if floating {
            egui::Frame::NONE
                .fill(theme::backdrop_fill(p))
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
    if st.panels_hidden {
        // Only the toolbar and the canvas.
    } else if st.chat_mode {
        chat::panel(
            ui,
            ed,
            st,
            if floating {
                side_frame(gap as i8, gap as i8, gap as i8, gap as i8)
            } else {
                egui::Frame::NONE
                    .fill(sidebar_fill)
                    .inner_margin(egui::Margin::symmetric(10, 8))
            },
            floating,
            sidebar_fill,
        );
    } else {
        let left_range = 190.0f32.max(st.panel_need[0])..=side_max(190.0).max(st.panel_need[0]);
        let left_w = panel_handle::resized(st.panel_w[0].unwrap_or(252.0 + 2.0 * gap), 0.0, left_range.clone());
        let right_range = 230.0f32.max(st.panel_need[1])..=side_max(230.0).max(st.panel_need[1]);
        let right_w = panel_handle::resized(st.panel_w[1].unwrap_or(272.0 + 2.0 * gap), 0.0, right_range.clone());
        // A window too narrow for the canvas and both panels gives up the right one, then the left.
        let show_right = window_w - left_w - right_w >= MIN_CANVAS_W;
        let show_left = show_right || window_w - left_w >= MIN_CANVAS_W;
        if show_left {
        let left = egui::Panel::left("layers")
            .resizable(false)
            .exact_size(left_w)
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
                            kit::brand_mark(ui, 28.0);
                            ui.label(RichText::new("PaperCraft").font(theme::semibold(16.0)));
                        });
                        ui.add_space(10.0);
                    }
                    layers(ui, ed, st)
                };
                if floating {
                    floating_panel(ui, sidebar_fill, egui::Margin::symmetric(14, 12), body)
                } else {
                    let room = ui.available_width();
                    body(ui);
                    overflow(ui, room)
                }
            });
        st.panel_need[0] = grown_need(st.panel_need[0], left.response.rect.width(), left.inner);
        let grip = panel_handle::show(ui.ctx(), "layers-grip".into(), left.response.rect, panel_handle::Side::Left, grip_gap);
        st.panel_w[0] = grip_width(st.panel_w[0], grip, left_w, left_range);
        }
        if show_right {
        let right = egui::Panel::right("design")
            .resizable(false)
            .exact_size(right_w)
            .frame(if floating {
                side_frame(gap as i8, gap as i8, gap as i8, gap as i8)
            } else {
                egui::Frame::NONE
                    .fill(p.inspector)
                    .inner_margin(inspector::MARGIN)
            })
            .show(ui, |ui| {
                if floating {
                    floating_panel(ui, p.inspector, inspector::MARGIN, |ui| {
                        inspector::panel(ui, ed, st)
                    })
                } else {
                    let room = ui.available_width();
                    inspector::panel(ui, ed, st);
                    overflow(ui, room)
                }
            });
        st.panel_need[1] = grown_need(st.panel_need[1], right.response.rect.width(), right.inner);
        let grip = panel_handle::show(ui.ctx(), "design-grip".into(), right.response.rect, panel_handle::Side::Right, grip_gap);
        st.panel_w[1] = grip_width(st.panel_w[1], grip, right_w, right_range);
        }
    }
    egui::Panel::top("toolbar")
        .exact_size(if floating {
            theme::TOOLBAR_H + gap * 2.0
        } else {
            theme::TOOLBAR_H
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
            .frame(egui::Frame::NONE.fill(theme::backdrop_fill(p)))
            .show(ui, |_| {});
    }
    let canvas = ui.available_rect_before_wrap();
    if floating {
        let screen = ui.ctx().content_rect();
        if theme::is_acrylic() {
            ui.painter()
                .set(backdrop_slot, acrylic_backdrop(p, screen, canvas));
        }
        round_corners(ui.painter(), canvas, f32::from(theme::radius_panel()), |q| {
            if theme::is_acrylic() {
                theme::backdrop_at(p, screen, q)
            } else {
                p.backdrop
            }
        });
    }
    toolbar(ui.ctx(), ed, canvas);
    annotations::overlays(ui.ctx(), ed, st, canvas);
    chat::chat_pick::overlay(ui.ctx(), ed, st, canvas);
    chat::chat_pick::ask_popover(ui.ctx(), ed, st, canvas);
    if st.chat_mode && st.chat.inspector {
        chat::inspector_overlay(ui.ctx(), ed, st, canvas);
    }
    if st.chat_mode {
        chat::canvas_pill(ui.ctx(), st, canvas);
    }
    agent_glow(ui.ctx(), st, canvas);
    status(ui.ctx(), ed, canvas);
    review_overlay(ui.ctx(), ed, st, canvas);
    chat::follow(ed, &mut st.chat);
    if !st.present {
        screen_scroll_toggles(ui.ctx(), ed, canvas);
    }
    agent_overlay(ui.ctx(), ed, st, canvas);
    annotations_canvas::draw(ui.ctx(), ed, &mut st.annotations, &mut st.selected_annotation, canvas);
    props_modal::show(ui.ctx(), ed, st);
    run_toast(ui.ctx(), st, canvas);
    perf::overlay(ui.ctx(), &mut st.perf, canvas);
    palette::draw(ui.ctx(), ed, st, canvas);
    if st.show_help {
        help(ui.ctx(), st, canvas);
    }
    settings::show(ui.ctx(), st);
    variables::show(ui.ctx(), ed, st);
    libraries::show(ui.ctx(), ed, st);
    #[cfg(not(target_arch = "wasm32"))]
    st.updater.show(ui.ctx());
    confirm::show(ui.ctx(), ed, st);
    canvas
}

fn menu_item(ui: &mut Ui, ed: &mut Editor, label: &str, action: Action) {
    if menus::item(ui, label, shortcuts::chord_text(action), menus::icon_of(action)).clicked() {
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
                ui.menu_button(*label, |ui| {
            crate::theme::popup_backing(ui);
            menu_entries(ui, ed, items)
        });
            }
        }
    }
}

/// The File / Edit strip. `open` is its eased 0..=1 state: while it moves, the strip is laid out
/// at its natural width but only `open` of it is shown (clipped and faded), and the toolbar
/// beside it follows that width, so it slides rather than pops.
fn menu_strip(ui: &mut Ui, ed: &mut Editor, st: &mut UiState, open: f32) {
    if open <= 0.0 {
        return;
    }
    let width_id = egui::Id::new("menu-strip-width");
    let natural: f32 = ui.ctx().data(|d| d.get_temp(width_id)).unwrap_or(MENU_STRIP_GUESS_W);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(natural * open, ui.available_height()), egui::Sense::hover());
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(egui::Layout::left_to_right(egui::Align::Center)));
    child.set_clip_rect(rect.intersect(ui.clip_rect()));
    child.set_opacity(open);
    menu_bar(&mut child, ed, &mut st.agent_link);
    let measured = child.min_rect().width();
    ui.ctx().data_mut(|d| d.insert_temp(width_id, measured));
}

/// The unified toolbar: document title on the left, zoom and help on the right. Without native
/// chrome (Linux) it also carries an egui menu bar.
fn top_bar(ui: &mut Ui, ed: &mut Editor, st: &mut UiState) {
    let p = theme::pal(ui);
    // In a narrow window only the essentials stay: zoom, present and share.
    // The window buttons take 3 x 46 px, so the bar gives up its extras that much sooner.
    let buttons_w = if st.custom_titlebar { 3.0 * WIN_BTN_W } else { 0.0 };
    let compact = ui.max_rect().width() < 720.0 + buttons_w;
    let tight = ui.max_rect().width() < TIGHT_BAR_W + buttons_w + TIGHT_EXTRA;
    if st.custom_titlebar {
        drag_area(ui, ui.max_rect(), st);
    }
    ui.horizontal_centered(|ui| {
        if !st.native_chrome {
            if ui.max_rect().width() < MENU_BAR_W {
                // Not enough room for every menu: the button opens them all in one popup.
                let menu = menu_toggle::menu_button(ui);
                egui::Popup::menu(&menu).gap(theme::POPUP_GAP).show(|ui| {
                    theme::popup_backing(ui);
                    flat_menu(ui, ed)
                });
            } else {
                let open = motion::progress(ui.ctx(), "menu-strip", !st.menu_bar_hidden, MENU_STRIP_SECS, motion::ease_in_out_cubic);
                if menu_toggle::button(ui, open).clicked() {
                    st.menu_bar_hidden = !st.menu_bar_hidden;
                    menu_toggle::save_hidden(st.menu_bar_hidden);
                }
                ui.add_space(4.0);
                menu_strip(ui, ed, st, open);
            }
            ui.add_space(6.0);
            ui.separator();
            ui.add_space(6.0);
        }
        let home = theme::icon_button(ui, theme::Icon::Grid, 26.0, false);
        if home.on_hover_text("All projects").clicked() {
            ed.requests.push(crate::editor::AppRequest::Home);
        }
        ui.add_space(8.0);
        if let Some(on) = chat::mode_switch(ui, st.chat_mode) {
            st.set_chat_mode(on);
        }
        ui.add_space(8.0);
        let title = if st.project_name.is_empty() {
            ed.doc.name.clone()
        } else {
            st.project_name.clone()
        };
        // The buttons on the right, as wide as last frame: the title and the agent pill leave
        // them that room.
        let right_id = egui::Id::new("top-bar-right-width");
        let right_w = ui.ctx().data(|d| d.get_temp::<f32>(right_id)).unwrap_or(0.0);
        ui.horizontal(|ui| {
            // Long titles truncate. Before the first measure, a guess by window width.
            let reserve = if right_w > 0.0 {
                right_w + TITLE_ROOM
            } else if tight {
                150.0
            } else if compact {
                300.0
            } else {
                420.0
            };
            ui.set_max_width((ui.available_width() - reserve).max(40.0));
            let state = if !st.save_state.is_empty() {
                st.save_state.clone()
            } else if ed.is_dirty() {
                "Edited".into()
            } else {
                String::new()
            };
            // Title and page share one row: two rows outgrow the bar and spill onto the canvas.
            // The page name opens a page menu: Chat mode has no Pages panel to switch with.
            let many = ed.pages.len() > 1;
            ui.horizontal(|ui| {
                ui.add(egui::Label::new(title_text(title)).truncate());
                ui.label(RichText::new("/").size(theme::SECONDARY).color(p.secondary));
                let page = ui
                    .scope(|ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        let label = ui.add(
                            egui::Label::new(RichText::new(ed.page_name()).size(theme::SECONDARY).color(p.secondary))
                                .truncate()
                                .sense(egui::Sense::click()),
                        );
                        if many {
                            let (r, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
                            theme::paint_icon(ui.painter(), r, Icon::ChevronDown, p.secondary);
                        }
                        label
                    })
                    .inner;
                if many {
                    let page = page.on_hover_text("Switch page").on_hover_cursor(egui::CursorIcon::PointingHand);
                    egui::Popup::menu(&page).gap(crate::theme::POPUP_GAP).show(|ui| {
                        crate::theme::popup_backing(ui);
                        ui.set_min_width(180.0);
                        egui::ScrollArea::vertical().max_height(PAGE_MENU_MAX_H).show(ui, |ui| {
                            for i in 0..ed.pages.len() {
                                let name = ed.pages[i].name.clone();
                                if ui.selectable_label(i == ed.active_page, name).clicked() {
                                    ed.switch_page(i);
                                    ui.close();
                                }
                            }
                        });
                    });
                }
                if !state.is_empty() {
                    kit::chip(ui, &state, state == "Saved");
                }
            });
        });
        agent_pill(ui, st, !tight, right_w);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            if st.custom_titlebar {
                window_buttons(ui, st, theme::TOOLBAR_H);
                ui.add_space(8.0);
            }
            let share = kit::button_compact(ui, kit::ButtonKind::Primary, Some(Icon::Export), "Share");
            egui::Popup::menu(&share).gap(crate::theme::POPUP_GAP).show(|ui| {
                crate::theme::popup_backing(ui);
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
                if icon_menu_item(ui, Icon::Library, "Libraries…").clicked() {
                    st.libraries.open = true;
                    ui.close();
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
            if theme::icon_button(ui, theme::Icon::Play, 26.0, false).on_hover_text("Present").clicked() {
                start_present(ed, st);
            }
            if st.chat_mode
                && !compact
                && theme::icon_button(ui, theme::Icon::Sidebar, 26.0, st.chat.inspector)
                    .on_hover_text("Inspector")
                    .clicked()
            {
                st.chat.inspector = !st.chat.inspector;
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
                    .inner_margin(egui::Margin::symmetric(4, 2))
                    .show_surface(ui, |ui| {
                        // As tall as the Design/Chat switch.
                        ui.spacing_mut().item_spacing.x = 2.0;
                        ui.spacing_mut().interact_size.y = ZOOM_BTN;
                        ui.horizontal(|ui| {
                            if theme::icon_button(ui, theme::Icon::Minus, ZOOM_BTN, false)
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
                                .min_size(egui::vec2(48.0, ZOOM_BTN)),
                            );
                            if z.on_hover_text("Zoom to fit").clicked() {
                                ed.run(Action::ZoomFit);
                            }
                            if theme::icon_button(ui, theme::Icon::Plus, ZOOM_BTN, false)
                                .on_hover_text("Zoom in")
                                .clicked()
                            {
                                ed.run(Action::ZoomIn);
                            }
                        });
                    });
            }
            let w = ui.min_rect().width();
            ui.ctx().data_mut(|d| d.insert_temp(right_id, w));
        });
    });
}

/// Zoom pill buttons: with the pill's 2px margin, as tall as [`kit::SEGMENTED_H`].
const ZOOM_BTN: f32 = kit::SEGMENTED_H - 4.0;

/// egui menus for platforms without a native menu bar (inline, so the toolbar row keeps
/// the title and zoom controls).
fn menu_bar(ui: &mut Ui, ed: &mut Editor, agent: &mut Option<agent::AgentUi>) {
    flat_menus(ui);
    for (title, entries) in shortcuts::menus() {
        menus::menu_button(ui, title, |ui| {
            crate::theme::popup_backing(ui);
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
            .rect_filled(rect, theme::radius_md(), p.hover.gamma_multiply(hover));
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
    menus::menu_button(ui, "Theme", |ui| {
        crate::theme::popup_backing(ui);
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

const WIN_BTN_W: f32 = 46.0;
/// Stroke of the minimize, maximize and close glyphs: the weight of the toolbar's icons.
const WIN_GLYPH_STROKE: f32 = 1.25;
/// How long the File / Edit strip takes to slide open or shut.
const MENU_STRIP_SECS: f32 = 0.22;
/// The strip's width before it has been laid out once (it is measured every frame after).
const MENU_STRIP_GUESS_W: f32 = 300.0;

/// Without native chrome (Windows, Linux) the toolbar is the title bar: its empty space drags
/// the window and a double click maximizes. Widgets added after this take the clicks.
fn drag_area(ui: &mut Ui, rect: egui::Rect, st: &mut UiState) {
    let drag = ui.interact(rect, ui.id().with("window-drag"), egui::Sense::click_and_drag());
    if drag.double_clicked() {
        st.win_cmd = Some(WinCmd::ToggleMaximize);
    } else if drag.drag_started() {
        st.win_cmd = Some(WinCmd::Drag);
    }
}

/// Minimize, maximize and close (right to left from the cursor), `h` tall. They follow the
/// toolbar's other buttons: the animated hover tint and the pointing hand come from
/// `motion::hover`, and Close lights up in the theme's danger colour on the accent surface.
fn window_buttons(ui: &mut Ui, st: &mut UiState, h: f32) {
    let p = theme::pal(ui);
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        for (cmd, tip) in [(WinCmd::Close, "Close"), (WinCmd::ToggleMaximize, "Maximize"), (WinCmd::Minimize, "Minimize")] {
            let (r, resp) = ui.allocate_exact_size(egui::vec2(WIN_BTN_W, h), egui::Sense::click());
            let hover = motion::hover(ui.ctx(), resp.id.with("win-btn"), &resp);
            let down = resp.is_pointer_button_down_on();
            let ppp = ui.ctx().pixels_per_point();
            let cell = egui::emath::GuiRounding::round_to_pixels(r.shrink2(egui::vec2(2.0, 4.0)), ppp);
            let closing = matches!(cmd, WinCmd::Close);
            let mut ink = p.text;
            if closing && (hover > 0.0 || down) {
                let fill = if down { p.danger.gamma_multiply(0.8) } else { p.danger.gamma_multiply(hover) };
                theme::fill_accent(ui, cell, theme::radius_md(), fill);
                ink = kit::ink_on(p.danger);
            } else if !closing {
                let fill = if down { p.pressed } else { p.hover.gamma_multiply(hover) };
                ui.painter().rect_filled(cell, theme::radius_md(), fill);
            }
            // Whole-pixel strokes on the pixel grid, so the straight glyphs stay sharp.
            let (stroke, px) = crisp::stroke(ppp, WIN_GLYPH_STROKE, ink);
            let c = crisp::snap_pos(r.center(), ppp, 2);
            let k = (5.0 * ppp).round() / ppp;
            match cmd {
                WinCmd::Minimize => {
                    let y = crisp::snap(c.y, ppp, px);
                    ui.painter().hline(c.x - k..=c.x + k, y, stroke);
                }
                WinCmd::ToggleMaximize => {
                    ui.painter().rect_stroke(egui::Rect::from_center_size(c, egui::Vec2::splat(k * 2.0)), 0.0, stroke, egui::StrokeKind::Inside);
                }
                _ => {
                    let (d, e) = (egui::vec2(k, k), egui::vec2(k, -k));
                    ui.painter().line_segment([c - d, c + d], stroke);
                    ui.painter().line_segment([c - e, c + e], stroke);
                }
            }
            if resp.on_hover_text(tip).clicked() {
                st.win_cmd = Some(cmd);
            }
        }
    });
}

/// The home screen has no toolbar: its top strip drags the window, the buttons sit top right.
fn home_window_chrome(ui: &mut Ui, st: &mut UiState) {
    let screen = ui.ctx().content_rect();
    let strip = egui::Rect::from_min_size(screen.min, egui::vec2(screen.width(), theme::TOOLBAR_H));
    egui::Area::new("home-window-drag".into()).order(egui::Order::Background).fixed_pos(strip.min).show(ui.ctx(), |ui| {
        drag_area(ui, strip, st);
    });
    egui::Area::new("home-window-buttons".into())
        .order(egui::Order::Foreground)
        .pivot(egui::Align2::RIGHT_TOP)
        .fixed_pos(screen.right_top())
        .show(ui.ctx(), |ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| window_buttons(ui, st, theme::TOOLBAR_H));
        });
}

/// The sidebar views as a strip that fills the sidebar: layers, library, design (variables, modes,
/// components), history, comments and the agent log. Each tab grows with the width; when there is
/// room it also shows its name.
fn side_tabs(ui: &mut Ui, st: &mut UiState) {
    const TABS: [(i32, &str); 7] = [
        (0, "Layers"),
        (1, "Assets"),
        (2, "Design"),
        (3, "History"),
        (4, "Notes"),
        (5, "Agent"),
        (6, "Annotations"),
    ];
    let p = theme::pal(ui);
    let current = match st.side_extra {
        Some(panels::SideView::History) => 3,
        Some(panels::SideView::Comments) => 4,
        Some(panels::SideView::Agent) => 5,
        Some(panels::SideView::Annotations) => 6,
        None if st.show_design => 2,
        None => i32::from(st.show_library),
    };
    let pad = 3.0;
    let gap = 2.0;
    let dark = ui.visuals().dark_mode;
    egui::Frame::new()
        .fill(theme::well_fill(p, dark))
        .corner_radius(theme::radius_lg())
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
                    if on {
                        theme::fill_selected(ui, rect, crate::theme::ru(9), p.sidebar);
                    } else {
                        ui.painter()
                            .rect_filled(rect, crate::theme::r(9.0), p.hover.gamma_multiply(hover));
                    }
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
                        1 => theme::paint_icon(ui.painter(), icon_box, Icon::Component, tint),
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
                        6 => theme::paint_icon(ui.painter(), icon_box, Icon::Note, tint),
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
                            6 => Some(panels::SideView::Annotations),
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
            panels::SideView::Agent => panels::agent(ui, ed, st),
            panels::SideView::Annotations => annotations::panel(ui, ed, st),
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
        kit::icon_field(ui, Icon::Search, &mut st.layer_filter, "Search layers");
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
            if st.layer_heights_key != (ed.doc_stamp, st.collapsed.len()) {
                st.layer_heights.clear();
                st.layer_heights_key = (ed.doc_stamp, st.collapsed.len());
            }
            // Rows that come into view by scrolling just appear; the pop-in is for a list that
            // was not on screen, or rows that were unfolded.
            let offset = ui.clip_rect().top() - ui.min_rect().top();
            st.layer_animate = (offset - st.layer_offset).abs() < 0.5;
            st.layer_offset = offset;
            for id in roots.iter().rev() {
                layer_item(ui, ed, st, *id, 0);
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
                    .rect_stroke(r, crate::theme::r(6.0), stroke, egui::StrokeKind::Inside);
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

/// Height of a layer row.
const LAYER_ROW_HEIGHT: f32 = 30.0;
/// Space between layer rows (and between a row and the rows below it).
const LAYER_ROW_GAP: f32 = 1.0;
/// Rows this far outside the visible list are still built, so a scroll never shows a gap.
const LAYER_OFFSCREEN_MARGIN: f32 = 200.0;

/// Height of the layer rows of `id` and everything unfolded below it, as `layer_row` lays them
/// out (each child is drawn in its own scope, followed by one gap).
fn layer_subtree_height(
    doc: &crate::model::Document,
    collapsed: &HashSet<NodeId>,
    id: NodeId,
    memo: &mut HashMap<NodeId, f32>,
) -> f32 {
    if let Some(h) = memo.get(&id) {
        return *h;
    }
    let Some(n) = doc.get(id) else {
        return 0.0;
    };
    let mut h = LAYER_ROW_HEIGHT;
    if !collapsed.contains(&id) {
        for c in &n.children {
            h += LAYER_ROW_GAP + layer_subtree_height(doc, collapsed, *c, memo);
        }
    }
    memo.insert(id, h);
    h
}

/// A layer row and its unfolded children. A page can hold thousands of layers, but only a
/// screenful is visible: rows far outside the list are not built, their space is just reserved.
fn layer_item(ui: &mut Ui, ed: &mut Editor, st: &mut UiState, id: NodeId, depth: usize) {
    // A filter builds the whole tree (matches can be anywhere). Scrolling to a layer builds only
    // the subtree that holds it, so the rest stays virtual.
    let holds_reveal = st.reveal.is_some_and(|t| is_within(&ed.doc, t, id));
    if st.layer_filter.trim().is_empty() && !holds_reveal {
        let h = layer_subtree_height(&ed.doc, &st.collapsed, id, &mut st.layer_heights);
        let top = ui.cursor().top();
        let view = ui.clip_rect();
        if top + h < view.top() - LAYER_OFFSCREEN_MARGIN || top > view.bottom() + LAYER_OFFSCREEN_MARGIN {
            ui.add_space(h + LAYER_ROW_GAP);
            return;
        }
    }
    let index = st.layer_rows.len();
    let animate = st.layer_animate;
    motion::pop_in_if(ui, ("layer-pop", id), index, animate, |ui| layer_row(ui, ed, st, id, depth));
}

/// Whether `id` is `root` or below it.
fn is_within(doc: &crate::model::Document, id: NodeId, root: NodeId) -> bool {
    let mut cur = Some(id);
    while let Some(c) = cur {
        if c == root {
            return true;
        }
        cur = doc.get(c).and_then(|n| n.parent);
    }
    false
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
        ui.allocate_exact_size(egui::vec2(width, LAYER_ROW_HEIGHT), egui::Sense::click_and_drag());
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
        theme::fill_selected(ui, row, crate::theme::ru(10), p.field);
    } else if hovered {
        ui.painter().rect_filled(row, crate::theme::r(10.0), p.hover);
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
                ui.painter().rect_filled(r, crate::theme::r(5.0), tint.gamma_multiply(t));
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

    // Right-click acts on the row's layer: it joins the selection first unless already in it.
    if resp.secondary_clicked() && !selected {
        ed.end_text_edit();
        ed.selection = vec![id];
    }
    resp.context_menu(|ui| {
            crate::theme::popup_backing(ui);
            layer_menu(ui, ed, st, id)
        });
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
            layer_item(ui, ed, st, *c, depth + 1);
        }
    }
}

/// The layers panel's right-click menu: the common edits on the selection, then its Actions.
fn layer_menu(ui: &mut Ui, ed: &mut Editor, st: &mut UiState, id: NodeId) {
    use crate::editor::shortcuts::Action as A;
    let Some(n) = ed.doc.get(id) else { return };
    let (name, visible, locked) = (n.name.clone(), n.visible, n.locked);
    let container = n.is_container() && !n.children.is_empty();
    let frame_like = n.is_frame() || n.is_group();
    let mut run = None;
    let mut item = |ui: &mut Ui, icon: Icon, label: &str, keys: &str, act: A| {
        if icon_menu_item_keys(ui, icon, label, keys).clicked() {
            run = Some(act);
            ui.close();
        }
    };
    item(ui, Icon::Export, "Copy", "⌘C", A::Copy);
    item(ui, Icon::Page, "Paste", "⌘V", A::Paste);
    item(ui, Icon::Instance, "Duplicate", "⌘D", A::Duplicate);
    if icon_menu_item_keys(ui, Icon::Text, "Rename", "⌘R").clicked() {
        st.renaming = Some((id, name));
        ui.close();
    }
    ui.separator();
    item(ui, Icon::Group, "Group", "⌘G", A::Group);
    if container {
        item(ui, Icon::Group, "Ungroup", "⇧⌘G", A::Ungroup);
    }
    item(ui, Icon::Frame, "Wrap in frame", "⌥⌘G", A::WrapInFrame);
    if frame_like {
        item(ui, Icon::Component, "Create component", "⌥⌘K", A::CreateComponent);
    }
    ui.separator();
    item(ui, Icon::ChevronUp, "Bring to front", "⇧⌘]", A::ToFront);
    item(ui, Icon::ChevronDown, "Send to back", "⇧⌘[", A::ToBack);
    ui.separator();
    let (eye, eye_label) = if visible { (Icon::EyeSlash, "Hide") } else { (Icon::Eye, "Show") };
    item(ui, eye, eye_label, "⇧⌘H", A::ToggleVisible);
    let (lock, lock_label) = if locked { (Icon::LockOpen, "Unlock") } else { (Icon::Lock, "Lock") };
    item(ui, lock, lock_label, "⇧⌘L", A::ToggleLock);
    item(ui, Icon::Search, "Zoom to layer", "⇧2", A::ZoomSelection);
    ui.separator();
    menus::menu_button(ui, "Actions", |ui| {
        crate::theme::popup_backing(ui);
        if let Some(a) = actions::menu(ui, ed) {
            actions::dispatch(ed, st, a);
            ui.close();
        }
    });
    ui.separator();
    item(ui, Icon::Trash, "Delete", "⌫", A::Delete);
    if let Some(a) = run {
        ed.run(a);
    }
}

/// [`icon_menu_item`] with its shortcut on the right.
fn icon_menu_item_keys(ui: &mut Ui, icon: Icon, label: &str, keys: &str) -> egui::Response {
    let r = icon_menu_item(ui, icon, label);
    ui.painter().text(
        egui::pos2(r.rect.right() - 10.0, r.rect.center().y),
        Align2::RIGHT_CENTER,
        keys,
        egui::FontId::proportional(theme::SECONDARY),
        theme::pal(ui).tertiary,
    );
    r
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
        Tool::Annotate => Icon::Note,
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
                .corner_radius(theme::radius_panel())
                .inner_margin(4.0)
                .stroke(egui::Stroke::NONE)
                .show_surface(ui, |ui| {
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
                .corner_radius(crate::theme::r(16.0))
                .inner_margin(egui::Margin::symmetric(16, 10))
                .show_surface(ui, |ui| {
                    ui.set_max_width(520.0);
                    ui.label(RichText::new(text).color(ink));
                });
        });
}

/// How long the "Agent finished" toast stays on the canvas.
const RUN_TOAST_SECS: f32 = 8.0;
/// Gap between the toast and the canvas bottom, clear of the toolbar.
const RUN_TOAST_LIFT: f32 = 72.0;
/// Corner radius of the review outlines (screen px).
const REVIEW_RADIUS: f32 = 2.0;
const REVIEW_STROKE: f32 = 2.0;
const REVIEW_DASH: f32 = 6.0;

/// While a run is under review: green/yellow outlines on added/changed nodes, a red dashed ghost
/// where removed nodes were, and a count tag over each frame with changes (active page only).
fn review_overlay(ctx: &egui::Context, ed: &Editor, st: &UiState, canvas: egui::Rect) {
    let Some(rd) = &st.review_diff else {
        return;
    };
    let id = ed.active_page_id();
    let Some((i, page)) = rd.pages.iter().enumerate().find(|(_, p)| p.page_id == id) else {
        return;
    };
    let panels::PageReview::Diff(diff) = &page.review else {
        return;
    };
    let snap = rd.snaps.get(i).and_then(Option::as_ref);
    let to_screen = |r: &crate::geom::Rect| {
        let s = ed.camera.world_rect_to_screen(r);
        egui::Rect::from_min_size(
            canvas.min + egui::vec2(s.x as f32, s.y as f32),
            egui::vec2(s.w as f32, s.h as f32),
        )
    };
    egui::Area::new("review-overlay".into())
        .order(egui::Order::Middle)
        .fixed_pos(canvas.min)
        .interactable(false)
        .show(ctx, |ui| {
            let p = theme::pal(ui);
            let painter = ui.painter().with_clip_rect(canvas);
            for g in &diff.groups {
                for n in &g.nodes {
                    let (rect, color, ghost) = match n.status {
                        "added" => (n.id_b.and_then(|b| ed.doc.world_rect(b)), p.success, false),
                        "removed" => (
                            n.id_a.and_then(|a| snap?.world_rect(a)),
                            p.danger,
                            true,
                        ),
                        _ => (n.id_b.and_then(|b| ed.doc.world_rect(b)), p.warning, false),
                    };
                    let Some(rect) = rect else { continue };
                    let r = to_screen(&rect);
                    let stroke = egui::Stroke::new(REVIEW_STROKE, color);
                    if ghost {
                        painter.rect_filled(r, REVIEW_RADIUS, color.gamma_multiply(0.08));
                        let pts = [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()];
                        painter.extend(egui::Shape::dashed_line(&pts, stroke, REVIEW_DASH, REVIEW_DASH * 0.6));
                    } else {
                        painter.rect_stroke(r, REVIEW_RADIUS, stroke, egui::StrokeKind::Outside);
                    }
                }
                let Some(fr) = ed.doc.world_rect(g.frame).or_else(|| snap?.world_rect(g.frame)) else {
                    continue;
                };
                let fr = to_screen(&fr);
                let galley = painter.layout_no_wrap(
                    panels::count(g.nodes.len(), "change"),
                    theme::semibold(theme::SECONDARY),
                    kit::ink_on(p.warning),
                );
                let size = galley.size() + egui::vec2(16.0, 6.0);
                let tag = egui::Rect::from_min_size(
                    egui::pos2(fr.right() - size.x, fr.top() - size.y - 6.0),
                    size,
                );
                painter.rect_filled(tag, crate::theme::r(255.0), p.warning);
                painter.galley(tag.min + egui::vec2(8.0, 3.0), galley, p.warning);
            }
        });
}

/// Narrowest screen (on screen, px) that gets its scroll toggle.
const SCROLL_TOGGLE_MIN_W: f64 = 120.0;
const SCROLL_TOGGLE: f32 = 18.0;

/// Beside each screen's name: a toggle that makes it scroll what overflows (fixed size). Shown
/// on screens that scroll or overflow, and on the one under the pointer.
fn screen_scroll_toggles(ctx: &egui::Context, ed: &mut Editor, canvas: egui::Rect) {
    let hover = ctx.pointer_hover_pos().map(|p| ed.screen_to_world(crate::geom::v2(f64::from(p.x - canvas.min.x), f64::from(p.y - canvas.min.y))));
    let mut toggle = None;
    for (id, label) in ed.frame_labels() {
        let Some(n) = ed.doc.get(id) else { continue };
        let Some(wr) = ed.doc.world_rect(id) else { continue };
        let sr = ed.camera.world_rect_to_screen(&wr);
        if sr.w < SCROLL_TOGGLE_MIN_W {
            continue;
        }
        let overflows = crate::editor::scroll_range(&ed.doc, id) > 0.0;
        if !n.scroll && !overflows && !hover.is_some_and(|p| wr.contains(p)) {
            continue;
        }
        let at = canvas.min + egui::vec2((sr.x + sr.w) as f32 - SCROLL_TOGGLE, label.y as f32);
        let on = n.scroll;
        egui::Area::new(egui::Id::new(("screen-scroll", id)))
            .order(egui::Order::Middle)
            .fixed_pos(at)
            .show(ctx, |ui| {
                let tip = if on { "Scrolls what overflows: click for a plain frame" } else if overflows { "Content overflows: click to keep the size and scroll it" } else { "Keep this screen's size and scroll what overflows" };
                if theme::icon_button(ui, Icon::Scroll, SCROLL_TOGGLE, on).on_hover_text(tip).clicked() {
                    toggle = Some((id, !on));
                }
            });
    }
    if let Some((id, on)) = toggle {
        ed.set_scroll(&[id], on);
    }
}

const AGENT_OUTLINE: f32 = 2.0;
/// How long fresh agent content takes to come in through its skeleton.
const REVEAL_SECS: f32 = 0.35;
/// The skeleton over the area a call being written will change.
const DRAFT_SKELETON_ALPHA: f32 = 0.55;
const DRAFT_OUTLINE_ALPHA: f32 = 0.8;
/// Where a drafted call with no target (a new artboard) shows its skeleton, in world units.
const DRAFT_SPOT: (f64, f64) = (390.0, 280.0);
const DRAFT_SPOT_GAP: f64 = 80.0;

/// The free spot right of the page's artboards, where a new one goes.
fn draft_spot(doc: &crate::model::Document) -> crate::geom::Rect {
    let rects: Vec<crate::geom::Rect> = doc.root.iter().filter_map(|id| doc.world_rect(*id)).collect();
    let (x, y) = crate::geom::Rect::union_all(&rects).map_or((0.0, 0.0), |b| (b.x + b.w + DRAFT_SPOT_GAP, b.y));
    crate::geom::Rect::new(x, y, DRAFT_SPOT.0, DRAFT_SPOT.1)
}
/// Size of the agent cursor's arrow (screen px).
const AGENT_CURSOR: f32 = 12.0;

/// While agents work: accent outlines on the layers they just touched, fading over
/// [`AGENT_MARK_SECS`], and an agent cursor (verb and client) on the newest. When the newest is
/// on another page, a chip "Agent on page X →" switches to it. Repaints only while a mark fades.
fn agent_overlay(ctx: &egui::Context, ed: &mut Editor, st: &mut UiState, canvas: egui::Rect) {
    let marks = st.live_marks();
    let draft = st.chat.drafting.clone().filter(|d| crate::mcp::tools::edits_doc(&d.name, &serde_json::Value::Null));
    if !marks && draft.is_none() {
        return;
    }
    ctx.request_repaint_after(std::time::Duration::from_millis(AGENT_PILL_FRAME_MS));
    let accent = theme::accent();
    let page = ed.active_page_id().to_string();
    let to_screen = |r: &crate::geom::Rect| {
        let s = ed.camera.world_rect_to_screen(r);
        egui::Rect::from_min_size(
            canvas.min + egui::vec2(s.x as f32, s.y as f32),
            egui::vec2(s.w as f32, s.h as f32),
        )
    };
    let fade = |m: &AgentMark| (1.0 - m.at.elapsed().as_secs_f32() / AGENT_MARK_SECS).clamp(0.0, 1.0);
    egui::Area::new("agent-overlay".into())
        .order(egui::Order::Middle)
        .fixed_pos(canvas.min)
        .interactable(false)
        .show(ctx, |ui| {
            let painter = ui.painter().with_clip_rect(canvas);
            let t = ctx.input(|i| i.time) as f32;
            // The call being written: a skeleton where it lands (its target, or the free spot a
            // new artboard takes), until it has run.
            if let Some(d) = &draft {
                let at = match d.target.as_deref().and_then(|k| k.trim_start_matches('#').split_once(':')) {
                    Some((prefix, n)) => crate::mcp::exec::find_page(ed, prefix).ok().filter(|p| *p == ed.active_page).and(n.parse().ok()).and_then(|n| ed.doc.world_rect(NodeId(n))),
                    None => Some(draft_spot(&ed.doc)),
                };
                if let Some(r) = at.map(|r| to_screen(&r)) {
                    kit::skeleton(&painter, r, t, DRAFT_SKELETON_ALPHA);
                    let stroke = egui::Stroke::new(AGENT_OUTLINE, accent.gamma_multiply(DRAFT_OUTLINE_ALPHA));
                    painter.add(egui::Shape::dashed_line(&[r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()], stroke, 6.0, 4.0));
                    let ink = kit::ink_on(accent);
                    let galley = painter.layout_no_wrap(format!("{}\u{2026}", agent_verb(&d.name)), theme::semibold(theme::SECONDARY), ink);
                    let tag = egui::Rect::from_min_size(r.left_top() - egui::vec2(0.0, galley.size().y + 8.0), galley.size() + egui::vec2(12.0, 4.0));
                    painter.rect_filled(tag, crate::theme::r(255.0), accent);
                    painter.galley(tag.min + egui::vec2(6.0, 2.0), galley, ink);
                }
            }
            let rect = |m: &AgentMark| (m.page_id == page).then(|| ed.doc.world_rect(m.node)).flatten().map(|r| to_screen(&r));
            for m in &st.agent_marks {
                let Some(r) = rect(m) else { continue };
                // New content comes in through its skeleton, so the outline never frames nothing.
                let age = m.at.elapsed().as_secs_f32();
                if age < REVEAL_SECS {
                    kit::skeleton(&painter, r, t, (1.0 - age / REVEAL_SECS).powi(2));
                }
                let stroke = egui::Stroke::new(AGENT_OUTLINE, accent.gamma_multiply(fade(m)));
                painter.rect_stroke(r, REVIEW_RADIUS, stroke, egui::StrokeKind::Outside);
            }
            let Some((m, r)) = st.agent_marks.last().and_then(|m| Some((m, rect(m)?))) else { return };
            let a = fade(m);
            let tip = r.left_top();
            let arrow = vec![tip, tip + egui::vec2(0.0, AGENT_CURSOR), tip + egui::vec2(AGENT_CURSOR * 0.75, AGENT_CURSOR * 0.75)];
            painter.add(egui::Shape::convex_polygon(arrow, accent.gamma_multiply(a), egui::Stroke::NONE));
            let text = if m.client.is_empty() { m.verb.to_string() } else { format!("{} \u{b7} {}", m.verb, m.client) };
            let ink = kit::ink_on(accent);
            let galley = painter.layout_no_wrap(text, theme::semibold(theme::SECONDARY), ink.gamma_multiply(a));
            let at = tip + egui::vec2(AGENT_CURSOR * 0.6, AGENT_CURSOR * 0.9);
            let tag = egui::Rect::from_min_size(at, galley.size() + egui::vec2(12.0, 4.0));
            painter.rect_filled(tag, crate::theme::r(255.0), accent.gamma_multiply(a));
            painter.galley(tag.min + egui::vec2(6.0, 2.0), galley, ink);
        });
    let Some(m) = st.agent_marks.last().filter(|m| m.page_id != page) else { return };
    let Some(i) = ed.pages.iter().position(|p| p.id == m.page_id) else { return };
    let text = format!("Agent on page {} \u{2192}", ed.pages[i].name);
    let mut go = false;
    egui::Area::new("agent-elsewhere".into())
        .order(egui::Order::Middle)
        .pivot(Align2::CENTER_TOP)
        .fixed_pos(canvas.center_top() + egui::vec2(0.0, 14.0))
        .show(ctx, |ui| {
            go = kit::chip(ui, &text, true).clicked();
        });
    if go {
        ed.switch_page(i);
    }
}

/// Bottom-centre toast when an agent run ends with changes: Review, Revert, auto-hides.
fn run_toast(ctx: &egui::Context, st: &mut UiState, canvas: egui::Rect) {
    let Some((id, text, at)) = st.run_toast.clone() else {
        return;
    };
    if at.elapsed().as_secs_f32() > RUN_TOAST_SECS {
        st.run_toast = None;
        return;
    }
    ctx.request_repaint_after(std::time::Duration::from_millis(250));
    let mut act = None;
    egui::Area::new("run-toast".into())
        .order(egui::Order::Foreground)
        .pivot(Align2::CENTER_BOTTOM)
        .fixed_pos(egui::pos2(canvas.center().x, canvas.bottom() - RUN_TOAST_LIFT))
        .show(ctx, |ui| {
            ui.multiply_opacity(motion::appear(
                ctx,
                "run-toast-appear",
                motion::MEDIUM,
                motion::ease_out_cubic,
            ));
            let p = theme::pal(ui);
            egui::Frame::popup(ui.style())
                .fill(p.popover)
                .stroke(egui::Stroke::new(1.0, p.hover))
                .corner_radius(crate::theme::r(16.0))
                .inner_margin(egui::Margin::symmetric(16, 12))
                .show_surface(ui, |ui| {
                    ui.horizontal(|ui| {
                        let (r, _) =
                            ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::hover());
                        theme::paint_icon(ui.painter(), r, Icon::Pen, p.secondary);
                        ui.label(RichText::new(text).font(theme::semibold(theme::BODY)));
                        ui.add_space(8.0);
                        if kit::button(ui, kit::ButtonKind::Primary, None, "Review").clicked() {
                            act = Some(false);
                        }
                        if kit::button(ui, kit::ButtonKind::Secondary, Some(Icon::Restore), "Revert")
                            .clicked()
                        {
                            act = Some(true);
                        }
                    });
                });
        });
    egui::Area::new("run-toast-note".into())
        .order(egui::Order::Foreground)
        .fixed_pos(canvas.min + egui::vec2(24.0, 24.0))
        .interactable(false)
        .show(ctx, |ui| {
            let p = theme::pal(ui);
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
                theme::paint_icon(ui.painter(), r, Icon::Restore, p.secondary);
                ui.label(
                    RichText::new("Snapshot saved")
                        .size(theme::SECONDARY)
                        .color(p.secondary),
                );
            });
        });
    let Some(revert) = act else { return };
    st.run_toast = None;
    st.run_banner = None;
    st.side_extra = Some(panels::SideView::Agent);
    if revert {
        st.pending_run_revert = Some(id);
    } else {
        st.review_run = Some(id);
    }
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
                .show_surface(ui, |ui| {
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
                .corner_radius(crate::theme::r(24.0))
                .inner_margin(24.0)
                .stroke(egui::Stroke::NONE)
                .show_surface(ui, |ui| {
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

#[cfg(test)]
mod layer_tests {
    use super::*;
    use crate::geom::Rect;
    use crate::model::Node;

    fn frame(ctx: &egui::Context, ed: &mut Editor, st: &mut UiState, i: u32, h: f32) -> std::time::Duration {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, h))),
            time: Some(f64::from(i) / 60.0),
            predicted_dt: 1.0 / 60.0,
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| {
            draw(ui, ed, st);
        });
        out.textures_delta.clear();
        out.viewport_output[&egui::ViewportId::ROOT].repaint_delay
    }

    fn rect(ed: &mut Editor, parent: Option<NodeId>, frame: bool) -> NodeId {
        let kind = if frame { NodeKind::Frame { clip: false } } else { NodeKind::Rect };
        ed.add_node(Node::new(NodeId(0), kind, Rect::new(0.0, 0.0, 10.0, 10.0)), parent)
    }

    fn many_roots(n: usize) -> Editor {
        let mut ed = Editor::default();
        for _ in 0..n {
            rect(&mut ed, None, false);
        }
        ed
    }

    #[test]
    fn a_resting_layers_list_does_not_repaint() {
        let ctx = egui::Context::default();
        setup_style(&ctx);
        let mut ed = many_roots(300);
        let mut st = UiState::default();
        let mut last = std::time::Duration::ZERO;
        for i in 0..180 {
            last = frame(&ctx, &mut ed, &mut st, i, 800.0);
        }
        assert!(!last.is_zero(), "rows keep asking for frames after their pop-in");
    }

    #[test]
    fn an_agent_pill_is_the_only_thing_repainting_while_it_works() {
        let ctx = egui::Context::default();
        setup_style(&ctx);
        let mut ed = many_roots(300);
        let mut st = UiState::default();
        st.log_agent("get_basic_info", &serde_json::json!({}), None);
        for i in 0..180 {
            frame(&ctx, &mut ed, &mut st, i, 800.0);
        }
        let causes: Vec<String> = ctx.repaint_causes().iter().map(|c| c.to_string()).collect();
        assert!(causes.iter().all(|c| !c.contains("motion.rs")), "{causes:?}");
    }

    fn shown(ctx: &egui::Context, area: &str) -> bool {
        ctx.memory(|m| m.areas().visible_last_frame(&egui::LayerId::new(egui::Order::Middle, egui::Id::new(area))))
    }

    #[test]
    fn agent_outlines_show_after_a_call_and_go_after_three_seconds_without_repainting() {
        let ctx = egui::Context::default();
        setup_style(&ctx);
        let mut ed = many_roots(3);
        let mut st = UiState::default();
        let mut idle = std::time::Duration::ZERO;
        for i in 0..120 {
            idle = frame(&ctx, &mut ed, &mut st, i, 800.0);
        }
        let quiet = std::time::Duration::from_millis(AGENT_PILL_FRAME_MS);
        assert!(idle > quiet, "settled canvas still repaints: {idle:?}");
        let page = ed.active_page_id().to_string();
        let id = ed.doc.root[0];
        st.mark_agent(&ed, "Claude Code".into(), "update_styles", &serde_json::json!({}), &[(page, id)]);
        assert!(frame(&ctx, &mut ed, &mut st, 120, 800.0) <= quiet);
        frame(&ctx, &mut ed, &mut st, 121, 800.0);
        assert!(shown(&ctx, "agent-overlay"));
        assert!(!shown(&ctx, "agent-elsewhere"));
        assert_eq!((st.agent_marks[0].verb, st.agent_marks[0].client.as_str()), ("Styling", "Claude Code"));
        let old = web_time::Instant::now() - std::time::Duration::from_secs_f32(AGENT_MARK_SECS + 0.1);
        st.agent_marks[0].at = old;
        let mut last = std::time::Duration::ZERO;
        for i in 122..240 {
            last = frame(&ctx, &mut ed, &mut st, i, 800.0);
        }
        assert!(st.agent_marks.is_empty());
        assert!(!shown(&ctx, "agent-overlay"));
        assert!(last > quiet, "faded outlines keep repainting: {last:?}");
    }

    #[test]
    fn a_read_sweep_outlines_one_layer_and_follows_nothing_more() {
        let mut ed = many_roots(0);
        let ids: Vec<_> = (0..5).map(|_| rect(&mut ed, None, true)).collect();
        let mut st = UiState::default();
        for id in &ids {
            let key = crate::mcp::exec::node_key(&ed, 0, *id);
            st.mark_agent(&ed, String::new(), "get_node_info", &serde_json::json!({ "nodeId": key }), &[]);
            if *id == ids[0] {
                assert!(st.chat.follow.is_some(), "a lone lookup should still be followed");
            }
            st.chat.follow = None;
        }
        assert_eq!(st.agent_marks.len(), 1);
        assert_eq!(st.agent_marks[0].node, ids[0]);
        assert!(st.chat.follow.is_none(), "a swept read panned the camera");
        // An edit in the middle of a sweep still marks and follows.
        let page = ed.active_page_id().to_string();
        st.mark_agent(&ed, String::new(), "update_styles", &serde_json::json!({}), &[(page, ids[3])]);
        assert_eq!(st.agent_marks.last().map(|m| m.node), Some(ids[3]));
        assert!(st.chat.follow.is_some());
    }

    #[test]
    fn a_lone_read_outlines_again_once_the_sweep_is_over() {
        let mut ed = many_roots(0);
        let id = rect(&mut ed, None, true);
        let key = crate::mcp::exec::node_key(&ed, 0, id);
        let args = serde_json::json!({ "nodeId": key });
        let mut st = UiState::default();
        st.mark_agent(&ed, String::new(), "get_node_info", &args, &[]);
        st.agent_marks.clear();
        st.last_read = Some(web_time::Instant::now() - std::time::Duration::from_secs_f32(AGENT_READ_GAP_SECS + 0.1));
        st.mark_agent(&ed, String::new(), "get_node_info", &args, &[]);
        assert_eq!(st.agent_marks.len(), 1);
    }

    #[test]
    fn the_pill_lapses_sooner_after_reads_than_after_edits() {
        let none = serde_json::json!({});
        let mut st = UiState::default();
        st.log_agent("get_node_info", &none, None);
        assert_eq!(st.agent_working(), Some("Looking at the design"));
        let past = web_time::Instant::now() - std::time::Duration::from_millis(100);
        st.agent_until = Some(past);
        assert!(st.agent_working().is_none());
        let mut st = UiState::default();
        st.log_agent("update_styles", &none, None);
        let until = st.agent_until.unwrap();
        assert!(until > web_time::Instant::now() + std::time::Duration::from_secs(AGENT_WORKING_SECS - 1));
        // A read after an edit never shortens the edit's window.
        st.log_agent("get_node_info", &none, None);
        assert_eq!(st.agent_until, Some(until));
    }

    #[test]
    fn reads_have_honest_verbs_and_unknown_tools_just_work() {
        for t in [
            "get_node_info", "get_children", "get_tree_summary", "get_bounds", "get_selection", "get_basic_info",
            "get_inventory", "get_variables", "get_components", "find_nodes", "get_annotations", "search_icons",
        ] {
            assert_eq!(agent_verb(t), "Looking at the design", "{t}");
        }
        assert_eq!(agent_verb("get_feedback"), "Reading notes");
        for t in ["lint_design", "verify_code", "diff_frames"] {
            assert_eq!(agent_verb(t), "Checking the design", "{t}");
        }
        assert_eq!(agent_verb("write_html"), "Drawing layout");
        assert_eq!(agent_verb("no_such_tool"), "Working");
    }

    #[test]
    fn agent_on_another_page_shows_a_chip_that_switches_to_it() {
        let ctx = egui::Context::default();
        setup_style(&ctx);
        let mut ed = many_roots(1);
        let other = ed.add_page();
        ed.switch_page(other);
        let id = rect(&mut ed, None, true);
        let page = ed.active_page_id().to_string();
        ed.switch_page(0);
        // Following off: a chip offers the agent's page.
        let mut st = UiState::default();
        st.settings.chat.follow_agents = false;
        st.chat.prefs.follow_agents = false;
        let key = crate::mcp::exec::node_key(&ed, other, id);
        st.mark_agent(&ed, String::new(), "get_screenshot", &serde_json::json!({ "nodeId": key }), &[]);
        assert_eq!((st.agent_marks.len(), st.agent_marks[0].page_id.as_str()), (1, page.as_str()));
        for i in 0..2 {
            frame(&ctx, &mut ed, &mut st, i, 800.0);
        }
        assert!(shown(&ctx, "agent-elsewhere"));
        assert_eq!(ed.active_page, 0);
        // Following on (the default): the canvas goes there itself.
        let mut st = UiState::default();
        st.mark_agent(&ed, String::new(), "get_screenshot", &serde_json::json!({ "nodeId": key }), &[]);
        frame(&ctx, &mut ed, &mut st, 2, 800.0);
        assert_eq!(ed.active_page, other);
    }

    #[test]
    fn write_html_outlines_only_its_top_layer_and_counts_group_digits() {
        let mut ed = many_roots(0);
        let top = rect(&mut ed, None, true);
        let kids: Vec<_> = (0..5).map(|_| rect(&mut ed, Some(top), false)).collect();
        let page = ed.active_page_id().to_string();
        let touched: Vec<_> = std::iter::once(top).chain(kids).map(|id| (page.clone(), id)).collect();
        let mut st = UiState::default();
        st.mark_agent(&ed, "Chat".into(), "write_html", &serde_json::json!({}), &touched);
        assert_eq!(st.agent_marks.iter().map(|m| m.node).collect::<Vec<_>>(), [top]);
        assert_eq!((grouped(7), grouped(1340), grouped(1_234_567)), ("7".into(), "1,340".into(), "1,234,567".into()));
        let run = crate::runs::Run {
            calls: 212,
            pages: vec![crate::runs::RunPage { nodes: (0..1340).map(NodeId).collect(), ..Default::default() }],
            ..Default::default()
        };
        assert_eq!(run_progress(&[run]).unwrap(), "212 calls \u{b7} 1,340 changes");
        assert!(run_progress(&[]).is_none());
    }

    #[test]
    fn only_the_visible_layer_rows_are_built() {
        let ctx = egui::Context::default();
        setup_style(&ctx);
        let mut ed = many_roots(3000);
        let mut st = UiState::default();
        for i in 0..3 {
            frame(&ctx, &mut ed, &mut st, i, 600.0);
        }
        assert!(!st.layer_rows.is_empty());
        assert!(st.layer_rows.len() < 60, "{} rows built", st.layer_rows.len());
    }

    /// Measuring the tree walks each unfolded node once (a row's subtree is a memo hit for
    /// the rows below it), not once per ancestor.
    #[test]
    fn layer_heights_measure_each_node_once() {
        let mut ed = Editor::default();
        let mut parent = rect(&mut ed, None, true);
        let top = parent;
        for _ in 0..200 {
            parent = rect(&mut ed, Some(parent), true);
        }
        let mut memo = HashMap::new();
        let h = layer_subtree_height(&ed.doc, &HashSet::new(), top, &mut memo);
        assert_eq!(memo.len(), 201);
        assert!((h - (201.0 * LAYER_ROW_HEIGHT + 200.0 * LAYER_ROW_GAP)).abs() < 0.01);
    }

    /// Rows that are not built are replaced by space computed from `layer_subtree_height`; it
    /// must match what drawing them would take, or the list's scroll range drifts.
    #[test]
    fn skipped_layer_space_matches_drawn_rows() {
        let ctx = egui::Context::default();
        setup_style(&ctx);
        let mut ed = Editor::default();
        for _ in 0..3 {
            let f = rect(&mut ed, None, true);
            let inner = rect(&mut ed, Some(f), true);
            rect(&mut ed, Some(inner), false);
            rect(&mut ed, Some(inner), false);
            rect(&mut ed, Some(f), false);
        }
        let mut st = UiState::default();
        // Fold the middle frame, so the model has to honour it.
        let middle = ed.doc.root[1];
        st.collapsed.insert(middle);
        // Long enough for the pop-in to settle: rows are displaced while it runs.
        for i in 0..90 {
            frame(&ctx, &mut ed, &mut st, i, 2000.0);
        }
        let tops: HashMap<NodeId, f32> =
            st.layer_rows.iter().map(|(id, r, _)| (*id, r.top())).collect();
        let roots: Vec<NodeId> = ed.doc.root.iter().rev().copied().collect();
        let mut expected = tops[&roots[0]];
        for r in &roots {
            assert!((tops[r] - expected).abs() < 0.6, "root {r:?} at {} not {expected}", tops[r]);
            expected += layer_subtree_height(&ed.doc, &st.collapsed, *r, &mut HashMap::new()) + LAYER_ROW_GAP;
        }
    }
}
