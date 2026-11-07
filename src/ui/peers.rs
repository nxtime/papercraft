//! Other devices on the same shared project: their cursors and selections, each in its own
//! colour with the device's name, fading when the device is idle.
//!
//! Drawn only; nothing here touches the document. This device's own pointer and selection go to
//! the presence link, which reports them to the relay (`crate::presence`).

use super::{UiState, kit, theme};
use crate::editor::Editor;
use crate::geom::V2;
use crate::model::NodeId;
use crate::presence::{FADE_START, GONE_AFTER, Report, hue};
use egui::{Color32, Pos2, Rect, Stroke, StrokeKind, vec2};
use std::time::Duration;

const CURSOR: f32 = 12.0;
const OUTLINE: f32 = 1.5;
const LABEL_MAX: usize = 24;
/// How dim an almost-gone device gets.
const MIN_ALPHA: f32 = 0.25;
/// Repaint rate while other devices are shown (their data arrives off the UI thread), and the
/// slower one that still lets a newcomer appear on an otherwise idle canvas.
const BUSY_REPAINT: Duration = Duration::from_millis(500);
const IDLE_REPAINT: Duration = Duration::from_secs(1);

/// Opacity for a device idle for `idle` seconds: full, then fading toward [`MIN_ALPHA`].
pub fn alpha(idle: u32) -> f32 {
    if idle <= FADE_START {
        return 1.0;
    }
    let t = (idle - FADE_START) as f32 / (GONE_AFTER - FADE_START) as f32;
    (1.0 - t * (1.0 - MIN_ALPHA)).clamp(MIN_ALPHA, 1.0)
}

fn colour_of(id: &str) -> Color32 {
    egui::ecolor::Hsva::new(hue(id) / 360.0, 0.62, 0.92, 1.0).into()
}

fn name_of(label: &str) -> String {
    let name = if label.is_empty() { "Device" } else { label };
    if name.chars().count() > LABEL_MAX {
        name.chars().take(LABEL_MAX - 1).chain(std::iter::once('\u{2026}')).collect()
    } else {
        name.to_string()
    }
}

/// Reports this device's pointer and selection, and draws the other devices on the open page.
pub fn overlay(ctx: &egui::Context, ed: &Editor, st: &UiState, canvas: Rect) {
    let Some(link) = st.presence.as_ref().filter(|l| l.is_supported()) else {
        return;
    };
    let cursor = ctx.input(|i| i.pointer.hover_pos()).filter(|p| canvas.contains(*p)).map(|p| {
        let w = ed.screen_to_world(V2 { x: f64::from(p.x - canvas.min.x), y: f64::from(p.y - canvas.min.y) });
        [w.x as f32, w.y as f32]
    });
    let selection: Vec<u64> = ed.selection.iter().map(|n| n.0).collect();
    let page = ed.active_page_id();
    link.set_local(page, cursor, &selection);

    let here: Vec<Report> = link.peers().into_iter().filter(|r| r.page == page).collect();
    ctx.request_repaint_after(if here.is_empty() { IDLE_REPAINT } else { BUSY_REPAINT });
    if here.is_empty() {
        return;
    }
    egui::Area::new("peer-presence".into())
        .order(egui::Order::Middle)
        .fixed_pos(canvas.min)
        .interactable(false)
        .show(ctx, |ui| {
            let painter = ui.painter().with_clip_rect(canvas);
            for r in &here {
                paint(&painter, ed, canvas, r);
            }
        });
}

fn paint(painter: &egui::Painter, ed: &Editor, canvas: Rect, r: &Report) {
    let (colour, a) = (colour_of(&r.id), alpha(r.idle));
    let tint = colour.gamma_multiply(a);
    let mut first = None;
    for id in &r.sel {
        let Some(w) = ed.doc.world_rect(NodeId(*id)) else { continue };
        let s = ed.camera.world_rect_to_screen(&w);
        let rect = Rect::from_min_size(canvas.min + vec2(s.x as f32, s.y as f32), vec2(s.w as f32, s.h as f32));
        painter.rect_stroke(rect, 2.0, Stroke::new(OUTLINE, tint), StrokeKind::Outside);
        first.get_or_insert(rect.left_top());
    }
    let tip = match r.cursor {
        Some([x, y]) => {
            let s = ed.camera.world_to_screen(V2 { x: f64::from(x), y: f64::from(y) });
            let tip = canvas.min + vec2(s.x as f32, s.y as f32);
            let arrow = vec![tip, tip + vec2(0.0, CURSOR), tip + vec2(CURSOR * 0.75, CURSOR * 0.75)];
            painter.add(egui::Shape::convex_polygon(arrow, tint, Stroke::NONE));
            Some(tip + vec2(CURSOR * 0.6, CURSOR * 0.9))
        }
        // No pointer on the canvas: the name sits above what the device has selected.
        None => first.map(|p: Pos2| p - vec2(0.0, 22.0)),
    };
    let Some(at) = tip else { return };
    let ink = kit::ink_on(colour).gamma_multiply(a);
    let galley = painter.layout_no_wrap(name_of(&r.label), theme::semibold(theme::SECONDARY), ink);
    let tag = Rect::from_min_size(at, galley.size() + vec2(12.0, 4.0));
    painter.rect_filled(tag, theme::r(255.0), tint);
    painter.galley(tag.min + vec2(6.0, 2.0), galley, ink);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn devices_fade_when_idle_but_never_vanish_before_the_relay_drops_them() {
        assert_eq!(alpha(0), 1.0);
        assert_eq!(alpha(FADE_START), 1.0);
        assert!(alpha(30) < 1.0 && alpha(30) > MIN_ALPHA);
        assert!((alpha(GONE_AFTER) - MIN_ALPHA).abs() < 1e-6);
    }

    #[test]
    fn names_are_short_and_never_empty() {
        assert_eq!(name_of(""), "Device");
        assert_eq!(name_of(&"n".repeat(60)).chars().count(), LABEL_MAX);
    }

    #[test]
    fn each_device_keeps_its_colour() {
        assert_eq!(colour_of("dev-a"), colour_of("dev-a"));
    }
}
