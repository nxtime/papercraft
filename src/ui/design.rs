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

fn sanitize_name(s: &str) -> String {
    s.trim()
        .trim_start_matches("--")
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect()
}

pub fn panel(ui: &mut Ui, ed: &mut Editor, st: &mut UiState) {
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
            // ---- modes
            heading(ui, "Modes");
            let modes: Vec<String> = ed.doc.modes.keys().cloned().collect();
            let active = ed.doc.mode.clone();
            if !modes.is_empty() {
                ui.horizontal_wrapped(|ui| {
                    for m in &modes {
                        let on = active.as_deref() == Some(m);
                        if ui.selectable_label(on, m).clicked() && !on {
                            let m = m.clone();
                            on_every_page(ed, "Set variable mode", |tx| {
                                tx.set_mode(&m);
                            });
                        }
                        if crate::theme::small_button(ui, "✕")
                            .on_hover_text("Delete mode")
                            .clicked()
                        {
                            let m = m.clone();
                            on_every_page(ed, "Remove variable mode", |tx| {
                                tx.remove_mode(&m);
                            });
                        }
                    }
                });
            }
            ui.horizontal(|ui| {
                let w = (ui.available_width() - 44.0).max(60.0);
                let r = ui.add(
                    egui::TextEdit::singleline(&mut st.mode_draft)
                        .hint_text("New mode (e.g. Dark)")
                        .desired_width(w)
                        .margin(crate::theme::INPUT_MARGIN),
                );
                let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                let mode_name = st.mode_draft.trim().to_string();
                if (crate::theme::small_button(ui, "Add").clicked() || enter)
                    && !mode_name.is_empty()
                {
                    st.mode_draft.clear();
                    let snapshot: Vec<(String, Color)> = ed
                        .doc
                        .variables
                        .iter()
                        .map(|(k, v)| (k.clone(), *v))
                        .collect();
                    on_every_page(ed, "Add variable mode", |tx| {
                        tx.doc.modes.entry(mode_name.clone()).or_default();
                        for (k, v) in &snapshot {
                            tx.set_variable_in_mode(&mode_name, k, *v);
                        }
                        tx.set_mode(&mode_name);
                    });
                }
            });
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
                    let is_renaming = st
                        .var_renaming
                        .as_ref()
                        .is_some_and(|(old, _)| old == &name);
                    if is_renaming {
                        let (_, mut draft) = st.var_renaming.clone().unwrap();
                        let r = ui.add(
                            egui::TextEdit::singleline(&mut draft)
                                .desired_width(90.0)
                                .margin(crate::theme::INPUT_MARGIN),
                        );
                        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                            st.var_renaming = None;
                        } else if r.lost_focus() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            let new = sanitize_name(&draft);
                            let old = name.clone();
                            st.var_renaming = None;
                            if !new.is_empty() && new != old {
                                on_every_page(ed, "Rename variable", |tx| {
                                    tx.rename_variable(&old, &new);
                                });
                            }
                        } else {
                            st.var_renaming = Some((name.clone(), draft));
                        }
                    } else {
                        let lbl = ui.add(
                            egui::Label::new(
                                RichText::new(format!("--{name}"))
                                    .monospace()
                                    .size(theme::SECONDARY + 1.0),
                            )
                            .sense(egui::Sense::click()),
                        );
                        if lbl.on_hover_text("Click to rename").clicked() {
                            st.var_renaming = Some((name.clone(), name.clone()));
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if crate::theme::small_button(ui, "✕")
                            .on_hover_text("Delete (fills keep their color)")
                            .clicked()
                        {
                            let n = name.clone();
                            on_every_page(ed, "Delete variable", |tx| tx.remove_variable(&n));
                        }
                        ui.label(RichText::new(color.to_hex()).monospace().weak());
                    });
                });
            }
            ui.horizontal(|ui| {
                let w = (ui.available_width() - 44.0).max(60.0);
                let r = ui.add(
                    egui::TextEdit::singleline(&mut st.variable_draft)
                        .hint_text("new-variable")
                        .desired_width(w)
                        .margin(crate::theme::INPUT_MARGIN),
                );
                let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                let name = sanitize_name(&st.variable_draft);
                if (crate::theme::small_button(ui, "Add").clicked() || enter) && !name.is_empty() {
                    st.variable_draft.clear();
                    let base = Color::rgb(0x80, 0x80, 0x80);
                    on_every_page(ed, "Add variable", |tx| {
                        tx.set_variable(&name, base);
                    });
                }
            });
            if !ed.doc.variables.is_empty() || !ed.doc.tokens.is_empty() {
                ui.horizontal(|ui| {
                    if crate::theme::small_button(ui, "Copy CSS")
                        .on_hover_text("Copy variables and tokens as CSS custom properties")
                        .clicked()
                    {
                        let mut lines = vec![":root {".to_string()];
                        for (k, v) in &ed.doc.variables {
                            lines.push(format!("  --{k}: {};", v.to_hex()));
                        }
                        for (k, v) in &ed.doc.tokens {
                            lines.push(format!("  --{k}: {v};"));
                        }
                        lines.push("}".to_string());
                        for (mode, map) in &ed.doc.modes {
                            lines.push(format!("[data-theme=\"{mode}\"] {{"));
                            for (k, v) in map {
                                lines.push(format!("  --{k}: {};", v.to_hex()));
                            }
                            lines.push("}".to_string());
                        }
                        ui.ctx().copy_text(lines.join("\n"));
                        ed.status = "Copied tokens to clipboard".into();
                    }
                    if crate::theme::small_button(ui, "Copy JSON")
                        .on_hover_text("Copy variables and tokens in W3C DTCG JSON format")
                        .clicked()
                    {
                        let mut root = serde_json::Map::new();
                        for (k, v) in &ed.doc.variables {
                            root.insert(
                                k.clone(),
                                serde_json::json!({ "$type": "color", "$value": v.to_hex() }),
                            );
                        }
                        for (k, v) in &ed.doc.tokens {
                            root.insert(
                                k.clone(),
                                serde_json::json!({ "$type": "dimension", "$value": v }),
                            );
                        }
                        if let Ok(json) =
                            serde_json::to_string_pretty(&serde_json::Value::Object(root))
                        {
                            ui.ctx().copy_text(json);
                            ed.status = "Copied tokens to clipboard".into();
                        }
                    }
                });
            }
            // ---- tokens
            heading(ui, "Tokens");
            let tokens: Vec<(String, String)> = ed
                .doc
                .tokens
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            for (k, v) in tokens {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("--{k}")).monospace().size(theme::SECONDARY));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if crate::theme::small_button(ui, "✕")
                            .on_hover_text("Delete token")
                            .clicked()
                        {
                            on_every_page(ed, "Delete token", |tx| tx.remove_token(&k));
                        }
                        ui.label(RichText::new(&v).monospace().weak().size(theme::SECONDARY));
                    });
                });
            }
            ui.horizontal(|ui| {
                let avail = (ui.available_width() - 48.0).max(80.0);
                let r1 = ui.add(
                    egui::TextEdit::singleline(&mut st.token_name_draft)
                        .hint_text("spacing-md")
                        .desired_width(avail * 0.55)
                        .margin(crate::theme::INPUT_MARGIN),
                );
                let r2 = ui.add(
                    egui::TextEdit::singleline(&mut st.token_val_draft)
                        .hint_text("16px")
                        .desired_width(avail * 0.45)
                        .margin(crate::theme::INPUT_MARGIN),
                );
                let enter = (r1.lost_focus() || r2.lost_focus())
                    && ui.input(|i| i.key_pressed(egui::Key::Enter));
                let name = sanitize_name(&st.token_name_draft);
                let val = st.token_val_draft.trim().to_string();
                if (crate::theme::small_button(ui, "Add").clicked() || enter)
                    && !name.is_empty()
                    && !val.is_empty()
                {
                    st.token_name_draft.clear();
                    st.token_val_draft.clear();
                    on_every_page(ed, "Set token", |tx| tx.set_token(&name, &val));
                }
            });
            // ---- components
            heading(ui, "Components");
            let mut comps: Vec<(NodeId, String, f64, f64, usize)> = ed
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
                    (n.id, n.name.clone(), n.rect.w, n.rect.h, uses)
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
            if comps.len() > 3 || !st.component_filter.is_empty() {
                ui.add(
                    egui::TextEdit::singleline(&mut st.component_filter)
                        .hint_text("Filter components…")
                        .desired_width(f32::INFINITY)
                        .margin(crate::theme::INPUT_MARGIN),
                );
                ui.add_space(2.0);
            }
            if let Some(inst_id) = ed
                .selection
                .first()
                .copied()
                .filter(|id| ed.is_instance_root(*id))
            {
                let cur_main = ed.instance_main_root(inst_id);
                let cur_name = cur_main
                    .and_then(|m| ed.doc.get(m))
                    .map(|n| n.name.clone())
                    .unwrap_or_else(|| "Instance".into());
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        RichText::new("Instance")
                            .size(theme::SECONDARY)
                            .color(p.secondary),
                    );
                    egui::ComboBox::from_id_salt("design_swap_instance")
                        .selected_text(cur_name)
                        .width(100.0)
                        .show_ui(ui, |ui| {
                            for (cid, cname, _, _, _) in &comps {
                                if ui.selectable_label(cur_main == Some(*cid), cname).clicked()
                                    && cur_main != Some(*cid)
                                {
                                    ed.swap_instance(inst_id, *cid);
                                }
                            }
                        });
                    if crate::theme::small_button(ui, "Reset").clicked() {
                        ed.reset_overrides();
                    }
                    if crate::theme::small_button(ui, "Detach").clicked() {
                        ed.detach_instance();
                    }
                });
            }
            let needle = st.component_filter.trim().to_lowercase();
            for (id, name, w, h, uses) in comps {
                if !needle.is_empty() && !name.to_lowercase().contains(&needle) {
                    continue;
                }
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&name).font(theme::medium(theme::BODY)));
                    ui.label(
                        RichText::new(format!("{w:.0}×{h:.0} · {uses}×"))
                            .weak()
                            .size(theme::SECONDARY),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if crate::theme::small_button(ui, "Insert")
                            .on_hover_text("Place an instance in view")
                            .clicked()
                        {
                            let at = ed.camera.screen_to_world(ed.viewport * 0.5);
                            ed.create_instance(id, at);
                        }
                        if crate::theme::small_button(ui, "Go to")
                            .on_hover_text("Select the main component")
                            .clicked()
                        {
                            ed.selection = vec![id];
                            ed.run(crate::editor::shortcuts::Action::ZoomSelection);
                        }
                    });
                });
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
                        if crate::theme::small_button(ui, "Fix all")
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
                            if crate::theme::small_button(ui, "Fix")
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
