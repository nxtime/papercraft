//! Node rotation (CSS `transform: rotate()`): visual only, so `rect` and layout stay unrotated and
//! a point is mapped through the chain of rotations from the page down to a node.

use super::document::Document;
use super::node::NodeId;
use crate::geom::{Rect, V2, bounds_of, rotate_about};

/// One rotation: degrees clockwise around a world point (the node's unrotated centre).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rot {
    pub centre: V2,
    pub deg: f64,
}

/// Where `p` ends up after `chain` (listed from the page down): the innermost turn goes first.
pub fn apply_chain(chain: &[Rot], p: V2) -> V2 {
    chain.iter().rev().fold(p, |p, r| rotate_about(p, r.centre, r.deg))
}

impl Document {
    /// Rotations that apply to `id`, page first: its ancestors', then its own when `own`.
    pub fn rotation_chain(&self, id: NodeId, own: bool) -> Vec<Rot> {
        let mut ids = self.ancestors(id);
        ids.reverse();
        if own {
            ids.push(id);
        }
        ids.into_iter()
            .filter_map(|a| {
                let deg = self.get(a)?.style.rotation;
                let centre = self.world_rect(a)?.center();
                (deg != 0.0).then_some(Rot { centre, deg })
            })
            .collect()
    }

    /// A point on the canvas in the unrotated frame of `id`'s rect (its own turn included when `own`).
    pub fn to_unrotated(&self, id: NodeId, p: V2, own: bool) -> V2 {
        self.rotation_chain(id, own)
            .iter()
            .fold(p, |p, r| rotate_about(p, r.centre, -r.deg))
    }

    /// Corners of the node as drawn (top-left, top-right, bottom-right, bottom-left).
    pub fn world_outline(&self, id: NodeId) -> Option<[V2; 4]> {
        let r = self.world_rect(id)?;
        let chain = self.rotation_chain(id, true);
        Some(r.corners().map(|p| apply_chain(&chain, p)))
    }

    /// Bounding box of the node as drawn; its `world_rect` when nothing above it is rotated.
    pub fn world_bounds(&self, id: NodeId) -> Option<Rect> {
        let r = self.world_rect(id)?;
        if self.rotation_chain(id, true).is_empty() {
            return Some(r);
        }
        bounds_of(&self.world_outline(id)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Node, NodeKind};

    fn doc_with_rotated_child() -> (Document, NodeId, NodeId) {
        let mut d = Document::default();
        let (f, c) = (d.alloc_id(), d.alloc_id());
        let mut frame = Node::new(f, NodeKind::Frame { clip: false }, Rect::new(100.0, 0.0, 200.0, 200.0));
        frame.children = vec![c];
        let mut child = Node::new(c, NodeKind::Rect, Rect::new(0.0, 0.0, 100.0, 20.0));
        child.parent = Some(f);
        d.nodes.insert(f, frame);
        d.nodes.insert(c, child);
        d.root = vec![f];
        (d, f, c)
    }

    #[test]
    fn a_quarter_turn_swaps_the_bounds_around_the_centre() {
        let (mut d, _, c) = doc_with_rotated_child();
        d.nodes.get_mut(&c).unwrap().style.rotation = 90.0;
        let b = d.world_bounds(c).unwrap();
        // Centre (150, 10) stays; 100x20 becomes 20x100.
        assert!((b.x - 140.0).abs() < 1e-9 && (b.y + 40.0).abs() < 1e-9);
        assert!((b.w - 20.0).abs() < 1e-9 && (b.h - 100.0).abs() < 1e-9);
    }

    #[test]
    fn a_parent_turn_carries_the_child_and_the_hit_maps_back() {
        let (mut d, f, c) = doc_with_rotated_child();
        d.nodes.get_mut(&f).unwrap().style.rotation = 90.0;
        // Frame centre (200, 100); the child's unrotated centre (150, 10) lands at (290, 50).
        let b = d.world_bounds(c).unwrap().center();
        assert!((b.x - 290.0).abs() < 1e-9 && (b.y - 50.0).abs() < 1e-9);
        let back = d.to_unrotated(c, b, false);
        assert!((back.x - 150.0).abs() < 1e-9 && (back.y - 10.0).abs() < 1e-9);
    }
}
