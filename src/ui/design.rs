//! The Design tab: the project's color variables (edit one and every fill following it moves),
//! variable modes (themes), and the page's components.

use super::UiState;
use crate::editor::Editor;
use crate::model::{Color, NodeId};
use crate::theme;
use egui::{RichText, Ui};

fn c32(c: Color) -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(c.r, c.g, c.b, c.a)
}

/// Run `f` as one undoable edit on every page, so variables stay in step across the project.
fn on_every_page(ed: &mut Editor, label: &str, f: impl Fn(&mut crate::history::Tx)) {
    for page in 0..ed.pages.len() {
        ed.with_page(page, |ed| ed.edit(label, |tx| f(tx)));
    }
}

pub fn panel(ui: &mut Ui, ed: &mut Editor, _st: &mut UiState) {
    let p = theme::pal(ui);
    let heading = |ui: &mut Ui, t: &str| {
        ui.add_space(6.0);
        ui.label(
            RichText::new(t.to_uppercase())
                .font(theme::semibold(theme::SECONDARY - 1.0))
                .color(p.secondary),
        );
        ui.add_space(2.0);
    };
    egui::ScrollArea::vertical()
        .id_salt("design")
        .auto_shrink(false)
        .show(ui, |ui| {
            // ---- modes
            if !ed.doc.modes.is_empty() {
                heading(ui, "Mode");
                let modes: Vec<String> = ed.doc.modes.keys().cloned().collect();
                let active = ed.doc.mode.clone();
                ui.horizontal_wrapped(|ui| {
                    for m in &modes {
                        let on = active.as_deref() == Some(m);
                        if ui.selectable_label(on, m).clicked() && !on {
                            let m = m.clone();
                            on_every_page(ed, "Set variable mode", |tx| {
                                tx.set_mode(&m);
                            });
                        }
                    }
                });
            }
            // ---- variables
            heading(ui, "Variables");
            if ed.doc.variables.is_empty() {
                ui.label(
                    RichText::new("No variables yet. Agents add them with\nset_variables; fills use var(--name).")
                        .size(theme::SECONDARY)
                        .color(p.secondary),
                );
            }
            let vars: Vec<(String, Color)> = ed
                .doc
                .variables
                .iter()
                .map(|(k, v)| (k.clone(), *v))
                .collect();
            for (name, color) in vars {
                ui.horizontal(|ui| {
                    let mut c = c32(color);
                    if ui.color_edit_button_srgba(&mut c).changed() {
                        let new = Color::rgba(c.r(), c.g(), c.b(), c.a());
                        if new != color {
                            let (n, mode) = (name.clone(), ed.doc.mode.clone());
                            on_every_page(ed, "Change variable", |tx| {
                                match &mode {
                                    Some(m) => tx.set_variable_in_mode(m, &n, new),
                                    None => tx.set_variable(&n, new),
                                };
                            });
                        }
                    }
                    ui.label(RichText::new(format!("--{name}")).monospace().size(theme::SECONDARY + 1.0));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new(color.to_hex()).monospace().weak());
                    });
                });
            }
            // ---- components
            heading(ui, "Components");
            let mut comps: Vec<(NodeId, String, usize)> = ed
                .doc
                .nodes
                .values()
                .filter(|n| n.component)
                .map(|n| {
                    let uses = ed
                        .doc
                        .nodes
                        .values()
                        .filter(|i| i.id != n.id && i.instance.as_ref().is_some_and(|l| l.main == n.id))
                        .count();
                    (n.id, n.name.clone(), uses)
                })
                .collect();
            comps.sort_by_key(|c| c.0);
            if comps.is_empty() {
                ui.label(
                    RichText::new("Select a frame and press ⌘⌥K,\nor use create_component.")
                        .size(theme::SECONDARY)
                        .color(p.secondary),
                );
            }
            for (id, name, uses) in comps {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&name).font(theme::medium(theme::BODY)));
                    ui.label(RichText::new(format!("{uses}")).weak().size(theme::SECONDARY));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("Insert").on_hover_text("Place an instance in view").clicked() {
                            let at = ed.camera.screen_to_world(ed.viewport * 0.5);
                            ed.create_instance(id, at);
                        }
                        if ui.small_button("Go to").on_hover_text("Select the main component").clicked() {
                            ed.selection = vec![id];
                            ed.run(crate::editor::shortcuts::Action::ZoomSelection);
                        }
                    });
                });
            }
        });
}
