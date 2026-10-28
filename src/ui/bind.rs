//! Variable bindings in the inspector: one picker (grouped by collection then group, filtered by
//! kind, with search), the bind button, and the chip a bound field shows instead of its input.

use crate::editor::Editor;
use crate::model::{Color, NodeId};
use crate::theme::{self, Icon};
use crate::variables::{self, DesignSystem, Kind, PageModes, Value};
use egui::{Align2, Rect, RichText, Sense, Ui, vec2};

pub const BIND_W: f32 = 20.0;
const POPUP_W: f32 = 280.0;
const ROW_H: f32 = 28.0;
const LIST_MAX_H: f32 = 300.0;

pub struct PickItem {
    pub css: String,
    /// Name inside its group.
    pub name: String,
    /// Current value as text (hex, number, true/false).
    pub value: String,
    pub color: Option<Color>,
}

pub struct PickGroup {
    pub collection: String,
    pub library: bool,
    /// "" = variables with no group.
    pub group: String,
    pub items: Vec<PickItem>,
}

fn value_text(v: &Value) -> (String, Option<Color>) {
    match v {
        Value::Color(c) => (c.to_hex(), Some(*c)),
        Value::Number(n) => (format!("{}", (n * 100.0).round() / 100.0), None),
        Value::Boolean(b) => (b.to_string(), None),
        Value::String(s) => (s.clone(), None),
        Value::Alias(_) => (String::new(), None),
    }
}

/// Variables of `kind` whose name matches `query`, grouped by collection then group.
pub fn pick_list(ds: &DesignSystem, modes: &PageModes, kind: Kind, query: &str) -> Vec<PickGroup> {
    let needle = query.trim().to_lowercase();
    let mut out = Vec::new();
    for c in &ds.collections {
        let mut groups: std::collections::BTreeMap<&str, Vec<PickItem>> = Default::default();
        for v in c.variables.iter().filter(|v| {
            v.kind == kind && (needle.is_empty() || v.name.to_lowercase().contains(&needle))
        }) {
            let group = variables::group_of(&v.name);
            let (value, color) = ds.resolve(&v.id, modes).map(|x| value_text(&x)).unwrap_or_default();
            groups.entry(group).or_default().push(PickItem {
                css: variables::css_name(v),
                name: v.name.strip_prefix(group).map_or(&*v.name, |n| n.trim_start_matches('/')).into(),
                value,
                color,
            });
        }
        out.extend(groups.into_iter().map(|(g, items)| PickGroup {
            collection: c.name.clone(),
            library: c.library.is_some(),
            group: g.into(),
            items,
        }));
    }
    out
}

fn kind_label(kind: Kind) -> &'static str {
    match kind {
        Kind::Color => "color",
        Kind::Number => "number",
        Kind::Boolean => "boolean",
        Kind::String => "text",
    }
}

/// Display name (`group/name`) of the variable with CSS name `css`.
pub fn display_name(ds: &DesignSystem, css: &str) -> String {
    ds.find_by_css(css).map_or_else(|| css.to_string(), |(_, v)| v.name.clone())
}

/// The popup body: search plus the grouped list. Returns the picked CSS name.
fn picker(ui: &mut Ui, ed: &Editor, kind: Kind, current: Option<&str>) -> Option<String> {
    let p = theme::pal(ui);
    let id = ui.make_persistent_id("bind-search");
    let mut query = ui.data(|d| d.get_temp::<String>(id)).unwrap_or_default();
    ui.set_width(POPUP_W);
    crate::theme::lit(ui, |ui| ui.add(
        egui::TextEdit::singleline(&mut query)
            .hint_text(format!("Search {} variables", kind_label(kind)))
            .desired_width(f32::INFINITY)
            .margin(theme::INPUT_MARGIN),
    ));
    ui.data_mut(|d| d.insert_temp(id, query.clone()));
    let list = pick_list(&ed.design, &ed.pages[ed.active_page].modes, kind, &query);
    let count: usize = list.iter().map(|g| g.items.len()).sum();
    ui.label(
        RichText::new(format!("{count} {} variables", kind_label(kind)))
            .size(theme::SECONDARY)
            .color(p.tertiary),
    );
    let mut picked = None;
    egui::ScrollArea::vertical().max_height(LIST_MAX_H).show(ui, |ui| {
        if list.is_empty() {
            ui.label(RichText::new("No variables of this type").color(p.secondary));
        }
        let mut last: Option<&str> = None;
        for g in &list {
            if g.library && last != Some(g.collection.as_str()) {
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(12.0, 14.0), Sense::hover());
                    theme::paint_icon(ui.painter(), r, Icon::Lock, p.secondary);
                    ui.label(RichText::new(&g.collection).font(theme::semibold(theme::SECONDARY)).color(p.secondary));
                });
            }
            last = Some(g.collection.as_str());
            if !g.group.is_empty() {
                ui.label(RichText::new(&g.group).font(theme::semibold(theme::SECONDARY)).color(p.tertiary));
            }
            for it in &g.items {
                let (rect, r) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click());
                let on = current == Some(it.css.as_str());
                let hover = crate::ui::motion::hover(ui.ctx(), r.id.with("pick"), &r);
                let fill = if on { p.field } else { p.hover.gamma_multiply(hover) };
                ui.painter().rect_filled(rect, theme::radius_md(), fill);
                let mark = Rect::from_center_size(rect.left_center() + vec2(14.0, 0.0), vec2(14.0, 14.0));
                match it.color {
                    Some(c) => {
                        ui.painter().rect_filled(mark, crate::theme::r(3.0), super::c32(c));
                    }
                    None => {
                        ui.painter().text(mark.center(), Align2::CENTER_CENTER, "#", theme::semibold(theme::BODY), p.secondary);
                    }
                }
                ui.painter().text(
                    rect.left_center() + vec2(30.0, 0.0),
                    Align2::LEFT_CENTER,
                    &it.name,
                    if on { theme::semibold(theme::BODY) } else { egui::FontId::proportional(theme::BODY) },
                    p.text,
                );
                ui.painter().text(
                    rect.right_center() - vec2(if on { 28.0 } else { 10.0 }, 0.0),
                    Align2::RIGHT_CENTER,
                    &it.value,
                    egui::FontId::proportional(theme::SECONDARY),
                    p.tertiary,
                );
                if on {
                    ui.painter().text(rect.right_center() - vec2(12.0, 0.0), Align2::CENTER_CENTER, "✓", egui::FontId::proportional(theme::BODY), theme::accent());
                }
                if r.clicked() {
                    picked = Some(it.css.clone());
                    ui.close();
                }
            }
        }
    });
    picked
}

fn open_picker(resp: &egui::Response, ed: &Editor, kind: Kind, current: Option<&str>) -> Option<String> {
    egui::Popup::menu(resp).gap(crate::theme::POPUP_GAP)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .width(POPUP_W)
        .show(|ui| {
            crate::theme::popup_backing(ui);
            picker(ui, ed, kind, current)
        })
        .and_then(|r| r.inner)
}

/// The bind icon button next to an unbound field. Returns the picked CSS name.
pub fn bind_button(ui: &mut Ui, ed: &Editor, kind: Kind) -> Option<String> {
    let r = theme::icon_button(ui, Icon::Diamond, BIND_W, false).on_hover_text("Bind to a variable");
    open_picker(&r, ed, kind, None)
}

/// A painted detach cross; true when clicked.
pub fn cross(ui: &mut Ui, rect: Rect) -> bool {
    let r = ui.interact(rect, ui.id().with("detach"), Sense::click()).on_hover_text("Detach variable");
    let c = rect.center();
    let ink = if r.hovered() { theme::pal(ui).text } else { theme::pal(ui).secondary };
    for (a, b) in [((-3.5, -3.5), (3.5, 3.5)), ((-3.5, 3.5), (3.5, -3.5))] {
        ui.painter().line_segment([c + vec2(a.0, a.1), c + vec2(b.0, b.1)], egui::Stroke::new(1.4, ink));
    }
    r.clicked()
}

/// What a bound field's chip asks for: `Some(None)` detach, `Some(Some(css))` rebind.
pub type ChipAction = Option<Option<String>>;

/// The field box of a bound field: a chip with the variable's `group/name`, the current value,
/// and a detach cross. Clicking the chip opens the picker to switch variable.
pub fn bound_chip(ui: &mut Ui, ed: &Editor, kind: Kind, css: &str, value: &str, width: f32) -> ChipAction {
    let p = theme::pal(ui);
    let (rect, resp) = ui.allocate_exact_size(vec2(width, theme::FIELD_H), Sense::click());
    theme::fill_control(ui, rect, theme::radius_md(), ui.visuals().widgets.inactive.bg_fill);
    let x = Rect::from_center_size(rect.right_center() - vec2(14.0, 0.0), vec2(16.0, 16.0));
    let detach = cross(ui, x);
    let mut right = x.left() - 4.0;
    if !value.is_empty() {
        let g = ui.painter().layout_no_wrap(value.into(), egui::FontId::proportional(theme::SECONDARY + 1.0), p.secondary);
        ui.painter().galley(egui::pos2(right - g.size().x, rect.center().y - g.size().y / 2.0), g.clone(), p.secondary);
        right -= g.size().x + 6.0;
    }
    let label = format!("{}{}", if kind == Kind::Number { "# " } else { "" }, display_name(&ed.design, css));
    let g = ui.painter().layout_no_wrap(label, theme::semibold(theme::SECONDARY + 1.0), theme::accent());
    let chip = Rect::from_min_max(
        rect.left_top() + vec2(4.0, 4.0),
        egui::pos2((rect.left() + 4.0 + g.size().x + 16.0).min(right), rect.bottom() - 4.0),
    );
    ui.painter().rect_filled(chip, crate::theme::r(5.0), theme::accent().gamma_multiply(0.18));
    ui.painter().with_clip_rect(chip.shrink2(vec2(6.0, 0.0))).galley(
        egui::pos2(chip.left() + 8.0, chip.center().y - g.size().y / 2.0),
        g,
        theme::accent(),
    );
    resp.clone().on_hover_text(display_name(&ed.design, css));
    if detach {
        return Some(None);
    }
    open_picker(&resp, ed, kind, Some(css)).map(Some)
}

/// The variable all of `ids` have bound to `field`, if they agree.
pub fn shared_binding(ed: &Editor, ids: &[NodeId], field: &str) -> Option<String> {
    let mut it = ids.iter().map(|id| ed.doc.get(*id).and_then(|n| n.bindings.get(field)));
    let first = it.next()??;
    it.all(|b| b == Some(first)).then(|| first.clone())
}

/// Apply a chip or button result to `field` of `ids`; `None` (nothing happened) does nothing.
pub fn apply(ed: &mut Editor, ids: &[NodeId], field: &str, action: ChipAction) {
    let Some(css) = action else { return };
    if let Err(e) = ed.bind(ids, field, css.as_deref()) {
        ed.status = e;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ds() -> DesignSystem {
        let mut ds = DesignSystem::default();
        let c = ds.add_collection("Tokens");
        for (n, k) in [("space/md", Kind::Number), ("space/sm", Kind::Number), ("radius/sm", Kind::Number), ("flag", Kind::Boolean), ("color/bg", Kind::Color)] {
            ds.add_variable(&c, n, k).unwrap();
        }
        let mut lib = ds.collections[0].clone();
        lib.id = "lib".into();
        lib.name = "Brand Kit".into();
        lib.library = Some("l1".into());
        lib.variables.retain(|v| v.name == "space/sm");
        lib.variables[0].name = "brand/tight".into();
        lib.variables[0].id = "libvar".into();
        ds.collections.push(lib);
        ds
    }

    #[test]
    fn list_is_grouped_and_kind_filtered() {
        let ds = ds();
        let l = pick_list(&ds, &PageModes::new(), Kind::Number, "");
        let heads: Vec<_> = l.iter().map(|g| (g.collection.as_str(), g.group.as_str(), g.items.len(), g.library)).collect();
        assert_eq!(heads, [("Tokens", "radius", 1, false), ("Tokens", "space", 2, false), ("Brand Kit", "brand", 1, true)]);
        assert_eq!(l[1].items[0].name, "md");
        assert_eq!(l[1].items[0].css, "space-md");
        assert!(pick_list(&ds, &PageModes::new(), Kind::Boolean, "").iter().all(|g| g.items.len() == 1 && g.group.is_empty()));
        assert_eq!(pick_list(&ds, &PageModes::new(), Kind::Color, "").len(), 1);
        let q = pick_list(&ds, &PageModes::new(), Kind::Number, "TIGHT");
        assert_eq!(q.len(), 1);
        assert_eq!(q[0].items[0].css, "brand-tight");
    }
}
