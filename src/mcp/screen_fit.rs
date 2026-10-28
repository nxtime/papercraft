//! Screens keep their device height. When what `write_html` put in a fixed-height screen is
//! taller than the screen, the part that overflows scrolls instead, as an app does: the main
//! column under a fixed sidebar and header, the content between a header and a tab bar. The
//! screen itself scrolls only when nothing inside can.

use crate::history::Tx;
use crate::layout::{flow_children, hug_size};
use crate::model::{Direction, Document, Node, NodeId, NodeKind, Sizing};

/// Content past the bottom by up to this much is rounding noise.
const TOLERANCE: f64 = 0.5;

/// The device height of a screen this wide, if it is a device size (`screens::DEVICES`).
pub fn device_height(width: f64) -> Option<f64> {
    crate::editor::screens::DEVICES.iter().find(|d| (d.1 - width).abs() < TOLERANCE).map(|d| d.2)
}

/// Make an overflowing fixed-height screen scroll where an app would and return the names of
/// the layers that now scroll. `relayout` lays the screen out again after a size change;
/// `stretches` tells whether a row stretches its children (CSS `align-items` left alone).
pub fn fit(tx: &mut Tx, screen: NodeId, relayout: fn(&mut Tx, Option<NodeId>), stretches: &dyn Fn(&Node) -> bool) -> Vec<String> {
    let doc = tx.doc();
    let Some(n) = doc.get(screen) else { return Vec::new() };
    if n.parent.is_some() || !n.is_frame() || n.layout.is_none() || n.layout_child.sizing[1] == Sizing::Hug {
        return Vec::new();
    }
    if !overflows(doc, screen) {
        return Vec::new();
    }
    // An app shell (a sidebar beside the main column) that hugs its content takes the screen's
    // height, so the sidebar stays and the column overflows inside it.
    let column = n.layout.is_some_and(|l| l.direction == Direction::Vertical);
    let shells: Vec<NodeId> = flow_children(doc, screen)
        .into_iter()
        .filter(|k| {
            doc.get(*k).is_some_and(|c| {
                c.instance.is_none()
                    && c.layout_child.sizing[1] == Sizing::Hug
                    && c.layout.is_some_and(|l| l.direction == Direction::Horizontal && !l.wrap)
                    && flow_children(doc, *k).len() >= 2
            }) && overflows_by_content(doc, *k, doc.get(screen).map_or(0.0, |s| s.rect.h))
        })
        .collect();
    if column && let [shell] = shells.as_slice() {
        let kids: Vec<NodeId> = if tx.doc().get(*shell).is_some_and(stretches) {
            flow_children(tx.doc(), *shell)
                .into_iter()
                .filter(|k| tx.doc().get(*k).is_some_and(|n| n.layout.is_some() && n.layout_child.sizing[1] == Sizing::Hug))
                .collect()
        } else {
            Vec::new()
        };
        tx.node_mut(*shell).unwrap().layout_child.sizing[1] = Sizing::Fill;
        for k in kids {
            tx.node_mut(k).unwrap().layout_child.sizing[1] = Sizing::Fill;
        }
        relayout(tx, Some(screen));
    }
    let mut scrolled = Vec::new();
    for k in flow_children(tx.doc(), screen) {
        deepest_overflow(tx.doc(), k, &mut scrolled);
    }
    if scrolled.is_empty() && overflows(tx.doc(), screen) {
        scrolled.push(screen);
    }
    scrolled
        .into_iter()
        .filter_map(|id| {
            let n = tx.node_mut(id)?;
            if let NodeKind::Frame { clip } = &mut n.kind {
                *clip = true;
            }
            n.scroll = true;
            Some(n.name.clone())
        })
        .collect()
}

/// Its content is taller than it is.
fn overflows(doc: &Document, id: NodeId) -> bool {
    doc.get(id).is_some_and(|n| overflows_by_content(doc, id, n.rect.h))
}

fn overflows_by_content(doc: &Document, id: NodeId, h: f64) -> bool {
    hug_size(doc, id, 1) > h + TOLERANCE
}

/// The innermost frames under `id` whose height is set from outside (fixed, or filling their
/// parent) and whose content is taller: those are what scroll. Frames that already scroll or
/// clip on purpose, and instances, are left alone with what is inside them.
fn deepest_overflow(doc: &Document, id: NodeId, out: &mut Vec<NodeId>) -> bool {
    let Some(n) = doc.get(id) else { return false };
    if n.layout.is_none() || n.instance.is_some() || n.scroll || matches!(n.kind, NodeKind::Frame { clip: true }) {
        return n.scroll;
    }
    let mut inner = false;
    for k in flow_children(doc, id) {
        inner |= deepest_overflow(doc, k, out);
    }
    if inner {
        return true;
    }
    if n.layout_child.sizing[1] != Sizing::Hug && overflows(doc, id) {
        out.push(id);
        return true;
    }
    false
}
