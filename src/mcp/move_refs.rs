//! References between layers (click links, connector ends) when a layer moves to another page.
//! Ids are per page, so a reference that does not travel with the layer would end up naming an
//! unrelated layer, or none. Such references are cleared and counted.

use crate::history::Tx;
use crate::model::{LinkAction, NodeId, NodeKind};
use std::collections::{HashMap, HashSet};

/// References that could not move along and were cleared.
#[derive(Debug, Default, PartialEq)]
pub struct Cleared {
    pub links: usize,
    pub connector_ends: usize,
}

impl Cleared {
    pub fn warning(&self, what: &str) -> Option<String> {
        (self.links + self.connector_ends > 0).then(|| {
            format!(
                "{what}: cleared {} prototype link(s) and {} connector end(s) that cannot cross pages",
                self.links, self.connector_ends
            )
        })
    }
}

/// On the source page, before the subtree is removed: layers outside it that link to, or have
/// connector ends bound to, a layer inside it lose those references.
pub fn clear_incoming(tx: &mut Tx, gone: &HashSet<NodeId>) -> Cleared {
    let mut out = Cleared::default();
    let ids: Vec<NodeId> = tx.doc().nodes.keys().copied().filter(|i| !gone.contains(i)).collect();
    for id in ids {
        let Some(n) = tx.doc().get(id) else { continue };
        let hit = |t: &NodeId| gone.contains(t);
        let link = n.link.as_ref().is_some_and(hit);
        let overlay = matches!(&n.link_action, Some(LinkAction::Overlay { target }) if hit(target));
        let ends = match &n.kind {
            NodeKind::Connector { start, end, .. } => [start, end].iter().filter(|b| b.is_some_and(|b| hit(&b.node))).count(),
            _ => 0,
        };
        if !(link || overlay || ends > 0) {
            continue;
        }
        let n = tx.node_mut(id).expect("node listed above");
        if link {
            n.link = None;
        }
        if overlay {
            n.link_action = None;
        }
        if let NodeKind::Connector { start, end, .. } = &mut n.kind {
            for b in [start, end] {
                if b.is_some_and(|b| hit(&b.node)) {
                    *b = None;
                }
            }
        }
        out.links += usize::from(link) + usize::from(overlay);
        out.connector_ends += ends;
    }
    out
}

/// On the destination page, after the copy: references inside the moved subtree follow the
/// `old -> new` map; those to layers left behind are cleared.
pub fn remap_outgoing(tx: &mut Tx, map: &HashMap<NodeId, NodeId>) -> Cleared {
    let mut out = Cleared::default();
    for new in map.values() {
        let Some(n) = tx.node_mut(*new) else { continue };
        if let Some(t) = n.link {
            match map.get(&t) {
                Some(m) => n.link = Some(*m),
                None => (n.link, out.links) = (None, out.links + 1),
            }
        }
        if let Some(LinkAction::Overlay { target }) = &mut n.link_action {
            match map.get(target) {
                Some(m) => *target = *m,
                None => (n.link_action, out.links) = (None, out.links + 1),
            }
        }
        if let NodeKind::Connector { start, end, .. } = &mut n.kind {
            for b in [start, end] {
                let Some(bind) = b.as_mut() else { continue };
                match map.get(&bind.node) {
                    Some(m) => bind.node = *m,
                    None => (*b, out.connector_ends) = (None, out.connector_ends + 1),
                }
            }
        }
    }
    out
}
