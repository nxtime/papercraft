//! Responsive breakpoints: frames of one screen at several widths become one page. A frame
//! named `Home @mobile` (or `Home @768`) joins the frame named `Home`; the narrower frame's
//! differences are written as `@media (max-width: ..)` rules.

use crate::model::{Document, NodeId};
use std::collections::HashMap;

/// A narrower design of a page: its frame and the largest window width it applies to.
pub type Variant = (NodeId, u32);

const PATH_SEP: char = '\u{1f}';

/// `(page frame, its narrower variants widest first)` per page, and a warning per frame that
/// could not join one (it stays a page of its own).
pub fn group(doc: &Document, ids: Vec<NodeId>) -> (Vec<(NodeId, Vec<Variant>)>, Vec<String>) {
    let name = |id: NodeId| {
        doc.get(id)
            .map(|n| n.name.trim().to_owned())
            .unwrap_or_default()
    };
    let width = |id: NodeId| doc.get(id).map_or(0.0, |n| n.rect.w);
    let mut groups: Vec<(NodeId, Vec<Variant>)> = Vec::new();
    let mut warnings = Vec::new();
    let mut variants = Vec::new();
    for id in ids {
        match name(id).split_once(" @") {
            Some((base, suffix)) => {
                variants.push((id, base.trim().to_lowercase(), suffix.trim().to_owned()))
            }
            None => groups.push((id, Vec::new())),
        }
    }
    for (id, base, suffix) in variants {
        let label = name(id);
        let found: Vec<usize> = (0..groups.len())
            .filter(|&g| name(groups[g].0).to_lowercase() == base)
            .collect();
        let [g] = found[..] else {
            let why = if found.is_empty() {
                "no frame has that name before ` @`"
            } else {
                "several frames have that name before ` @`"
            };
            warnings.push(format!(
                "`{label}`: {why}, so it is exported as its own page"
            ));
            groups.push((id, Vec::new()));
            continue;
        };
        let max = suffix.parse::<u32>().unwrap_or(width(id).round() as u32);
        let taken = groups[g].1.iter().any(|v| v.1 == max);
        if width(id) >= width(groups[g].0) || taken {
            let why = if taken {
                "another variant already applies at that width"
            } else {
                "it is not narrower than the frame it belongs to"
            };
            warnings.push(format!(
                "`{label}`: {why}, so it is exported as its own page"
            ));
            groups.push((id, Vec::new()));
            continue;
        }
        groups[g].1.push((id, max));
    }
    for g in &mut groups {
        g.1.sort_by(|a, b| b.1.cmp(&a.1));
    }
    (groups, warnings)
}

/// Where `id` sits under `root`: the layer names from the root down, each with its index among
/// same-named siblings. The same screen in another frame has the same path for the same layer.
pub fn path(doc: &Document, root: NodeId, id: NodeId) -> String {
    let mut segs = Vec::new();
    let mut cur = id;
    while cur != root {
        let Some(n) = doc.get(cur) else { break };
        let Some((p, parent)) = n.parent.and_then(|p| doc.get(p).map(|pn| (p, pn))) else {
            break;
        };
        let k = parent
            .children
            .iter()
            .filter(|c| doc.get(**c).is_some_and(|s| s.name == n.name))
            .position(|c| *c == cur)
            .unwrap_or(0);
        segs.push(format!("{}#{k}", n.name.replace(PATH_SEP, " ")));
        cur = p;
    }
    segs.reverse();
    segs.join(&PATH_SEP.to_string())
}

/// The path of the parent layer; empty for a child of the root.
pub fn parent_path(path: &str) -> &str {
    path.rsplit_once(PATH_SEP).map_or("", |p| p.0)
}

/// Every layer of `root`'s subtree by path.
pub fn paths(doc: &Document, root: NodeId) -> HashMap<String, NodeId> {
    doc.subtree(root)
        .into_iter()
        .map(|id| (path(doc, root, id), id))
        .collect()
}
