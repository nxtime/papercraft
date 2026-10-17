//! Auto layout (Figma / CSS flexbox) and resize constraints.
//!
//! Behavior follows `analysis/FIGMA_NOTES.md` §3–4: children in flow are placed along the main
//! axis with gap and padding, aligned on both axes, sized Fixed / Hug / Fill; hidden and
//! absolute children are skipped. Hug sizes resolve bottom-up, fill sizes top-down.

use crate::geom::{Rect, V2, v2};
use crate::history::Tx;
use crate::model::{Align, AutoLayout, Constraint, Direction, Document, NodeId, NodeKind, Sizing};

fn main_axis(l: &AutoLayout) -> usize {
    match l.direction {
        Direction::Horizontal => 0,
        Direction::Vertical => 1,
    }
}

fn get(r: &Rect, axis: usize) -> (f64, f64) {
    if axis == 0 { (r.x, r.w) } else { (r.y, r.h) }
}

fn set(r: &mut Rect, axis: usize, pos: f64, size: f64) {
    if axis == 0 {
        r.x = pos;
        r.w = size;
    } else {
        r.y = pos;
        r.h = size;
    }
}

/// Children that take part in the flow.
pub fn flow_children(doc: &Document, id: NodeId) -> Vec<NodeId> {
    doc.children(Some(id))
        .iter()
        .copied()
        .filter(|c| {
            doc.get(*c)
                .is_some_and(|n| n.visible && !n.layout_child.absolute)
        })
        .collect()
}

/// Intrinsic (hug) size of a node along an axis, without changing anything.
fn hug_size(doc: &Document, id: NodeId, axis: usize) -> f64 {
    let n = doc.get(id).unwrap();
    let (_, cur) = get(&n.rect, axis);
    let Some(l) = n.layout else { return cur };
    let kids = flow_children(doc, id);
    let pad = if axis == 0 {
        l.padding[3] + l.padding[1]
    } else {
        l.padding[0] + l.padding[2]
    };
    let sizes: Vec<f64> = kids
        .iter()
        .map(|k| {
            let kn = doc.get(*k).unwrap();
            match kn.layout_child.sizing[axis] {
                Sizing::Hug => hug_size(doc, *k, axis),
                Sizing::Fill => 0.0,
                Sizing::Fixed => get(&kn.rect, axis).1,
            }
        })
        .collect();
    if axis == main_axis(&l) && !l.wrap {
        let gap = if l.gap_auto { 0.0 } else { l.gap };
        pad + sizes.iter().sum::<f64>() + gap * (kids.len().saturating_sub(1)) as f64
    } else if axis == main_axis(&l) {
        // Wrapping: hug the widest child.
        pad + sizes.iter().cloned().fold(0.0, f64::max)
    } else if l.wrap {
        // Counter axis with wrapping: lines stacked (computed from the current main size).
        let (_, main_size) = get(&n.rect, main_axis(&l));
        let lines = wrap_lines(doc, id, &l, main_size, &kids);
        let cross: Vec<f64> = lines
            .iter()
            .map(|line| {
                line.iter()
                    .map(|k| {
                        let kn = doc.get(*k).unwrap();
                        match kn.layout_child.sizing[axis] {
                            Sizing::Hug => hug_size(doc, *k, axis),
                            Sizing::Fill => get(&kn.rect, axis).1,
                            Sizing::Fixed => get(&kn.rect, axis).1,
                        }
                    })
                    .fold(0.0, f64::max)
            })
            .collect();
        pad + cross.iter().sum::<f64>() + l.cross_gap * cross.len().saturating_sub(1) as f64
    } else {
        pad + sizes.iter().cloned().fold(0.0, f64::max)
    }
}

fn wrap_lines(
    doc: &Document,
    _id: NodeId,
    l: &AutoLayout,
    main_size: f64,
    kids: &[NodeId],
) -> Vec<Vec<NodeId>> {
    let ma = main_axis(l);
    let inner = main_size
        - if ma == 0 {
            l.padding[3] + l.padding[1]
        } else {
            l.padding[0] + l.padding[2]
        };
    let mut lines: Vec<Vec<NodeId>> = vec![Vec::new()];
    let mut x = 0.0;
    for k in kids {
        let kn = doc.get(*k).unwrap();
        let s = match kn.layout_child.sizing[ma] {
            Sizing::Hug => hug_size(doc, *k, ma),
            _ => get(&kn.rect, ma).1,
        };
        let line = lines.last_mut().unwrap();
        if !line.is_empty() && x + l.gap + s > inner + 1e-9 {
            lines.push(vec![*k]);
            x = s;
        } else {
            x += if line.is_empty() { s } else { l.gap + s };
            line.push(*k);
        }
    }
    lines.retain(|l| !l.is_empty());
    lines
}

/// Lay out one auto-layout frame and, recursively, its auto-layout descendants.
/// Returns true if anything moved or resized.
pub fn layout_frame(tx: &mut Tx, id: NodeId) -> bool {
    let Some(l) = tx.doc().get(id).and_then(|n| n.layout) else {
        return false;
    };
    let before: Vec<(NodeId, Rect)> = tx
        .doc()
        .subtree(id)
        .iter()
        .filter_map(|i| tx.doc().get(*i).map(|n| (*i, n.rect)))
        .collect();

    // 1. Children that hug resolve first (bottom-up).
    for k in flow_children(tx.doc(), id) {
        if tx.doc().get(k).is_some_and(|c| c.layout.is_some()) {
            layout_frame(tx, k);
        }
    }
    // 2. This frame's hug size.
    let ma = main_axis(&l);
    let ca = 1 - ma;
    let n = tx.doc().get(id).unwrap().clone();
    let sizing = n.layout_child.sizing;
    let mut rect = n.rect;
    for axis in [0, 1] {
        if sizing[axis] == Sizing::Hug {
            let any_fill = flow_children(tx.doc(), id)
                .iter()
                .any(|k| tx.doc().get(*k).unwrap().layout_child.sizing[axis] == Sizing::Fill);
            // A fill child on the main axis turns hug into fixed (Figma rule).
            if !(any_fill && axis == ma) {
                let s = hug_size(tx.doc(), id, axis);
                let (p, _) = get(&rect, axis);
                set(&mut rect, axis, p, s.max(0.0));
            }
        }
    }
    if rect != n.rect {
        tx.node_mut(id).unwrap().rect = rect;
    }
    arrange(tx, id, &l, rect, ma, ca);
    let after: Vec<(NodeId, Rect)> = tx
        .doc()
        .subtree(id)
        .iter()
        .filter_map(|i| tx.doc().get(*i).map(|n| (*i, n.rect)))
        .collect();
    before != after
}

fn arrange(tx: &mut Tx, id: NodeId, l: &AutoLayout, rect: Rect, ma: usize, ca: usize) {
    let kids = flow_children(tx.doc(), id);
    let pad = l.padding; // t r b l
    let (p0m, p1m) = if ma == 0 {
        (pad[3], pad[1])
    } else {
        (pad[0], pad[2])
    };
    let (p0c, p1c) = if ma == 0 {
        (pad[0], pad[2])
    } else {
        (pad[3], pad[1])
    };
    let inner_m = get(&rect, ma).1 - p0m - p1m;
    let inner_c = get(&rect, ca).1 - p0c - p1c;

    let lines: Vec<Vec<NodeId>> = if l.wrap {
        wrap_lines(tx.doc(), id, l, get(&rect, ma).1, &kids)
    } else {
        vec![kids.clone()]
    };
    let line_cross_sizes: Vec<f64> = if l.wrap {
        lines
            .iter()
            .map(|line| {
                line.iter()
                    .map(|k| {
                        let kn = tx.doc().get(*k).unwrap();
                        match kn.layout_child.sizing[ca] {
                            Sizing::Hug => hug_size(tx.doc(), *k, ca),
                            Sizing::Fill | Sizing::Fixed => get(&kn.rect, ca).1,
                        }
                    })
                    .fold(0.0, f64::max)
            })
            .collect()
    } else {
        vec![inner_c; lines.len()]
    };
    let mut cross_cursor = if l.wrap {
        let total_cross: f64 = line_cross_sizes.iter().sum::<f64>()
            + l.cross_gap * lines.len().saturating_sub(1) as f64;
        let free_cross = (inner_c - total_cross).max(0.0);
        p0c + match l.align_cross {
            Align::Start => 0.0,
            Align::Center => free_cross * 0.5,
            Align::End => free_cross,
        }
    } else {
        p0c
    };
    for (li, line) in lines.into_iter().enumerate() {
        // Main sizes: fixed / hug as they are; fill shares the rest equally.
        let mut sizes: Vec<f64> = line
            .iter()
            .map(|k| {
                let kn = tx.doc().get(*k).unwrap();
                match kn.layout_child.sizing[ma] {
                    Sizing::Fill => 0.0,
                    Sizing::Hug => hug_size(tx.doc(), *k, ma),
                    Sizing::Fixed => get(&kn.rect, ma).1,
                }
            })
            .collect();
        let gap = if l.gap_auto { 0.0 } else { l.gap };
        let fixed: f64 = sizes.iter().sum::<f64>() + gap * line.len().saturating_sub(1) as f64;
        let fills: Vec<usize> = (0..line.len())
            .filter(|i| tx.doc().get(line[*i]).unwrap().layout_child.sizing[ma] == Sizing::Fill)
            .collect();
        if !fills.is_empty() && !l.wrap {
            let share = ((inner_m - fixed) / fills.len() as f64).max(0.0);
            for i in &fills {
                sizes[*i] = share;
            }
        }
        let used: f64 = sizes.iter().sum::<f64>() + gap * line.len().saturating_sub(1) as f64;
        let free = inner_m - used;
        let (mut cursor, gap) = if l.gap_auto && line.len() > 1 {
            (
                p0m,
                ((inner_m - sizes.iter().sum::<f64>()) / (line.len() - 1) as f64).max(0.0),
            )
        } else {
            let start = match l.align_main {
                Align::Start => p0m,
                Align::Center => p0m + free * 0.5,
                Align::End => p0m + free,
            };
            (start, gap)
        };
        let lh = line_cross_sizes[li];
        for (i, k) in line.iter().enumerate() {
            let kn = tx.doc().get(*k).unwrap();
            let mut r = kn.rect;
            let (cpos, cs) = match kn.layout_child.sizing[ca] {
                Sizing::Fill => (cross_cursor, lh.max(0.0)),
                Sizing::Fixed | Sizing::Hug => {
                    let ch = if kn.layout_child.sizing[ca] == Sizing::Hug {
                        hug_size(tx.doc(), *k, ca)
                    } else {
                        get(&r, ca).1
                    };
                    let cy = match l.align_cross {
                        Align::Start => cross_cursor,
                        Align::Center => cross_cursor + (lh - ch) * 0.5,
                        Align::End => cross_cursor + lh - ch,
                    };
                    (cy, ch)
                }
            };
            set(&mut r, ma, cursor, sizes[i]);
            set(&mut r, ca, cpos, cs);
            cursor += sizes[i] + gap;
            if r != kn.rect {
                let resized = (r.w - kn.rect.w).abs() > 1e-9 || (r.h - kn.rect.h).abs() > 1e-9;
                let old = kn.rect;
                let n = tx.node_mut(*k).unwrap();
                if resized {
                    n.set_rect_scaling(r);
                } else {
                    n.rect = r;
                }
                if resized {
                    resize_children(tx, *k, old, r);
                    fit_text_height(tx, *k);
                }
            }
        }
        cross_cursor += lh + l.cross_gap;
    }
    // Fill children that are auto-layout frames lay out their own content again.
    for k in kids {
        if tx.doc().get(k).is_some_and(|c| c.layout.is_some()) {
            let sub = tx.doc().get(k).unwrap();
            let (l2, r2) = (sub.layout.unwrap(), sub.rect);
            let ma2 = main_axis(&l2);
            arrange(tx, k, &l2, r2, ma2, 1 - ma2);
        }
    }
    // Absolute children keep their constraints-based position.
}

/// Lay out `flow_children(tx.doc(), id)` into a `cols`-column CSS-style grid inside frame `id`.
/// Returns true if anything moved or resized.
pub fn layout_grid(tx: &mut Tx, id: NodeId, cols: usize, col_gap: f64, row_gap: f64) -> bool {
    let Some(n0) = tx.doc().get(id).cloned() else {
        return false;
    };
    let cols = cols.max(1);
    let before: Vec<(NodeId, Rect)> = tx
        .doc()
        .subtree(id)
        .iter()
        .filter_map(|i| tx.doc().get(*i).map(|n| (*i, n.rect)))
        .collect();

    let kids = flow_children(tx.doc(), id);
    for k in &kids {
        if tx.doc().get(*k).is_some_and(|c| c.layout.is_some()) {
            layout_frame(tx, *k);
        }
    }

    let n = tx.doc().get(id).unwrap().clone();
    let pad = n.layout.unwrap_or_default().padding;
    let (pt, pr, pb, pl) = (pad[0], pad[1], pad[2], pad[3]);
    let rows = kids.len().div_ceil(cols);

    let mut col_w: Vec<f64> = vec![0.0; cols];
    let mut col_fill = vec![false; cols];
    for (idx, k) in kids.iter().enumerate() {
        let c = idx % cols;
        let kn = tx.doc().get(*k).unwrap();
        if kn.layout_child.sizing[0] == Sizing::Fill {
            col_fill[c] = true;
        } else {
            let w = if kn.layout_child.sizing[0] == Sizing::Hug {
                hug_size(tx.doc(), *k, 0)
            } else {
                kn.rect.w
            };
            col_w[c] = col_w[c].max(w);
        }
    }

    let total_col_gaps = col_gap * cols.saturating_sub(1) as f64;
    let fill_cols: Vec<usize> = (0..cols).filter(|&c| col_fill[c]).collect();
    if !fill_cols.is_empty() {
        let fixed_w: f64 = (0..cols).filter(|&c| !col_fill[c]).map(|c| col_w[c]).sum();
        let inner_w = (n.rect.w - pl - pr).max(0.0);
        let share = ((inner_w - total_col_gaps - fixed_w) / fill_cols.len() as f64).max(0.0);
        for c in fill_cols {
            col_w[c] = share;
        }
    }

    let mut row_h: Vec<f64> = vec![0.0; rows];
    for (idx, k) in kids.iter().enumerate() {
        let r = idx / cols;
        let kn = tx.doc().get(*k).unwrap();
        let h = if kn.layout_child.sizing[1] == Sizing::Hug {
            hug_size(tx.doc(), *k, 1)
        } else {
            kn.rect.h
        };
        row_h[r] = row_h[r].max(h);
    }

    let total_row_gaps = row_gap * rows.saturating_sub(1) as f64;
    let mut rect = n.rect;
    if n.layout_child.sizing[0] == Sizing::Hug {
        let tracks_w = if kids.is_empty() {
            0.0
        } else {
            col_w.iter().sum::<f64>() + total_col_gaps
        };
        rect.w = (pl + pr + tracks_w).max(0.0);
    }
    if n.layout_child.sizing[1] == Sizing::Hug {
        let tracks_h = if rows == 0 {
            0.0
        } else {
            row_h.iter().sum::<f64>() + total_row_gaps
        };
        rect.h = (pt + pb + tracks_h).max(0.0);
    }
    if rect != n0.rect {
        tx.node_mut(id).unwrap().rect = rect;
    }

    let mut col_x = vec![0.0; cols];
    let mut cx = pl;
    for c in 0..cols {
        col_x[c] = cx;
        cx += col_w[c] + col_gap;
    }
    let mut row_y = vec![0.0; rows];
    let mut ry = pt;
    for r in 0..rows {
        row_y[r] = ry;
        ry += row_h[r] + row_gap;
    }

    for (idx, k) in kids.iter().enumerate() {
        let c = idx % cols;
        let r = idx / cols;
        let kn = tx.doc().get(*k).unwrap();
        let mut cr = kn.rect;
        cr.x = col_x[c];
        cr.y = row_y[r];
        if kn.layout_child.sizing[0] == Sizing::Fill {
            cr.w = col_w[c];
        } else if kn.layout_child.sizing[0] == Sizing::Hug {
            cr.w = hug_size(tx.doc(), *k, 0);
        }
        if kn.layout_child.sizing[1] == Sizing::Fill {
            cr.h = row_h[r];
        } else if kn.layout_child.sizing[1] == Sizing::Hug {
            cr.h = hug_size(tx.doc(), *k, 1);
        }
        if cr != kn.rect {
            let resized = (cr.w - kn.rect.w).abs() > 1e-9 || (cr.h - kn.rect.h).abs() > 1e-9;
            let old = kn.rect;
            let cn = tx.node_mut(*k).unwrap();
            if resized {
                cn.set_rect_scaling(cr);
            } else {
                cn.rect = cr;
            }
            if resized {
                resize_children(tx, *k, old, cr);
                fit_text_height(tx, *k);
            }
        }
    }

    let after: Vec<(NodeId, Rect)> = tx
        .doc()
        .subtree(id)
        .iter()
        .filter_map(|i| tx.doc().get(*i).map(|n| (*i, n.rect)))
        .collect();
    before != after
}

/// Re-measure a text node's height after its width changed.
pub fn fit_text_height(tx: &mut Tx, id: NodeId) {
    let Some(n) = tx.doc().get(id) else { return };
    if !matches!(n.kind, NodeKind::Text { .. }) {
        return;
    }
    if let Some(l) = crate::text::layout_node(n)
        && (l.height - n.rect.h).abs() > 1e-9
    {
        tx.node_mut(id).unwrap().rect.h = l.height;
    }
}

/// After a container resized from `old` to `new` (both in its parent's space), update its
/// children: constraints for frames, proportional scaling for groups, layout for auto layout.
pub fn resize_children(tx: &mut Tx, id: NodeId, old: Rect, new: Rect) {
    let Some(n) = tx.doc().get(id) else { return };
    if n.children.is_empty() {
        return;
    }
    let group = n.is_group();
    let auto = n.layout.is_some();
    let kids = n.children.clone();
    if auto {
        let l = n.layout.unwrap();
        let ma = main_axis(&l);
        // Absolute children still follow constraints.
        for k in &kids {
            if tx.doc().get(*k).is_some_and(|c| c.layout_child.absolute) {
                apply_constraints(tx, *k, old, new);
            }
        }
        arrange(tx, id, &l, new, ma, 1 - ma);
        return;
    }
    for k in kids {
        if group {
            scale_child(tx, k, old, new);
        } else {
            apply_constraints(tx, k, old, new);
        }
    }
}

fn scale_child(tx: &mut Tx, k: NodeId, old: Rect, new: Rect) {
    let sx = if old.w.abs() > 1e-9 {
        new.w / old.w
    } else {
        1.0
    };
    let sy = if old.h.abs() > 1e-9 {
        new.h / old.h
    } else {
        1.0
    };
    let Some(c) = tx.doc().get(k) else { return };
    let r0 = c.rect;
    let r = Rect::new(r0.x * sx, r0.y * sy, r0.w * sx, r0.h * sy);
    tx.node_mut(k).unwrap().set_rect_scaling(r);
    resize_children(tx, k, r0, r);
}

/// Reposition/resize one child for a parent size change (positions relative to the parent).
pub fn apply_constraints(tx: &mut Tx, k: NodeId, old: Rect, new: Rect) {
    let Some(c) = tx.doc().get(k) else { return };
    let r0 = c.rect;
    let cons = c.layout_child.constraints;
    let axis = |con: Constraint, p: f64, s: f64, old_s: f64, new_s: f64| -> (f64, f64) {
        let far = old_s - (p + s);
        match con {
            Constraint::Min => (p, s),
            Constraint::Max => (new_s - far - s, s),
            Constraint::Stretch => (p, (new_s - p - far).max(0.0)),
            Constraint::Center => (new_s * 0.5 + (p + s * 0.5 - old_s * 0.5) - s * 0.5, s),
            Constraint::Scale => {
                let k = if old_s.abs() > 1e-9 {
                    new_s / old_s
                } else {
                    1.0
                };
                (p * k, s * k)
            }
        }
    };
    let (x, w) = axis(cons.h, r0.x, r0.w, old.w, new.w);
    let (y, h) = axis(cons.v, r0.y, r0.h, old.h, new.h);
    let r = Rect::new(x, y, w, h);
    if r == r0 {
        return;
    }
    let is_text = matches!(c.kind, NodeKind::Text { .. });
    let n = tx.node_mut(k).unwrap();
    if (r.w - r0.w).abs() > 1e-9 || (r.h - r0.h).abs() > 1e-9 {
        n.set_rect_scaling(r);
    } else {
        n.rect = r;
    }
    let _ = is_text;
    resize_children(tx, k, r0, r);
}

/// Recompute a group's box to the union of its children (keeping their world positions).
/// Empty groups are removed. Returns true if changed.
pub fn normalize_group(tx: &mut Tx, id: NodeId) -> bool {
    let Some(g) = tx.doc().get(id) else {
        return false;
    };
    if !g.is_group() {
        return false;
    }
    if g.children.is_empty() {
        tx.remove(id);
        return true;
    }
    let rects: Vec<Rect> = g
        .children
        .iter()
        .filter_map(|c| tx.doc().get(*c).map(|n| n.rect))
        .collect();
    let u = Rect::union_all(&rects).unwrap();
    if u.x == 0.0 && u.y == 0.0 && u.w == g.rect.w && u.h == g.rect.h {
        return false;
    }
    let d = v2(u.x, u.y);
    let kids = g.children.clone();
    let gr = g.rect;
    for c in kids {
        let n = tx.node_mut(c).unwrap();
        n.rect = n.rect.translate(V2::ZERO - d);
    }
    tx.node_mut(id).unwrap().rect = Rect::new(gr.x + d.x, gr.y + d.y, u.w, u.h);
    true
}

/// Child index at which a dragged child should land in an auto-layout frame, from the
/// child's center (frame-local) along the main axis.
pub fn drop_index(doc: &Document, frame: NodeId, dragged: NodeId, center: V2) -> usize {
    let Some(l) = doc.get(frame).and_then(|f| f.layout) else {
        return 0;
    };
    let ma = main_axis(&l);
    let c = if ma == 0 { center.x } else { center.y };
    let others: Vec<NodeId> = doc
        .children(Some(frame))
        .iter()
        .copied()
        .filter(|k| *k != dragged)
        .collect();
    let mut idx = 0;
    for (i, k) in others.iter().enumerate() {
        let r = doc.get(*k).unwrap().rect;
        let (p, s) = get(&r, ma);
        if c > p + s * 0.5 {
            idx = i + 1;
        }
    }
    idx
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::Recording;
    use crate::model::{Constraints, Node};

    fn doc_with(
        frame: AutoLayout,
        sizing: [Sizing; 2],
        kids: &[(f64, f64, [Sizing; 2])],
    ) -> (Document, NodeId, Vec<NodeId>) {
        let mut doc = Document::default();
        let mut rec = Recording::new("t", &doc);
        let mut tx = Tx::new(&mut doc, &mut rec);
        let fid = tx.alloc_id();
        let mut f = Node::new(
            fid,
            NodeKind::Frame { clip: true },
            Rect::new(100.0, 100.0, 300.0, 200.0),
        );
        f.layout = Some(frame);
        f.layout_child.sizing = sizing;
        tx.insert(f, None, None);
        let mut ids = Vec::new();
        for (w, h, s) in kids {
            let id = tx.alloc_id();
            let mut n = Node::new(id, NodeKind::Rect, Rect::new(0.0, 0.0, *w, *h));
            n.layout_child.sizing = *s;
            tx.insert(n, Some(fid), None);
            ids.push(id);
        }
        layout_frame(&mut tx, fid);
        (doc, fid, ids)
    }

    const FX: [Sizing; 2] = [Sizing::Fixed, Sizing::Fixed];

    #[test]
    fn horizontal_hug_with_gap_and_padding() {
        let l = AutoLayout {
            gap: 8.0,
            padding: [10.0, 20.0, 10.0, 20.0],
            ..Default::default()
        };
        let (doc, f, k) = doc_with(l, [Sizing::Hug; 2], &[(50.0, 30.0, FX), (40.0, 60.0, FX)]);
        let fr = doc.get(f).unwrap().rect;
        assert_eq!(
            (fr.w, fr.h),
            (20.0 + 50.0 + 8.0 + 40.0 + 20.0, 10.0 + 60.0 + 10.0)
        );
        assert_eq!(
            doc.get(k[0]).unwrap().rect,
            Rect::new(20.0, 10.0, 50.0, 30.0)
        );
        assert_eq!(
            doc.get(k[1]).unwrap().rect,
            Rect::new(78.0, 10.0, 40.0, 60.0)
        );
        assert_eq!((fr.x, fr.y), (100.0, 100.0), "hug keeps the frame's origin");
    }

    #[test]
    fn fill_children_share_space_and_center_alignment() {
        let l = AutoLayout {
            direction: Direction::Vertical,
            gap: 10.0,
            padding: [0.0; 4],
            align_cross: Align::Center,
            ..Default::default()
        };
        let (doc, _, k) = doc_with(
            l,
            FX,
            &[
                (100.0, 50.0, FX),
                (50.0, 0.0, [Sizing::Fixed, Sizing::Fill]),
                (300.0, 0.0, [Sizing::Fill, Sizing::Fill]),
            ],
        );
        // 200 tall: 50 fixed + 2 gaps of 10 = 70; two fills share 130.
        assert_eq!(
            doc.get(k[1]).unwrap().rect,
            Rect::new(125.0, 60.0, 50.0, 65.0)
        );
        assert_eq!(
            doc.get(k[2]).unwrap().rect,
            Rect::new(0.0, 135.0, 300.0, 65.0)
        );
        assert_eq!(doc.get(k[0]).unwrap().rect.x, 100.0);
    }

    #[test]
    fn space_between_and_end_alignment() {
        let l = AutoLayout {
            gap_auto: true,
            padding: [0.0; 4],
            align_cross: Align::End,
            ..Default::default()
        };
        let (doc, _, k) = doc_with(
            l,
            FX,
            &[(50.0, 20.0, FX), (50.0, 20.0, FX), (50.0, 20.0, FX)],
        );
        let xs: Vec<f64> = k.iter().map(|i| doc.get(*i).unwrap().rect.x).collect();
        assert_eq!(xs, [0.0, 125.0, 250.0]);
        assert_eq!(doc.get(k[0]).unwrap().rect.y, 180.0);
    }

    #[test]
    fn wrap_breaks_lines() {
        let l = AutoLayout {
            wrap: true,
            gap: 10.0,
            cross_gap: 5.0,
            padding: [0.0; 4],
            ..Default::default()
        };
        let (doc, f, k) = doc_with(
            l,
            [Sizing::Fixed, Sizing::Hug],
            &[(120.0, 20.0, FX), (120.0, 30.0, FX), (120.0, 20.0, FX)],
        );
        assert_eq!(doc.get(k[2]).unwrap().rect.min(), v2(0.0, 35.0));
        assert_eq!(doc.get(f).unwrap().rect.h, 30.0 + 5.0 + 20.0);
    }

    #[test]
    fn hidden_and_absolute_children_are_skipped() {
        let mut doc = Document::default();
        let mut rec = Recording::new("t", &doc);
        let mut tx = Tx::new(&mut doc, &mut rec);
        let fid = tx.alloc_id();
        let mut f = Node::new(
            fid,
            NodeKind::Frame { clip: true },
            Rect::new(0.0, 0.0, 10.0, 10.0),
        );
        f.layout = Some(AutoLayout {
            padding: [0.0; 4],
            gap: 5.0,
            ..Default::default()
        });
        f.layout_child.sizing = [Sizing::Hug; 2];
        tx.insert(f, None, None);
        for (vis, abs) in [(true, false), (false, false), (true, true), (true, false)] {
            let id = tx.alloc_id();
            let mut n = Node::new(id, NodeKind::Rect, Rect::new(500.0, 500.0, 10.0, 10.0));
            n.visible = vis;
            n.layout_child.absolute = abs;
            tx.insert(n, Some(fid), None);
        }
        layout_frame(&mut tx, fid);
        assert_eq!(tx.doc().get(fid).unwrap().rect.w, 25.0);
        assert_eq!(
            tx.doc().get(NodeId(4)).unwrap().rect.x,
            500.0,
            "absolute untouched"
        );
    }

    #[test]
    fn constraints_on_resize() {
        let mut doc = Document::default();
        let mut rec = Recording::new("t", &doc);
        let mut tx = Tx::new(&mut doc, &mut rec);
        let fid = tx.alloc_id();
        tx.insert(
            Node::new(
                fid,
                NodeKind::Frame { clip: true },
                Rect::new(0.0, 0.0, 200.0, 100.0),
            ),
            None,
            None,
        );
        let mut ids = Vec::new();
        for c in [
            Constraint::Min,
            Constraint::Max,
            Constraint::Stretch,
            Constraint::Center,
            Constraint::Scale,
        ] {
            let id = tx.alloc_id();
            let mut n = Node::new(id, NodeKind::Rect, Rect::new(20.0, 10.0, 40.0, 20.0));
            n.layout_child.constraints = Constraints { h: c, v: c };
            tx.insert(n, Some(fid), None);
            ids.push(id);
        }
        let old = Rect::new(0.0, 0.0, 200.0, 100.0);
        let new = Rect::new(0.0, 0.0, 400.0, 200.0);
        tx.node_mut(fid).unwrap().rect = new;
        resize_children(&mut tx, fid, old, new);
        let r = |i: usize| tx.doc().get(ids[i]).unwrap().rect;
        assert_eq!(r(0), Rect::new(20.0, 10.0, 40.0, 20.0));
        assert_eq!(r(1), Rect::new(220.0, 110.0, 40.0, 20.0));
        assert_eq!(r(2), Rect::new(20.0, 10.0, 240.0, 120.0));
        assert_eq!(r(3), Rect::new(120.0, 60.0, 40.0, 20.0));
        assert_eq!(r(4), Rect::new(40.0, 20.0, 80.0, 40.0));
    }

    #[test]
    fn wrap_lines_cross_align_center_and_end() {
        let base = AutoLayout {
            wrap: true,
            gap: 10.0,
            cross_gap: 10.0,
            padding: [0.0; 4],
            align_cross: Align::Center,
            ..Default::default()
        };
        // Frame 300x200: line 0 has 20 & 30 (lh=30), line 1 has 20 (lh=20).
        // total_cross = 30 + 10 + 20 = 60, free_cross = 140.
        let (doc_c, _, kc) = doc_with(
            base,
            FX,
            &[(120.0, 20.0, FX), (120.0, 30.0, FX), (120.0, 20.0, FX)],
        );
        assert_eq!(
            doc_c.get(kc[0]).unwrap().rect,
            Rect::new(0.0, 75.0, 120.0, 20.0)
        );
        assert_eq!(
            doc_c.get(kc[1]).unwrap().rect,
            Rect::new(130.0, 70.0, 120.0, 30.0)
        );
        assert_eq!(
            doc_c.get(kc[2]).unwrap().rect,
            Rect::new(0.0, 110.0, 120.0, 20.0)
        );

        let end_l = AutoLayout {
            align_cross: Align::End,
            ..base
        };
        let (doc_e, _, ke) = doc_with(
            end_l,
            FX,
            &[(120.0, 20.0, FX), (120.0, 30.0, FX), (120.0, 20.0, FX)],
        );
        assert_eq!(
            doc_e.get(ke[0]).unwrap().rect,
            Rect::new(0.0, 150.0, 120.0, 20.0)
        );
        assert_eq!(
            doc_e.get(ke[1]).unwrap().rect,
            Rect::new(130.0, 140.0, 120.0, 30.0)
        );
        assert_eq!(
            doc_e.get(ke[2]).unwrap().rect,
            Rect::new(0.0, 180.0, 120.0, 20.0)
        );
    }

    #[test]
    fn wrap_per_line_item_align_and_fill_stretch() {
        let l = AutoLayout {
            wrap: true,
            gap: 10.0,
            cross_gap: 10.0,
            padding: [10.0; 4],
            align_cross: Align::Center,
            ..Default::default()
        };
        // Inner width = 300 - 20 = 280.
        // Line 0: three 80w items (80*3 + 20 = 260 <= 280), heights: Fill, 20, 40 -> lh = 40.
        // Line 1: two 120w items (120*2 + 10 = 250 <= 280), heights: 60, Fill -> lh = 60.
        let (doc, f, k) = doc_with(
            l,
            [Sizing::Fixed, Sizing::Hug],
            &[
                (80.0, 5.0, [Sizing::Fixed, Sizing::Fill]),
                (80.0, 20.0, FX),
                (80.0, 40.0, FX),
                (120.0, 60.0, FX),
                (120.0, 5.0, [Sizing::Fixed, Sizing::Fill]),
            ],
        );
        assert_eq!(doc.get(f).unwrap().rect.h, 10.0 + 40.0 + 10.0 + 60.0 + 10.0);
        assert_eq!(
            doc.get(k[0]).unwrap().rect,
            Rect::new(10.0, 10.0, 80.0, 40.0)
        );
        assert_eq!(
            doc.get(k[1]).unwrap().rect,
            Rect::new(100.0, 20.0, 80.0, 20.0)
        );
        assert_eq!(
            doc.get(k[2]).unwrap().rect,
            Rect::new(190.0, 10.0, 80.0, 40.0)
        );
        assert_eq!(
            doc.get(k[3]).unwrap().rect,
            Rect::new(10.0, 60.0, 120.0, 60.0)
        );
        assert_eq!(
            doc.get(k[4]).unwrap().rect,
            Rect::new(140.0, 60.0, 120.0, 60.0)
        );
    }

    #[test]
    fn layout_grid_columns_fill_and_hug() {
        // 2-column grid with Hug height and one Fill column.
        let mut doc = Document::default();
        let mut rec = Recording::new("t", &doc);
        let mut tx = Tx::new(&mut doc, &mut rec);
        let fid = tx.alloc_id();
        let mut f = Node::new(
            fid,
            NodeKind::Frame { clip: true },
            Rect::new(0.0, 0.0, 300.0, 50.0),
        );
        f.layout = Some(AutoLayout {
            padding: [10.0, 10.0, 10.0, 10.0],
            ..Default::default()
        });
        f.layout_child.sizing = [Sizing::Fixed, Sizing::Hug];
        tx.insert(f, None, None);

        let specs = [
            (80.0, 30.0, FX),
            (20.0, 40.0, [Sizing::Fill, Sizing::Fixed]),
            (100.0, 50.0, FX),
            (20.0, 10.0, [Sizing::Fill, Sizing::Fill]),
        ];
        let mut ids = Vec::new();
        for (w, h, s) in specs {
            let id = tx.alloc_id();
            let mut n = Node::new(id, NodeKind::Rect, Rect::new(0.0, 0.0, w, h));
            n.layout_child.sizing = s;
            tx.insert(n, Some(fid), None);
            ids.push(id);
        }
        assert!(layout_grid(&mut tx, fid, 2, 20.0, 15.0));
        // Col 0 max fixed width = 100. Inner width = 280, col_gap = 20 -> Col 1 Fill width = 160.
        // Row 0 height = max(30, 40) = 40; Row 1 height = max(50, 10) = 50.
        // Hug height = 10 + 40 + 15 + 50 + 10 = 125.
        assert_eq!(tx.doc().get(fid).unwrap().rect.h, 125.0);
        assert_eq!(
            tx.doc().get(ids[0]).unwrap().rect,
            Rect::new(10.0, 10.0, 80.0, 30.0)
        );
        assert_eq!(
            tx.doc().get(ids[1]).unwrap().rect,
            Rect::new(130.0, 10.0, 160.0, 40.0)
        );
        assert_eq!(
            tx.doc().get(ids[2]).unwrap().rect,
            Rect::new(10.0, 65.0, 100.0, 50.0)
        );
        assert_eq!(
            tx.doc().get(ids[3]).unwrap().rect,
            Rect::new(130.0, 65.0, 160.0, 50.0)
        );

        // Re-layout as 3-column grid (col 0 has Fixed 80 & Fill -> Fill; col 1 has Fill -> Fill; col 2 has Fixed 100).
        // Inner width = 280, gaps = 2 * 10 = 20, fixed col 2 = 100 -> remaining 160 shared by col 0 and col 1 (80 each).
        assert!(layout_grid(&mut tx, fid, 3, 10.0, 10.0));
        // Row 0: ids[0](30), ids[1](40), ids[2](50) -> row_h[0] = 50.
        // Row 1: ids[3](50 from previous stretch) -> row_h[1] = 50.
        assert_eq!(
            tx.doc().get(fid).unwrap().rect.h,
            10.0 + 50.0 + 10.0 + 50.0 + 10.0
        );
        assert_eq!(
            tx.doc().get(ids[0]).unwrap().rect,
            Rect::new(10.0, 10.0, 80.0, 30.0)
        );
        assert_eq!(
            tx.doc().get(ids[1]).unwrap().rect,
            Rect::new(100.0, 10.0, 80.0, 40.0)
        );
        assert_eq!(
            tx.doc().get(ids[2]).unwrap().rect,
            Rect::new(190.0, 10.0, 100.0, 50.0)
        );
        assert_eq!(
            tx.doc().get(ids[3]).unwrap().rect,
            Rect::new(10.0, 70.0, 80.0, 50.0)
        );
    }
}
