//! The Design tab: this page's variable modes, a variables summary, feedback and lint. Variables
//! are edited in the Variables modal, components live in Assets.

use super::UiState;
use crate::editor::Editor;
use crate::model::NodeId;
use crate::theme;
use egui::{RichText, Ui};

pub fn panel(ui: &mut Ui, ed: &mut Editor, st: &mut UiState) {
    let p = theme::pal(ui);
    let heading = |ui: &mut Ui, t: &str| {
        ui.add_space(6.0);
        ui.label(
            RichText::new(t.to_uppercase())
                .font(theme::semibold(theme::SECONDARY))
                .color(p.tertiary),
        );
        ui.add_space(2.0);
    };
    egui::ScrollArea::vertical()
        .id_salt("design")
        .auto_shrink(false)
        .show(ui, |ui| {
            crate::theme::figma_fields(ui);
            // ---- open feedback on this page (and a pointer to other pages)
            let mut open: Vec<(NodeId, String, String)> = ed
                .doc
                .nodes
                .values()
                .filter(|n| n.open_comments() > 0)
                .flat_map(|n| {
                    n.comments
                        .iter()
                        .filter(|c| !c.resolved)
                        .map(|c| (n.id, n.name.clone(), c.text.clone()))
                        .collect::<Vec<_>>()
                })
                .collect();
            open.sort_by_key(|o| o.0);
            let elsewhere: usize = (0..ed.pages.len())
                .filter(|i| *i != ed.active_page)
                .map(|i| {
                    ed.page_doc(i)
                        .nodes
                        .values()
                        .map(|n| n.open_comments())
                        .sum::<usize>()
                })
                .sum();
            if !open.is_empty() || elsewhere > 0 {
                heading(ui, &format!("Feedback · {} open", open.len() + elsewhere));
                for (id, name, note) in open {
                    let short: String = note.chars().take(46).collect();
                    let r = ui.add(
                        egui::Label::new(
                            RichText::new(format!("● {name}: {short}")).size(theme::SECONDARY + 1.0),
                        )
                        .sense(egui::Sense::click()),
                    );
                    if r.on_hover_text(note).clicked() {
                        ed.selection = vec![id];
                        ed.run(crate::editor::shortcuts::Action::ZoomSelection);
                    }
                }
                if elsewhere > 0 {
                    ui.label(
                        RichText::new(format!("{elsewhere} more on other pages"))
                            .size(theme::SECONDARY)
                            .color(p.secondary),
                    );
                }
            }
            // ---- modes per collection, for this page
            let multi: Vec<_> = ed.design.collections.iter().filter(|c| c.modes.len() > 1).cloned().collect();
            if !multi.is_empty() {
                heading(ui, "Modes on this page");
            }
            for c in multi {
                let active = ed.pages[ed.active_page].modes.get(&c.id).unwrap_or(&c.default_mode);
                let cur = c.modes.iter().position(|m| &m.id == active).unwrap_or(0);
                let opts: Vec<(usize, &str)> = c.modes.iter().enumerate().map(|(i, m)| (i, m.name.as_str())).collect();
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&c.name).size(theme::SECONDARY + 1.0).color(p.secondary));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if let Some(i) = super::kit::select(ui, cur, &opts, 120.0)
                            && let Err(e) = ed.set_page_mode(&c.id, &c.modes[i].id)
                        {
                            ed.status = e.to_string();
                        }
                    });
                });
            }
            // ---- variables
            heading(ui, "Variables");
            let vars: usize = ed.design.collections.iter().map(|c| c.variables.len()).sum();
            ui.label(
                RichText::new(format!("{vars} variables in {} collections", ed.design.collections.len()))
                    .size(theme::SECONDARY + 1.0)
                    .color(p.secondary),
            );
            if super::kit::button(ui, super::kit::ButtonKind::Secondary, None, "Open variables").clicked() {
                st.variables.open = true;
            }
            // ---- design lint
            let issues = crate::mcp::lint::lint(
                &ed.doc,
                &ed.doc.root,
                &crate::mcp::lint::DEFAULT_SCALE,
            );
            ui.horizontal(|ui| {
                let title = if issues.is_empty() {
                    "Lint · Clean".to_string()
                } else {
                    format!("Lint · {} issues", issues.len())
                };
                heading(ui, &title);
                if !issues.is_empty() {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if crate::theme::small_icon_button(ui, Some(crate::theme::Icon::Zap), "Fix all")
                            .on_hover_text("Auto-fix grid, color variables, text scale, and touch targets")
                            .clicked()
                        {
                            ed.edit("Auto-fix lint issues", |tx| {
                                let roots = tx.doc().root.clone();
                                crate::mcp::lint::autofix(
                                    tx,
                                    &roots,
                                    &crate::mcp::lint::DEFAULT_SCALE,
                                );
                            });
                        }
                    });
                }
            });
            for issue in issues.iter().take(30) {
                let node_name = ed
                    .doc
                    .get(issue.node)
                    .map(|n| n.name.clone())
                    .unwrap_or_else(|| "Layer".into());
                let dot_color = match issue.severity {
                    "error" => egui::Color32::from_rgb(239, 68, 68),
                    "warning" => egui::Color32::from_rgb(245, 158, 11),
                    _ => p.secondary,
                };
                let fixable = matches!(
                    issue.rule,
                    "spacing-grid"
                        | "hard-coded-color"
                        | "text-size"
                        | "type-scale"
                        | "touch-target"
                );
                ui.horizontal(|ui| {
                    ui.label(RichText::new("●").color(dot_color).size(theme::SECONDARY));
                    let r = ui.add(
                        egui::Label::new(
                            RichText::new(format!("{node_name}: {}", issue.message))
                                .size(theme::SECONDARY),
                        )
                        .truncate()
                        .sense(egui::Sense::click()),
                    );
                    if r.on_hover_text(&issue.fix).clicked() {
                        ed.selection = vec![issue.node];
                        ed.run(crate::editor::shortcuts::Action::ZoomSelection);
                    }
                    if fixable {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if crate::theme::small_icon_button(ui, Some(crate::theme::Icon::Zap), "Fix")
                                .on_hover_text(&issue.fix)
                                .clicked()
                            {
                                let target = issue.node;
                                ed.edit("Fix lint issue", |tx| {
                                    crate::mcp::lint::autofix_node(
                                        tx,
                                        target,
                                        &crate::mcp::lint::DEFAULT_SCALE,
                                    );
                                });
                            }
                        });
                    }
                });
            }
        });
}
