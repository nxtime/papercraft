//! The Libraries modal: publish this file, enable libraries from other projects, review updates.

use super::{UiState, confirm, home::relative_time, kit};
use crate::editor::Editor;
use crate::libraries::{self, EnabledStatus, LibrarySummary, PublishDiff, Published};
use crate::project::{Store, now_ms};
use crate::theme::{self, Icon};
use crate::variables::{Collection, Value, Variable};
use egui::{Color32, RichText, Ui};
use std::collections::HashMap;
use std::path::Path;

const MODAL_W: f32 = 720.0;
const MODAL_H: f32 = 560.0;
const CARD_RADIUS: u8 = 16;

#[derive(Default, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    #[default]
    File,
    Libraries,
    Updates,
}

/// A variable value as the Updates tab shows it.
#[derive(Debug, Clone, PartialEq)]
pub enum Shown {
    None,
    Color(Color32, String),
    Text(String),
}

struct VarRow {
    name: String,
    before: Shown,
    after: Shown,
    /// "Added", "Changed" or "Removed".
    how: &'static str,
}

struct UpdateInfo {
    id: String,
    name: String,
    from: u32,
    to: u32,
    published: u64,
    vars: Vec<VarRow>,
    comps: Vec<(String, &'static str)>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RowState {
    Available,
    Enabled,
    UpdateAvailable,
    Missing,
}

struct Row {
    id: String,
    name: String,
    version: u32,
    components: usize,
    variables: usize,
    state: RowState,
    source: Option<LibrarySummary>,
}

/// What the modal shows, read from disk when it opens and after each action.
#[derive(Default)]
struct Scan {
    rows: Vec<Row>,
    updates: Vec<UpdateInfo>,
    previous: Option<(u32, u64)>,
    diff: Option<PublishDiff>,
    components: usize,
    variables: usize,
    hidden: usize,
}

enum Act {
    Publish,
    Enable(String),
    AskDisable(String),
    Disable(String),
    Update(String),
    Skip(String, u32),
}

#[derive(Default)]
pub struct LibrariesUi {
    pub open: bool,
    was_open: bool,
    tab: Tab,
    search: String,
    description: String,
    scan: Scan,
    /// Library id -> source version the user skipped this session.
    skipped: HashMap<String, u32>,
    confirm_disable: Option<String>,
    error: Option<String>,
    note: Option<String>,
}

/// The value of `v` in the collection's default mode (or its first value).
pub fn shown(c: &Collection, v: &Variable) -> Shown {
    match v.values.get(&c.default_mode).or_else(|| v.values.values().next()) {
        Some(Value::Color(x)) => Shown::Color(super::c32(*x), x.to_hex()),
        Some(Value::Number(n)) => Shown::Text(format!("{n}")),
        Some(Value::String(s)) => Shown::Text(s.clone()),
        Some(Value::Boolean(b)) => Shown::Text(b.to_string()),
        Some(Value::Alias(_)) => Shown::Text("alias".into()),
        None => Shown::None,
    }
}

fn find<'a>(cols: impl Iterator<Item = &'a Collection>, name: &str) -> Shown {
    for c in cols {
        if let Some(v) = c.variables.iter().find(|v| v.name == name) {
            return shown(c, v);
        }
    }
    Shown::None
}

/// Mains that publishing would include.
fn count_components(ed: &Editor) -> usize {
    (0..ed.pages.len())
        .filter(|&i| !ed.pages[i].name.starts_with(['_', '.']))
        .map(|i| {
            let doc = ed.page_doc(i);
            doc.nodes
                .values()
                .filter(|n| n.component && n.component_key.is_some() && !n.name.starts_with(['_', '.']))
                .filter(|n| !doc.ancestors(n.id).iter().any(|a| doc.get(*a).is_some_and(|a| a.component)))
                .count()
        })
        .sum()
}

fn scan(ed: &Editor, dir: &Path) -> Scan {
    let mut s = Scan::default();
    let Some(root) = dir.parent() else { return s };
    let store = Store { root: root.to_path_buf() };
    s.components = count_components(ed);
    let local = ed.design.collections.iter().filter(|c| c.library.is_none());
    for v in local.flat_map(|c| &c.variables) {
        if v.hidden {
            s.hidden += 1;
        } else {
            s.variables += 1;
        }
    }
    s.previous = crate::vfs::read_to_string(dir.join(libraries::LIBRARY_FILE))
        .ok()
        .and_then(|t| serde_json::from_str::<Published>(&t).ok())
        .map(|p| (p.version, p.published));
    s.diff = libraries::publish_diff(dir, ed).ok();
    let available = libraries::available(&store, Some(dir));
    let status = libraries::status(&store, dir, ed);
    for st in &status {
        s.rows.push(enabled_row(ed, st, &available));
        if st.update_available {
            s.updates.extend(update_info(ed, &store, dir, st, &available));
        }
    }
    for l in available.iter().filter(|l| !status.iter().any(|st| st.library_id == l.library_id)) {
        s.rows.push(Row {
            id: l.library_id.clone(),
            name: l.name.clone(),
            version: l.version,
            components: l.components,
            variables: l.variables,
            state: RowState::Available,
            source: Some(l.clone()),
        });
    }
    s
}

fn enabled_row(ed: &Editor, st: &EnabledStatus, available: &[LibrarySummary]) -> Row {
    let state = match (st.source_version, st.update_available) {
        (None, _) => RowState::Missing,
        (Some(_), true) => RowState::UpdateAvailable,
        (Some(_), false) => RowState::Enabled,
    };
    Row {
        id: st.library_id.clone(),
        name: st.name.clone(),
        version: st.local_version,
        components: ed.library_docs.get(&st.library_id).map_or(0, |d| d.root.len()),
        variables: ed
            .design
            .collections
            .iter()
            .filter(|c| c.library.as_deref() == Some(&st.library_id))
            .map(|c| c.variables.len())
            .sum(),
        state,
        source: available.iter().find(|l| l.library_id == st.library_id).cloned(),
    }
}

fn update_info(
    ed: &Editor,
    store: &Store,
    dir: &Path,
    st: &EnabledStatus,
    available: &[LibrarySummary],
) -> Option<UpdateInfo> {
    let d = libraries::update_diff(store, dir, &st.library_id).ok()?;
    let src = available.iter().find(|l| l.library_id == st.library_id)?;
    let new: Published = serde_json::from_str(&crate::vfs::read_to_string(src.dir.join(libraries::LIBRARY_FILE)).ok()?).ok()?;
    let own = || ed.design.collections.iter().filter(|c| c.library.as_deref() == Some(&st.library_id));
    let mut vars = Vec::new();
    for (names, how) in [
        (&d.variables_changed, "Changed"),
        (&d.variables_added, "Added"),
        (&d.variables_removed, "Removed"),
    ] {
        vars.extend(names.iter().map(|n| VarRow {
            name: n.clone(),
            before: find(own(), n),
            after: find(new.collections.iter(), n),
            how,
        }));
    }
    let mut comps = Vec::new();
    for (names, how) in [
        (&d.components_changed, "Changed"),
        (&d.components_added, "Added"),
        (&d.components_removed, "Removed"),
    ] {
        comps.extend(names.iter().map(|n| (n.clone(), how)));
    }
    Some(UpdateInfo {
        id: st.library_id.clone(),
        name: st.name.clone(),
        from: st.local_version,
        to: src.version,
        published: src.published,
        vars,
        comps,
    })
}

impl LibrariesUi {
    fn refresh(&mut self, ed: &Editor, dir: Option<&Path>) {
        self.scan = dir.map(|d| scan(ed, d)).unwrap_or_default();
    }

    fn visible_updates(&self) -> impl Iterator<Item = &UpdateInfo> {
        self.scan.updates.iter().filter(|u| self.skipped.get(&u.id) != Some(&u.to))
    }
}

pub fn show(ctx: &egui::Context, ed: &mut Editor, st: &mut UiState) {
    let mut lib = std::mem::take(&mut st.libraries);
    let dir = st.project_dir.clone();
    if lib.open {
        if !lib.was_open {
            lib.error = None;
            lib.note = None;
            lib.refresh(ed, dir.as_deref());
            lib.description = lib.scan.previous.map_or_else(String::new, |_| {
                dir.as_deref()
                    .and_then(|d| crate::vfs::read_to_string(d.join(libraries::LIBRARY_FILE)).ok())
                    .and_then(|t| serde_json::from_str::<Published>(&t).ok())
                    .map(|p| p.description)
                    .unwrap_or_default()
            });
        }
        draw(ctx, ed, st, &mut lib, dir.as_deref());
    }
    lib.was_open = lib.open;
    st.libraries = lib;
}

fn draw(ctx: &egui::Context, ed: &mut Editor, st: &UiState, lib: &mut LibrariesUi, dir: Option<&Path>) {
    let p = theme::pal_from_ctx(ctx);
    let mut act = None;
    let frame = egui::Frame::new().fill(p.sidebar).corner_radius(24.0).inner_margin(24);
    let modal = egui::Modal::new(egui::Id::new("libraries-modal")).frame(frame).show(ctx, |ui| {
        ui.set_width(MODAL_W.min(ctx.content_rect().width() - 96.0));
        if lib.tab != Tab::File {
            ui.set_height(MODAL_H.min(ctx.content_rect().height() - 96.0));
        }
        ui.spacing_mut().item_spacing.y = 8.0;
        header(ui, lib);
        ui.add_space(8.0);
        match lib.tab {
            Tab::File => act = file_tab(ui, st, lib, true),
            Tab::Libraries => act = libraries_tab(ui, st, lib),
            Tab::Updates => act = updates_tab(ui, lib),
        }
    });
    let asking = lib.confirm_disable.clone();
    if let Some(id) = asking {
        let name = lib.scan.rows.iter().find(|r| r.id == id).map_or(id.clone(), |r| r.name.clone());
        let msg = format!(
            "Disable {name}? Fills and fields bound to its variables keep their current value but lose the binding, \
             and its component instances become plain copies."
        );
        match confirm::dialog(ctx, "libraries-disable", "Disable library?", &msg, "Disable") {
            Some(true) => act = Some(Act::Disable(id)),
            Some(false) => lib.confirm_disable = None,
            None => {}
        }
    } else if modal.should_close() || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        lib.open = false;
    }
    if let Some(a) = act {
        apply(a, ed, lib, dir);
    }
}

fn apply(act: Act, ed: &mut Editor, lib: &mut LibrariesUi, dir: Option<&Path>) {
    // Publishing, enabling and disabling look at every page.
    ed.finish_loading();
    lib.error = None;
    lib.note = None;
    let Some(dir) = dir else { return };
    let store = dir.parent().map(|r| Store { root: r.to_path_buf() });
    let result: Result<Option<String>, String> = match act {
        Act::Publish => libraries::publish(dir, ed, lib.description.trim()).map(|p| Some(format!("Published v{}", p.version))),
        Act::Enable(id) => match lib.scan.rows.iter().find(|r| r.id == id).and_then(|r| r.source.clone()) {
            Some(src) => libraries::enable(dir, ed, &src).map(|()| None),
            None => Err("library source not found".into()),
        },
        Act::AskDisable(id) => {
            lib.confirm_disable = Some(id);
            return;
        }
        Act::Disable(id) => {
            lib.confirm_disable = None;
            libraries::disable(dir, ed, &id).map(|r| {
                Some(format!(
                    "Disabled: {} fills, {} bindings and {} instances detached",
                    r.fills, r.bindings, r.instances
                ))
            })
        }
        Act::Update(id) => match &store {
            Some(s) => libraries::update(s, dir, ed, &id).map(|()| Some("Library updated".into())),
            None => Err("library source not found".into()),
        },
        Act::Skip(id, v) => {
            lib.skipped.insert(id, v);
            return;
        }
    };
    match result {
        Ok(note) => lib.note = note,
        Err(e) => lib.error = Some(e),
    }
    lib.refresh(ed, Some(dir));
}

fn header(ui: &mut Ui, lib: &mut LibrariesUi) {
    let n = lib.visible_updates().count();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        let (r, _) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::hover());
        theme::paint_icon(ui.painter(), r, Icon::Library, theme::accent());
        ui.label(RichText::new("Libraries").font(theme::semibold(20.0)));
        ui.add_space(8.0);
        let updates = if n > 0 { format!("Updates · {n}") } else { "Updates".to_string() };
        for (tab, label) in [(Tab::File, "This file"), (Tab::Libraries, "Libraries"), (Tab::Updates, updates.as_str())] {
            if kit::chip(ui, label, lib.tab == tab).clicked() {
                lib.tab = tab;
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if kit::close_button(ui).clicked() {
                lib.open = false;
            }
        });
    });
}

fn count(n: usize, unit: &str) -> String {
    format!("{n} {unit}{}", if n == 1 { "" } else { "s" })
}

fn card(ui: &Ui) -> egui::Frame {
    egui::Frame::new()
        .fill(theme::pal(ui).window)
        .corner_radius(CARD_RADIUS)
        .inner_margin(egui::Margin::symmetric(12, 10))
}

fn status_line(ui: &mut Ui, lib: &LibrariesUi) {
    let p = theme::pal(ui);
    if let Some(e) = &lib.error {
        ui.label(RichText::new(e).size(theme::SECONDARY).color(p.danger));
    }
    if let Some(n) = &lib.note {
        ui.label(RichText::new(n).size(theme::SECONDARY).color(p.success));
    }
}

fn file_tab(ui: &mut Ui, st: &UiState, lib: &mut LibrariesUi, full: bool) -> Option<Act> {
    let p = theme::pal(ui);
    let mut act = None;
    let next = lib.scan.previous.map_or(1, |(v, _)| v + 1);
    card(ui).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 6.0;
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("This file · {}", st.project_name)).font(theme::semibold(theme::BODY)));
                    let chip = lib.scan.previous.map_or("Not published".to_string(), |(v, _)| format!("v{v} published"));
                    kit::chip_ink(ui, &chip, Some(theme::accent()));
                });
                let age = lib.scan.previous.map(|(_, t)| format!("Last published {}.", relative_time(t, now_ms())));
                let diff = lib.scan.diff.as_ref().filter(|d| !d.is_empty()).map(|d| format!("{d} since."));
                let line = [age, diff].into_iter().flatten().collect::<Vec<_>>().join(" ");
                let line = if line.is_empty() { "Nothing to publish yet.".to_string() } else { line };
                ui.label(RichText::new(line).size(theme::SECONDARY).color(p.secondary));
                ui.horizontal(|ui| {
                    kit::chip(ui, &count(lib.scan.components, "component"), false);
                    kit::chip(ui, &count(lib.scan.variables, "variable"), false);
                    kit::chip(ui, &format!("{} hidden", lib.scan.hidden), false);
                });
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let enabled = st.project_dir.is_some();
                if ui
                    .add_enabled_ui(enabled, |ui| kit::button(ui, kit::ButtonKind::Primary, Some(Icon::Export), &format!("Publish v{next}")))
                    .inner
                    .clicked()
                {
                    act = Some(Act::Publish);
                }
            });
        });
    });
    if full {
        ui.add_space(4.0);
        kit::section_header(ui, "Description", None);
        ui.add(
            egui::TextEdit::multiline(&mut lib.description)
                .hint_text("What this library is for")
                .desired_rows(3)
                .desired_width(f32::INFINITY)
                .margin(theme::INPUT_MARGIN),
        );
        ui.label(
            RichText::new("Hidden variables and components named with a leading _ or . are not published.")
                .size(theme::SECONDARY)
                .color(p.tertiary),
        );
    }
    status_line(ui, lib);
    act
}

fn libraries_tab(ui: &mut Ui, st: &UiState, lib: &mut LibrariesUi) -> Option<Act> {
    let p = theme::pal(ui);
    let mut act = file_tab(ui, st, lib, false);
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        kit::section_header(ui, "Available libraries", None);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add(
                egui::TextEdit::singleline(&mut lib.search)
                    .hint_text("Search libraries")
                    .desired_width(200.0)
                    .margin(theme::INPUT_MARGIN),
            );
        });
    });
    let q = lib.search.to_lowercase();
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 6.0;
        let mut any = false;
        for r in lib.scan.rows.iter().filter(|r| r.name.to_lowercase().contains(&q)) {
            any = true;
            card(ui).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 10.0;
                    kit::badge(ui, Icon::Library, theme::accent(), 36.0);
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        ui.spacing_mut().interact_size.y = 16.0;
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(&r.name).font(theme::semibold(theme::BODY)));
                            match r.state {
                                RowState::UpdateAvailable => kit::chip_ink(ui, "Update available", Some(theme::accent())),
                                RowState::Enabled => kit::chip(ui, "Enabled", false),
                                RowState::Missing => kit::chip_ink(ui, "Missing", Some(p.warning)),
                                RowState::Available => return,
                            };
                        });
                        let counts = format!("v{} · {} · {}", r.version, count(r.components, "component"), count(r.variables, "variable"));
                        ui.label(RichText::new(counts).size(theme::SECONDARY).color(p.secondary));
                        let (dot, text) = match r.state {
                            RowState::Missing => (p.warning, "Source not found · copy still works"),
                            RowState::UpdateAvailable => (p.success, "Source found · update ready"),
                            RowState::Enabled => (p.success, "Source found · up to date"),
                            RowState::Available => (p.success, "Source found"),
                        };
                        ui.horizontal(|ui| {
                            let (rect, _) = ui.allocate_exact_size(egui::vec2(8.0, 14.0), egui::Sense::hover());
                            ui.painter().circle_filled(rect.center(), 3.0, dot);
                            ui.label(RichText::new(text).size(theme::SECONDARY).color(p.tertiary));
                        });
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if r.state == RowState::Missing {
                            if kit::button(ui, kit::ButtonKind::Secondary, None, "Disable").clicked() {
                                act = Some(Act::AskDisable(r.id.clone()));
                            }
                            return;
                        }
                        let on = r.state != RowState::Available;
                        let mut v = on;
                        if kit::toggle(ui, &mut v).changed() {
                            act = Some(if on { Act::AskDisable(r.id.clone()) } else { Act::Enable(r.id.clone()) });
                        }
                    });
                });
            });
        }
        if !any {
            ui.label(
                RichText::new("No libraries yet. Publish another project, then enable it here.")
                    .size(theme::SECONDARY)
                    .color(p.secondary),
            );
        }
    });
    act
}

fn swatch(ui: &mut Ui, s: &Shown) {
    let p = theme::pal(ui);
    match s {
        Shown::None => {
            ui.label(RichText::new("none").size(theme::BODY).color(p.secondary));
        }
        Shown::Text(t) => {
            ui.label(RichText::new(t).size(theme::BODY));
        }
        Shown::Color(c, hex) => {
            let (r, _) = ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
            ui.painter().rect_filled(r, 4.0, *c);
            ui.label(RichText::new(hex.to_uppercase()).size(theme::BODY));
        }
    }
}

fn updates_tab(ui: &mut Ui, lib: &mut LibrariesUi) -> Option<Act> {
    let p = theme::pal(ui);
    let mut act = None;
    let updates: Vec<&UpdateInfo> = lib.visible_updates().collect();
    if updates.is_empty() {
        kit::empty_state(ui, Icon::Library, theme::accent(), "Everything is up to date", "", |_| {});
        status_line(ui, lib);
        return None;
    }
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        for u in updates {
            ui.horizontal(|ui| {
                kit::badge(ui, Icon::Library, theme::accent(), 36.0);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&u.name).font(theme::semibold(15.0)));
                        kit::chip_ink(ui, &format!("v{} → v{}", u.from, u.to), Some(theme::accent()));
                    });
                    let t = format!("Published {} · source found", relative_time(u.published, now_ms()));
                    ui.label(RichText::new(t).size(theme::SECONDARY).color(p.secondary));
                });
            });
            ui.add_space(4.0);
            kit::section_header(ui, &format!("Variables · {} changed", u.vars.len()), None);
            if !u.vars.is_empty() {
                card(ui).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    for r in &u.vars {
                        ui.horizontal(|ui| {
                            ui.allocate_ui_with_layout(egui::vec2(190.0, 20.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                ui.set_min_width(190.0);
                                ui.add(egui::Label::new(RichText::new(&r.name).font(theme::semibold(theme::BODY))).truncate());
                            });
                            ui.allocate_ui_with_layout(egui::vec2(110.0, 20.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                ui.set_min_width(110.0);
                                swatch(ui, &r.before);
                            });
                            let (a, _) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
                            theme::paint_icon(ui.painter(), a, Icon::ArrowRight, p.secondary);
                            if r.how == "Removed" {
                                ui.label(RichText::new("removed").size(theme::BODY).color(p.secondary));
                            } else {
                                swatch(ui, &r.after);
                            }
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                chip_how(ui, r.how);
                            });
                        });
                    }
                });
            }
            ui.add_space(4.0);
            let added = u.comps.iter().filter(|c| c.1 == "Added").count();
            let changed = u.comps.iter().filter(|c| c.1 == "Changed").count();
            let removed = u.comps.len() - added - changed;
            let mut title = format!("Components · {added} added · {changed} changed");
            if removed > 0 {
                title += &format!(" · {removed} removed");
            }
            kit::section_header(ui, &title, None);
            ui.horizontal_wrapped(|ui| {
                for (name, how) in &u.comps {
                    card(ui).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(name).font(theme::semibold(theme::SECONDARY)));
                            chip_how(ui, how);
                        });
                    });
                }
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let (a, _) = ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
                theme::paint_icon(ui.painter(), a, Icon::Question, p.secondary);
                ui.label(
                    RichText::new("Instance overrides are kept. Nothing changes until you press Update.")
                        .size(theme::SECONDARY)
                        .color(p.secondary),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if kit::button(ui, kit::ButtonKind::Primary, Some(Icon::Restore), &format!("Update to v{}", u.to)).clicked() {
                        act = Some(Act::Update(u.id.clone()));
                    }
                    if kit::button(ui, kit::ButtonKind::Secondary, None, "Skip").clicked() {
                        act = Some(Act::Skip(u.id.clone(), u.to));
                    }
                });
            });
            ui.add_space(8.0);
        }
        status_line(ui, lib);
    });
    act
}

fn chip_how(ui: &mut Ui, how: &str) {
    let p = theme::pal(ui);
    let ink = match how {
        "Added" => p.success,
        "Removed" => p.danger,
        _ => theme::accent(),
    };
    kit::chip_ink(ui, how, Some(ink));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(lib: &mut LibrariesUi, ed: &mut Editor, st: &mut UiState) {
        let ctx = egui::Context::default();
        super::super::setup_style(&ctx);
        for _ in 0..2 {
            st.libraries = std::mem::take(lib);
            let mut out = ctx.run_ui(Default::default(), |ui| show(ui.ctx(), ed, st));
            out.textures_delta.clear();
            *lib = std::mem::take(&mut st.libraries);
        }
    }

    #[test]
    fn each_tab_renders() {
        let root = std::env::temp_dir().join(format!("papercraft-libui-{:016x}", fastrand::u64(..)));
        let store = Store::open(root).unwrap();
        let (project, _) = store.create("Kit").unwrap();
        for dir in [None, Some(project.dir.clone())] {
            for tab in [Tab::File, Tab::Libraries, Tab::Updates] {
                let mut ed = Editor::default();
                let mut st = UiState { project_dir: dir.clone(), project_name: "Kit".into(), ..UiState::default() };
                let mut lib = LibrariesUi { open: true, tab, ..LibrariesUi::default() };
                render(&mut lib, &mut ed, &mut st);
                assert!(lib.open);
            }
        }
    }

    #[test]
    fn shown_formats_values() {
        let c = Collection {
            id: "c".into(),
            name: "C".into(),
            modes: vec![],
            default_mode: "m".into(),
            variables: vec![],
            library: None,
        };
        let var = |v: Value| Variable {
            id: "v".into(),
            name: "v".into(),
            kind: crate::variables::Kind::Number,
            values: [("m".to_string(), v)].into(),
            description: String::new(),
            hidden: false,
        };
        assert_eq!(shown(&c, &var(Value::Number(16.0))), Shown::Text("16".into()));
        let s = shown(&c, &var(Value::Color(crate::model::Color::rgb(1, 2, 3))));
        assert!(matches!(s, Shown::Color(_, h) if h == "#010203"));
    }
}
