//! `get_bounds`: the rects of a whole subtree in one read, so an agent can check layout (overflow,
//! alignment, overlap) without one `get_node_info` call per layer.

use crate::model::{Document, NodeId, NodeKind};
use serde_json::{Value, json};

/// Rows returned per call; the rest is reported as `truncated` with where to continue.
pub const MAX_ROWS: usize = 2000;

/// Column names of a row, in order.
pub const COLUMNS: [&str; 9] = ["id", "parent", "x", "y", "w", "h", "absX", "absY", "clip"];

/// Rows for the subtrees under `roots`, depth first (parents before children).
pub struct Collected {
    pub rows: Vec<Value>,
    /// Layers not reached because of the row cap, in the order they would have come.
    pub pending: Vec<NodeId>,
}

/// One decimal is far finer than a pixel and keeps rows short.
fn tenth(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

/// Whether the layer clips what is drawn outside it (a clipping frame, or a scrolling one).
fn clips(doc: &Document, id: NodeId) -> bool {
    doc.get(id)
        .is_some_and(|n| n.scroll || matches!(n.kind, NodeKind::Frame { clip: true }))
}

/// Walk `roots` and emit at most `cap` rows. `depth` counts the roots as level 1 (`None` = all
/// levels). `key` turns a node id into the id string tools accept.
pub fn collect(doc: &Document, roots: &[NodeId], depth: Option<usize>, cap: usize, key: &dyn Fn(NodeId) -> String) -> Collected {
    let mut stack: Vec<(NodeId, usize)> = roots.iter().rev().map(|r| (*r, 1)).collect();
    let mut rows = Vec::new();
    while let Some((id, level)) = stack.pop() {
        let (Some(n), Some(w)) = (doc.get(id), doc.world_rect(id)) else { continue };
        if rows.len() >= cap {
            stack.push((id, level));
            break;
        }
        let parent = n.parent.map_or(Value::Null, |p| json!(key(p)));
        rows.push(json!([
            key(id),
            parent,
            tenth(n.rect.x),
            tenth(n.rect.y),
            tenth(n.rect.w),
            tenth(n.rect.h),
            tenth(w.x),
            tenth(w.y),
            clips(doc, id)
        ]));
        if depth.is_none_or(|d| level < d) {
            stack.extend(n.children.iter().rev().map(|c| (*c, level + 1)));
        }
    }
    Collected { rows, pending: stack.into_iter().rev().map(|(id, _)| id).collect() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::Rect;
    use crate::model::Node;

    fn add(doc: &mut Document, parent: Option<NodeId>, kind: NodeKind, r: Rect) -> NodeId {
        let id = doc.alloc_id();
        let mut n = Node::new(id, kind, r);
        n.parent = parent;
        doc.nodes.insert(id, n);
        match parent {
            Some(p) => doc.nodes.get_mut(&p).unwrap().children.push(id),
            None => doc.root.push(id),
        }
        id
    }

    fn tree() -> (Document, NodeId, NodeId, NodeId) {
        let mut d = Document::default();
        let board = add(&mut d, None, NodeKind::Frame { clip: true }, Rect::new(100.0, 50.0, 300.0, 200.0));
        let card = add(&mut d, Some(board), NodeKind::Frame { clip: false }, Rect::new(10.0, 20.0, 100.0, 80.0));
        let dot = add(&mut d, Some(card), NodeKind::Rect, Rect::new(5.04, 6.0, 8.0, 8.0));
        (d, board, card, dot)
    }

    fn key(id: NodeId) -> String {
        format!("p:{}", id.0)
    }

    #[test]
    fn rows_carry_local_and_world_positions_in_paint_order() {
        let (d, board, card, dot) = tree();
        let c = collect(&d, &[board], None, MAX_ROWS, &key);
        assert_eq!(c.rows.len(), 3);
        assert!(c.pending.is_empty());
        assert_eq!(c.rows[0], json!([key(board), null, 100.0, 50.0, 300.0, 200.0, 100.0, 50.0, true]));
        assert_eq!(c.rows[1], json!([key(card), key(board), 10.0, 20.0, 100.0, 80.0, 110.0, 70.0, false]));
        assert_eq!(c.rows[2][0], json!(key(dot)));
        assert_eq!(c.rows[2][2], json!(5.0), "rounded to a tenth");
        assert_eq!(c.rows[2][6], json!(115.0));
    }

    #[test]
    fn depth_counts_the_roots_as_level_one() {
        let (d, board, ..) = tree();
        assert_eq!(collect(&d, &[board], Some(1), MAX_ROWS, &key).rows.len(), 1);
        assert_eq!(collect(&d, &[board], Some(2), MAX_ROWS, &key).rows.len(), 2);
    }

    #[test]
    fn the_cap_stops_the_walk_and_names_what_is_left() {
        let (d, board, card, dot) = tree();
        let c = collect(&d, &[board], None, 1, &key);
        assert_eq!(c.rows.len(), 1);
        assert_eq!(c.pending, vec![card]);
        let rest = collect(&d, &c.pending, None, 1, &key);
        assert_eq!(rest.pending, vec![dot]);
    }

    #[test]
    fn a_scrolling_frame_counts_as_clipping() {
        let (mut d, _, card, _) = tree();
        d.nodes.get_mut(&card).unwrap().scroll = true;
        assert!(clips(&d, card));
    }
}
