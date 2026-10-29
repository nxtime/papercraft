//! Scrolling screens unrolled for print: paper cannot scroll, so each frame whose content
//! scrolls grows to show all of it and its screen grows with it, like a full-page capture.
//! Parts pinned to the bottom (a tab bar) follow the new bottom; what sits below a grown
//! screen on the canvas moves down so it stays clear of it.

use crate::editor::{Editor, scroll_range};
use crate::layout::resize_children;
use crate::model::{Constraint, Document, NodeId};

/// How close to the bottom edge a pinned bar sits.
const FLUSH: f64 = 1.0;

/// `doc` with the `screens` unrolled; None when none of them scrolls.
pub(crate) fn unrolled(doc: &Document, screens: &[NodeId]) -> Option<Document> {
    let scrolls = |doc: &Document, s: NodeId| -> Vec<NodeId> {
        let mut ids = doc.subtree(s);
        // Innermost first: unrolling one grows its ancestors, not what is inside it.
        ids.reverse();
        ids.retain(|id| doc.get(*id).is_some_and(|n| n.scroll && n.visible) && scroll_range(doc, *id) > 0.0);
        ids
    };
    if screens.iter().all(|s| scrolls(doc, *s).is_empty()) {
        return None;
    }
    let mut ed = Editor::default();
    ed.doc = doc.clone();
    let mut order = screens.to_vec();
    order.sort_by(|a, b| doc.get(*a).map(|n| n.rect.y).partial_cmp(&doc.get(*b).map(|n| n.rect.y)).unwrap_or(std::cmp::Ordering::Equal));
    ed.edit("Unroll scrolling screens", |tx| {
        for s in order {
            let Some(before) = tx.doc().world_rect(s) else { continue };
            for f in scrolls(tx.doc(), s) {
                let grow = scroll_range(tx.doc(), f);
                let Some(n) = tx.node_mut(f) else { continue };
                n.scroll = false;
                n.scroll_y = 0.0;
                let mut cur = Some(f);
                while let Some(id) = cur {
                    // A bar flush with the bottom edge is meant to stay there, however it is
                    // positioned (`top: 756px` on an 844 px screen).
                    let h = tx.doc().get(id).map_or(0.0, |n| n.rect.h);
                    let pinned: Vec<NodeId> = tx
                        .doc()
                        .get(id)
                        .map(|n| n.children.clone())
                        .unwrap_or_default()
                        .into_iter()
                        .filter(|k| tx.doc().get(*k).is_some_and(|c| c.layout_child.absolute && (c.rect.bottom() - h).abs() < FLUSH))
                        .collect();
                    for k in pinned {
                        if let Some(c) = tx.node_mut(k) {
                            c.layout_child.constraints.v = Constraint::Max;
                        }
                    }
                    let Some(n) = tx.node_mut(id) else { break };
                    let old = n.rect;
                    n.rect.h += grow;
                    let (new, parent) = (n.rect, n.parent);
                    resize_children(tx, id, old, new);
                    cur = parent.filter(|_| id != s);
                }
            }
            let Some(after) = tx.doc().world_rect(s) else { continue };
            let grew = after.h - before.h;
            let below: Vec<NodeId> = tx
                .doc()
                .root
                .iter()
                .copied()
                .filter(|id| *id != s && tx.doc().get(*id).is_some_and(|n| n.rect.y >= before.bottom() && n.rect.x < before.right() && n.rect.right() > before.x))
                .collect();
            for id in below {
                if let Some(n) = tx.node_mut(id) {
                    n.rect.y += grew;
                }
            }
        }
    });
    Some(ed.doc)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::Rect;
    use crate::model::{Node, NodeKind};

    #[test]
    fn a_scrolling_screen_shows_all_its_content_with_its_tab_bar_at_the_bottom() {
        let mut ed = Editor::default();
        let frame = |r: Rect| Node::new(NodeId(0), NodeKind::Frame { clip: true }, r);
        let screen = ed.add_node(frame(Rect::new(0.0, 0.0, 390.0, 844.0)), None);
        ed.doc.nodes.get_mut(&screen).unwrap().scroll = true;
        ed.add_node(frame(Rect::new(0.0, 0.0, 390.0, 1500.0)), Some(screen));
        let mut bar = frame(Rect::new(0.0, 764.0, 390.0, 80.0));
        bar.layout_child.absolute = true;
        let bar = ed.add_node(bar, Some(screen));
        let below = ed.add_node(frame(Rect::new(0.0, 1484.0, 390.0, 844.0)), None);
        let beside = ed.add_node(frame(Rect::new(1000.0, 1484.0, 390.0, 844.0)), None);
        assert!(unrolled(&ed.doc, &[below]).is_none(), "nothing scrolls");
        let doc = unrolled(&ed.doc, &[screen, below]).unwrap();
        let rect = |id| doc.get(id).unwrap().rect;
        assert_eq!(rect(screen).h, 1500.0);
        assert!(!doc.get(screen).unwrap().scroll);
        assert_eq!(rect(bar).bottom(), 1500.0, "pinned to the new bottom");
        assert_eq!(rect(below).y, 1484.0 + 656.0, "moved clear of the grown screen");
        assert_eq!(rect(beside).y, 1484.0, "not under it");
    }
}
