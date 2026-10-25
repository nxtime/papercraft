//! "Edit properties" for a main component: each property's name, kind, layer and default, an
//! enum's options with what each changes (overrides JSON), and templates to start from.

use super::{UiState, kit};
use crate::editor::Editor;
use crate::model::{NodeId, PropDef, PropOption};
use crate::theme;
use egui::{Align, Layout, RichText};

const MODAL_W: f32 = 640.0;
const KINDS: [(&str, &str); 4] = [("text", "Text"), ("bool", "Show / hide"), ("icon", "Icon"), ("enum", "Options")];

/// The draft being edited: the main (on the page being viewed) and its properties, with each
/// option's changes as editable JSON text.
#[derive(Default)]
pub struct PropsModal {
    pub main: Option<NodeId>,
    draft: Vec<(PropDef, Vec<String>)>,
    error: Option<String>,
}

impl PropsModal {
    /// Open for `main`, from its current properties.
    pub fn open(&mut self, ed: &Editor, main: NodeId) {
        let props = ed.doc.get(main).map(|n| n.props.clone()).unwrap_or_default();
        self.draft = props.into_iter().map(|d| {
            let sets = d.options.iter().map(|o| serde_json::to_string_pretty(&o.set).unwrap_or_default()).collect();
            (d, sets)
        }).collect();
        self.main = Some(main);
        self.error = None;
    }
}

/// The properties as edited, or what is wrong with an option's changes.
fn defs(draft: &[(PropDef, Vec<String>)]) -> Result<Vec<PropDef>, String> {
    draft
        .iter()
        .map(|(d, sets)| {
            let mut d = d.clone();
            if d.name.trim().is_empty() {
                return Err("every property needs a name".into());
            }
            for (o, s) in d.options.iter_mut().zip(sets) {
                o.set = serde_json::from_str(if s.trim().is_empty() { "{}" } else { s }).map_err(|e| format!("`{}` > `{}`: {e}", d.name, o.value))?;
            }
            Ok(d)
        })
        .collect()
}

pub fn show(ctx: &egui::Context, ed: &mut Editor, st: &mut UiState) {
    let Some(main) = st.props_modal.main else { return };
    if !ed.doc.get(main).is_some_and(|n| n.component) {
        st.props_modal.main = None;
        return;
    }
    // Layer names inside the main, for text, show/hide and icon properties.
    let mut layers: Vec<String> = ed.doc.subtree(main).into_iter().skip(1).filter_map(|i| ed.doc.get(i)).map(|n| n.name.clone()).collect();
    layers.sort();
    layers.dedup();
    let name = ed.doc.get(main).map(|n| n.name.clone()).unwrap_or_default();
    let p = theme::pal_from_ctx(ctx);
    let frame = egui::Frame::new().fill(p.sidebar).corner_radius(24.0).inner_margin(24);
    let (mut close, mut save, mut template) = (false, false, None);
    let m = &mut st.props_modal;
    egui::Modal::new(egui::Id::new("component-props")).frame(frame).show(ctx, |ui| {
        theme::figma_fields(ui);
        ui.set_width(MODAL_W.min(ctx.content_rect().width() - 96.0));
        ui.spacing_mut().item_spacing.y = 8.0;
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("Properties · {name}")).font(theme::semibold(18.0)));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if kit::close_button(ui).clicked() {
                    close = true;
                }
            });
        });
        ui.label(RichText::new("Each instance sets these. Options change any layer: a name (or Parent > Layer, or $root) to a text, or to styles such as {\"background\": \"var(--danger-bg)\", \"color\": \"#b91c1c\", \"icon\": \"lucide:x\", \"visibility\": \"hidden\"}.").size(theme::SECONDARY).color(p.secondary));
        let mut drop = None;
        egui::ScrollArea::vertical().max_height((ctx.content_rect().height() - 260.0).max(200.0)).show(ui, |ui| {
            for (i, (d, sets)) in m.draft.iter_mut().enumerate() {
                egui::Frame::new().fill(p.field).corner_radius(12.0).inner_margin(12).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.add(egui::TextEdit::singleline(&mut d.name).hint_text("Name").desired_width(180.0));
                        if let Some(k) = kit::select(ui, d.kind.as_str(), &KINDS, 140.0) {
                            d.kind = k.to_owned();
                        }
                        if d.kind != "enum" {
                            let opts: Vec<(&str, &str)> = layers.iter().map(|l| (l.as_str(), l.as_str())).collect();
                            let cur = d.layer.clone().unwrap_or_default();
                            if let Some(l) = kit::select(ui, cur.as_str(), &opts, 180.0) {
                                d.layer = Some(l.to_owned());
                            }
                        }
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if kit::chip(ui, "Remove", false).clicked() {
                                drop = Some(i);
                            }
                        });
                    });
                    let mut default = d.default.clone().unwrap_or_default();
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Default").size(theme::SECONDARY));
                        if d.kind == "enum" {
                            let opts: Vec<(&str, &str)> = d.options.iter().map(|o| (o.value.as_str(), o.value.as_str())).collect();
                            if let Some(v) = kit::select(ui, default.as_str(), &opts, 200.0) {
                                default = v.to_owned();
                            }
                        } else {
                            ui.add(egui::TextEdit::singleline(&mut default).desired_width(200.0));
                        }
                    });
                    d.default = (!default.is_empty()).then_some(default);
                    if d.kind == "enum" {
                        let mut gone = None;
                        for (j, (o, s)) in d.options.iter_mut().zip(sets.iter_mut()).enumerate() {
                            ui.horizontal(|ui| {
                                ui.add(egui::TextEdit::singleline(&mut o.value).hint_text("Option").desired_width(180.0));
                                if kit::chip(ui, "×", false).on_hover_text("Remove this option").clicked() {
                                    gone = Some(j);
                                }
                            });
                            ui.add(egui::TextEdit::multiline(s).code_editor().desired_rows(2).desired_width(f32::INFINITY));
                        }
                        if let Some(j) = gone {
                            d.options.remove(j);
                            sets.remove(j);
                        }
                        if kit::chip(ui, "+ Option", false).clicked() {
                            // A new option starts from the first one's changes, ready to adjust.
                            sets.push(sets.first().cloned().unwrap_or_else(|| "{}".into()));
                            d.options.push(PropOption { value: format!("Option {}", d.options.len() + 1), set: serde_json::Value::Null });
                        }
                    }
                });
            }
        });
        if let Some(i) = drop {
            m.draft.remove(i);
        }
        ui.horizontal_wrapped(|ui| {
            for (kind, label) in [("text", "+ Text"), ("bool", "+ Show / hide"), ("icon", "+ Icon"), ("enum", "+ Options")] {
                if kit::chip(ui, label, false).clicked() {
                    let layer = (kind != "enum").then(|| layers.first().cloned()).flatten();
                    let n = m.draft.len() + 1;
                    let d = PropDef { name: format!("Property {n}"), kind: kind.into(), layer, ..Default::default() };
                    m.draft.push((d, Vec::new()));
                }
            }
            ui.separator();
            for (t, label, tip) in [("text", "Texts from layers", "A text property for each text layer"), ("active", "Active item", "Which item of a tab bar, nav or segmented row is active")] {
                if kit::chip(ui, label, false).on_hover_text(tip).clicked() {
                    template = Some(t);
                }
            }
        });
        if let Some(e) = &m.error {
            ui.label(RichText::new(e).size(theme::SECONDARY).color(p.danger));
        }
        ui.horizontal(|ui| {
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
        match defs(&st.props_modal.draft).and_then(|d| crate::mcp::exec::save_props_ui(ed, main, d)).and_then(|()| crate::mcp::exec::props_template_ui(ed, main, t)) {
            Ok(()) => st.props_modal.open(ed, main),
            Err(e) => st.props_modal.error = Some(e),
        }
    }
    if save {
        match defs(&st.props_modal.draft).and_then(|d| crate::mcp::exec::save_props_ui(ed, main, d)) {
            Ok(()) => st.props_modal.main = None,
            Err(e) => st.props_modal.error = Some(e),
        }
    }
    if close {
        st.props_modal.main = None;
    }
}
