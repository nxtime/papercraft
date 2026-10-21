//! Structural and pixel diffing between frames (`diff_subtrees`, `diff_rgba`).

use crate::geom::V2;
use crate::model::{
    BlendMode, Direction, Document, Node, NodeId, NodeKind, Paint, Stop, TextAlign,
};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

/// One property difference on a paired node.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PropChange {
    pub field: &'static str,
    pub before: String,
    pub after: String,
}

/// Structural or property diff entry for a single node.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NodeDiff {
    pub id_a: Option<NodeId>,
    pub id_b: Option<NodeId>,
    pub path: String,
    pub name: String,
    pub kind: &'static str,
    pub status: &'static str,
    pub changes: Vec<PropChange>,
}

/// Summary and per-node list of differences between two subtrees.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FrameDiff {
    pub added: usize,
    pub removed: usize,
    pub modified: usize,
    pub unchanged: usize,
    pub nodes: Vec<NodeDiff>,
}

/// Result of a pixel-by-pixel RGBA comparison.
#[derive(Debug, Clone, PartialEq)]
pub struct PixelDiff {
    pub width: u32,
    pub height: u32,
    pub total_pixels: usize,
    pub mismatched_pixels: usize,
    pub ratio: f64,
    pub bounds: Option<[u32; 4]>,
    pub diff_png: Vec<u8>,
}

fn fmt_f64(v: f64) -> String {
    let r = (v * 100.0).round() / 100.0;
    if r == 0.0 {
        return "0".into();
    }
    if (r - r.round()).abs() < 1e-9 {
        format!("{}", r as i64)
    } else {
        format!("{r}")
    }
}

fn fmt_v2(v: V2) -> String {
    format!("({}, {})", fmt_f64(v.x), fmt_f64(v.y))
}

fn fmt_stops(stops: &[Stop]) -> String {
    stops
        .iter()
        .map(|s| format!("{}@{}", s.color.to_hex(), fmt_f64(s.pos)))
        .collect::<Vec<_>>()
        .join(", ")
}

fn fmt_fills(n: &Node) -> String {
    if n.style.fills.is_empty() {
        return "none".into();
    }
    n.style
        .fills
        .iter()
        .map(|p| match p {
            Paint::Solid {
                color,
                var: Some(v),
            } => format!("var(--{v}) ({})", color.to_hex()),
            Paint::Solid { color, var: None } => color.to_hex(),
            Paint::Linear { stops, from, to } => {
                format!(
                    "linear({}, {} -> {})",
                    fmt_stops(stops),
                    fmt_v2(*from),
                    fmt_v2(*to)
                )
            }
            Paint::Radial {
                stops,
                center,
                radius,
            } => {
                format!(
                    "radial({}, center={}, radius={})",
                    fmt_stops(stops),
                    fmt_v2(*center),
                    fmt_v2(*radius)
                )
            }
            Paint::Image {
                asset,
                mode,
                opacity,
            } => {
                format!("image({asset}, {mode:?}, {})", fmt_f64(*opacity))
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn fmt_stroke(n: &Node) -> String {
    match n.style.stroke {
        Some(c) if n.style.stroke_width > 0.0 => {
            format!("{} {}px", c.to_hex(), fmt_f64(n.style.stroke_width))
        }
        _ => "none".into(),
    }
}

fn fmt_corner_radius(n: &Node) -> String {
    match n.style.radii {
        Some([tl, tr, br, bl]) => {
            if (tl - tr).abs() < 1e-9 && (tr - br).abs() < 1e-9 && (br - bl).abs() < 1e-9 {
                fmt_f64(tl)
            } else {
                format!(
                    "{} {} {} {}",
                    fmt_f64(tl),
                    fmt_f64(tr),
                    fmt_f64(br),
                    fmt_f64(bl)
                )
            }
        }
        None => fmt_f64(n.style.corner_radius),
    }
}

fn fmt_opacity(n: &Node) -> String {
    if n.style.blend == BlendMode::Normal {
        fmt_f64(n.style.opacity)
    } else {
        format!("{} {}", fmt_f64(n.style.opacity), n.style.blend.css())
    }
}

fn fmt_font_props(n: &Node) -> Option<String> {
    match &n.kind {
        NodeKind::Text {
            font_size, align, ..
        } => {
            let mut parts = vec![format!("{}px", fmt_f64(*font_size))];
            if n.text_style.bold {
                parts.push("bold".into());
            }
            if n.text_style.italic {
                parts.push("italic".into());
            }
            let al = match align {
                TextAlign::Left => "left",
                TextAlign::Center => "center",
                TextAlign::Right => "right",
            };
            parts.push(al.into());
            Some(parts.join(" "))
        }
        NodeKind::Sticky { font_size, .. } => Some(format!("{}px", fmt_f64(*font_size))),
        _ => None,
    }
}

fn fmt_layout(n: &Node) -> String {
    let Some(l) = n.layout else {
        return "none".into();
    };
    let dir = match l.direction {
        Direction::Horizontal => "row",
        Direction::Vertical => "column",
    };
    let gap = if l.gap_auto {
        "auto".into()
    } else {
        fmt_f64(l.gap)
    };
    let [pt, pr, pb, pl] = l.padding;
    let pad = format!(
        "{},{},{},{}",
        fmt_f64(pt),
        fmt_f64(pr),
        fmt_f64(pb),
        fmt_f64(pl)
    );
    let wrap = if l.wrap { " wrap" } else { "" };
    format!(
        "{dir}{wrap} gap={gap} pad={pad} align={:?}/{:?}",
        l.align_main, l.align_cross
    )
    .to_lowercase()
}

fn compare_node_props(na: &Node, nb: &Node, is_root: bool) -> Vec<PropChange> {
    let mut out = Vec::new();
    let mut check = |field: &'static str, before: String, after: String| {
        if before != after {
            out.push(PropChange {
                field,
                before,
                after,
            });
        }
    };

    check(
        "size",
        format!("{} × {}", fmt_f64(na.rect.w), fmt_f64(na.rect.h)),
        format!("{} × {}", fmt_f64(nb.rect.w), fmt_f64(nb.rect.h)),
    );

    if !is_root || (na.parent.is_some() && nb.parent.is_some()) {
        check(
            "position",
            format!("{}, {}", fmt_f64(na.rect.x), fmt_f64(na.rect.y)),
            format!("{}, {}", fmt_f64(nb.rect.x), fmt_f64(nb.rect.y)),
        );
    }

    check("fills", fmt_fills(na), fmt_fills(nb));
    check("stroke", fmt_stroke(na), fmt_stroke(nb));
    check(
        "corner_radius",
        fmt_corner_radius(na),
        fmt_corner_radius(nb),
    );
    check("opacity", fmt_opacity(na), fmt_opacity(nb));

    if let (Some(ta), Some(tb)) = (na.kind.text(), nb.kind.text()) {
        check("text", ta.clone(), tb.clone());
    }
    if let (Some(fa), Some(fb)) = (fmt_font_props(na), fmt_font_props(nb)) {
        check("font_size", fa, fb);
    }

    check("layout", fmt_layout(na), fmt_layout(nb));
    check("visible", na.visible.to_string(), nb.visible.to_string());

    out
}

fn record_unmatched(doc: &Document, id: NodeId, path: String, added: bool, diff: &mut FrameDiff) {
    let Some(n) = doc.get(id) else { return };
    if added {
        diff.added += 1;
    } else {
        diff.removed += 1;
    }
    diff.nodes.push(NodeDiff {
        id_a: (!added).then_some(id),
        id_b: added.then_some(id),
        path: path.clone(),
        name: n.name.clone(),
        kind: n.kind.type_name(),
        status: if added { "added" } else { "removed" },
        changes: Vec::new(),
    });
    for &child_id in &n.children {
        if let Some(child) = doc.get(child_id) {
            let child_path = format!("{path} > {}", child.name);
            record_unmatched(doc, child_id, child_path, added, diff);
        }
    }
}

fn walk_pair(
    doc_a: &Document,
    id_a: NodeId,
    doc_b: &Document,
    id_b: NodeId,
    path: String,
    is_root: bool,
    diff: &mut FrameDiff,
) {
    let (Some(na), Some(nb)) = (doc_a.get(id_a), doc_b.get(id_b)) else {
        return;
    };

    let changes = compare_node_props(na, nb, is_root);
    if changes.is_empty() {
        diff.unchanged += 1;
    } else {
        diff.modified += 1;
        diff.nodes.push(NodeDiff {
            id_a: Some(id_a),
            id_b: Some(id_b),
            path: path.clone(),
            name: nb.name.clone(),
            kind: nb.kind.type_name(),
            status: "modified",
            changes,
        });
    }

    let mut b_counts: HashMap<(&str, &'static str), usize> = HashMap::new();
    let mut b_map: HashMap<(&str, &'static str, usize), NodeId> = HashMap::new();
    for &cid_b in &nb.children {
        if let Some(cb) = doc_b.get(cid_b) {
            let key = (cb.name.as_str(), cb.kind.type_name());
            let occ = *b_counts.entry(key).and_modify(|c| *c += 1).or_insert(0);
            b_map.insert((key.0, key.1, occ), cid_b);
        }
    }

    let mut a_counts: HashMap<(&str, &'static str), usize> = HashMap::new();
    let mut matched_b: HashSet<NodeId> = HashSet::new();

    for &cid_a in &na.children {
        let Some(ca) = doc_a.get(cid_a) else { continue };
        let key = (ca.name.as_str(), ca.kind.type_name());
        let occ = *a_counts.entry(key).and_modify(|c| *c += 1).or_insert(0);
        let child_path = format!("{path} > {}", ca.name);
        if let Some(&cid_b) = b_map.get(&(key.0, key.1, occ)) {
            matched_b.insert(cid_b);
            walk_pair(doc_a, cid_a, doc_b, cid_b, child_path, false, diff);
        } else {
            record_unmatched(doc_a, cid_a, child_path, false, diff);
        }
    }

    for &cid_b in &nb.children {
        if matched_b.contains(&cid_b) {
            continue;
        }
        if let Some(cb) = doc_b.get(cid_b) {
            let child_path = format!("{path} > {}", cb.name);
            record_unmatched(doc_b, cid_b, child_path, true, diff);
        }
    }
}

/// Compare two frames or subtrees recursively, matching siblings by `(name, kind, occurrence)`.
pub fn diff_subtrees(
    doc_a: &Document,
    root_a: NodeId,
    doc_b: &Document,
    root_b: NodeId,
) -> Result<FrameDiff, String> {
    if doc_a.get(root_a).is_none() {
        return Err(format!("node {root_a} not found in first document"));
    }
    let nb = doc_b
        .get(root_b)
        .ok_or_else(|| format!("node {root_b} not found in second document"))?;

    let mut diff = FrameDiff {
        added: 0,
        removed: 0,
        modified: 0,
        unchanged: 0,
        nodes: Vec::new(),
    };
    walk_pair(
        doc_a,
        root_a,
        doc_b,
        root_b,
        nb.name.clone(),
        true,
        &mut diff,
    );
    Ok(diff)
}

/// Rows for one top-level frame in a [`PageDiff`].
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FrameGroup {
    pub frame: NodeId,
    pub name: String,
    pub nodes: Vec<NodeDiff>,
}

/// Id-paired diff of two versions of one page.
///
/// A subtree that was added or removed is one row (its top node); new descendants are not rows and
/// not counted. Each entry of `variables` counts once in `modified`, so
/// `added + removed + modified` equals node rows plus `variables.len()`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PageDiff {
    pub added: usize,
    pub removed: usize,
    pub modified: usize,
    /// By top-level frame: its id in `after` (removed nodes: their frame in `before`), root order.
    pub groups: Vec<FrameGroup>,
    /// Variable, mode, active-mode and token changes, as `name: value` pairs.
    pub variables: Vec<PropChange>,
}

impl PageDiff {
    pub fn is_empty(&self) -> bool {
        self.groups.is_empty() && self.variables.is_empty()
    }
}

fn top_frame(doc: &Document, mut id: NodeId) -> NodeId {
    while let Some(p) = doc.get(id).and_then(|n| n.parent) {
        id = p;
    }
    id
}

fn node_path(doc: &Document, id: NodeId) -> String {
    let mut names = Vec::new();
    let mut cur = Some(id);
    while let Some(n) = cur.and_then(|c| doc.get(c)) {
        names.push(n.name.as_str());
        cur = n.parent;
    }
    names.reverse();
    names.join(" > ")
}

fn siblings(doc: &Document, parent: Option<NodeId>) -> &[NodeId] {
    match parent.and_then(|p| doc.get(p)) {
        Some(p) => &p.children,
        None => &doc.root,
    }
}

/// Index among the siblings present in both docs, so adding or removing a sibling is not a move.
fn common_index(doc: &Document, other: &Document, id: NodeId, parent: Option<NodeId>) -> usize {
    siblings(doc, parent)
        .iter()
        .filter(|s| other.get(**s).is_some())
        .position(|s| *s == id)
        .unwrap_or(0)
}

fn parent_label(doc: &Document, parent: Option<NodeId>) -> String {
    match parent.and_then(|p| doc.get(p)) {
        Some(p) => p.name.clone(),
        None => "page".into(),
    }
}

fn push_group(groups: &mut Vec<FrameGroup>, doc: &Document, id: NodeId, nd: NodeDiff) {
    let frame = top_frame(doc, id);
    if let Some(g) = groups.iter_mut().find(|g| g.frame == frame) {
        g.nodes.push(nd);
        return;
    }
    groups.push(FrameGroup {
        frame,
        name: doc.get(frame).map(|n| n.name.clone()).unwrap_or_default(),
        nodes: vec![nd],
    });
}

fn walk_page(before: &Document, after: &Document, id: NodeId, diff: &mut PageDiff) {
    let Some(nb) = after.get(id) else { return };
    match before.get(id) {
        None => {
            // one row per added subtree; keep walking for old-id descendants moved under it
            if nb.parent.is_none_or(|p| before.get(p).is_some()) {
                diff.added += 1;
                let nd = NodeDiff {
                    id_a: None,
                    id_b: Some(id),
                    path: node_path(after, id),
                    name: nb.name.clone(),
                    kind: nb.kind.type_name(),
                    status: "added",
                    changes: Vec::new(),
                };
                push_group(&mut diff.groups, after, id, nd);
            }
        }
        Some(na) => {
            let mut changes = Vec::new();
            if na.name != nb.name {
                changes.push(PropChange {
                    field: "name",
                    before: na.name.clone(),
                    after: nb.name.clone(),
                });
            }
            changes.extend(compare_node_props(na, nb, false));
            if na.comments != nb.comments {
                let show = |n: &Node| format!("{} ({} open)", n.comments.len(), n.open_comments());
                changes.push(PropChange {
                    field: "comments",
                    before: show(na),
                    after: show(nb),
                });
            }
            if na.parent != nb.parent {
                changes.push(PropChange {
                    field: "parent",
                    before: parent_label(before, na.parent),
                    after: parent_label(after, nb.parent),
                });
            } else {
                let (ia, ib) = (
                    common_index(before, after, id, na.parent),
                    common_index(after, before, id, nb.parent),
                );
                if ia != ib {
                    changes.push(PropChange {
                        field: "order",
                        before: ia.to_string(),
                        after: ib.to_string(),
                    });
                }
            }
            if !changes.is_empty() {
                diff.modified += 1;
                let nd = NodeDiff {
                    id_a: Some(id),
                    id_b: Some(id),
                    path: node_path(after, id),
                    name: nb.name.clone(),
                    kind: nb.kind.type_name(),
                    status: "modified",
                    changes,
                };
                push_group(&mut diff.groups, after, id, nd);
            }
        }
    }
    for &c in &nb.children {
        walk_page(before, after, c, diff);
    }
}

fn walk_removed(before: &Document, after: &Document, id: NodeId, diff: &mut PageDiff) {
    let Some(na) = before.get(id) else { return };
    if after.get(id).is_some() {
        for &c in &na.children {
            walk_removed(before, after, c, diff);
        }
        return;
    }
    diff.removed += 1;
    let nd = NodeDiff {
        id_a: Some(id),
        id_b: None,
        path: node_path(before, id),
        name: na.name.clone(),
        kind: na.kind.type_name(),
        status: "removed",
        changes: Vec::new(),
    };
    push_group(&mut diff.groups, before, id, nd);
}

fn diff_maps<V: PartialEq>(
    field: &'static str,
    prefix: &str,
    a: &std::collections::BTreeMap<String, V>,
    b: &std::collections::BTreeMap<String, V>,
    show: impl Fn(&V) -> String,
    out: &mut Vec<PropChange>,
) {
    let keys: std::collections::BTreeSet<&String> = a.keys().chain(b.keys()).collect();
    for k in keys {
        let (va, vb) = (a.get(k), b.get(k));
        if va == vb {
            continue;
        }
        let side = |v: Option<&V>| format!("{prefix}{k}: {}", v.map_or("none".into(), &show));
        out.push(PropChange {
            field,
            before: side(va),
            after: side(vb),
        });
    }
}

/// Compare two versions of one page, pairing nodes by id (a snapshot and the live page share
/// ids), unlike [`diff_subtrees`]. Reports added/removed subtrees once, property, name, parent
/// and sibling-order changes per node, and variable/mode/token changes separately.
pub fn diff_pages(before: &Document, after: &Document) -> PageDiff {
    let mut diff = PageDiff {
        added: 0,
        removed: 0,
        modified: 0,
        groups: Vec::new(),
        variables: Vec::new(),
    };
    for &id in &after.root {
        walk_page(before, after, id, &mut diff);
    }
    for &id in &before.root {
        walk_removed(before, after, id, &mut diff);
    }

    let color = |c: &crate::model::Color| c.to_hex();
    diff_maps(
        "variable",
        "",
        &before.variables,
        &after.variables,
        color,
        &mut diff.variables,
    );
    let names: std::collections::BTreeSet<&String> =
        before.modes.keys().chain(after.modes.keys()).collect();
    let empty = Default::default();
    for m in names {
        let (ha, hb) = (before.modes.get(m), after.modes.get(m));
        if ha.is_some() != hb.is_some() {
            let (was, now) = if hb.is_some() {
                ("none", "created")
            } else {
                ("exists", "deleted")
            };
            diff.variables.push(PropChange {
                field: "mode",
                before: format!("{m}: {was}"),
                after: format!("{m}: {now}"),
            });
        }
        let (a, b) = (ha.unwrap_or(&empty), hb.unwrap_or(&empty));
        diff_maps("mode", &format!("{m}/"), a, b, color, &mut diff.variables);
    }
    if before.mode != after.mode {
        let show = |m: &Option<String>| format!("active mode: {}", m.as_deref().unwrap_or("none"));
        diff.variables.push(PropChange {
            field: "mode",
            before: show(&before.mode),
            after: show(&after.mode),
        });
    }
    diff_maps(
        "token",
        "",
        &before.tokens,
        &after.tokens,
        |t| t.clone(),
        &mut diff.variables,
    );
    diff.modified += diff.variables.len();
    diff
}

/// Serialize a [`FrameDiff`] into a clean JSON value for MCP output.
pub fn to_json(d: &FrameDiff) -> Value {
    let changes: Vec<Value> = d
        .nodes
        .iter()
        .map(|n| {
            let mut obj = json!({
                "path": n.path,
                "name": n.name,
                "type": n.kind,
                "status": n.status,
            });
            if let Some(id_a) = n.id_a {
                obj["idA"] = json!(id_a.0);
            }
            if let Some(id_b) = n.id_b {
                obj["idB"] = json!(id_b.0);
            }
            if !n.changes.is_empty() {
                let props: Vec<Value> = n
                    .changes
                    .iter()
                    .map(|c| {
                        json!({
                            "field": c.field,
                            "before": c.before,
                            "after": c.after,
                        })
                    })
                    .collect();
                obj["changes"] = Value::Array(props);
            }
            obj
        })
        .collect();
    json!({
        "added": d.added,
        "removed": d.removed,
        "modified": d.modified,
        "unchanged": d.unchanged,
        "changes": changes,
    })
}

/// Compare two RGBA8 buffers over their union dimensions and return a visual diff PNG.
pub fn diff_rgba(
    a: &[u8],
    w_a: u32,
    h_a: u32,
    b: &[u8],
    w_b: u32,
    h_b: u32,
    threshold: u8,
) -> Result<PixelDiff, String> {
    let expected_a = (w_a as usize)
        .checked_mul(h_a as usize)
        .and_then(|px| px.checked_mul(4))
        .ok_or_else(|| "overflow in image A dimensions".to_string())?;
    let expected_b = (w_b as usize)
        .checked_mul(h_b as usize)
        .and_then(|px| px.checked_mul(4))
        .ok_or_else(|| "overflow in image B dimensions".to_string())?;
    if a.len() < expected_a {
        return Err(format!(
            "image A buffer too short: got {}, expected {expected_a}",
            a.len()
        ));
    }
    if b.len() < expected_b {
        return Err(format!(
            "image B buffer too short: got {}, expected {expected_b}",
            b.len()
        ));
    }

    let width = w_a.max(w_b);
    let height = h_a.max(h_b);
    if width == 0 || height == 0 {
        return Err("cannot diff zero-sized images".into());
    }

    let total_pixels = (width as usize) * (height as usize);
    let mut diff_buf = vec![0u8; total_pixels * 4];
    let mut mismatched_pixels = 0usize;
    let mut min_x = u32::MAX;
    let mut min_y = u32::MAX;
    let mut max_x = 0u32;
    let mut max_y = 0u32;

    for y in 0..height {
        for x in 0..width {
            let pa = if x < w_a && y < h_a {
                let idx = ((y * w_a + x) as usize) * 4;
                [a[idx], a[idx + 1], a[idx + 2], a[idx + 3]]
            } else {
                [0, 0, 0, 0]
            };
            let pb = if x < w_b && y < h_b {
                let idx = ((y * w_b + x) as usize) * 4;
                [b[idx], b[idx + 1], b[idx + 2], b[idx + 3]]
            } else {
                [0, 0, 0, 0]
            };

            let differs = (0..4).any(|c| pa[c].abs_diff(pb[c]) > threshold);
            let out_idx = ((y * width + x) as usize) * 4;
            if differs {
                mismatched_pixels += 1;
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
                diff_buf[out_idx..out_idx + 4].copy_from_slice(&[239, 68, 68, 255]);
            } else {
                let alpha = pa[3] as f64 / 255.0;
                let r = pa[0] as f64 * alpha + 255.0 * (1.0 - alpha);
                let g = pa[1] as f64 * alpha + 255.0 * (1.0 - alpha);
                let bl = pa[2] as f64 * alpha + 255.0 * (1.0 - alpha);
                let lum = 0.299 * r + 0.587 * g + 0.114 * bl;
                let dimmed = (255.0 * 0.65 + lum * 0.35).round().clamp(0.0, 255.0) as u8;
                diff_buf[out_idx..out_idx + 4].copy_from_slice(&[dimmed, dimmed, dimmed, 255]);
            }
        }
    }

    let bounds = (mismatched_pixels > 0).then_some([min_x, min_y, max_x, max_y]);
    let ratio = mismatched_pixels as f64 / total_pixels as f64;
    let img = image::RgbaImage::from_raw(width, height, diff_buf)
        .ok_or_else(|| "failed to build diff image".to_string())?;
    let diff_png = crate::io::export::encode_png(&img)?;

    Ok(PixelDiff {
        width,
        height,
        total_pixels,
        mismatched_pixels,
        ratio,
        bounds,
        diff_png,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::Rect;
    use crate::model::Color;

    fn add_node(
        doc: &mut Document,
        parent: Option<NodeId>,
        name: &str,
        kind: NodeKind,
        rect: Rect,
    ) -> NodeId {
        let id = doc.alloc_id();
        let mut n = Node::new(id, kind, rect);
        n.name = name.to_string();
        n.parent = parent;
        doc.nodes.insert(id, n);
        doc.children_mut(parent).unwrap().push(id);
        id
    }

    #[test]
    fn diff_subtrees_detects_modified_added_removed() {
        let mut doc_a = Document::default();
        let root_a = add_node(
            &mut doc_a,
            None,
            "Card",
            NodeKind::Frame { clip: true },
            Rect::new(0.0, 0.0, 320.0, 200.0),
        );
        let title_a = add_node(
            &mut doc_a,
            Some(root_a),
            "Title",
            NodeKind::Text {
                text: "Hello".into(),
                font_size: 16.0,
                auto_width: true,
                align: TextAlign::Left,
            },
            Rect::new(16.0, 16.0, 100.0, 24.0),
        );
        doc_a
            .nodes
            .get_mut(&title_a)
            .unwrap()
            .style
            .set_fill(Some(Color::rgb(0x11, 0x22, 0x33)));
        let _old_badge = add_node(
            &mut doc_a,
            Some(root_a),
            "OldBadge",
            NodeKind::Rect,
            Rect::new(16.0, 50.0, 40.0, 20.0),
        );

        let mut doc_b = doc_a.clone();
        // Modify root size, title text + fill, remove OldBadge, and add NewButton.
        doc_b.nodes.get_mut(&root_a).unwrap().rect.h = 240.0;
        if let NodeKind::Text { text, .. } = &mut doc_b.nodes.get_mut(&title_a).unwrap().kind {
            *text = "Welcome".into();
        }
        doc_b.nodes.get_mut(&title_a).unwrap().style.fills =
            vec![Paint::solid_var(Color::rgb(0xff, 0x00, 0x00), "accent")];

        // Remove OldBadge from doc_b
        let old_id = doc_b.nodes[&root_a].children[1];
        doc_b.nodes.get_mut(&root_a).unwrap().children.remove(1);
        doc_b.nodes.remove(&old_id);

        // Add NewButton to doc_b
        add_node(
            &mut doc_b,
            Some(root_a),
            "NewButton",
            NodeKind::Rect,
            Rect::new(16.0, 180.0, 120.0, 40.0),
        );

        let d = diff_subtrees(&doc_a, root_a, &doc_b, root_a).unwrap();
        assert_eq!(d.modified, 2);
        assert_eq!(d.added, 1);
        assert_eq!(d.removed, 1);
        assert_eq!(d.unchanged, 0);

        let card_diff = d.nodes.iter().find(|n| n.path == "Card").unwrap();
        assert_eq!(card_diff.status, "modified");
        assert!(
            card_diff
                .changes
                .iter()
                .any(|c| c.field == "size" && c.before == "320 × 200" && c.after == "320 × 240")
        );

        let title_diff = d.nodes.iter().find(|n| n.path == "Card > Title").unwrap();
        assert_eq!(title_diff.status, "modified");
        assert!(
            title_diff
                .changes
                .iter()
                .any(|c| c.field == "text" && c.before == "Hello" && c.after == "Welcome")
        );
        assert!(
            title_diff
                .changes
                .iter()
                .any(|c| c.field == "fills" && c.after.contains("var(--accent)"))
        );

        assert!(
            d.nodes
                .iter()
                .any(|n| n.path == "Card > OldBadge" && n.status == "removed")
        );
        assert!(
            d.nodes
                .iter()
                .any(|n| n.path == "Card > NewButton" && n.status == "added")
        );

        let j = to_json(&d);
        assert_eq!(j["modified"], 2);
        assert_eq!(j["added"], 1);
        assert_eq!(j["removed"], 1);
        assert_eq!(j["changes"].as_array().unwrap().len(), 4);
    }

    #[test]
    fn diff_rgba_identical_and_changed() {
        let w = 4u32;
        let h = 4u32;
        let a = vec![200u8; (w * h * 4) as usize];
        let same = diff_rgba(&a, w, h, &a, w, h, 2).unwrap();
        assert_eq!(same.mismatched_pixels, 0);
        assert_eq!(same.bounds, None);
        assert_eq!(same.ratio, 0.0);
        assert!(same.diff_png.starts_with(b"\x89PNG"));

        let mut b = a.clone();
        // Change pixels at (1, 2) and (2, 3)
        let idx1 = ((2 * w + 1) * 4) as usize;
        b[idx1] = 10;
        let idx2 = ((3 * w + 2) * 4) as usize;
        b[idx2 + 1] = 20;

        let changed = diff_rgba(&a, w, h, &b, w, h, 2).unwrap();
        assert_eq!(changed.mismatched_pixels, 2);
        assert_eq!(changed.bounds, Some([1, 2, 2, 3]));
        assert!((changed.ratio - 2.0 / 16.0).abs() < 1e-9);
        assert!(changed.diff_png.starts_with(b"\x89PNG"));
    }

    fn page() -> (Document, NodeId, NodeId, NodeId) {
        let mut d = Document::default();
        let f = add_node(
            &mut d,
            None,
            "Hero",
            NodeKind::Frame { clip: true },
            Rect::new(0.0, 0.0, 300.0, 200.0),
        );
        let a = add_node(
            &mut d,
            Some(f),
            "A",
            NodeKind::Rect,
            Rect::new(0.0, 0.0, 10.0, 10.0),
        );
        let b = add_node(
            &mut d,
            Some(f),
            "B",
            NodeKind::Rect,
            Rect::new(20.0, 0.0, 10.0, 10.0),
        );
        (d, f, a, b)
    }

    #[test]
    fn diff_pages_identical_is_empty() {
        let (d, ..) = page();
        let p = diff_pages(&d, &d.clone());
        assert!(p.is_empty());
        assert_eq!((p.added, p.removed, p.modified), (0, 0, 0));
    }

    #[test]
    fn diff_pages_added_removed_once_per_subtree() {
        let (before, f, a, _) = page();
        let mut after = before.clone();
        let n = add_node(
            &mut after,
            Some(f),
            "New",
            NodeKind::Rect,
            Rect::new(0.0, 0.0, 5.0, 5.0),
        );
        add_node(
            &mut after,
            Some(n),
            "Kid",
            NodeKind::Rect,
            Rect::new(0.0, 0.0, 1.0, 1.0),
        );
        let p = diff_pages(&after, &before);
        assert_eq!((p.added, p.removed, p.modified), (0, 1, 0));
        assert_eq!(p.groups[0].nodes[0].status, "removed");
        assert_eq!(p.groups[0].frame, f);
        let p = diff_pages(&before, &after);
        assert_eq!((p.added, p.removed, p.modified), (1, 0, 0));
        assert_eq!(p.groups[0].nodes[0].id_b, Some(n));
        // removing a sibling is not a move of the others
        let mut gone = before.clone();
        gone.children_mut(Some(f)).unwrap().retain(|c| *c != a);
        gone.nodes.remove(&a);
        let p = diff_pages(&before, &gone);
        assert_eq!((p.added, p.removed, p.modified), (0, 1, 0));
    }

    #[test]
    fn diff_pages_rename_is_one_changed_row() {
        let (before, _, a, _) = page();
        let mut after = before.clone();
        after.nodes.get_mut(&a).unwrap().name = "Renamed".into();
        let p = diff_pages(&before, &after);
        assert_eq!((p.added, p.removed, p.modified), (0, 0, 1));
        let row = &p.groups[0].nodes[0];
        assert_eq!(row.changes.len(), 1);
        assert_eq!(row.changes[0].field, "name");
        assert_eq!(row.path, "Hero > Renamed");
    }

    #[test]
    fn diff_pages_property_change() {
        let (before, _, a, _) = page();
        let mut after = before.clone();
        after.nodes.get_mut(&a).unwrap().rect.w = 50.0;
        let p = diff_pages(&before, &after);
        assert_eq!(p.modified, 1);
        assert_eq!(p.groups[0].nodes[0].changes[0].field, "size");
    }

    #[test]
    fn diff_pages_reparent_and_root_reorder() {
        let (before, f, a, b) = page();
        let mut after = before.clone();
        let g = add_node(
            &mut after,
            None,
            "Footer",
            NodeKind::Frame { clip: true },
            Rect::new(0.0, 300.0, 300.0, 50.0),
        );
        let mut before = before;
        before.root.push(g);
        before.nodes.insert(g, after.nodes[&g].clone());
        // move B under Footer
        after.children_mut(Some(f)).unwrap().retain(|c| *c != b);
        after.children_mut(Some(g)).unwrap().push(b);
        after.nodes.get_mut(&b).unwrap().parent = Some(g);
        let p = diff_pages(&before, &after);
        assert_eq!((p.added, p.removed, p.modified), (0, 0, 1));
        let row = &p.groups[0].nodes[0];
        assert_eq!(p.groups[0].frame, g);
        assert!(
            row.changes
                .iter()
                .any(|c| c.field == "parent" && c.before == "Hero" && c.after == "Footer")
        );
        let _ = a;

        let mut after = before.clone();
        after.root.reverse();
        let p = diff_pages(&before, &after);
        assert_eq!(p.modified, 2);
        assert!(
            p.groups
                .iter()
                .all(|g| g.nodes[0].changes.iter().any(|c| c.field == "order"))
        );
    }

    #[test]
    fn diff_pages_variables_modes_tokens() {
        let (before, ..) = page();
        let mut after = before.clone();
        after
            .variables
            .insert("accent".into(), Color::rgb(0xff, 0, 0));
        after.tokens.insert("spacing-md".into(), "16px".into());
        after.mode = Some("Dark".into());
        let p = diff_pages(&before, &after);
        assert!(p.groups.is_empty());
        let fields: Vec<_> = p.variables.iter().map(|c| c.field).collect();
        assert_eq!(fields, ["variable", "mode", "token"]);
        assert_eq!(p.variables[0].before, "accent: none");
        assert_eq!(p.variables[2].after, "spacing-md: 16px");
    }

    #[test]
    fn diff_pages_counts_variable_rows() {
        let (before, ..) = page();
        let mut after = before.clone();
        after.tokens.insert("spacing-md".into(), "16px".into());
        let p = diff_pages(&before, &after);
        assert_eq!((p.added, p.removed, p.modified), (0, 0, 1));
        assert_eq!(p.modified, p.variables.len());
    }

    #[test]
    fn diff_pages_moved_old_node_under_added_parent() {
        let (before, f, _, b) = page();
        let mut after = before.clone();
        let n = add_node(
            &mut after,
            Some(f),
            "Wrap",
            NodeKind::Frame { clip: false },
            Rect::new(0.0, 0.0, 50.0, 50.0),
        );
        after.children_mut(Some(f)).unwrap().retain(|c| *c != b);
        after.children_mut(Some(n)).unwrap().push(b);
        after.nodes.get_mut(&b).unwrap().parent = Some(n);
        let p = diff_pages(&before, &after);
        assert_eq!((p.added, p.removed, p.modified), (1, 0, 1));
        let row = p.groups[0]
            .nodes
            .iter()
            .find(|r| r.id_b == Some(b))
            .unwrap();
        assert!(
            row.changes
                .iter()
                .any(|c| c.field == "parent" && c.after == "Wrap")
        );
    }

    #[test]
    fn diff_pages_empty_mode_created_and_deleted() {
        let (before, ..) = page();
        let mut after = before.clone();
        after.modes.insert("Dark".into(), Default::default());
        let p = diff_pages(&before, &after);
        assert_eq!(p.variables.len(), 1);
        assert_eq!(p.variables[0].field, "mode");
        assert_eq!(
            (
                p.variables[0].before.as_str(),
                p.variables[0].after.as_str()
            ),
            ("Dark: none", "Dark: created")
        );
        assert_eq!(
            diff_pages(&after, &before).variables[0].after,
            "Dark: deleted"
        );
    }

    #[test]
    fn diff_pages_comment_change() {
        let (before, _, a, _) = page();
        let mut after = before.clone();
        after
            .nodes
            .get_mut(&a)
            .unwrap()
            .comments
            .push(crate::model::Comment {
                id: 1,
                text: "tighten".into(),
                author: "user".into(),
                resolved: false,
                reply: None,
            });
        let p = diff_pages(&before, &after);
        assert!(!p.is_empty());
        let c = &p.groups[0].nodes[0].changes[0];
        assert_eq!(
            (c.field, c.before.as_str(), c.after.as_str()),
            ("comments", "0 (0 open)", "1 (1 open)")
        );
        after.nodes.get_mut(&a).unwrap().comments[0].resolved = true;
        let p2 = diff_pages(&before, &after);
        assert_eq!(p2.groups[0].nodes[0].changes[0].after, "1 (0 open)");
    }
}
