//! `box-shadow` falloff matches CSS (Gaussian, sigma = blur / 2) and is cut out under the box.

use papercraft::geom::{Rect, V2, v2};
use papercraft::model::{Color, Node, NodeId, NodeKind, Shadow};
use papercraft::render::draw::{DrawOp, PaintSpec, node_draw};

/// Composited shadow alpha at `q` from the node's fill ops (shadow color only).
fn alpha_at(ops: &[DrawOp], q: V2) -> f64 {
    let clear: f64 = ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::Fill {
                path,
                paint: PaintSpec::Solid(c),
                even_odd,
            } if c[0] == 1.0 => path
                .contains(q, *even_odd, 0.1)
                .then_some(1.0 - c[3] as f64),
            _ => None,
        })
        .product();
    1.0 - clear
}

fn glow(x: f64, y: f64) -> Vec<DrawOp> {
    let mut n = Node::new(NodeId(1), NodeKind::Rect, Rect::new(0.0, 0.0, 200.0, 100.0));
    n.style.corner_radius = 16.0;
    // White glow, so the box's own (absent) fill can't be mistaken for shadow.
    n.style.shadow = Some(Shadow {
        x,
        y,
        blur: 40.0,
        spread: 0.0,
        color: Color::rgba(255, 255, 255, 128),
    });
    node_draw(&n, n.rect, 1.0).ops
}

#[test]
fn centered_glow_follows_css_falloff_and_skips_the_box() {
    let ops = glow(0.0, 0.0);
    let a = |d: f64| alpha_at(&ops, v2(100.0, 100.0 + d));
    let full = 128.0 / 255.0;
    assert!(
        (a(0.5) - full * 0.5).abs() < 0.06,
        "half the alpha at the edge: {}",
        a(0.5)
    );
    assert!(a(20.0) < full * 0.2, "one sigma out is faint: {}", a(20.0));
    assert!(
        a(10.0) > a(20.0) && a(20.0) > a(30.0),
        "fades monotonically"
    );
    assert!(a(42.0) < 0.01, "gone past the blur radius: {}", a(42.0));
    assert_eq!(
        alpha_at(&ops, v2(100.0, 50.0)),
        0.0,
        "nothing under the box"
    );
}

#[test]
fn offset_shadow_is_cut_out_under_the_box() {
    let ops = glow(0.0, 24.0);
    assert_eq!(
        alpha_at(&ops, v2(100.0, 90.0)),
        0.0,
        "nothing under the box"
    );
    assert!(
        alpha_at(&ops, v2(100.0, 110.0)) > 0.2,
        "shadow below the box"
    );
}
