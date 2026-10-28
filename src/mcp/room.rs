//! Keeps top-level frames clear of each other after an edit: a board that grew (a hugging section
//! that gained rows, an artboard made taller) pushes the frames it now runs into down, keeping
//! the gap they had, and those push the ones under them. Frames that already overlapped before
//! the edit are left alone: that overlap was there on purpose or is not this edit's doing.

use crate::geom::Rect;
use crate::model::{Document, NodeId};
use std::collections::HashMap;

/// Where each visible top-level frame is.
pub fn snapshot(doc: &Document) -> HashMap<NodeId, Rect> {
    doc.root.iter().filter_map(|id| doc.get(*id).filter(|n| n.visible && n.is_frame()).map(|n| (*id, n.rect))).collect()
}

/// The moves (frame, dy) that clear the overlaps this edit made, given the frames before it.
pub fn moves(doc: &Document, before: &HashMap<NodeId, Rect>) -> Vec<(NodeId, f64)> {
    let mut now = snapshot(doc);
    let mut moved: HashMap<NodeId, f64> = HashMap::new();
    // A cascade moves each frame at most once per frame above it; this bounds a bad loop.
    for _ in 0..now.len() * 4 + 4 {
        let mut ids: Vec<NodeId> = now.keys().copied().collect();
        ids.sort_by(|a, b| now[a].y.total_cmp(&now[b].y).then(a.0.cmp(&b.0)));
        // Only frames that grew (or were pushed) push: one placed on purpose moves nothing.
        let grew = |id: &NodeId| moved.contains_key(id) || before.get(id).is_some_and(|p| now[id].h > p.h || now[id].w > p.w);
        let hit = ids.iter().filter(|a| grew(a)).find_map(|a| {
            let ra = now[a];
            ids.iter().filter(|b| *b != a && now[*b].y >= ra.y).find_map(|b| {
                let rb = now[b];
                let was = match (before.get(a), before.get(b)) {
                    (Some(pa), Some(pb)) => pa.intersects(pb),
                    _ => false,
                };
                if !ra.intersects(&rb) || was {
                    return None;
                }
                // The gap they had, else the usual distance between boards.
                let gap = match (before.get(a), before.get(b)) {
                    (Some(pa), Some(pb)) if pb.y >= pa.bottom() => pb.y - pa.bottom(),
                    _ => BOARD_GAP,
                };
                Some((*b, ra.bottom() + gap - rb.y))
            })
        });
        let Some((id, dy)) = hit else { break };
        now.get_mut(&id).unwrap().y += dy;
        *moved.entry(id).or_default() += dy;
    }
    let mut out: Vec<(NodeId, f64)> = moved.into_iter().filter(|(_, dy)| *dy > 0.0).collect();
    out.sort_by_key(|(id, _)| id.0);
    out
}

/// Room left between boards pushed apart when they had no gap to keep.
const BOARD_GAP: f64 = 80.0;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Node, NodeKind};

    fn board(doc: &mut Document, r: Rect) -> NodeId {
        let id = doc.alloc_id();
        doc.nodes.insert(id, Node::new(id, NodeKind::Frame { clip: true }, r));
        doc.root.push(id);
        id
    }

    #[test]
    fn a_board_that_grew_pushes_the_stack_under_it_keeping_the_gaps() {
        let mut doc = Document::default();
        let a = board(&mut doc, Rect::new(0.0, 0.0, 400.0, 300.0));
        let b = board(&mut doc, Rect::new(0.0, 400.0, 400.0, 300.0));
        let c = board(&mut doc, Rect::new(0.0, 760.0, 400.0, 100.0));
        let side = board(&mut doc, Rect::new(600.0, 350.0, 200.0, 100.0));
        let before = snapshot(&doc);
        doc.nodes.get_mut(&a).unwrap().rect.h = 500.0; // now runs 100 into b
        let m = moves(&doc, &before);
        assert_eq!(m, vec![(b, 200.0), (c, 200.0)], "b keeps its 100 gap, c its 60: {m:?}");
        assert!(!m.iter().any(|(id, _)| *id == side), "a frame beside it stays");
    }

    #[test]
    fn frames_that_already_overlapped_stay() {
        let mut doc = Document::default();
        let a = board(&mut doc, Rect::new(0.0, 0.0, 400.0, 300.0));
        board(&mut doc, Rect::new(100.0, 100.0, 100.0, 100.0));
        let before = snapshot(&doc);
        doc.nodes.get_mut(&a).unwrap().rect.h = 320.0;
        assert!(moves(&doc, &before).is_empty());
    }
}
