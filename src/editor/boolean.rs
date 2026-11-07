//! Live boolean groups: the operands stay as children and the group's shape is their
//! combination, recomputed when an operand changes.

use super::vector_ops::{Contours, boolean, contour_bounds, leaf_contours, shapes_to_contours};
use crate::geom::{Rect, V2, v2};
use crate::history::Tx;
use crate::model::rotation::{Rot, apply_chain};
use crate::model::{BoolOp, Document, Node, NodeId, NodeKind, PathPoint, SubPath};
use i_overlay::core::fill_rule::FillRule as OvFill;
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::float::single::SingleFloatOverlay;
use std::hash::{DefaultHasher, Hash, Hasher};

/// Rotations from `top` (exclusive) down to `id` (inclusive): the turns that act on an operand
/// inside the group's own unrotated frame. The group's turn and those above it are applied
/// when the group itself is drawn.
fn chain_below(doc: &Document, id: NodeId, top: NodeId) -> Vec<Rot> {
    let mut ids = doc.ancestors(id);
    ids.reverse();
    let mut below: Vec<NodeId> = ids.into_iter().skip_while(|a| *a != top).skip(1).collect();
    below.push(id);
    below
        .into_iter()
        .filter_map(|a| {
            let deg = doc.get(a)?.style.rotation;
            let centre = doc.world_rect(a)?.center();
            (deg != 0.0).then_some(Rot { centre, deg })
        })
        .collect()
}

/// The area `id` adds to boolean group `top`: shapes by their outline, containers by the union of
/// their parts, all turned by the rotations between them and the group. Hidden layers add nothing.
fn operand_contours(doc: &Document, id: NodeId, top: NodeId) -> Contours {
    let Some(n) = doc.get(id) else { return vec![] };
    if !n.visible {
        return vec![];
    }
    if matches!(n.kind, NodeKind::Group | NodeKind::Frame { .. }) {
        let mut acc: Contours = Vec::new();
        for c in &n.children {
            let cc = operand_contours(doc, *c, top);
            acc = if acc.is_empty() {
                cc
            } else {
                shapes_to_contours(acc.overlay(&cc, OverlayRule::Union, OvFill::NonZero))
            };
        }
        return acc;
    }
    let chain = chain_below(doc, id, top);
    let mut c = leaf_contours(doc, id);
    if !chain.is_empty() {
        for p in c.iter_mut().flatten() {
            let q = apply_chain(&chain, v2(p[0], p[1]));
            *p = [q.x, q.y];
        }
    }
    c
}

/// The group's operands, bottom first, as polygons in its unrotated world frame.
fn operands(doc: &Document, group: NodeId) -> Vec<Contours> {
    let Some(g) = doc.get(group) else {
        return vec![];
    };
    g.children
        .iter()
        .map(|c| operand_contours(doc, *c, group))
        .filter(|c| !c.is_empty())
        .collect()
}

/// Operand points are compared to a millionth of a unit.
const SIG_SCALE: f64 = 1e6;

/// What the result depends on, relative to the group's origin so that moving the whole group
/// does not count as a change.
fn signature(op: BoolOp, operands: &[Contours], origin: V2) -> u64 {
    let mut h = DefaultHasher::new();
    op.name().hash(&mut h);
    for o in operands {
        o.len().hash(&mut h);
        for ring in o {
            ring.len().hash(&mut h);
            for p in ring {
                // Rounded: the refit shifts operands by float noise that must not look like an edit.
                ((p[0] - origin.x) * SIG_SCALE)
                    .round()
                    .to_bits()
                    .hash(&mut h);
                ((p[1] - origin.y) * SIG_SCALE)
                    .round()
                    .to_bits()
                    .hash(&mut h);
            }
        }
    }
    h.finish()
}

/// Brings boolean group `id` up to date: recompute its shape when an operand (or the op)
/// changed since the last time, then fit its box to the result, keeping the operands where
/// they are on the canvas. An empty group is removed. Returns whether anything was touched.
pub fn refresh(tx: &mut Tx, id: NodeId) -> bool {
    let Some(n) = tx.doc().get(id) else {
        return false;
    };
    let NodeKind::Boolean { op, sig, .. } = &n.kind else {
        return false;
    };
    if n.children.is_empty() {
        tx.remove(id);
        return true;
    }
    let (op, sig, kids) = (*op, *sig, n.children.clone());
    let Some(wr) = tx.doc().world_rect(id) else {
        return false;
    };
    let ops = operands(tx.doc(), id);
    let now = signature(op, &ops, wr.min());
    if now == sig {
        return false;
    }
    let result = boolean(op, &ops);
    // An empty result (operands that do not meet) keeps the box where it was.
    let bounds = contour_bounds(&result).unwrap_or(wr);
    let d = bounds.min() - wr.min();
    for k in kids {
        if let Some(c) = tx.node_mut(k) {
            c.rect = c.rect.translate(V2::ZERO - d);
        }
    }
    let subpaths: Vec<SubPath> = result
        .iter()
        .map(|ring| SubPath {
            points: ring
                .iter()
                .map(|p| PathPoint::corner(v2(p[0], p[1]) - bounds.min()))
                .collect(),
            closed: true,
        })
        .collect();
    if let Some(g) = tx.node_mut(id) {
        g.rect = Rect::new(g.rect.x + d.x, g.rect.y + d.y, bounds.w, bounds.h);
        g.kind = NodeKind::Boolean {
            op,
            subpaths,
            sig: now,
        };
    }
    true
}

impl super::Editor {
    /// Combine the selection into a live boolean group: the layers stay editable inside it.
    pub fn boolean(&mut self, op: BoolOp) {
        let mut ids = self.top_selection();
        if ids.len() < 2 {
            self.status = "Select two or more layers".into();
            return;
        }
        if let Some(bad) = ids.iter().find(|id| !self.is_operand(**id)) {
            let what = self.doc.get(*bad).map_or("layer", |n| n.kind.type_name());
            self.status = format!("A {what} has no area to combine");
            return;
        }
        // Paint order, bottom first: the bottom layer is what Subtract cuts from.
        let order = self.doc.paint_order();
        ids.sort_by_key(|id| order.iter().position(|x| x == id));
        let (bottom, top) = (ids[0], ids[ids.len() - 1]);
        let style_from = if op == BoolOp::Subtract { bottom } else { top };
        let parent = self.doc.get(top).and_then(|n| n.parent);
        let index = self
            .doc
            .children(parent)
            .iter()
            .position(|x| *x == top)
            .map(|i| i + 1);
        let mut style = self
            .doc
            .get(style_from)
            .map(|n| n.style.clone())
            .unwrap_or_default();
        style.rough = None;
        style.rotation = 0.0;
        let rects: Vec<Rect> = ids
            .iter()
            .filter_map(|id| self.doc.world_bounds(*id))
            .collect();
        let Some(bounds) = Rect::union_all(&rects) else {
            return;
        };
        let gid = self.edit(op.label(), |tx| {
            let gid = tx.alloc_id();
            let o = tx.doc().parent_origin(parent);
            let kind = NodeKind::Boolean {
                op,
                subpaths: Vec::new(),
                sig: 0,
            };
            let mut g = Node::new(gid, kind, bounds.translate(V2::ZERO - o));
            g.style = style;
            g.name = op.label().into();
            tx.insert(g, parent, index);
            for id in &ids {
                tx.reparent(*id, Some(gid), None);
            }
            gid
        });
        self.selection = vec![gid];
    }

    /// Whether `id` has an outline a boolean can use (not a line or an arrow).
    fn is_operand(&self, id: NodeId) -> bool {
        self.doc
            .get(id)
            .is_some_and(|n| !matches!(n.kind, NodeKind::Line { .. } | NodeKind::Connector { .. }))
    }

    /// Change how the selected boolean groups combine their operands.
    pub fn set_bool_op(&mut self, op: BoolOp) {
        let ids: Vec<NodeId> = self
            .top_selection()
            .into_iter()
            .filter(|id| {
                self.doc
                    .get(*id)
                    .is_some_and(|n| matches!(n.kind, NodeKind::Boolean { op: o, .. } if o != op))
            })
            .collect();
        if ids.is_empty() {
            return;
        }
        self.edit(op.label(), |tx| {
            for id in ids {
                let Some(n) = tx.node_mut(id) else { continue };
                if let NodeKind::Boolean { op: cur, .. } = &mut n.kind {
                    // A name the user chose stays; the default one follows the operation.
                    if n.name == cur.label() {
                        n.name = op.label().into();
                    }
                    *cur = op;
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::{Button, Editor, InputEvent};

    fn add(e: &mut Editor, x: f64, y: f64, w: f64, h: f64) -> NodeId {
        e.add_node(
            Node::new(NodeId(0), NodeKind::Rect, Rect::new(x, y, w, h)),
            None,
        )
    }

    fn group_of(e: &Editor, id: NodeId) -> (BoolOp, f64) {
        let NodeKind::Boolean { op, subpaths, .. } = &e.doc.get(id).unwrap().kind else {
            panic!("not a boolean group");
        };
        let area: f64 = subpaths
            .iter()
            .map(|s| {
                let n = s.points.len();
                (0..n)
                    .map(|i| {
                        let (p, q) = (s.points[i].p, s.points[(i + 1) % n].p);
                        p.x * q.y - q.x * p.y
                    })
                    .sum::<f64>()
                    * 0.5
            })
            .sum();
        (*op, area.abs())
    }

    /// Two overlapping 100 squares, the second offset by 50, combined with `op`.
    fn pair(op: BoolOp) -> (Editor, NodeId, NodeId, NodeId) {
        let mut e = Editor::default();
        let a = add(&mut e, 0.0, 0.0, 100.0, 100.0);
        let b = add(&mut e, 50.0, 50.0, 100.0, 100.0);
        e.select(vec![a, b]);
        e.boolean(op);
        let g = e.selection[0];
        (e, g, a, b)
    }

    #[test]
    fn subtract_keeps_the_operands_and_the_bottom_layer_as_minuend() {
        let (e, g, a, b) = pair(BoolOp::Subtract);
        assert_eq!(e.doc.get(g).unwrap().children, vec![a, b]);
        assert!((group_of(&e, g).1 - 7500.0).abs() < 1e-3);
        assert_eq!(
            e.doc.get(g).unwrap().rect,
            Rect::new(0.0, 0.0, 100.0, 100.0)
        );
    }

    #[test]
    fn each_op_is_one_undo_step_and_changing_it_recomputes() {
        let (mut e, g, ..) = pair(BoolOp::Union);
        assert!((group_of(&e, g).1 - 17500.0).abs() < 1e-3);
        e.set_bool_op(BoolOp::Intersect);
        assert_eq!(group_of(&e, g).0, BoolOp::Intersect);
        assert!((group_of(&e, g).1 - 2500.0).abs() < 1e-3);
        e.undo();
        assert_eq!(group_of(&e, g).0, BoolOp::Union);
        e.undo();
        assert!(e.doc.get(g).is_none(), "one undo removes the whole group");
        assert_eq!(e.doc.children(None).len(), 2);
    }

    #[test]
    fn moving_or_resizing_an_operand_updates_the_result() {
        let (mut e, g, _, b) = pair(BoolOp::Subtract);
        e.edit("Move", |tx| tx.node_mut(b).unwrap().rect.x += 200.0);
        assert!(
            (group_of(&e, g).1 - 10000.0).abs() < 1e-3,
            "no overlap, nothing cut"
        );
        e.edit("Resize", |tx| {
            tx.node_mut(b).unwrap().rect = Rect::new(0.0, 0.0, 50.0, 100.0)
        });
        assert!((group_of(&e, g).1 - 5000.0).abs() < 1e-3);
        e.undo();
        assert!((group_of(&e, g).1 - 10000.0).abs() < 1e-3);
    }

    #[test]
    fn the_result_is_cached_until_an_operand_changes() {
        let (mut e, g, a, _) = pair(BoolOp::Union);
        let sig = |e: &Editor| match e.doc.get(g).unwrap().kind {
            NodeKind::Boolean { sig, .. } => sig,
            _ => unreachable!(),
        };
        let before = sig(&e);
        e.edit("Rename", |tx| tx.node_mut(a).unwrap().name = "A".into());
        assert_eq!(sig(&e), before, "a rename is not an edit of the shape");
        e.edit("Move group", |tx| tx.node_mut(g).unwrap().rect.x += 30.0);
        assert_eq!(sig(&e), before, "moving the whole group keeps the result");
        e.edit("Grow", |tx| tx.node_mut(a).unwrap().rect.w += 10.0);
        assert_ne!(sig(&e), before);
    }

    #[test]
    fn flatten_bakes_the_group_into_a_path_in_one_step() {
        let (mut e, g, a, _) = pair(BoolOp::Subtract);
        e.flatten_selection();
        let n = e.doc.get(g).unwrap();
        assert!(matches!(n.kind, NodeKind::Path { .. }) && n.children.is_empty());
        assert!(e.doc.get(a).is_none());
        e.undo();
        assert!(e.doc.get(g).unwrap().is_boolean() && e.doc.get(g).unwrap().children.len() == 2);
    }

    #[test]
    fn hit_testing_uses_the_result_shape() {
        let (e, g, ..) = pair(BoolOp::Subtract);
        let zoom = 1.0;
        assert_eq!(
            crate::editor::hit::hit_test(&e.doc, v2(20.0, 20.0), zoom),
            Some(g)
        );
        assert_eq!(
            crate::editor::hit::hit_test(&e.doc, v2(75.0, 75.0), zoom),
            None,
            "the cut-out part"
        );
    }

    #[test]
    fn a_rotated_operand_counts_with_its_turn() {
        let mut e = Editor::default();
        let bar = add(&mut e, 0.0, 0.0, 100.0, 20.0);
        let flat = add(&mut e, 0.0, 0.0, 100.0, 10.0);
        e.edit("Turn", |tx| tx.node_mut(bar).unwrap().style.rotation = 90.0);
        e.select(vec![bar, flat]);
        e.boolean(BoolOp::Union);
        let g = e.selection[0];
        // A 20x100 upright bar over a 100x10 strip that overlaps it in 20x10.
        assert!(
            (group_of(&e, g).1 - 2800.0).abs() < 1e-3,
            "{}",
            group_of(&e, g).1
        );
    }

    #[test]
    fn a_boolean_group_can_be_an_operand() {
        let (mut e, g, ..) = pair(BoolOp::Union);
        let cut = add(&mut e, 0.0, 0.0, 50.0, 50.0);
        e.select(vec![g, cut]);
        e.boolean(BoolOp::Subtract);
        let outer = e.selection[0];
        assert!((group_of(&e, outer).1 - 15000.0).abs() < 1e-3);
        e.edit("Move", |tx| tx.node_mut(cut).unwrap().rect.x += 500.0);
        assert!(
            (group_of(&e, outer).1 - 17500.0).abs() < 1e-3,
            "the inner result flows outward"
        );
    }

    #[test]
    fn lines_and_single_layers_are_refused() {
        let mut e = Editor::default();
        let a = add(&mut e, 0.0, 0.0, 10.0, 10.0);
        e.select(vec![a]);
        e.boolean(BoolOp::Union);
        assert!(!e.doc.get(a).unwrap().is_group());
    }

    #[test]
    fn double_click_enters_a_selected_group_and_picks_the_operand() {
        let (mut e, g, a, _) = pair(BoolOp::Union);
        let click = |e: &mut Editor, t: f64| {
            for ev in [
                InputEvent::PointerDown {
                    pos: v2(20.0, 20.0),
                    button: Button::Primary,
                    pressure: None,
                    time_ms: t,
                },
                InputEvent::PointerUp {
                    pos: v2(20.0, 20.0),
                    button: Button::Primary,
                    time_ms: t + 20.0,
                },
            ] {
                e.handle(ev);
            }
        };
        e.selection = vec![g];
        click(&mut e, 1000.0);
        click(&mut e, 1100.0);
        assert_eq!(e.selection, vec![a]);
    }
}
