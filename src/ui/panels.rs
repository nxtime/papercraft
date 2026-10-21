//! Sidebar views beyond layers, library and design: history, comments and the agent log.

use super::{UiState, kit};
use crate::editor::Editor;
use crate::model::NodeId;
use crate::theme::{self, Icon};
use egui::{RichText, Ui};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SideView {
    History,
    Comments,
    Agent,
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
        ui.painter().rect_filled(rect, theme::RADIUS_LG, p.field);
    } else if hover > 0.0 {
        ui.painter()
            .rect_filled(rect, theme::RADIUS_LG, p.hover.gamma_multiply(hover));
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
    ui.add(
        egui::TextEdit::singleline(&mut st.version_draft)
            .hint_text("Name this version")
            .desired_width(f32::INFINITY)
            .margin(theme::INPUT_MARGIN),
    );
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
                    let r = ui.add(
                        egui::TextEdit::singleline(draft)
                            .desired_width(f32::INFINITY)
                            .margin(theme::INPUT_MARGIN),
                    );
                    r.request_focus();
                    if r.lost_focus() {
                        if !ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                            rename = Some((id.clone(), draft.clone()));
                        }
                        st.version_renaming = None;
                    }
                    continue;
                }
                let resp = row(ui, dot, &v.name, &sub, false, v.auto)
                    .on_hover_text("Click to restore this version");
                if resp.clicked() {
                    restore = Some(v.id.clone());
                }
                resp.context_menu(|ui| {
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
                        .corner_radius(theme::RADIUS_PANEL)
                        .inner_margin(12)
                        .show(ui, |ui| {
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
        ui.add(
            egui::TextEdit::singleline(&mut st.feedback_draft)
                .hint_text(if target.is_some() {
                    "Note for the agent…"
                } else {
                    "Select a layer to add a note"
                })
                .desired_width(f32::INFINITY)
                .margin(theme::INPUT_MARGIN),
        );
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

/// Where an agent connects, and the tool calls it ran: name, outcome and how long ago.
pub fn agent(ui: &mut Ui, st: &mut UiState) {
    let p = theme::pal(ui);
    title(ui, "Agent", "Tools an agent ran on this project.");
    let live = st
        .agent
        .as_ref()
        .is_some_and(|(_, at)| at.elapsed().as_secs() < 8);
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
    egui::ScrollArea::vertical()
        .id_salt("agent")
        .auto_shrink(false)
        .show(ui, |ui| {
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
        });
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_secs(5));
}
