//! The grip between a side panel and the canvas. The panels float with a gap around them, so the
//! grip sits in the middle of that gap: a slim rounded pill that is faint at rest, lengthens and
//! brightens under the pointer and takes the accent colour while dragging. A double click puts the
//! panel back to its default width.

use super::motion;
use crate::theme;
use egui::{Area, Context, CursorIcon, Id, Order, Rect, Sense, Vec2, pos2, vec2};

/// Which side of the canvas the panel is on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Side {
    Left,
    Right,
}

/// Width of the grabbable strip, and the pill's thickness at rest and when active.
const STRIP_MIN_W: f32 = 14.0;
const PILL_W: f32 = 4.0;
/// Pill length at rest and when hovered or dragged.
const PILL_H: f32 = 36.0;
const PILL_H_ACTIVE: f32 = 56.0;
/// Opacity of the pill at rest.
const REST_ALPHA: f32 = 0.35;

/// What the grip asked for this frame.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Grip {
    /// Points the panel should grow by (negative: shrink).
    pub delta: f32,
    /// A double click: back to the default width.
    pub reset: bool,
}

/// Where the grip's centre line is, for a panel occupying `panel` with `gap` of empty margin on
/// its canvas side.
pub fn centre_x(panel: Rect, side: Side, gap: f32) -> f32 {
    match side {
        Side::Left => panel.right() - gap / 2.0,
        Side::Right => panel.left() + gap / 2.0,
    }
}

/// `width` plus `delta`, kept inside `range`.
pub fn resized(width: f32, delta: f32, range: std::ops::RangeInclusive<f32>) -> f32 {
    (width + delta).clamp(*range.start(), (*range.end()).max(*range.start()))
}

/// Draws the grip and reads the drag. `id` must be unique per panel.
pub fn show(ctx: &Context, id: Id, panel: Rect, side: Side, gap: f32) -> Grip {
    let strip_w = (gap + 8.0).max(STRIP_MIN_W);
    let x = centre_x(panel, side, gap);
    let strip = Rect::from_center_size(pos2(x, panel.center().y), vec2(strip_w, panel.height()));
    let mut grip = Grip::default();
    Area::new(id).order(Order::Foreground).fixed_pos(strip.min).show(ctx, |ui| {
        let (rect, resp) = ui.allocate_exact_size(strip.size(), Sense::click_and_drag());
        if resp.hovered() || resp.dragged() {
            ctx.set_cursor_icon(CursorIcon::ResizeHorizontal);
        }
        let active = resp.hovered() || resp.dragged();
        let t = motion::progress(ctx, id.with("grip"), active, motion::FAST, motion::ease_out_cubic);
        let p = theme::pal(ui);
        let ink = if resp.dragged() { theme::accent() } else { motion::mix(p.tertiary, p.secondary, t) };
        let ink = ink.gamma_multiply(motion::lerp(REST_ALPHA, 1.0, t));
        let h = motion::lerp(PILL_H, PILL_H_ACTIVE, t);
        let pill = Rect::from_center_size(rect.center(), Vec2::new(PILL_W, h));
        ui.painter().rect_filled(pill, PILL_W / 2.0, ink);
        let sign = if side == Side::Left { 1.0 } else { -1.0 };
        grip.delta = resp.drag_delta().x * sign;
        grip.reset = resp.double_clicked();
    });
    grip
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_grip_sits_in_the_middle_of_the_gap() {
        let panel = Rect::from_min_max(pos2(0.0, 0.0), pos2(264.0, 800.0));
        assert_eq!(centre_x(panel, Side::Left, 12.0), 258.0);
        let right = Rect::from_min_max(pos2(1000.0, 0.0), pos2(1272.0, 800.0));
        assert_eq!(centre_x(right, Side::Right, 12.0), 1006.0);
    }

    #[test]
    fn resizing_stays_inside_the_range() {
        assert_eq!(resized(250.0, 30.0, 190.0..=400.0), 280.0);
        assert_eq!(resized(250.0, -200.0, 190.0..=400.0), 190.0);
        assert_eq!(resized(390.0, 50.0, 190.0..=400.0), 400.0);
        assert_eq!(resized(250.0, 10.0, 190.0..=100.0), 190.0, "an inverted range doesn't panic");
    }
}
