//! Icons of the menu bar's commands, on the same 16x16 grid as [`super::paint_icon`].

use super::{Icon, arc};
use egui::{Color32, Painter, Pos2, Rect, Stroke, Vec2, pos2, vec2};

/// Faint enough to tell the area a boolean keeps from the outlines it started with.
const KEPT_FILL: f32 = 0.6;
/// Two squares overlapping on the diagonal, the operands of every boolean icon.
const OPERAND_A: (f32, f32, f32, f32) = (2.5, 2.5, 10.5, 10.5);
const OPERAND_B: (f32, f32, f32, f32) = (5.5, 5.5, 13.5, 13.5);
const OVERLAP: (f32, f32, f32, f32) = (5.5, 5.5, 10.5, 10.5);

pub(super) fn paint(painter: &Painter, rect: Rect, icon: Icon, color: Color32) {
    let s = rect.width().min(rect.height());
    let c = rect.center();
    let u = s / 16.0;
    let pt = |x: f32, y: f32| -> Pos2 { pos2(c.x + (x - 8.0) * u, c.y + (y - 8.0) * u) };
    let st = Stroke::new((1.25 * u).clamp(1.0, 1.6), color);
    let line = |pts: &[Pos2]| painter.add(egui::Shape::line(pts.to_vec(), st));
    let closed = |pts: &[Pos2]| painter.add(egui::Shape::closed_line(pts.to_vec(), st));
    let boxed = |(x0, y0, x1, y1): (f32, f32, f32, f32)| {
        closed(&[pt(x0, y0), pt(x1, y0), pt(x1, y1), pt(x0, y1)]);
    };
    let fill = |(x0, y0, x1, y1): (f32, f32, f32, f32)| {
        painter.rect_filled(
            Rect::from_min_max(pt(x0, y0), pt(x1, y1)),
            0.0,
            color.gamma_multiply(KEPT_FILL),
        );
    };
    // Sheets stacked towards the top left, the shape of every copy command.
    let stacked = || {
        line(&[
            pt(3.5, 10.5),
            pt(2.5, 10.5),
            pt(2.5, 2.5),
            pt(10.5, 2.5),
            pt(10.5, 3.5),
        ]);
        boxed((5.5, 5.5, 13.5, 13.5));
    };
    let lens = || {
        painter.circle_stroke(pt(7.0, 7.0), 4.5 * u, st);
        line(&[pt(10.4, 10.4), pt(13.8, 13.8)]);
    };
    let arrow_up = |top: f32, bottom: f32| {
        line(&[pt(8.0, bottom), pt(8.0, top)]);
        line(&[pt(5.0, top + 3.0), pt(8.0, top), pt(11.0, top + 3.0)]);
    };
    let arrow_down = |top: f32, bottom: f32| {
        line(&[pt(8.0, top), pt(8.0, bottom)]);
        line(&[
            pt(5.0, bottom - 3.0),
            pt(8.0, bottom),
            pt(11.0, bottom - 3.0),
        ]);
    };
    let bars = || {
        boxed((2.5, 3.0, 8.5, 6.5));
        boxed((2.5, 9.5, 8.5, 13.0));
    };
    match icon {
        Icon::Redo => {
            line(&arc(c, 5.5 * u, 5.74, 0.94, 14));
            let start = c + 5.5 * u * Vec2::angled(5.74);
            line(&[
                start + vec2(0.5 * u, -3.0 * u),
                start,
                start + vec2(-3.0 * u, 0.3 * u),
            ]);
        }
        Icon::Cut => {
            painter.circle_stroke(pt(4.5, 12.0), 2.0 * u, st);
            painter.circle_stroke(pt(11.5, 12.0), 2.0 * u, st);
            line(&[pt(5.6, 10.3), pt(11.5, 2.5)]);
            line(&[pt(10.4, 10.3), pt(4.5, 2.5)]);
        }
        Icon::Copy => stacked(),
        Icon::Paste => {
            boxed((3.0, 3.5, 13.0, 14.0));
            boxed((6.0, 2.0, 10.0, 5.0));
        }
        Icon::Duplicate => {
            stacked();
            line(&[pt(9.5, 7.8), pt(9.5, 11.2)]);
            line(&[pt(7.8, 9.5), pt(11.2, 9.5)]);
        }
        Icon::Save => {
            closed(&[
                pt(2.5, 2.5),
                pt(11.0, 2.5),
                pt(13.5, 5.0),
                pt(13.5, 13.5),
                pt(2.5, 13.5),
            ]);
            line(&[pt(5.0, 13.5), pt(5.0, 9.0), pt(11.0, 9.0), pt(11.0, 13.5)]);
            line(&[pt(5.5, 2.5), pt(5.5, 5.5), pt(10.0, 5.5)]);
        }
        Icon::Ungroup => {
            boxed((2.5, 2.5, 7.0, 7.0));
            boxed((9.0, 9.0, 13.5, 13.5));
            line(&[pt(10.0, 3.0), pt(13.0, 6.0)]);
            line(&[pt(13.0, 3.0), pt(10.0, 6.0)]);
        }
        Icon::FlipH => {
            painter.add(egui::Shape::dashed_line(
                &[pt(8.0, 2.0), pt(8.0, 14.0)],
                st,
                1.6 * u,
                1.4 * u,
            ));
            closed(&[pt(5.8, 4.5), pt(5.8, 12.0), pt(2.0, 12.0)]);
            closed(&[pt(10.2, 4.5), pt(10.2, 12.0), pt(14.0, 12.0)]);
        }
        Icon::FlipV => {
            painter.add(egui::Shape::dashed_line(
                &[pt(2.0, 8.0), pt(14.0, 8.0)],
                st,
                1.6 * u,
                1.4 * u,
            ));
            closed(&[pt(4.5, 5.8), pt(12.0, 5.8), pt(12.0, 2.0)]);
            closed(&[pt(4.5, 10.2), pt(12.0, 10.2), pt(12.0, 14.0)]);
        }
        Icon::Magnet => {
            let pi = std::f32::consts::PI;
            let mut shape = vec![pt(3.0, 2.5)];
            shape.extend(arc(pt(8.0, 8.0), 5.0 * u, pi, 0.0, 12));
            shape.extend([pt(13.0, 2.5), pt(10.0, 2.5)]);
            shape.extend(arc(pt(8.0, 8.0), 2.0 * u, 0.0, pi, 8));
            shape.push(pt(6.0, 2.5));
            closed(&shape);
            line(&[pt(3.0, 5.5), pt(6.0, 5.5)]);
            line(&[pt(10.0, 5.5), pt(13.0, 5.5)]);
        }
        Icon::Flatten => {
            arrow_down(2.5, 9.5);
            line(&[pt(2.5, 13.0), pt(13.5, 13.0)]);
        }
        Icon::Zoom100 => {
            lens();
            line(&[pt(5.5, 5.6), pt(5.5, 8.6)]);
            line(&[pt(8.5, 5.6), pt(8.5, 8.6)]);
            painter.circle_filled(pt(7.0, 6.2), 0.6 * u, color);
            painter.circle_filled(pt(7.0, 8.0), 0.6 * u, color);
        }
        Icon::ZoomSelection => {
            lens();
            boxed((5.3, 5.3, 8.7, 8.7));
        }
        Icon::PageNext => {
            boxed((2.5, 2.5, 8.5, 13.5));
            line(&[pt(6.5, 8.0), pt(13.5, 8.0)]);
            line(&[pt(11.0, 5.5), pt(13.5, 8.0), pt(11.0, 10.5)]);
        }
        Icon::PagePrev => {
            boxed((7.5, 2.5, 13.5, 13.5));
            line(&[pt(9.5, 8.0), pt(2.5, 8.0)]);
            line(&[pt(5.0, 5.5), pt(2.5, 8.0), pt(5.0, 10.5)]);
        }
        Icon::Union => {
            fill(OPERAND_A);
            fill((10.5, 5.5, 13.5, 10.5));
            fill((5.5, 10.5, 13.5, 13.5));
            closed(&[
                pt(2.5, 2.5),
                pt(10.5, 2.5),
                pt(10.5, 5.5),
                pt(13.5, 5.5),
                pt(13.5, 13.5),
                pt(5.5, 13.5),
                pt(5.5, 10.5),
                pt(2.5, 10.5),
            ]);
        }
        Icon::Subtract => {
            fill((2.5, 2.5, 10.5, 5.5));
            fill((2.5, 5.5, 5.5, 10.5));
            boxed(OPERAND_A);
            boxed(OPERAND_B);
        }
        Icon::Intersect => {
            fill(OVERLAP);
            boxed(OPERAND_A);
            boxed(OPERAND_B);
        }
        Icon::Exclude => {
            fill((2.5, 2.5, 10.5, 5.5));
            fill((2.5, 5.5, 5.5, 10.5));
            fill((10.5, 5.5, 13.5, 13.5));
            fill((5.5, 10.5, 10.5, 13.5));
            boxed(OPERAND_A);
            boxed(OPERAND_B);
        }
        Icon::LayoutAdd => {
            bars();
            line(&[pt(12.0, 5.5), pt(12.0, 10.5)]);
            line(&[pt(9.5, 8.0), pt(14.5, 8.0)]);
        }
        Icon::LayoutRemove => {
            bars();
            line(&[pt(9.5, 8.0), pt(14.5, 8.0)]);
        }
        Icon::Forward => {
            boxed((3.0, 9.5, 13.0, 13.5));
            arrow_up(2.5, 9.5);
        }
        Icon::Backward => {
            boxed((3.0, 2.5, 13.0, 6.5));
            arrow_down(6.5, 13.5);
        }
        Icon::ToFront => {
            line(&[pt(3.0, 2.5), pt(13.0, 2.5)]);
            arrow_up(5.0, 14.0);
        }
        Icon::ToBack => {
            line(&[pt(3.0, 13.5), pt(13.0, 13.5)]);
            arrow_down(2.0, 11.0);
        }
        Icon::CopyProps => {
            closed(&[
                pt(2.5, 13.5),
                pt(3.0, 11.0),
                pt(9.0, 5.0),
                pt(11.0, 7.0),
                pt(5.0, 13.0),
            ]);
            line(&[pt(7.8, 3.8), pt(12.2, 8.2)]);
            line(&[pt(10.0, 6.0), pt(12.0, 4.0)]);
        }
        Icon::PasteProps => {
            boxed((2.5, 2.5, 11.5, 6.5));
            line(&[
                pt(11.5, 4.5),
                pt(13.5, 4.5),
                pt(13.5, 8.5),
                pt(8.0, 8.5),
                pt(8.0, 10.0),
            ]);
            boxed((7.0, 10.0, 9.0, 14.0));
        }
        Icon::Presentation => {
            boxed((2.5, 3.0, 13.5, 10.5));
            line(&[pt(8.0, 10.5), pt(8.0, 13.5)]);
            line(&[pt(5.5, 13.5), pt(10.5, 13.5)]);
        }
        Icon::Bold => {
            let half = std::f32::consts::FRAC_PI_2;
            let bowl = |top: f32, bottom: f32, reach: f32| {
                let r = (bottom - top) / 2.0;
                let mut shape = vec![pt(4.5, top), pt(reach, top)];
                shape.extend(arc(pt(reach, top + r), r * u, -half, half, 8));
                shape.push(pt(4.5, bottom));
                closed(&shape);
            };
            bowl(3.0, 8.0, 8.5);
            bowl(8.0, 13.0, 9.0);
        }
        Icon::Italic => {
            line(&[pt(9.5, 3.0), pt(6.5, 13.0)]);
            line(&[pt(7.0, 3.0), pt(12.0, 3.0)]);
            line(&[pt(4.0, 13.0), pt(9.0, 13.0)]);
        }
        Icon::Underline => {
            let mut shape = vec![pt(4.5, 2.5)];
            shape.extend(arc(pt(8.0, 7.5), 3.5 * u, std::f32::consts::PI, 0.0, 10));
            shape.push(pt(11.5, 2.5));
            line(&shape);
            line(&[pt(3.0, 13.5), pt(13.0, 13.5)]);
        }
        Icon::Strikethrough => {
            line(&[
                pt(11.5, 4.0),
                pt(9.5, 3.0),
                pt(6.5, 3.0),
                pt(4.5, 4.5),
                pt(5.5, 6.5),
            ]);
            line(&[
                pt(10.5, 9.5),
                pt(11.0, 11.5),
                pt(9.0, 13.0),
                pt(6.5, 13.0),
                pt(4.5, 12.0),
            ]);
            line(&[pt(2.5, 8.0), pt(13.5, 8.0)]);
        }
        _ => {}
    }
}
