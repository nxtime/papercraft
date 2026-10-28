//! The Design tab: this page's variable modes, a variables summary, feedback and lint. Variables
//! are edited in the Variables modal, components live in Assets.

use super::UiState;
use crate::editor::Editor;
use crate::mcp::lint::Issue;
use crate::model::NodeId;
use crate::theme;
use egui::{RichText, Ui};

/// Issues listed in the tab; the rest are only counted.
const MAX_ISSUES_SHOWN: usize = 30;
/// How often the open-feedback count of the other pages is refreshed, in seconds.
const ELSEWHERE_REFRESH_SECS: f64 = 1.0;

/// What the tab derives from the page. The lint and the comment scan visit every node, far too
/// much to redo each frame, so they are kept until the document changes.
#[derive(Default)]
pub struct Cache {
    key: Option<(u64, usize)>,
    issues: Vec<Issue>,
    open: Vec<(NodeId, String, String)>,
    elsewhere: Option<(f64, usize)>,
}

impl Cache {
    /// Bring the cache up to date at time `now` (seconds). True when the page was walked again.
    fn refresh(&mut self, ed: &Editor, now: f64) -> bool {
        let key = (ed.doc_stamp, ed.active_page);
        let walked = self.key != Some(key);
        if walked {
            self.issues = crate::mcp::lint::lint(&ed.doc, &ed.doc.root, &crate::mcp::lint::DEFAULT_SCALE);
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
            self.open = open;
            self.key = Some(key);
        }
        if self.elsewhere.is_none_or(|(t, _)| now - t >= ELSEWHERE_REFRESH_SECS) {
            let count: usize = (0..ed.pages.len())
                .filter(|i| *i != ed.active_page)
                .map(|i| ed.page_doc(i).nodes.values().map(|n| n.open_comments()).sum::<usize>())
                .sum();
            self.elsewhere = Some((now, count));
        }
        walked
    }

    fn elsewhere(&self) -> usize {
        self.elsewhere.map_or(0, |(_, n)| n)
    }
}

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
            let now = ui.input(|i| i.time);
            st.design_cache.refresh(ed, now);
            let open = st.design_cache.open.clone();
            let elsewhere = st.design_cache.elsewhere();
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
            let total = st.design_cache.issues.len();
            let shown: Vec<Issue> = st.design_cache.issues.iter().take(MAX_ISSUES_SHOWN).cloned().collect();
            ui.horizontal(|ui| {
                let title = if total == 0 {
                    "Lint · Clean".to_string()
                } else {
                    format!("Lint · {total} issues")
                };
                heading(ui, &title);
                if total > 0 {
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
            for issue in &shown {
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
                // The fix button is an icon at the row's end and takes its room first, so the
                // text beside it truncates instead of running underneath.
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if fixable
                        && crate::theme::small_icon_button(ui, Some(crate::theme::Icon::Zap), "")
                            .on_hover_text(format!("Fix: {}", issue.fix))
                            .clicked()
                    {
                        let target = issue.node;
                        ed.edit("Fix lint issue", |tx| {
                            crate::mcp::lint::autofix_node(tx, target, &crate::mcp::lint::DEFAULT_SCALE);
                        });
                    }
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        ui.label(RichText::new("●").color(dot_color).size(theme::SECONDARY));
                        let r = ui.add(
                            egui::Label::new(
                                RichText::new(format!("{node_name}: {}", issue.message)).size(theme::SECONDARY),
                            )
                            .truncate()
                            .sense(egui::Sense::click()),
                        );
                        if r.on_hover_text(&issue.fix).clicked() {
                            ed.selection = vec![issue.node];
                            ed.run(crate::editor::shortcuts::Action::ZoomSelection);
                        }
                    });
                });
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::Rect;
    use crate::model::{Node, NodeKind};

    fn add_rect(ed: &mut Editor) {
        ed.edit("Add", |tx| {
            let mut n = Node::new(NodeId(0), NodeKind::Rect, Rect::new(0.0, 0.0, 10.0, 10.0));
            n.id = tx.alloc_id();
            tx.insert(n, None, None);
        });
    }

    #[test]
    fn walks_the_page_only_when_the_document_changes() {
        let mut ed = Editor::default();
        let mut c = Cache::default();
        assert!(c.refresh(&ed, 0.0), "first frame walks");
        assert!(!c.refresh(&ed, 0.1), "an unchanged page is not walked again");
        assert!(!c.refresh(&ed, 5.0), "time alone does not walk it");
        add_rect(&mut ed);
        assert!(c.refresh(&ed, 5.1), "an edit walks it once");
        assert!(!c.refresh(&ed, 5.2));
    }

    #[test]
    fn other_pages_are_recounted_on_a_timer_not_a_change() {
        let ed = Editor::default();
        let mut c = Cache::default();
        c.refresh(&ed, 0.0);
        let first = c.elsewhere.map(|e| e.0);
        c.refresh(&ed, ELSEWHERE_REFRESH_SECS / 2.0);
        assert_eq!(c.elsewhere.map(|e| e.0), first, "too soon to recount");
        c.refresh(&ed, ELSEWHERE_REFRESH_SECS);
        assert_ne!(c.elsewhere.map(|e| e.0), first, "recounted after the interval");
    }
}
