//! Smart transitions in Present mode: layers that share a name (and kind) between the frame being
//! left and the one being shown move, resize, fade and recolour from their old look to their own.
//! The tween writes into the live document for its short run and puts every value back after, so
//! nothing it does is an edit or an undo step.

use crate::geom::Rect;
use crate::model::{Color, Document, NodeId, Paint};

/// How long a smart transition takes.
pub const SMART_SECS: f32 = 0.35;

/// What a tween moves: position and size (relative to the parent), opacity and first solid fill.
#[derive(Clone)]
struct Look {
    rect: Rect,
    opacity: f64,
    fill: Option<Color>,
}

pub struct Pair {
    id: NodeId,
    from: Look,
    to: Look,
    /// The layer's own fills, put back when the tween ends.
    fills: Vec<Paint>,
}

fn solid(fills: &[Paint]) -> Option<Color> {
    match fills.first() {
        Some(Paint::Solid { color, .. }) => Some(*color),
        _ => None,
    }
}

fn mix(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

fn mix_color(a: Color, b: Color, t: f64) -> Color {
    let c = |x: u8, y: u8| mix(f64::from(x), f64::from(y), t).round().clamp(0.0, 255.0) as u8;
    Color::rgba(c(a.r, b.r), c(a.g, b.g), c(a.b, b.b), c(a.a, b.a))
}

/// The layers of `to` that have a namesake of the same kind in `from`, with where each starts.
/// A name pairs with one layer only, in layer order, so repeated names pair first to first.
pub fn plan(doc: &Document, from: NodeId, to: NodeId) -> Vec<Pair> {
    let (Some(fr), Some(tr)) = (doc.world_rect(from), doc.world_rect(to)) else {
        return Vec::new();
    };
    let mut free: Vec<NodeId> = doc.subtree(from).into_iter().skip(1).collect();
    let mut out = Vec::new();
    for id in doc.subtree(to).into_iter().skip(1) {
        let Some(n) = doc.get(id) else { continue };
        let Some(at) = free.iter().position(|f| {
            doc.get(*f).is_some_and(|m| m.name == n.name && std::mem::discriminant(&m.kind) == std::mem::discriminant(&n.kind))
        }) else {
            continue;
        };
        let src = free.remove(at);
        let (Some(m), Some(sw)) = (doc.get(src), doc.world_rect(src)) else { continue };
        // The old world rect, moved to the new frame, then made relative to the new parent.
        let start = Rect::new(sw.x - fr.x + tr.x, sw.y - fr.y + tr.y, sw.w, sw.h);
        let parent = doc.parent_origin(n.parent);
        let from_look = Look {
            rect: Rect::new(start.x - parent.x, start.y - parent.y, start.w, start.h),
            opacity: m.style.opacity,
            fill: solid(&m.style.fills),
        };
        let to_look = Look { rect: n.rect, opacity: n.style.opacity, fill: solid(&n.style.fills) };
        let same = from_look.rect == to_look.rect && from_look.opacity == to_look.opacity && from_look.fill == to_look.fill;
        if !same {
            out.push(Pair { id, from: from_look, to: to_look, fills: n.style.fills.clone() });
        }
    }
    out
}

/// Sets every paired layer to `t` (0 = old look, 1 = its own, restoring its fills exactly).
/// Returns the layers it touched, for the renderer's change log.
pub fn apply(doc: &mut Document, pairs: &[Pair], t: f64) -> Vec<NodeId> {
    let mut touched = Vec::new();
    for p in pairs {
        let Some(n) = doc.nodes.get_mut(&p.id) else { continue };
        if t >= 1.0 {
            n.rect = p.to.rect;
            n.style.opacity = p.to.opacity;
            n.style.fills = p.fills.clone();
        } else {
            n.rect = Rect::new(
                mix(p.from.rect.x, p.to.rect.x, t),
                mix(p.from.rect.y, p.to.rect.y, t),
                mix(p.from.rect.w, p.to.rect.w, t),
                mix(p.from.rect.h, p.to.rect.h, t),
            );
            n.style.opacity = mix(p.from.opacity, p.to.opacity, t);
            if let (Some(a), Some(b)) = (p.from.fill, p.to.fill) {
                n.style.fills = vec![Paint::solid(mix_color(a, b, t))];
            }
        }
        touched.push(p.id);
    }
    touched
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Node, NodeKind};

    fn add(doc: &mut Document, parent: Option<NodeId>, name: &str, kind: NodeKind, rect: Rect, fill: Option<Color>) -> NodeId {
        let id = doc.alloc_id();
        let mut n = Node::new(id, kind, rect);
        n.name = name.into();
        n.parent = parent;
        if let Some(c) = fill {
            n.style.fills = vec![Paint::solid(c)];
        }
        doc.nodes.insert(id, n);
        doc.children_mut(parent).unwrap().push(id);
        id
    }

    fn two_frames() -> (Document, NodeId, NodeId, NodeId, NodeId) {
        let mut d = Document::default();
        let f1 = add(&mut d, None, "A", NodeKind::Frame { clip: true }, Rect::new(0.0, 0.0, 300.0, 300.0), None);
        let f2 = add(&mut d, None, "B", NodeKind::Frame { clip: true }, Rect::new(500.0, 0.0, 300.0, 300.0), None);
        let a = add(&mut d, Some(f1), "Card", NodeKind::Rect, Rect::new(10.0, 10.0, 50.0, 50.0), Some(Color::rgb(255, 0, 0)));
        let b = add(&mut d, Some(f2), "Card", NodeKind::Rect, Rect::new(100.0, 40.0, 150.0, 50.0), Some(Color::rgb(0, 0, 255)));
        add(&mut d, Some(f2), "Only here", NodeKind::Rect, Rect::new(0.0, 0.0, 5.0, 5.0), None);
        (d, f1, f2, a, b)
    }

    #[test]
    fn only_layers_with_a_namesake_tween_and_start_where_the_old_one_sat() {
        let (mut d, f1, f2, _, b) = two_frames();
        let plan = plan(&d, f1, f2);
        assert_eq!(plan.len(), 1, "`Only here` has no namesake");
        let before = d.get(b).unwrap().clone();
        apply(&mut d, &plan, 0.0);
        let n = d.get(b).unwrap();
        assert_eq!((n.rect.x, n.rect.y, n.rect.w), (10.0, 10.0, 50.0), "starts at the old spot in the new frame");
        assert_eq!(n.style.fill(), Some(Color::rgb(255, 0, 0)));
        apply(&mut d, &plan, 0.5);
        let n = d.get(b).unwrap();
        assert_eq!((n.rect.x, n.rect.w), (55.0, 100.0));
        assert_eq!(n.style.fill(), Some(Color::rgb(128, 0, 128)));
        apply(&mut d, &plan, 1.0);
        let n = d.get(b).unwrap();
        assert_eq!((n.rect, n.style.fills.clone()), (before.rect, before.style.fills), "put back exactly");
    }

    #[test]
    fn a_name_pairs_once_and_kinds_must_match() {
        let (mut d, f1, f2, ..) = two_frames();
        let f3 = add(&mut d, None, "C", NodeKind::Frame { clip: true }, Rect::new(900.0, 0.0, 10.0, 10.0), None);
        add(&mut d, Some(f3), "Card", NodeKind::Rect, Rect::new(1.0, 1.0, 5.0, 5.0), None);
        add(&mut d, Some(f3), "Card", NodeKind::Rect, Rect::new(2.0, 2.0, 5.0, 5.0), None);
        assert_eq!(plan(&d, f1, f3).len(), 1, "one `Card` in A feeds one in C");
        assert!(plan(&d, f1, f1).is_empty(), "same frame: nothing moves");
        let _ = f2;
    }
}
