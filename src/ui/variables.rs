//! The Variables modal: collections and their group tree on the left, a table with one column per
//! mode in the middle, the selected variable's details on the right. Every edit goes through
//! `Editor::edit_design` (one undo step per page); a refused edit shows in the footer.

use super::{UiState, kit, motion};
use crate::editor::Editor;
use crate::mcp::codegen;
use crate::model::{Color, Document};
use crate::theme::{self, Icon};
use crate::variables::{self, Collection, DesignSystem, Error, Kind, Value, Variable};
use egui::{Align, Align2, Layout, Rect, RichText, Sense, Ui, UiBuilder, vec2};
use std::collections::BTreeSet;

const MODAL_W: f32 = 1280.0;
const MODAL_H: f32 = 820.0;
const SCREEN_MARGIN: f32 = 24.0;
const MODAL_PAD: f32 = 16.0;
const LEFT_W: f32 = 200.0;
const RIGHT_W: f32 = 290.0;
const GAP: f32 = 12.0;
const HEADER_H: f32 = 44.0;
const FOOTER_H: f32 = 52.0;
const ROW_H: f32 = 34.0;
const NAME_W: f32 = 230.0;
const MODE_MIN_W: f32 = 150.0;
const TREE_INDENT: f32 = 14.0;
const PICKER_MAX_ROWS: usize = 200;

#[derive(Default)]
pub struct VariablesUi {
    pub open: bool,
    coll: Option<String>,
    group: Option<String>,
    /// Group headers folded in the table.
    folded: BTreeSet<String>,
    /// Group paths unfolded in the left tree.
    tree_open: BTreeSet<String>,
    selected: Option<String>,
    search: String,
    /// Footer message and whether it is an error.
    note: Option<(String, bool)>,
    /// New collection name being typed.
    new_coll: Option<String>,
    /// Mode being renamed: (collection id, mode id, draft).
    mode_edit: Option<(String, String, String)>,
    confirm: Option<Confirm>,
    /// `used_count` per CSS name, valid for the editor revision it was filled at.
    used: (u64, std::collections::HashMap<String, usize>),
}

enum Confirm {
    DeleteVar {
        id: String,
        name: String,
        deps: Vec<String>,
    },
    DeleteMode {
        coll: String,
        mode: String,
        name: String,
    },
}

enum Op {
    Collection(String),
    Variable(String, String, Kind),
    Mode(String),
    RemoveMode(String, String),
    DefaultMode(String, String),
    RenameMode(String, String, String),
    Value(String, String, Value),
    Rename(String, String),
    Describe(String, String),
    Hide(String, bool),
    Delete(String, bool),
}

enum Done {
    Nothing,
    Coll(String),
    Var(String),
}

// ------------------------------------------------------------------------------ pure helpers

/// One table row.
#[derive(Debug, PartialEq)]
pub enum Row<'a> {
    Group(&'a str),
    Var(&'a Variable),
}

fn in_group(name: &str, group: &str) -> bool {
    variables::group_of(name) == group
        || name.strip_prefix(group).is_some_and(|r| r.starts_with('/'))
}

/// Variables of `c` under `group` (all when `None`) whose name contains `search`, ordered by
/// group with a header before each non-empty group. A folded group keeps its header only
/// (searching ignores folds).
pub fn rows<'a>(
    c: &'a Collection,
    group: Option<&str>,
    search: &str,
    folded: &BTreeSet<String>,
) -> Vec<Row<'a>> {
    let needle = search.trim().to_lowercase();
    let mut vars: Vec<&Variable> = c
        .variables
        .iter()
        .filter(|v| group.is_none_or(|g| in_group(&v.name, g)))
        .filter(|v| needle.is_empty() || v.name.to_lowercase().contains(&needle))
        .collect();
    vars.sort_by_key(|v| variables::group_of(&v.name));
    let mut out = Vec::new();
    let mut last = None;
    for v in vars {
        let g = variables::group_of(&v.name);
        if last != Some(g) {
            last = Some(g);
            if !g.is_empty() {
                out.push(Row::Group(g));
            }
        }
        if needle.is_empty() && folded.contains(g) {
            continue;
        }
        out.push(Row::Var(v));
    }
    out
}

/// Fills and field bindings on one page that follow variable `css`.
pub fn used_in(doc: &Document, css: &str) -> usize {
    doc.nodes
        .values()
        .map(|n| {
            n.style.fills.iter().filter(|p| p.var() == Some(css)).count()
                + n.bindings.values().filter(|b| b.as_str() == css).count()
        })
        .sum()
}

/// [`used_in`] summed over every page.
pub fn used_count(ed: &Editor, css: &str) -> usize {
    (0..ed.pages.len()).map(|i| used_in(ed.page_doc(i), css)).sum()
}

/// Local collections first, then library ones.
fn ordered(ds: &DesignSystem) -> Vec<&Collection> {
    let (local, lib): (Vec<_>, Vec<_>) = ds.collections.iter().partition(|c| c.library.is_none());
    local.into_iter().chain(lib).collect()
}

fn group_count(c: &Collection, g: &str) -> usize {
    c.variables.iter().filter(|v| in_group(&v.name, g)).count()
}

fn kind_icon(k: Kind) -> Icon {
    match k {
        Kind::Color => Icon::Ellipse,
        Kind::Number => Icon::Grid,
        Kind::String => Icon::Text,
        Kind::Boolean => Icon::Eye,
    }
}

fn kind_label(k: Kind) -> &'static str {
    match k {
        Kind::Color => "Color",
        Kind::Number => "Number",
        Kind::String => "String",
        Kind::Boolean => "Boolean",
    }
}

fn c32(c: Color) -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(c.r, c.g, c.b, c.a)
}

fn fmt_num(n: f64) -> String {
    format!("{n}")
}

// ------------------------------------------------------------------------------ writes

fn local<'a>(ds: &'a mut DesignSystem, id: &str) -> Result<&'a mut Collection, Error> {
    let c = ds
        .collections
        .iter_mut()
        .find(|c| c.id == id)
        .ok_or_else(|| Error::NotFound(id.into()))?;
    if c.library.is_some() {
        return Err(Error::ReadOnly(c.name.clone()));
    }
    Ok(c)
}

fn local_var<'a>(ds: &'a mut DesignSystem, id: &str) -> Result<&'a mut Variable, Error> {
    let c = ds
        .collections
        .iter_mut()
        .find(|c| c.variables.iter().any(|v| v.id == id))
        .ok_or_else(|| Error::NotFound(id.into()))?;
    if c.library.is_some() {
        return Err(Error::ReadOnly(c.name.clone()));
    }
    Ok(c.variables.iter_mut().find(|v| v.id == id).unwrap())
}

fn run(ds: &mut DesignSystem, op: &Op) -> Result<Done, Error> {
    match op {
        Op::Collection(name) => Ok(Done::Coll(ds.add_collection(name))),
        Op::Variable(c, name, kind) => ds.add_variable(c, name, *kind).map(Done::Var),
        Op::Mode(c) => {
            let n = local(ds, c)?.modes.len() + 1;
            ds.add_mode(c, &format!("Mode {n}"))?;
            Ok(Done::Nothing)
        }
        Op::RemoveMode(c, m) => ds.remove_mode(c, m).map(|_| Done::Nothing),
        Op::DefaultMode(c, m) => {
            local(ds, c)?.default_mode = m.clone();
            Ok(Done::Nothing)
        }
        Op::RenameMode(c, m, name) => {
            let mode = local(ds, c)?
                .modes
                .iter_mut()
                .find(|x| &x.id == m)
                .ok_or_else(|| Error::NotFound(m.clone()))?;
            mode.name = name.clone();
            Ok(Done::Nothing)
        }
        Op::Value(v, m, value) => ds.set_value(v, m, value.clone()).map(|_| Done::Nothing),
        Op::Rename(v, name) => ds.rename_variable(v, name).map(|_| Done::Nothing),
        Op::Describe(v, text) => {
            local_var(ds, v)?.description = text.clone();
            Ok(Done::Nothing)
        }
        Op::Hide(v, hidden) => {
            local_var(ds, v)?.hidden = *hidden;
            Ok(Done::Nothing)
        }
        Op::Delete(v, force) => ds.remove_variable(v, *force).map(|_| Done::Nothing),
    }
}

fn label(op: &Op) -> &'static str {
    match op {
        Op::Collection(_) => "Add collection",
        Op::Variable(..) => "Add variable",
        Op::Mode(_) => "Add mode",
        Op::RemoveMode(..) => "Delete mode",
        Op::DefaultMode(..) => "Set default mode",
        Op::RenameMode(..) => "Rename mode",
        Op::Value(..) => "Change variable",
        Op::Rename(..) => "Rename variable",
        Op::Describe(..) | Op::Hide(..) => "Edit variable",
        Op::Delete(..) => "Delete variable",
    }
}

fn apply(ed: &mut Editor, st: &mut UiState, op: Op) {
    match ed.edit_design(label(&op), |ds| run(ds, &op)) {
        Ok((done, _)) => {
            st.variables.note = None;
            match done {
                Done::Coll(id) => {
                    st.variables.coll = Some(id);
                    st.variables.group = None;
                    st.variables.selected = None;
                }
                Done::Var(id) => st.variables.selected = Some(id),
                Done::Nothing => {
                    if matches!(op, Op::Delete(..)) {
                        st.variables.selected = None;
                    }
                }
            }
        }
        Err(e) => st.variables.note = Some((e.to_string(), true)),
    }
}

// ------------------------------------------------------------------------------ widgets

/// Text field that commits on Enter or focus loss and never mid-typing. Returns the new text when
/// it was committed and differs; Escape reverts.
fn commit_text(
    ui: &mut Ui,
    salt: impl std::hash::Hash + std::fmt::Debug,
    current: &str,
    width: f32,
    multiline: bool,
    frame: bool,
) -> Option<String> {
    let id = ui.make_persistent_id(salt);
    let edit_id = id.with("edit");
    let mut text = if ui.memory(|m| m.has_focus(edit_id)) {
        ui.data(|d| d.get_temp::<String>(id))
            .unwrap_or_else(|| current.to_owned())
    } else {
        current.to_owned()
    };
    let edit = if multiline {
        egui::TextEdit::multiline(&mut text).desired_rows(3)
    } else {
        egui::TextEdit::singleline(&mut text)
    };
    let edit = if frame { edit } else { edit.frame(egui::Frame::NONE) };
    let r = ui.add(
        edit.id(edit_id)
            .desired_width(width)
            .margin(egui::Margin::symmetric(8, if frame { 7 } else { 4 })),
    );
    if r.has_focus() {
        ui.data_mut(|d| d.insert_temp(id, text.clone()));
        return None;
    }
    if !r.lost_focus() {
        return None;
    }
    ui.data_mut(|d| d.remove_temp::<String>(id));
    let escaped = ui.input(|i| i.key_pressed(egui::Key::Escape));
    (!escaped && text != current).then_some(text)
}

fn child(ui: &mut Ui, rect: Rect, f: impl FnOnce(&mut Ui)) {
    ui.scope_builder(UiBuilder::new().max_rect(rect), |ui| {
        ui.set_clip_rect(rect.intersect(ui.clip_rect()));
        f(ui);
    });
}

/// A left-column row: optional chevron, icon, label, right text, lock. Returns the row response
/// and whether the chevron was hit.
#[allow(clippy::too_many_arguments)]
fn nav_row(
    ui: &mut Ui,
    indent: f32,
    chevron: Option<bool>,
    icon: Icon,
    text: &str,
    right: &str,
    locked: bool,
    selected: bool,
) -> (egui::Response, bool) {
    let p = theme::pal(ui);
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::click());
    let hover = motion::hover(ui.ctx(), resp.id.with("nav"), &resp);
    if selected {
        ui.painter().rect_filled(rect, theme::RADIUS_MD, p.sidebar);
    } else if hover > 0.0 {
        ui.painter()
            .rect_filled(rect, theme::RADIUS_MD, p.hover.gamma_multiply(hover));
    }
    let mut x = rect.left() + 8.0 + indent;
    let mut chev_hit = false;
    let mid = rect.center().y;
    if let Some(open) = chevron {
        let r = Rect::from_center_size(egui::pos2(x + 6.0, mid), vec2(18.0, 24.0));
        let ic = if open { Icon::ChevronDown } else { Icon::ChevronRight };
        theme::paint_icon(
            ui.painter(),
            Rect::from_center_size(r.center(), vec2(12.0, 12.0)),
            ic,
            p.secondary,
        );
        chev_hit = resp.clicked()
            && ui
                .input(|i| i.pointer.interact_pos())
                .is_some_and(|pos| r.contains(pos));
    }
    x += 18.0;
    theme::paint_icon(
        ui.painter(),
        Rect::from_center_size(egui::pos2(x + 7.0, mid), vec2(14.0, 14.0)),
        icon,
        p.secondary,
    );
    x += 22.0;
    let font = if selected {
        theme::semibold(theme::BODY)
    } else {
        egui::FontId::proportional(theme::BODY)
    };
    ui.painter()
        .text(egui::pos2(x, mid), Align2::LEFT_CENTER, text, font, p.text);
    let mut rx = rect.right() - 8.0;
    if locked {
        theme::paint_icon(
            ui.painter(),
            Rect::from_center_size(egui::pos2(rx - 7.0, mid), vec2(14.0, 14.0)),
            Icon::Lock,
            p.secondary,
        );
        rx -= 20.0;
    }
    if !right.is_empty() {
        let font = egui::FontId::proportional(theme::SECONDARY);
        if locked {
            let w = ui
                .fonts_mut(|f| f.layout_no_wrap(right.to_owned(), font.clone(), p.text).size().x)
                + 14.0;
            let chip = Rect::from_min_max(
                egui::pos2(rx - w, mid - 10.0),
                egui::pos2(rx, mid + 10.0),
            );
            ui.painter().rect_filled(chip, 255, p.sidebar);
            ui.painter()
                .text(chip.center(), Align2::CENTER_CENTER, right, font, p.secondary);
        } else {
            ui.painter()
                .text(egui::pos2(rx, mid), Align2::RIGHT_CENTER, right, font, p.tertiary);
        }
    }
    (resp, chev_hit)
}

// ------------------------------------------------------------------------------ the modal

pub fn show(ctx: &egui::Context, ed: &mut Editor, st: &mut UiState) {
    let t = motion::progress(
        ctx,
        "variables-open",
        st.variables.open,
        motion::FAST,
        motion::ease_out_cubic,
    );
    if !st.variables.open && t <= 0.0 {
        return;
    }
    let live = st.variables.open;
    let ds: std::borrow::Cow<DesignSystem> = if ed.design.collections.is_empty() {
        std::borrow::Cow::Owned(variables::migrate_from_document(ed.page_doc(0)))
    } else {
        std::borrow::Cow::Borrowed(&ed.design)
    };
    let colls = ordered(&ds);
    let coll_id = st
        .variables
        .coll
        .clone()
        .filter(|id| colls.iter().any(|c| &c.id == id))
        .or_else(|| colls.first().map(|c| c.id.clone()));
    st.variables.coll = coll_id.clone();
    let cur = coll_id.as_deref().and_then(|id| ds.collection(id));
    if st
        .variables
        .selected
        .as_deref()
        .is_some_and(|id| cur.is_none_or(|c| !c.variables.iter().any(|v| v.id == id)))
    {
        st.variables.selected = None;
    }
    let p = theme::pal_from_ctx(ctx);
    let screen = ctx.content_rect();
    let w = (MODAL_W).min(screen.width() - 2.0 * SCREEN_MARGIN) - 2.0 * MODAL_PAD;
    let h = (MODAL_H).min(screen.height() - 2.0 * SCREEN_MARGIN) - 2.0 * MODAL_PAD;
    let mut ops: Vec<Op> = Vec::new();
    let mut copy: Option<(&str, String)> = None;
    let frame = egui::Frame::new()
        .fill(p.sidebar)
        .corner_radius(24.0)
        .inner_margin(MODAL_PAD);
    let modal = egui::Modal::new(egui::Id::new("variables-modal"))
        .frame(frame)
        .show(ctx, |ui| {
            ui.multiply_opacity(t);
            if !live {
                ui.disable();
            }
            let area = Rect::from_min_size(ui.cursor().min, vec2(w, h));
            ui.allocate_rect(area, Sense::hover());
            theme::figma_fields(ui);
            let header = Rect::from_min_size(area.min, vec2(w, HEADER_H));
            let footer = Rect::from_min_max(egui::pos2(area.left(), area.bottom() - FOOTER_H), area.max);
            let body = Rect::from_min_max(
                egui::pos2(area.left(), header.bottom()),
                egui::pos2(area.right(), footer.top()),
            );
            let left = Rect::from_min_size(body.min, vec2(LEFT_W, body.height()));
            let right = Rect::from_min_max(egui::pos2(body.right() - RIGHT_W, body.top()), body.max);
            let main = Rect::from_min_max(
                egui::pos2(left.right() + GAP, body.top()),
                egui::pos2(right.left() - GAP, body.bottom()),
            );
            for r in [left, main, right] {
                ui.painter().rect_filled(r, theme::RADIUS_PANEL, p.field);
            }
            child(ui, header, |ui| title(ui, st));
            child(ui, left.shrink(12.0), |ui| {
                collections_column(ui, &ds, &colls, st, ed, &mut ops);
            });
            child(ui, main.shrink(12.0), |ui| {
                main_column(ui, &ds, cur, st, ed, &mut ops);
            });
            child(ui, right.shrink(12.0), |ui| {
                details_column(ui, &ds, cur, st, ed, &mut ops);
            });
            child(ui, footer, |ui| {
                footer_row(ui, cur, st, ed, &ds, &mut copy);
            });
        });
    if let Some((what, text)) = copy {
        ctx.copy_text(text);
        st.variables.note = Some((format!("Copied {what} to the clipboard"), false));
    }
    for op in ops {
        apply(ed, st, op);
    }
    confirm_dialog(ctx, ed, st);
    if live && st.variables.confirm.is_none() && modal.should_close() {
        st.variables.open = false;
    }
}

fn title(ui: &mut Ui, st: &mut UiState) {
    let p = theme::pal(ui);
    ui.horizontal_centered(|ui| {
        ui.add_space(4.0);
        let (r, _) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
        theme::paint_icon(ui.painter(), r, Icon::Grid, theme::accent());
        ui.label(RichText::new("Variables").font(theme::semibold(20.0)).color(p.text));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if kit::close_button(ui).clicked() {
                st.variables.open = false;
            }
        });
    });
}

fn collections_column(
    ui: &mut Ui,
    ds: &DesignSystem,
    colls: &[&Collection],
    st: &mut UiState,
    ed: &Editor,
    ops: &mut Vec<Op>,
) {
    let p = theme::pal(ui);
    kit::section_header(ui, "Collections", None);
    ui.add_space(4.0);
    ui.spacing_mut().item_spacing.y = 2.0;
    let new_h = 36.0;
    let list_h = (ui.available_height() - new_h).max(40.0);
    egui::ScrollArea::vertical()
        .id_salt("var-collections")
        .max_height(list_h)
        .auto_shrink(false)
        .show(ui, |ui| {
            let mut any_lib = false;
            for c in colls {
                let selected = st.variables.coll.as_deref() == Some(c.id.as_str());
                let lib = c.library.as_ref();
                any_lib |= lib.is_some();
                let (icon, right) = match lib {
                    Some(l) => {
                        let v = ds.libraries.iter().find(|x| &x.library_id == l);
                        (Icon::Library, v.map_or(String::new(), |v| format!("v{}", v.version)))
                    }
                    None => (Icon::Grid, c.variables.len().to_string()),
                };
                let (resp, _) = nav_row(
                    ui,
                    0.0,
                    None,
                    icon,
                    &c.name,
                    &right,
                    lib.is_some(),
                    selected && st.variables.group.is_none(),
                );
                if resp.clicked() {
                    st.variables.coll = Some(c.id.clone());
                    st.variables.group = None;
                }
                if !selected {
                    continue;
                }
                let gs = variables::groups(c);
                for g in &gs {
                    let parts: Vec<&str> = g.split('/').collect();
                    let visible = (1..parts.len())
                        .all(|n| st.variables.tree_open.contains(&parts[..n].join("/")));
                    if !visible {
                        continue;
                    }
                    let has_kids = gs.iter().any(|o| o.starts_with(&format!("{g}/")));
                    let open = st.variables.tree_open.contains(g);
                    let on = st.variables.group.as_deref() == Some(g.as_str());
                    let (resp, chev) = nav_row(
                        ui,
                        (parts.len() - 1) as f32 * TREE_INDENT + 8.0,
                        has_kids.then_some(open),
                        Icon::Group,
                        parts.last().copied().unwrap_or(g),
                        &group_count(c, g).to_string(),
                        false,
                        on,
                    );
                    if chev {
                        if open {
                            st.variables.tree_open.remove(g);
                        } else {
                            st.variables.tree_open.insert(g.clone());
                        }
                    } else if resp.clicked() {
                        st.variables.group = Some(g.clone());
                    }
                }
            }
            if any_lib {
                ui.add_space(8.0);
                ui.label(
                    RichText::new("Library collections are read-only. You can alias and bind them.")
                        .size(theme::SECONDARY)
                        .color(p.secondary),
                );
            }
        });
    let _ = ed;
    ui.add_space(4.0);
    match &mut st.variables.new_coll {
        Some(name) => {
            let r = ui.add(
                egui::TextEdit::singleline(name)
                    .hint_text("Collection name")
                    .desired_width(f32::INFINITY)
                    .margin(theme::INPUT_MARGIN),
            );
            r.request_focus();
            let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
            let esc = ui.input(|i| i.key_pressed(egui::Key::Escape));
            if enter && !name.trim().is_empty() {
                ops.push(Op::Collection(name.trim().to_owned()));
                st.variables.new_coll = None;
            } else if esc || r.lost_focus() {
                st.variables.new_coll = None;
            }
        }
        None => {
            let (rect, resp) =
                ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::click());
            let hover = motion::hover(ui.ctx(), resp.id.with("newc"), &resp);
            ui.painter()
                .rect_filled(rect, theme::RADIUS_MD, p.hover.gamma_multiply(hover));
            theme::paint_icon(
                ui.painter(),
                Rect::from_center_size(egui::pos2(rect.left() + 16.0, rect.center().y), vec2(14.0, 14.0)),
                Icon::Plus,
                theme::accent(),
            );
            ui.painter().text(
                egui::pos2(rect.left() + 32.0, rect.center().y),
                Align2::LEFT_CENTER,
                "New collection",
                theme::semibold(theme::BODY),
                theme::accent(),
            );
            if resp.clicked() {
                st.variables.new_coll = Some(String::new());
            }
        }
    }
}

fn unique_name(c: &Collection, group: Option<&str>, kind: Kind) -> String {
    let base = format!("new-{}", kind_label(kind).to_lowercase());
    let prefix = group.map_or(String::new(), |g| format!("{g}/"));
    (1..)
        .map(|i| {
            if i == 1 {
                format!("{prefix}{base}")
            } else {
                format!("{prefix}{base}-{i}")
            }
        })
        .find(|n| !c.variables.iter().any(|v| &v.name == n))
        .unwrap()
}

fn main_column(
    ui: &mut Ui,
    ds: &DesignSystem,
    cur: Option<&Collection>,
    st: &mut UiState,
    ed: &Editor,
    ops: &mut Vec<Op>,
) {
    let Some(c) = cur else {
        kit::empty_state(
            ui,
            Icon::Grid,
            theme::pal(ui).secondary,
            "No collections yet",
            "Create a collection to add variables.",
            |_| {},
        );
        return;
    };
    let ro = c.library.is_some();
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut st.variables.search)
                .hint_text("Search variables")
                .desired_width(220.0)
                .margin(theme::INPUT_MARGIN),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.add_enabled_ui(!ro, |ui| {
                if kit::button(ui, kit::ButtonKind::Secondary, Some(Icon::Plus), "Mode").clicked() {
                    ops.push(Op::Mode(c.id.clone()));
                }
                let r = kit::button(ui, kit::ButtonKind::Primary, Some(Icon::Plus), "Variable");
                egui::Popup::menu(&r).show(|ui| {
                    for k in [Kind::Color, Kind::Number, Kind::String, Kind::Boolean] {
                        if super::icon_menu_item(ui, kind_icon(k), kind_label(k)).clicked() {
                            let name = unique_name(c, st.variables.group.as_deref(), k);
                            ops.push(Op::Variable(c.id.clone(), name, k));
                            ui.close();
                        }
                    }
                });
            });
        });
    });
    ui.add_space(6.0);
    let n = c.modes.len().max(1) as f32;
    let avail = ui.available_width();
    let mode_w = ((avail - NAME_W) / n).max(MODE_MIN_W);
    let total_w = NAME_W + mode_w * n;
    egui::ScrollArea::horizontal()
        .id_salt("var-table-h")
        .auto_shrink(false)
        .show(ui, |ui| {
            ui.set_width(total_w);
            ui.spacing_mut().item_spacing.y = 0.0;
            table_header(ui, c, mode_w, st, ops);
            egui::ScrollArea::vertical()
                .id_salt("var-table-v")
                .auto_shrink(false)
                .show(ui, |ui| {
                    ui.set_width(total_w);
                    ui.spacing_mut().item_spacing.y = 0.0;
                    let list = rows(
                        c,
                        st.variables.group.as_deref(),
                        &st.variables.search,
                        &st.variables.folded,
                    );
                    if list.is_empty() {
                        ui.add_space(24.0);
                        ui.vertical_centered(|ui| {
                            ui.label(
                                RichText::new(if c.variables.is_empty() {
                                    "No variables yet. Use + Variable."
                                } else {
                                    "No variables match."
                                })
                                .color(theme::pal(ui).secondary),
                            );
                        });
                    }
                    for row in list {
                        match row {
                            Row::Group(g) => group_row(ui, g, total_w, st),
                            Row::Var(v) => var_row(ui, ds, c, v, mode_w, total_w, st, ed, ops),
                        }
                    }
                });
        });
}

fn table_header(ui: &mut Ui, c: &Collection, mode_w: f32, st: &mut UiState, ops: &mut Vec<Op>) {
    let p = theme::pal(ui);
    let ro = c.library.is_some();
    let (rect, _) = ui.allocate_exact_size(vec2(NAME_W + mode_w * c.modes.len() as f32, 32.0), Sense::hover());
    let font = theme::semibold(theme::SECONDARY + 1.0);
    ui.painter().text(
        egui::pos2(rect.left() + 10.0, rect.center().y),
        Align2::LEFT_CENTER,
        "Name",
        font,
        p.secondary,
    );
    for (i, m) in c.modes.iter().enumerate() {
        let cell = Rect::from_min_size(
            egui::pos2(rect.left() + NAME_W + i as f32 * mode_w, rect.top()),
            vec2(mode_w, rect.height()),
        );
        child(ui, cell, |ui| {
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                ui.set_height(cell.height());
                ui.spacing_mut().item_spacing.x = 6.0;
                let editing = st
                    .variables
                    .mode_edit
                    .as_ref()
                    .is_some_and(|(cc, mm, _)| cc == &c.id && mm == &m.id);
                if editing {
                    let draft = &mut st.variables.mode_edit.as_mut().unwrap().2;
                    let r = ui.add(
                        egui::TextEdit::singleline(draft)
                            .desired_width(90.0)
                            .margin(egui::Margin::symmetric(6, 3)),
                    );
                    r.request_focus();
                    let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                    let esc = ui.input(|i| i.key_pressed(egui::Key::Escape));
                    if enter && !draft.trim().is_empty() {
                        ops.push(Op::RenameMode(c.id.clone(), m.id.clone(), draft.trim().to_owned()));
                        st.variables.mode_edit = None;
                    } else if esc || r.lost_focus() {
                        st.variables.mode_edit = None;
                    }
                } else {
                    ui.label(
                        RichText::new(&m.name)
                            .font(theme::semibold(theme::SECONDARY + 1.0))
                            .color(p.text),
                    );
                }
                if m.id == c.default_mode {
                    kit::chip(ui, "Default", false);
                }
                if ro {
                    return;
                }
                let r = theme::icon_button(ui, Icon::More, 22.0, false).on_hover_text("Mode options");
                egui::Popup::menu(&r).show(|ui| {
                    if super::icon_menu_item(ui, Icon::Pen, "Rename").clicked() {
                        st.variables.mode_edit = Some((c.id.clone(), m.id.clone(), m.name.clone()));
                        ui.close();
                    }
                    if m.id != c.default_mode
                        && super::icon_menu_item(ui, Icon::Star, "Set as default").clicked()
                    {
                        ops.push(Op::DefaultMode(c.id.clone(), m.id.clone()));
                        ui.close();
                    }
                    if c.modes.len() > 1
                        && super::icon_menu_item(ui, Icon::Trash, "Delete mode").clicked()
                    {
                        st.variables.confirm = Some(Confirm::DeleteMode {
                            coll: c.id.clone(),
                            mode: m.id.clone(),
                            name: m.name.clone(),
                        });
                        ui.close();
                    }
                });
            });
        });
    }
}

fn group_row(ui: &mut Ui, g: &str, total_w: f32, st: &mut UiState) {
    let p = theme::pal(ui);
    let (rect, resp) = ui.allocate_exact_size(vec2(total_w, 30.0), Sense::click());
    let folded = st.variables.folded.contains(g);
    theme::paint_icon(
        ui.painter(),
        Rect::from_center_size(egui::pos2(rect.left() + 16.0, rect.center().y), vec2(12.0, 12.0)),
        if folded { Icon::ChevronRight } else { Icon::ChevronDown },
        p.secondary,
    );
    ui.painter().text(
        egui::pos2(rect.left() + 30.0, rect.center().y),
        Align2::LEFT_CENTER,
        g,
        theme::semibold(theme::SECONDARY + 1.0),
        p.secondary,
    );
    if resp.clicked() && !st.variables.folded.remove(g) {
        st.variables.folded.insert(g.to_owned());
    }
}

#[allow(clippy::too_many_arguments)]
fn var_row(
    ui: &mut Ui,
    ds: &DesignSystem,
    c: &Collection,
    v: &Variable,
    mode_w: f32,
    total_w: f32,
    st: &mut UiState,
    _ed: &Editor,
    ops: &mut Vec<Op>,
) {
    let p = theme::pal(ui);
    let ro = c.library.is_some();
    let (rect, resp) = ui.allocate_exact_size(vec2(total_w, ROW_H), Sense::click());
    let selected = st.variables.selected.as_deref() == Some(v.id.as_str());
    let hot = ui.rect_contains_pointer(rect);
    if selected {
        ui.painter().rect_filled(rect, theme::RADIUS_MD, p.sidebar);
    } else if hot {
        ui.painter()
            .rect_filled(rect, theme::RADIUS_MD, p.hover);
    }
    if resp.clicked() || (hot && ui.input(|i| i.pointer.primary_pressed())) {
        st.variables.selected = Some(v.id.clone());
    }
    theme::paint_icon(
        ui.painter(),
        Rect::from_center_size(egui::pos2(rect.left() + 20.0, rect.center().y), vec2(14.0, 14.0)),
        kind_icon(v.kind),
        p.secondary,
    );
    let leaf = v.name.rsplit('/').next().unwrap_or(&v.name);
    ui.painter().text(
        egui::pos2(rect.left() + 38.0, rect.center().y),
        Align2::LEFT_CENTER,
        leaf,
        egui::FontId::proportional(theme::BODY),
        p.text,
    );
    for (i, m) in c.modes.iter().enumerate() {
        let cell = Rect::from_min_size(
            egui::pos2(rect.left() + NAME_W + i as f32 * mode_w, rect.top()),
            vec2(mode_w - 8.0, ROW_H),
        );
        child(ui, cell, |ui| {
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                ui.set_height(ROW_H);
                ui.spacing_mut().item_spacing.x = 6.0;
                value_cell(ui, ds, v, &m.id, cell.width(), ro, hot || selected, ops);
            });
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn value_cell(
    ui: &mut Ui,
    ds: &DesignSystem,
    v: &Variable,
    mode: &str,
    width: f32,
    ro: bool,
    hot: bool,
    ops: &mut Vec<Op>,
) {
    let p = theme::pal(ui);
    let value = v.values.get(mode).cloned().unwrap_or_else(|| v.kind.default_value());
    let salt = (&v.id, mode);
    let set = |ops: &mut Vec<Op>, val: Value| ops.push(Op::Value(v.id.clone(), mode.to_owned(), val));
    let mut picker_anchor: Option<egui::Response> = None;
    match &value {
        Value::Alias(target) => {
            let name = ds.find(target).map_or("missing", |(_, t)| t.name.as_str());
            let text = format!("→ {name}");
            let font = theme::semibold(theme::SECONDARY + 1.0);
            let tw = ui.fonts_mut(|f| f.layout_no_wrap(text.clone(), font.clone(), p.text).size().x);
            let (rect, resp) = ui.allocate_exact_size(vec2((tw + 20.0).min(width), 24.0), Sense::click());
            let a = theme::accent();
            ui.painter().rect_filled(rect, 255, a.gamma_multiply(0.18));
            ui.painter().with_clip_rect(rect).text(
                egui::pos2(rect.left() + 10.0, rect.center().y),
                Align2::LEFT_CENTER,
                text,
                font,
                a,
            );
            picker_anchor = Some(resp.on_hover_text("Change alias"));
        }
        Value::Color(col) => {
            let (r, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
            ui.painter().rect_filled(r, 4.0, c32(*col));
            ui.painter().rect_stroke(r, 4.0, egui::Stroke::new(1.0, p.hover), egui::StrokeKind::Inside);
            ui.add_enabled_ui(!ro, |ui| {
                if let Some(t) = commit_text(ui, salt, &col.to_hex().to_uppercase(), 84.0, false, false)
                    && let Some(new) = Color::from_hex(t.trim())
                {
                    set(ops, Value::Color(new));
                }
            });
        }
        Value::Number(n) => {
            ui.add_enabled_ui(!ro, |ui| {
                if let Some(t) = commit_text(ui, salt, &fmt_num(*n), 56.0, false, false)
                    && let Ok(new) = t.trim().parse::<f64>()
                    && new.is_finite()
                {
                    set(ops, Value::Number(new));
                }
            });
            ui.label(RichText::new("px").size(theme::SECONDARY).color(p.tertiary));
        }
        Value::String(s) => {
            ui.add_enabled_ui(!ro, |ui| {
                if let Some(t) = commit_text(ui, salt, s, (width - 36.0).max(40.0), false, false) {
                    set(ops, Value::String(t));
                }
            });
        }
        Value::Boolean(b) => {
            let mut on = *b;
            ui.add_enabled_ui(!ro, |ui| {
                if kit::toggle(ui, &mut on).changed() {
                    set(ops, Value::Boolean(on));
                }
            });
        }
    }
    if matches!(value, Value::Alias(_)) {
        // The chip itself opens the picker.
    } else if hot {
        let r = theme::icon_button(ui, Icon::ArrowRight, 20.0, false).on_hover_text("Alias to a variable");
        picker_anchor = Some(r);
    }
    let Some(anchor) = picker_anchor else {
        return;
    };
    if ro {
        return;
    }
    egui::Popup::menu(&anchor)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .width(280.0)
        .show(|ui| alias_picker(ui, ds, v, mode, &value, ops));
}

fn alias_picker(
    ui: &mut Ui,
    ds: &DesignSystem,
    v: &Variable,
    mode: &str,
    value: &Value,
    ops: &mut Vec<Op>,
) {
    let p = theme::pal(ui);
    let id = ui.make_persistent_id(("alias-filter", &v.id, mode));
    let mut filter = ui.data(|d| d.get_temp::<String>(id)).unwrap_or_default();
    ui.add(
        egui::TextEdit::singleline(&mut filter)
            .hint_text("Search variables")
            .desired_width(f32::INFINITY)
            .margin(theme::INPUT_MARGIN),
    );
    ui.data_mut(|d| d.insert_temp(id, filter.clone()));
    let needle = filter.trim().to_lowercase();
    ui.add_space(4.0);
    if matches!(value, Value::Alias(_)) {
        let lit = ds
            .resolve(&v.id, &variables::PageModes::from([]))
            .unwrap_or_else(|_| v.kind.default_value());
        if super::icon_menu_item(ui, Icon::Pen, "Use a literal value").clicked() {
            ops.push(Op::Value(v.id.clone(), mode.to_owned(), lit));
            ui.close();
        }
    }
    egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
        let mut shown = 0;
        for c in &ds.collections {
            for t in c.variables.iter().filter(|t| t.kind == v.kind && t.id != v.id) {
                if !needle.is_empty() && !t.name.to_lowercase().contains(&needle) {
                    continue;
                }
                if shown >= PICKER_MAX_ROWS {
                    return;
                }
                shown += 1;
                let (rect, r) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::click());
                let h = motion::hover(ui.ctx(), r.id.with("alias"), &r);
                ui.painter()
                    .rect_filled(rect, theme::RADIUS_MD, p.hover.gamma_multiply(h));
                ui.painter().text(
                    egui::pos2(rect.left() + 10.0, rect.center().y),
                    Align2::LEFT_CENTER,
                    &t.name,
                    egui::FontId::proportional(theme::BODY),
                    p.text,
                );
                ui.painter().text(
                    egui::pos2(rect.right() - 10.0, rect.center().y),
                    Align2::RIGHT_CENTER,
                    &c.name,
                    egui::FontId::proportional(theme::SECONDARY),
                    p.tertiary,
                );
                if r.clicked() {
                    ops.push(Op::Value(v.id.clone(), mode.to_owned(), Value::Alias(t.id.clone())));
                    ui.close();
                }
            }
        }
        if shown == 0 {
            ui.label(RichText::new("No variables of this type").color(p.secondary));
        }
    });
}

fn details_column(
    ui: &mut Ui,
    ds: &DesignSystem,
    cur: Option<&Collection>,
    st: &mut UiState,
    ed: &Editor,
    ops: &mut Vec<Op>,
) {
    let p = theme::pal(ui);
    let var = st
        .variables
        .selected
        .as_deref()
        .and_then(|id| cur.and_then(|c| c.variables.iter().find(|v| v.id == id)));
    let (Some(c), Some(v)) = (cur, var) else {
        ui.add_space(24.0);
        ui.vertical_centered(|ui| {
            ui.label(RichText::new("Select a variable to see its details.").color(p.secondary));
        });
        return;
    };
    let ro = c.library.is_some();
    let css = variables::css_name(v);
    let label = |ui: &mut Ui, t: &str| {
        ui.add_space(8.0);
        ui.label(
            RichText::new(t)
                .font(theme::semibold(theme::SECONDARY + 1.0))
                .color(p.secondary),
        );
    };
    ui.spacing_mut().item_spacing.y = 4.0;
    ui.horizontal(|ui| {
        ui.label(RichText::new("Variable").font(theme::semibold(theme::SECONDARY + 1.0)).color(p.secondary));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            kit::chip_ink(ui, kind_label(v.kind), Some(theme::accent()));
        });
    });
    ui.add_enabled_ui(!ro, |ui| {
        label(ui, "Name");
        if let Some(n) = commit_text(ui, ("var-name", &v.id), &v.name, ui.available_width(), false, true) {
            ops.push(Op::Rename(v.id.clone(), n.trim().to_owned()));
        }
        label(ui, "Description");
        if let Some(t) = commit_text(ui, ("var-desc", &v.id), &v.description, ui.available_width(), true, true) {
            ops.push(Op::Describe(v.id.clone(), t));
        }
    });
    label(ui, "CSS name");
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width() - 64.0, theme::FIELD_H), Sense::hover());
        ui.painter().rect_filled(rect, theme::RADIUS_MD, p.sidebar.gamma_multiply(0.8));
        ui.painter().text(
            egui::pos2(rect.left() + 10.0, rect.center().y),
            Align2::LEFT_CENTER,
            format!("--{css}"),
            egui::FontId::monospace(theme::BODY),
            theme::accent(),
        );
        if theme::small_button(ui, "Copy").on_hover_text("Copy the CSS name").clicked() {
            ui.ctx().copy_text(format!("--{css}"));
        }
    });
    ui.add_space(8.0);
    ui.add_enabled_ui(!ro, |ui| {
        let mut hidden = v.hidden;
        ui.horizontal(|ui| {
            ui.label(RichText::new("Hide from publishing").font(theme::semibold(theme::BODY)));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if kit::toggle(ui, &mut hidden).changed() {
                    ops.push(Op::Hide(v.id.clone(), hidden));
                }
            });
        });
    });
    ui.label(
        RichText::new("Hidden variables stay in this file and are left out of the library.")
            .size(theme::SECONDARY)
            .color(p.secondary),
    );
    ui.add_space(8.0);
    if st.variables.used.0 != ed.revision {
        st.variables.used = (ed.revision, Default::default());
    }
    let used = *st.variables.used.1.entry(css.clone()).or_insert_with(|| used_count(ed, &css));
    egui::Frame::new()
        .fill(p.sidebar)
        .corner_radius(theme::RADIUS_LG)
        .inner_margin(egui::Margin::symmetric(12, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width() - 24.0);
            ui.label(
                RichText::new(format!("Used {used} time{}", if used == 1 { "" } else { "s" }))
                    .font(theme::semibold(theme::BODY)),
            );
        });
    let bottom = ui.available_height() - 40.0;
    if bottom > 0.0 {
        ui.add_space(bottom);
    }
    ui.add_enabled_ui(!ro, |ui| {
        if kit::button_wide(ui, kit::ButtonKind::Danger, Some(Icon::Trash), "Delete variable").clicked() {
            st.variables.confirm = Some(Confirm::DeleteVar {
                id: v.id.clone(),
                name: v.name.clone(),
                deps: ds.dependents(&v.id),
            });
        }
    });
}

fn footer_row(
    ui: &mut Ui,
    cur: Option<&Collection>,
    st: &UiState,
    ed: &Editor,
    ds: &DesignSystem,
    copy: &mut Option<(&'static str, String)>,
) {
    let p = theme::pal(ui);
    ui.horizontal_centered(|ui| {
        ui.add_space(4.0);
        match &st.variables.note {
            Some((msg, err)) => {
                ui.label(RichText::new(msg).color(if *err { p.danger } else { p.success }));
            }
            None => {
                let (nv, nm) = cur.map_or((0, 0), |c| (c.variables.len(), c.modes.len()));
                ui.label(RichText::new(format!(
                        "{nv} variable{} · {nm} mode{}",
                        if nv == 1 { "" } else { "s" },
                        if nm == 1 { "" } else { "s" }
                    )).color(p.secondary));
            }
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let doc = ed.page_doc(ed.active_page);
            if kit::button(ui, kit::ButtonKind::Secondary, None, "Copy Tailwind").clicked() {
                *copy = Some(("Tailwind", codegen::export_variables_tailwind(doc)));
            }
            if kit::button(ui, kit::ButtonKind::Secondary, None, "Copy JSON").clicked() {
                let json = codegen::export_dtcg_system(ds, &ed.pages[ed.active_page].modes);
                *copy = Some(("JSON", serde_json::to_string_pretty(&json).unwrap_or_default()));
            }
            if kit::button(ui, kit::ButtonKind::Secondary, None, "Copy CSS").clicked() {
                *copy = Some(("CSS", codegen::export_variables_css(doc)));
            }
        });
    });
}

fn confirm_dialog(ctx: &egui::Context, ed: &mut Editor, st: &mut UiState) {
    let Some(cf) = &st.variables.confirm else {
        return;
    };
    let (title, msg, button) = match cf {
        Confirm::DeleteVar { name, deps, .. } if deps.is_empty() => (
            format!("Delete {name}?"),
            "Fills and fields that use it keep their current value.".to_owned(),
            "Delete",
        ),
        Confirm::DeleteVar { name, deps, .. } => (
            format!("Delete {name}?"),
            format!(
                "{} alias it. They keep the value it has now. Fills and fields that use it keep their current value.",
                deps.join(", ")
            ),
            "Delete anyway",
        ),
        Confirm::DeleteMode { name, .. } => (
            format!("Delete mode {name}?"),
            "Its values are removed. Pages using it fall back to the default mode.".to_owned(),
            "Delete",
        ),
    };
    let Some(answer) = super::confirm::dialog(ctx, "variables-confirm", &title, &msg, button) else {
        return;
    };
    let cf = st.variables.confirm.take();
    if !answer {
        return;
    }
    let op = match cf {
        Some(Confirm::DeleteVar { id, deps, .. }) => Op::Delete(id, !deps.is_empty()),
        Some(Confirm::DeleteMode { coll, mode, .. }) => Op::RemoveMode(coll, mode),
        None => return,
    };
    apply(ed, st, op);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::Rect as GRect;
    use crate::model::{Node, NodeId, NodeKind, Paint};

    fn coll() -> Collection {
        let mut ds = DesignSystem::default();
        let c = ds.add_collection("C");
        for n in ["color/brand/primary", "color/bg", "space/sm", "solo", "color/brand/accent"] {
            ds.add_variable(&c, n, Kind::Color).unwrap();
        }
        ds.collections.remove(0)
    }

    fn names(rows: &[Row]) -> Vec<String> {
        rows.iter()
            .map(|r| match r {
                Row::Group(g) => format!("[{g}]"),
                Row::Var(v) => v.name.clone(),
            })
            .collect()
    }

    #[test]
    fn rows_group_filter_search_fold() {
        let c = coll();
        let none = BTreeSet::new();
        let all = names(&rows(&c, None, "", &none));
        assert_eq!(
            all,
            [
                "solo",
                "[color]",
                "color/bg",
                "[color/brand]",
                "color/brand/primary",
                "color/brand/accent",
                "[space]",
                "space/sm"
            ]
        );
        let brand = names(&rows(&c, Some("color"), "", &none));
        assert_eq!(brand.len(), 5, "{brand:?}");
        assert!(!brand.contains(&"space/sm".to_string()));
        assert_eq!(names(&rows(&c, None, "ACC", &none)), ["[color/brand]", "color/brand/accent"]);
        let folded = BTreeSet::from(["color/brand".to_string()]);
        let f = names(&rows(&c, Some("color"), "", &folded));
        assert_eq!(f, ["[color]", "color/bg", "[color/brand]"]);
        assert_eq!(group_count(&c, "color"), 3);
    }

    #[test]
    fn counts_fills_and_bindings() {
        let mut doc = Document::default();
        let mut n = Node::new(NodeId(1), NodeKind::Frame { clip: false }, GRect::new(0.0, 0.0, 10.0, 10.0));
        n.style.fills = vec![Paint::solid_var(Color::BLACK, "brand"), Paint::solid_var(Color::BLACK, "other")];
        n.bindings.insert("gap".into(), "brand".into());
        doc.nodes.insert(NodeId(1), n);
        assert_eq!(used_in(&doc, "brand"), 2);
        assert_eq!(used_in(&doc, "none"), 0);
    }

    #[test]
    fn run_surfaces_errors() {
        let mut ds = DesignSystem::default();
        let c = ds.add_collection("C");
        let a = match run(&mut ds, &Op::Variable(c.clone(), "a".into(), Kind::Color)).unwrap() {
            Done::Var(id) => id,
            _ => panic!(),
        };
        assert!(run(&mut ds, &Op::Variable(c.clone(), "a".into(), Kind::Color)).is_err());
        assert!(matches!(run(&mut ds, &Op::Delete("zz".into(), false)), Err(Error::NotFound(_))));
        assert!(run(&mut ds, &Op::Hide(a, true)).is_ok());
    }
}
