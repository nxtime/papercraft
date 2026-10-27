//! Annotations on the canvas: note cards in screen space, elbow leader lines to their targets,
//! drag to move, click to select, and the view states (global mode, per-category, per-note).
//! Drawn as an egui overlay only, so exports and Present mode never include it.

use crate::theme::Surface as _;
use crate::editor::Editor;
use crate::editor::shortcuts;
use crate::geom::V2;
use crate::model::annotation::{
    Annotation, AnnotationCategory, CARD_W, Display, Side, anchor_world, card_world, numbers,
};
use crate::model::annotation_layout::{self as layout, BADGE, BODY_LH, BODY_LINES, HEAD_GAP, LABEL_LINES, LABEL_W};
use crate::theme::{self, Icon};
use egui::{Align2, Color32, Galley, Pos2, Rect, Sense, Stroke, vec2};
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::Arc;

/// How much of each note the canvas shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Full,
    Badges,
    Hidden,
}

impl Mode {
    pub fn next(self) -> Mode {
        match self {
            Mode::Full => Mode::Badges,
            Mode::Badges => Mode::Hidden,
            Mode::Hidden => Mode::Full,
        }
    }
}

impl From<Display> for Mode {
    fn from(d: Display) -> Mode {
        match d {
            Display::Full => Mode::Full,
            Display::Badges => Mode::Badges,
        }
    }
}

/// (annotation, text hash, scale bucket, kind): 0 label, 1 label in full, 2 body in full,
/// 3 body capped at [`BODY_LINES`] rows.
type GalleyKey = (u64, u64, u32, u8);
type RouteKey = (u64, u64);

/// View state (never saved): global mode, hidden categories, collapsed notes, the drag in
/// progress and the text layout cache.
#[derive(Default)]
pub struct AnnotationsView {
    /// `None` follows the page template's display.
    pub mode: Option<Mode>,
    pub hidden_categories: HashSet<String>,
    pub collapsed: HashSet<u64>,
    /// Notes showing every body line instead of "+N more lines".
    pub expanded: HashSet<u64>,
    drag: Option<(u64, V2)>,
    galleys: HashMap<GalleyKey, Arc<Galley>>,
    nums: Option<(u64, Arc<Numbers>)>,
    /// Leader routes in world space, keyed by annotation; valid while the key (doc revision,
    /// card rects around it) is unchanged.
    routes: HashMap<u64, (RouteKey, Vec<V2>)>,
}

pub type Numbers = HashMap<u64, (Option<crate::model::NodeId>, u32)>;

impl AnnotationsView {
    /// Note numbers of the active page, recomputed only when the document changed.
    pub fn numbers(&mut self, ed: &Editor) -> Arc<Numbers> {
        if let Some((stamp, n)) = &self.nums
            && *stamp == ed.doc_stamp
        {
            return n.clone();
        }
        let n = Arc::new(numbers(&ed.doc));
        self.nums = Some((ed.doc_stamp, n.clone()));
        n
    }

    pub fn effective(&self, template_display: Display) -> Mode {
        self.mode.unwrap_or_else(|| template_display.into())
    }

    /// Full -> Badges -> Hidden -> Full.
    pub fn cycle(&mut self, template_display: Display) {
        self.mode = Some(self.effective(template_display).next());
    }
}

const SCALE_MIN: f64 = 0.6;
const SCALE_MAX: f64 = 1.5;
const SCALE_STEP: f64 = 0.1;
const GALLEY_CACHE_MAX: usize = 512;
const PAD: f32 = 12.0;
const BADGE_R: f32 = BADGE as f32 / 2.0;
const BADGE_ONLY: f32 = 22.0;
const LABEL_PT: f32 = 13.0;
const BODY_PT: f32 = 11.0;
const ICON_PX: f32 = 14.0;
const RADIUS: f32 = 10.0;
const LINE_W: f32 = 1.5;
const DOT_R: f32 = 3.5;
const LINE_HIT: f32 = 5.0;
const OUTLINE_W: f32 = 2.0;
const OUTLINE_R: f32 = 4.0;
const PILL_LIFT: f32 = 72.0;

/// Card scale for a zoom: follows the zoom but stays readable, in coarse steps so the text
/// layout cache is not thrashed while zooming.
fn scale_bucket(zoom: f64) -> u32 {
    (zoom.clamp(SCALE_MIN, SCALE_MAX) / SCALE_STEP).round() as u32
}

fn bucket_scale(b: u32) -> f32 {
    (f64::from(b) * SCALE_STEP) as f32
}

fn near_line(pts: &[Pos2], p: Pos2) -> bool {
    pts.windows(2).any(|w| {
        let (a, b) = (w[0], w[1]);
        let ab = b - a;
        let len2 = ab.length_sq();
        let t = if len2 > 0.0 { ((p - a).dot(ab) / len2).clamp(0.0, 1.0) } else { 0.0 };
        (a + ab * t).distance(p) <= LINE_HIT
    })
}

fn c32(c: crate::model::Color) -> Color32 {
    super::c32(c)
}

struct Item<'a> {
    a: &'a Annotation,
    n: u32,
    color: Color32,
    icon: Icon,
    rect: Rect,
    badge_only: bool,
    route: Vec<Pos2>,
    target: Option<Rect>,
    label: Option<Arc<Galley>>,
    body: Option<Arc<Galley>>,
    /// Body rows cut off by the cap (shown as "+N more lines").
    more: usize,
    /// Head row height (screen): the taller of badge and label.
    head_h: f32,
}

fn text_hash(a: &Annotation) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    a.label.hash(&mut h);
    a.body.hash(&mut h);
    h.finish()
}

fn galley(
    ctx: &egui::Context,
    cache: &mut HashMap<GalleyKey, Arc<Galley>>,
    key: GalleyKey,
    text: &str,
    font: egui::FontId,
    wrap: f32,
    rows: usize,
) -> Arc<Galley> {
    if cache.len() > GALLEY_CACHE_MAX {
        cache.clear();
    }
    cache
        .entry(key)
        .or_insert_with(|| {
            let mut job = egui::text::LayoutJob::simple(text.to_owned(), font, Color32::WHITE, wrap);
            job.wrap.max_rows = rows;
            job.wrap.break_anywhere = false;
            job.wrap.overflow_character = (key.3 == 0).then_some('…');
            ctx.fonts_mut(|f| f.layout_job(job))
        })
        .clone()
}

fn to_pos(canvas: Rect, cam: &crate::geom::Camera, w: V2) -> Pos2 {
    let v = cam.world_to_screen(w);
    canvas.min + vec2(v.x as f32, v.y as f32)
}

fn world_rect(canvas: Rect, cam: &crate::geom::Camera, r: Rect) -> crate::geom::Rect {
    let min = cam.screen_to_world(V2 { x: f64::from(r.min.x - canvas.min.x), y: f64::from(r.min.y - canvas.min.y) });
    crate::geom::Rect::new(min.x, min.y, f64::from(r.width()) / cam.zoom, f64::from(r.height()) / cam.zoom)
}

/// Draw the overlay and the bottom pill; apply a finished drag as one undo step.
pub fn draw(
    ctx: &egui::Context,
    ed: &mut Editor,
    view: &mut AnnotationsView,
    selected: &mut Option<u64>,
    canvas: egui::Rect,
) {
    if ed.doc.annotations.is_empty() {
        return;
    }
    let mode = view.effective(ed.active_template(ed.active_page).display);
    let mut commit: Option<(u64, V2)> = None;
    if mode != Mode::Hidden {
        commit = cards(ctx, ed, view, selected, canvas, mode);
    }
    pill(ctx, ed, view, canvas, mode);
    if let Some((id, off)) = commit {
        let doc = ed.doc.clone();
        ed.update_annotation(id, |a| {
            a.position = a.position + off;
            let (Some(t), Some(at)) = (a.target.and_then(|t| doc.world_rect(t)), Some(card_world(&doc, a) + off)) else { return };
            let s = layout::card_size(a);
            a.anchor = layout::anchor_toward(t, crate::geom::Rect::new(at.x, at.y, s.x, s.y));
        });
    }
}

fn cards(
    ctx: &egui::Context,
    ed: &Editor,
    view: &mut AnnotationsView,
    selected: &mut Option<u64>,
    canvas: Rect,
    mode: Mode,
) -> Option<(u64, V2)> {
    let cam = ed.camera;
    let bucket = scale_bucket(cam.zoom);
    let s = bucket_scale(bucket);
    let cats: Vec<AnnotationCategory> = ed.design.categories();
    let nums = view.numbers(ed);
    let to_screen = |w: V2| to_pos(canvas, &cam, w);
    let drag = view.drag;
    let mut items: Vec<Item> = Vec::new();
    for a in &ed.doc.annotations {
        if view.hidden_categories.contains(&a.category) {
            continue;
        }
        let cat = cats.iter().find(|c| c.id == a.category).or(cats.first());
        let (color, icon) = cat.map_or((Color32::GRAY, Icon::Sticky), |c| {
            (c32(c.color), theme::icon_by_name(&c.icon).unwrap_or(Icon::Sticky))
        });
        let mut at = card_world(&ed.doc, a);
        if let Some((_, off)) = drag.filter(|(id, _)| *id == a.id) {
            at = at + off;
        }
        // Below SCALE_MIN a full card would outgrow the footprint placement kept free for it.
        let badge_only = mode == Mode::Badges || view.collapsed.contains(&a.id) || cam.zoom < SCALE_MIN;
        let (mut label, mut body, mut more, mut head_h) = (None, None, 0, BADGE as f32 * s);
        let size = if badge_only {
            vec2(BADGE_ONLY * s, BADGE_ONLY * s)
        } else {
            let th = text_hash(a);
            let expanded = view.expanded.contains(&a.id);
            let lw = LABEL_W as f32 * s;
            let (lk, lrows) = if expanded { (1, usize::MAX) } else { (0, LABEL_LINES) };
            let lg = galley(ctx, &mut view.galleys, (a.id, th, bucket, lk), &a.label, theme::semibold(LABEL_PT * s), lw, lrows);
            head_h = head_h.max(lg.size().y);
            let mut h = (layout::PAD * 2.0) as f32 * s + head_h;
            if !a.body.is_empty() {
                let bw = (CARD_W - 2.0 * layout::PAD) as f32 * s;
                let font = egui::FontId::proportional(BODY_PT * s);
                let full = galley(ctx, &mut view.galleys, (a.id, th, bucket, 2), &a.body, font.clone(), bw, usize::MAX);
                let bg = if expanded || full.rows.len() <= BODY_LINES {
                    full
                } else {
                    more = full.rows.len() - BODY_LINES;
                    galley(ctx, &mut view.galleys, (a.id, th, bucket, 3), &a.body, font, bw, BODY_LINES)
                };
                h += HEAD_GAP as f32 * s + bg.size().y + if more > 0 { BODY_LH as f32 * s } else { 0.0 };
                body = Some(bg);
            }
            label = Some(lg);
            vec2(CARD_W as f32 * s, h)
        };
        let rect = Rect::from_min_size(to_screen(at), size);
        let lineless = a.detached || (a.target.is_none() && a.point.is_none());
        let target = a.target.and_then(|t| ed.doc.world_rect(t)).map(|r| {
            let r = cam.world_rect_to_screen(&r);
            Rect::from_min_size(
                canvas.min + vec2(r.x as f32, r.y as f32),
                vec2(r.w as f32, r.h as f32),
            )
        });
        let bounds = target.map_or(rect, |t| rect.union(t));
        if !canvas.intersects(bounds) {
            continue;
        }
        items.push(Item {
            a,
            n: nums.get(&a.id).map_or(0, |n| n.1),
            color,
            icon,
            rect,
            badge_only,
            route: if lineless { Vec::new() } else { vec![Pos2::ZERO] },
            target,
            label,
            body,
            more,
            head_h,
        });
    }
    route_items(ed, view, &mut items, canvas);

    let mut commit = None;
    let pointer = ctx.pointer_hover_pos();
    egui::Area::new("annotations-overlay".into())
        .order(egui::Order::Middle)
        .fade_in(false)
        .fixed_pos(canvas.min)
        .show(ctx, |ui| {
            ui.set_clip_rect(canvas);
            let p = theme::pal(ui);
            let painter = ui.painter().clone();
            for it in &items {
                if it.route.len() < 2 {
                    continue;
                }
                painter.add(egui::Shape::line(it.route.clone(), Stroke::new(LINE_W, it.color)));
                painter.circle_filled(*it.route.last().unwrap(), DOT_R, it.color);
            }
            for it in &items {
                let a = it.a;
                let resp = ui.interact(it.rect, egui::Id::new(("annotation-card", a.id)), Sense::click_and_drag());
                let badge_rect = Rect::from_center_size(
                    if it.badge_only {
                        it.rect.center()
                    } else {
                        it.rect.min + vec2(PAD + BADGE_R, PAD + BADGE_R) * s
                    },
                    vec2(BADGE_R * 2.0, BADGE_R * 2.0) * s,
                );
                let badge = ui.interact(badge_rect, egui::Id::new(("annotation-badge", a.id)), Sense::click());
                let on_line = pointer.is_some_and(|ptr| near_line(&it.route, ptr));
                let is_sel = *selected == Some(a.id);
                if (resp.hovered() || on_line || is_sel)
                    && let Some(t) = it.target
                {
                    painter.rect_stroke(t, OUTLINE_R, Stroke::new(OUTLINE_W, it.color), egui::StrokeKind::Outside);
                }
                if resp.hovered() || on_line {
                    ctx.set_cursor_icon(if resp.dragged() { egui::CursorIcon::Grabbing } else { egui::CursorIcon::PointingHand });
                }
                if let Some(lg) = &it.label {
                    painter.rect_filled(it.rect, RADIUS * s, p.popover);
                    let stroke = if is_sel {
                        Stroke::new(LINE_W, it.color)
                    } else {
                        Stroke::new(1.0, p.separator)
                    };
                    painter.rect_stroke(it.rect, RADIUS * s, stroke, egui::StrokeKind::Inside);
                    let text_x = badge_rect.right() + 8.0 * s;
                    let icon_rect = Rect::from_center_size(
                        Pos2::new(it.rect.right() - (PAD + ICON_PX / 2.0) * s, badge_rect.center().y),
                        vec2(ICON_PX, ICON_PX) * s,
                    );
                    let head_top = it.rect.top() + PAD * s;
                    painter.galley_with_override_text_color(
                        Pos2::new(text_x, head_top + (it.head_h - lg.size().y) / 2.0), lg.clone(), p.text);
                    theme::paint_icon(&painter, icon_rect, it.icon, it.color);
                    let body_top = head_top + it.head_h + HEAD_GAP as f32 * s;
                    if let Some(bg) = &it.body {
                        painter.galley_with_override_text_color(
                            Pos2::new(it.rect.left() + PAD * s, body_top), bg.clone(), p.secondary);
                        if it.more > 0 {
                            let more_rect = Rect::from_min_size(
                                Pos2::new(it.rect.left() + PAD * s, body_top + bg.size().y),
                                vec2(it.rect.width() - PAD * 2.0 * s, BODY_LH as f32 * s),
                            );
                            let r = ui.interact(more_rect, egui::Id::new(("annotation-more", a.id)), Sense::click());
                            let text = format!("+{} more lines", it.more);
                            painter.text(more_rect.left_center(), Align2::LEFT_CENTER, text,
                                theme::semibold(BODY_PT * s), if r.hovered() { p.text } else { it.color });
                            if r.hovered() {
                                ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
                            }
                            if r.clicked() {
                                *selected = Some(a.id);
                                view.expanded.insert(a.id);
                            }
                        }
                    }
                }
                let detached = a.detached || (a.target.is_none() && a.point.is_none());
                let fill = if detached { p.warning } else { it.color };
                painter.circle_filled(badge_rect.center(), BADGE_R * s, fill);
                let mark = if detached { "!".to_owned() } else { it.n.to_string() };
                painter.text(
                    badge_rect.center(),
                    Align2::CENTER_CENTER,
                    mark,
                    theme::semibold(BODY_PT * s),
                    super::kit::ink_on(fill),
                );
                if badge.clicked() {
                    *selected = Some(a.id);
                    if mode == Mode::Full && !view.collapsed.remove(&a.id) {
                        view.collapsed.insert(a.id);
                    }
                } else if resp.clicked() || resp.drag_started() {
                    *selected = Some(a.id);
                }
                if resp.dragged() {
                    let prev = view.drag.filter(|(id, _)| *id == a.id).map_or(V2::ZERO, |d| d.1);
                    let d = resp.drag_delta();
                    view.drag = Some((a.id, prev + V2 { x: f64::from(d.x), y: f64::from(d.y) } / cam.zoom));
                }
                if resp.drag_stopped()
                    && let Some((id, off)) = view.drag.take()
                    && off != V2::ZERO
                {
                    commit = Some((id, off));
                }
            }
        });
    commit
}

/// Fill each item's screen-space leader route. Routes go around the page's nodes and the other
/// cards, are computed in world space from each card's layout size (so they do not change with
/// zoom) and cached until the document or a nearby card changes. The drawn card or badge joins
/// the route with an elbow from its centre, hidden under a full card.
fn route_items(ed: &Editor, view: &mut AnnotationsView, items: &mut [Item], canvas: Rect) {
    let cam = ed.camera;
    let cards: Vec<(u64, crate::geom::Rect)> = items
        .iter()
        .map(|it| {
            let at = world_rect(canvas, &cam, it.rect).min();
            let s = layout::card_size(it.a);
            (it.a.id, crate::geom::Rect::new(at.x, at.y, s.x, s.y))
        })
        .collect();
    view.routes.retain(|id, _| items.iter().any(|it| it.a.id == *id));
    for (i, it) in items.iter_mut().enumerate() {
        if it.route.is_empty() {
            continue;
        }
        let a = it.a;
        let card = cards[i].1;
        let Some(trect) = a.target.and_then(|t| ed.doc.world_rect(t)) else {
            let end = anchor_world(&ed.doc, a);
            it.route = layout::elbow(card, end).into_iter().map(|w| to_pos(canvas, &cam, w)).collect();
            continue;
        };
        let end = if a.anchor.side == Side::Center {
            layout::anchor_point(trect, layout::anchor_toward(trect, card))
        } else {
            anchor_world(&ed.doc, a)
        };
        let win = card.union(&trect).inflate(300.0);
        let mut h = std::collections::hash_map::DefaultHasher::new();
        (card.x.to_bits(), card.y.to_bits(), card.w.to_bits(), card.h.to_bits(), end.x.to_bits(), end.y.to_bits()).hash(&mut h);
        let others: Vec<crate::geom::Rect> = cards
            .iter()
            .enumerate()
            .filter(|(j, (_, r))| *j != i && r.intersects(&win))
            .map(|(_, (_, r))| *r)
            .collect();
        for r in &others {
            (r.x.to_bits(), r.y.to_bits(), r.w.to_bits(), r.h.to_bits()).hash(&mut h);
        }
        let key = (ed.doc_stamp, h.finish());
        let world = match view.routes.get(&a.id) {
            Some((k, r)) if *k == key => r.clone(),
            _ => {
                let mut obs = layout::route_obstacles(&ed.doc, a.target.unwrap());
                obs.extend(others);
                let r = layout::route(card, trect, end, &obs);
                view.routes.insert(a.id, (key, r.clone()));
                r
            }
        };
        let route: Vec<Pos2> = world.into_iter().map(|w| to_pos(canvas, &cam, w)).collect();
        let c = it.rect.center();
        it.route = [c, Pos2::new(route[0].x, c.y)].into_iter().chain(route).collect();
    }
}

fn pill(ctx: &egui::Context, ed: &Editor, view: &mut AnnotationsView, canvas: Rect, mode: Mode) {
    let chord = shortcuts::chord_text(shortcuts::Action::ToggleAnnotations);
    let text = if mode == Mode::Hidden {
        format!("Annotations off · {chord}")
    } else {
        format!("Annotations on · {} · {chord}", ed.doc.annotations.len())
    };
    let display = ed.active_template(ed.active_page).display;
    egui::Area::new("annotations-pill".into())
        .order(egui::Order::Middle)
        .fade_in(false)
        .pivot(Align2::CENTER_BOTTOM)
        .fixed_pos(Pos2::new(canvas.center().x, canvas.bottom() - PILL_LIFT))
        .show(ctx, |ui| {
            let p = theme::pal(ui);
            let r = egui::Frame::new()
                .fill(p.popover)
                .corner_radius(egui::CornerRadius::same(255))
                .inner_margin(egui::Margin::symmetric(14, 6))
                .show_surface(ui, |ui| {
                    ui.horizontal(|ui| {
                        let (ir, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
                        theme::paint_icon(ui.painter(), ir, Icon::Page, p.secondary);
                        ui.add(egui::Label::new(
                            egui::RichText::new(text).font(theme::semibold(theme::SECONDARY)).color(p.text),
                        ).selectable(false));
                    });
                });
            if ui.interact(r.response.rect, egui::Id::new("annotations-pill-click"), Sense::click()).clicked() {
                view.cycle(display);
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cycle_full_badges_hidden() {
        let mut v = AnnotationsView::default();
        assert_eq!(v.effective(Display::Full), Mode::Full);
        v.cycle(Display::Full);
        assert_eq!(v.effective(Display::Full), Mode::Badges);
        v.cycle(Display::Full);
        assert_eq!(v.effective(Display::Full), Mode::Hidden);
        v.cycle(Display::Full);
        assert_eq!(v.effective(Display::Full), Mode::Full);
        // Template default Badges starts there.
        let mut v = AnnotationsView::default();
        assert_eq!(v.effective(Display::Badges), Mode::Badges);
        v.cycle(Display::Badges);
        assert_eq!(v.effective(Display::Badges), Mode::Hidden);
    }

    #[test]
    fn scale_is_bucketed_and_clamped() {
        assert_eq!(scale_bucket(0.01), scale_bucket(0.6));
        assert_eq!(scale_bucket(50.0), scale_bucket(1.5));
        assert_eq!(scale_bucket(1.0), 10);
    }

    #[test]
    fn line_hit_test() {
        let pts = [Pos2::new(0.0, 0.0), Pos2::new(100.0, 0.0)];
        assert!(near_line(&pts, Pos2::new(50.0, 3.0)));
        assert!(!near_line(&pts, Pos2::new(50.0, 20.0)));
    }
}
