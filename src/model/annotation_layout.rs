//! Note card sizing, placement and leader-line routing. Pure, in world units (card scale 1).
//!
//! Cards grow to their text (label up to 2 lines, body up to 8, then "+N more lines"); placement
//! picks the free slot nearest the target that overlaps no node and no other card; leader lines
//! are routed orthogonally around obstacle rects with a small grid A*.

use super::annotation::{Anchor, Annotation, CARD_W, Side, card_world, frame_of};
use super::document::Document;
use super::node::NodeId;
use crate::geom::{Rect, V2, v2};
use std::cmp::Reverse;
use std::collections::BinaryHeap;

pub const PAD: f64 = 12.0;
pub const BADGE: f64 = 18.0;
pub const ICON: f64 = 14.0;
pub const LABEL_PT: f64 = 13.0;
pub const BODY_PT: f64 = 11.0;
pub const LABEL_LH: f64 = 16.0;
pub const BODY_LH: f64 = 14.0;
pub const LABEL_LINES: usize = 2;
pub const BODY_LINES: usize = 8;
pub const HEAD_GAP: f64 = 6.0;
/// Free space kept between a card and any node or other card.
pub const MARGIN: f64 = 12.0;
const ROUTE_PAD: f64 = 6.0;
const ROUTE_WINDOW: f64 = 300.0;
const ROUTE_MAX_OBSTACLES: usize = 160;
const BEND_COST: f64 = 24.0;
const OUTSIDE_FRAME_COST: f64 = 120.0;
const STRADDLE_COST: f64 = 1200.0;
const CROSS_COST: f64 = 600.0;
const LINE_COST: f64 = 0.6;
const BEND_PENALTY: f64 = 40.0;
const FEASIBLE_TRIED: usize = 24;
const RINGS: [(f64, f64); 3] = [(360.0, 12.0), (1200.0, 40.0), (4000.0, 120.0)];

/// Width available to the label and to the body, in world units.
pub const LABEL_W: f64 = CARD_W - 2.0 * PAD - BADGE - 8.0 - ICON - 6.0;
pub const BODY_W: f64 = CARD_W - 2.0 * PAD;

fn char_w(c: char, pt: f64) -> f64 {
    pt * match c {
        ' ' => 0.28,
        'i' | 'l' | 'j' | 't' | 'f' | 'I' | '.' | ',' | ':' | ';' | '\'' | '!' | '|' | '(' | ')' | '[' | ']' => 0.32,
        'm' | 'w' | 'M' | 'W' | '@' => 0.85,
        c if c.is_uppercase() => 0.64,
        _ => 0.52,
    }
}

/// Estimated wrapped line count of `text` at `width` (word wrap, long words break, `\n` splits).
pub fn wrap_lines(text: &str, width: f64, pt: f64) -> usize {
    text.split('\n')
        .map(|para| {
            let (mut lines, mut x) = (1usize, 0.0);
            for word in para.split_whitespace() {
                let w: f64 = word.chars().map(|c| char_w(c, pt)).sum();
                let space = char_w(' ', pt);
                if x > 0.0 && x + space + w <= width {
                    x += space + w;
                    continue;
                }
                if x > 0.0 {
                    lines += 1;
                }
                let extra = (w / width).ceil().max(1.0) - 1.0;
                lines += extra as usize;
                x = if extra > 0.0 { w - extra * width } else { w };
            }
            lines
        })
        .sum()
}

/// What a card shows: label rows, body rows, and body rows cut off (`+N more lines`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lines {
    pub label: usize,
    pub body: usize,
    pub more: usize,
}

pub fn card_lines(label: &str, body: &str, expanded: bool) -> Lines {
    let label_n = wrap_lines(label, LABEL_W, LABEL_PT);
    let body_n = if body.is_empty() { 0 } else { wrap_lines(body, BODY_W, BODY_PT) };
    if expanded {
        return Lines { label: label_n, body: body_n, more: 0 };
    }
    let shown = body_n.min(BODY_LINES);
    Lines { label: label_n.min(LABEL_LINES), body: shown, more: body_n - shown }
}

/// Card height from its measured row heights (`head` = tallest of badge and label).
pub fn height_of(head: f64, body: f64, has_body: bool) -> f64 {
    PAD + head + if has_body { HEAD_GAP + body } else { 0.0 } + PAD
}

/// Estimated card size in world units.
pub fn card_size(a: &Annotation) -> V2 {
    let l = card_lines(&a.label, &a.body, false);
    let head = BADGE.max(l.label as f64 * LABEL_LH);
    let body = (l.body + usize::from(l.more > 0)) as f64 * BODY_LH;
    v2(CARD_W, height_of(head, body, !a.body.is_empty()))
}

fn overlaps(a: &Rect, b: &Rect) -> bool {
    a.x < b.right() && b.x < a.right() && a.y < b.bottom() && b.y < a.bottom()
}

fn strictly_inside(r: &Rect, p: V2) -> bool {
    p.x > r.x && p.x < r.right() && p.y > r.y && p.y < r.bottom()
}

/// Where a leader line ends on `target`: the edge facing `card`, level with it where they
/// overlap in span so the line is straight.
pub fn anchor_toward(target: Rect, card: Rect) -> Anchor {
    let span = |lo: f64, hi: f64, clo: f64, chi: f64, c: f64| {
        let (a, b) = (lo.max(clo), hi.min(chi));
        if a <= b { (a + b) / 2.0 } else { c.clamp(lo, hi) }
    };
    let c = card.center();
    if card.x >= target.right() {
        let y = span(target.y, target.bottom(), card.y, card.bottom(), c.y);
        Anchor { side: Side::Right, t: (y - target.y) / target.h.max(1.0) }
    } else if card.right() <= target.x {
        let y = span(target.y, target.bottom(), card.y, card.bottom(), c.y);
        Anchor { side: Side::Left, t: (y - target.y) / target.h.max(1.0) }
    } else {
        let x = span(target.x, target.right(), card.x, card.right(), c.x);
        let side = if card.y >= target.bottom() { Side::Bottom } else if card.bottom() <= target.y { Side::Top } else { return Anchor::default() };
        Anchor { side, t: (x - target.x) / target.w.max(1.0) }
    }
}

pub fn anchor_point(target: Rect, a: Anchor) -> V2 {
    let t = a.t.clamp(0.0, 1.0);
    match a.side {
        Side::Top => v2(target.x + t * target.w, target.y),
        Side::Bottom => v2(target.x + t * target.w, target.bottom()),
        Side::Left => v2(target.x, target.y + t * target.h),
        Side::Right => v2(target.right(), target.y + t * target.h),
        Side::Center => target.center(),
    }
}

/// Simple orthogonal leader line from `card` to `end`: straight when level, else one elbow.
pub fn elbow(card: Rect, end: V2) -> Vec<V2> {
    if end.x >= card.x && end.x <= card.right() {
        let y = if end.y < card.y { card.y } else { card.bottom() };
        return vec![v2(end.x, y), end];
    }
    let sx = if end.x > card.right() { card.right() } else { card.x };
    let sy = if end.y >= card.y && end.y <= card.bottom() { end.y } else { card.center().y };
    let s = v2(sx, sy);
    if (s.y - end.y).abs() < 0.5 {
        return vec![s, end];
    }
    let mx = (s.x + end.x) / 2.0;
    vec![s, v2(mx, s.y), v2(mx, end.y), end]
}

fn seg_box(a: V2, b: V2) -> Rect {
    Rect::from_points(a, b)
}

fn crosses_rect(path: &[V2], r: &Rect) -> bool {
    path.windows(2).any(|w| overlaps(&seg_box(w[0], w[1]).inflate(0.5), r))
}

fn paths_cross(a: &[V2], b: &[V2]) -> bool {
    a.windows(2).any(|s| {
        let sb = seg_box(s[0], s[1]);
        b.windows(2).any(|t| {
            let tb = seg_box(t[0], t[1]);
            sb.x <= tb.right() && tb.x <= sb.right() && sb.y <= tb.bottom() && tb.y <= sb.bottom()
        })
    })
}

// ------------------------------------------------------------------------------ obstacles

/// Rects a card or line must keep clear of when annotating `target`: every node on the page
/// except `target` itself and its ancestors (their other children count, as whole nodes).
pub fn obstacles(doc: &Document, target: Option<NodeId>) -> Vec<Rect> {
    fn walk(doc: &Document, ids: &[NodeId], target: Option<NodeId>, out: &mut Vec<Rect>) {
        for &id in ids {
            let Some(n) = doc.get(id) else { continue };
            if !n.visible || Some(id) == target {
                continue;
            }
            if target.is_some_and(|t| doc.is_ancestor(id, t)) {
                walk(doc, &n.children, target, out);
            } else if let Some(r) = doc.world_rect(id) {
                out.push(r);
            }
        }
    }
    let mut out = Vec::new();
    walk(doc, doc.children(None), target, &mut out);
    out
}

/// The screen holding `target`: its outermost ancestor below the top-level frame that does not
/// hug its width (rows and grids of screens hug, a screen has a set width).
pub fn screen_of(doc: &Document, target: NodeId) -> Option<Rect> {
    let anc = doc.ancestors(target);
    anc.iter()
        .rev()
        .skip(1)
        .find(|id| doc.get(**id).is_some_and(|n| n.layout_child.sizing[0] != crate::model::paint::Sizing::Hug))
        .and_then(|s| doc.world_rect(*s))
}

/// Obstacles a leader line routes around: as `obstacles`, minus what lies inside the target's
/// own screen (a line has to enter that screen anyway, so it goes straight to the target).
pub fn route_obstacles(doc: &Document, target: NodeId) -> Vec<Rect> {
    let screen = screen_of(doc, target);
    let mut obs = obstacles(doc, Some(target));
    obs.retain(|o| !screen.is_some_and(|s| s.contains_rect(o)));
    obs
}

/// A note already on the page: its card and its (simple) leader line.
#[derive(Debug, Clone)]
pub struct Placed {
    pub id: u64,
    pub rect: Rect,
    pub line: Vec<V2>,
}

pub fn placed_of(doc: &Document, a: &Annotation) -> Placed {
    let at = card_world(doc, a);
    let s = card_size(a);
    let rect = Rect::new(at.x, at.y, s.x, s.y);
    let line = match a.target.and_then(|t| doc.world_rect(t)) {
        Some(t) if !a.detached => elbow(rect, anchor_point(t, anchor_toward(t, rect))),
        _ => Vec::new(),
    };
    Placed { id: a.id, rect, line }
}

pub fn existing(doc: &Document, skip: impl Fn(u64) -> bool) -> Vec<Placed> {
    doc.annotations.iter().filter(|a| !skip(a.id)).map(|a| placed_of(doc, a)).collect()
}

// ------------------------------------------------------------------------------ placement

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    /// Card top-left relative to the target's top-left.
    pub position: V2,
    pub anchor: Anchor,
}

fn gap_distance(a: &Rect, b: &Rect) -> f64 {
    let dx = (a.x - b.right()).max(b.x - a.right()).max(0.0);
    let dy = (a.y - b.bottom()).max(b.y - a.bottom()).max(0.0);
    dx + dy
}

/// Place `a`'s card (its target must exist) beside the target, clear of all nodes and `placed`.
pub fn place_among(doc: &Document, a: &Annotation, placed: &[Placed]) -> Option<Placement> {
    let target = a.target?;
    let t = doc.world_rect(target)?;
    let size = card_size(a);
    let frame = frame_of(doc, target)
        .filter(|f| *f != target)
        .and_then(|f| doc.world_rect(f));
    let mut obs = obstacles(doc, Some(target));
    obs.push(t);
    // The screen holding the target: a card never sits on its
    // background, only the line may enter it. A target placed straight in its artboard has no
    // inner screen: the artboard is the screen (a phone frame), not a board for notes.
    let inner = screen_of(doc, target);
    let screen = inner.or(frame);
    let frame = frame.filter(|_| inner.is_some());
    let route_obs = route_obstacles(doc, target);
    let blocked = |r: &Rect| {
        let g = r.inflate(MARGIN);
        obs.iter().chain(&screen).any(|o| overlaps(&g, o)) || placed.iter().any(|p| overlaps(&g, &p.rect))
    };
    let card_at = |x: f64, y: f64| Rect::new(x, y, size.x, size.y);
    let mut found: Option<Rect> = None;
    let mut best = f64::MAX;
    for (radius, step) in RINGS {
        let win = t.inflate(radius);
        let near: Vec<Rect> = obs.iter().chain(placed.iter().map(|p| &p.rect)).filter(|o| overlaps(&win.inflate(size.x), o)).copied().collect();
        let mut xs = vec![t.right() + MARGIN, t.x - MARGIN - size.x, t.x, t.right() - size.x, t.center().x - size.x / 2.0];
        let mut ys = vec![t.bottom() + MARGIN, t.y - MARGIN - size.y, t.y, t.bottom() - size.y, t.center().y - size.y / 2.0];
        for o in near.iter().chain(&screen) {
            xs.extend([o.right() + MARGIN, o.x - MARGIN - size.x, o.x]);
            ys.extend([o.bottom() + MARGIN, o.y - MARGIN - size.y, o.y]);
        }
        let mut g = win.x;
        while g <= win.right() - size.x {
            xs.push(g);
            g += step;
        }
        let mut g = win.y;
        while g <= win.bottom() - size.y {
            ys.push(g);
            g += step;
        }
        let keep = |v: &mut Vec<f64>, lo: f64, hi: f64| {
            v.retain(|p| *p >= lo && *p <= hi);
            v.sort_by(f64::total_cmp);
            v.dedup_by(|a, b| (*a - *b).abs() < 0.5);
        };
        keep(&mut xs, win.x, win.right() - size.x);
        keep(&mut ys, win.y, win.bottom() - size.y);
        let mut cands: Vec<(f64, Rect)> = Vec::with_capacity(xs.len() * ys.len());
        for &x in &xs {
            for &y in &ys {
                let r = card_at(x, y);
                let frame_cost = frame.map_or(0.0, |f| {
                    if f.inflate(-MARGIN).contains_rect(&r) {
                        0.0
                    } else if overlaps(&f, &r) {
                        STRADDLE_COST // straddling the frame's edge
                    } else {
                        OUTSIDE_FRAME_COST
                    }
                });
                let c = gap_distance(&r, &t) + 0.3 * (r.center() - t.center()).len() + frame_cost;
                cands.push((c, r));
            }
        }
        cands.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut tried = 0;
        for (base, r) in cands {
            if tried >= FEASIBLE_TRIED || base >= best {
                break;
            }
            if blocked(&r) {
                continue;
            }
            tried += 1;
            let end = anchor_point(t, anchor_toward(t, r));
            let mut around = route_obs.clone();
            around.extend(placed.iter().map(|p| p.rect));
            let line = route(r, t, end, &around);
            let crossings = around.iter().filter(|o| crosses_rect(&line, o)).count()
                + placed.iter().filter(|p| paths_cross(&line, &p.line) || crosses_rect(&p.line, &r)).count();
            let len: f64 = line.windows(2).map(|w| (w[1] - w[0]).len()).sum();
            let bends = line.len().saturating_sub(2) as f64;
            let cost = base + LINE_COST * len + BEND_PENALTY * bends + CROSS_COST * crossings as f64;
            if cost < best {
                best = cost;
                found = Some(r);
            }
        }
        // A card straddling the frame's edge is worth a wider search.
        if best < STRADDLE_COST {
            break;
        }
    }
    let r = found.unwrap_or_else(|| {
        // Nothing free nearby: below everything on the page, level with the target's x.
        let all = Rect::union_all(obs.iter().chain(placed.iter().map(|p| &p.rect))).unwrap_or(t);
        card_at(t.x, all.bottom() + MARGIN * 2.0)
    });
    Some(Placement { position: r.min() - t.min(), anchor: anchor_toward(t, r) })
}

/// Place `a` among every other note on the page.
pub fn place(doc: &Document, a: &Annotation) -> Option<Placement> {
    place_among(doc, a, &existing(doc, |id| id == a.id))
}

/// New positions for the notes on `frame` (all targeted notes when `None`), in reading order,
/// each placed clear of the others.
pub fn tidy(doc: &Document, frame: Option<NodeId>) -> Vec<(u64, Placement)> {
    let mut todo: Vec<(&Annotation, Rect)> = doc
        .annotations
        .iter()
        .filter(|a| !a.detached)
        .filter_map(|a| Some((a, doc.world_rect(a.target?)?)))
        .filter(|(a, _)| frame.is_none_or(|f| a.target.and_then(|t| frame_of(doc, t)) == Some(f)))
        .collect();
    todo.sort_by(|a, b| (a.1.y, a.1.x, a.0.id).partial_cmp(&(b.1.y, b.1.x, b.0.id)).unwrap_or(std::cmp::Ordering::Equal));
    let moving: Vec<u64> = todo.iter().map(|(a, _)| a.id).collect();
    let mut placed = existing(doc, |id| moving.contains(&id));
    let mut out = Vec::new();
    for (a, t) in todo {
        let Some(p) = place_among(doc, a, &placed) else { continue };
        let mut moved = a.clone();
        moved.position = p.position;
        moved.anchor = p.anchor;
        let at = t.min() + p.position;
        let s = card_size(a);
        let rect = Rect::new(at.x, at.y, s.x, s.y);
        let line = elbow(rect, anchor_point(t, p.anchor));
        placed.push(Placed { id: a.id, rect, line });
        out.push((a.id, p));
    }
    out
}

// ------------------------------------------------------------------------------ routing

/// Orthogonal leader line from `card` to `end` (on `target`'s edge), around `obstacles` (which
/// must not include the card or target). Falls back to the simple elbow when no path exists.
pub fn route(card: Rect, target: Rect, end: V2, obstacles: &[Rect]) -> Vec<V2> {
    let simple = elbow(card, end);
    let start = simple[0];
    let win = card.union(&target).inflate(ROUTE_WINDOW);
    let mut obs: Vec<Rect> = obstacles
        .iter()
        .filter(|o| overlaps(&win, o))
        .map(|o| o.inflate(ROUTE_PAD))
        .filter(|o| !strictly_inside(o, start) && !strictly_inside(o, end))
        .collect();
    if obs.len() > ROUTE_MAX_OBSTACLES {
        return simple;
    }
    obs.push(card);
    obs.push(target);
    if !obs[..obs.len() - 2].iter().any(|o| crosses_rect(&simple, o)) {
        return simple;
    }
    astar(start, end, &obs, win).unwrap_or(simple)
}

fn axis(vals: impl Iterator<Item = f64>) -> Vec<f64> {
    let mut v: Vec<f64> = vals.collect();
    v.sort_by(f64::total_cmp);
    v.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
    v
}

fn idx(v: &[f64], x: f64) -> usize {
    v.partition_point(|p| *p < x - 1e-6)
}

fn astar(start: V2, end: V2, obs: &[Rect], win: Rect) -> Option<Vec<V2>> {
    let xs = axis(obs.iter().flat_map(|o| [o.x, o.right()]).chain([start.x, end.x, win.x, win.right()]));
    let ys = axis(obs.iter().flat_map(|o| [o.y, o.bottom()]).chain([start.y, end.y, win.y, win.bottom()]));
    let (nx, ny) = (xs.len(), ys.len());
    let at = |i: usize, j: usize| j * nx + i;
    let mut node = vec![false; nx * ny];
    let mut hedge = vec![false; nx * ny];
    let mut vedge = vec![false; nx * ny];
    for o in obs {
        let (i0, i1, j0, j1) = (idx(&xs, o.x), idx(&xs, o.right()), idx(&ys, o.y), idx(&ys, o.bottom()));
        for j in j0..=j1.min(ny - 1) {
            for i in i0..=i1.min(nx - 1) {
                let (in_x, in_y) = (i > i0 && i < i1, j > j0 && j < j1);
                if in_x && in_y {
                    node[at(i, j)] = true;
                }
                if i < i1 && in_y {
                    hedge[at(i, j)] = true;
                }
                if j < j1 && in_x {
                    vedge[at(i, j)] = true;
                }
            }
        }
    }
    let (si, sj, ei, ej) = (idx(&xs, start.x), idx(&ys, start.y), idx(&xs, end.x), idx(&ys, end.y));
    const DIRS: [(i32, i32); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];
    let state = |i: usize, j: usize, d: usize| at(i, j) * 5 + d;
    let mut dist = vec![f64::MAX; nx * ny * 5];
    let mut parent = vec![usize::MAX; nx * ny * 5];
    let mut heap = BinaryHeap::new();
    let key = |f: f64| Reverse((f * 1000.0) as u64);
    let h = |i: usize, j: usize| (xs[i] - end.x).abs() + (ys[j] - end.y).abs();
    dist[state(si, sj, 4)] = 0.0;
    heap.push((key(h(si, sj)), si, sj, 4usize));
    let mut goal = None;
    while let Some((_, i, j, d)) = heap.pop() {
        if (i, j) == (ei, ej) {
            goal = Some(state(i, j, d));
            break;
        }
        let cur = dist[state(i, j, d)];
        for (nd, (di, dj)) in DIRS.iter().enumerate() {
            let (ni, nj) = (i as i32 + di, j as i32 + dj);
            if ni < 0 || nj < 0 || ni as usize >= nx || nj as usize >= ny {
                continue;
            }
            let (ni, nj) = (ni as usize, nj as usize);
            let blocked = node[at(ni, nj)]
                || match nd {
                    0 => hedge[at(i, j)],
                    1 => hedge[at(ni, nj)],
                    2 => vedge[at(i, j)],
                    _ => vedge[at(ni, nj)],
                };
            if blocked {
                continue;
            }
            let len = (xs[ni] - xs[i]).abs() + (ys[nj] - ys[j]).abs();
            let cost = cur + len + if d != 4 && d != nd { BEND_COST } else { 0.0 };
            let s = state(ni, nj, nd);
            if cost < dist[s] {
                dist[s] = cost;
                parent[s] = state(i, j, d);
                heap.push((key(cost + h(ni, nj)), ni, nj, nd));
            }
        }
    }
    let mut s = goal?;
    let mut pts = Vec::new();
    loop {
        let cell = s / 5;
        pts.push(v2(xs[cell % nx], ys[cell / nx]));
        if parent[s] == usize::MAX {
            break;
        }
        s = parent[s];
    }
    pts.reverse();
    pts.dedup_by(|a, b| (a.x - b.x).abs() < 1e-6 && (a.y - b.y).abs() < 1e-6);
    let mut out: Vec<V2> = Vec::new();
    for p in pts {
        while out.len() >= 2 {
            let (a, b) = (out[out.len() - 2], out[out.len() - 1]);
            let collinear = ((a.x - b.x).abs() < 1e-6 && (b.x - p.x).abs() < 1e-6) || ((a.y - b.y).abs() < 1e-6 && (b.y - p.y).abs() < 1e-6);
            if !collinear {
                break;
            }
            out.pop();
        }
        out.push(p);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_estimate_counts_words_and_newlines() {
        assert_eq!(wrap_lines("", 200.0, 11.0), 1);
        assert_eq!(wrap_lines("short", 200.0, 11.0), 1);
        assert_eq!(wrap_lines("a\nb\nc", 200.0, 11.0), 3);
        let long = "word ".repeat(60);
        let n = wrap_lines(&long, BODY_W, BODY_PT);
        assert!((5..=9).contains(&n), "{n}");
        assert!(wrap_lines(&"x".repeat(200), 100.0, 11.0) >= 6);
    }

    #[test]
    fn card_grows_then_caps_with_more_lines() {
        let mut a = Annotation::new(None, "One Primary button per view and nothing else here", "rule");
        let small = card_size(&a);
        assert!(small.y >= PAD * 2.0 + LABEL_LH * 2.0 - 1.0, "label wraps to 2 rows: {small:?}");
        a.body = "Danger only for destructive actions\nsecond".into();
        assert!(card_size(&a).y > small.y);
        let l = card_lines(&a.label, &a.body, false);
        assert_eq!((l.label, l.body, l.more), (2, 2, 0));
        a.body = (1..=20).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n");
        let l = card_lines(&a.label, &a.body, false);
        assert_eq!((l.body, l.more), (BODY_LINES, 12));
        assert_eq!(card_lines(&a.label, &a.body, true).body, 20);
        assert!(card_size(&a).y < 400.0);
    }

    #[test]
    fn route_goes_around_a_wall_and_straight_when_clear() {
        let card = Rect::new(0.0, 0.0, 100.0, 40.0);
        let target = Rect::new(400.0, 0.0, 40.0, 40.0);
        let end = v2(400.0, 20.0);
        let direct = route(card, target, end, &[]);
        assert_eq!(direct.len(), 2);
        let wall = Rect::new(200.0, -200.0, 40.0, 400.0);
        let r = route(card, target, end, &[wall]);
        assert!(r.len() > 2);
        assert!(r.windows(2).all(|w| !overlaps(&seg_box(w[0], w[1]).inflate(0.1), &wall)));
        assert!(r.windows(2).all(|w| (w[0].x - w[1].x).abs() < 1e-6 || (w[0].y - w[1].y).abs() < 1e-6), "orthogonal");
        assert_eq!(*r.last().unwrap(), v2(400.0, 20.0));
    }

    #[test]
    fn anchor_faces_the_card() {
        let t = Rect::new(100.0, 100.0, 200.0, 50.0);
        let right = Rect::new(400.0, 300.0, 50.0, 20.0);
        assert_eq!(anchor_toward(t, right).side, Side::Right);
        let below = Rect::new(150.0, 200.0, 280.0, 20.0);
        let a = anchor_toward(t, below);
        assert_eq!(a.side, Side::Bottom);
        assert!((anchor_point(t, a).x - 225.0).abs() < 1e-6);
    }
}
