//! Screens: frames declared as one device viewport of the product (not a section, slot or
//! component), and what inside one is hidden behind a drawer or modal ([`coverage`]), so
//! agents don't edit what nobody can see. Spec: `specs/screens/spec.md`.

use super::Editor;
use crate::geom::Rect;
use crate::model::{Document, Node, NodeId, NodeKind, Paint};

/// Device sizes a screen is usually declared at.
pub const DEVICES: [(&str, f64, f64); 4] = [
    ("Mobile", 390.0, 844.0),
    ("Tablet", 768.0, 1024.0),
    ("Desktop", 1440.0, 900.0),
    ("Large desktop", 2560.0, 1440.0),
];

/// A layer with less than this share of its area in view counts as hidden.
pub const HIDDEN_BELOW: f64 = 0.01;
/// A layer with at least this share in view counts as fully visible.
const VISIBLE_FROM: f64 = 0.99;
/// Fill alpha and opacity from which a layer hides what is under it.
const OPAQUE: f64 = 0.95;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Screen,
    Overlay,
    None,
}

/// The device whose width matches `w` (heights vary: screens scroll).
pub fn device_of(w: f64) -> Option<&'static str> {
    DEVICES.iter().find(|d| (d.1 - w).abs() < 0.5).map(|d| d.0)
}

/// Declared screens of the page, in paint order.
pub fn declared(doc: &Document) -> Vec<NodeId> {
    doc.paint_order().into_iter().filter(|id| doc.get(*id).is_some_and(|n| n.screen)).collect()
}

/// The declared screen `id` is inside (not `id` itself).
pub fn screen_of(doc: &Document, id: NodeId) -> Option<NodeId> {
    doc.ancestors(id).into_iter().find(|a| doc.get(*a).is_some_and(|n| n.screen))
}

/// Why `id` can't take `role`, if it can't.
pub fn refusal(doc: &Document, id: NodeId, role: Role) -> Option<&'static str> {
    let n = doc.get(id)?;
    let in_component = || n.component || n.instance.is_some() || doc.ancestors(id).iter().any(|a| doc.get(*a).is_some_and(|p| p.component));
    match role {
        Role::None => None,
        Role::Screen if !n.is_frame() => Some("only a frame can be a screen"),
        Role::Screen if in_component() => Some("a component main, an instance or a layer inside a main can't be a screen"),
        Role::Screen if screen_of(doc, id).is_some() => Some("it is already inside a screen"),
        Role::Screen if doc.subtree(id).iter().skip(1).any(|d| doc.get(*d).is_some_and(|c| c.screen)) => {
            Some("it holds screens (a section): declare the screens inside it")
        }
        Role::Screen => None,
        Role::Overlay if screen_of(doc, id).is_none() => Some("an overlay (drawer, modal, sheet) must be inside a screen"),
        Role::Overlay => None,
    }
}

/// Frames that look like screens: named `Screen / ...`, the frame shown in a `Slot / ...`
/// frame, or a top-level frame at exactly a device size (a tall section at a device width
/// is not one). The outermost of nested candidates wins,
/// and frames that can't be screens are left out.
pub fn candidates(doc: &Document) -> Vec<NodeId> {
    let looks = |n: &Node| {
        n.is_frame()
            && !n.screen
            && (n.name.starts_with("Screen /")
                || n.parent.and_then(|p| doc.get(p)).is_some_and(|p| p.name.starts_with("Slot /"))
                || (n.parent.is_none() && DEVICES.iter().any(|d| (d.1 - n.rect.w).abs() < 0.5 && (d.2 - n.rect.h).abs() < 0.5)))
    };
    let found: Vec<NodeId> = doc
        .paint_order()
        .into_iter()
        .filter(|id| doc.get(*id).is_some_and(looks) && refusal(doc, *id, Role::Screen).is_none())
        .collect();
    found.iter().copied().filter(|id| !doc.ancestors(*id).iter().any(|a| found.contains(a))).collect()
}

/// How much of a layer inside a screen is in view, and what covers the rest.
#[derive(Debug, Clone, PartialEq)]
pub struct Coverage {
    pub screen: NodeId,
    /// Share of the layer's area not covered, 0..=1.
    pub visible: f64,
    /// Layers painted over it inside the screen: overlays and opaque layers.
    pub covers: Vec<NodeId>,
}

impl Coverage {
    pub fn is_hidden(&self) -> bool {
        self.visible < HIDDEN_BELOW
    }
    pub fn is_partial(&self) -> bool {
        !self.is_hidden() && self.visible < VISIBLE_FROM
    }
    pub fn state(&self) -> &'static str {
        if self.is_hidden() {
            "hidden"
        } else if self.is_partial() {
            "partly covered"
        } else {
            "visible"
        }
    }
}

/// Coverage of `id` by layers painted after it in its screen. None outside a screen. Clipping
/// and scrolling don't count as covering: only layers on top do.
pub fn coverage(doc: &Document, id: NodeId) -> Option<Coverage> {
    let screen = screen_of(doc, id)?;
    let target = doc.world_rect(id)?;
    let screen_rect = doc.world_rect(screen)?;
    let mut order = Vec::new();
    paint_walk(doc, screen, &mut order);
    let at = order.iter().position(|x| *x == id)?;
    let mut covers = Vec::new();
    let mut rects = Vec::new();
    for &c in &order[at + 1..] {
        if doc.is_ancestor(id, c) || !shown(doc, c, screen) || !hides_below(doc, c, screen) {
            continue;
        }
        // ponytail: covers are clipped to the screen only, not to frames between; refine if
        // a clipped cover ever hides more than it shows.
        let Some(r) = doc.world_rect(c).and_then(|r| r.intersection(&screen_rect)).and_then(|r| r.intersection(&target)) else {
            continue;
        };
        covers.push(c);
        rects.push(r);
    }
    let area = target.w * target.h;
    let visible = if area <= 0.0 { 1.0 } else { (1.0 - union_area(&rects) / area).clamp(0.0, 1.0) };
    Some(Coverage { screen, visible, covers })
}

/// Parents before children, children bottom to top: the order layers are painted in.
fn paint_walk(doc: &Document, id: NodeId, out: &mut Vec<NodeId>) {
    out.push(id);
    for c in doc.get(id).map(|n| n.children.as_slice()).unwrap_or_default() {
        paint_walk(doc, *c, out);
    }
}

/// `id` and every ancestor up to `screen` are visible.
fn shown(doc: &Document, id: NodeId, screen: NodeId) -> bool {
    std::iter::once(id).chain(doc.ancestors(id).into_iter().take_while(|a| *a != screen)).all(|a| doc.get(a).is_some_and(|n| n.visible))
}

/// `id` hides what is under it: an overlay or inside one, or an opaque frame or rectangle.
fn hides_below(doc: &Document, id: NodeId, screen: NodeId) -> bool {
    let chain: Vec<NodeId> = std::iter::once(id).chain(doc.ancestors(id).into_iter().take_while(|a| *a != screen)).collect();
    if chain.iter().any(|a| doc.get(*a).is_some_and(|n| n.overlay)) {
        return true;
    }
    let Some(n) = doc.get(id) else { return false };
    let opacity: f64 = chain.iter().filter_map(|a| doc.get(*a)).map(|n| n.style.opacity).product();
    matches!(n.kind, NodeKind::Frame { .. } | NodeKind::Rect)
        && opacity >= OPAQUE
        && n.style.fills.iter().any(|f| match f {
            Paint::Solid { color, .. } => f64::from(color.a) / 255.0 >= OPAQUE,
            Paint::Image { opacity, .. } => *opacity >= OPAQUE,
            _ => false,
        })
}

/// Area covered by the union of `rects` (coordinate compression; covers are few).
fn union_area(rects: &[Rect]) -> f64 {
    let mut xs: Vec<f64> = rects.iter().flat_map(|r| [r.x, r.right()]).collect();
    let mut ys: Vec<f64> = rects.iter().flat_map(|r| [r.y, r.bottom()]).collect();
    xs.sort_by(f64::total_cmp);
    ys.sort_by(f64::total_cmp);
    let mut area = 0.0;
    for xw in xs.windows(2) {
        for yw in ys.windows(2) {
            let (cx, cy) = ((xw[0] + xw[1]) / 2.0, (yw[0] + yw[1]) / 2.0);
            if rects.iter().any(|r| r.x <= cx && cx <= r.right() && r.y <= cy && cy <= r.bottom()) {
                area += (xw[1] - xw[0]) * (yw[1] - yw[0]);
            }
        }
    }
    area
}

impl Editor {
    /// Declares `ids` as screens or overlays, or back to plain frames: one undo step. Errors
    /// name the first layer that can't take the role, and nothing changes then.
    pub fn set_role(&mut self, ids: &[NodeId], role: Role) -> Result<(), String> {
        if let Some((id, why)) = ids.iter().find_map(|id| refusal(&self.doc, *id, role).map(|w| (*id, w))) {
            let name = self.doc.get(id).map_or("?", |n| n.name.as_str());
            return Err(format!("`{name}` can't be a {}: {why}", if role == Role::Overlay { "overlay" } else { "screen" }));
        }
        let label = match role {
            Role::Screen => "Declare screens",
            Role::Overlay => "Mark overlays",
            Role::None => "Clear screen roles",
        };
        self.edit(label, |tx| {
            for id in ids {
                if let Some(n) = tx.node_mut(*id) {
                    n.screen = role == Role::Screen;
                    n.overlay = role == Role::Overlay;
                }
            }
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Color;

    fn frame(ed: &mut Editor, parent: Option<NodeId>, r: Rect, fill: Option<Color>) -> NodeId {
        ed.edit("Add", |tx| {
            let id = tx.alloc_id();
            let mut n = Node::new(id, NodeKind::Frame { clip: true }, r);
            n.style.fills = fill.map(Paint::solid).into_iter().collect();
            tx.insert(n, parent, None);
            id
        })
    }

    #[test]
    fn drawer_hides_what_is_under_it_and_partial_passes() {
        let mut ed = Editor::default();
        let screen = frame(&mut ed, None, Rect::new(0.0, 0.0, 400.0, 800.0), None);
        let under = frame(&mut ed, Some(screen), Rect::new(0.0, 0.0, 200.0, 100.0), None);
        let half = frame(&mut ed, Some(screen), Rect::new(100.0, 200.0, 200.0, 100.0), None);
        let drawer = frame(&mut ed, Some(screen), Rect::new(0.0, 0.0, 200.0, 800.0), Some(Color { r: 255, g: 255, b: 255, a: 255 }));
        assert!(coverage(&ed.doc, under).is_none(), "not a screen yet");
        ed.set_role(&[screen], Role::Screen).unwrap();
        let c = coverage(&ed.doc, under).unwrap();
        assert!(c.is_hidden() && c.covers == vec![drawer], "{c:?}");
        let c = coverage(&ed.doc, half).unwrap();
        assert!(c.is_partial() && (c.visible - 0.5).abs() < 1e-9, "{c:?}");
        assert_eq!(coverage(&ed.doc, drawer).unwrap().state(), "visible");
        // A translucent drawer hides only once it is marked an overlay.
        ed.edit("Fade", |tx| tx.node_mut(drawer).unwrap().style.opacity = 0.5);
        assert_eq!(coverage(&ed.doc, under).unwrap().state(), "visible");
        ed.set_role(&[drawer], Role::Overlay).unwrap();
        assert!(coverage(&ed.doc, under).unwrap().is_hidden());
        // Hidden drawer: nothing covered.
        ed.edit("Hide", |tx| tx.node_mut(drawer).unwrap().visible = false);
        assert_eq!(coverage(&ed.doc, under).unwrap().state(), "visible");
    }

    #[test]
    fn roles_are_refused_where_they_make_no_sense() {
        let mut ed = Editor::default();
        let section = frame(&mut ed, None, Rect::new(0.0, 0.0, 3000.0, 3000.0), None);
        let slot = frame(&mut ed, Some(section), Rect::new(0.0, 0.0, 1440.0, 924.0), None);
        let screen = frame(&mut ed, Some(slot), Rect::new(0.0, 24.0, 1440.0, 900.0), None);
        let inner = frame(&mut ed, Some(screen), Rect::new(0.0, 0.0, 10.0, 10.0), None);
        ed.edit("Name", |tx| tx.node_mut(slot).unwrap().name = "Slot / Home".into());
        assert_eq!(candidates(&ed.doc), vec![screen]);
        assert!(ed.set_role(&[inner], Role::Overlay).is_err(), "overlay outside a screen");
        ed.set_role(&[screen], Role::Screen).unwrap();
        assert!(ed.set_role(&[inner], Role::Screen).is_err(), "screen in a screen");
        assert!(ed.set_role(&[section], Role::Screen).is_err(), "a section holding screens");
        ed.set_role(&[inner], Role::Overlay).unwrap();
        assert_eq!(ed.screens(), vec![screen]);
        ed.undo();
        ed.undo();
        assert!(ed.doc.get(screen).is_some_and(|n| !n.screen), "one undo step each");
    }

    #[test]
    fn union_area_counts_overlaps_once() {
        let a = Rect::new(0.0, 0.0, 10.0, 10.0);
        let b = Rect::new(5.0, 5.0, 10.0, 10.0);
        assert!((union_area(&[a, b]) - 175.0).abs() < 1e-9);
    }
}
