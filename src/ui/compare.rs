//! Version compare: what changed between two saved versions of the page, or between a version
//! and the page as it is now. Read-only: it never edits the document or touches the undo history.

use super::panels::{self, ReviewAct};
use super::{kit, UiState};
use crate::editor::Editor;
use crate::mcp::diff::PageDiff;
use crate::model::Document;
use crate::theme;
use egui::{RichText, Ui};

/// An open compare: `a` (a version id) against `b` (another version, or the live page).
pub struct Compare {
    pub page_id: String,
    pub a: String,
    pub b: Option<String>,
    /// Document stamp and history generation the diff was built at; only the live side moves.
    stamp: (u64, u64),
    before: Document,
    diff: PageDiff,
}

impl Compare {
    /// `None` when a version file cannot be read.
    pub fn open(dir: &std::path::Path, ed: &Editor, a: String, b: Option<String>) -> Option<Self> {
        let (before, diff) = crate::versions::compare(dir, &a, b.as_deref(), &ed.doc)?;
        Some(Self {
            page_id: ed.active_page_id().to_owned(),
            a,
            b,
            stamp: (ed.doc_stamp, ed.history.generation()),
            before,
            diff,
        })
    }

    fn refresh(&mut self, dir: &std::path::Path, ed: &Editor) {
        let now = (ed.doc_stamp, ed.history.generation());
        if self.b.is_some() || self.stamp == now {
            return;
        }
        if let Some((before, diff)) = crate::versions::compare(dir, &self.a, None, &ed.doc) {
            self.before = before;
            self.diff = diff;
            self.stamp = now;
        }
    }
}

fn label(versions: &[crate::versions::Version], id: Option<&str>) -> String {
    id.map_or_else(
        || "Current page".to_owned(),
        |id| versions.iter().find(|v| v.id == id).map_or_else(|| "Missing version".to_owned(), |v| v.name.clone()),
    )
}

/// One side's picker. `live` adds the "Current page" choice (the To side only).
fn pick(ui: &mut Ui, title: &str, versions: &[crate::versions::Version], cur: &mut Option<String>, live: bool) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(title).size(theme::SECONDARY).color(theme::pal(ui).secondary));
        egui::ComboBox::from_id_salt(("compare-side", title))
            .width(ui.available_width())
            .selected_text(label(versions, cur.as_deref()))
            .show_ui(ui, |ui| {
                if live {
                    ui.selectable_value(cur, None, "Current page");
                }
                for v in versions {
                    ui.selectable_value(cur, Some(v.id.clone()), &v.name);
                }
            });
    });
}

/// The compare view in the History panel. Returns false once the user leaves it.
pub(super) fn view(ui: &mut Ui, ed: &mut Editor, st: &mut UiState) -> bool {
    let p = theme::pal(ui);
    let (Some(dir), Some(mut cmp)) = (st.project_dir.clone(), st.compare.take()) else {
        return false;
    };
    if cmp.page_id != ed.active_page_id() {
        return false;
    }
    cmp.refresh(&dir, ed);
    let mut from = Some(cmp.a.clone());
    let mut to = cmp.b.clone();
    let mut back = false;
    ui.horizontal(|ui| {
        ui.label(RichText::new("Compare versions").font(theme::semibold(16.0)));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            back = kit::close_button(ui).on_hover_text("Back to history").clicked();
        });
    });
    ui.add_space(8.0);
    pick(ui, "From", &st.versions, &mut from, false);
    pick(ui, "To", &st.versions, &mut to, true);
    ui.add_space(8.0);
    let d = &cmp.diff;
    ui.horizontal(|ui| {
        kit::chip_ink(ui, &format!("+{}", d.added), Some(p.success));
        kit::chip_ink(ui, &format!("~{}", d.modified), Some(p.warning));
        kit::chip_ink(ui, &format!("−{}", d.removed), Some(p.danger));
        ui.label(
            RichText::new(format!("on {}", panels::count(d.groups.len(), "frame")))
                .size(theme::SECONDARY)
                .color(p.secondary),
        );
    });
    ui.add_space(8.0);
    let live = cmp.b.is_none();
    if d.is_empty() {
        ui.label(RichText::new("No differences.").size(theme::SECONDARY).color(p.secondary));
    } else if !live {
        ui.label(
            RichText::new("Two saved versions: the canvas keeps showing the page as it is now.")
                .size(theme::SECONDARY)
                .color(p.secondary),
        );
    }
    let mut act = None;
    egui::ScrollArea::vertical()
        .id_salt("compare")
        .max_height((ui.available_height() - 8.0).max(80.0))
        .auto_shrink(false)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 4.0;
            for g in &d.groups {
                ui.add_space(8.0);
                kit::section_header(ui, &format!("{} · {}", g.name, g.nodes.len()), None);
                for n in &g.nodes {
                    // Read-only rows: no revert button.
                    panels::diff_row(ui, &cmp.page_id, n, true, &mut act);
                }
            }
            if !d.variables.is_empty() {
                ui.add_space(8.0);
                kit::section_header(ui, &format!("Variables · {}", d.variables.len()), None);
                for v in &d.variables {
                    panels::variable_row(ui, &panels::property_row(v));
                }
            }
        });
    // Only the live side has layers to jump to.
    if let (Some(ReviewAct::Open(_, Some(n))), true) = (act, live)
        && ed.doc.get(n).is_some()
    {
        ed.select(vec![n]);
        if let Some(r) = ed.doc.world_rect(n) {
            ed.camera.fit(&r, ed.viewport, 64.0, 1.0);
        }
        ed.revision += 1;
    }
    if to != cmp.b || from.as_deref() != Some(cmp.a.as_str()) {
        let a = from.unwrap_or_else(|| cmp.a.clone());
        match Compare::open(&dir, ed, a, to) {
            Some(next) => cmp = next,
            None => ed.status = "Could not read that version".into(),
        }
    }
    if back {
        return false;
    }
    st.compare = Some(cmp);
    true
}

/// Outlines on the canvas for the open compare, when it is against the live page.
pub(super) fn overlay(ctx: &egui::Context, ed: &Editor, st: &UiState, canvas: egui::Rect) {
    if let Some(c) = &st.compare
        && c.b.is_none()
        && c.page_id == ed.active_page_id()
    {
        super::diff_overlay(ctx, ed, canvas, "compare-overlay", &c.diff, Some(&c.before));
    }
}
