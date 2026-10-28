//! Sidebar views beyond layers, library and design: history, comments and the agent log.

use crate::theme::Surface as _;
use super::{UiState, kit};
use crate::editor::Editor;
use crate::model::NodeId;
use crate::runs::RunState;
use crate::theme::{self, Icon};
use egui::{RichText, Ui};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SideView {
    History,
    Comments,
    Agent,
    Annotations,
}

/// Which comments the Comments view shows.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum CommentFilter {
    #[default]
    Open,
    Resolved,
    All,
}

fn title(ui: &mut Ui, text: &str, sub: &str) {
    let p = theme::pal(ui);
    ui.add_space(6.0);
    ui.label(RichText::new(text).font(theme::semibold(16.0)));
    if !sub.is_empty() {
        ui.label(RichText::new(sub).size(theme::SECONDARY).color(p.secondary));
    }
    ui.add_space(8.0);
}

/// One row of a list: a coloured dot, a title and a muted line, optionally highlighted.
fn row(
    ui: &mut Ui,
    dot: egui::Color32,
    title: &str,
    sub: &str,
    active: bool,
    dim: bool,
) -> egui::Response {
    let p = theme::pal(ui);
    let (rect, resp) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 44.0), egui::Sense::click());
    let hover = super::motion::hover(ui.ctx(), resp.id.with("row"), &resp);
    if active {
        theme::fill_selected(ui, rect, theme::radius_lg(), p.field);
    } else if hover > 0.0 {
        ui.painter()
            .rect_filled(rect, theme::radius_lg(), p.hover.gamma_multiply(hover));
    }
    ui.painter()
        .circle_filled(egui::pos2(rect.left() + 14.0, rect.center().y), 4.0, dot);
    let ink = if dim { p.secondary } else { p.text };
    ui.painter().text(
        egui::pos2(rect.left() + 30.0, rect.center().y - 8.0),
        egui::Align2::LEFT_CENTER,
        title,
        if active {
            theme::semibold(theme::BODY)
        } else {
            egui::FontId::proportional(theme::BODY)
        },
        ink,
    );
    if !sub.is_empty() {
        ui.painter().text(
            egui::pos2(rect.left() + 30.0, rect.center().y + 9.0),
            egui::Align2::LEFT_CENTER,
            sub,
            egui::FontId::proportional(theme::SECONDARY),
            p.tertiary,
        );
    }
    resp
}

/// "5 minutes ago" for a saved version.
fn ago(then_ms: u64) -> String {
    super::home::relative_time(then_ms, crate::project::format::now_ms())
}

/// Saved versions of the page (named snapshots kept in the project folder) and, below them, the
/// recent changes from the undo stack: click a version to restore it, a step to return to it.
pub fn history(ui: &mut Ui, ed: &mut Editor, st: &mut UiState) {
    let p = theme::pal(ui);
    title(
        ui,
        "History",
        "Save versions of this page and go back to them.",
    );
    versions_section(ui, ed, st);
    ui.add_space(10.0);
    kit::section_header(ui, "Recent changes", None);
    let undo: Vec<String> = ed.history.undo_labels().map(str::to_owned).collect();
    let redo: Vec<String> = ed.history.redo_labels().map(str::to_owned).collect();
    if undo.is_empty() && redo.is_empty() {
        ui.add_space(12.0);
        kit::empty_state(
            ui,
            Icon::Restore,
            theme::accent(),
            "No changes yet",
            "Edits you make on this page show up here.",
            |_| {},
        );
        return;
    }
    let mut jump: Option<(bool, usize)> = None;
    egui::ScrollArea::vertical()
        .id_salt("history")
        .auto_shrink(false)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            for (i, label) in redo.iter().enumerate().rev() {
                if row(ui, p.tertiary, label, "Undone", false, true).clicked() {
                    jump = Some((false, i + 1));
                }
            }
            for (i, label) in undo.iter().enumerate().rev().take(40) {
                let current = i + 1 == undo.len();
                let sub = if current { "Current" } else { "" };
                let dot = if current {
                    theme::accent()
                } else {
                    p.secondary
                };
                if row(ui, dot, label, sub, current, false).clicked() && !current {
                    jump = Some((true, undo.len() - 1 - i));
                }
            }
        });
    if let Some((back, n)) = jump {
        for _ in 0..n {
            if back {
                ed.undo();
            } else {
                ed.redo();
            }
        }
    }
}

fn versions_section(ui: &mut Ui, ed: &mut Editor, st: &mut UiState) {
    let p = theme::pal(ui);
    let Some(dir) = st.project_dir.clone() else {
        return;
    };
    let page_id = ed.active_page_id().to_owned();
    let key = Some((dir.clone(), page_id.clone()));
    if st.versions_key != key {
        st.versions = crate::versions::list(&dir, &page_id);
        st.versions_key = key;
    }
    crate::theme::lit(ui, |ui| ui.add(
        egui::TextEdit::singleline(&mut st.version_draft)
            .hint_text("Name this version")
            .desired_width(f32::INFINITY)
            .margin(theme::INPUT_MARGIN),
    ));
    ui.add_space(4.0);
    if kit::button_wide(
        ui,
        kit::ButtonKind::Primary,
        Some(Icon::Plus),
        "Save version",
    )
    .clicked()
    {
        let page = ed.page_data(ed.active_page);
        let name = std::mem::take(&mut st.version_draft);
        match crate::versions::save(&dir, &page_id, &name, false, &page.doc) {
            Ok(_) => {
                st.versions_key = None;
                ed.status = "Saved a version of this page".into();
            }
            Err(e) => ed.status = format!("Could not save the version: {e}"),
        }
    }
    ui.add_space(8.0);
    kit::section_header(ui, "Versions", None);
    if st.versions.is_empty() {
        ui.label(
            RichText::new("No saved versions yet.")
                .size(theme::SECONDARY)
                .color(p.secondary),
        );
        return;
    }
    let mut restore: Option<String> = None;
    let mut delete: Option<String> = None;
    let mut rename: Option<(String, String)> = None;
    egui::ScrollArea::vertical()
        .id_salt("versions")
        .max_height(220.0)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            for v in &st.versions {
                let sub = format!(
                    "{}{}",
                    if v.auto { "Automatic · " } else { "" },
                    ago(v.saved)
                );
                let dot = if v.auto { p.tertiary } else { theme::accent() };
                if let Some((id, draft)) = &mut st.version_renaming
                    && *id == v.id
                {
                    let r = crate::theme::lit(ui, |ui| ui.add(
                        egui::TextEdit::singleline(draft)
                            .desired_width(f32::INFINITY)
                            .margin(theme::INPUT_MARGIN),
                    ));
                    // Check before taking focus back (see `home::project_card`).
                    if r.lost_focus() {
                        if !ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                            rename = Some((id.clone(), draft.clone()));
                        }
                        st.version_renaming = None;
                    } else {
                        r.request_focus();
                    }
                    continue;
                }
                let resp = row(ui, dot, &v.name, &sub, false, v.auto)
                    .on_hover_text("Click to restore this version");
                if resp.clicked() {
                    restore = Some(v.id.clone());
                }
                resp.context_menu(|ui| {
                    crate::theme::popup_backing(ui);
                    if super::icon_menu_item(ui, crate::theme::Icon::Restore, "Restore").clicked() {
                        restore = Some(v.id.clone());
                        ui.close();
                    }
                    if super::icon_menu_item(ui, crate::theme::Icon::Text, "Rename").clicked() {
                        st.version_renaming = Some((v.id.clone(), v.name.clone()));
                        ui.close();
                    }
                    if super::icon_menu_item(ui, crate::theme::Icon::Trash, "Delete version")
                        .clicked()
                    {
                        delete = Some(v.id.clone());
                        ui.close();
                    }
                });
            }
        });
    if let Some(id) = restore
        && let Some(snap) = crate::versions::load(&dir, &id)
    {
        // Keep what is on the page now, so a restore can itself be undone by restoring that.
        let before = ed.page_data(ed.active_page);
        let _ = crate::versions::save(
            &dir,
            &page_id,
            "Before restoring a version",
            true,
            &before.doc,
        );
        ed.restore_snapshot(&snap);
        st.versions_key = None;
        ed.status = "Restored a version".into();
    }
    if let Some(id) = delete {
        crate::versions::delete(&dir, &id);
        st.versions_key = None;
    }
    if let Some((id, name)) = rename
        && !name.trim().is_empty()
    {
        let _ = crate::versions::rename(&dir, &id, &name);
        st.versions_key = None;
    }
}

/// Open and resolved feedback on this page as cards; click one to select its layer, resolve it,
/// or add a note to the selected layer below.
pub fn comments(ui: &mut Ui, ed: &mut Editor, st: &mut UiState) {
    let p = theme::pal(ui);
    struct Item {
        node: NodeId,
        cid: u32,
        layer: String,
        author: String,
        text: String,
        resolved: bool,
        reply: Option<String>,
    }
    let mut items: Vec<Item> = Vec::new();
    for n in ed.doc.nodes.values() {
        for c in &n.comments {
            items.push(Item {
                node: n.id,
                cid: c.id,
                layer: n.name.clone(),
                author: c.author.clone(),
                text: c.text.clone(),
                resolved: c.resolved,
                reply: c.reply.clone(),
            });
        }
    }
    items.sort_by_key(|i| (i.resolved, i.node, i.cid));
    let open = items.iter().filter(|i| !i.resolved).count();
    let total = items.len();
    items.retain(|i| match st.comment_filter {
        CommentFilter::Open => !i.resolved,
        CommentFilter::Resolved => i.resolved,
        CommentFilter::All => true,
    });
    title(
        ui,
        &format!("Comments · {open} open"),
        "Notes for the agent, and its replies.",
    );
    ui.horizontal(|ui| {
        for (f, label) in [
            (CommentFilter::Open, "Open"),
            (CommentFilter::Resolved, "Resolved"),
            (CommentFilter::All, "All"),
        ] {
            if kit::chip(ui, label, st.comment_filter == f).clicked() {
                st.comment_filter = f;
            }
        }
    });
    ui.add_space(6.0);
    let reserve = 96.0;
    if items.is_empty() {
        ui.add_space(12.0);
        kit::empty_state(
            ui,
            Icon::Question,
            theme::accent(),
            if total == 0 {
                "No comments"
            } else {
                "Nothing here"
            },
            if total == 0 {
                "Select a layer and add a note below."
            } else {
                "No comments match this filter."
            },
            |_| {},
        );
    } else {
        let mut resolve: Option<(NodeId, u32, bool)> = None;
        let mut delete: Option<(NodeId, u32)> = None;
        egui::ScrollArea::vertical()
            .id_salt("comments")
            .max_height((ui.available_height() - reserve).max(80.0))
            .auto_shrink([false, true])
            .show(ui, |ui| {
                for it in &items {
                    let mut select = false;
                    egui::Frame::new()
                        .fill(p.field)
                        .corner_radius(theme::radius_panel())
                        .inner_margin(12)
                        .show_surface(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.spacing_mut().item_spacing.y = 6.0;
                            ui.horizontal(|ui| {
                                let name = ui.add(
                                    egui::Label::new(
                                        RichText::new(&it.layer).font(theme::semibold(theme::BODY)),
                                    )
                                    .truncate()
                                    .sense(egui::Sense::click()),
                                );
                                select = name.on_hover_text("Select this layer").clicked();
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if kit::chip(ui, "Delete", false)
                                            .on_hover_text("Delete this comment")
                                            .clicked()
                                        {
                                            delete = Some((it.node, it.cid));
                                        }
                                        if it.resolved {
                                            if kit::chip(ui, "Reopen", false)
                                                .on_hover_text("Mark as open again")
                                                .clicked()
                                            {
                                                resolve = Some((it.node, it.cid, false));
                                            }
                                        } else if kit::chip(ui, "Resolve", true)
                                            .on_hover_text("Mark as resolved")
                                            .clicked()
                                        {
                                            resolve = Some((it.node, it.cid, true));
                                        }
                                    },
                                );
                            });
                            ui.label(
                                RichText::new(format!("{}: {}", it.author, it.text))
                                    .color(if it.resolved { p.secondary } else { p.text }),
                            );
                            if let Some(reply) = &it.reply {
                                ui.label(
                                    RichText::new(format!("↳ {reply}"))
                                        .size(theme::SECONDARY)
                                        .color(p.secondary),
                                );
                            }
                        });
                    if select {
                        ed.selection = vec![it.node];
                        ed.run(crate::editor::shortcuts::Action::ZoomSelection);
                    }
                    ui.add_space(6.0);
                }
            });
        if let Some((node, cid, done)) = resolve {
            ed.edit(
                if done {
                    "Resolve feedback"
                } else {
                    "Reopen feedback"
                },
                |tx| {
                    if let Some(x) = tx
                        .node_mut(node)
                        .and_then(|n| n.comments.iter_mut().find(|x| x.id == cid))
                    {
                        x.resolved = done;
                    }
                },
            );
        }
        if let Some((node, cid)) = delete {
            ed.edit("Delete feedback", |tx| {
                if let Some(n) = tx.node_mut(node) {
                    n.comments.retain(|x| x.id != cid);
                }
            });
        }
    }
    ui.add_space(8.0);
    let target = ed.selection.first().copied();
    ui.add_enabled_ui(target.is_some(), |ui| {
        crate::theme::lit(ui, |ui| ui.add(
            egui::TextEdit::singleline(&mut st.feedback_draft)
                .hint_text(if target.is_some() {
                    "Note for the agent…"
                } else {
                    "Select a layer to add a note"
                })
                .desired_width(f32::INFINITY)
                .margin(theme::INPUT_MARGIN),
        ));
        ui.add_space(4.0);
        if kit::button_wide(ui, kit::ButtonKind::Primary, None, "Add note").clicked()
            && !st.feedback_draft.trim().is_empty()
            && let Some(id) = target
        {
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
}

/// Where an agent connects, its runs (with Review and Revert) and the tool calls it ran.
pub fn agent(ui: &mut Ui, ed: &mut Editor, st: &mut UiState) {
    if st.review_run.is_some() {
        return review(ui, ed, st);
    }
    st.review_diff = None;
    let p = theme::pal(ui);
    title(ui, "Agent", "Tools an agent ran on this project.");
    let live = st.agent_working().is_some();
    ui.horizontal(|ui| {
        kit::chip(ui, if live { "Working" } else { "Idle" }, live);
        if !st.agent_log.is_empty() && kit::chip(ui, "Clear log", false).clicked() {
            st.agent_log.clear();
        }
    });
    if st.agent_endpoint.is_empty() {
        ui.label(
            RichText::new("Agent connection is off.")
                .size(theme::SECONDARY)
                .color(p.secondary),
        );
    } else {
        ui.add_space(6.0);
        if kit::button_wide(ui, kit::ButtonKind::Secondary, None, "Copy agent URL")
            .on_hover_text(&st.agent_endpoint)
            .clicked()
        {
            ui.ctx().copy_text(st.agent_endpoint.clone());
        }
    }
    ui.add_space(10.0);
    egui::ScrollArea::vertical()
        .id_salt("agent")
        .auto_shrink(false)
        .show(ui, |ui| {
            runs_section(ui, ed, st);
            activity_section(ui, st);
        });
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_secs(5));
}

pub(super) fn count(n: usize, unit: &str) -> String {
    format!("{n} {unit}{}", if n == 1 { "" } else { "s" })
}

/// The "Agent finished" banner and the run cards, newest first.
fn runs_section(ui: &mut Ui, ed: &mut Editor, st: &mut UiState) {
    let p = theme::pal(ui);
    let Some(dir) = st.project_dir.clone() else {
        return;
    };
    if let Some(run) = st.run_banner.clone() {
        let changes = run.added + run.changed + run.removed;
        egui::Frame::new()
            .fill(p.field)
            .stroke(egui::Stroke::new(1.0, theme::accent()))
            .corner_radius(theme::radius_lg())
            .inner_margin(12)
            .show_surface(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    kit::badge(ui, Icon::Pen, theme::accent(), 32.0);
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        ui.label(
                            RichText::new("Agent finished").font(theme::semibold(theme::BODY)),
                        );
                        ui.label(
                            RichText::new(count(changes, "change"))
                                .size(theme::SECONDARY)
                                .color(p.secondary),
                        );
                    });
                });
                ui.add_space(8.0);
                ui.columns(2, |cols| {
                    if kit::button_wide(&mut cols[0], kit::ButtonKind::Primary, None, "Review")
                        .clicked()
                    {
                        st.review_run = Some(run.id.clone());
                        st.run_banner = None;
                    }
                    if kit::button_wide(&mut cols[1], kit::ButtonKind::Secondary, None, "Revert")
                        .clicked()
                    {
                        st.pending_run_revert = Some(run.id.clone());
                        st.run_banner = None;
                    }
                });
            });
        ui.add_space(10.0);
    }
    revert_dialog(ui.ctx(), ed, st, &dir);
    let saved = st.runs.iter().filter(|r| !st.run_open.iter().any(|o| o.id == r.id));
    let all: Vec<_> = st.run_open.iter().chain(saved).cloned().collect();
    if all.is_empty() {
        return;
    }
    kit::section_header(ui, "Runs", None);
    ui.spacing_mut().item_spacing.y = 8.0;
    for run in &all {
        let selected = st.review_run.as_ref() == Some(&run.id);
        let changes = run.added + run.changed + run.removed;
        let calls = match run.client.as_str() {
            "" => count(run.calls as usize, "call"),
            client => format!("{client} \u{b7} {}", count(run.calls as usize, "call")),
        };
        let (label, sub) = match run.state {
            RunState::Running => ("Running", format!("{calls} · in progress")),
            state => {
                let label = match state {
                    RunState::Review => "Review",
                    RunState::Accepted => "Accepted",
                    _ => "Reverted",
                };
                let when = ago(run.ended.unwrap_or(run.started));
                (label, format!("{calls} · {} · {when}", count(changes, "change")))
            }
        };
        let stroke = if selected {
            egui::Stroke::new(1.0, theme::accent())
        } else {
            egui::Stroke::NONE
        };
        let card = egui::Frame::new()
            .fill(p.field)
            .stroke(stroke)
            .corner_radius(theme::radius_lg())
            .inner_margin(12)
            .show_surface(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = 6.0;
                let top = egui::vec2(ui.available_width(), 22.0);
                let rtl = egui::Layout::right_to_left(egui::Align::Center);
                ui.allocate_ui_with_layout(top, rtl, |ui| {
                    let ink = match run.state {
                        RunState::Review | RunState::Running => Some(theme::accent()),
                        RunState::Accepted => Some(p.success),
                        RunState::Reverted => None,
                    };
                    kit::chip_ink(ui, label, ink);
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        ui.add(
                            egui::Label::new(
                                RichText::new(match run.title.as_str() {
                                    "" => format!("Run {}", run.number),
                                    t => format!("Run {} · {t}", run.number),
                                })
                                    .font(theme::semibold(theme::BODY)),
                            )
                            .truncate(),
                        );
                    });
                });
                ui.label(RichText::new(sub).size(theme::SECONDARY).color(p.secondary));
            });
        let id = ui.id().with(("run", &run.id));
        let click = ui.interact(card.response.rect, id, egui::Sense::click());
        if click.clicked() && run.state != RunState::Running {
            st.review_run = Some(run.id.clone());
        }
    }
    ui.add_space(10.0);
}

/// Marks a run Accepted (the run card's Accept in Chat mode).
pub(super) fn accept_run(ed: &mut Editor, st: &mut UiState, id: &str) {
    let (Some(dir), Some(mut run)) = (st.project_dir.clone(), st.runs.iter().find(|r| r.id == id).cloned()) else {
        return;
    };
    run.state = RunState::Accepted;
    if let Err(e) = crate::runs::save(&dir, &run) {
        ed.status = format!("Could not save the run: {e}");
    }
    st.runs_key = None;
}

/// Confirm, then put every page of the run back to how it was before it.
pub(super) fn revert_dialog(ctx: &egui::Context, ed: &mut Editor, st: &mut UiState, dir: &std::path::Path) {
    let Some(id) = st.pending_run_revert.clone() else {
        return;
    };
    // A later run changed the same layers: this one only goes back together with it.
    let blocker = st.runs.iter().find(|r| r.id == id).and_then(|r| crate::runs::blocker(&st.runs, r));
    let answer = match blocker {
        Some(n) => super::confirm::dialog(
            ctx,
            "run-revert",
            "Revert agent run?",
            &format!("Run {n} changed the same layers afterwards, so this run cannot be reverted on its own. Restore to here reverts it and every later run."),
            "Restore to here",
        ),
        None => super::confirm::dialog(
            ctx,
            "run-revert",
            "Revert agent run?",
            "Every page the agent changed goes back to how it was before the run, including any edits you made to those pages since.",
            "Revert",
        ),
    };
    let Some(yes) = answer else {
        return;
    };
    st.pending_run_revert = None;
    if yes && blocker.is_some() {
        return restore_to(ed, st, dir, &id);
    }
    let Some(mut run) = st.runs.iter().find(|r| r.id == id).cloned().filter(|_| yes) else {
        return;
    };
    if st.review_run.as_ref() == Some(&id) {
        st.review_run = None;
    }
    ed.status = match crate::runs::revert(dir, ed, &mut run) {
        Ok(()) => format!("Reverted agent run {}", run.number),
        Err(e) => format!("Could not revert the run: {e}"),
    };
    st.runs_key = None;
}

/// One confirm for "Restore to here": reverts the run and every later run, newest first.
pub(super) fn restore_dialog(ctx: &egui::Context, ed: &mut Editor, st: &mut UiState, dir: &std::path::Path) {
    let Some(id) = st.pending_restore.clone() else {
        return;
    };
    let runs = crate::runs::runs_from(&st.runs, &id);
    let body = format!(
        "This reverts {}, newest first, on every page they touched. Edits you made to those pages since are lost.",
        count(runs.len(), "agent run")
    );
    let Some(yes) = super::confirm::dialog(ctx, "run-restore", "Restore to here?", &body, "Restore") else {
        return;
    };
    st.pending_restore = None;
    if yes {
        restore_to(ed, st, dir, &id);
    }
}

/// Reverts run `id` and every later run, newest first.
fn restore_to(ed: &mut Editor, st: &mut UiState, dir: &std::path::Path, id: &str) {
    let runs = crate::runs::runs_from(&st.runs, id);
    if runs.is_empty() {
        return;
    }
    if !st.run_open.is_empty() {
        ed.status = "Wait for the agent run in progress to finish".into();
        return;
    }
    if st.review_run.as_ref().is_some_and(|r| runs.iter().any(|x| &x.id == r)) {
        st.review_run = None;
    }
    ed.status = match crate::runs::revert_all(dir, ed, runs) {
        Ok(n) => format!("Reverted {}", count(n, "agent run")),
        Err(e) => format!("Could not restore: {e}"),
    };
    st.runs_key = None;
}

fn activity_section(ui: &mut Ui, st: &mut UiState) {
    let p = theme::pal(ui);
    kit::section_header(ui, "Recent activity", None);
    if st.agent_log.is_empty() {
        ui.add_space(24.0);
        kit::empty_state(
            ui,
            Icon::Pen,
            theme::accent(),
            "No agent activity yet",
            "Tool calls from an agent appear here.",
            |_| {},
        );
        return;
    }
    ui.spacing_mut().item_spacing.y = 2.0;
    for call in st.agent_log.iter().rev() {
        let secs = call.at.elapsed().as_secs();
        let ago = match secs {
            0..=4 => "just now".to_string(),
            5..=59 => format!("{secs}s ago"),
            60..=3599 => format!("{}m ago", secs / 60),
            _ => format!("{}h ago", secs / 3600),
        };
        let (dot, sub) = match &call.error {
            Some(e) => (p.danger, format!("Failed · {ago} · {e}")),
            None => (theme::accent(), ago),
        };
        row(ui, dot, &call.name, &sub, false, false)
            .on_hover_text(call.error.as_deref().unwrap_or(""));
    }
}

/// How one page of a run compares to its snapshot.
pub enum PageReview {
    /// The run created the page.
    Created,
    /// The version file is gone.
    Missing,
    Diff(crate::mcp::diff::PageDiff),
    /// The page was deleted during the run; its snapshot diffed against nothing, so every node is removed.
    Deleted(crate::mcp::diff::PageDiff),
}

pub struct ReviewPage {
    pub page_id: String,
    pub name: String,
    pub review: PageReview,
}

/// The diff of one run against the live pages; `snaps` are the loaded versions, kept so a changed
/// document only re-runs the in-memory diff.
pub struct ReviewDiff {
    pub run: String,
    /// `doc_stamp` and history generation it was built at: an undo refreshes it too.
    stamp: (u64, u64),
    pub snaps: Vec<Option<crate::model::Document>>,
    pub pages: Vec<ReviewPage>,
}

fn live_doc<'a>(ed: &'a Editor, page_id: &str) -> Option<&'a crate::model::Document> {
    let slot = ed.pages.iter().find(|p| p.id == page_id)?;
    match &slot.stash {
        Some(s) => Some(&s.doc),
        None => Some(&ed.doc),
    }
}

fn build_review(ed: &Editor, run: &crate::runs::Run, dir: &std::path::Path) -> ReviewDiff {
    let snaps = run
        .pages
        .iter()
        .map(|rp| rp.version.as_deref().and_then(|v| crate::versions::load(dir, v)))
        .collect();
    let mut r = ReviewDiff {
        run: run.id.clone(),
        stamp: (0, 0),
        snaps,
        pages: Vec::new(),
    };
    refresh_review(&mut r, ed, run);
    r
}

fn refresh_review(r: &mut ReviewDiff, ed: &Editor, run: &crate::runs::Run) {
    r.stamp = (ed.doc_stamp, ed.history.generation());
    r.pages = run
        .pages
        .iter()
        .zip(&r.snaps)
        .map(|(rp, snap)| {
            let name = ed
                .pages
                .iter()
                .find(|p| p.id == rp.page_id)
                .map_or_else(|| rp.page_id.clone(), |p| p.name.clone());
            ReviewPage {
                page_id: rp.page_id.clone(),
                name,
                review: match page_review(rp.version.is_some(), snap.as_ref(), live_doc(ed, &rp.page_id)) {
                    PageReview::Diff(mut d) => {
                        crate::runs::keep_own(&mut d, rp);
                        PageReview::Diff(d)
                    }
                    other => other,
                },
            }
        })
        .collect();
}

/// `has_version`: the run changed an existing page (it has a snapshot to compare against).
fn page_review(
    has_version: bool,
    snap: Option<&crate::model::Document>,
    live: Option<&crate::model::Document>,
) -> PageReview {
    if !has_version {
        return PageReview::Created;
    }
    match (snap, live) {
        (Some(before), Some(after)) => PageReview::Diff(crate::mcp::diff::diff_pages(before, after)),
        (Some(before), None) => {
            PageReview::Deleted(crate::mcp::diff::diff_pages(before, &crate::model::Document::default()))
        }
        (None, _) => PageReview::Missing,
    }
}

/// One line for a changed node: the first property change, then "+N more".
fn detail(n: &crate::mcp::diff::NodeDiff) -> String {
    match (n.status, n.changes.first()) {
        ("added", _) => "added".into(),
        ("removed", _) => "deleted".into(),
        (_, Some(c)) => {
            let more = n.changes.len() - 1;
            let tail = if more > 0 { format!(" +{more} more") } else { String::new() };
            format!("{} {} → {}{tail}", c.field, c.before, c.after)
        }
        _ => "changed".into(),
    }
}

enum ReviewAct {
    Open(String, Option<NodeId>),
    Undo(String, crate::mcp::diff::NodeDiff),
}

/// Space between a review row's hover tint and its badge and revert button, so they don't sit on
/// the tint's edge.
const DIFF_ROW_PAD: f32 = 10.0;

fn diff_row(
    ui: &mut Ui,
    page_id: &str,
    n: &crate::mcp::diff::NodeDiff,
    deleted: bool,
    act: &mut Option<ReviewAct>,
) {
    let p = theme::pal(ui);
    let (tint, icon) = match n.status {
        "added" => (p.success, Icon::Plus),
        "removed" => (p.danger, Icon::Minus),
        _ => (p.warning, Icon::Pencil),
    };
    let (rect, resp) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 48.0), egui::Sense::click());
    let hover = super::motion::hover(ui.ctx(), resp.id.with("rv"), &resp);
    ui.painter()
        .rect_filled(rect, theme::radius_lg(), p.hover.gamma_multiply(hover));
    let badge = egui::Rect::from_min_size(
        rect.left_center() + egui::vec2(DIFF_ROW_PAD, -14.0),
        egui::vec2(28.0, 28.0),
    );
    ui.painter()
        .rect_filled(badge, theme::radius_md(), tint.gamma_multiply(0.18));
    theme::paint_icon(ui.painter(), badge.shrink(6.0), icon, tint);
    let x = rect.left() + DIFF_ROW_PAD + 40.0;
    let clip = ui.painter().with_clip_rect(egui::Rect::from_min_max(
        rect.min,
        egui::pos2(rect.right() - DIFF_ROW_PAD - 40.0, rect.bottom()),
    ));
    clip.text(
        egui::pos2(x, rect.center().y - 9.0),
        egui::Align2::LEFT_CENTER,
        &n.name,
        egui::FontId::proportional(theme::BODY),
        p.text,
    );
    clip.text(
        egui::pos2(x, rect.center().y + 10.0),
        egui::Align2::LEFT_CENTER,
        detail(n),
        egui::FontId::proportional(theme::SECONDARY),
        p.secondary,
    );
    // A deleted page has no live node to revert into; "Revert run" re-creates it.
    if !deleted {
        let btn = egui::Rect::from_min_size(
            egui::pos2(rect.right() - DIFF_ROW_PAD - 32.0, rect.center().y - 16.0),
            egui::vec2(32.0, 32.0),
        );
        let undo = ui
            .scope_builder(egui::UiBuilder::new().max_rect(btn), |ui| {
                theme::icon_button(ui, Icon::Restore, 32.0, false)
            })
            .inner
            .on_hover_text("Revert this change");
        if undo.clicked() {
            *act = Some(ReviewAct::Undo(page_id.to_string(), n.clone()));
            return;
        }
    }
    if resp.clicked() {
        *act = Some(ReviewAct::Open(page_id.to_string(), n.id_b));
    }
}

/// The run review: changes by frame, per-row revert, Revert run and Accept all.
pub(super) fn review(ui: &mut Ui, ed: &mut Editor, st: &mut UiState) {
    let p = theme::pal(ui);
    let (Some(dir), Some(id)) = (st.project_dir.clone(), st.review_run.clone()) else {
        st.review_run = None;
        return;
    };
    let Some(mut run) = st.runs.iter().find(|r| r.id == id).cloned() else {
        if !st.runs.is_empty() || st.runs_key.is_some() {
            st.review_run = None;
        }
        return;
    };
    match &mut st.review_diff {
        Some(r) if r.run == id => {
            if r.stamp != (ed.doc_stamp, ed.history.generation()) {
                refresh_review(r, ed, &run);
            }
        }
        _ => st.review_diff = Some(build_review(ed, &run, &dir)),
    }
    revert_dialog(ui.ctx(), ed, st, &dir);
    let Some(rd) = &st.review_diff else {
        return;
    };
    let (mut added, mut changed, mut removed, mut frames) = (0, 0, 0, 0);
    for page in &rd.pages {
        match &page.review {
            PageReview::Created => added += 1,
            PageReview::Missing => {}
            PageReview::Diff(d) => {
                (added, changed, removed) = (added + d.added, changed + d.modified, removed + d.removed);
                frames += d.groups.len();
            }
            PageReview::Deleted(d) => {
                removed += d.removed;
                frames += d.groups.len();
            }
        }
    }
    let mut back = false;
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(match run.title.as_str() {
                "" => format!("Run {}", run.number),
                t => format!("Run {} · {t}", run.number),
            })
            .font(theme::semibold(16.0)),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            back = kit::close_button(ui).on_hover_text("Back to runs").clicked();
        });
    });
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        kit::chip_ink(ui, &format!("+{added}"), Some(p.success));
        kit::chip_ink(ui, &format!("~{changed}"), Some(p.warning));
        kit::chip_ink(ui, &format!("−{removed}"), Some(p.danger));
        ui.label(
            RichText::new(format!("on {}", count(frames, "frame")))
                .size(theme::SECONDARY)
                .color(p.secondary),
        );
    });
    ui.add_space(8.0);
    let mut act = None;
    let footer = 44.0 + 16.0;
    egui::ScrollArea::vertical()
        .id_salt("review")
        .max_height((ui.available_height() - footer).max(80.0))
        .auto_shrink(false)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 4.0;
            for page in &rd.pages {
                match &page.review {
                    PageReview::Created => {
                        kit::section_header(ui, &page.name, None);
                        ui.label(RichText::new("Page created").color(p.secondary));
                    }
                    PageReview::Missing => {
                        ui.label(
                            RichText::new(format!("Snapshot missing for page {}", page.name))
                                .color(p.danger),
                        );
                    }
                    PageReview::Diff(d) | PageReview::Deleted(d) => {
                        let deleted = matches!(page.review, PageReview::Deleted(_));
                        if deleted {
                            kit::section_header(ui, &page.name, None);
                            ui.label(RichText::new("Page deleted").color(p.danger));
                        }
                        for g in &d.groups {
                            ui.add_space(8.0);
                            kit::section_header(ui, &format!("{} · {}", g.name, g.nodes.len()), None);
                            for n in &g.nodes {
                                diff_row(ui, &page.page_id, n, deleted, &mut act);
                            }
                        }
                        if !d.variables.is_empty() {
                            ui.add_space(8.0);
                            kit::section_header(ui, &format!("Variables · {}", d.variables.len()), None);
                            for v in &d.variables {
                                let n = crate::mcp::diff::NodeDiff {
                                    id_a: None,
                                    id_b: None,
                                    path: String::new(),
                                    name: v.after.split(':').next().unwrap_or("").to_string(),
                                    kind: "variable",
                                    status: "modified",
                                    changes: vec![v.clone()],
                                };
                                variable_row(ui, &n);
                            }
                        }
                        if !d.annotations.is_empty() {
                            ui.add_space(8.0);
                            kit::section_header(ui, &format!("Notes · {}", d.annotations.len()), None);
                            for a in &d.annotations {
                                let n = crate::mcp::diff::NodeDiff {
                                    id_a: None,
                                    id_b: None,
                                    path: String::new(),
                                    name: a.label.clone(),
                                    kind: "annotation",
                                    status: a.status,
                                    changes: Vec::new(),
                                };
                                variable_row(ui, &n);
                            }
                        }
                    }
                }
            }
        });
    ui.add_space(8.0);
    let (mut revert, mut accept) = (false, false);
    ui.columns(2, |c| {
        revert = kit::button_wide(&mut c[0], kit::ButtonKind::Danger, Some(Icon::Restore), "Revert run").clicked();
        accept = kit::button_wide(&mut c[1], kit::ButtonKind::Primary, None, "Accept all").clicked();
    });
    match act {
        Some(ReviewAct::Open(page_id, node)) => {
            if let Some(i) = ed.pages.iter().position(|p| p.id == page_id) {
                ed.switch_page(i);
                if let Some(n) = node.filter(|n| ed.doc.get(*n).is_some()) {
                    ed.select(vec![n]);
                    if let Some(b) = ed.doc.world_rect(n) {
                        ed.camera.fit(&b, ed.viewport, 64.0, 1.0);
                    }
                    ed.revision += 1;
                }
            }
        }
        Some(ReviewAct::Undo(page_id, n)) => {
            if let Err(e) = crate::runs::revert_change(&dir, ed, &run, &page_id, &n) {
                ed.status = format!("Could not revert the change: {e}");
            }
        }
        None => {}
    }
    if back {
        st.review_run = None;
        st.review_diff = None;
    }
    if revert {
        st.pending_run_revert = Some(id);
    }
    if accept {
        run.state = RunState::Accepted;
        if let Err(e) = crate::runs::save(&dir, &run) {
            ed.status = format!("Could not save the run: {e}");
        }
        st.runs_key = None;
        st.review_run = None;
        st.review_diff = None;
    }
}

/// A Variables row: same look as a diff row, without the per-row revert.
fn variable_row(ui: &mut Ui, n: &crate::mcp::diff::NodeDiff) {
    let p = theme::pal(ui);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 48.0), egui::Sense::hover());
    let badge = egui::Rect::from_min_size(rect.left_center() - egui::vec2(0.0, 14.0), egui::vec2(28.0, 28.0));
    let (tint, icon) = match n.status {
        "added" => (p.success, Icon::Plus),
        "removed" => (p.danger, Icon::Minus),
        _ => (p.warning, Icon::Pencil),
    };
    ui.painter().rect_filled(badge, theme::radius_md(), tint.gamma_multiply(0.18));
    theme::paint_icon(ui.painter(), badge.shrink(6.0), icon, tint);
    let x = rect.left() + 40.0;
    let clip = ui.painter().with_clip_rect(rect);
    clip.text(egui::pos2(x, rect.center().y - 9.0), egui::Align2::LEFT_CENTER, &n.name, egui::FontId::proportional(theme::BODY), p.text);
    clip.text(egui::pos2(x, rect.center().y + 10.0), egui::Align2::LEFT_CENTER, detail(n), egui::FontId::proportional(theme::SECONDARY), p.secondary);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::Rect;
    use crate::model::{Node, NodeKind};

    fn doc_with_frame() -> crate::model::Document {
        let mut doc = crate::model::Document::default();
        let id = doc.alloc_id();
        let mut n = Node::new(id, NodeKind::Frame { clip: true }, Rect::new(0.0, 0.0, 320.0, 200.0));
        n.name = "Card".into();
        doc.nodes.insert(id, n);
        doc.children_mut(None).unwrap().push(id);
        doc
    }

    #[test]
    fn deleted_page_diffs_snapshot_against_empty() {
        let snap = doc_with_frame();
        let PageReview::Deleted(d) = page_review(true, Some(&snap), None) else {
            panic!("expected Deleted");
        };
        assert!(d.removed > 0);
        assert_eq!(d.added, 0);
    }

    #[test]
    fn missing_snapshot_stays_missing() {
        let live = doc_with_frame();
        assert!(matches!(page_review(true, None, Some(&live)), PageReview::Missing));
        assert!(matches!(page_review(true, None, None), PageReview::Missing));
    }

    #[test]
    fn page_without_version_is_created() {
        let live = doc_with_frame();
        assert!(matches!(page_review(false, None, Some(&live)), PageReview::Created));
    }
}
