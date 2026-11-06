//! The accessibility lint rules, over the annotations from `set_accessibility`:
//! `image-alt` (a picture with neither alt text nor a decorative mark), `interactive-name` (a
//! clickable layer with no text, alt or real name), `heading-skip` (explicit headings that jump
//! a level) and `reading-order` (an explicit order that disagrees with the visual one).

use super::lint::Issue;
use crate::model::{Document, LinkAction, Node, NodeId, NodeKind, Paint};

/// Issues for the layers in `ids`, which must be in document order (one artboard's subtree).
pub fn a11y(doc: &Document, ids: &[NodeId]) -> Vec<Issue> {
    let mut out = Vec::new();
    let mut last_heading: Option<u8> = None;
    for id in ids {
        let Some(n) = doc.get(*id).filter(|n| n.instance.is_none() && doc.is_effectively_visible(*id)) else {
            continue;
        };
        let issue = |rule, message: String, fix: &str| Issue { rule, severity: "warning", node: *id, message, fix: fix.into() };
        let a = n.a11y.as_ref();
        if is_picture(n) && !a.is_some_and(|a| a.decorative || a.alt.is_some()) {
            out.push(issue(
                "image-alt",
                format!("picture `{}` has no alt text", n.name),
                "set_accessibility {alt:\"what it shows\"}, or {decorative:true} if it is only ornament",
            ));
        }
        let acts = n.link.is_some()
            || matches!(n.link_action, Some(LinkAction::Url { .. }))
            || a.and_then(|a| a.role.as_deref()).is_some_and(|r| r == "button" || r == "link");
        if acts && !has_name(doc, n) {
            out.push(issue(
                "interactive-name",
                format!("`{}` is clickable but has no visible text or alt text (the layer name is not exported)", n.name),
                "set_accessibility {alt:\"what it does\"} (and a role such as button or link)",
            ));
        }
        if let Some(level) = a.filter(|a| a.role.as_deref() == Some("heading")).map(|a| a.level.unwrap_or(2)) {
            if let Some(prev) = last_heading.filter(|prev| level > prev + 1) {
                out.push(issue(
                    "heading-skip",
                    format!("heading `{}` is h{level} after h{prev}", n.name),
                    "use the next level down, or add the missing heading",
                ));
            }
            last_heading = Some(level);
        }
        if let Some(message) = order_mismatch(doc, n) {
            out.push(issue("reading-order", message, "change the `order` values to follow the layout (top to bottom, then left to right), or move the layers"));
        }
    }
    out
}

/// A rect, ellipse or frame whose only fill is a picture: what the site export writes as `<img>`.
fn is_picture(n: &Node) -> bool {
    matches!(n.kind, NodeKind::Rect | NodeKind::Ellipse | NodeKind::Frame { .. })
        && matches!(n.style.fills.as_slice(), [Paint::Image { .. }])
}

/// Text inside or an alt text; the layer name does not reach the export.
fn has_name(doc: &Document, n: &Node) -> bool {
    n.a11y.as_ref().is_some_and(|a| a.alt.as_deref().is_some_and(|t| !t.is_empty()))
        || doc.subtree(n.id).iter().filter_map(|c| doc.get(*c)).any(|c| c.visible && c.kind.text().is_some_and(|t| !t.trim().is_empty()))
}

/// Children with an explicit order, read in that order, against where they sit on the page.
fn order_mismatch(doc: &Document, n: &Node) -> Option<String> {
    let mut kids: Vec<(u32, (i64, i64), NodeId)> = n
        .children
        .iter()
        .filter_map(|c| {
            let k = doc.get(*c).filter(|k| k.visible)?;
            let r = doc.world_rect(*c)?;
            Some((k.a11y.as_ref()?.order?, (r.y.round() as i64, r.x.round() as i64), *c))
        })
        .collect();
    if kids.len() < 2 {
        return None;
    }
    kids.sort_by_key(|k| k.0);
    if let Some(w) = kids.windows(2).find(|w| w[0].0 == w[1].0) {
        return Some(format!("two children of `{}` share `order` {}", n.name, w[0].0));
    }
    let mut visual = kids.clone();
    visual.sort_by_key(|k| k.1);
    let (first, _) = kids.iter().zip(&visual).find(|(a, b)| a.2 != b.2)?;
    Some(format!(
        "the reading order of `{}` disagrees with the layout (starting at `{}`)",
        n.name,
        doc.get(first.2).map_or("", |k| k.name.as_str())
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::Rect;
    use crate::model::A11y;

    fn add(doc: &mut Document, parent: Option<NodeId>, kind: NodeKind, r: (f64, f64), name: &str) -> NodeId {
        let id = doc.alloc_id();
        let mut n = Node::new(id, kind, Rect::new(r.0, r.1, 100.0, 40.0));
        n.name = name.into();
        n.parent = parent;
        doc.nodes.insert(id, n);
        match parent {
            Some(p) => doc.nodes.get_mut(&p).unwrap().children.push(id),
            None => doc.root.push(id),
        }
        id
    }

    fn set(doc: &mut Document, id: NodeId, a: A11y) {
        doc.nodes.get_mut(&id).unwrap().a11y = Some(a);
    }

    fn rules(doc: &Document) -> Vec<&'static str> {
        a11y(doc, &doc.paint_order()).iter().map(|i| i.rule).collect()
    }

    fn board(d: &mut Document) -> NodeId {
        add(d, None, NodeKind::Frame { clip: true }, (0.0, 0.0), "Board")
    }

    fn text(d: &mut Document, parent: NodeId, y: f64, label: &str) -> NodeId {
        let kind = NodeKind::Text { text: label.into(), font_size: 16.0, auto_width: true, align: Default::default() };
        add(d, Some(parent), kind, (0.0, y), label)
    }

    #[test]
    fn a_picture_needs_alt_text_or_a_decorative_mark() {
        let mut d = Document::default();
        let b = board(&mut d);
        let p = add(&mut d, Some(b), NodeKind::Rect, (0.0, 0.0), "Hero photo");
        d.nodes.get_mut(&p).unwrap().style.fills = vec![Paint::Image { asset: "a".into(), mode: Default::default(), opacity: 1.0 }];
        assert_eq!(rules(&d), ["image-alt"]);
        set(&mut d, p, A11y { alt: Some("A fish".into()), ..Default::default() });
        assert!(rules(&d).is_empty());
        set(&mut d, p, A11y { decorative: true, ..Default::default() });
        assert!(rules(&d).is_empty());
    }

    #[test]
    fn a_clickable_layer_needs_a_name() {
        let mut d = Document::default();
        let b = board(&mut d);
        let btn = add(&mut d, Some(b), NodeKind::Frame { clip: false }, (0.0, 0.0), "Frame 3");
        d.nodes.get_mut(&btn).unwrap().link = Some(b);
        assert_eq!(rules(&d), ["interactive-name"]);
        set(&mut d, btn, A11y { alt: Some("Close".into()), role: Some("button".into()), ..Default::default() });
        assert!(rules(&d).is_empty());
        set(&mut d, btn, A11y::default());
        text(&mut d, btn, 0.0, "Buy");
        assert!(rules(&d).is_empty(), "text inside names it");
    }

    #[test]
    fn a_button_role_needs_alt_or_text_whatever_the_layer_is_called() {
        let mut d = Document::default();
        let b = board(&mut d);
        let btn = add(&mut d, Some(b), NodeKind::Frame { clip: false }, (0.0, 0.0), "Close button");
        set(&mut d, btn, A11y { role: Some("button".into()), ..Default::default() });
        assert_eq!(rules(&d), ["interactive-name"]);
        set(&mut d, btn, A11y { role: Some("button".into()), alt: Some("Close".into()), ..Default::default() });
        assert!(rules(&d).is_empty());
    }

    #[test]
    fn siblings_may_not_share_a_reading_order() {
        let mut d = Document::default();
        let b = board(&mut d);
        let top = text(&mut d, b, 0.0, "Top");
        let bottom = text(&mut d, b, 100.0, "Bottom");
        for id in [top, bottom] {
            set(&mut d, id, A11y { order: Some(1), ..Default::default() });
        }
        assert_eq!(rules(&d), ["reading-order"]);
    }

    #[test]
    fn explicit_headings_may_not_skip_a_level() {
        let mut d = Document::default();
        let b = board(&mut d);
        let mut heading = |y, level| {
            let t = text(&mut d, b, y, "Title");
            set(&mut d, t, A11y { role: Some("heading".into()), level: Some(level), ..Default::default() });
        };
        heading(0.0, 1);
        heading(50.0, 2);
        heading(100.0, 2);
        assert!(rules(&d).is_empty());
        let t = text(&mut d, b, 150.0, "Deep");
        set(&mut d, t, A11y { role: Some("heading".into()), level: Some(4), ..Default::default() });
        assert_eq!(rules(&d), ["heading-skip"]);
    }

    #[test]
    fn an_explicit_reading_order_must_follow_the_layout() {
        let mut d = Document::default();
        let b = board(&mut d);
        let top = text(&mut d, b, 0.0, "Top");
        let bottom = text(&mut d, b, 100.0, "Bottom");
        let order = |o| A11y { order: Some(o), ..Default::default() };
        assert!(rules(&d).is_empty(), "no order set, nothing to compare");
        set(&mut d, top, order(1));
        set(&mut d, bottom, order(2));
        assert!(rules(&d).is_empty());
        set(&mut d, top, order(2));
        set(&mut d, bottom, order(1));
        assert_eq!(rules(&d), ["reading-order"]);
    }
}
