//! Offscreen-layer effects on the canvas: rounded clips, layer and background blur, and the blend
//! modes that read the backdrop. Skipped when no GPU adapter exists.

mod common;

use common::*;

use papercraft::geom::{Rect, v2};
use papercraft::model::{BlendMode, Color, Node, NodeId, NodeKind};

fn rect(e: &mut papercraft::editor::Editor, r: Rect, c: Color, parent: Option<NodeId>) -> NodeId {
    let mut n = Node::new(NodeId(0), NodeKind::Rect, r);
    n.style.set_fill(Some(c));
    e.add_node(n, parent)
}

fn clip_frame(e: &mut papercraft::editor::Editor, r: Rect, radius: f64, parent: Option<NodeId>) -> NodeId {
    let mut f = Node::new(NodeId(0), NodeKind::Frame { clip: true }, r);
    f.style.corner_radius = radius;
    e.add_node(f, parent)
}

#[test]
fn rounded_clip_hides_the_corner_and_keeps_the_center() {
    let Some(g) = gpu() else { return };
    let mut e = editor(400.0, 300.0);
    let f = clip_frame(&mut e, Rect::new(100.0, 50.0, 200.0, 200.0), 80.0, None);
    rect(&mut e, Rect::new(0.0, 0.0, 200.0, 200.0), Color::BLACK, Some(f));
    let img = render_canvas(g, &e);
    assert!(close(px(&img, v2(200.0, 150.0)), Color::BLACK, 3));
    assert!(!close(px(&img, v2(103.0, 53.0)), Color::BLACK, 30), "corner must be clipped");
    // Along an edge, past the corner arc, the child is still drawn.
    assert!(close(px(&img, v2(200.0, 52.0)), Color::BLACK, 3));
}

#[test]
fn per_corner_radii_clip_only_their_corner() {
    let Some(g) = gpu() else { return };
    let mut e = editor(400.0, 300.0);
    let f = clip_frame(&mut e, Rect::new(100.0, 50.0, 200.0, 200.0), 0.0, None);
    e.edit("radii", |tx| {
        if let Some(n) = tx.node_mut(f) {
            n.style.radii = Some([80.0, 0.0, 0.0, 0.0]);
        }
    });
    rect(&mut e, Rect::new(0.0, 0.0, 200.0, 200.0), Color::BLACK, Some(f));
    let img = render_canvas(g, &e);
    assert!(!close(px(&img, v2(103.0, 53.0)), Color::BLACK, 30));
    assert!(close(px(&img, v2(297.0, 53.0)), Color::BLACK, 3));
}

#[test]
fn nested_rounded_clips_both_apply() {
    let Some(g) = gpu() else { return };
    let mut e = editor(400.0, 300.0);
    let outer = clip_frame(&mut e, Rect::new(50.0, 50.0, 300.0, 200.0), 60.0, None);
    let inner = clip_frame(&mut e, Rect::new(100.0, 0.0, 200.0, 200.0), 0.0, Some(outer));
    rect(&mut e, Rect::new(-100.0, 0.0, 400.0, 200.0), Color::BLACK, Some(inner));
    let img = render_canvas(g, &e);
    assert!(close(px(&img, v2(200.0, 150.0)), Color::BLACK, 3));
    // Left of the inner clip: gone. The outer frame's far corner is rounded away.
    assert!(!close(px(&img, v2(120.0, 150.0)), Color::BLACK, 30));
    assert!(!close(px(&img, v2(347.0, 52.0)), Color::BLACK, 30));
}

#[test]
fn layer_blur_softens_an_edge() {
    let Some(g) = gpu() else { return };
    let mut e = editor(400.0, 300.0);
    let sharp = rect(&mut e, Rect::new(100.0, 100.0, 100.0, 100.0), Color::BLACK, None);
    let crisp = render_canvas(g, &e);
    assert!(close(px(&crisp, v2(100.5, 150.0)), Color::BLACK, 20) || is_dark(px(&crisp, v2(101.0, 150.0))));
    e.edit("blur", |tx| {
        if let Some(n) = tx.node_mut(sharp) {
            n.style.blur = 8.0;
        }
    });
    let img = render_canvas(g, &e);
    let edge = px(&img, v2(100.0, 150.0));
    assert!(edge[0] > 60 && edge[0] < 200, "edge should be mid gray, got {edge:?}");
    assert!(close(px(&img, v2(150.0, 150.0)), Color::BLACK, 12));
}

#[test]
fn background_blur_blurs_only_inside_the_glass() {
    let Some(g) = gpu() else { return };
    let mut e = editor(400.0, 300.0);
    for i in 0..10 {
        let c = if i % 2 == 0 { Color::BLACK } else { Color::WHITE };
        rect(&mut e, Rect::new(i as f64 * 40.0, 0.0, 40.0, 300.0), c, None);
    }
    let before = render_canvas(g, &e);
    let glass = clip_frame(&mut e, Rect::new(100.0, 100.0, 200.0, 100.0), 0.0, None);
    e.edit("clear", |tx| {
        if let Some(n) = tx.node_mut(glass) {
            n.style.set_fill(None);
        }
    });
    e.edit("glass", |tx| {
        if let Some(n) = tx.node_mut(glass) {
            n.style.backdrop_blur = 20.0;
        }
    });
    let img = render_canvas(g, &e);
    // The stripe edge at x = 160 is blurred inside the glass and untouched outside it.
    let inside = px(&img, v2(160.0, 150.0));
    assert!(inside[0] > 60 && inside[0] < 200, "got {inside:?}");
    assert_eq!(px(&img, v2(160.0, 50.0)), px(&before, v2(160.0, 50.0)));
}

#[test]
fn overlay_and_dodge_read_the_backdrop() {
    let Some(g) = gpu() else { return };
    let mut e = editor(400.0, 300.0);
    rect(&mut e, Rect::new(0.0, 0.0, 400.0, 300.0), Color::rgb(64, 64, 64), None);
    let top = rect(&mut e, Rect::new(100.0, 100.0, 100.0, 100.0), Color::rgb(204, 204, 204), None);
    let set = |e: &mut papercraft::editor::Editor, b: BlendMode, c: Color| {
        e.edit("blend", |tx| {
            if let Some(n) = tx.node_mut(top) {
                n.style.blend = b;
                n.style.set_fill(Some(c));
            }
        });
    };
    set(&mut e, BlendMode::Overlay, Color::rgb(204, 204, 204));
    let overlay = px(&render_canvas(g, &e), v2(150.0, 150.0));
    // Overlay of source 0.8 on backdrop 0.25 is 2 * 0.25 * 0.8 = 0.4 -> 102.
    assert!((overlay[0] as i32 - 102).abs() <= 4, "overlay {overlay:?}");
    set(&mut e, BlendMode::ColorDodge, Color::rgb(128, 128, 128));
    let dodge = px(&render_canvas(g, &e), v2(150.0, 150.0));
    // 0.25 / (1 - 0.5) = 0.5 -> 128.
    assert!((dodge[0] as i32 - 128).abs() <= 4, "dodge {dodge:?}");
}

fn turn(e: &mut papercraft::editor::Editor, id: NodeId, deg: f64) {
    e.edit("rotate", |tx| {
        if let Some(n) = tx.node_mut(id) {
            n.style.rotation = deg;
        }
    });
}

#[test]
fn a_rotated_rect_is_drawn_turned_around_its_centre() {
    let Some(g) = gpu() else { return };
    let mut e = editor(400.0, 300.0);
    let r = rect(&mut e, Rect::new(150.0, 100.0, 100.0, 100.0), Color::BLACK, None);
    turn(&mut e, r, 45.0);
    let img = render_canvas(g, &e);
    assert!(close(px(&img, v2(200.0, 150.0)), Color::BLACK, 3));
    // The diamond's top point (200, 79) is covered; the square's old corner (152, 102) is not.
    assert!(close(px(&img, v2(200.0, 84.0)), Color::BLACK, 3));
    assert!(!close(px(&img, v2(153.0, 103.0)), Color::BLACK, 30));
}

#[test]
fn a_rotated_frame_carries_its_children_and_clips_to_its_outline() {
    let Some(g) = gpu() else { return };
    let mut e = editor(400.0, 300.0);
    let f = clip_frame(&mut e, Rect::new(100.0, 50.0, 200.0, 200.0), 0.0, None);
    rect(&mut e, Rect::new(-50.0, -50.0, 300.0, 300.0), Color::BLACK, Some(f));
    turn(&mut e, f, 45.0);
    let img = render_canvas(g, &e);
    assert!(close(px(&img, v2(200.0, 150.0)), Color::BLACK, 3));
    // Inside the diamond, past the square's top edge; outside it at the square's corner.
    assert!(close(px(&img, v2(200.0, 20.0)), Color::BLACK, 3));
    assert!(!close(px(&img, v2(106.0, 56.0)), Color::BLACK, 30), "the clip turns with the frame");
}

#[test]
fn a_rotated_blurred_node_keeps_its_centre_and_loses_its_old_corner() {
    let Some(g) = gpu() else { return };
    let mut e = editor(400.0, 300.0);
    let r = rect(&mut e, Rect::new(150.0, 100.0, 100.0, 100.0), Color::BLACK, None);
    turn(&mut e, r, 45.0);
    e.edit("blur", |tx| {
        if let Some(n) = tx.node_mut(r) {
            n.style.blur = 4.0;
        }
    });
    let img = render_canvas(g, &e);
    assert!(close(px(&img, v2(200.0, 150.0)), Color::BLACK, 12));
    assert!(!close(px(&img, v2(153.0, 103.0)), Color::BLACK, 60));
}

#[test]
fn a_boolean_group_paints_its_result_not_its_operands() {
    let Some(g) = gpu() else { return };
    let mut e = editor(400.0, 300.0);
    let a = rect(&mut e, Rect::new(50.0, 50.0, 200.0, 200.0), Color::BLACK, None);
    let b = rect(&mut e, Rect::new(150.0, 150.0, 200.0, 100.0), Color::rgb(255, 0, 0), None);
    e.select(vec![a, b]);
    e.boolean(papercraft::model::BoolOp::Subtract);
    let img = render_canvas(g, &e);
    assert!(close(px(&img, v2(100.0, 100.0)), Color::BLACK, 3), "the minuend's fill paints the result");
    assert!(!close(px(&img, v2(200.0, 200.0)), Color::BLACK, 30), "the cut-out is empty");
    assert!(!close(px(&img, v2(300.0, 200.0)), Color::rgb(255, 0, 0), 30), "the subtrahend is not drawn");
}
