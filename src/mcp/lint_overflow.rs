//! The `overflow` lint rule: layers that stick out of the artboard or clipping frame they sit in,
//! and text taller than its box.
//!
//! A pure function over a document and a set of node ids, so a caller can scope it to any subset
//! (a subtree, the layers changed since a revision). It reads each layer's stored rect (auto
//! layout has already placed it) and the text height from the same layout the renderer uses.
//!
//! Not flagged: content of a `scroll` frame (it clips on purpose and the overflow is what
//! scrolls), and layers pinned inside one. Layers hidden themselves or by an ancestor are skipped.
//! `lint_design {fix}` does not repair overflow: whether to shrink, wrap, move or enlarge the
//! parent is a design call.

use super::lint::Issue;
use crate::geom::Rect;
use crate::model::{Document, Node, NodeId, NodeKind};

/// Overflow up to this many pixels is rounding noise from fractional layout.
const TOLERANCE: f64 = 0.5;

/// Overflow issues for the layers in `ids` (ids missing from `doc` are ignored).
pub fn overflow(doc: &Document, ids: &[NodeId]) -> Vec<Issue> {
    ids.iter().flat_map(|id| check(doc, *id)).collect()
}

/// A frame the renderer clips to, or the artboard a layer must stay inside.
fn bounds_layer(n: &Node) -> bool {
    n.screen || (n.parent.is_none() && n.is_frame()) || matches!(n.kind, NodeKind::Frame { clip: true })
}

/// The nearest ancestor `id` must stay inside; `None` for a top-level layer or one in scrolled content.
fn container_of(doc: &Document, id: NodeId) -> Option<NodeId> {
    for a in doc.ancestors(id) {
        let n = doc.get(a)?;
        if n.scroll {
            return None;
        }
        if bounds_layer(n) {
            return Some(a);
        }
    }
    None
}

/// How far `r` extends past `bounds` on each side, as `(side, px)` for the sides beyond tolerance.
fn past(r: Rect, bounds: Rect) -> Vec<(&'static str, f64)> {
    let sides = [
        ("left", bounds.x - r.x),
        ("top", bounds.y - r.y),
        ("right", (r.x + r.w) - (bounds.x + bounds.w)),
        ("bottom", (r.y + r.h) - (bounds.y + bounds.h)),
    ];
    sides.into_iter().filter(|(_, px)| *px > TOLERANCE).collect()
}

fn describe(sides: &[(&'static str, f64)]) -> String {
    let parts: Vec<String> = sides.iter().map(|(s, px)| format!("{}px past the {s} edge", px.round())).collect();
    parts.join(", ")
}

fn check(doc: &Document, id: NodeId) -> Vec<Issue> {
    let Some(n) = doc.get(id) else { return Vec::new() };
    if !doc.is_effectively_visible(id) {
        return Vec::new();
    }
    let Some(container) = container_of(doc, id) else { return Vec::new() };
    let mut out = Vec::new();
    out.extend(outside_container(doc, n, container));
    out.extend(text_taller_than_box(n));
    out
}

fn outside_container(doc: &Document, n: &Node, container: NodeId) -> Option<Issue> {
    let (rect, bounds) = (doc.world_rect(n.id)?, doc.world_rect(container)?);
    let sides = past(rect, bounds);
    if sides.is_empty() {
        return None;
    }
    // A layer inside something that already overflows would repeat that finding.
    if let Some(p) = n.parent.filter(|p| *p != container)
        && doc.world_rect(p).is_some_and(|r| !past(r, bounds).is_empty())
    {
        return None;
    }
    let cn = doc.get(container)?;
    Some(Issue {
        rule: "overflow",
        severity: "error",
        node: n.id,
        message: format!("`{}` is {} of `{}`", n.name, describe(&sides), cn.name),
        fix: "move or resize it inside its parent, or let the parent grow (hug sizing); do not rely on clipping".into(),
    })
}

fn text_taller_than_box(n: &Node) -> Option<Issue> {
    if !matches!(n.kind, NodeKind::Text { .. }) {
        return None;
    }
    let extra = crate::text::layout_node(n)?.height - n.rect.h;
    if extra <= TOLERANCE {
        return None;
    }
    Some(Issue {
        rule: "overflow",
        severity: "error",
        node: n.id,
        message: format!("text `{}` is {}px taller than its {}px box and is clipped", n.name, extra.round(), n.rect.h.round()),
        fix: "widen the box, shorten the text, lower the font size, or give it hug height".into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    type R = (f64, f64, f64, f64);

    fn add(doc: &mut Document, parent: Option<NodeId>, kind: NodeKind, r: R, name: &str) -> NodeId {
        let id = doc.alloc_id();
        let mut n = Node::new(id, kind, Rect::new(r.0, r.1, r.2, r.3));
        n.name = name.into();
        n.parent = parent;
        doc.nodes.insert(id, n);
        match parent {
            Some(p) => doc.nodes.get_mut(&p).unwrap().children.push(id),
            None => doc.root.push(id),
        }
        id
    }

    fn frame(doc: &mut Document, parent: Option<NodeId>, r: R, name: &str) -> NodeId {
        add(doc, parent, NodeKind::Frame { clip: true }, r, name)
    }

    fn run(doc: &Document) -> Vec<Issue> {
        overflow(doc, &doc.paint_order())
    }

    #[test]
    fn child_past_the_right_edge_is_flagged_with_amount_and_side() {
        let mut d = Document::default();
        let board = frame(&mut d, None, (0.0, 0.0, 200.0, 200.0), "Board");
        let kid = add(&mut d, Some(board), NodeKind::Rect, (150.0, 10.0, 80.0, 20.0), "Chip");
        let issues = run(&d);
        assert_eq!(issues.len(), 1);
        assert_eq!((issues[0].rule, issues[0].node), ("overflow", kid));
        assert!(issues[0].message.contains("30px past the right edge"), "{}", issues[0].message);
    }

    #[test]
    fn a_child_inside_a_clipping_frame_is_flagged_on_every_side() {
        let mut d = Document::default();
        let board = frame(&mut d, None, (0.0, 0.0, 400.0, 400.0), "Board");
        let card = frame(&mut d, Some(board), (50.0, 50.0, 100.0, 100.0), "Card");
        add(&mut d, Some(card), NodeKind::Rect, (-10.0, 90.0, 50.0, 30.0), "Badge");
        let issues = run(&d);
        assert_eq!(issues.len(), 1);
        let m = &issues[0].message;
        assert!(m.contains("10px past the left edge") && m.contains("20px past the bottom edge") && m.contains("`Card`"), "{m}");
    }

    #[test]
    fn a_non_clipping_frame_only_matters_against_the_artboard() {
        let mut d = Document::default();
        let board = frame(&mut d, None, (0.0, 0.0, 300.0, 300.0), "Board");
        let loose = add(&mut d, Some(board), NodeKind::Frame { clip: false }, (0.0, 0.0, 100.0, 100.0), "Loose");
        add(&mut d, Some(loose), NodeKind::Rect, (80.0, 0.0, 60.0, 20.0), "Inside board");
        assert!(run(&d).is_empty(), "sticks out of Loose but stays on the artboard");
        add(&mut d, Some(loose), NodeKind::Rect, (250.0, 0.0, 100.0, 20.0), "Off board");
        let issues = run(&d);
        assert_eq!(issues.len(), 1);
        assert!(issues[0].message.contains("`Board`"));
    }

    #[test]
    fn scrolling_content_and_pinned_layers_are_not_flagged() {
        let mut d = Document::default();
        let screen = frame(&mut d, None, (0.0, 0.0, 390.0, 800.0), "Home");
        {
            let s = d.nodes.get_mut(&screen).unwrap();
            s.scroll = true;
            s.screen = true;
        }
        add(&mut d, Some(screen), NodeKind::Rect, (0.0, 0.0, 390.0, 3000.0), "Long feed");
        let bar = add(&mut d, Some(screen), NodeKind::Rect, (0.0, 780.0, 390.0, 60.0), "Tab bar");
        d.nodes.get_mut(&bar).unwrap().layout_child.absolute = true;
        assert!(run(&d).is_empty());
    }

    #[test]
    fn overflow_inside_scrolled_content_is_still_checked_against_its_own_parent() {
        let mut d = Document::default();
        let screen = frame(&mut d, None, (0.0, 0.0, 390.0, 800.0), "Home");
        d.nodes.get_mut(&screen).unwrap().scroll = true;
        let card = frame(&mut d, Some(screen), (0.0, 1000.0, 200.0, 100.0), "Card");
        add(&mut d, Some(card), NodeKind::Rect, (150.0, 0.0, 100.0, 20.0), "Pill");
        let issues = run(&d);
        assert_eq!(issues.len(), 1);
        assert!(issues[0].message.contains("`Card`"));
    }

    #[test]
    fn long_text_in_a_fixed_box_is_flagged_when_the_parent_clips() {
        let mut d = Document::default();
        let board = frame(&mut d, None, (0.0, 0.0, 300.0, 300.0), "Board");
        let kind = NodeKind::Text {
            text: "A very long title that has to wrap onto several lines in a narrow box".into(),
            font_size: 16.0,
            auto_width: false,
            align: Default::default(),
        };
        let t = add(&mut d, Some(board), kind, (10.0, 10.0, 80.0, 20.0), "Title");
        let issues = run(&d);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].node, t);
        assert!(issues[0].message.contains("taller than its 20px box"), "{}", issues[0].message);
    }

    #[test]
    fn text_that_fits_its_box_is_clean() {
        let mut d = Document::default();
        let board = frame(&mut d, None, (0.0, 0.0, 300.0, 300.0), "Board");
        let kind = NodeKind::Text { text: "Hi".into(), font_size: 16.0, auto_width: false, align: Default::default() };
        let t = add(&mut d, Some(board), kind, (10.0, 10.0, 80.0, 20.0), "Title");
        let h = crate::text::layout_node(d.get(t).unwrap()).unwrap().height;
        d.nodes.get_mut(&t).unwrap().rect.h = h;
        assert!(run(&d).is_empty());
    }

    #[test]
    fn half_a_pixel_is_tolerated_and_a_bit_more_is_not() {
        let mut d = Document::default();
        let board = frame(&mut d, None, (0.0, 0.0, 100.0, 100.0), "Board");
        let kid = add(&mut d, Some(board), NodeKind::Rect, (0.0, 0.0, 100.5, 50.0), "Edge");
        assert!(run(&d).is_empty());
        d.nodes.get_mut(&kid).unwrap().rect.w = 100.6;
        assert_eq!(run(&d).len(), 1);
    }

    #[test]
    fn hidden_layers_and_children_of_an_overflowing_parent_are_not_repeated() {
        let mut d = Document::default();
        let board = frame(&mut d, None, (0.0, 0.0, 200.0, 200.0), "Board");
        let wide = frame(&mut d, Some(board), (100.0, 0.0, 200.0, 50.0), "Wide");
        add(&mut d, Some(wide), NodeKind::Rect, (120.0, 0.0, 40.0, 20.0), "Inner");
        let ghost = add(&mut d, Some(board), NodeKind::Rect, (500.0, 0.0, 10.0, 10.0), "Ghost");
        d.nodes.get_mut(&ghost).unwrap().visible = false;
        let issues = run(&d);
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert_eq!(issues[0].node, wide);
    }

    #[test]
    fn only_the_given_nodes_are_checked() {
        let mut d = Document::default();
        let board = frame(&mut d, None, (0.0, 0.0, 100.0, 100.0), "Board");
        let bad = add(&mut d, Some(board), NodeKind::Rect, (90.0, 0.0, 50.0, 10.0), "Bad");
        let good = add(&mut d, Some(board), NodeKind::Rect, (0.0, 0.0, 50.0, 10.0), "Good");
        assert!(overflow(&d, &[good]).is_empty());
        assert_eq!(overflow(&d, &[bad]).len(), 1);
    }
}
