//! Comment threads as cards: the root note, its replies with author and age, `@mentions`
//! highlighted, resolve / reopen, and a reply box. Shared by the Comments view and the inspector.

use super::{UiState, kit};
use crate::editor::Editor;
use crate::model::NodeId;
use crate::model::comment::{Comment, spans};
use crate::theme;
use egui::{Color32, RichText, Ui, text::LayoutJob};

/// The local author of comments made in the UI.
const USER: &str = "user";
const MAX_LEN: usize = 500;

/// Text that wraps, with `@mentions` in the accent colour.
pub fn mention_text(ui: &mut Ui, text: &str, size: f32, color: Color32) {
    let mut job = LayoutJob::default();
    job.wrap.max_width = ui.available_width();
    for (run, mention) in spans(text) {
        let font = if mention { theme::semibold(size) } else { egui::FontId::proportional(size) };
        let c = if mention { theme::accent() } else { color };
        job.append(run, 0.0, egui::TextFormat { font_id: font, color: c, ..Default::default() });
    }
    ui.add(egui::Label::new(job).wrap());
}

/// "now", "5m", "3h" or "2d" since `created` (empty for a note made before timestamps).
pub fn age(created: u64, now: u64) -> String {
    if created == 0 {
        return String::new();
    }
    let s = now.saturating_sub(created) / 1000;
    match s {
        0..=59 => "now".into(),
        60..=3599 => format!("{}m", s / 60),
        3600..=86399 => format!("{}h", s / 3600),
        _ => format!("{}d", s / 86400),
    }
}

/// What a card asks of its caller.
#[derive(Default)]
pub struct Action {
    pub select_layer: bool,
}

/// One thread on `node`. `layer` names the layer when the card is not already under it.
pub fn thread(ui: &mut Ui, ed: &mut Editor, st: &mut UiState, node: NodeId, c: &Comment, layer: Option<&str>) -> Action {
    let p = theme::pal(ui);
    let now = crate::project::format::now_ms();
    let ink = if c.resolved { p.secondary } else { p.text };
    let mut act = Action::default();
    let mut resolve = None;
    let mut delete = false;
    ui.horizontal_wrapped(|ui| {
        if let Some(name) = layer {
            let r = ui.add(egui::Label::new(RichText::new(name).font(theme::semibold(theme::BODY))).truncate().sense(egui::Sense::click()));
            act.select_layer = r.on_hover_text("Select this layer").clicked();
        }
        if kit::chip(ui, if c.resolved { "Reopen" } else { "Resolve" }, !c.resolved).clicked() {
            resolve = Some(!c.resolved);
        }
        if kit::chip(ui, "Delete", false).on_hover_text("Delete this thread").clicked() {
            delete = true;
        }
    });
    let head = |author: &str, created: u64| {
        let a = age(created, now);
        if a.is_empty() { author.to_string() } else { format!("{author} · {a}") }
    };
    ui.label(RichText::new(head(&c.author, c.created)).size(theme::SECONDARY).color(p.secondary));
    mention_text(ui, &c.text, theme::BODY, ink);
    if let Some(r) = &c.reply {
        mention_text(ui, &format!("↳ {r}"), theme::SECONDARY, p.secondary);
    }
    for r in &c.replies {
        ui.add_space(2.0);
        ui.label(RichText::new(head(&r.author, r.created)).size(theme::SECONDARY).color(p.secondary));
        mention_text(ui, &r.text, theme::BODY, ink);
    }
    let key = (node, c.id);
    let draft = st.reply_drafts.entry(key).or_default();
    let r = kit::icon_field(ui, theme::Icon::Note, draft, "Reply…");
    let send = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
    if send && !draft.trim().is_empty() {
        let text: String = std::mem::take(draft).trim().chars().take(MAX_LEN).collect();
        ed.edit("Reply to feedback", |tx| {
            if let Some(n) = tx.node_mut(node) {
                n.reply_to(c.id, USER, &text, now);
            }
        });
    }
    if let Some(done) = resolve {
        ed.edit(if done { "Resolve feedback" } else { "Reopen feedback" }, |tx| {
            if let Some(n) = tx.node_mut(node) {
                n.set_thread_resolved(c.id, done);
            }
        });
    }
    if delete {
        st.reply_drafts.remove(&key);
        ed.edit("Delete feedback", |tx| {
            if let Some(n) = tx.node_mut(node) {
                n.comments.retain(|x| x.id != c.id);
            }
        });
    }
    act
}

/// Start a thread on `node` from the UI.
pub fn post(ed: &mut Editor, node: NodeId, text: &str) {
    let text: String = text.trim().chars().take(MAX_LEN).collect();
    let now = crate::project::format::now_ms();
    ed.edit("Add feedback", |tx| {
        if let Some(n) = tx.node_mut(node) {
            n.post_comment(USER, &text, now, None);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::age;

    #[test]
    fn age_is_short_and_empty_for_old_notes() {
        assert_eq!(age(0, 99), "");
        assert_eq!(age(1000, 1000 + 5 * 60_000), "5m");
        assert_eq!(age(1000, 1000 + 3 * 3_600_000), "3h");
        assert_eq!(age(1000, 1000 + 2 * 86_400_000), "2d");
        assert_eq!(age(5000, 1000), "now");
    }
}
