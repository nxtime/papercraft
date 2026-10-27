//! "Edit properties" for a main component: a tab per property (its name, kind, layer and
//! default), and for an options property its options either as a table (option × layer style,
//! with colour swatches, toggles, icon and image fields) or as highlighted JSON source.

use crate::theme::Surface as _;
use super::{UiState, kit};
use crate::editor::Editor;
use crate::model::{NodeId, PropDef, PropOption};
use crate::theme::{self, Icon};
use egui::{Align, Color32, Layout, RichText, text::LayoutJob};
use serde_json::{Map, Value};

const MODAL_W: f32 = 760.0;
const KINDS: [(&str, &str); 4] = [("text", "Text"), ("bool", "Show / hide"), ("icon", "Icon"), ("enum", "Options")];
/// Styles a column can set, in the order the "+ Column" menu offers them.
const CELL_KEYS: [&str; 9] = ["background", "color", "text", "icon", "visibility", "border", "fontWeight", "opacity", "backgroundImage"];
const CELL_W: f32 = 132.0;
/// Styles whose value is a colour: picked from the project's variables.
const COLOR_KEYS: [&str; 5] = ["background", "backgroundColor", "color", "border", "fill"];
const OPTION_W: f32 = 140.0;

/// The draft being edited: the main (on the page being viewed), its properties, which tab is
/// open, and for each options property the JSON source of every option as typed.
#[derive(Default)]
pub struct PropsModal {
    pub main: Option<NodeId>,
    draft: Vec<PropDef>,
    source: Vec<Vec<String>>,
    tab: usize,
    table: bool,
    error: Option<String>,
    new_layer: String,
    new_key: String,
}

impl PropsModal {
    /// Open for `main`, from its current properties.
    pub fn open(&mut self, ed: &Editor, main: NodeId) {
        self.draft = ed.doc.get(main).map(|n| n.props.clone()).unwrap_or_default();
        self.source = self.draft.iter().map(sources).collect();
        self.main = Some(main);
        self.tab = self.tab.min(self.draft.len().saturating_sub(1));
        self.table = true;
        self.error = None;
    }
}

fn sources(d: &PropDef) -> Vec<String> {
    d.options.iter().map(|o| serde_json::to_string_pretty(&o.set).unwrap_or_default()).collect()
}

/// Every (layer, style) an options property changes, in first-seen order: the table's columns.
fn columns(d: &PropDef) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for o in &d.options {
        for (layer, v) in o.set.as_object().into_iter().flatten() {
            match v {
                Value::Object(m) => {
                    for k in m.keys() {
                        if !out.iter().any(|(l, kk)| l == layer && kk == k) {
                            out.push((layer.clone(), k.clone()));
                        }
                    }
                }
                _ => {
                    if !out.iter().any(|(l, kk)| l == layer && kk == "text") {
                        out.push((layer.clone(), "text".into()));
                    }
                }
            }
        }
    }
    out
}

/// One cell of the table: the option's value for (layer, style), as text.
fn cell(o: &PropOption, layer: &str, key: &str) -> String {
    match o.set.get(layer) {
        Some(Value::String(t)) if key == "text" => t.clone(),
        Some(Value::Object(m)) => m.get(key).map(|v| v.as_str().map_or_else(|| v.to_string(), str::to_owned)).unwrap_or_default(),
        _ => String::new(),
    }
}

fn set_cell(o: &mut PropOption, layer: &str, key: &str, value: String) {
    if !o.set.is_object() {
        o.set = Value::Object(Map::new());
    }
    let set = o.set.as_object_mut().unwrap();
    let entry = set.entry(layer.to_owned()).or_insert_with(|| Value::Object(Map::new()));
    // A plain text entry becomes {"text": ...} once another style joins it.
    if let Value::String(t) = entry {
        *entry = serde_json::json!({ "text": t.clone() });
    }
    let m = entry.as_object_mut().unwrap();
    if value.is_empty() {
        m.remove(key);
    } else {
        m.insert(key.to_owned(), Value::String(value));
    }
}

/// A colour a cell names, a literal or a variable of the project.
fn swatch(ed: &Editor, v: &str) -> Option<Color32> {
    let v = v.trim();
    let c = match v.strip_prefix("var(--").and_then(|r| r.strip_suffix(')')) {
        Some(name) => ed.doc.variables.get(name).copied(),
        None => crate::mcp::css::color(v),
    }?;
    Some(super::c32(c))
}

/// JSON with keys, strings, numbers and punctuation in their own colours.
fn highlight(ui: &egui::Ui, text: &str, wrap: f32) -> LayoutJob {
    let p = theme::pal(ui);
    let font = egui::FontId::monospace(12.0);
    let (key, string, number, punct) = (theme::accent(), Color32::from_rgb(0x4a, 0xde, 0x80), Color32::from_rgb(0xfb, 0xbf, 0x24), p.secondary);
    let mut job = LayoutJob::default();
    job.wrap.max_width = wrap;
    let push = |job: &mut LayoutJob, s: &str, c: Color32| job.append(s, 0.0, egui::TextFormat::simple(font.clone(), c));
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut i = 0;
    while i < chars.len() {
        let (start, c) = chars[i];
        if c == '"' {
            let mut j = i + 1;
            while j < chars.len() && (chars[j].1 != '"' || chars[j - 1].1 == '\\') {
                j += 1;
            }
            let end = chars.get(j + 1).map_or(text.len(), |(b, _)| *b);
            let is_key = text[end..].trim_start().starts_with(':');
            push(&mut job, &text[start..end], if is_key { key } else { string });
            i = j + 1;
        } else if c.is_ascii_digit() || c == '-' {
            let mut j = i;
            while j < chars.len() && (chars[j].1.is_ascii_alphanumeric() || ".-".contains(chars[j].1)) {
                j += 1;
            }
            let end = chars.get(j).map_or(text.len(), |(b, _)| *b);
            push(&mut job, &text[start..end], number);
            i = j;
        } else {
            let end = chars.get(i + 1).map_or(text.len(), |(b, _)| *b);
            push(&mut job, &text[start..end], if "{}[]:,".contains(c) { punct } else { p.text });
            i += 1;
        }
    }
    job
}

/// The properties as edited, or what is wrong with an option's source.
fn defs(m: &PropsModal) -> Result<Vec<PropDef>, String> {
    m.draft
        .iter()
        .map(|d| {
            if d.name.trim().is_empty() {
                return Err("every property needs a name".into());
            }
            Ok(d.clone())
        })
        .collect()
}

pub fn show(ctx: &egui::Context, ed: &mut Editor, st: &mut UiState) {
    let Some(main) = st.props_modal.main else { return };
    if !ed.doc.get(main).is_some_and(|n| n.component) {
        st.props_modal.main = None;
        return;
    }
    let mut layers: Vec<String> = ed.doc.subtree(main).into_iter().skip(1).filter_map(|i| ed.doc.get(i)).map(|n| n.name.clone()).collect();
    layers.sort();
    layers.dedup();
    layers.insert(0, "$root".into());
    let name = ed.doc.get(main).map(|n| n.name.clone()).unwrap_or_default();
    let p = theme::pal_from_ctx(ctx);
    let frame = egui::Frame::new().fill(p.sidebar).corner_radius(crate::theme::r(24.0)).inner_margin(24);
    let (mut close, mut save, mut template) = (false, false, None);
    let m = &mut st.props_modal;
    egui::Modal::new(egui::Id::new("component-props")).frame(frame).show(ctx, |ui| {
        theme::modal_backing(ui, theme::pal(ui).sidebar, theme::ru(24));
        theme::figma_fields(ui);
        ui.set_width(MODAL_W.min(ctx.content_rect().width() - 96.0));
        ui.spacing_mut().item_spacing.y = 10.0;
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("Properties · {name}")).font(theme::semibold(theme::BODY + 2.0)));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if kit::close_button(ui).clicked() {
                    close = true;
                }
            });
        });
        // Tabs: one per property, a dot in its kind's colour; then adding.
        ui.horizontal_wrapped(|ui| {
            for (i, d) in m.draft.iter().enumerate() {
                let label = if d.name.is_empty() { "Untitled" } else { d.name.as_str() };
                if kit::chip(ui, label, i == m.tab).clicked() {
                    m.tab = i;
                }
            }
            ui.menu_button(RichText::new("+ Property").size(theme::SECONDARY), |ui| {
                crate::theme::popup_backing(ui);
                for (kind, label) in KINDS {
                    if ui.button(label).clicked() {
                        let layer = (kind != "enum").then(|| layers.get(1).cloned()).flatten();
                        let n = m.draft.len() + 1;
                        m.draft.push(PropDef { name: format!("Property {n}"), kind: kind.into(), layer, ..Default::default() });
                        m.source.push(Vec::new());
                        m.tab = m.draft.len() - 1;
                        ui.close();
                    }
                }
                ui.separator();
                for (t, label) in [("text", "Texts from layers"), ("active", "Active item (tabs, nav)")] {
                    if ui.button(label).clicked() {
                        template = Some(t);
                        ui.close();
                    }
                }
            });
        });
        let mut drop = false;
        egui::ScrollArea::vertical().max_height((ctx.content_rect().height() - 300.0).max(220.0)).show(ui, |ui| {
            let Some(d) = m.draft.get_mut(m.tab) else {
                ui.label(RichText::new("No properties yet. Add one, or start from a template.").color(p.secondary));
                return;
            };
            let src = &mut m.source[m.tab];
            egui::Frame::new().fill(p.field).corner_radius(crate::theme::r(12.0)).inner_margin(12).show_surface(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    crate::theme::lit(ui, |ui| ui.add(egui::TextEdit::singleline(&mut d.name).hint_text("Name").desired_width(200.0).margin(theme::INPUT_MARGIN)));
                    let current = KINDS.iter().position(|(k, _)| *k == d.kind).unwrap_or(0);
                    let items: Vec<(&str, Option<&str>)> = KINDS.iter().map(|(_, l)| (*l, None)).collect();
                    if let Some(i) = kit::segmented(ui, &items, current) {
                        d.kind = KINDS[i].0.into();
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if theme::icon_button(ui, Icon::Trash, 26.0, false).on_hover_text("Delete this property").clicked() {
                            drop = true;
                        }
                    });
                });
                if d.kind != "enum" {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Layer").size(theme::SECONDARY).color(p.secondary));
                        let opts: Vec<(&str, &str)> = layers.iter().skip(1).map(|l| (l.as_str(), l.as_str())).collect();
                        let cur = d.layer.clone().unwrap_or_default();
                        if let Some(l) = kit::select(ui, cur.as_str(), &opts, 260.0) {
                            d.layer = Some(l.to_owned());
                        }
                        ui.label(RichText::new("Default").size(theme::SECONDARY).color(p.secondary));
                        let mut def = d.default.clone().unwrap_or_default();
                        if d.kind == "bool" {
                            let mut on = def != "false";
                            if kit::checkbox(ui, &mut on, "Shown").changed() {
                                def = on.to_string();
                            }
                        } else {
                            crate::theme::lit(ui, |ui| ui.add(egui::TextEdit::singleline(&mut def).hint_text(if d.kind == "icon" { "lucide:check" } else { "Text" }).desired_width(180.0)));
                        }
                        d.default = (!def.is_empty()).then_some(def);
                    });
                    return;
                }
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Default").size(theme::SECONDARY).color(p.secondary));
                    let opts: Vec<(&str, &str)> = d.options.iter().map(|o| (o.value.as_str(), o.value.as_str())).collect();
                    let def = d.default.clone().unwrap_or_default();
                    if let Some(v) = kit::select(ui, def.as_str(), &opts, 200.0) {
                        d.default = Some(v.to_owned());
                    }
                    ui.add_space(24.0);
                    ui.label(RichText::new("View").size(theme::SECONDARY).color(p.secondary));
                    if let Some(i) = kit::segmented(ui, &[("Table", None), ("Source", None)], usize::from(!m.table)) {
                        m.table = i == 0;
                        *src = sources(d);
                    }
                });
                if src.len() != d.options.len() {
                    *src = sources(d);
                }
                if m.table {
                    table(ui, ed, d, &layers, &mut m.new_layer, &mut m.new_key);
                    *src = sources(d);
                } else {
                    let mut gone = None;
                    for (j, (o, s)) in d.options.iter_mut().zip(src.iter_mut()).enumerate() {
                        ui.horizontal(|ui| {
                            crate::theme::lit(ui, |ui| ui.add(egui::TextEdit::singleline(&mut o.value).hint_text("Option").desired_width(OPTION_W)));
                            if theme::icon_button(ui, Icon::Trash, 22.0, false).on_hover_text("Delete this option").clicked() {
                                gone = Some(j);
                            }
                        });
                        let mut layouter = |ui: &egui::Ui, buf: &dyn egui::TextBuffer, w: f32| ui.fonts_mut(|f| f.layout_job(highlight(ui, buf.as_str(), w)));
                        let r = crate::theme::lit(ui, |ui| ui.add(egui::TextEdit::multiline(s).code_editor().desired_rows(3).desired_width(f32::INFINITY).layouter(&mut layouter)));
                        if r.changed() {
                            match serde_json::from_str::<Value>(if s.trim().is_empty() { "{}" } else { s }) {
                                Ok(v) => o.set = v,
                                Err(e) => m.error = Some(format!("{}: {e}", o.value)),
                            }
                        }
                    }
                    if let Some(j) = gone {
                        d.options.remove(j);
                        src.remove(j);
                    }
                    if kit::chip(ui, "+ Option", false).clicked() {
                        let set = d.options.first().map(|o| o.set.clone()).unwrap_or(Value::Object(Map::new()));
                        d.options.push(PropOption { value: format!("Option {}", d.options.len() + 1), set });
                        *src = sources(d);
                    }
                }
            });
        });
        if drop && m.tab < m.draft.len() {
            m.draft.remove(m.tab);
            m.source.remove(m.tab);
            m.tab = m.tab.saturating_sub(1);
        }
        if let Some(e) = &m.error {
            ui.label(RichText::new(e).size(theme::SECONDARY).color(p.danger));
        }
        ui.horizontal(|ui| {
            ui.label(RichText::new("Options change any layer ($root, a name, or Parent > Layer). Variables like var(--primary) follow the theme.").size(theme::SECONDARY).color(p.secondary));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if kit::button(ui, kit::ButtonKind::Primary, None, "Save").clicked() {
                    save = true;
                }
                if kit::button(ui, kit::ButtonKind::Secondary, None, "Cancel").clicked() {
                    close = true;
                }
            });
        });
    });
    if let Some(t) = template {
        // Saves the draft first, so the template adds to what is being edited.
        match defs(&st.props_modal).and_then(|d| crate::mcp::exec::save_props_ui(ed, main, d)).and_then(|()| crate::mcp::exec::props_template_ui(ed, main, t)) {
            Ok(()) => st.props_modal.open(ed, main),
            Err(e) => st.props_modal.error = Some(e),
        }
    }
    if save {
        match defs(&st.props_modal).and_then(|d| crate::mcp::exec::save_props_ui(ed, main, d)) {
            Ok(()) => st.props_modal.main = None,
            Err(e) => st.props_modal.error = Some(e),
        }
    }
    if close {
        st.props_modal.main = None;
    }
}

/// The options as a table: a row per option, a column per (layer, style), each cell edited by
/// its kind (a swatch for colours, a toggle for visibility, an icon or image field, text).
fn table(ui: &mut egui::Ui, ed: &Editor, d: &mut PropDef, layers: &[String], new_layer: &mut String, new_key: &mut String) {
    let p = theme::pal(ui);
    let cols = columns(d);
    let mut gone_row = None;
    let mut gone_col = None;
    egui::ScrollArea::horizontal().id_salt("props-table").show(ui, |ui| {
        egui::Grid::new("props-grid").spacing([8.0, 6.0]).striped(true).show(ui, |ui| {
            ui.label(RichText::new("Option").size(theme::SECONDARY).color(p.secondary));
            for (ci, (layer, key)) in cols.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("{layer} · {key}")).size(theme::SECONDARY).color(p.secondary));
                    if theme::icon_button(ui, Icon::Trash, 18.0, false).on_hover_text("Remove this column").clicked() {
                        gone_col = Some(ci);
                    }
                });
            }
            ui.end_row();
            for (ri, o) in d.options.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    crate::theme::lit(ui, |ui| ui.add(egui::TextEdit::singleline(&mut o.value).desired_width(OPTION_W - 30.0)));
                    if theme::icon_button(ui, Icon::Trash, 20.0, false).on_hover_text("Delete this option").clicked() {
                        gone_row = Some(ri);
                    }
                });
                for (layer, key) in &cols {
                    let mut v = cell(o, layer, key);
                    let before = v.clone();
                    ui.horizontal(|ui| {
                        ui.set_width(CELL_W);
                        match key.as_str() {
                            "visibility" => {
                                let mut on = v != "hidden";
                                let label = if on { "Shown" } else { "Hidden" };
                                if kit::checkbox(ui, &mut on, label).changed() {
                                    v = if on { "visible" } else { "hidden" }.into();
                                }
                            }
                            k if COLOR_KEYS.contains(&k) => {
                                // Colours come from the project's variables (its tokens); a literal
                                // that is not one yet still shows, to be swapped for a token.
                                let (mark, _) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
                                if let Some(c) = swatch(ed, &v) {
                                    ui.painter().rect_filled(mark, crate::theme::r(4.0), c);
                                }
                                ui.painter().rect_stroke(mark, crate::theme::r(4.0), egui::Stroke::new(1.0, p.secondary.gamma_multiply(0.4)), egui::StrokeKind::Inside);
                                let vars: Vec<String> = ed.doc.variables.keys().map(|n| format!("var(--{n})")).collect();
                                let mut opts: Vec<(&str, &str)> = vars.iter().map(|x| (x.as_str(), &x[6..x.len() - 1])).collect();
                                if !v.is_empty() && !vars.contains(&v) {
                                    opts.insert(0, (v.as_str(), v.as_str()));
                                }
                                opts.insert(0, ("", "None"));
                                let cur = v.clone();
                                if let Some(x) = kit::select(ui, cur.as_str(), &opts, CELL_W - 22.0) {
                                    v = x.to_owned();
                                }
                            }
                            k => {
                                // A small mark of what the cell holds: an icon, an image, a text.
                                let (mark, _) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
                                match k {
                                    "icon" => theme::paint_icon(ui.painter(), mark, Icon::Star, p.secondary),
                                    "backgroundImage" => theme::paint_icon(ui.painter(), mark, Icon::Image, p.secondary),
                                    "text" => theme::paint_icon(ui.painter(), mark, Icon::Text, p.secondary),
                                    _ => match swatch(ed, &v) {
                                        Some(c) => {
                                            ui.painter().rect_filled(mark, crate::theme::r(4.0), c);
                                            ui.painter().rect_stroke(mark, crate::theme::r(4.0), egui::Stroke::new(1.0, p.secondary.gamma_multiply(0.5)), egui::StrokeKind::Inside);
                                        }
                                        None => {
                                            ui.painter().rect_stroke(mark, crate::theme::r(4.0), egui::Stroke::new(1.0, p.secondary.gamma_multiply(0.3)), egui::StrokeKind::Inside);
                                        }
                                    },
                                }
                                let hint = match k {
                                    "icon" => "lucide:check",
                                    "backgroundImage" => "url(…)",
                                    "text" => "Text",
                                    _ => "var(--primary)",
                                };
                                crate::theme::lit(ui, |ui| ui.add(egui::TextEdit::singleline(&mut v).hint_text(hint).desired_width(CELL_W - 22.0)));
                            }
                        }
                    });
                    if v != before {
                        set_cell(o, layer, key, v);
                    }
                }
                ui.end_row();
            }
        });
    });
    if let Some(r) = gone_row {
        d.options.remove(r);
    }
    if let Some(c) = gone_col {
        let (layer, key) = &cols[c];
        for o in &mut d.options {
            set_cell(o, layer, key, String::new());
            if o.set.get(layer).is_some_and(|v| v.as_object().is_some_and(Map::is_empty)) {
                o.set.as_object_mut().unwrap().remove(layer);
            }
        }
    }
    ui.horizontal(|ui| {
        if kit::chip(ui, "+ Option", false).clicked() {
            let set = d.options.first().map(|o| o.set.clone()).unwrap_or(Value::Object(Map::new()));
            d.options.push(PropOption { value: format!("Option {}", d.options.len() + 1), set });
        }
        ui.separator();
        let lopts: Vec<(&str, &str)> = layers.iter().map(|l| (l.as_str(), l.as_str())).collect();
        if new_layer.is_empty() {
            *new_layer = layers.first().cloned().unwrap_or_default();
        }
        if new_key.is_empty() {
            *new_key = CELL_KEYS[0].into();
        }
        if let Some(l) = kit::select(ui, new_layer.as_str(), &lopts, 180.0) {
            *new_layer = l.to_owned();
        }
        let kopts: Vec<(&str, &str)> = CELL_KEYS.iter().map(|k| (*k, *k)).collect();
        if let Some(k) = kit::select(ui, new_key.as_str(), &kopts, 140.0) {
            *new_key = k.to_owned();
        }
        if kit::chip(ui, "+ Column", false).clicked() {
            // An empty column: each option fills in its own value.
            for o in &mut d.options {
                if !o.set.is_object() {
                    o.set = Value::Object(Map::new());
                }
                let set = o.set.as_object_mut().unwrap();
                let e = set.entry(new_layer.clone()).or_insert_with(|| Value::Object(Map::new()));
                if let Some(m) = e.as_object_mut() {
                    m.entry(new_key.clone()).or_insert(Value::String(String::new()));
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_cells_read_and_write_the_overrides_format() {
        let mut o = PropOption { value: "Erro".into(), set: serde_json::json!({"Label": "Rejeitada", "$root": {"background": "#fee2e2"}}) };
        let d = PropDef { name: "S".into(), kind: "enum".into(), options: vec![o.clone()], ..Default::default() };
        assert_eq!(columns(&d), vec![("$root".into(), "background".into()), ("Label".into(), "text".into())]);
        assert_eq!(cell(&o, "Label", "text"), "Rejeitada");
        set_cell(&mut o, "Label", "color", "#b91c1c".into());
        assert_eq!(o.set["Label"], serde_json::json!({"text": "Rejeitada", "color": "#b91c1c"}), "a text entry turns into an object");
        set_cell(&mut o, "$root", "background", String::new());
        assert_eq!(o.set["$root"], serde_json::json!({}));
    }
}
