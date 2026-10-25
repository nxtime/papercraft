//! `lint_design`: checks a design for the mistakes agents make most, with a concrete fix for each.
//!
//! The rules are conventions, not law: WCAG 2.x contrast, a 4px spacing grid, a type scale, hit
//! areas for buttons and tabs (44px on touch screens, 24px on desktop screens), real layer names, and colors that should be variables. Each
//! issue says which node, what is wrong, and what to change.

use crate::model::{Color, Document, Node, NodeId, Paint};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub struct Issue {
    pub rule: &'static str,
    pub severity: &'static str,
    pub node: NodeId,
    pub message: String,
    pub fix: String,
}

/// Screens at least this wide are desktop (pointer) UIs.
const DESKTOP_MIN_W: f64 = 1024.0;
/// Touch targets: Apple HIG 44pt, WCAG 2.5.5 (AAA).
const TOUCH_TARGET: f64 = 44.0;
/// Pointer targets: WCAG 2.2 2.5.8 (AA). Compact 32px desktop controls pass.
const POINTER_TARGET: f64 = 24.0;

/// Smallest height for a button or tab: 24 inside a desktop-wide screen, else 44 (a screen
/// narrower than a desktop, or a component on its own, whose screen is unknown).
fn min_target(doc: &Document, id: NodeId) -> f64 {
    let top = doc.ancestors(id).last().copied();
    let desktop = top.and_then(|t| doc.get(t)).is_some_and(|t| t.rect.w >= DESKTOP_MIN_W);
    if desktop { POINTER_TARGET } else { TOUCH_TARGET }
}

/// Sizes the type-scale rule accepts by default.
pub const DEFAULT_SCALE: [f64; 9] = [12.0, 14.0, 16.0, 20.0, 24.0, 32.0, 40.0, 48.0, 64.0];

/// WCAG relative luminance of an sRGB color.
fn luminance(c: Color) -> f64 {
    let lin = |v: u8| {
        let v = v as f64 / 255.0;
        if v <= 0.03928 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(c.r) + 0.7152 * lin(c.g) + 0.0722 * lin(c.b)
}

/// WCAG contrast ratio, 1..=21.
pub fn contrast(a: Color, b: Color) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// `over` composited on an opaque `base`.
fn blend(base: Color, over: Color) -> Color {
    let a = over.a as f64 / 255.0;
    let mix = |b: u8, o: u8| (b as f64 * (1.0 - a) + o as f64 * a).round() as u8;
    Color::rgb(
        mix(base.r, over.r),
        mix(base.g, over.g),
        mix(base.b, over.b),
    )
}

fn solid(n: &Node) -> Option<(Color, Option<&str>)> {
    match n.style.fills.last() {
        Some(Paint::Solid { color, var }) if color.a > 0 => Some((*color, var.as_deref())),
        _ => None,
    }
}

/// The color text sits on: the nearest opaque fill up the tree (translucent ones blend over
/// what is behind them); white when nothing is filled.
fn backdrop(doc: &Document, id: NodeId) -> Color {
    let mut layers: Vec<Color> = Vec::new();
    let mut cur = doc.get(id).and_then(|n| n.parent);
    while let Some(p) = cur {
        let Some(n) = doc.get(p) else { break };
        if n.visible
            && let Some((c, _)) = solid(n)
        {
            layers.push(c);
            if c.a == 255 {
                break;
            }
        }
        cur = n.parent;
    }
    layers
        .into_iter()
        .rev()
        .fold(Color::rgb(255, 255, 255), blend)
}

pub(crate) fn is_default_name(name: &str) -> bool {
    let base = name.trim_end_matches(|c: char| c.is_ascii_digit() || c == ' ');
    matches!(
        base,
        "Frame"
            | "Rectangle"
            | "Ellipse"
            | "Group"
            | "Path"
            | "Line"
            | "Arrow"
            | "Layer"
            | "Vector"
            | "Polygon"
            | "Star"
            | "Diamond"
            | "Triangle"
            | "Image"
            | "Icon"
    ) && name.len() < 14
}

fn off_grid(v: f64) -> bool {
    v > 0.0 && (v / 4.0 - (v / 4.0).round()).abs() > 1e-6
}

/// Lint the subtrees under `roots` (instances are skipped: their component is linted instead).
pub fn lint(doc: &Document, roots: &[NodeId], scale: &[f64]) -> Vec<Issue> {
    let mut out: Vec<Issue> = Vec::new();
    let mut ids: Vec<NodeId> = Vec::new();
    for r in roots {
        ids.extend(doc.subtree(*r));
    }
    let vars: BTreeMap<String, Color> = doc.variables.clone();
    // Colors used literally, with the first node using each, for the variables report.
    let mut literal: BTreeMap<String, (usize, NodeId)> = BTreeMap::new();
    for id in ids {
        let Some(n) = doc.get(id) else { continue };
        if !n.visible || n.instance.is_some() {
            continue;
        }
        let mut add = |rule: &'static str, severity: &'static str, message: String, fix: String| {
            out.push(Issue {
                rule,
                severity,
                node: id,
                message,
                fix,
            });
        };
        // ---- text
        if let (Some(size), Some(_)) = (n.kind.font_size(), n.kind.text()) {
            if let Some((fg, _)) = solid(n) {
                let bg = backdrop(doc, id);
                let fg = blend(bg, fg);
                let ratio = contrast(fg, bg);
                let large = size >= 24.0 || (size >= 18.66 && n.text_style.bold);
                let need = if large { 3.0 } else { 4.5 };
                if ratio < need {
                    add(
                        "contrast",
                        "error",
                        format!(
                            "text {} on {} is {ratio:.1}:1 (needs {need}:1)",
                            fg.to_hex(),
                            bg.to_hex()
                        ),
                        "use a lighter or darker text color, or change the background".into(),
                    );
                }
            }
            if size < 12.0 {
                add(
                    "text-size",
                    "warning",
                    format!("{size}px text is hard to read"),
                    "use 12px or more (14px+ for body copy)".into(),
                );
            } else if !scale.is_empty() && !scale.iter().any(|s| (s - size).abs() < 0.01) {
                let near = scale
                    .iter()
                    .min_by(|a, b| (*a - size).abs().total_cmp(&(*b - size).abs()))
                    .copied()
                    .unwrap_or(size);
                add(
                    "type-scale",
                    "info",
                    format!("{size}px is not on the type scale"),
                    format!("use {near}px (scale: {})", join(scale)),
                );
            }
        }
        // ---- spacing on the 4px grid
        if let Some(l) = &n.layout {
            let mut bad: Vec<String> = Vec::new();
            if off_grid(l.gap) {
                bad.push(format!("gap {}", l.gap));
            }
            for (side, p) in ["top", "right", "bottom", "left"].iter().zip(l.padding) {
                if off_grid(p) {
                    bad.push(format!("padding-{side} {p}"));
                }
            }
            if !bad.is_empty() {
                add(
                    "spacing-grid",
                    "warning",
                    format!("off the 4px grid: {}", bad.join(", ")),
                    "use multiples of 4 (4, 8, 12, 16, 24, 32, 48)".into(),
                );
            }
        }
        // ---- hit areas
        let lower = n.name.to_lowercase();
        let tap = lower.starts_with("button")
            || lower.starts_with("tab /")
            || lower.starts_with("tab bar /");
        let min = min_target(doc, id);
        if tap && n.is_frame() && n.rect.h < min && n.rect.h > 0.0 && !lower.contains("icon") {
            add(
                "touch-target",
                "warning",
                format!("`{}` is {:.0}px tall", n.name, n.rect.h),
                format!("make interactive targets at least {min:.0}px tall (padding, not just text size)"),
            );
        }
        // ---- names
        if !n.component
            && n.kind.text().is_none()
            && is_default_name(&n.name)
            && !doc.children(Some(id)).is_empty()
        {
            add(
                "layer-name",
                "warning",
                format!("`{}` is a default name", n.name),
                "name it for what it is (layer-name=\"Task Row\"), `Component / Variant` for parts"
                    .into(),
            );
        }
        // ---- colors that should be variables
        // A `text-color` binding (bind_variable) follows its variable without a fill link.
        let bound_text = n.kind.text().is_some() && n.bindings.contains_key("text-color");
        if let Some((c, var)) = solid(n)
            && var.is_none()
            && !bound_text
            && c.a == 255
        {
            if let Some((name, _)) = vars.iter().find(|(_, v)| **v == c) {
                add(
                    "hard-coded-color",
                    "warning",
                    format!("{} is the value of --{name}", c.to_hex()),
                    format!("write var(--{name}) so a theme change reaches it"),
                );
            } else if !vars.is_empty() {
                let e = literal.entry(c.to_hex()).or_insert((0, id));
                e.0 += 1;
            }
        }
    }
    for (hex, (count, first)) in literal {
        out.push(Issue {
            rule: "hard-coded-color",
            severity: "info",
            node: first,
            message: format!("{hex} is used {count}× and is not a variable"),
            fix: "add it with set_variables, or use an existing variable".into(),
        });
    }
    out
}

/// Largest width or height difference at which a frame still counts as a copy of a component.
const COPY_SIZE_TOLERANCE: f64 = 4.0;
/// Text size difference at which two texts still have the same role.
const TEXT_SIZE_TOLERANCE: f64 = 0.5;

/// Corner radius difference at which two frames still look the same.
const COPY_RADIUS_TOLERANCE: f64 = 1.0;

/// Same kinds and child counts all the way down (children in order, as `diff_frames` pairs
/// them), frames that look alike (fill, radius, stroke), and texts with the same role (size
/// and weight). Without the look a tab strip and a button of the same size would match.
fn same_shape(a: &Document, ia: NodeId, b: &Document, ib: NodeId) -> bool {
    let (Some(na), Some(nb)) = (a.get(ia), b.get(ib)) else { return false };
    if na.kind.type_name() != nb.kind.type_name() || na.children.len() != nb.children.len() {
        return false;
    }
    // An instance cannot swap its icon, so a different icon is a different component.
    let icon = |n: &Node| n.name.strip_prefix(ICON_PREFIX).map(str::to_owned);
    if (icon(na).is_some() || icon(nb).is_some()) && icon(na) != icon(nb) {
        return false;
    }
    if na.is_frame()
        && (na.style.fills.first() != nb.style.fills.first()
            || (na.style.corner_radius - nb.style.corner_radius).abs() > COPY_RADIUS_TOLERANCE
            || (na.style.stroke.is_some() != nb.style.stroke.is_some()))
    {
        return false;
    }
    if let (Some(sa), Some(sb)) = (na.kind.font_size(), nb.kind.font_size())
        && ((sa - sb).abs() > TEXT_SIZE_TOLERANCE || na.text_style.bold != nb.text_style.bold)
    {
        return false;
    }
    na.children.iter().zip(&nb.children).all(|(ca, cb)| same_shape(a, *ca, b, *cb))
}

/// Every main component the editor knows: on any page and in enabled libraries.
pub fn component_mains(ed: &crate::editor::Editor) -> Vec<(&Document, NodeId)> {
    let docs = (0..ed.pages.len()).map(|p| ed.page_doc(p)).chain(ed.library_docs.values());
    docs.flat_map(|d| d.nodes.values().filter(|n| n.component).map(move |n| (d, n.id))).collect()
}

/// Name prefix of a layer placed with `<x-paper-icon>` (`Icon / lucide:check`).
const ICON_PREFIX: &str = "Icon / ";

/// `not-an-instance`: frames under `roots` built by hand to look like one of `mains` (same
/// structure, size within 4px) instead of being an instance of it. `lint_design {fix}` leaves
/// these alone; swapping is the user's call.
pub fn not_an_instance(doc: &Document, roots: &[NodeId], mains: &[(&Document, NodeId)]) -> Vec<Issue> {
    let mut out = Vec::new();
    for id in roots.iter().flat_map(|r| doc.subtree(*r)) {
        let Some(n) = doc.get(id) else { continue };
        if !n.visible || !n.is_frame() || n.component || n.instance.is_some() || n.children.is_empty() {
            continue;
        }
        let near = |m: &Node| (m.rect.w - n.rect.w).abs() <= COPY_SIZE_TOLERANCE && (m.rect.h - n.rect.h).abs() <= COPY_SIZE_TOLERANCE;
        let hit = mains.iter().find(|(d, m)| d.get(*m).is_some_and(near) && same_shape(doc, id, d, *m));
        let Some(m) = hit.and_then(|(d, m)| d.get(*m)) else { continue };
        out.push(Issue {
            rule: "not-an-instance",
            severity: "warning",
            node: id,
            message: format!("`{}` looks like component `{}` but is not an instance of it", n.name, m.name),
            fix: format!("write_html mode replace on it with <x-paper-instance component=\"{}\"/> (plus overrides for its texts)", m.name),
        });
    }
    out
}

fn snap4(v: f64) -> f64 {
    if v <= 0.0 {
        0.0
    } else {
        ((v / 4.0).round() * 4.0).max(4.0)
    }
}

/// Apply safe automatic fixes (`spacing-grid`, `hard-coded-color`, `text-size`, `type-scale`,
/// `touch-target`) to `id` (or all non-instance visible descendants under `roots`).
/// Returns the number of nodes modified.
pub fn autofix(tx: &mut crate::history::Tx, roots: &[NodeId], scale: &[f64]) -> usize {
    let mut ids: Vec<NodeId> = Vec::new();
    for r in roots {
        ids.extend(tx.doc().subtree(*r));
    }
    let mut fixed = 0;
    for id in ids {
        if autofix_node(tx, id, scale) {
            fixed += 1;
        }
    }
    fixed
}

/// Apply safe automatic fixes to a single node `id`. Returns true if anything changed.
pub fn autofix_node(tx: &mut crate::history::Tx, id: NodeId, scale: &[f64]) -> bool {
    let Some(n) = tx.doc().get(id).cloned() else {
        return false;
    };
    if !n.visible || n.instance.is_some() {
        return false;
    }
    let vars = tx.doc().variables.clone();
    let mut changed = false;

    // 1. Snap off-grid auto-layout spacing/padding to 4px multiples.
    let mut new_layout = n.layout;
    if let Some(l) = &mut new_layout {
        if off_grid(l.gap) {
            l.gap = snap4(l.gap);
            changed = true;
        }
        if off_grid(l.cross_gap) {
            l.cross_gap = snap4(l.cross_gap);
            changed = true;
        }
        for p in &mut l.padding {
            if off_grid(*p) {
                *p = snap4(*p);
                changed = true;
            }
        }
    }

    // 2. Relink hard-coded solid fills to matching document color variables.
    let mut new_fills = n.style.fills.clone();
    for fill in &mut new_fills {
        if let Paint::Solid { color, var } = fill
            && var.is_none()
            && color.a == 255
            && let Some((name, _)) = vars.iter().find(|(_, v)| **v == *color)
        {
            *var = Some(name.clone());
            changed = true;
        }
    }

    // 3. Snap text size to >= 12px and nearest type-scale value.
    let mut new_kind = n.kind.clone();
    if let crate::model::NodeKind::Text { font_size, .. } = &mut new_kind {
        let target = if *font_size < 12.0 {
            scale.iter().copied().find(|s| *s >= 12.0).unwrap_or(12.0)
        } else if !scale.is_empty() && !scale.iter().any(|s| (s - *font_size).abs() < 0.01) {
            scale
                .iter()
                .min_by(|a, b| (*a - *font_size).abs().total_cmp(&(*b - *font_size).abs()))
                .copied()
                .unwrap_or(*font_size)
        } else {
            *font_size
        };
        if (target - *font_size).abs() > 0.01 {
            *font_size = target;
            changed = true;
        }
    }

    // 4. Touch target height for buttons/tabs (see `min_target`).
    let lower = n.name.to_lowercase();
    let tap =
        lower.starts_with("button") || lower.starts_with("tab /") || lower.starts_with("tab bar /");
    let mut new_h = n.rect.h;
    let min = min_target(tx.doc(), id);
    if tap && n.is_frame() && n.rect.h < min && n.rect.h > 0.0 && !lower.contains("icon") {
        let extra = ((min - n.rect.h) / 2.0).ceil();
        if let Some(l) = &mut new_layout
            && n.layout_child.sizing[1] == crate::model::Sizing::Hug
        {
            l.padding[0] = snap4(l.padding[0] + extra);
            l.padding[2] = snap4(l.padding[2] + extra);
        }
        new_h = min;
        changed = true;
    }

    if changed && let Some(m) = tx.node_mut(id) {
        m.layout = new_layout;
        m.style.fills = new_fills;
        m.kind = new_kind;
        m.rect.h = new_h;
    }
    changed
}

fn join(v: &[f64]) -> String {
    v.iter()
        .map(|s| format!("{s}"))
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contrast_matches_wcag_reference_values() {
        assert!((contrast(Color::rgb(0, 0, 0), Color::rgb(255, 255, 255)) - 21.0).abs() < 0.01);
        assert!(
            (contrast(Color::rgb(119, 119, 119), Color::rgb(255, 255, 255)) - 4.48).abs() < 0.05
        );
        assert!(off_grid(13.0) && !off_grid(12.0) && !off_grid(0.0));
        assert!(is_default_name("Frame 12") && !is_default_name("Task Row"));
    }

    #[test]
    fn text_colour_binding_is_not_hard_coded() {
        let mut doc = Document::default();
        let c = Color::rgb(0x8a, 0x6d, 0);
        doc.variables.insert("tag-text".into(), c);
        let id = doc.alloc_id();
        let mut n = Node::new(
            id,
            crate::model::NodeKind::Text { text: "Hi".into(), font_size: 16.0, auto_width: true, align: Default::default() },
            crate::geom::Rect::new(0.0, 0.0, 40.0, 20.0),
        );
        n.name = "Label".into();
        n.style.fills = vec![Paint::solid(c)];
        doc.nodes.insert(id, n);
        doc.root.push(id);
        let hard = |d: &Document| lint(d, &[id], &DEFAULT_SCALE).iter().filter(|i| i.rule == "hard-coded-color").count();
        assert_eq!(hard(&doc), 1);
        doc.nodes.get_mut(&id).unwrap().bindings.insert("text-color".into(), "tag-text".into());
        assert_eq!(hard(&doc), 0);
    }

    #[test]
    fn hand_made_copy_of_a_component_is_flagged_and_an_instance_is_not() {
        use crate::mcp::{Out, exec};
        use serde_json::{Value, json};
        let dir = std::env::temp_dir().join(format!("papercraft-lint-{:016x}", fastrand::u64(..)));
        let mut h = exec::SessionHost::new(crate::project::Store::open(dir).unwrap());
        h.session.create(&mut h.ed, "Lint").unwrap();
        let mut call = |name: &str, args: Value| match exec::call(&mut h, name, &args).unwrap_or_else(|e| panic!("{name}: {e}")).remove(0) {
            Out::Text(t) => serde_json::from_str::<Value>(&t).unwrap(),
            _ => Value::Null,
        };
        let button = |name: &str, label: &str| format!("<div layer-name=\"{name}\" style=\"display:flex;padding:12px 16px\"><div style=\"font-size:16px\">{label}</div></div>");
        let ab = call("create_artboard", json!({"name": "Screen", "styles": {"width": "400px", "height": "400px", "display": "flex", "gap": "8px"}}));
        let r = call("write_html", json!({"targetNodeId": ab["id"], "mode": "insert-children", "html": button("Button", "Save")}));
        let main = call("create_component", json!({"nodeId": r["createdNodeIds"][0], "name": "Button"}));
        call("create_instance", json!({"componentId": main["id"], "parentId": ab["id"]}));
        let copy = call("write_html", json!({"targetNodeId": ab["id"], "mode": "insert-children", "html": button("Save copy", "Send")}));
        call("write_html", json!({"targetNodeId": ab["id"], "mode": "insert-children", "html": "<div layer-name=\"Other\" style=\"display:flex;padding:12px 16px\"><div style=\"font-size:24px\">Big</div></div>"}));
        // Same structure and size but a different look (a filled tab): not a copy of Button.
        call("write_html", json!({"targetNodeId": ab["id"], "mode": "insert-children", "html": "<div layer-name=\"Tab\" style=\"display:flex;padding:12px 16px;background:#1677ff\"><div style=\"font-size:16px\">Code</div></div>"}));
        let hits = |r: &Value| r["issues"].as_array().unwrap().iter().filter(|i| i["rule"] == "not-an-instance").cloned().collect::<Vec<_>>();
        let r = call("lint_design", json!({}));
        let found = hits(&r);
        assert_eq!(found.len(), 1, "only the hand-made copy: {r}");
        assert_eq!(found[0]["node"], copy["createdNodeIds"][0]);
        assert!(found[0]["fix"].as_str().unwrap().contains("<x-paper-instance component=\"Button\"/>"), "{}", found[0]);
        let r = call("lint_design", json!({"fix": true}));
        assert_eq!(hits(&r).len(), 1, "fix does not swap: {r}");
    }

    #[test]
    fn a_different_icon_is_a_different_component() {
        // Issue #16.
        let mut doc = Document::default();
        use crate::geom::Rect;
        use crate::model::NodeKind;
        let chip = |doc: &mut Document, icon: &str| {
            let (root, kid) = (doc.alloc_id(), doc.alloc_id());
            let mut r = Node::new(root, NodeKind::Frame { clip: false }, Rect::new(0.0, 0.0, 60.0, 20.0));
            r.children = vec![kid];
            let mut i = Node::new(kid, NodeKind::Frame { clip: false }, Rect::new(0.0, 0.0, 14.0, 14.0));
            i.name = format!("Icon / {icon}");
            i.parent = Some(root);
            doc.nodes.insert(root, r);
            doc.nodes.insert(kid, i);
            root
        };
        let (star, heart, star2) = (chip(&mut doc, "phosphor:star"), chip(&mut doc, "phosphor:heart"), chip(&mut doc, "phosphor:star"));
        assert!(!same_shape(&doc, star, &doc, heart));
        assert!(same_shape(&doc, star, &doc, star2));
    }

    #[test]
    fn autofix_snaps_grid_and_links_variables() {
        let mut doc = Document::default();
        let accent = Color::rgb(0x3b, 0x82, 0xf6);
        doc.variables.insert("accent".into(), accent);
        let id = doc.alloc_id();
        let mut n = Node::new(
            id,
            crate::model::NodeKind::Frame { clip: false },
            crate::geom::Rect::new(0.0, 0.0, 120.0, 36.0),
        );
        n.name = "Button / Primary".into();
        n.style.fills = vec![Paint::solid(accent)];
        n.layout = Some(crate::model::AutoLayout {
            gap: 10.0,
            padding: [6.0, 14.0, 6.0, 14.0],
            ..Default::default()
        });
        doc.nodes.insert(id, n);
        doc.root.push(id);

        let mut rec = crate::history::Recording::new("autofix", &doc);
        let mut tx = crate::history::Tx::new(&mut doc, &mut rec);
        let count = autofix(&mut tx, &[id], &DEFAULT_SCALE);
        assert_eq!(count, 1);
        let fixed = doc.get(id).unwrap();
        assert_eq!(fixed.style.fills[0].var(), Some("accent"));
        let l = fixed.layout.unwrap();
        assert_eq!(l.gap, 12.0);
        assert_eq!(l.padding, [8.0, 16.0, 8.0, 16.0]);
        assert_eq!(fixed.rect.h, 44.0);
    }

    #[test]
    fn compact_buttons_pass_on_a_desktop_screen_not_on_a_phone() {
        let mut doc = Document::default();
        let mut screen = |w: f64| {
            let s = doc.alloc_id();
            doc.nodes.insert(s, Node::new(s, crate::model::NodeKind::Frame { clip: false }, crate::geom::Rect::new(0.0, 0.0, w, 800.0)));
            doc.root.push(s);
            let b = doc.alloc_id();
            let mut n = Node::new(b, crate::model::NodeKind::Frame { clip: false }, crate::geom::Rect::new(0.0, 0.0, 96.0, 32.0));
            n.name = "Button / Primary".into();
            n.parent = Some(s);
            doc.nodes.insert(b, n);
            doc.nodes.get_mut(&s).unwrap().children.push(b);
            s
        };
        let (desktop, phone) = (screen(1440.0), screen(390.0));
        let targets = |root| lint(&doc, &[root], &DEFAULT_SCALE).into_iter().filter(|i| i.rule == "touch-target").count();
        assert_eq!(targets(desktop), 0, "32px clears WCAG 2.2 AA on a pointer UI");
        assert_eq!(targets(phone), 1, "a phone still needs 44px");
    }
}
