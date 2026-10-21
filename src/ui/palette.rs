//! ⌘K command palette: fuzzy search over every action, tool, page, layer and library item.

use super::UiState;
use crate::editor::shortcuts::{self, Action};
use crate::editor::{Editor, Tool};
use crate::model::NodeId;
use crate::theme;

#[derive(Default)]
pub struct PaletteUi {
    pub open: bool,
    pub query: String,
    pub selected: usize,
    /// Focus the text field on the next frame.
    focus: bool,
}

impl PaletteUi {
    pub fn toggle(&mut self) {
        self.open = !self.open;
        self.query.clear();
        self.selected = 0;
        self.focus = self.open;
    }
}

#[derive(Clone)]
pub enum Command {
    Run(Action),
    Page(usize),
    Layer(NodeId),
    Library(String),
    ToggleLibrary,
    Theme(theme::Choice),
    /// Set the interface scale (native app only).
    Scale(f32),
    /// Open the settings dialog.
    Settings,
    /// Back to the projects home screen.
    Home,
    /// Check for a newer PaperCraft.
    Updates,
    /// Step through the page's frames full screen.
    Present,
    /// Show a sidebar view.
    Side(super::panels::SideView),
    /// Review a saved agent run.
    ReviewRun(String),
    /// Revert a saved agent run.
    RevertRun(String),
}

pub struct Entry {
    pub label: String,
    pub detail: String,
    pub cmd: Command,
}

/// Subsequence match score (higher is better); None if `q` isn't a subsequence of `s`.
pub fn fuzzy(q: &str, s: &str) -> Option<i64> {
    if q.is_empty() {
        return Some(0);
    }
    let s: Vec<char> = s.to_lowercase().chars().collect();
    let mut score = 0i64;
    let mut si = 0;
    let mut prev_match: Option<usize> = None;
    for qc in q.to_lowercase().chars().filter(|c| !c.is_whitespace()) {
        let mut found = None;
        while si < s.len() {
            if s[si] == qc {
                found = Some(si);
                si += 1;
                break;
            }
            si += 1;
        }
        let i = found?;
        // Word starts and consecutive runs score higher; late matches cost a little.
        if i == 0 || !s[i - 1].is_alphanumeric() {
            score += 10;
        }
        if prev_match.is_some_and(|p| p + 1 == i) {
            score += 12;
        }
        score -= i as i64 / 4;
        prev_match = Some(i);
    }
    Some(score - s.len() as i64 / 8)
}

pub fn entries(ed: &Editor, st: &UiState) -> Vec<Entry> {
    let keys: Vec<(String, &str)> = shortcuts::help_table();
    let key_for = |a: Action| -> String {
        shortcuts::lookup_desc(a)
            .and_then(|d| keys.iter().find(|(_, kd)| *kd == d))
            .map(|(k, _)| k.split(" / ").next().unwrap_or("").to_string())
            .unwrap_or_default()
    };
    let mut out: Vec<Entry> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (_, _, a, desc) in shortcuts::all_bindings() {
        if matches!(a, Action::Nudge(..) | Action::CommandPalette) || !seen.insert(desc.to_string())
        {
            continue;
        }
        out.push(Entry {
            label: desc.to_string(),
            detail: key_for(a),
            cmd: Command::Run(a),
        });
    }
    for t in Tool::ALL {
        let label = format!("{} tool", t.label());
        if seen.insert(label.clone()) {
            out.push(Entry {
                label,
                detail: t.shortcut().into(),
                cmd: Command::Run(Action::Tool(t)),
            });
        }
    }
    for (label, a) in [
        ("Export PNG…", Action::ExportPng),
        ("Export SVG…", Action::ExportSvg),
        (
            "Union selection",
            Action::Boolean(crate::editor::vector_ops::BoolOp::Union),
        ),
        ("Hand-drawn style on/off", Action::ToggleSketch),
    ] {
        if seen.insert(label.to_string()) {
            out.push(Entry {
                label: label.into(),
                detail: key_for(a),
                cmd: Command::Run(a),
            });
        }
    }
    out.push(Entry {
        label: if st.show_library {
            "Show layers".into()
        } else {
            "Show library".into()
        },
        detail: String::new(),
        cmd: Command::ToggleLibrary,
    });
    for (label, cmd) in [
        ("Go to projects home", Command::Home),
        ("Check for updates", Command::Updates),
        ("Present this page", Command::Present),
        (
            "Show history",
            Command::Side(super::panels::SideView::History),
        ),
        (
            "Show comments",
            Command::Side(super::panels::SideView::Comments),
        ),
        (
            "Show agent activity",
            Command::Side(super::panels::SideView::Agent),
        ),
    ] {
        out.push(Entry {
            label: label.into(),
            detail: String::new(),
            cmd,
        });
    }
    if let Some(run) = st
        .runs
        .iter()
        .find(|run| run.state != crate::runs::RunState::Running)
    {
        out.push(Entry {
            label: "Review last agent run".into(),
            detail: String::new(),
            cmd: Command::ReviewRun(run.id.clone()),
        });
        if run.state != crate::runs::RunState::Reverted {
            out.push(Entry {
                label: "Revert last agent run".into(),
                detail: String::new(),
                cmd: Command::RevertRun(run.id.clone()),
            });
        }
    }
    out.push(Entry {
        label: "Settings…".into(),
        detail: "Preferences".into(),
        cmd: Command::Settings,
    });
    for c in theme::Choice::ALL {
        out.push(Entry {
            label: format!("Theme: {}", c.label()),
            detail: "Appearance".into(),
            cmd: Command::Theme(c),
        });
    }
    #[cfg(not(target_arch = "wasm32"))]
    for scale in theme::UI_SCALES {
        out.push(Entry {
            label: format!("Interface scale: {:.0}%", scale * 100.0),
            detail: "Appearance".into(),
            cmd: Command::Scale(scale),
        });
    }
    for (i, p) in ed.pages.iter().enumerate() {
        out.push(Entry {
            label: format!("Go to page: {}", p.name),
            detail: "Page".into(),
            cmd: Command::Page(i),
        });
    }
    for item in &st.library.library.items {
        out.push(Entry {
            label: format!("Insert: {}", item.name),
            detail: "Library".into(),
            cmd: Command::Library(item.id.clone()),
        });
    }
    // Layers last: there can be many.
    for id in ed.doc.paint_order().into_iter().rev().take(500) {
        let n = ed.doc.get(id).unwrap();
        out.push(Entry {
            label: format!("Select layer: {}", n.name),
            detail: n.kind.type_name().into(),
            cmd: Command::Layer(id),
        });
    }
    out
}

/// Ranked matches for a query.
pub fn search(ed: &Editor, st: &UiState, q: &str) -> Vec<Entry> {
    let mut scored: Vec<(i64, usize, Entry)> = entries(ed, st)
        .into_iter()
        .enumerate()
        .filter_map(|(i, e)| fuzzy(q, &e.label).map(|s| (s, i, e)))
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    scored.into_iter().map(|(_, _, e)| e).collect()
}

pub fn run(ed: &mut Editor, st: &mut UiState, cmd: Command) {
    match cmd {
        Command::Run(a) => ed.run(a),
        Command::Page(i) => ed.switch_page(i),
        Command::Layer(id) => {
            ed.end_text_edit();
            ed.selection = vec![id];
            ed.run(Action::ZoomSelection);
        }
        Command::Library(id) => {
            if let Some(item) = st
                .library
                .library
                .items
                .iter()
                .find(|x| x.id == id)
                .cloned()
            {
                let at = ed.camera.screen_to_world(ed.viewport * 0.5);
                ed.insert_library_item(&item, at);
            }
        }
        Command::ToggleLibrary => {
            st.show_library = !st.show_library;
            st.show_design = false;
            st.side_extra = None;
        }
        Command::Home => ed.requests.push(crate::editor::AppRequest::Home),
        Command::Updates => {
            #[cfg(not(target_arch = "wasm32"))]
            {
                st.updater.open = true;
            }
        }
        Command::Present => super::start_present(ed, st),
        Command::Side(v) => {
            st.side_extra = Some(v);
            st.show_library = false;
            st.show_design = false;
        }
        Command::ReviewRun(id) => {
            st.review_run = Some(id);
            st.side_extra = Some(super::panels::SideView::Agent);
            st.show_library = false;
            st.show_design = false;
        }
        Command::RevertRun(id) => {
            st.pending_run_revert = Some(id);
            st.side_extra = Some(super::panels::SideView::Agent);
            st.show_library = false;
            st.show_design = false;
        }
        Command::Settings => st.settings.open = true,
        // Needs the egui context; handled where the palette is drawn.
        Command::Theme(_) | Command::Scale(_) => {}
    }
}

pub fn draw(ctx: &egui::Context, ed: &mut Editor, st: &mut UiState, canvas: egui::Rect) {
    // Open and close both animate: the palette keeps drawing, inert and fading, for a beat after it closes.
    let t = super::motion::progress(
        ctx,
        "palette-open",
        st.palette.open,
        super::motion::FAST,
        super::motion::ease_out_cubic,
    );
    if !st.palette.open && t <= 0.0 {
        return;
    }
    let live = st.palette.open;
    let results = search(ed, st, &st.palette.query.clone());
    let n = results.len();
    let (up, down, enter, esc) = ctx.input(|i| {
        (
            live && i.key_pressed(egui::Key::ArrowUp),
            live && i.key_pressed(egui::Key::ArrowDown),
            live && i.key_pressed(egui::Key::Enter),
            live && i.key_pressed(egui::Key::Escape),
        )
    });
    if down && n > 0 {
        st.palette.selected = (st.palette.selected + 1).min(n - 1);
    }
    if up {
        st.palette.selected = st.palette.selected.saturating_sub(1);
    }
    st.palette.selected = st.palette.selected.min(n.saturating_sub(1));
    let mut chosen: Option<Command> = None;
    if esc {
        st.palette.open = false;
        ctx.request_repaint();
        return;
    }
    if enter && n > 0 {
        chosen = Some(results[st.palette.selected].cmd.clone());
    }
    let width = 640.0f32.min(canvas.width() - 40.0);
    egui::Area::new("palette".into())
        .order(egui::Order::Foreground)
        .fade_in(false)
        .pivot(egui::Align2::CENTER_TOP)
        .fixed_pos(egui::pos2(
            canvas.center().x,
            canvas.top() + 70.0 + (1.0 - super::motion::ease_out_back(t)) * -10.0,
        ))
        .interactable(live)
        .show(ctx, |ui| {
            ui.multiply_opacity(t);
            let p = theme::pal(ui);
            egui::Frame::popup(ui.style())
                .fill(p.sidebar)
                .corner_radius(24.0)
                .inner_margin(16.0)
                .stroke(egui::Stroke::NONE)
                .show(ui, |ui| {
                    ui.set_width(width);
                    let r = egui::Frame::new()
                        .fill(p.field)
                        .corner_radius(16.0)
                        .inner_margin(egui::Margin::symmetric(14, 6))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let (ir, _) = ui.allocate_exact_size(
                                    egui::vec2(18.0, 18.0),
                                    egui::Sense::hover(),
                                );
                                theme::paint_icon(
                                    ui.painter(),
                                    ir,
                                    theme::Icon::Pointer,
                                    p.secondary,
                                );
                                let r = ui.add(
                                    egui::TextEdit::singleline(&mut st.palette.query)
                                        .margin(crate::theme::INPUT_MARGIN)
                                        .hint_text("Search actions, tools, pages, layers…")
                                        .font(egui::FontId::proportional(15.0))
                                        .frame(egui::Frame::NONE)
                                        .desired_width(ui.available_width() - 44.0),
                                );
                                super::kit::key_cap(ui, "Esc");
                                r
                            })
                            .inner
                        })
                        .inner;
                    if st.palette.focus {
                        r.request_focus();
                        st.palette.focus = false;
                    }
                    if r.changed() {
                        st.palette.selected = 0;
                    }
                    ui.add_space(8.0);
                    egui::ScrollArea::vertical()
                        .max_height(340.0)
                        .show(ui, |ui| {
                            for (i, e) in results.iter().enumerate().take(60) {
                                let sel = i == st.palette.selected;
                                let (row, resp) = ui.allocate_exact_size(
                                    egui::vec2(ui.available_width(), 40.0),
                                    egui::Sense::click(),
                                );
                                if sel {
                                    ui.painter().rect_filled(row, 12.0, p.field);
                                    ui.scroll_to_rect(row, None);
                                } else if resp.hovered() {
                                    ui.painter().rect_filled(row, 12.0, p.hover);
                                }
                                ui.painter().text(
                                    egui::pos2(row.left() + 14.0, row.center().y),
                                    egui::Align2::LEFT_CENTER,
                                    &e.label,
                                    if sel {
                                        theme::semibold(theme::BODY + 1.0)
                                    } else {
                                        egui::FontId::proportional(theme::BODY + 1.0)
                                    },
                                    if sel { p.text } else { p.text },
                                );
                                ui.painter().text(
                                    egui::pos2(row.right() - 14.0, row.center().y),
                                    egui::Align2::RIGHT_CENTER,
                                    &e.detail,
                                    egui::FontId::proportional(theme::SECONDARY),
                                    p.secondary,
                                );
                                if resp.clicked() {
                                    chosen = Some(e.cmd.clone());
                                }
                            }
                            if results.is_empty() {
                                ui.add_space(12.0);
                                super::kit::empty_state(
                                    ui,
                                    theme::Icon::Question,
                                    p.secondary,
                                    "No matches",
                                    "Try a different word, or search layers, pages and commands.",
                                    |_| {},
                                );
                                ui.add_space(12.0);
                            }
                        });
                });
        });
    if let Some(c) = chosen {
        st.palette.open = false;
        if let Command::Theme(t) = c {
            theme::apply_choice(ctx, t);
            t.save();
        } else if let Command::Scale(v) = c {
            theme::apply_ui_scale(ctx, v, true);
        } else {
            run(ed, st, c);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuzzy_prefers_word_starts() {
        assert!(fuzzy("grp", "Group selection").is_some());
        assert!(fuzzy("xyz", "Group selection").is_none());
        let a = fuzzy("al", "Align left").unwrap();
        let b = fuzzy("al", "Hand-drawn style on/off").unwrap_or(i64::MIN);
        assert!(a > b);
    }

    #[test]
    fn search_finds_actions_pages_and_layers() {
        let ed = crate::demo::showcase();
        let st = UiState::default();
        let r = search(&ed, &st, "union");
        assert!(
            matches!(r[0].cmd, Command::Run(Action::Boolean(_))),
            "{}",
            r[0].label
        );
        let r = search(&ed, &st, "week plan");
        assert!(r.iter().any(|e| matches!(e.cmd, Command::Layer(_))));
        let r = search(&ed, &st, "page 1");
        assert!(r.iter().any(|e| matches!(e.cmd, Command::Page(0))));
    }
}
