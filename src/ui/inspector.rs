//! The Design inspector: macOS-style grouped sections for the selection's geometry, layout,
//! constraints, text, fills, stroke and layer properties. Edits are coalesced per field so a
//! drag on a number is one undo step, and edits inside instances become overrides.

use super::{UiState, c32, from32, section, title_text};
use crate::editor::vector_ops::BoolOp;
use crate::editor::{AppRequest, Editor};
use crate::geom::v2;
use crate::model::{
    Align, AutoLayout, BlendMode, Cap, Color, Constraint, Direction, ImageMode, Join, Node, NodeId,
    NodeKind, Paint, Sizing, Stop, StrokeAlign, TextAlign,
};
use crate::theme;
use super::bind::BIND_W;
use crate::variables::Kind;
use egui::{RichText, Ui};

const LABEL_W: f32 = 64.0;

/// Apply a property edit as a coalesced transaction keyed by field + selection.
pub fn apply(ed: &mut Editor, field: &str, ids: &[NodeId], f: impl Fn(&mut Node)) {
    let key = format!("{field}:{ids:?}");
    ed.begin_coalescing(&format!("Change {field}"), Some(&key));
    {
        let mut tx = ed.tx();
        for id in ids {
            if let Some(n) = tx.node_mut(*id) {
                f(n);
            }
        }
    }
    ed.mark_overrides(ids, field);
    for id in ids {
        ed.fit_text(*id);
    }
    ed.commit();
}

fn seal_on_release(ed: &mut Editor, r: &egui::Response) {
    if r.drag_stopped() || r.lost_focus() {
        ed.history.seal();
    }
}

/// A row: secondary label on the left, controls on the right.
fn row<R>(ui: &mut Ui, label: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(egui::vec2(LABEL_W, 20.0), egui::Sense::hover());
        ui.painter().text(
            r.left_center(),
            egui::Align2::LEFT_CENTER,
            label,
            egui::FontId::proportional(theme::SECONDARY + 1.0),
            theme::pal(ui).secondary,
        );
        add(ui)
    })
    .inner
}

/// Width of one of two equal columns that fill the row.
fn half(ui: &Ui) -> f32 {
    (ui.available_width() - ui.spacing().item_spacing.x) / 2.0
}

/// Width of one of three equal columns that fill the row.
fn third(ui: &Ui) -> f32 {
    (ui.available_width() - 2.0 * ui.spacing().item_spacing.x) / 3.0
}

enum Cell<'a> {
    Icon(theme::Icon),
    Text(&'a str),
}

/// A Figma-style segmented control: one tinted box split into equal cells. Returns the cell
/// that was clicked.
fn segmented(ui: &mut Ui, cells: &[(Cell, bool, &str)], width: f32) -> Option<usize> {
    let h = theme::FIELD_H;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, h), egui::Sense::hover());
    let p = theme::pal(ui);
    let rest = ui.visuals().widgets.inactive.bg_fill;
    let (hover, on_fill) = (
        ui.visuals().widgets.hovered.bg_fill,
        ui.visuals().widgets.active.bg_fill,
    );
    ui.painter().rect_filled(rect, 5.0, rest);
    let cw = width / cells.len() as f32;
    let mut clicked = None;
    for (i, (cell, on, tip)) in cells.iter().enumerate() {
        let r =
            egui::Rect::from_min_size(rect.min + egui::vec2(cw * i as f32, 0.0), egui::vec2(cw, h));
        let resp = ui
            .allocate_rect(r, egui::Sense::click())
            .on_hover_text(*tip);
        if *on || resp.hovered() {
            ui.painter()
                .rect_filled(r.shrink(2.0), 4.0, if *on { on_fill } else { hover });
        }
        let ink = if *on { p.text } else { p.secondary };
        match cell {
            Cell::Icon(ic) => theme::paint_icon(
                ui.painter(),
                egui::Rect::from_center_size(r.center(), egui::Vec2::splat(14.0)),
                *ic,
                ink,
            ),
            Cell::Text(t) => {
                ui.painter().text(
                    r.center(),
                    egui::Align2::CENTER_CENTER,
                    *t,
                    theme::semibold(12.0),
                    ink,
                );
            }
        }
        if resp.clicked() {
            clicked = Some(i);
        }
    }
    clicked
}

/// Colour swatch plus hex, in one tinted field (Figma's fill/stroke row).
fn color_field(
    ui: &mut Ui,
    ed: &mut Editor,
    c: Color,
    bound: Option<&str>,
    detach: &mut bool,
    width: f32,
) -> Option<Color> {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, theme::FIELD_H), egui::Sense::hover());
    ui.painter()
        .rect_filled(rect, 5.0, ui.visuals().widgets.inactive.bg_fill);
    let sw = egui::Rect::from_min_size(
        rect.left_top() + egui::vec2(5.0, (theme::FIELD_H - 18.0) / 2.0),
        egui::Vec2::splat(18.0),
    );
    let mut out = None;
    let mut c2 = c32(c);
    let mut swatch_ui = ui.new_child(egui::UiBuilder::new().max_rect(sw));
    swatch_ui.spacing_mut().interact_size = egui::Vec2::splat(18.0);
    let r = swatch_ui.color_edit_button_srgba(&mut c2);
    seal_on_release(ed, &r);
    if r.changed() && from32(c2) != c {
        out = Some(from32(c2));
    }
    let text_rect = egui::Rect::from_min_max(
        egui::pos2(sw.right() + 8.0, rect.top()),
        egui::pos2(rect.right() - 8.0, rect.bottom()),
    );
    if let Some(v) = bound {
        let x = egui::Rect::from_center_size(rect.right_center() - egui::vec2(14.0, 0.0), egui::Vec2::splat(16.0));
        *detach = super::bind::cross(ui, x);
        let name = super::bind::display_name(&ed.design, v);
        let text_rect = egui::Rect::from_min_max(text_rect.min, egui::pos2(x.left() - 4.0, rect.bottom()));
        let mut label_ui = ui.new_child(egui::UiBuilder::new().max_rect(text_rect));
        label_ui.set_clip_rect(text_rect.intersect(ui.clip_rect()));
        label_ui
            .add_sized(
                text_rect.size(),
                egui::Label::new(
                    RichText::new(&name)
                        .font(theme::semibold(theme::SECONDARY + 1.0))
                        .color(theme::accent()),
                )
                .truncate(),
            )
            .on_hover_text(name);
        return out;
    }
    let id = ui.id().with("hex");
    let shown = c.to_hex().trim_start_matches('#').to_uppercase();
    let mut text = if ui.memory(|m| m.has_focus(id)) {
        ui.data(|d| d.get_temp::<String>(id)).unwrap_or(shown)
    } else {
        shown
    };
    let mut text_ui = ui.new_child(egui::UiBuilder::new().max_rect(text_rect));
    text_ui.set_clip_rect(text_rect.intersect(ui.clip_rect()));
    let r = text_ui.put(
        text_rect,
        egui::TextEdit::singleline(&mut text)
            .id(id)
            .frame(egui::Frame::NONE)
            .margin(egui::Margin::ZERO)
            .vertical_align(egui::Align::Center)
            .font(egui::FontId::proportional(theme::SECONDARY + 1.0)),
    );
    if r.has_focus() {
        ui.data_mut(|d| d.insert_temp(id, text.clone()));
    }
    if r.lost_focus() {
        if let Some(nc) = Color::from_hex(&format!("#{}", text.trim().trim_start_matches('#'))) {
            if nc != c {
                out = Some(nc);
            }
            ed.history.seal();
        }
        ui.data_mut(|d| d.remove::<String>(id));
    }
    out
}

/// A Figma-style number field: tinted box, secondary label then the value, both on the left.
/// Drag sideways to scrub, click to type.
fn num_field(
    ui: &mut Ui,
    ed: &mut Editor,
    prefix: &str,
    value: f64,
    speed: f64,
    range: std::ops::RangeInclusive<f64>,
    width: f32,
) -> Option<f64> {
    field(ui, ed, prefix, "", value, speed, range, width)
}

#[allow(clippy::too_many_arguments)]
fn field(
    ui: &mut Ui,
    ed: &mut Editor,
    prefix: &str,
    suffix: &str,
    value: f64,
    speed: f64,
    range: std::ops::RangeInclusive<f64>,
    width: f32,
) -> Option<f64> {
    let enabled = ui.is_enabled();
    let (rect, resp) = ui.allocate_exact_size(
        egui::vec2(width, theme::FIELD_H),
        egui::Sense::click_and_drag(),
    );
    let id = resp.id;
    let p = theme::pal(ui);
    let font = egui::FontId::proportional(theme::SECONDARY + 1.0);
    let label = prefix.trim();
    let inset = 8.0;
    let mut out = None;
    let mut buf = ui.data(|d| d.get_temp::<String>(id));
    let text_x = if label.is_empty() {
        rect.left() + inset
    } else {
        let w = ui
            .painter()
            .layout_no_wrap(label.into(), font.clone(), p.secondary)
            .size()
            .x;
        rect.left() + inset + w + 8.0
    };
    if enabled && resp.hovered() || resp.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
    }
    if enabled && resp.dragged() && buf.is_none() {
        let v = (value + resp.drag_delta().x as f64 * speed).clamp(*range.start(), *range.end());
        if v != value {
            out = Some(v);
        }
    }
    if enabled && resp.drag_stopped() {
        ed.history.seal();
    }
    if enabled && resp.clicked() && buf.is_none() {
        buf = Some(fmt_num(value));
        ui.memory_mut(|m| m.request_focus(id.with("edit")));
    }
    let fill = if buf.is_some() || !enabled {
        ui.visuals().widgets.inactive.bg_fill
    } else {
        ui.style().interact(&resp).bg_fill
    };
    let edit_stroke = if buf.is_some() {
        egui::Stroke::new(1.0, theme::accent())
    } else {
        egui::Stroke::NONE
    };
    ui.painter().rect(
        rect,
        egui::CornerRadius::same(5),
        fill,
        edit_stroke,
        egui::StrokeKind::Inside,
    );
    let ink = if enabled { p.text } else { p.tertiary };
    if !label.is_empty() {
        ui.painter().text(
            egui::pos2(rect.left() + inset, rect.center().y),
            egui::Align2::LEFT_CENTER,
            label,
            font.clone(),
            p.secondary,
        );
    }
    if let Some(mut text) = buf {
        let te_id = id.with("edit");
        let edit_rect = egui::Rect::from_min_max(
            egui::pos2(text_x, rect.top()),
            egui::pos2(rect.right() - inset, rect.bottom()),
        );
        let r = ui.put(
            edit_rect,
            egui::TextEdit::singleline(&mut text)
                .id(te_id)
                .frame(egui::Frame::NONE)
                .margin(egui::Margin::ZERO)
                .vertical_align(egui::Align::Center)
                .font(font),
        );
        let esc = ui.input(|i| i.key_pressed(egui::Key::Escape));
        if r.lost_focus() {
            if !esc && let Ok(v) = text.trim().trim_end_matches(suffix).trim().parse::<f64>() {
                let v = v.clamp(*range.start(), *range.end());
                if v != value {
                    out = Some(v);
                }
                ed.history.seal();
            }
            ui.data_mut(|d| d.remove::<String>(id));
        } else {
            ui.data_mut(|d| d.insert_temp(id, text));
        }
    } else {
        ui.painter().text(
            egui::pos2(text_x, rect.center().y),
            egui::Align2::LEFT_CENTER,
            format!("{}{suffix}", fmt_num(value)),
            font,
            ink,
        );
    }
    out
}

/// [`field`] with a variable affordance: a bind button beside it, or, when every selected layer
/// binds `bfield` to the same variable, the variable's chip in its place.
#[allow(clippy::too_many_arguments)]
fn bound_field(
    ui: &mut Ui,
    ed: &mut Editor,
    ids: &[NodeId],
    bfield: &str,
    prefix: &str,
    suffix: &str,
    value: f64,
    speed: f64,
    range: std::ops::RangeInclusive<f64>,
    width: f32,
) -> Option<f64> {
    if let Some(css) = super::bind::shared_binding(ed, ids, bfield) {
        let shown = format!("{}{suffix}", fmt_num(value));
        let act = super::bind::bound_chip(ui, ed, Kind::Number, &css, &shown, width);
        super::bind::apply(ed, ids, bfield, act);
        return None;
    }
    let w = (width - BIND_W - ui.spacing().item_spacing.x).max(36.0);
    let out = field(ui, ed, prefix, suffix, value, speed, range, w);
    if let Some(css) = super::bind::bind_button(ui, ed, Kind::Number) {
        super::bind::apply(ed, ids, bfield, Some(Some(css)));
    }
    out
}

/// A row for a field that has no room for a bind button: label, then the bind button or chip.
fn bind_row(ui: &mut Ui, ed: &mut Editor, ids: &[NodeId], bfield: &str, label: &str, kind: Kind, shown: &str) {
    let bound = super::bind::shared_binding(ed, ids, bfield);
    row(ui, label, |ui| match bound {
        Some(css) => {
            let w = ui.available_width();
            let act = super::bind::bound_chip(ui, ed, kind, &css, shown, w);
            super::bind::apply(ed, ids, bfield, act);
        }
        None => {
            if let Some(css) = super::bind::bind_button(ui, ed, kind) {
                super::bind::apply(ed, ids, bfield, Some(Some(css)));
            }
            ui.label(RichText::new("Bind to a variable").size(theme::SECONDARY).color(theme::pal(ui).tertiary));
        }
    });
}

fn fmt_num(v: f64) -> String {
    let s = format!("{v:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn color_button(ui: &mut Ui, ed: &mut Editor, c: Color) -> Option<Color> {
    let mut c2 = c32(c);
    let r = ui.color_edit_button_srgba(&mut c2);
    seal_on_release(ed, &r);
    (r.changed() && from32(c2) != c).then(|| from32(c2))
}

fn combo<T: PartialEq + Copy>(
    ui: &mut Ui,
    id: &str,
    current: T,
    options: &[(T, &str)],
    width: f32,
) -> Option<T> {
    let _ = id;
    super::kit::select(ui, current, options, width)
}

/// Sections are separated by space, not lines.
fn separator(ui: &mut Ui) {
    ui.add_space(14.0);
}

fn icon_btn(ui: &mut Ui, icon: theme::Icon, on: bool, tip: &str) -> bool {
    theme::icon_button(ui, icon, 24.0, on)
        .on_hover_text(tip)
        .clicked()
}

pub fn panel(ui: &mut Ui, ed: &mut Editor, st: &mut UiState) {
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            theme::figma_fields(ui);
            let ids = ed.top_selection();
            ui.horizontal(|ui| {
                if super::kit::chip(ui, "Design", !st.proto_tab).clicked() {
                    st.proto_tab = false;
                }
                if super::kit::chip(ui, "Prototype", st.proto_tab).clicked() {
                    st.proto_tab = true;
                }
            });
            ui.add_space(2.0);
            if st.proto_tab {
                prototype_tab(ui, ed, &ids);
                return;
            }
            if ids.is_empty() {
                nothing_selected(ui, ed);
                separator(ui);
                export_section(ui, ed, st);
                return;
            }
            let first = ed.doc.get(ids[0]).cloned().unwrap();
            header(ui, ed, &ids, &first);
            separator(ui);
            feedback_section(ui, ed, st, &ids, &first);
            align_row(ui, ed, &ids);
            separator(ui);
            geometry(ui, ed, &ids, &first);
            child_layout(ui, ed, &ids, &first);
            if first.is_frame() && ids.len() == 1 {
                auto_layout(ui, ed, &ids, &first);
            }
            if matches!(first.kind, NodeKind::Text { .. }) {
                separator(ui);
                text_section(ui, ed, &ids, &first);
            }
            if crate::render::draw::is_area(&first)
                && !matches!(first.kind, NodeKind::Stroke { .. })
            {
                separator(ui);
                fills(ui, ed, &ids, &first);
            }
            if !matches!(
                first.kind,
                NodeKind::Text { .. } | NodeKind::Stroke { .. } | NodeKind::Group
            ) {
                separator(ui);
                stroke(ui, ed, &ids, &first);
            }
            if matches!(first.kind, NodeKind::Connector { .. }) {
                separator(ui);
                arrow_section(ui, ed, &ids, &first);
            }
            if matches!(first.kind, NodeKind::Sticky { .. }) {
                separator(ui);
                sticky_section(ui, ed, &ids, &first);
            }
            if first.is_frame() || crate::render::draw::is_area(&first) {
                separator(ui);
                effects(ui, ed, &ids, &first);
            }
            separator(ui);
            layer(ui, ed, &ids, &first);
            if !matches!(
                first.kind,
                NodeKind::Text { .. }
                    | NodeKind::Stroke { .. }
                    | NodeKind::Group
                    | NodeKind::Sticky { .. }
            ) {
                separator(ui);
                sketch_section(ui, ed, &ids, &first);
            }
            separator(ui);
            export_section(ui, ed, st);
        });
}

/// Prototype tab: which top-level frame a click on the selected layer opens in Present mode.
fn prototype_tab(ui: &mut Ui, ed: &mut Editor, ids: &[NodeId]) {
    let Some(first) = ids.first().and_then(|id| ed.doc.get(*id)).cloned() else {
        ui.label(
            RichText::new("Select a layer to link it to a frame.").color(theme::pal(ui).secondary),
        );
        return;
    };
    let first = &first;
    section(ui, "Interaction");
    let frames: Vec<(Option<NodeId>, String)> = std::iter::once((None, "None".to_string()))
        .chain(ed.doc.root.iter().filter_map(|id| {
            let n = ed.doc.get(*id)?;
            (n.is_frame() && !n.component && *id != first.id).then(|| (Some(*id), n.name.clone()))
        }))
        .collect();
    let opts: Vec<(Option<NodeId>, &str)> = frames.iter().map(|(i, n)| (*i, n.as_str())).collect();
    row(ui, "On click", |ui| {
        if let Some(v) = combo(ui, "link", first.link, &opts, 140.0) {
            apply(ed, "link", ids, |n| n.link = v);
        }
    });
}

/// Notes on the selected layer: type one for an agent (it reads them with `get_feedback`), or
/// resolve the ones already there.
fn feedback_section(ui: &mut Ui, ed: &mut Editor, st: &mut UiState, ids: &[NodeId], first: &Node) {
    if ids.len() != 1 {
        return;
    }
    let id = ids[0];
    let p = theme::pal(ui);
    let open = first.open_comments();
    section(
        ui,
        &if open > 0 {
            format!("Feedback · {open} open")
        } else {
            "Feedback".to_string()
        },
    );
    for c in first.comments.clone() {
        ui.horizontal(|ui| {
            let color = if c.resolved { p.tertiary } else { p.text };
            ui.add(
                egui::Label::new(
                    RichText::new(format!("{}: {}", c.author, c.text))
                        .size(theme::SECONDARY + 1.0)
                        .color(color),
                )
                .wrap(),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if crate::theme::small_button(ui, "✕")
                    .on_hover_text("Delete")
                    .clicked()
                {
                    ed.edit("Delete feedback", |tx| {
                        if let Some(n) = tx.node_mut(id) {
                            n.comments.retain(|x| x.id != c.id);
                        }
                    });
                }
                if !c.resolved
                    && crate::theme::small_button(ui, "✓")
                        .on_hover_text("Resolve")
                        .clicked()
                {
                    ed.edit("Resolve feedback", |tx| {
                        if let Some(x) = tx
                            .node_mut(id)
                            .and_then(|n| n.comments.iter_mut().find(|x| x.id == c.id))
                        {
                            x.resolved = true;
                        }
                    });
                }
            });
        });
        if let Some(r) = &c.reply {
            ui.label(
                RichText::new(format!("  ↳ {r}"))
                    .size(theme::SECONDARY)
                    .color(p.secondary),
            );
        }
    }
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let add = crate::theme::small_button(ui, "Add").clicked();
            let r = ui.add(
                egui::TextEdit::singleline(&mut st.feedback_draft)
                    .hint_text("Note for the agent…")
                    .desired_width(ui.available_width())
                    .margin(crate::theme::INPUT_MARGIN),
            );
            let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if (add || enter) && !st.feedback_draft.trim().is_empty() {
                let text = std::mem::take(&mut st.feedback_draft).trim().to_string();
                ed.edit("Add feedback", |tx| {
                    if let Some(n) = tx.node_mut(id) {
                        let next = n.comments.iter().map(|c| c.id).max().unwrap_or(0) + 1;
                        n.comments.push(crate::model::Comment {
                            id: next,
                            text,
                            author: "user".into(),
                            resolved: false,
                            reply: None,
                        });
                    }
                });
            }
        });
    });
    separator(ui);
}

/// FigJam/Excalidraw-style sticky colours.
pub const STICKY_COLORS: [Color; 6] = [
    Color::rgb(0xff, 0xdf, 0x6b),
    Color::rgb(0xff, 0xb8, 0xa8),
    Color::rgb(0xb5, 0xe6, 0xa2),
    Color::rgb(0xa8, 0xd8, 0xff),
    Color::rgb(0xd7, 0xc2, 0xff),
    Color::rgb(0xe8, 0xe8, 0xec),
];

fn swatches(ui: &mut Ui, current: Color) -> Option<Color> {
    let mut out = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        for c in STICKY_COLORS {
            let (r, resp) = ui.allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::click());
            ui.painter().rect_filled(r, 5.0, c32(c));
            if c == current {
                ui.painter().rect_stroke(
                    r,
                    5.0,
                    egui::Stroke::new(2.0, theme::accent()),
                    egui::StrokeKind::Outside,
                );
            }
            if resp.clicked() {
                out = Some(c);
            }
        }
    });
    out
}

fn sticky_section(ui: &mut Ui, ed: &mut Editor, ids: &[NodeId], first: &Node) {
    section(ui, "Sticky note");
    let cur = first.style.fill().unwrap_or(STICKY_COLORS[0]);
    if let Some(c) = swatches(ui, cur) {
        apply(ed, "fill", ids, |n| n.style.set_fill(Some(c)));
        ed.sticky_color = c;
    }
    if let NodeKind::Sticky { font_size, .. } = first.kind {
        row(ui, "Font size", |ui| {
            if let Some(v) = num_field(ui, ed, "", font_size, 0.5, 8.0..=200.0, 70.0) {
                apply(ed, "font size", ids, |n| {
                    if let NodeKind::Sticky { font_size, .. } = &mut n.kind {
                        *font_size = v;
                    }
                });
            }
        });
    }
}

fn arrow_section(ui: &mut Ui, ed: &mut Editor, ids: &[NodeId], first: &Node) {
    use crate::model::{Head, Route};
    let NodeKind::Connector {
        route,
        start_head,
        end_head,
        start,
        end,
        ..
    } = &first.kind
    else {
        return;
    };
    section(ui, "Arrow");
    let routes = [
        (Route::Straight, "Straight"),
        (Route::Curved, "Curved"),
        (Route::Elbow, "Elbow"),
    ];
    row(ui, "Type", |ui| {
        if let Some(v) = combo(ui, "route", *route, &routes, 96.0) {
            apply(ed, "route", ids, |n| {
                if let NodeKind::Connector { route, points, .. } = &mut n.kind {
                    *route = v;
                    if v != Route::Elbow && points.len() > 2 && v == Route::Straight {
                        let (a, b) = (points[0], *points.last().unwrap());
                        *points = vec![a, b];
                    }
                }
            });
            ed.arrow_route = v;
            // Re-route through the binding logic.
            ed.begin("Change route");
            {
                let mut tx = ed.tx();
                for id in ids {
                    crate::editor::whiteboard::update_connector(&mut tx, *id);
                }
            }
            ed.commit();
        }
    });
    let heads: Vec<(Head, &str)> = Head::ALL.iter().map(|h| (*h, h.label())).collect();
    row(ui, "Heads", |ui| {
        if let Some(v) = combo(ui, "h-start", *start_head, &heads, 72.0) {
            apply(ed, "heads", ids, |n| {
                if let NodeKind::Connector { start_head, .. } = &mut n.kind {
                    *start_head = v;
                }
            });
        }
        if let Some(v) = combo(ui, "h-end", *end_head, &heads, 72.0) {
            apply(ed, "heads", ids, |n| {
                if let NodeKind::Connector { end_head, .. } = &mut n.kind {
                    *end_head = v;
                }
            });
        }
    });
    let bound = |b: &Option<crate::model::Binding>| {
        b.and_then(|b| ed.doc.get(b.node).map(|n| n.name.clone()))
            .unwrap_or_else(|| "—".into())
    };
    ui.label(
        RichText::new(format!("Connects {} → {}", bound(start), bound(end)))
            .size(theme::SECONDARY)
            .color(theme::pal(ui).secondary),
    );
}

fn sketch_section(ui: &mut Ui, ed: &mut Editor, ids: &[NodeId], first: &Node) {
    use crate::model::rough::{FillStyle, Rough};
    ui.horizontal(|ui| {
        section(ui, "Hand-drawn");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let mut on = first.style.rough.is_some();
            if super::kit::toggle(ui, &mut on)
                .on_hover_text("Sketchy rough.js-style lines (⌘⌥H)")
                .changed()
            {
                ed.toggle_sketch();
            }
        });
    });
    let Some(r) = first.style.rough else { return };
    let levels = [(0u8, "Architect"), (1, "Artist"), (2, "Cartoonist")];
    let cur = r.roughness.round().clamp(0.0, 2.0) as u8;
    row(ui, "Sloppiness", |ui| {
        if let Some(v) = combo(ui, "rough", cur, &levels, 96.0) {
            apply(ed, "rough", ids, |n| {
                if let Some(r) = &mut n.style.rough {
                    r.roughness = v as f64;
                }
            });
        }
    });
    let fills: Vec<(FillStyle, &str)> = FillStyle::ALL.iter().map(|f| (*f, f.label())).collect();
    row(ui, "Fill style", |ui| {
        if let Some(v) = combo(ui, "fillstyle", r.fill_style, &fills, 96.0) {
            apply(ed, "rough", ids, |n| {
                if let Some(r) = &mut n.style.rough {
                    r.fill_style = v;
                }
            });
        }
    });
    if crate::theme::small_button(ui, "New wobble")
        .on_hover_text("Re-roll the random seed")
        .clicked()
    {
        apply(ed, "rough", ids, |n| {
            if let Some(r) = &mut n.style.rough {
                *r = Rough {
                    seed: r.seed.wrapping_mul(1103515245).wrapping_add(12345) | 1,
                    ..*r
                };
            }
        });
    }
}

/// Export settings and button, like Figma's Export section.
/// Most export presets the inspector keeps.
const MAX_EXPORT_PRESETS: usize = 6;

fn export_section(ui: &mut Ui, ed: &mut Editor, st: &mut UiState) {
    use crate::io::export::{Format, Scope};
    if super::kit::section_header(
        ui,
        "Export",
        Some((theme::Icon::Plus, "Keep these export settings as a preset")),
    ) {
        let spec = st.export;
        if !st.export_presets.contains(&spec) && st.export_presets.len() < MAX_EXPORT_PRESETS {
            st.export_presets.push(spec);
        }
    }
    let mut remove = None;
    ui.horizontal_wrapped(|ui| {
        for (i, preset) in st.export_presets.iter().enumerate() {
            let label = match preset.format {
                Format::Png => format!("PNG {}x", preset.scale),
                Format::Svg => "SVG".into(),
                Format::Pdf => "PDF".into(),
            };
            let chip = super::kit::chip(ui, &label, *preset == st.export)
                .on_hover_text(format!("{} · right-click to remove", preset.scope.label()));
            if chip.clicked() {
                st.export = *preset;
            }
            chip.context_menu(|ui| {
                if super::icon_menu_item(ui, crate::theme::Icon::Trash, "Remove preset").clicked() {
                    remove = Some(i);
                    ui.close();
                }
            });
        }
    });
    if let Some(i) = remove {
        st.export_presets.remove(i);
    }
    let spec = &mut st.export;
    ui.horizontal(|ui| {
        let w = half(ui);
        let fmts: Vec<(Format, &str)> = Format::ALL
            .iter()
            .map(|f| {
                (
                    *f,
                    match f {
                        Format::Png => "PNG",
                        Format::Svg => "SVG",
                        Format::Pdf => "PDF",
                    },
                )
            })
            .collect();
        if let Some(v) = combo(ui, "exp-format", spec.format, &fmts, w) {
            spec.format = v;
        }
        if spec.format == Format::Png {
            let scales = [(1u8, "1x"), (2, "2x"), (3, "3x"), (4, "4x")];
            if let Some(v) = combo(ui, "exp-scale", spec.scale as u8, &scales, w) {
                spec.scale = v as f64;
            }
        }
    });
    let scopes: Vec<(Scope, &str)> = Scope::ALL.iter().map(|s| (*s, s.label())).collect();
    row(ui, "Export", |ui| {
        if let Some(v) = combo(ui, "exp-scope", spec.scope, &scopes, ui.available_width()) {
            spec.scope = v;
        }
    });
    if spec.format != Format::Svg || spec.transparent {
        super::kit::checkbox(ui, &mut spec.transparent, "Transparent background");
    }
    let what = match (spec.scope, ed.top_selection().len()) {
        (Scope::Selection, 0) => format!("page “{}”", ed.page_name()),
        (Scope::Selection, 1) => ed
            .doc
            .get(ed.top_selection()[0])
            .map_or("selection".into(), |n| format!("“{}”", n.name)),
        (Scope::Selection, n) => format!("{n} layers"),
        (Scope::Frames, _) => "each frame".into(),
        (Scope::Page, _) => format!("page “{}”", ed.page_name()),
        (Scope::AllPages, _) => format!("{} pages", ed.pages.len()),
    };
    if ui
        .add(
            egui::Button::new(format!("Export {what}…"))
                .min_size(egui::vec2(ui.available_width(), 24.0)),
        )
        .clicked()
    {
        ed.requests.push(AppRequest::Export(*spec));
    }
    ui.horizontal(|ui| {
        let w = half(ui);
        if ui
            .add(egui::Button::new("Copy as PNG").min_size(egui::vec2(w, 24.0)))
            .clicked()
        {
            ed.run(crate::editor::shortcuts::Action::CopyAsPng);
        }
        if ui
            .add(egui::Button::new("Copy as SVG").min_size(egui::vec2(w, 24.0)))
            .clicked()
        {
            ed.run(crate::editor::shortcuts::Action::CopyAsSvg);
        }
    });
    let root_id = ed
        .top_selection()
        .first()
        .copied()
        .or_else(|| ed.doc.root.first().copied());
    if let Some(root_id) = root_id {
        ui.add_space(6.0);
        section(ui, "Code & AI Handoff");
        if st.code_target.is_empty() {
            st.code_target = "react".into();
        }
        let targets = [
            ("react", "React (TSX)"),
            ("tailwind", "Tailwind JSX"),
            ("angular", "Angular"),
            ("html", "HTML + CSS"),
            ("egui", "Rust (egui)"),
        ];
        let cur = targets
            .iter()
            .find(|(k, _)| *k == st.code_target)
            .map_or("react", |(k, _)| *k);
        row(ui, "Target", |ui| {
            if let Some(v) = combo(ui, "code-target", cur, &targets, ui.available_width()) {
                st.code_target = v.into();
            }
        });
        ui.horizontal(|ui| {
            let w = half(ui);
            if ui
                .add(egui::Button::new("Copy Code").min_size(egui::vec2(w, 24.0)))
                .clicked()
                && let Ok(code) =
                    crate::mcp::codegen::code_for_target(&ed.doc, root_id, &st.code_target)
            {
                ui.ctx().copy_text(code);
                ed.status = format!("Copied {} code to clipboard", st.code_target);
            }
            if ui
                .add(egui::Button::new("Copy AI Spec").min_size(egui::vec2(w, 24.0)))
                .clicked()
                && let Ok(md) =
                    crate::mcp::codegen::to_ai_spec_markdown(&ed.doc, root_id, &st.code_target)
            {
                ui.ctx().copy_text(md);
                ed.status = format!("Copied {} AI spec to clipboard", st.code_target);
            }
        });
    }
}

fn nothing_selected(ui: &mut Ui, ed: &mut Editor) {
    section(ui, "New shapes");
    let mut sketch = ed.sketch.is_some();
    if super::kit::checkbox(ui, &mut sketch, "Hand-drawn style").changed() {
        ed.toggle_sketch();
    }
    let routes = [
        (crate::model::Route::Straight, "Straight"),
        (crate::model::Route::Curved, "Curved"),
        (crate::model::Route::Elbow, "Elbow"),
    ];
    row(ui, "Arrows", |ui| {
        if let Some(v) = combo(ui, "new-route", ed.arrow_route, &routes, 96.0) {
            ed.arrow_route = v;
        }
    });
    row(ui, "Sticky", |ui| {
        if let Some(c) = swatches(ui, ed.sticky_color) {
            ed.sticky_color = c;
        }
    });
    separator(ui);
    section(ui, "Pencil");
    row(ui, "Width", |ui| {
        let r = super::kit::slider(ui, &mut ed.ink_width, 0.5..=24.0);
        ui.label(format!("{:.1}", ed.ink_width));
        r
    });
    row(ui, "Smoothing", |ui| {
        let r = super::kit::slider(ui, &mut ed.ink_smoothing, 0.0..=1.0);
        ui.label(format!("{:.2}", ed.ink_smoothing));
        r
    });
    row(ui, "Color", |ui| {
        let mut c = c32(ed.ink_color);
        if ui.color_edit_button_srgba(&mut c).changed() {
            ed.ink_color = from32(c);
        }
    });
    separator(ui);
    section(ui, "Page");
    let mut name = ed.page_name().to_string();
    if ui
        .add(
            egui::TextEdit::singleline(&mut name)
                .margin(crate::theme::INPUT_MARGIN)
                .desired_width(f32::INFINITY),
        )
        .changed()
    {
        let i = ed.active_page;
        ed.rename_page(i, &name);
    }
    ui.label(
        RichText::new(match ed.doc.nodes.len() {
            1 => "1 layer".to_string(),
            n => format!("{n} layers"),
        })
        .size(theme::SECONDARY)
        .color(theme::pal(ui).secondary),
    );
}

fn header(ui: &mut Ui, ed: &mut Editor, ids: &[NodeId], first: &Node) {
    if ids.len() > 1 {
        ui.label(title_text(format!("{} layers", ids.len())));
        ui.horizontal_wrapped(|ui| {
            for op in [
                BoolOp::Union,
                BoolOp::Subtract,
                BoolOp::Intersect,
                BoolOp::Exclude,
            ] {
                if crate::theme::small_button(ui, op.label()).clicked() {
                    ed.boolean(op);
                }
            }
        });
        ui.horizontal(|ui| {
            if crate::theme::small_button(ui, "Group").clicked() {
                ed.group_selection();
            }
            if crate::theme::small_button(ui, "Frame").clicked() {
                ed.run(crate::editor::shortcuts::Action::WrapInFrame);
            }
            if crate::theme::small_button(ui, "Component").clicked() {
                ed.create_component();
            }
        });
        return;
    }
    let kind = if first.component {
        "Component"
    } else if ed.is_instance_root(first.id) {
        "Instance"
    } else {
        first.kind.type_name()
    };
    ui.label(title_text(format!("{kind} · {}", first.name)));
    let is_inst_root = ed.is_instance_root(first.id);
    let has_subtree_overrides = is_inst_root
        && ed.doc.subtree(first.id).into_iter().any(|id| {
            ed.doc
                .get(id)
                .and_then(|n| n.instance.as_ref())
                .is_some_and(|l| !l.overrides.is_empty())
        });
    let other_comps: Vec<(NodeId, String)> = if is_inst_root {
        let cur_main = ed.instance_main_root(first.id);
        let mut v: Vec<(NodeId, String)> = ed
            .doc
            .nodes
            .values()
            .filter(|n| n.component && Some(n.id) != cur_main && n.id != first.id)
            .map(|n| (n.id, n.name.clone()))
            .collect();
        v.sort_by_key(|c| c.0);
        v
    } else {
        Vec::new()
    };
    ui.horizontal_wrapped(|ui| {
        if first.component {
            if crate::theme::small_button(ui, "Create instance").clicked() {
                ed.instance_of_selection();
            }
        } else if is_inst_root {
            if crate::theme::small_button(ui, "Go to main").clicked() {
                ed.go_to_main();
            }
            if !other_comps.is_empty() {
                ui.menu_button("Swap", |ui| {
                    for (comp_id, cname) in &other_comps {
                        if ui.button(cname).clicked() {
                            ed.swap_instance(first.id, *comp_id);
                            ui.close();
                        }
                    }
                });
            }
            if has_subtree_overrides && crate::theme::small_button(ui, "Reset").clicked() {
                ed.reset_overrides();
            }
            if crate::theme::small_button(ui, "Detach").clicked() {
                ed.detach_instance();
            }
        } else if (first.is_frame() || first.is_group())
            && crate::theme::small_button(ui, "Create component").clicked()
        {
            ed.create_component();
        }
        if !is_inst_root
            && first.instance.is_some()
            && first
                .instance
                .as_ref()
                .is_some_and(|l| !l.overrides.is_empty())
            && crate::theme::small_button(ui, "Reset overrides").clicked()
        {
            ed.reset_overrides();
        }
        if first.is_container()
            && !first.children.is_empty()
            && crate::theme::small_button(ui, "Ungroup").clicked()
        {
            ed.ungroup_selection();
        }
        if !first.is_container()
            && !matches!(first.kind, NodeKind::Path { .. })
            && crate::theme::small_button(ui, "Flatten").clicked()
        {
            ed.flatten_selection();
        }
        if matches!(first.kind, NodeKind::Path { .. })
            && crate::theme::small_button(ui, "Edit points").clicked()
        {
            ed.start_path_edit(first.id);
        }
    });
    if let Some(l) = &first.instance
        && !l.overrides.is_empty()
    {
        ui.label(
            RichText::new(format!("Overrides: {}", l.overrides.join(", ")))
                .size(theme::SECONDARY)
                .color(theme::pal(ui).secondary),
        );
    }
}

fn align_row(ui: &mut Ui, ed: &mut Editor, _ids: &[NodeId]) {
    use theme::Icon as I;
    let aligns = [
        (I::AlignLeft, 0, 0.0, "Align left (⌥A)"),
        (I::AlignHCenter, 0, 0.5, "Align horizontal centers (⌥H)"),
        (I::AlignRight, 0, 1.0, "Align right (⌥D)"),
        (I::AlignTop, 1, 0.0, "Align top (⌥W)"),
        (I::AlignVCenter, 1, 0.5, "Align vertical centers (⌥V)"),
        (I::AlignBottom, 1, 1.0, "Align bottom (⌥S)"),
    ];
    ui.horizontal(|ui| {
        let unit = (ui.available_width() - ui.spacing().item_spacing.x) / 8.0;
        let cells: Vec<_> = aligns
            .iter()
            .map(|a| (Cell::Icon(a.0), false, a.3))
            .collect();
        if let Some(i) = segmented(ui, &cells, unit * 6.0) {
            ed.align(aligns[i].1, aligns[i].2);
        }
        let dist = [
            (
                Cell::Icon(I::DistributeH),
                false,
                "Distribute horizontal spacing (⌥⇧H)",
            ),
            (
                Cell::Icon(I::DistributeV),
                false,
                "Distribute vertical spacing (⌥⇧V)",
            ),
        ];
        if let Some(i) = segmented(ui, &dist, unit * 2.0) {
            ed.distribute(i);
        }
    });
}

fn geometry(ui: &mut Ui, ed: &mut Editor, ids: &[NodeId], first: &Node) {
    if ids.len() == 1 {
        let r = first.rect;
        ui.horizontal(|ui| {
            let w = half(ui);
            if let Some(v) = num_field(ui, ed, "X  ", r.x, 1.0, f64::MIN..=f64::MAX, w) {
                apply(ed, "x", ids, |n| n.rect.x = v);
            }
            if let Some(v) = num_field(ui, ed, "Y  ", r.y, 1.0, f64::MIN..=f64::MAX, w) {
                apply(ed, "y", ids, |n| n.rect.y = v);
            }
        });
        ui.horizontal(|ui| {
            let w = half(ui);
            if let Some(v) = bound_field(ui, ed, ids, "width", "W  ", "", r.w, 1.0, 0.0..=1e6, w) {
                resize(ed, ids[0], v2(v, r.h));
            }
            // Disabling the Ui itself (not `add_enabled_ui`, whose scope shifts the cell).
            if matches!(first.kind, NodeKind::Text { .. }) {
                ui.disable();
            }
            if let Some(v) = bound_field(ui, ed, ids, "height", "H  ", "", r.h, 1.0, 0.0..=1e6, w) {
                resize(ed, ids[0], v2(r.w, v));
            }
        });
    }
    // Shape parameters.
    match first.kind {
        NodeKind::Polygon { sides } => {
            row(ui, "Sides", |ui| {
                if let Some(v) = num_field(ui, ed, "", sides as f64, 0.1, 3.0..=60.0, 60.0) {
                    apply(ed, "sides", ids, |n| {
                        if let NodeKind::Polygon { sides } = &mut n.kind {
                            *sides = v.round() as u32;
                        }
                    });
                }
            });
        }
        NodeKind::Star { points, inner } => {
            ui.horizontal(|ui| {
                if let Some(v) = num_field(ui, ed, "Points ", points as f64, 0.1, 3.0..=60.0, 92.0)
                {
                    apply(ed, "points", ids, |n| {
                        if let NodeKind::Star { points, .. } = &mut n.kind {
                            *points = v.round() as u32;
                        }
                    });
                }
                if let Some(v) = num_field(ui, ed, "Ratio ", inner * 100.0, 0.5, 1.0..=100.0, 92.0)
                {
                    apply(ed, "ratio", ids, |n| {
                        if let NodeKind::Star { inner, .. } = &mut n.kind {
                            *inner = v / 100.0;
                        }
                    });
                }
            });
        }
        _ => {}
    }
    if matches!(
        first.kind,
        NodeKind::Rect | NodeKind::Frame { .. } | NodeKind::Polygon { .. } | NodeKind::Star { .. }
    ) {
        let per_corner = first.style.radii.is_some();
        ui.horizontal(|ui| {
            // The field(s) take the row, the corners button keeps its square.
            let sp = ui.spacing().item_spacing.x;
            let room = ui.available_width() - 32.0;
            if !per_corner {
                if let Some(v) = bound_field(
                    ui,
                    ed,
                    ids,
                    "radius",
                    "Radius  ",
                    "",
                    first.style.corner_radius,
                    0.5,
                    0.0..=1e4,
                    room.max(60.0),
                ) {
                    apply(ed, "radius", ids, |n| n.style.corner_radius = v);
                }
            } else {
                let rr = first.style.corner_radii();
                let cw = ((room - 3.0 * sp) / 4.0).max(36.0);
                for (i, p) in ["TL", "TR", "BR", "BL"].iter().enumerate() {
                    if let Some(v) = num_field(ui, ed, &format!("{p} "), rr[i], 0.5, 0.0..=1e4, cw)
                    {
                        apply(ed, "radius", ids, |n| {
                            let mut r = n.style.corner_radii();
                            r[i] = v;
                            n.style.radii = Some(r);
                        });
                    }
                }
            }
            if matches!(first.kind, NodeKind::Rect | NodeKind::Frame { .. })
                && icon_btn(ui, theme::Icon::Corners, per_corner, "Independent corners")
            {
                apply(ed, "radius", ids, |n| {
                    n.style.radii = if n.style.radii.is_some() {
                        None
                    } else {
                        Some([n.style.corner_radius; 4])
                    }
                });
            }
        });
    }
    if let NodeKind::Frame { clip } = first.kind {
        let mut c = clip;
        if super::kit::checkbox(ui, &mut c, "Clip content").changed() {
            apply(ed, "clip", ids, |n| n.kind = NodeKind::Frame { clip: c });
        }
    }
}

/// Resize through the layout engine so constraints and auto layout follow.
fn resize(ed: &mut Editor, id: NodeId, size: crate::geom::V2) {
    let key = format!("size:{id:?}");
    ed.begin_coalescing("Resize", Some(&key));
    {
        let mut tx = ed.tx();
        let tx = &mut tx;
        let old = tx.doc().get(id).unwrap().rect;
        let new = crate::geom::Rect::new(old.x, old.y, size.x.max(0.0), size.y.max(0.0));
        let n = tx.node_mut(id).unwrap();
        n.set_rect_scaling(new);
        if (new.w - old.w).abs() > 1e-9 {
            n.layout_child.sizing[0] = Sizing::Fixed;
        }
        if (new.h - old.h).abs() > 1e-9 {
            n.layout_child.sizing[1] = Sizing::Fixed;
        }
        crate::layout::resize_children(tx, id, old, new);
    }
    ed.mark_overrides(&[id], "size");
    ed.fit_text(id);
    ed.commit();
}

fn child_layout(ui: &mut Ui, ed: &mut Editor, ids: &[NodeId], first: &Node) {
    let parent = first.parent.and_then(|p| ed.doc.get(p)).cloned();
    let in_auto = parent.as_ref().is_some_and(|p| p.layout.is_some());
    let sizing_opts_for = |axis: usize| -> Vec<(Sizing, &'static str)> {
        let mut v = vec![(Sizing::Fixed, "Fixed")];
        if first.layout.is_some() || matches!(first.kind, NodeKind::Text { .. }) {
            v.push((Sizing::Hug, "Hug"));
        }
        if in_auto && !first.layout_child.absolute {
            v.push((Sizing::Fill, "Fill"));
        }
        let _ = axis;
        v
    };
    let show_sizing = first.layout.is_some() || (in_auto && !first.layout_child.absolute);
    if show_sizing {
        ui.horizontal(|ui| {
            let cell = half(ui);
            for (axis, label) in [(0usize, "W"), (1, "H")] {
                ui.label(RichText::new(label).color(theme::pal(ui).secondary));
                let opts = sizing_opts_for(axis);
                if let Some(v) = combo(
                    ui,
                    &format!("sizing{axis}"),
                    first.layout_child.sizing[axis],
                    &opts,
                    (cell - 24.0).max(50.0),
                ) {
                    apply(ed, "layout", ids, |n| {
                        n.layout_child.sizing[axis] = v;
                        if let NodeKind::Text { auto_width, .. } = &mut n.kind
                            && axis == 0
                        {
                            *auto_width = v == Sizing::Hug;
                        }
                    });
                }
            }
        });
    }
    if in_auto {
        let mut abs = first.layout_child.absolute;
        if super::kit::checkbox(ui, &mut abs, "Ignore auto layout").changed() {
            apply(ed, "layout", ids, |n| n.layout_child.absolute = abs);
        }
    }
    let constrained =
        parent.as_ref().is_some_and(|p| p.is_frame()) && (!in_auto || first.layout_child.absolute);
    if constrained {
        let opts_h = [
            (Constraint::Min, "Left"),
            (Constraint::Max, "Right"),
            (Constraint::Stretch, "Left & right"),
            (Constraint::Center, "Center"),
            (Constraint::Scale, "Scale"),
        ];
        let opts_v = [
            (Constraint::Min, "Top"),
            (Constraint::Max, "Bottom"),
            (Constraint::Stretch, "Top & bottom"),
            (Constraint::Center, "Center"),
            (Constraint::Scale, "Scale"),
        ];
        row(ui, "Constraints", |ui| {
            if let Some(v) = combo(ui, "con-h", first.layout_child.constraints.h, &opts_h, 74.0) {
                apply(ed, "layout", ids, |n| n.layout_child.constraints.h = v);
            }
            if let Some(v) = combo(ui, "con-v", first.layout_child.constraints.v, &opts_v, 74.0) {
                apply(ed, "layout", ids, |n| n.layout_child.constraints.v = v);
            }
        });
    }
}

fn auto_layout(ui: &mut Ui, ed: &mut Editor, ids: &[NodeId], first: &Node) {
    separator(ui);
    ui.horizontal(|ui| {
        section(ui, "Auto layout");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if first.layout.is_some() {
                if crate::theme::icon_button(ui, crate::theme::Icon::Minus, 24.0, false)
                    .on_hover_text("Remove auto layout (⌥⇧A)")
                    .clicked()
                {
                    ed.remove_auto_layout();
                }
            } else if crate::theme::icon_button(ui, crate::theme::Icon::Plus, 24.0, false)
                .on_hover_text("Add auto layout (⇧A)")
                .clicked()
            {
                ed.add_auto_layout();
            }
        });
    });
    let Some(l) = first.layout else { return };
    let set = |ed: &mut Editor, f: &dyn Fn(&mut AutoLayout)| {
        apply(ed, "layout", ids, |n| {
            if let Some(l) = &mut n.layout {
                f(l);
            }
        });
    };
    ui.horizontal(|ui| {
        if icon_btn(
            ui,
            theme::Icon::ArrowRight,
            l.direction == Direction::Horizontal,
            "Horizontal",
        ) {
            set(ed, &|l| l.direction = Direction::Horizontal);
        }
        if icon_btn(
            ui,
            theme::Icon::ArrowDown,
            l.direction == Direction::Vertical,
            "Vertical",
        ) {
            set(ed, &|l| l.direction = Direction::Vertical);
        }
        if icon_btn(ui, theme::Icon::Wrap, l.wrap, "Wrap") {
            set(ed, &|l| l.wrap = !l.wrap);
        }
        ui.add_space(6.0);
        ui.add_enabled_ui(!l.gap_auto, |ui| {
            let room = (ui.available_width() - 34.0).max(60.0);
            if let Some(v) = bound_field(ui, ed, ids, "gap", "Gap ", "", l.gap, 0.5, -1e4..=1e4, room) {
                set(ed, &|l| l.gap = v);
            }
        });
        if icon_btn(ui, theme::Icon::SpaceBetween, l.gap_auto, "Space between") {
            set(ed, &|l| l.gap_auto = !l.gap_auto);
        }
    });
    ui.horizontal(|ui| {
        let cw = ((ui.available_width() - 3.0 * ui.spacing().item_spacing.x) / 4.0).max(36.0);
        for (i, p) in ["T ", "R ", "B ", "L "].iter().enumerate() {
            let side = ["padding-top", "padding-right", "padding-bottom", "padding-left"][i];
            if let Some(v) = bound_field(ui, ed, ids, side, p, "", l.padding[i], 0.5, 0.0..=1e4, cw) {
                set(ed, &|l| l.padding[i] = v);
            }
        }
    });
    let opts = [
        (Align::Start, "Start"),
        (Align::Center, "Center"),
        (Align::End, "End"),
    ];
    row(ui, "Align", |ui| {
        let cw = half(ui).max(60.0);
        if let Some(v) = combo(ui, "al-main", l.align_main, &opts, cw) {
            set(ed, &|l| l.align_main = v);
        }
        if let Some(v) = combo(ui, "al-cross", l.align_cross, &opts, cw) {
            set(ed, &|l| l.align_cross = v);
        }
    });
}

fn text_section(ui: &mut Ui, ed: &mut Editor, ids: &[NodeId], first: &Node) {
    let NodeKind::Text {
        font_size, align, ..
    } = &first.kind
    else {
        return;
    };
    let editing_range = ed.text_selection().is_some();
    ui.horizontal(|ui| {
        section(ui, "Text");
        if editing_range {
            ui.label(
                RichText::new("selection")
                    .size(theme::SECONDARY)
                    .color(theme::pal(ui).secondary),
            );
        }
    });
    let cs = ed.current_char_style();
    ui.horizontal(|ui| {
        let size = cs.map_or(*font_size, |c| c.size);
        let w = (ui.available_width() - 3.0 * ui.spacing().item_spacing.x) / 4.0;
        if let Some(v) = num_field(ui, ed, "Size ", size, 0.5, 1.0..=1000.0, w * 1.5) {
            ed.style_text("font size", |s| s.size = Some(v));
        }
        let c = cs.map(|c| c.color).unwrap_or(Color::INK);
        if let Some(nc) = color_button(ui, ed, c) {
            ed.style_text("color", |s| s.color = Some(nc));
        }
        let styles = [
            (Cell::Text("B"), cs.is_some_and(|c| c.bold), "Bold (⌘B)"),
            (Cell::Text("I"), cs.is_some_and(|c| c.italic), "Italic (⌘I)"),
            (
                Cell::Text("U"),
                cs.is_some_and(|c| c.underline),
                "Underline (⌘U)",
            ),
            (
                Cell::Text("S"),
                cs.is_some_and(|c| c.strike),
                "Strikethrough (⌘⇧X)",
            ),
        ];
        match segmented(ui, &styles, ui.available_width()) {
            Some(0) => ed.toggle_bold(),
            Some(1) => ed.toggle_italic(),
            Some(2) => ed.toggle_underline(),
            Some(3) => ed.toggle_strike(),
            _ => {}
        }
    });
    ui.horizontal(|ui| {
        let w = (ui.available_width() - 2.0 * ui.spacing().item_spacing.x) / 5.0;
        let aligns = [TextAlign::Left, TextAlign::Center, TextAlign::Right];
        let cells = [
            (
                Cell::Icon(theme::Icon::AlignLeft),
                *align == aligns[0],
                "Align text left",
            ),
            (
                Cell::Icon(theme::Icon::AlignHCenter),
                *align == aligns[1],
                "Align text center",
            ),
            (
                Cell::Icon(theme::Icon::AlignRight),
                *align == aligns[2],
                "Align text right",
            ),
        ];
        if let Some(i) = segmented(ui, &cells, w * 2.0) {
            apply(ed, "align", ids, |n| {
                if let NodeKind::Text { align, .. } = &mut n.kind {
                    *align = aligns[i];
                }
            });
        }
        let lh = first.text_style.line_height.unwrap_or(0.0) * 100.0;
        if let Some(v) = num_field(ui, ed, "LH ", lh, 1.0, 0.0..=1000.0, w) {
            apply(ed, "line height", ids, |n| {
                n.text_style.line_height = (v > 0.0).then_some(v / 100.0)
            });
        }
        if let Some(v) = num_field(
            ui,
            ed,
            "LS ",
            first.text_style.letter_spacing,
            0.1,
            -50.0..=200.0,
            w,
        ) {
            apply(ed, "letter spacing", ids, |n| {
                n.text_style.letter_spacing = v
            });
        }
    });
    bind_row(ui, ed, ids, "font-size", "Size", Kind::Number, &fmt_num(*font_size));
    bind_row(ui, ed, ids, "line-height", "Line height", Kind::Number, &fmt_num(first.text_style.line_height.unwrap_or(0.0)));
    bind_row(ui, ed, ids, "text-color", "Color", Kind::Color, "");
    if ids.len() == 1 && ed.text_edit.is_none() {
        let NodeKind::Text { text, .. } = &first.kind else {
            return;
        };
        let mut t = text.clone();
        let r = ui.add(
            egui::TextEdit::multiline(&mut t)
                .margin(crate::theme::INPUT_MARGIN)
                .desired_rows(2)
                .desired_width(f32::INFINITY),
        );
        if r.changed() {
            apply(ed, "text", ids, |n| {
                if let NodeKind::Text { text, .. } = &mut n.kind {
                    *text = t.clone();
                }
                n.text_style.spans.retain(|s| s.end <= t.chars().count());
            });
        }
        seal_on_release(ed, &r);
    }
}

fn fills(ui: &mut Ui, ed: &mut Editor, ids: &[NodeId], first: &Node) {
    let fills = first.style.fills.clone();
    ui.horizontal(|ui| {
        section(ui, "Fill");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if crate::theme::icon_button(ui, crate::theme::Icon::Plus, 24.0, false)
                .on_hover_text("Add fill")
                .clicked()
            {
                let c = fills
                    .last()
                    .map_or(Color::rgb(0xd9, 0xd9, 0xd9), |p| p.preview_color());
                apply(ed, "fills", ids, |n| n.style.fills.push(Paint::solid(c)));
            }
        });
    });
    // Top fill first, as in Figma.
    for i in (0..fills.len()).rev() {
        let p = fills[i].clone();
        ui.push_id(i, |ui| fill_row(ui, ed, ids, i, &p));
    }
}

fn fill_row(ui: &mut Ui, ed: &mut Editor, ids: &[NodeId], i: usize, p: &Paint) {
    let set = |ed: &mut Editor, np: Paint| {
        apply(ed, "fills", ids, |n| {
            if let Some(slot) = n.style.fills.get_mut(i) {
                *slot = np.clone();
            }
        });
    };
    ui.horizontal(|ui| {
        let kinds = [("Solid", 0u8), ("Linear", 1), ("Radial", 2), ("Image", 3)];
        let cur = match p {
            Paint::Solid { .. } => 0,
            Paint::Linear { .. } => 1,
            Paint::Radial { .. } => 2,
            Paint::Image { .. } => 3,
        };
        let opts: Vec<(u8, &str)> = kinds.iter().map(|(l, k)| (*k, *l)).collect();
        if let Some(k) = combo(ui, "kind", cur, &opts, 62.0) {
            let c = p.preview_color();
            let np = match k {
                0 => Paint::solid(c),
                1 => Paint::linear(c, Color::rgba(c.r, c.g, c.b, 0)),
                2 => Paint::radial(c, Color::rgba(c.r, c.g, c.b, 0)),
                _ => {
                    ed.requests.push(AppRequest::ImageFill(ids.to_vec(), i));
                    return;
                }
            };
            set(ed, np);
            return;
        }
        match p {
            Paint::Solid { color, var } => {
                let spacing = ui.spacing().item_spacing.x;
                let bind_w = if var.is_some() { 0.0 } else { BIND_W + spacing };
                let w = ui.available_width() - bind_w - 24.0 - spacing;
                let mut detach = false;
                if let Some(c) = color_field(ui, ed, *color, var.as_deref(), &mut detach, w.max(40.0)) {
                    set(ed, Paint::solid(c));
                }
                let mut picked = None;
                if detach {
                    let mut unlinked = p.clone();
                    unlinked.unlink_var();
                    set(ed, unlinked);
                } else if var.is_none() {
                    picked = super::bind::bind_button(ui, ed, Kind::Color);
                }
                if let Some(css) = picked {
                    let c = ed.doc.variables.get(&css).copied().unwrap_or(*color);
                    set(ed, Paint::solid_var(c, &css));
                }
            }
            Paint::Image {
                mode,
                opacity,
                asset,
            } => {
                let modes = [
                    (ImageMode::Fill, "Fill"),
                    (ImageMode::Fit, "Fit"),
                    (ImageMode::Stretch, "Stretch"),
                    (ImageMode::Tile, "Tile"),
                ];
                if let Some(m) = combo(ui, "mode", *mode, &modes, 62.0) {
                    set(
                        ed,
                        Paint::Image {
                            asset: asset.clone(),
                            mode: m,
                            opacity: *opacity,
                        },
                    );
                }
                if crate::theme::small_button(ui, "Replace…").clicked() {
                    ed.requests.push(AppRequest::ImageFill(ids.to_vec(), i));
                }
            }
            _ => {}
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if crate::theme::icon_button(ui, crate::theme::Icon::Minus, 24.0, false)
                .on_hover_text("Remove fill")
                .clicked()
            {
                apply(ed, "fills", ids, |n| {
                    if i < n.style.fills.len() {
                        n.style.fills.remove(i);
                    }
                });
            }
        });
    });
    // Gradient stops and geometry.
    if let Paint::Linear { stops, from, to }
    | Paint::Radial {
        stops,
        center: from,
        radius: to,
    } = p
    {
        let linear = matches!(p, Paint::Linear { .. });
        let rebuild = |stops: Vec<Stop>, a: crate::geom::V2, b: crate::geom::V2| {
            if linear {
                Paint::Linear {
                    stops,
                    from: a,
                    to: b,
                }
            } else {
                Paint::Radial {
                    stops,
                    center: a,
                    radius: b,
                }
            }
        };
        for (si, s) in stops.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.add_space(LABEL_W * 0.5);
                if let Some(c) = color_button(ui, ed, s.color) {
                    let mut ns = stops.clone();
                    ns[si].color = c;
                    set(ed, rebuild(ns, *from, *to));
                }
                if let Some(v) = num_field(ui, ed, "", s.pos * 100.0, 0.5, 0.0..=100.0, 56.0) {
                    let mut ns = stops.clone();
                    ns[si].pos = v / 100.0;
                    set(ed, rebuild(ns, *from, *to));
                }
                ui.label(RichText::new("%").weak());
                if stops.len() > 2
                    && crate::theme::icon_button(ui, crate::theme::Icon::Minus, 24.0, false)
                        .clicked()
                {
                    let mut ns = stops.clone();
                    ns.remove(si);
                    set(ed, rebuild(ns, *from, *to));
                }
            });
        }
        ui.horizontal(|ui| {
            ui.add_space(LABEL_W * 0.5);
            if crate::theme::small_button(ui, "+ stop").clicked() {
                let mut ns = stops.clone();
                let c = ns.last().map_or(Color::BLACK, |s| s.color);
                ns.push(Stop { pos: 0.5, color: c });
                ns.sort_by(|a, b| a.pos.total_cmp(&b.pos));
                set(ed, rebuild(ns, *from, *to));
            }
            if linear {
                let d = *to - *from;
                let ang = d.y.atan2(d.x).to_degrees();
                if let Some(v) = num_field(ui, ed, "Angle ", ang, 1.0, -360.0..=360.0, 70.0) {
                    let a = v.to_radians();
                    let half = v2(a.cos(), a.sin()) * 0.5;
                    let c = v2(0.5, 0.5);
                    set(ed, rebuild(stops.clone(), c - half, c + half));
                }
            } else if let Some(v) = num_field(ui, ed, "R ", to.x * 100.0, 0.5, 1.0..=400.0, 70.0) {
                set(ed, rebuild(stops.clone(), *from, v2(v / 100.0, v / 100.0)));
            }
        });
    }
}

fn stroke(ui: &mut Ui, ed: &mut Editor, ids: &[NodeId], first: &Node) {
    let s = &first.style;
    ui.horizontal(|ui| {
        section(ui, "Stroke");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if s.stroke.is_some() {
                if crate::theme::icon_button(ui, crate::theme::Icon::Minus, 24.0, false).clicked() {
                    if super::bind::shared_binding(ed, ids, "stroke").is_some() {
                        super::bind::apply(ed, ids, "stroke", Some(None));
                    }
                    apply(ed, "stroke", ids, |n| n.style.stroke = None);
                }
            } else if crate::theme::icon_button(ui, crate::theme::Icon::Plus, 24.0, false).clicked()
            {
                apply(ed, "stroke", ids, |n| n.style.stroke = Some(Color::INK));
            }
        });
    });
    let Some(c) = s.stroke else { return };
    let full = ui.available_width();
    let bound = super::bind::shared_binding(ed, ids, "stroke");
    let mut detach = false;
    let w = if bound.is_some() { full } else { full - BIND_W - ui.spacing().item_spacing.x };
    ui.horizontal(|ui| {
        if let Some(nc) = color_field(ui, ed, c, bound.as_deref(), &mut detach, w) {
            apply(ed, "stroke", ids, |n| n.style.stroke = Some(nc));
        }
        if detach {
            super::bind::apply(ed, ids, "stroke", Some(None));
        } else if bound.is_none()
            && let Some(css) = super::bind::bind_button(ui, ed, Kind::Color)
        {
            super::bind::apply(ed, ids, "stroke", Some(Some(css)));
        }
    });
    ui.horizontal(|ui| {
        let w = half(ui);
        if let Some(v) = num_field(ui, ed, "W ", s.stroke_width, 0.1, 0.0..=500.0, w) {
            apply(ed, "stroke width", ids, |n| n.style.stroke_width = v);
        }
        let al = [
            (StrokeAlign::Inside, "Inside"),
            (StrokeAlign::Center, "Center"),
            (StrokeAlign::Outside, "Outside"),
        ];
        if let Some(v) = combo(ui, "salign", s.stroke_align, &al, w) {
            apply(ed, "stroke align", ids, |n| n.style.stroke_align = v);
        }
    });
    ui.horizontal(|ui| {
        let w = third(ui);
        let dash_kind = match s.dash.as_slice() {
            [] => 0u8,
            [a, _] if *a <= 2.0 => 2,
            _ => 1,
        };
        let dashes = [(0u8, "Solid"), (1, "Dashed"), (2, "Dotted")];
        if let Some(k) = combo(ui, "dash", dash_kind, &dashes, w) {
            let w = s.stroke_width.max(1.0);
            apply(ed, "dash", ids, |n| {
                n.style.dash = match k {
                    0 => vec![],
                    1 => vec![w * 4.0, w * 3.0],
                    _ => vec![w.min(2.0), w * 2.5],
                };
                if k == 2 {
                    n.style.cap = Cap::Round;
                }
            });
        }
        let caps = [
            (Cap::Butt, "Butt"),
            (Cap::Round, "Round"),
            (Cap::Square, "Square"),
        ];
        if let Some(v) = combo(ui, "cap", s.cap, &caps, w) {
            apply(ed, "cap", ids, |n| n.style.cap = v);
        }
        let joins = [
            (Join::Miter, "Miter"),
            (Join::Round, "Round"),
            (Join::Bevel, "Bevel"),
        ];
        if let Some(v) = combo(ui, "join", s.join, &joins, w) {
            apply(ed, "join", ids, |n| n.style.join = v);
        }
    });
}

/// Drop shadow: one effect, as the model keeps a single `shadow` per box.
fn effects(ui: &mut Ui, ed: &mut Editor, ids: &[NodeId], first: &Node) {
    let shadow = first.style.shadow;
    ui.horizontal(|ui| {
        section(ui, "Effects");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let (icon, tip) = match shadow {
                Some(_) => (crate::theme::Icon::Minus, "Remove drop shadow"),
                None => (crate::theme::Icon::Plus, "Add drop shadow"),
            };
            if crate::theme::icon_button(ui, icon, 24.0, false)
                .on_hover_text(tip)
                .clicked()
            {
                let next = shadow.is_none().then_some(crate::model::Shadow {
                    x: 0.0,
                    y: 4.0,
                    blur: 24.0,
                    spread: 0.0,
                    color: Color::rgba(0, 0, 0, 102),
                });
                apply(ed, "shadow", ids, |n| n.style.shadow = next);
            }
        });
    });
    let Some(sh) = shadow else { return };
    let w = half(ui);
    let set = |ed: &mut Editor, f: fn(&mut crate::model::Shadow, f64), v: f64| {
        apply(ed, "shadow", ids, |n| {
            if let Some(s) = &mut n.style.shadow {
                f(s, v);
            }
        });
    };
    ui.horizontal(|ui| {
        if let Some(v) = num_field(ui, ed, "X", sh.x, 0.5, -500.0..=500.0, w) {
            set(ed, |s, v| s.x = v, v);
        }
        if let Some(v) = num_field(ui, ed, "Y", sh.y, 0.5, -500.0..=500.0, w) {
            set(ed, |s, v| s.y = v, v);
        }
    });
    ui.horizontal(|ui| {
        if let Some(v) = num_field(ui, ed, "Blur", sh.blur, 0.5, 0.0..=500.0, w) {
            set(ed, |s, v| s.blur = v, v);
        }
        if let Some(v) = num_field(ui, ed, "Spread", sh.spread, 0.5, -500.0..=500.0, w) {
            set(ed, |s, v| s.spread = v, v);
        }
    });
}

fn layer(ui: &mut Ui, ed: &mut Editor, ids: &[NodeId], first: &Node) {
    section(ui, "Layer");
    ui.horizontal(|ui| {
        let w = half(ui);
        let opts: Vec<(BlendMode, &str)> = BlendMode::ALL.iter().map(|b| (*b, b.label())).collect();
        if let Some(v) = combo(ui, "blend", first.style.blend, &opts, w) {
            apply(ed, "blend", ids, |n| n.style.blend = v);
        }
        let op = first.style.opacity * 100.0;
        if let Some(v) = bound_field(ui, ed, ids, "opacity", "", "%", op, 0.5, 0.0..=100.0, w) {
            apply(ed, "opacity", ids, |n| n.style.opacity = v / 100.0);
        }
    });
    bind_row(ui, ed, ids, "visible", "Visible", Kind::Boolean, &first.visible.to_string());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bound_ed() -> (Editor, NodeId, NodeId) {
        let mut ed = Editor::default();
        ed.edit_design("t", |ds| {
            let c = ds.add_collection("Tokens");
            ds.add_variable(&c, "space/md", Kind::Number)?;
            Ok(())
        })
        .unwrap();
        let mk = |ed: &mut Editor, bind: bool| {
            let id = ed.add_node(
                Node::new(NodeId(0), NodeKind::Frame { clip: false }, crate::geom::Rect::new(0.0, 0.0, 50.0, 30.0)),
                None,
            );
            if bind {
                ed.bind(&[id], "radius", Some("space-md")).unwrap();
            }
            id
        };
        let (a, b) = (mk(&mut ed, true), mk(&mut ed, false));
        (ed, a, b)
    }

    #[test]
    fn inspector_draws_bound_and_unbound_node() {
        for pick in [0, 1] {
            let ctx = egui::Context::default();
            super::super::setup_style(&ctx);
            let (mut ed, a, b) = bound_ed();
            ed.selection = vec![[a, b][pick]];
            let mut st = UiState::default();
            // Two frames: a bound chip must not detach itself while idle.
            let mut out = ctx.run_ui(Default::default(), |ui| panel(ui, &mut ed, &mut st));
            out.textures_delta.clear();
            let mut out = ctx.run_ui(Default::default(), |ui| panel(ui, &mut ed, &mut st));
            out.textures_delta.clear();
            let chip = out.shapes.iter().any(|s| {
                matches!(&s.shape, egui::epaint::Shape::Text(t) if t.galley.text().contains("space/md"))
            });
            assert_eq!(chip, pick == 0, "chip shown only for the bound node");
        }
    }

    #[test]
    fn bound_fill_row_fits_narrow_inspector() {
        for width in [192.0, 232.0, 392.0] {
            let ctx = egui::Context::default();
            super::super::setup_style(&ctx);
            let mut ed = Editor::default();
            let mut row_width = 0.0;
            let mut output = ctx.run_ui(Default::default(), |ui| {
                ui.set_width(width);
                theme::figma_fields(ui);
                let before = ui.min_rect().left();
                fill_row(
                    ui,
                    &mut ed,
                    &[],
                    0,
                    &Paint::solid_var(Color::BLACK, "surface-2-with-a-long-name"),
                );
                row_width = ui.min_rect().right() - before;
            });
            output.textures_delta.clear();
            assert!(
                row_width <= width + 0.1,
                "fill row {row_width} overflows inspector {width}"
            );
        }
    }
}
