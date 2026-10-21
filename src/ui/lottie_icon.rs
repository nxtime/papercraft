//! Animated icons: Lottie shape animations (`assets/icons-anim`) played with the egui painter, tinted with the
//! caller's colour. An icon with no animation falls back to its static Lucide drawing.
//!
//! Two kinds exist. A *toggle* has an `off` and an `on` marker and runs forward to open and backward to close
//! ([`toggle`]); a *one-shot* has a `play` marker and runs once when triggered ([`play`]).

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::lottie::{Animation, Cap, Item, Join, Polyline};
use egui::{Color32, Id, Pos2, Rect, Shape, Stroke, Ui};

mod files;

/// Icon at rest: animated icons draw their first frame (the Lucide shape), others the app's own icon.
fn rest(ui: &Ui, name: &str, rect: Rect, tint: Color32) {
    if let Some(animation) = library().get(name) {
        let (off, _) = if animation.marker(OFF).is_some() {
            toggle_range(animation)
        } else {
            (animation.first_frame(), animation.last_frame())
        };
        paint_frame(ui, name, rect, tint, off);
    } else if let Some(icon) = static_icon(name) {
        crate::theme::paint_icon(ui.painter(), rect, icon, tint);
    }
}

/// The Lucide name of an app icon, when it has one (the inverse of [`static_icon`]).
#[must_use]
pub fn lucide_name(icon: crate::theme::Icon) -> Option<&'static str> {
    use crate::theme::Icon;
    Some(match icon {
        Icon::ChevronRight => "chevron-right",
        Icon::ChevronDown => "chevron-down",
        Icon::ChevronUp => "chevron-up",
        Icon::Plus => "plus",
        Icon::Trash => "trash-2",
        Icon::Sidebar => "panel-left",
        Icon::Hand => "hand",
        Icon::Pointer => "mouse-pointer-2",
        Icon::Pencil => "pencil",
        Icon::Pen => "pen",
        Icon::Eraser => "eraser",
        Icon::Text => "type",
        Icon::Rect => "square",
        Icon::Image => "image",
        Icon::Grid => "grid-3x3",
        Icon::Export => "download",
        Icon::Question => "circle-question-mark",
        _ => return None,
    })
}

/// The app's hand-drawn icon for a Lucide name, when it has one.
#[must_use]
pub fn static_icon(name: &str) -> Option<crate::theme::Icon> {
    use crate::theme::Icon;
    Some(match name {
        "chevron-right" => Icon::ChevronRight,
        "chevron-down" => Icon::ChevronDown,
        "chevron-up" => Icon::ChevronUp,
        "plus" => Icon::Plus,
        "minus" => Icon::Minus,
        "trash-2" => Icon::Trash,
        "lock" => Icon::Lock,
        "unlock" => Icon::LockOpen,
        "eye" => Icon::Eye,
        "eye-off" => Icon::EyeSlash,
        "panel-left" => Icon::Sidebar,
        "hand" => Icon::Hand,
        "mouse-pointer-2" => Icon::Pointer,
        "pencil" => Icon::Pencil,
        "pen" => Icon::Pen,
        "eraser" => Icon::Eraser,
        "type" => Icon::Text,
        "square" => Icon::Rect,
        "circle" => Icon::Ellipse,
        "image" => Icon::Image,
        "star" => Icon::Star,
        "grid-3x3" => Icon::Grid,
        "ellipsis" => Icon::More,
        "download" => Icon::Export,
        "circle-question-mark" => Icon::Question,
        _ => return None,
    })
}

/// Marker name of a toggle's resting frame.
const OFF: &str = "off";
/// Marker name of a toggle's final frame.
const ON: &str = "on";
const MIN_DURATION: f32 = 0.05;
/// How far past its rect (as a fraction of its side) an animated icon may draw.
const OVERSHOOT: f32 = 0.35;

fn library() -> &'static HashMap<&'static str, Animation> {
    static LIBRARY: OnceLock<HashMap<&'static str, Animation>> = OnceLock::new();
    LIBRARY.get_or_init(|| {
        files::FILES
            .iter()
            .filter_map(|(name, text)| {
                Animation::parse(text)
                    .ok()
                    .map(|animation| (*name, animation))
            })
            .collect()
    })
}

/// Whether `name` has an animation.
#[must_use]
pub fn has(name: &str) -> bool {
    library().contains_key(name)
}

/// The first and last frame to review an icon over: a toggle's `off`..`on`, a one-shot's whole length.
#[must_use]
pub fn range(name: &str) -> Option<(f32, f32)> {
    let animation = library().get(name)?;
    Some(if animation.marker(OFF).is_some() {
        toggle_range(animation)
    } else {
        (animation.first_frame(), animation.last_frame())
    })
}

/// The clip of an icon drawn in `drawn`: its own rect plus the overshoot, but never beyond the
/// clip of the panel it is in (a union would let an icon scrolled out of its panel paint over the
/// panels around it).
fn clip_for(panel: Rect, drawn: Rect, side: f32) -> Rect {
    panel.intersect(drawn.expand(side * OVERSHOOT))
}

/// Paints icon `name` at the animation's `frame` into `rect` (square), tinted `tint`. Falls back to the static
/// icon when there is no animation.
pub fn paint_frame(ui: &Ui, name: &str, rect: Rect, tint: Color32, frame: f32) {
    paint_frame_mirrored(ui, name, rect, tint, frame, false);
}

/// [`paint_frame`], optionally flipped left to right (a redo arrow from the undo one).
pub fn paint_frame_mirrored(
    ui: &Ui,
    name: &str,
    rect: Rect,
    tint: Color32,
    frame: f32,
    mirror: bool,
) {
    let Some(animation) = library().get(name) else {
        rest(ui, name, rect, tint);
        return;
    };
    #[cfg(test)]
    ui.ctx().data_mut(|data| {
        let key = Id::new("lottie-evaluations");
        let count = data.get_temp::<usize>(key).unwrap_or(0);
        data.insert_temp(key, count + 1);
        data.insert_temp(key.with("frame"), frame);
    });
    let side = rect.width().min(rect.height());
    let drawn = Rect::from_center_size(rect.center(), egui::Vec2::splat(side));
    let (width, height) = animation.size();
    let scale = side
        / width
            .max(f32::EPSILON)
            .max(height.max(f32::EPSILON))
            .max(1.0);
    // A pop or a spring may swing past the icon's own rect: let it draw a little outside it.
    let mut painter = ui.painter().clone();
    painter.set_clip_rect(clip_for(painter.clip_rect(), drawn, side));
    let to_screen = |point: [f32; 2]| {
        let x = if mirror {
            drawn.right() - point[0] * scale
        } else {
            drawn.left() + point[0] * scale
        };
        Pos2::new(x, drawn.top() + point[1] * scale)
    };
    for item in animation.frame_at(frame).items {
        match item {
            Item::Stroke(stroke) => {
                let colour = tint.gamma_multiply(stroke.opacity.clamp(0.0, 1.0));
                let line = Stroke::new((stroke.width * scale).max(0.5), colour);
                for polyline in &stroke.polylines {
                    stroke_polyline(
                        &painter,
                        polyline,
                        line,
                        stroke.cap == Cap::Round,
                        stroke.join == Join::Round,
                        &to_screen,
                    );
                }
            }
            Item::Fill(fill) => {
                let colour = tint.gamma_multiply(fill.opacity.clamp(0.0, 1.0));
                for polyline in &fill.polylines {
                    let points: Vec<Pos2> = polyline
                        .points
                        .iter()
                        .map(|point| to_screen(*point))
                        .collect();
                    if points.len() >= 3 {
                        painter.add(fill_shape(points, colour));
                    }
                }
            }
        }
    }
}

fn fill_shape(mut points: Vec<Pos2>, colour: Color32) -> Shape {
    if points.first() == points.last() {
        points.pop();
    }
    let mut mesh = egui::Mesh::default();
    let mut remaining: Vec<_> = points.iter().zip(0_u32..).map(|(_, index)| index).collect();
    let area: f32 = points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .take(points.len())
        .map(|(first, second)| first.x * second.y - second.x * first.y)
        .sum();
    let winding = area.signum();
    mesh.vertices.extend(
        points
            .into_iter()
            .map(|point| egui::epaint::Vertex::untextured(point, colour)),
    );
    // Ear clipping consumes one vertex each iteration; output stays linear in the input size.
    while remaining.len() > 2 {
        let ear = remaining.iter().enumerate().find_map(|(index, &middle)| {
            let &before = index
                .checked_sub(1)
                .and_then(|index| remaining.get(index))
                .or_else(|| remaining.last())?;
            let &after = remaining
                .get(index.saturating_add(1))
                .or_else(|| remaining.first())?;
            let point = |index| {
                mesh.vertices
                    .get(usize::try_from(index).ok()?)
                    .map(|vertex| vertex.pos)
            };
            let (first, second, third) = (point(before)?, point(middle)?, point(after)?);
            let cross = triangle_cross(first, second, third) * winding;
            if cross < 0.0 {
                return None;
            }
            let occupied = cross > 0.0
                && remaining
                    .iter()
                    .filter(|&&other| ![before, middle, after].contains(&other))
                    .any(|&other| {
                        point(other).is_some_and(|point| {
                            triangle_cross(first, second, point) * winding >= 0.0
                                && triangle_cross(second, third, point) * winding >= 0.0
                                && triangle_cross(third, first, point) * winding >= 0.0
                        })
                    });
            (!occupied).then_some((index, [before, middle, after], cross > 0.0))
        });
        let Some((index, triangle, has_area)) = ear else {
            return Shape::Noop;
        };
        if has_area {
            let [first, second, third] = triangle;
            mesh.add_triangle(first, second, third);
        }
        remaining.remove(index);
    }
    Shape::mesh(mesh)
}

fn triangle_cross(first: Pos2, second: Pos2, third: Pos2) -> f32 {
    (second.x - first.x) * (third.y - first.y) - (second.y - first.y) * (third.x - first.x)
}

fn stroke_polyline(
    painter: &egui::Painter,
    polyline: &Polyline,
    stroke: Stroke,
    round_cap: bool,
    round_join: bool,
    to_screen: &dyn Fn([f32; 2]) -> Pos2,
) {
    let points: Vec<Pos2> = polyline
        .points
        .iter()
        .map(|point| to_screen(*point))
        .collect();
    if points.len() < 2 {
        return;
    }
    if round_join {
        round_stroke(painter, points, stroke, polyline.closed, round_cap);
        return;
    }
    if polyline.closed {
        painter.add(Shape::closed_line(points, stroke));
        return;
    }
    if round_cap {
        for end in [points.first(), points.last()].into_iter().flatten() {
            painter.circle_filled(*end, stroke.width / 2.0, stroke.color);
        }
    }
    painter.add(Shape::line(points, stroke));
}

fn round_stroke(
    painter: &egui::Painter,
    mut points: Vec<Pos2>,
    stroke: Stroke,
    closed: bool,
    round_cap: bool,
) {
    // Small angles are the flattened samples of a smooth cubic. Keep their antialiasing continuous.
    const CORNER_COSINE: f32 = 0.8;
    let is_corner = |first: Pos2, middle: Pos2, last: Pos2| {
        (middle - first)
            .normalized()
            .dot((last - middle).normalized())
            < CORNER_COSINE
    };
    points.dedup();
    if closed && points.first() == points.last() {
        points.pop();
    }
    if points.len() < 2 {
        return;
    }
    if closed {
        let pivot = points
            .iter()
            .cycle()
            .skip(points.len().saturating_sub(1))
            .zip(&points)
            .zip(points.iter().cycle().skip(1))
            .position(|((&first, &middle), &last)| is_corner(first, middle, last));
        let Some(pivot) = pivot else {
            painter.add(Shape::closed_line(points, stroke));
            return;
        };
        points.rotate_left(pivot);
        if let Some(first) = points.first().copied() {
            points.push(first);
            painter.circle_filled(first, stroke.width / 2.0, stroke.color);
        }
    } else if round_cap {
        for point in [points.first(), points.last()].into_iter().flatten() {
            painter.circle_filled(*point, stroke.width / 2.0, stroke.color);
        }
    }
    let mut span: Vec<_> = points.first().copied().into_iter().collect();
    for triple in points.windows(3) {
        let [first, middle, last] = triple else {
            continue;
        };
        span.push(*middle);
        if is_corner(*first, *middle, *last) {
            painter.add(Shape::line(std::mem::take(&mut span), stroke));
            painter.circle_filled(*middle, stroke.width / 2.0, stroke.color);
            span.push(*middle);
        }
    }
    if let Some(last) = points.last() {
        span.push(*last);
    }
    painter.add(Shape::line(span, stroke));
}

/// The frames a toggle runs between.
fn toggle_range(animation: &Animation) -> (f32, f32) {
    let off = animation
        .marker(OFF)
        .map_or_else(|| animation.first_frame(), |marker| marker.frame);
    let on = animation
        .marker(ON)
        .map_or_else(|| animation.last_frame(), |marker| marker.frame);
    (off, on)
}

/// Paints a toggle icon that plays forward when `on` turns true and backward when it turns false, so opening and
/// closing both animate. Without an animation it draws the static icon.
pub fn toggle(
    ui: &Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    name: &str,
    rect: Rect,
    tint: Color32,
    on: bool,
) {
    let Some(animation) = library().get(name) else {
        rest(ui, name, rect, tint);
        return;
    };
    let (off, end) = toggle_range(animation);
    let seconds = ((end - off).abs() / animation.frame_rate().max(1.0)).max(MIN_DURATION);
    let progress =
        crate::ui::motion::progress(ui.ctx(), (id, "lottie-toggle"), on, seconds, linear);
    paint_frame(ui, name, rect, tint, off + (end - off) * progress);
}

fn linear(t: f32) -> f32 {
    t
}

/// Paints a one-shot icon: it runs once each time `trigger` turns true (for example on hover) and rests on its
/// first frame otherwise.
pub fn play(
    ui: &Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    name: &str,
    rect: Rect,
    tint: Color32,
    trigger: bool,
) {
    let Some(animation) = library().get(name) else {
        rest(ui, name, rect, tint);
        return;
    };
    let ctx = ui.ctx();
    let key = Id::new((id, "lottie-play"));
    let now = ctx.input(|input| input.time);
    let frame_number = ctx.cumulative_frame_nr();
    let mut playback = ctx
        .data(|data| data.get_temp::<Playback>(key))
        .unwrap_or_default();
    if playback.last_seen.saturating_add(1) < frame_number {
        playback = Playback::default();
    }
    if trigger && !playback.trigger {
        playback.started = Some(now);
    }
    playback.trigger = trigger;
    playback.last_seen = frame_number;
    let elapsed = playback.started.map_or(f64::INFINITY, |start| now - start);
    let running = crate::ui::motion::enabled(ctx)
        && (0.0..f64::from(animation.duration_secs())).contains(&elapsed);
    if !running {
        playback.started = None;
    }
    if playback.trigger || playback.started.is_some() {
        ctx.data_mut(|data| data.insert_temp(key, playback));
    } else {
        ctx.data_mut(|data| data.remove::<Playback>(key));
    }
    if !running {
        rest(ui, name, rect, tint);
        return;
    }
    ctx.request_repaint();
    let offset = (elapsed as f32) * animation.frame_rate();
    paint_frame(ui, name, rect, tint, animation.first_frame() + offset);
}

#[derive(Clone, Default)]
struct Playback {
    trigger: bool,
    started: Option<f64>,
    last_seen: u64,
}

/// Paints inside an existing icon rect, playing once on whole-control hover entry.
/// Icons without an animation and icons at rest use the static Lucide drawing.
pub fn paint_hover(
    ui: &Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    name: &str,
    rect: Rect,
    tint: Color32,
    hovered: bool,
) {
    play(ui, id, name, rect, tint, hovered);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_shipped_file_is_listed_and_parses() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/icons-anim");
        let mut on_disk: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .filter_map(|e| e.ok()?.file_name().to_str().map(str::to_owned))
            .filter(|n| n.ends_with(".json") && n != "index.json")
            .map(|n| n.trim_end_matches(".json").to_owned())
            .collect();
        on_disk.sort();
        let listed: Vec<&str> = files::FILES.iter().map(|(n, _)| *n).collect();
        assert_eq!(on_disk, listed);
        for (name, text) in files::FILES {
            assert!(Animation::parse(text).is_ok(), "{name} parses");
            assert!(has(name), "{name} in the library");
        }
    }

    #[test]
    fn toggles_run_between_their_markers() {
        let (off, on) = range("lock").unwrap();
        assert!(on > off);
        assert!(range("nope").is_none());
    }

    #[test]
    fn animated_icons_without_a_hand_drawn_twin_still_resolve() {
        assert!(static_icon("chevron-right").is_some());
        assert!(static_icon("sparkles").is_none() && has("sparkles"));
    }
}

/// The first (resting) frame of `name` flipped left to right; nothing when it has no animation.
pub fn paint_rest_mirrored(ui: &Ui, name: &str, rect: Rect, tint: Color32) {
    if let Some(animation) = library().get(name) {
        let (off, _) = if animation.marker(OFF).is_some() {
            toggle_range(animation)
        } else {
            (animation.first_frame(), animation.last_frame())
        };
        paint_frame_mirrored(ui, name, rect, tint, off, true);
    }
}
