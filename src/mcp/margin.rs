//! CSS margin for `write_html`. The layout model has no margin field, so a margin becomes
//! what already expresses it: an offset for a free or absolute box, and for a box in auto layout
//! a transparent wrapper whose padding is the margin and whose alignment is `auto`.

use super::css::{self, Styles};
use crate::geom::Rect;
use crate::history::Tx;
use crate::model::{Align, AutoLayout, Direction, Node, NodeId, NodeKind, Sizing};

/// Top, right, bottom, left in px, and which sides are `auto`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Margins {
    pub px: [f64; 4],
    pub auto: [bool; 4],
}

impl Margins {
    pub fn is_empty(&self) -> bool {
        *self == Margins::default()
    }
}

const SIDES: [&str; 4] = ["Top", "Right", "Bottom", "Left"];

fn side(v: &str, fs: f64, warn: &mut Vec<String>) -> (f64, bool) {
    let v = css::substitute_vars(v).0;
    let v = v.trim();
    if v == "auto" {
        return (0.0, true);
    }
    match css::length(v, fs) {
        Some(px) if px >= 0.0 => (px, false),
        // Padding cannot be negative; a pulled-in box is not expressible.
        Some(_) => {
            warn.push("margin: negative values".into());
            (0.0, false)
        }
        None => {
            warn.push(format!("margin: {v}"));
            (0.0, false)
        }
    }
}

/// The margins in `styles` (later declarations win), or None when there are none. Values the
/// model cannot hold (negative, percentages) are added to `warn` as ignored styles.
pub fn parse(styles: &Styles, fs: f64, warn: &mut Vec<String>) -> Option<Margins> {
    let mut m = Margins::default();
    let mut seen = false;
    let put = |m: &mut Margins, i: usize, (px, auto): (f64, bool)| {
        m.px[i] = px;
        m.auto[i] = auto;
    };
    for (k, v) in styles {
        // Before parsing: other properties' values (colours, `rotate()`) are not margins to warn about.
        if !k.starts_with("margin") {
            continue;
        }
        let parts = css::split_ws(&css::substitute_vars(v).0);
        let vals: Vec<(f64, bool)> = parts.iter().map(|p| side(p, fs, warn)).collect();
        let [a, rest @ ..] = vals.as_slice() else { continue };
        let b = rest.first().copied().unwrap_or(*a);
        match k.as_str() {
            "margin" => {
                let (c, d) = (rest.get(1).copied().unwrap_or(*a), rest.get(2).copied().unwrap_or(b));
                for (i, s) in [*a, b, c, d].into_iter().enumerate() {
                    put(&mut m, i, s);
                }
            }
            "marginInline" => {
                put(&mut m, 3, *a);
                put(&mut m, 1, b);
            }
            "marginBlock" => {
                put(&mut m, 0, *a);
                put(&mut m, 2, b);
            }
            _ => match k.strip_prefix("margin").and_then(|s| SIDES.iter().position(|x| *x == s)) {
                Some(i) => put(&mut m, i, *a),
                None => continue,
            },
        }
        seen = true;
    }
    (seen && !m.is_empty()).then_some(m)
}

/// Apply `m` to the freshly inserted node `id` and return the node that now stands in its place.
/// `far[axis]` is true when the box is positioned from its right / bottom edge.
///
/// Margins along the flow of a container are turned into gaps and spacers earlier, in
/// `rewrite.rs`, where block siblings collapse as in a browser; what reaches here is the margin
/// across the flow, on an absolute box, or on content written into an existing layer.
pub fn apply(tx: &mut Tx, id: NodeId, m: Margins, parent: Option<NodeId>, in_flow: bool, far: [bool; 2]) -> NodeId {
    let [t, r, b, l] = m.px;
    if !in_flow {
        if let Some(n) = tx.node_mut(id) {
            n.rect.x += if far[0] { -r } else { l };
            n.rect.y += if far[1] { -b } else { t };
        }
        return id;
    }
    let Some(child) = tx.doc().get(id).cloned() else { return id };
    let sizing = child.layout_child.sizing;
    // `auto` takes the free space only where the box does not already fill it.
    let aligned = |axis: usize, (s, e): (usize, usize)| match (m.auto[s] && sizing[axis] != Sizing::Fill, m.auto[e] && sizing[axis] != Sizing::Fill) {
        (true, true) => (Align::Center, true),
        (true, false) => (Align::End, true),
        (false, true) => (Align::Start, true),
        _ => (Align::Start, false),
    };
    let ((main, grow_x), (cross, grow_y)) = (aligned(0, (3, 1)), aligned(1, (0, 2)));
    let wrap_sizing = [
        if sizing[0] == Sizing::Fill || grow_x { Sizing::Fill } else { Sizing::Hug },
        if sizing[1] == Sizing::Fill || grow_y { Sizing::Fill } else { Sizing::Hug },
    ];
    let rect = Rect::new(child.rect.x - l, child.rect.y - t, child.rect.w + l + r, child.rect.h + t + b);
    let mut w = Node::new(NodeId(0), NodeKind::Frame { clip: false }, rect);
    w.id = tx.alloc_id();
    w.style.fills.clear();
    w.name = format!("{} (margin)", child.name);
    w.layout = Some(AutoLayout {
        direction: Direction::Horizontal,
        gap: 0.0,
        cross_gap: 0.0,
        padding: [t, r, b, l],
        align_main: main,
        align_cross: cross,
        ..AutoLayout::default()
    });
    w.layout_child.sizing = wrap_sizing;
    w.layout_child.align_self = child.layout_child.align_self;
    let pos = parent.and_then(|p| tx.doc().children(Some(p)).iter().position(|c| *c == id));
    let wid = tx.insert(w, parent, pos);
    tx.reparent(id, Some(wid), Some(0));
    if let Some(n) = tx.node_mut(id) {
        n.layout_child.align_self = None;
    }
    crate::layout::layout_frame(tx, wid);
    wid
}
