//! Annotations UI: the Annotate tool's New annotation popover, the Annotations side view
//! (template picker, category filter, notes grouped by frame), the Customize templates modal,
//! "Annotate with AI" and the inspector section.
//!
//! All edits go through the `Editor` annotation / template ops, so each one is undoable. Hide
//! states (`hidden`) are view state only.

use crate::theme::Surface as _;
use super::{UiState, kit};
use crate::editor::Editor;
use crate::editor::shortcuts::Action;
use crate::geom::V2;
use crate::model::annotation::{
    self, Anchor, Annotation, AnnotationCategory, AnnotationTemplate, Display, Side,
    TemplateCategory, builtin_templates, category_by_key,
};
use crate::model::document::Document;
use crate::model::{Color, NodeId};
use crate::theme::{self, Icon};
use egui::{Align, Color32, Layout, RichText, Sense, Ui, vec2};
use std::collections::{HashMap, HashSet};

/// Longest label the popover accepts.
const LABEL_MAX: usize = 40;
const POPOVER_W: f32 = 340.0;
const MODAL_W: f32 = 620.0;
const BADGE: f32 = 22.0;
/// Group keys that are not frames.
const KEY_PAGE: u64 = u64::MAX - 1;
const KEY_DETACHED: u64 = u64::MAX;
const CATEGORY_ICON: &str = "tag";
const NEW_CATEGORY_COLOR: Color = Color::rgb(0x7c, 0x5c, 0xff);

// ---------------------------------------------------------------------------- pure helpers

/// One block of the side list: a top-level frame, the page's loose notes, or the detached ones.
#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    pub key: u64,
    pub frame: Option<NodeId>,
    pub title: String,
    pub detached: bool,
    /// Annotation ids in numbering order.
    pub ids: Vec<u64>,
}

/// Annotations grouped by top-level frame (reading order), then loose notes, then detached
/// ones. Categories in `hidden` are left out; empty groups are dropped.
pub fn group_annotations(doc: &Document, hidden: &HashSet<String>) -> Vec<Group> {
    group_with(doc, hidden, &annotation::numbers(doc))
}

/// Id -> position in `doc.annotations`, so rows look their note up in O(1).
pub fn index_by_id(doc: &Document) -> HashMap<u64, usize> {
    doc.annotations.iter().enumerate().map(|(i, a)| (a.id, i)).collect()
}

fn group_with(doc: &Document, hidden: &HashSet<String>, nums: &super::annotations_canvas::Numbers) -> Vec<Group> {
    let mut frames: Vec<(NodeId, V2)> = doc
        .annotations
        .iter()
        .filter(|a| !a.detached)
        .filter_map(|a| nums.get(&a.id)?.0)
        .filter_map(|f| Some((f, doc.world_rect(f)?.min())))
        .collect();
    frames.sort_by(|a, b| (a.1.y, a.1.x, a.0.0).partial_cmp(&(b.1.y, b.1.x, b.0.0)).unwrap_or(std::cmp::Ordering::Equal));
    frames.dedup_by_key(|f| f.0);
    let mut out: Vec<Group> = frames
        .iter()
        .map(|(f, _)| Group {
            key: f.0,
            frame: Some(*f),
            title: doc.get(*f).map(|n| n.name.clone()).unwrap_or_default(),
            detached: false,
            ids: Vec::new(),
        })
        .collect();
    out.push(Group { key: KEY_PAGE, frame: None, title: "Page".into(), detached: false, ids: Vec::new() });
    out.push(Group { key: KEY_DETACHED, frame: None, title: "Detached".into(), detached: true, ids: Vec::new() });
    for a in doc.annotations.iter().filter(|a| !hidden.contains(&a.category)) {
        let frame = nums.get(&a.id).and_then(|n| n.0);
        let g = if a.detached {
            out.iter_mut().find(|g| g.key == KEY_DETACHED)
        } else {
            out.iter_mut().find(|g| g.frame == frame && g.key != KEY_DETACHED)
        };
        if let Some(g) = g {
            g.ids.push(a.id);
        }
    }
    for g in &mut out {
        g.ids.sort_by_key(|id| nums.get(id).map_or(0, |n| n.1));
    }
    out.retain(|g| !g.ids.is_empty());
    out
}

/// How many annotations each category id has, in `cats` order.
pub fn category_counts(doc: &Document, cats: &[AnnotationCategory]) -> Vec<(String, usize)> {
    cats.iter()
        .map(|c| (c.id.clone(), doc.annotations.iter().filter(|a| a.category == c.id).count()))
        .collect()
}

/// A note's body from the template's labelled fields and the free text: `Name: value` lines
/// (blank values left out), then the text.
pub fn compose_body(fields: &[(String, String)], body: &str) -> String {
    let mut lines: Vec<String> = fields
        .iter()
        .filter(|(_, v)| !v.trim().is_empty())
        .map(|(n, v)| format!("{n}: {}", v.trim()))
        .collect();
    if !body.trim().is_empty() {
        lines.push(body.trim().to_string());
    }
    lines.join("\n")
}

fn first_line(body: &str) -> &str {
    body.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("")
}

/// The prompt "Annotate with AI" puts in the composer.
pub fn ai_prompt(subject: &str, template: &str) -> String {
    format!(
        "Annotate {subject} using the {template} template. Call get_annotation_template first, then add all notes in one annotate call."
    )
}

/// The annotation the popover saves.
pub fn build_annotation(
    doc: &Document,
    target: NodeId,
    category: &str,
    label: &str,
    fields: &[(String, String)],
    body: &str,
) -> Annotation {
    let mut a = Annotation::new(Some(target), label.trim(), category);
    a.body = compose_body(fields, body);
    annotation::auto_place(doc, &mut a);
    a
}

/// The categories a template offers in the popover (enabled ones that exist), with their
/// field names.
pub fn offered(tpl: &AnnotationTemplate, cats: &[AnnotationCategory]) -> Vec<(AnnotationCategory, Vec<String>)> {
    tpl.categories
        .iter()
        .filter(|t| t.enabled)
        .filter_map(|t| Some((category_by_key(cats, &t.key)?.clone(), t.fields.clone())))
        .collect()
}

// ---------------------------------------------------------------------------- state

/// Text of a note being edited inline.
#[derive(Clone, PartialEq)]
pub struct Draft {
    pub id: u64,
    pub label: String,
    pub body: String,
}

/// The New annotation popover.
#[derive(Clone)]
pub struct Popover {
    pub target: NodeId,
    /// Category id.
    pub category: String,
    pub label: String,
    pub body: String,
    pub values: HashMap<String, String>,
    focus: bool,
}

#[derive(Default)]
pub struct AnnotationsUi {
    /// Note highlighted in the list (and on the canvas).
    pub selected: Option<u64>,
    /// Category ids hidden from the list and the canvas (view state, not saved).
    pub hidden: HashSet<String>,
    pub popover: Option<Popover>,
    /// Right-click menu position on the canvas.
    pub menu: Option<egui::Pos2>,
    pub editing: Option<Draft>,
    pub pending_delete: Option<u64>,
    /// Folded groups of the list.
    pub folded: HashSet<u64>,
    /// Measured height of each list row (with spacing) at width `row_w`; off-screen rows are skipped.
    row_h: HashMap<u64, f32>,
    row_w: f32,
    /// The template picker edits the project default instead of this page.
    pub project_scope: bool,
    pub customize: Customize,
}

#[derive(Default)]
pub struct Customize {
    pub open: bool,
    draft: Option<TemplateDraft>,
    pending_delete: Option<String>,
}

struct TemplateDraft {
    tpl: AnnotationTemplate,
    cats: Vec<AnnotationCategory>,
    styling: Option<String>,
}

/// Takes the UI state out of `st`, joined with the canvas view's selection and hidden set.
fn take(st: &mut UiState) -> AnnotationsUi {
    let mut ann = std::mem::take(&mut st.ann_ui);
    ann.selected = st.selected_annotation;
    ann.hidden = std::mem::take(&mut st.annotations.hidden_categories);
    ann
}

fn put(st: &mut UiState, mut ann: AnnotationsUi) {
    st.selected_annotation = ann.selected;
    st.annotations.hidden_categories = std::mem::take(&mut ann.hidden);
    st.ann_ui = ann;
}

fn color_of(cats: &[AnnotationCategory], id: &str) -> Color32 {
    cats.iter().find(|c| c.id == id).map_or(Color32::GRAY, |c| super::c32(c.color))
}

/// Paints a category's icon: its Lucide animation or hand-drawn twin, else a dot.
pub fn paint_category_icon(ui: &Ui, rect: egui::Rect, name: &str, tint: Color32) {
    if let Some((first, _)) = super::lottie_icon::range(name) {
        super::lottie_icon::paint_frame(ui, name, rect, tint, first);
    } else if let Some(icon) = theme::icon_by_name(name) {
        theme::paint_icon(ui.painter(), rect, icon, tint);
    } else {
        ui.painter().circle_filled(rect.center(), rect.width() * 0.22, tint);
    }
}

fn number_badge(ui: &mut Ui, n: Option<u32>, fill: Color32) {
    let (rect, _) = ui.allocate_exact_size(vec2(BADGE, BADGE), Sense::hover());
    ui.painter().circle_filled(rect.center(), BADGE / 2.0, fill);
    let ink = kit::ink_on(fill);
    match n {
        Some(n) => {
            ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, n.to_string(), theme::semibold(theme::SECONDARY + 1.0), ink);
        }
        None => {
            ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "!", theme::semibold(theme::SECONDARY + 1.0), ink);
        }
    }
}

fn label(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).font(theme::semibold(theme::SECONDARY)).color(theme::pal(ui).secondary));
}

/// A category select that shows each option's colour dot and icon.
fn category_select(ui: &mut Ui, current: &str, cats: &[AnnotationCategory]) -> Option<String> {
    let p = theme::pal(ui);
    let cur = cats.iter().find(|c| c.id == current);
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), theme::FIELD_H), Sense::click());
    let hover = super::motion::hover(ui.ctx(), resp.id.with("catsel"), &resp);
    theme::fill_control(ui, rect, theme::radius_md(), theme::blend(p.field, p.hover.gamma_multiply(hover)));
    if let Some(c) = cur {
        ui.painter().circle_filled(egui::pos2(rect.left() + 16.0, rect.center().y), 5.0, super::c32(c.color));
    }
    ui.painter().text(
        egui::pos2(rect.left() + 30.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        cur.map_or("Category", |c| c.name.as_str()),
        egui::FontId::proportional(theme::BODY),
        p.text,
    );
    theme::paint_icon(
        ui.painter(),
        egui::Rect::from_center_size(egui::pos2(rect.right() - 14.0, rect.center().y), vec2(14.0, 14.0)),
        Icon::ChevronDown,
        p.secondary,
    );
    let mut chosen = None;
    egui::Popup::menu(&resp).width(rect.width().max(190.0)).show(|ui| {
        crate::theme::popup_backing(ui);
        ui.spacing_mut().item_spacing.y = 2.0;
        for c in cats {
            let (row, r) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::click());
            let on = c.id == current;
            let h = super::motion::hover(ui.ctx(), r.id.with("catopt"), &r);
            if on {
                theme::fill_selected(ui, row, theme::radius_md(), p.field);
            } else if h > 0.0 {
                ui.painter().rect_filled(row, theme::radius_md(), p.hover.gamma_multiply(h));
            }
            let tint = super::c32(c.color);
            ui.painter().circle_filled(egui::pos2(row.left() + 14.0, row.center().y), 4.5, tint);
            paint_category_icon(ui, egui::Rect::from_center_size(egui::pos2(row.left() + 36.0, row.center().y), vec2(15.0, 15.0)), &c.icon, tint);
            ui.painter().text(
                egui::pos2(row.left() + 54.0, row.center().y),
                egui::Align2::LEFT_CENTER,
                &c.name,
                egui::FontId::proportional(theme::BODY),
                p.text,
            );
            if r.clicked() {
                chosen = Some(c.id.clone());
                ui.close();
            }
        }
    });
    chosen.filter(|c| c != current)
}

// ---------------------------------------------------------------------------- popover

/// Open the New annotation popover on `target` (the first enabled category is preselected).
pub fn open_popover(ed: &mut Editor, ann: &mut AnnotationsUi, target: NodeId) {
    let tpl = ed.active_template(ed.active_page);
    let Some((cat, _)) = offered(&tpl, &ed.design.categories()).into_iter().next() else {
        ed.status = "Enable a category in the annotation template first".into();
        return;
    };
    ann.popover = Some(Popover {
        target,
        category: cat.id,
        label: String::new(),
        body: String::new(),
        values: HashMap::new(),
        focus: true,
    });
}

/// Screen position of the popover: right of the target, or left / below when out of room.
fn popover_pos(ed: &Editor, canvas: egui::Rect, target: NodeId) -> egui::Pos2 {
    let Some(r) = ed.doc.world_rect(target) else { return canvas.center() };
    anchor_pos(ed, canvas, &r, POPOVER_W)
}

/// Screen position of a `width`-wide popover next to the world rect `r` (the chat's Ask box too).
pub fn anchor_pos(ed: &Editor, canvas: egui::Rect, r: &crate::geom::Rect, width: f32) -> egui::Pos2 {
    let s = ed.camera.world_rect_to_screen(r);
    let (min, max) = (s.min(), V2 { x: s.x + s.w, y: s.y + s.h });
    let pos = egui::pos2(canvas.min.x + max.x as f32 + 12.0, canvas.min.y + min.y as f32);
    if pos.x + width > canvas.max.x {
        return egui::pos2(canvas.min.x + min.x as f32 - width - 12.0, pos.y);
    }
    pos
}

fn popover(ctx: &egui::Context, ed: &mut Editor, ann: &mut AnnotationsUi, canvas: egui::Rect) {
    let Some(mut pop) = ann.popover.take() else { return };
    if ed.doc.get(pop.target).is_none() {
        return;
    }
    let cats = ed.design.categories();
    let tpl = ed.active_template(ed.active_page);
    let choices = offered(&tpl, &cats);
    let fields: Vec<String> = choices.iter().find(|(c, _)| c.id == pop.category).map(|(_, f)| f.clone()).unwrap_or_default();
    let target_name = ed.doc.get(pop.target).map(|n| n.name.clone()).unwrap_or_default();
    let (mut save, mut cancel) = (false, false);
    let can_save = !pop.label.trim().is_empty();
    save |= ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Enter)) && can_save;
    cancel |= ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
    let pos = popover_pos(ed, canvas, pop.target);
    egui::Area::new("annotation-popover".into())
        .order(egui::Order::Foreground)
        .constrain_to(canvas)
        .fixed_pos(pos)
        .show(ctx, |ui| {
            theme::figma_fields(ui);
            let p = theme::pal(ui);
            kit::card_frame(ui).inner_margin(18).show_surface(ui, |ui| {
                ui.set_width(POPOVER_W - 36.0);
                ui.spacing_mut().item_spacing.y = 8.0;
                ui.horizontal(|ui| {
                    ui.label(RichText::new("New annotation").font(theme::semibold(16.0)));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if kit::close_button(ui).clicked() {
                            cancel = true;
                        }
                    });
                });
                egui::Frame::new().fill(p.field).corner_radius(theme::radius_md()).inner_margin(8).show_surface(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        let (r, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
                        theme::paint_icon(ui.painter(), r, Icon::Frame, theme::accent());
                        ui.label(RichText::new(&target_name).font(theme::semibold(theme::BODY)));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(RichText::new("Right edge").size(theme::SECONDARY).color(p.secondary));
                        });
                    });
                });
                label(ui, "Category");
                if let Some(c) = category_select(ui, &pop.category, &choices.iter().map(|(c, _)| c.clone()).collect::<Vec<_>>()) {
                    pop.category = c;
                }
                ui.horizontal(|ui| {
                    label(ui, "Label");
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(RichText::new(format!("{} / {LABEL_MAX}", pop.label.chars().count())).size(theme::SECONDARY).color(p.tertiary));
                    });
                });
                let r = crate::theme::lit(ui, |ui| ui.add(egui::TextEdit::singleline(&mut pop.label).char_limit(LABEL_MAX).desired_width(f32::INFINITY).margin(theme::INPUT_MARGIN)));
                if std::mem::take(&mut pop.focus) {
                    r.request_focus();
                }
                for f in &fields {
                    label(ui, f);
                    crate::theme::lit(ui, |ui| ui.add(egui::TextEdit::singleline(pop.values.entry(f.clone()).or_default()).desired_width(f32::INFINITY).margin(theme::INPUT_MARGIN)));
                }
                label(ui, "Note");
                crate::theme::lit(ui, |ui| ui.add(
                    egui::TextEdit::multiline(&mut pop.body)
                        .hint_text("Describe the rule, behaviour or data source. Markdown works.")
                        .desired_rows(4)
                        .desired_width(f32::INFINITY)
                        .margin(theme::INPUT_MARGIN),
                ));
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("⌘ Return to save").size(theme::SECONDARY).color(p.secondary));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.add_enabled_ui(can_save, |ui| {
                            if kit::button(ui, kit::ButtonKind::Primary, None, "Save").clicked() {
                                save = true;
                            }
                        });
                        if kit::button(ui, kit::ButtonKind::Secondary, None, "Cancel").clicked() {
                            cancel = true;
                        }
                    });
                });
            });
        });
    if save {
        let vals: Vec<(String, String)> = fields.iter().map(|f| (f.clone(), pop.values.get(f).cloned().unwrap_or_default())).collect();
        let a = build_annotation(&ed.doc, pop.target, &pop.category, &pop.label, &vals, &pop.body);
        ann.selected = Some(ed.add_annotation(a));
    } else if !cancel {
        ann.popover = Some(pop);
    }
}

/// Right-click menu on the canvas: Annotate the selection.
/// The right-click Actions menu on the selection. Annotate opens the popover here; the other
/// actions come back for `actions::dispatch`.
fn canvas_menu(ctx: &egui::Context, ed: &mut Editor, ann: &mut AnnotationsUi, canvas: egui::Rect) -> Option<super::actions::Act> {
    let clicked = ctx.input(|i| i.pointer.button_clicked(egui::PointerButton::Secondary));
    if clicked && !ed.selection.is_empty() {
        let at = ctx.input(|i| i.pointer.interact_pos());
        ann.menu = at.filter(|p| canvas.contains(*p));
    }
    let pos = ann.menu?;
    let mut act = None;
    let area = egui::Area::new("annotation-menu".into()).order(egui::Order::Foreground).fixed_pos(pos).show(ctx, |ui| {
        egui::Frame::popup(ui.style()).show(ui, |ui| {
            crate::theme::popup_backing(ui);
            ui.set_min_width(180.0);
            act = super::actions::menu(ui, ed);
        });
    });
    let away = ctx.input(|i| i.pointer.any_pressed()) && !area.response.contains_pointer();
    if act == Some(super::actions::Act::Annotate) {
        act = None;
        if let Some(t) = ed.top_selection().first().copied() {
            open_popover(ed, ann, t);
        }
        ann.menu = None;
    }
    if act.is_some() || away || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        ann.menu = None;
    }
    act
}

// ---------------------------------------------------------------------------- shared note editor

/// Writes the draft to the note when it differs.
fn flush(ed: &mut Editor, ann: &mut AnnotationsUi) {
    let Some(d) = ann.editing.take() else { return };
    let same = ed.doc.annotations.iter().find(|a| a.id == d.id).is_none_or(|a| a.label == d.label && a.body == d.body);
    if !same {
        ed.update_annotation(d.id, |a| {
            a.label = d.label.trim().to_string();
            a.body = d.body.clone();
        });
    }
}

fn begin_edit(ed: &mut Editor, ann: &mut AnnotationsUi, id: u64) {
    flush(ed, ann);
    if let Some(a) = ed.doc.annotations.iter().find(|a| a.id == id) {
        ann.editing = Some(Draft { id, label: a.label.clone(), body: a.body.clone() });
    }
}

/// Inline editor of one note: category, label, body (text saved when a field loses focus),
/// anchor side and delete.
fn edit_card(ui: &mut Ui, ed: &mut Editor, ann: &mut AnnotationsUi, id: u64, n: Option<u32>) {
    let Some(a) = ed.doc.annotations.iter().find(|a| a.id == id).cloned() else { return };
    let cats = ed.design.categories();
    let p = theme::pal(ui);
    let mut done = false;
    egui::Frame::new().fill(p.field).corner_radius(theme::radius_lg()).inner_margin(10).show_surface(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = 6.0;
        ui.horizontal(|ui| {
            number_badge(ui, n, color_of(&cats, &a.category));
            ui.label(RichText::new(&a.label).font(theme::semibold(theme::BODY)));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if theme::icon_button(ui, Icon::Trash, 22.0, false).on_hover_text("Delete").clicked() {
                    ann.pending_delete = Some(id);
                }
                if kit::chip(ui, "Done", true).clicked() {
                    done = true;
                }
            });
        });
        if let Some(c) = category_select(ui, &a.category, &cats) {
            ed.update_annotation(id, |a| a.category = c);
        }
        let Some(d) = ann.editing.as_mut().filter(|d| d.id == id) else { return };
        let l = crate::theme::lit(ui, |ui| ui.add(egui::TextEdit::singleline(&mut d.label).desired_width(f32::INFINITY).margin(theme::INPUT_MARGIN)));
        let b = crate::theme::lit(ui, |ui| ui.add(egui::TextEdit::multiline(&mut d.body).desired_rows(3).desired_width(f32::INFINITY).margin(theme::INPUT_MARGIN)));
        let blur = l.lost_focus() || b.lost_focus();
        ui.horizontal(|ui| {
            ui.label(RichText::new("Anchor").size(theme::SECONDARY).color(p.secondary));
            for (side, name) in [(Side::Top, "Top"), (Side::Right, "Right"), (Side::Bottom, "Bottom"), (Side::Left, "Left")] {
                if kit::chip(ui, name, a.anchor.side == side).clicked() {
                    ed.update_annotation(id, |a| a.anchor = Anchor { side, t: 0.5 });
                }
            }
        });
        if blur {
            let d = d.clone();
            ed.update_annotation(id, |a| {
                a.label = d.label.trim().to_string();
                a.body = d.body.clone();
            });
        }
    });
    if done {
        flush(ed, ann);
    }
}

/// Selects the note's layer and brings it into view.
fn reveal(ed: &mut Editor, a: &Annotation) {
    if let Some(t) = a.target.filter(|t| ed.doc.get(*t).is_some()) {
        ed.selection = vec![t];
        ed.run(Action::ZoomSelection);
        return;
    }
    let p = annotation::anchor_world(&ed.doc, a);
    ed.camera.pan = p - ed.viewport / (2.0 * ed.camera.zoom);
    ed.revision += 1;
}

// ---------------------------------------------------------------------------- side view

/// "Annotate with AI": opens Chat mode with the prompt and a chip of the frame, unsent.
pub fn annotate_with_ai(ed: &mut Editor, st: &mut UiState) {
    let tpl = ed.active_template(ed.active_page);
    let page = ed.active_page;
    let frame = ed.top_selection().first().and_then(|id| annotation::frame_of(&ed.doc, *id));
    let name = frame.and_then(|f| ed.doc.get(f)).map(|n| n.name.clone());
    let subject = name.as_ref().map_or_else(|| "this page".to_string(), |n| format!("the frame “{n}”"));
    st.chat.draft = ai_prompt(&subject, &tpl.name);
    if let Some(f) = frame {
        let chip = super::chat::layer_chip(ed, st.chat.gpu.as_ref(), page, &[f]);
        if !st.chat.sels.iter().any(|c| c.keys == chip.keys) {
            st.chat.sels.push(chip);
        }
    }
    st.set_chat_mode(true);
}

/// Annotation types "Send open notes to chat" picks up, besides open feedback.
const OPEN_NOTE_TYPES: [&str; 2] = ["todo", "question"];

/// The frame's open notes as one prompt: its chip, the context lines (open feedback, then
/// `todo`/`question` annotations from `selection_notes`) and the ask. None when nothing is open.
pub fn open_notes_message(ed: &Editor, frame: NodeId) -> Option<(super::chat::SelChip, Vec<String>, String)> {
    let page = ed.active_page;
    let name = ed.doc.get(frame)?.name.clone();
    let chip = super::chat::SelChip { label: name.clone(), keys: vec![crate::mcp::exec::node_key(ed, page, frame)], ..Default::default() };
    let feedback = ed.doc.subtree(frame).into_iter().filter_map(|id| Some((id, ed.doc.get(id)?))).flat_map(|(id, n)| {
        let key = crate::mcp::exec::node_key(ed, page, id);
        n.comments.iter().filter(|c| !c.resolved).map(move |c| format!("feedback #{} on {key} ({}): {}", c.id, n.name, c.text.trim()))
    });
    let typed = super::chat::selection_notes(ed, std::slice::from_ref(&chip)).into_iter().filter(|l| {
        l.split_once("] on ").is_some_and(|(head, _)| OPEN_NOTE_TYPES.iter().any(|k| head.to_lowercase().ends_with(&format!("[{k}"))))
    });
    let notes: Vec<String> = feedback.chain(typed).collect();
    if notes.is_empty() {
        return None;
    }
    let ask = format!(
        "Work through the {} open notes on the frame “{name}”: make each feedback and todo change, answer each question in a reply. Resolve the feedback you handled (resolve_feedback) and keep to the design system.",
        notes.len()
    );
    Some((chip, notes, ask))
}

/// "Send open notes to chat": sends the frame's open notes as one batched message with its chip
/// (an explicit action, so it sends) and opens Chat mode. The composer's own draft and chips stay.
pub fn send_open_notes(ctx: &egui::Context, ed: &mut Editor, st: &mut UiState, frame: NodeId) {
    let Some((chip, notes, ask)) = open_notes_message(ed, frame) else {
        ed.status = "No open notes on this frame".into();
        return;
    };
    st.set_chat_mode(true);
    let kept = (std::mem::replace(&mut st.chat.sels, vec![chip]), std::mem::replace(&mut st.chat.draft, ask));
    st.chat.sel_notes = notes;
    st.chat.refresh_ds(ed, true);
    let _ = st.chat.submit(ctx);
    (st.chat.sels, st.chat.draft) = kept;
}

fn template_picker(ui: &mut Ui, ed: &mut Editor, ann: &mut AnnotationsUi) {
    let page = ed.active_page;
    let templates = ed.design.templates();
    let over = ed.page_template_id(page).map(String::from);
    let default = ed.design.active_template(None);
    let eff = ed.active_template(page);
    let line = if over.is_some() { format!("This page: {}", eff.name) } else { format!("Project default: {}", default.name) };
    ui.label(RichText::new(line).font(theme::semibold(theme::BODY)));
    ui.horizontal(|ui| {
        if kit::chip(ui, "This page", !ann.project_scope).clicked() {
            ann.project_scope = false;
        }
        if kit::chip(ui, "Project default", ann.project_scope).clicked() {
            ann.project_scope = true;
        }
    });
    let page_scope = !ann.project_scope;
    let mut options: Vec<(usize, String)> = Vec::new();
    if page_scope {
        options.push((0, "Use project default".into()));
    }
    let offset = usize::from(page_scope);
    options.extend(templates.iter().enumerate().map(|(i, t)| (i + offset, t.name.clone())));
    let cur_id = if page_scope { over.as_deref() } else { Some(default.id.as_str()) };
    let current = cur_id.and_then(|id| templates.iter().position(|t| t.id == id)).map_or(0, |i| i + offset);
    let refs: Vec<(usize, &str)> = options.iter().map(|(i, s)| (*i, s.as_str())).collect();
    if let Some(i) = kit::select(ui, current, &refs, 0.0) {
        let id = i.checked_sub(offset).map(|i| templates[i].id.clone());
        let r = if page_scope { ed.set_page_template(page, id.as_deref()) } else { ed.set_default_template(id.as_deref()) };
        if let Err(e) = r {
            ed.status = e.to_string();
        }
    }
}

fn filter_chips(ui: &mut Ui, ed: &Editor, ann: &mut AnnotationsUi) {
    let cats = ed.design.categories();
    let counts = category_counts(&ed.doc, &cats);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
        if kit::chip(ui, &format!("All {}", ed.doc.annotations.len()), ann.hidden.is_empty()).clicked() {
            ann.hidden.clear();
        }
        for (c, (_, n)) in cats.iter().zip(counts) {
            let on = !ann.hidden.contains(&c.id);
            if kit::chip_ink(ui, &format!("{} {n}", c.name), on.then(|| super::c32(c.color))).clicked() && !ann.hidden.remove(&c.id) {
                ann.hidden.insert(c.id.clone());
            }
        }
    });
}

enum RowAct {
    Select,
    Edit,
    Delete,
}

fn note_row(ui: &mut Ui, a: &Annotation, n: u32, cat: Option<&AnnotationCategory>, selected: bool) -> Option<RowAct> {
    let p = theme::pal(ui);
    let mut act = None;
    let tint = cat.map_or(Color32::GRAY, |c| super::c32(c.color));
    let fill = if selected { p.field } else { Color32::TRANSPARENT };
    egui::Frame::new().fill(fill).corner_radius(theme::radius_lg()).inner_margin(8).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal_top(|ui| {
            number_badge(ui, (!a.detached).then_some(n), if a.detached { p.warning } else { tint });
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 3.0;
                ui.horizontal(|ui| {
                    if let Some(c) = cat {
                        kit::chip_ink(ui, &c.name, Some(tint));
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if theme::icon_button(ui, Icon::Trash, 20.0, false).on_hover_text("Delete").clicked() {
                            act = Some(RowAct::Delete);
                        }
                        if theme::icon_button(ui, Icon::Pencil, 20.0, false).on_hover_text("Edit").clicked() {
                            act = Some(RowAct::Edit);
                        }
                    });
                });
                let t = ui.add(egui::Label::new(RichText::new(&a.label).font(theme::semibold(theme::BODY))).sense(Sense::click()));
                let line = first_line(&a.body);
                let b = (!line.is_empty()).then(|| ui.add(egui::Label::new(RichText::new(line).color(p.secondary)).sense(Sense::click())));
                if t.clicked() || b.is_some_and(|b| b.clicked()) {
                    act = Some(RowAct::Select);
                }
                if t.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
            });
        });
    });
    act
}

/// The Annotations view of the left sidebar.
pub fn panel(ui: &mut Ui, ed: &mut Editor, st: &mut UiState) {
    let mut ann = take(st);
    let p = theme::pal(ui);
    let nums = st.annotations.numbers(ed);
    ui.horizontal(|ui| {
        ui.label(RichText::new("Annotations").font(theme::semibold(16.0)));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(RichText::new(ed.doc.annotations.len().to_string()).color(p.secondary));
        });
    });
    ui.add_space(4.0);
    template_picker(ui, ed, &mut ann);
    ui.add_space(2.0);
    if kit::button_wide(ui, kit::ButtonKind::Secondary, None, "Customize…").clicked() {
        ann.customize.open = true;
        ann.customize.draft = None;
    }
    let tidy_frame = ed.selection.first().and_then(|n| annotation::frame_of(&ed.doc, *n));
    ui.add_enabled_ui(!ed.doc.annotations.is_empty(), |ui| {
        if kit::button_wide(ui, kit::ButtonKind::Secondary, None, "Tidy notes")
            .on_hover_text("Re-place the notes of the selected frame (or the page) beside their targets")
            .clicked()
        {
            ed.tidy_annotations(tidy_frame);
        }
    });
    let can_chat = !cfg!(target_arch = "wasm32") && st.project_dir.is_some();
    ui.add_enabled_ui(can_chat, |ui| {
        if kit::button_wide(ui, kit::ButtonKind::Primary, None, "Annotate with AI")
            .on_disabled_hover_text("Open a saved project to use Chat")
            .clicked()
        {
            annotate_with_ai(ed, st);
        }
    });
    ui.add_space(4.0);
    filter_chips(ui, ed, &mut ann);
    ui.add_space(6.0);
    egui::ScrollArea::vertical().auto_shrink(false).show_viewport(ui, |ui, viewport| {
        theme::figma_fields(ui);
        if (ann.row_w - ui.available_width()).abs() > 0.5 {
            ann.row_h.clear();
            ann.row_w = ui.available_width();
        }
        let origin = ui.max_rect().top();
        let gap = ui.spacing().item_spacing.y;
        let groups = group_with(&ed.doc, &ann.hidden, &nums);
        let by_id = index_by_id(&ed.doc);
        if groups.is_empty() {
            ui.add_space(12.0);
            ui.label(RichText::new("No annotations yet. Pick the Annotate tool (N) and click a layer.").color(p.secondary));
        }
        let cats = ed.design.categories();
        for g in groups {
            let folded = ann.folded.contains(&g.key);
            let head = ui
                .horizontal(|ui| {
                    let (cr, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
                    theme::paint_icon(ui.painter(), cr, if folded { Icon::ChevronRight } else { Icon::ChevronDown }, p.secondary);
                    let (r, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
                    theme::paint_icon(ui.painter(), r, if g.detached { Icon::Question } else { Icon::Frame }, if g.detached { p.warning } else { p.secondary });
                    ui.add(egui::Label::new(RichText::new(&g.title).font(theme::semibold(theme::BODY))).sense(Sense::click()))
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                });
            if head.inner.clicked() && !ann.folded.remove(&g.key) {
                ann.folded.insert(g.key);
            }
            if folded {
                continue;
            }
            for id in g.ids {
                let Some(a) = by_id.get(&id).map(|i| ed.doc.annotations[*i].clone()) else { continue };
                let n = nums.get(&id).map_or(0, |n| n.1);
                if ann.editing.as_ref().is_some_and(|d| d.id == id) {
                    edit_card(ui, ed, &mut ann, id, (!a.detached).then_some(n));
                    continue;
                }
                let top = ui.cursor().top();
                if let Some(h) = ann.row_h.get(&id).copied()
                    && (top + h - origin < viewport.min.y || top - origin > viewport.max.y)
                {
                    ui.allocate_space(vec2(ui.available_width(), (h - gap).max(0.0)));
                    continue;
                }
                let cat = cats.iter().find(|c| c.id == a.category);
                match note_row(ui, &a, n, cat, ann.selected == Some(id)) {
                    Some(RowAct::Select) => {
                        flush(ed, &mut ann);
                        ann.selected = Some(id);
                        reveal(ed, &a);
                    }
                    Some(RowAct::Edit) => {
                        ann.selected = Some(id);
                        begin_edit(ed, &mut ann, id);
                    }
                    Some(RowAct::Delete) => ann.pending_delete = Some(id),
                    None => {}
                }
                ann.row_h.insert(id, ui.cursor().top() - top);
            }
        }
    });
    put(st, ann);
}

// ---------------------------------------------------------------------------- inspector

/// Inspector section for one layer: its notes and an add button.
pub fn inspector_section(ui: &mut Ui, ed: &mut Editor, st: &mut UiState, id: NodeId) {
    let mut ann = take(st);
    let p = theme::pal(ui);
    let cats = ed.design.categories();
    let nums = st.annotations.numbers(ed);
    let mine: Vec<Annotation> = ed.doc.annotations.iter().filter(|a| a.target == Some(id)).cloned().collect();
    ui.horizontal(|ui| {
        super::section(ui, "Annotations");
        if !mine.is_empty() {
            ui.label(RichText::new(mine.len().to_string()).size(theme::SECONDARY).color(p.secondary));
        }
    });
    for a in mine {
        let n = nums.get(&a.id).map(|n| n.1);
        if ann.editing.as_ref().is_some_and(|d| d.id == a.id) {
            edit_card(ui, ed, &mut ann, a.id, n);
            continue;
        }
        let r = ui
            .horizontal(|ui| {
                number_badge(ui, n, color_of(&cats, &a.category));
                ui.add(egui::Label::new(RichText::new(&a.label).font(theme::semibold(theme::BODY))).truncate().sense(Sense::click()))
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
            })
            .inner;
        if r.clicked() {
            ann.selected = Some(a.id);
            begin_edit(ed, &mut ann, a.id);
        }
    }
    if kit::button_wide(ui, kit::ButtonKind::Secondary, Some(Icon::Plus), "Add annotation").clicked() {
        open_popover(ed, &mut ann, id);
    }
    put(st, ann);
    let frame = annotation::frame_of(&ed.doc, id).filter(|f| *f == id);
    let can_chat = !cfg!(target_arch = "wasm32") && st.project_dir.is_some() && !st.chat.is_running();
    if let Some(f) = frame.filter(|f| can_chat && open_notes_message(ed, *f).is_some())
        && kit::button_wide(ui, kit::ButtonKind::Secondary, None, "Send open notes to chat")
            .on_hover_text("Open feedback and todo/question notes on this frame, as one chat message")
            .clicked()
    {
        send_open_notes(&ui.ctx().clone(), ed, st, f);
    }
}

// ---------------------------------------------------------------------------- customize modal

fn is_builtin(id: &str) -> bool {
    builtin_templates().iter().any(|t| t.id == id)
}

fn slug(name: &str, taken: &[AnnotationCategory]) -> String {
    let base: String = name.to_lowercase().chars().map(|c| if c.is_alphanumeric() { c } else { '-' }).collect();
    let base = base.trim_matches('-').to_string();
    let base = if base.is_empty() { "note".into() } else { base };
    let mut id = base.clone();
    let mut i = 1;
    while taken.iter().any(|c| c.id == id || c.key() == id) {
        i += 1;
        id = format!("{base}-{i}");
    }
    id
}

fn list_view(ui: &mut Ui, ed: &mut Editor, cu: &mut Customize) {
    let p = theme::pal(ui);
    let page = ed.active_page;
    let templates = ed.design.templates();
    let default = ed.design.active_template(None).id;
    let over = ed.page_template_id(page).map(String::from);
    let (mut edit, mut dup, mut reset, mut del) = (None, None, None, None);
    for t in &templates {
        egui::Frame::new().fill(p.field).corner_radius(theme::radius_lg()).inner_margin(10).show_surface(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(RichText::new(&t.name).font(theme::semibold(theme::BODY)));
                ui.label(RichText::new(if is_builtin(&t.id) { "Built-in" } else { "Custom" }).size(theme::SECONDARY).color(p.tertiary));
                if t.id == default {
                    kit::chip(ui, "Project default", true);
                }
                if over.as_deref() == Some(t.id.as_str()) {
                    kit::chip(ui, "This page", true);
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if is_builtin(&t.id) {
                        if theme::small_icon_button(ui, Some(crate::theme::Icon::Restore), "Reset").clicked() {
                            reset = Some(t.id.clone());
                        }
                    } else if theme::small_icon_button(ui, Some(crate::theme::Icon::Trash), "Delete").clicked() {
                        del = Some(t.id.clone());
                    }
                    if theme::small_button(ui, "Duplicate").clicked() {
                        dup = Some(t.id.clone());
                    }
                    if theme::small_icon_button(ui, Some(crate::theme::Icon::Pen), "Edit").clicked() {
                        edit = Some(t.clone());
                    }
                });
            });
            let keys = t.enabled_keys().join(", ");
            ui.label(RichText::new(keys).size(theme::SECONDARY).color(p.secondary));
        });
    }
    if let Some(t) = edit {
        cu.draft = Some(TemplateDraft { tpl: t, cats: ed.design.categories(), styling: None });
    }
    let res = if let Some(id) = dup {
        ed.duplicate_template(&id).map(|_| ())
    } else if let Some(id) = reset {
        ed.reset_template(&id)
    } else {
        Ok(())
    };
    if let Err(e) = res {
        ed.status = e.to_string();
    }
    if del.is_some() {
        cu.pending_delete = del;
    }
}

fn edit_view(ui: &mut Ui, d: &mut TemplateDraft) -> Option<bool> {
    let p = theme::pal(ui);
    let mut result = None;
    ui.horizontal(|ui| {
        label(ui, "Name");
        crate::theme::lit(ui, |ui| ui.add(egui::TextEdit::singleline(&mut d.tpl.name).desired_width(220.0).margin(theme::INPUT_MARGIN)));
        label(ui, "Display");
        for (disp, name) in [(Display::Full, "Full"), (Display::Badges, "Badges")] {
            if kit::chip(ui, name, d.tpl.display == disp).clicked() {
                d.tpl.display = disp;
            }
        }
    });
    let cats = &mut d.cats;
    let styling = &mut d.styling;
    for tc in &mut d.tpl.categories {
        let Some(ci) = cats.iter().position(|c| c.key() == tc.key) else { continue };
        egui::Frame::new().fill(p.field).corner_radius(theme::radius_lg()).inner_margin(10).show_surface(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 6.0;
            ui.horizontal(|ui| {
                kit::toggle(ui, &mut tc.enabled);
                let c = &cats[ci];
                ui.label(RichText::new(&c.name).font(theme::semibold(theme::BODY)).color(super::c32(c.color)));
                ui.label(RichText::new(&c.key().to_string()).size(theme::SECONDARY).color(p.tertiary));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let open = styling.as_deref() == Some(c.id.as_str());
                    if kit::chip(ui, "Style", open).clicked() {
                        *styling = if open { None } else { Some(c.id.clone()) };
                    }
                });
            });
            if styling.as_deref() == Some(cats[ci].id.as_str()) {
                let c = &mut cats[ci];
                let old_key = c.key().to_string();
                ui.horizontal(|ui| {
                    crate::theme::lit(ui, |ui| ui.add(egui::TextEdit::singleline(&mut c.name).hint_text("Name").desired_width(130.0).margin(theme::INPUT_MARGIN)));
                    crate::theme::lit(ui, |ui| ui.add(egui::TextEdit::singleline(&mut c.key).hint_text("Key").desired_width(70.0).margin(theme::INPUT_MARGIN)));
                    crate::theme::lit(ui, |ui| ui.add(egui::TextEdit::singleline(&mut c.icon).hint_text("Icon").desired_width(90.0).margin(theme::INPUT_MARGIN)));
                    let mut col = super::c32(c.color);
                    if egui::color_picker::color_edit_button_srgba(ui, &mut col, egui::color_picker::Alpha::Opaque).changed() {
                        c.color = super::from32(col);
                    }
                });
                if c.key() != old_key {
                    tc.key = c.key().to_string();
                }
            }
            label(ui, "Guidance for the note");
            crate::theme::lit(ui, |ui| ui.add(egui::TextEdit::multiline(&mut tc.prompt).desired_rows(2).desired_width(f32::INFINITY).margin(theme::INPUT_MARGIN)));
            label(ui, "Fields");
            let mut remove = None;
            for (i, f) in tc.fields.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    crate::theme::lit(ui, |ui| ui.add(egui::TextEdit::singleline(f).desired_width(200.0).margin(theme::INPUT_MARGIN)));
                    if theme::icon_button(ui, Icon::Minus, 22.0, false).on_hover_text("Remove field").clicked() {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                tc.fields.remove(i);
            }
            if theme::small_icon_button(ui, Some(crate::theme::Icon::Plus), "Field").clicked() {
                tc.fields.push(String::new());
            }
        });
    }
    ui.horizontal_wrapped(|ui| {
        let missing: Vec<AnnotationCategory> = d.cats.iter().filter(|c| !d.tpl.categories.iter().any(|t| t.key == c.key())).cloned().collect();
        for c in missing {
            if kit::chip_ink(ui, &format!("+ {}", c.name), Some(super::c32(c.color))).clicked() {
                d.tpl.categories.push(TemplateCategory { key: c.key().to_string(), enabled: true, prompt: String::new(), fields: Vec::new() });
            }
        }
        if kit::chip(ui, "+ New category", false).clicked() {
            let id = slug("note", &d.cats);
            d.cats.push(AnnotationCategory { id: id.clone(), key: id.clone(), name: "New category".into(), color: NEW_CATEGORY_COLOR, icon: CATEGORY_ICON.into() });
            d.tpl.categories.push(TemplateCategory { key: id.clone(), enabled: true, prompt: String::new(), fields: Vec::new() });
            d.styling = Some(id);
        }
    });
    ui.add_space(4.0);
    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        let dup = d.cats.iter().enumerate().any(|(i, c)| d.cats[..i].iter().any(|o| o.key().eq_ignore_ascii_case(c.key())));
        let ok = !dup && !d.tpl.name.trim().is_empty() && d.cats.iter().all(|c| !c.key().trim().is_empty() && !c.name.trim().is_empty());
        if dup {
            ui.label(RichText::new("Category keys must be unique").size(theme::SECONDARY).color(p.tertiary));
        }
        ui.add_enabled_ui(ok, |ui| {
            if kit::button(ui, kit::ButtonKind::Primary, None, "Save").clicked() {
                result = Some(true);
            }
        });
        if kit::button(ui, kit::ButtonKind::Secondary, None, "Back").clicked() {
            result = Some(false);
        }
    });
    result
}

fn customize(ctx: &egui::Context, ed: &mut Editor, ann: &mut AnnotationsUi) {
    let mut cu = std::mem::take(&mut ann.customize);
    if !cu.open {
        ann.customize = cu;
        return;
    }
    let p = theme::pal_from_ctx(ctx);
    let frame = egui::Frame::new().fill(p.sidebar).corner_radius(crate::theme::r(24.0)).inner_margin(24);
    let mut close = false;
    let modal = egui::Modal::new(egui::Id::new("annotation-customize")).frame(frame).show(ctx, |ui| {
        theme::modal_backing(ui, theme::pal(ui).sidebar, theme::ru(24));
        theme::figma_fields(ui);
        ui.set_width(MODAL_W.min(ctx.content_rect().width() - 96.0));
        ui.spacing_mut().item_spacing.y = 8.0;
        ui.horizontal(|ui| {
            ui.label(RichText::new("Annotation templates").font(theme::semibold(18.0)));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if kit::close_button(ui).clicked() {
                    close = true;
                }
            });
        });
        egui::ScrollArea::vertical().max_height((ctx.content_rect().height() - 220.0).max(200.0)).show(ui, |ui| {
            if let Some(d) = cu.draft.as_mut() {
                match edit_view(ui, d) {
                    Some(true) => {
                        if let Err(e) = ed.save_template(d.tpl.clone(), d.cats.clone()) {
                            ed.status = e.to_string();
                        }
                        cu.draft = None;
                    }
                    Some(false) => cu.draft = None,
                    None => {}
                }
            } else {
                list_view(ui, ed, &mut cu);
            }
        });
    });
    if let Some(id) = cu.pending_delete.clone() {
        let name = ed.design.templates().iter().find(|t| t.id == id).map_or(id.clone(), |t| t.name.clone());
        match super::confirm::dialog(ctx, "annotation-template-delete", "Delete template?", &format!("Delete “{name}”? Pages using it fall back to the project default."), "Delete") {
            Some(true) => {
                if let Err(e) = ed.delete_template(&id) {
                    ed.status = e.to_string();
                }
                cu.pending_delete = None;
            }
            Some(false) => cu.pending_delete = None,
            None => {}
        }
    } else if modal.should_close() || close {
        cu.open = false;
        cu.draft = None;
    }
    ann.customize = cu;
}

// ---------------------------------------------------------------------------- overlays

/// Popover, right-click menu, Customize modal and delete confirm. Called once per frame.
pub fn overlays(ctx: &egui::Context, ed: &mut Editor, st: &mut UiState, canvas: egui::Rect) {
    let mut ann = take(st);
    let act = canvas_menu(ctx, ed, &mut ann, canvas);
    popover(ctx, ed, &mut ann, canvas);
    customize(ctx, ed, &mut ann);
    if let Some(id) = ann.pending_delete {
        let label = ed.doc.annotations.iter().find(|a| a.id == id).map(|a| a.label.clone());
        match label {
            None => ann.pending_delete = None,
            Some(l) => match super::confirm::dialog(ctx, "annotation-delete", "Delete annotation?", &format!("Delete “{l}”? You can undo it."), "Delete") {
                Some(yes) => {
                    if yes {
                        ed.delete_annotation(id);
                        if ann.editing.as_ref().is_some_and(|d| d.id == id) {
                            ann.editing = None;
                        }
                    }
                    ann.pending_delete = None;
                }
                None => {}
            },
        }
    }
    put(st, ann);
    if let Some(a) = act {
        super::actions::dispatch(ed, st, a);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::Rect;
    use crate::model::{Node, NodeKind};

    fn frame(e: &mut Editor, name: &str, r: Rect) -> NodeId {
        let id = e.add_node(Node::new(NodeId(0), NodeKind::Frame { clip: true }, r), None);
        e.edit("name", |tx| tx.node_mut(id).unwrap().name = name.into());
        id
    }

    fn child(e: &mut Editor, parent: NodeId, r: Rect) -> NodeId {
        e.add_node(Node::new(NodeId(0), NodeKind::Rect, r), Some(parent))
    }

    #[test]
    fn compose_body_puts_fields_first_and_skips_blanks() {
        let f = vec![("Issue".to_string(), "No label".to_string()), ("WCAG".to_string(), "  ".to_string()), ("Fix".to_string(), "Add aria-label".to_string())];
        assert_eq!(compose_body(&f, " Eye toggle. "), "Issue: No label\nFix: Add aria-label\nEye toggle.");
        assert_eq!(compose_body(&[], ""), "");
    }

    #[test]
    fn grouping_filters_orders_and_detaches() {
        let mut e = Editor::default();
        let f2 = frame(&mut e, "Pricing", Rect::new(500.0, 0.0, 300.0, 300.0));
        let f1 = frame(&mut e, "Sign in", Rect::new(0.0, 0.0, 300.0, 300.0));
        let low = child(&mut e, f1, Rect::new(10.0, 200.0, 20.0, 20.0));
        let top = child(&mut e, f1, Rect::new(10.0, 10.0, 20.0, 20.0));
        let other = child(&mut e, f2, Rect::new(10.0, 10.0, 20.0, 20.0));
        let a = e.add_annotation(Annotation::new(Some(low), "low", "data"));
        let b = e.add_annotation(Annotation::new(Some(top), "top", "rule"));
        let c = e.add_annotation(Annotation::new(Some(other), "pricing", "rule"));
        let d = e.add_annotation(Annotation::new(None, "loose", "rule"));
        let g = group_annotations(&e.doc, &HashSet::new());
        assert_eq!(g.iter().map(|g| g.title.as_str()).collect::<Vec<_>>(), ["Sign in", "Pricing", "Page"]);
        assert_eq!(g[0].ids, [b, a], "numbering order");
        assert_eq!(g[1].ids, [c]);
        assert_eq!(g[2].ids, [d]);
        let hidden: HashSet<String> = ["rule".to_string()].into();
        let g = group_annotations(&e.doc, &hidden);
        assert_eq!(g.len(), 1, "empty groups dropped");
        assert_eq!(g[0].ids, [a]);
        e.edit("Delete", |tx| tx.remove(f2));
        let g = group_annotations(&e.doc, &HashSet::new());
        let det = g.last().unwrap();
        assert!(det.detached && det.ids == [c]);
        let counts = category_counts(&e.doc, &e.design.categories());
        assert_eq!(counts[0], ("rule".to_string(), 3));
        assert_eq!(counts[3], ("data".to_string(), 1));
    }

    #[test]
    fn popover_offers_only_enabled_categories_and_saves_fields() {
        let mut e = Editor::default();
        let f = frame(&mut e, "Sign in", Rect::new(0.0, 0.0, 300.0, 300.0));
        e.set_page_template(0, Some("accessibility-review")).unwrap();
        let tpl = e.active_template(0);
        let offer = offered(&tpl, &e.design.categories());
        assert_eq!(offer.len(), 1);
        assert_eq!(offer[0].0.id, "a11y");
        assert_eq!(offer[0].1, ["Issue", "WCAG", "Fix"]);
        let vals = vec![("Issue".to_string(), "No label".to_string()), ("WCAG".to_string(), String::new()), ("Fix".to_string(), "aria-label".to_string())];
        let a = build_annotation(&e.doc, f, "a11y", " Eye ", &vals, "Details");
        assert_eq!(a.label, "Eye");
        assert_eq!(a.body, "Issue: No label\nFix: aria-label\nDetails");
        e.add_annotation(a);
        assert_eq!(e.doc.annotations.len(), 1);
    }

    #[test]
    fn ai_prompt_and_chat_hand_off_do_not_send() {
        let mut e = Editor::default();
        let f = frame(&mut e, "Sign in", Rect::new(0.0, 0.0, 300.0, 300.0));
        let mut st = UiState::default();
        annotate_with_ai(&mut e, &mut st);
        assert_eq!(st.chat.draft, ai_prompt("this page", "Product spec"));
        assert!(st.chat.sels.is_empty());
        e.select(vec![f]);
        annotate_with_ai(&mut e, &mut st);
        assert!(st.chat.draft.starts_with("Annotate the frame “Sign in” using the Product spec template."));
        assert_eq!(st.chat.sels.len(), 1);
        assert!(st.chat.turn.is_none());
    }

    #[test]
    fn open_notes_batch_feedback_todos_and_questions_with_a_chip() {
        let mut e = Editor::default();
        let f = frame(&mut e, "Sign in", Rect::new(0.0, 0.0, 300.0, 300.0));
        let c = child(&mut e, f, Rect::new(10.0, 10.0, 20.0, 20.0));
        assert!(open_notes_message(&e, f).is_none(), "nothing open");
        let comment = |id, text: &str, resolved| crate::model::Comment { id, text: text.into(), author: "user".into(), resolved, reply: None };
        e.edit("fb", |tx| {
            let n = tx.node_mut(c).unwrap();
            n.comments.push(comment(1, "Bigger button", false));
            n.comments.push(comment(2, "Done already", true));
        });
        e.add_annotation(Annotation::new(Some(c), "Empty state", "todo"));
        e.add_annotation(Annotation::new(Some(f), "Which locale?", "Question"));
        e.add_annotation(Annotation::new(Some(c), "Reads users.email", "data"));
        let (chip, notes, ask) = open_notes_message(&e, f).unwrap();
        assert_eq!(chip.label, "Sign in");
        assert_eq!(chip.keys, [crate::mcp::exec::node_key(&e, 0, f)]);
        assert_eq!(notes.len(), 3, "{notes:?}");
        assert!(notes[0].starts_with("feedback #1 on ") && notes[0].ends_with("Bigger button"));
        assert!(notes[1..].iter().any(|n| n.contains("[todo]") && n.contains("Empty state")));
        assert!(notes[1..].iter().any(|n| n.contains("Which locale?")));
        assert!(ask.contains("3 open notes") && ask.contains("“Sign in”"));
    }

    fn run(f: impl FnMut(&mut Ui)) {
        let ctx = egui::Context::default();
        theme::install_fonts(&ctx);
        let mut f = f;
        for _ in 0..2 {
            ctx.run_ui(egui::RawInput::default(), |ui| f(ui)).textures_delta.clear();
        }
    }

    #[test]
    fn index_by_id_maps_every_note_and_long_list_renders_with_row_cache() {
        let mut e = Editor::default();
        let f = frame(&mut e, "Sign in", Rect::new(0.0, 0.0, 300.0, 300.0));
        let ids: Vec<u64> = (0..300).map(|i| e.add_annotation(Annotation::new(Some(f), &format!("n{i}"), "data"))).collect();
        let idx = index_by_id(&e.doc);
        assert_eq!(idx.len(), 300);
        assert!(ids.iter().all(|id| e.doc.annotations[idx[id]].id == *id));
        let mut st = UiState::default();
        run(|ui| panel(ui, &mut e, &mut st));
        assert!(!st.ann_ui.row_h.is_empty(), "drawn rows are measured");
        run(|ui| panel(ui, &mut e, &mut st));
    }

    #[test]
    fn numbers_cache_follows_document_stamp() {
        let mut e = Editor::default();
        let f = frame(&mut e, "Sign in", Rect::new(0.0, 0.0, 300.0, 300.0));
        let mut v = super::super::annotations_canvas::AnnotationsView::default();
        let a = v.numbers(&e);
        assert!(std::sync::Arc::ptr_eq(&a, &v.numbers(&e)), "reused while the doc is unchanged");
        let id = e.add_annotation(Annotation::new(Some(f), "x", "data"));
        assert_eq!(v.numbers(&e)[&id].1, 1);
    }

    #[test]
    fn side_view_smoke() {
        let mut e = Editor::default();
        let f = frame(&mut e, "Sign in", Rect::new(0.0, 0.0, 300.0, 300.0));
        let c = child(&mut e, f, Rect::new(10.0, 10.0, 20.0, 20.0));
        let mut a = Annotation::new(Some(c), "Email", "data");
        a.body = "Reads users.email".into();
        e.add_annotation(a);
        e.add_annotation(Annotation::new(None, "Loose", "content"));
        let mut st = UiState::default();
        run(|ui| panel(ui, &mut e, &mut st));
        st.ann_ui.editing = Some(Draft { id: e.doc.annotations[0].id, label: "Email".into(), body: String::new() });
        run(|ui| panel(ui, &mut e, &mut st));
        run(|ui| inspector_section(ui, &mut e, &mut st, c));
    }

    #[test]
    fn modal_and_popover_smoke_and_template_edit_is_undoable() {
        let mut e = Editor::default();
        let f = frame(&mut e, "Sign in", Rect::new(0.0, 0.0, 300.0, 300.0));
        let mut st = UiState::default();
        st.ann_ui.customize.open = true;
        let canvas = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), vec2(1000.0, 700.0));
        let ctx = egui::Context::default();
        theme::install_fonts(&ctx);
        let frames = |st: &mut UiState, e: &mut Editor| {
            for _ in 0..2 {
                ctx.run_ui(egui::RawInput::default(), |ui| overlays(ui.ctx(), e, st, canvas)).textures_delta.clear();
            }
        };
        frames(&mut st, &mut e);
        let tpl = e.design.templates().into_iter().find(|t| t.id == "minimal").unwrap();
        st.ann_ui.customize.draft = Some(TemplateDraft { tpl, cats: e.design.categories(), styling: Some("rule".into()) });
        frames(&mut st, &mut e);
        open_popover(&mut e, &mut st.ann_ui, f);
        assert!(st.ann_ui.popover.is_some());
        frames(&mut st, &mut e);
        let before = e.design.clone();
        let mut t = e.active_template(0);
        t.name = "Renamed".into();
        t.id = "minimal".into();
        e.upsert_template(t).unwrap();
        assert_ne!(e.design, before);
        e.undo();
        assert_eq!(e.design, before);
    }
}
