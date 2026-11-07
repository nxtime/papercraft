//! What a finger needs from the editor: larger targets, a test for "would this touch grab
//! something", and a way to revert the drag a second finger interrupted.

use super::{Editor, Tool, hit};
use crate::geom::V2;
use crate::model::NodeId;

/// Finger-sized hit tolerances (screen px): a 44 px target is 22 px each way.
const TOUCH_RADIUS_PX: f64 = 22.0;
const MOUSE_HANDLE_RADIUS_PX: f64 = 7.0;
const MOUSE_HIT_SLOP_PX: f64 = 4.0;

impl Editor {
    pub(super) fn handle_radius(&self) -> f64 {
        if self.touch { TOUCH_RADIUS_PX } else { MOUSE_HANDLE_RADIUS_PX }
    }

    fn hit_slop(&self) -> f64 {
        if self.touch { TOUCH_RADIUS_PX } else { MOUSE_HIT_SLOP_PX }
    }

    /// The operand of boolean group `group` under a world point (double-click enters the group).
    pub(super) fn operand_at(&self, group: NodeId, world: V2) -> Option<NodeId> {
        hit::hit_operand(&self.doc, group, world, self.camera.zoom, self.hit_slop())
    }

    /// Topmost layer under a world point, with the tolerance of the current input device.
    pub(super) fn hit_at(&self, world: V2) -> Option<NodeId> {
        let slop = self.hit_slop();
        let zoom = self.camera.zoom;
        // Inside a boolean group that is being edited its operands are hit by their own
        // outlines, so a part the result cuts away can still be picked.
        self.selection
            .iter()
            .find_map(|s| self.doc.ancestors(*s).into_iter().find(|a| self.doc.get(*a).is_some_and(|n| n.is_boolean())))
            .and_then(|g| hit::hit_operand(&self.doc, g, world, zoom, slop))
            .or_else(|| hit::hit_test_slop(&self.doc, world, zoom, slop))
    }

    /// Whether a finger landing at `pos` (canvas px) should drag rather than pan: any tool but
    /// Select, a resize handle of the selection, or a selected layer (or part of one).
    pub fn touch_grabs(&mut self, pos: V2) -> bool {
        if self.tool != Tool::Select || self.text_edit.is_some() {
            return true;
        }
        self.touch = true;
        let on_handle = !self.selection_rotated() && self.selection_bounds().is_some_and(|b| {
            let bs = self.camera.world_rect_to_screen(&b);
            hit::handle_at_radius(&bs, pos, self.handle_radius()).is_some()
        });
        let grabbed = on_handle
            || self.hit_at(self.screen_to_world(pos)).is_some_and(|h| {
                self.selection.contains(&h) || self.doc.ancestors(h).iter().any(|a| self.selection.contains(a))
            });
        self.touch = false;
        grabbed
    }

    /// The layer under a finger at `pos` (canvas px), with finger-sized tolerance.
    pub fn touch_pick(&mut self, pos: V2) -> Option<NodeId> {
        self.touch = true;
        let hit = self.hit_at(self.screen_to_world(pos));
        self.touch = false;
        hit
    }

    /// Revert the drag, move, resize or marquee in progress without touching the history.
    pub fn cancel_drag(&mut self) {
        self.cancel_gesture();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::{Camera, Rect, v2};
    use crate::gestures::{Out, Recognizer};
    use crate::editor::{Button, InputEvent};
    use crate::model::{Node, NodeKind};

    fn ed() -> (Editor, NodeId) {
        let mut e = Editor { viewport: v2(390.0, 800.0), camera: Camera { pan: V2::ZERO, zoom: 1.0 }, ..Editor::default() };
        let a = e.add_node(Node::new(NodeId(0), NodeKind::Rect, Rect::new(100.0, 100.0, 100.0, 100.0)), None);
        (e, a)
    }

    /// What the web shell does with the recogniser's output.
    fn apply(e: &mut Editor, outs: Vec<Out>, t: f64) {
        e.touch = true;
        for o in outs {
            match o {
                Out::Down(pos) => e.handle(InputEvent::PointerDown { pos, button: Button::Primary, pressure: None, time_ms: t }),
                Out::Move(pos) => e.handle(InputEvent::PointerMove { pos, pressure: None, time_ms: t }),
                Out::Up(pos) => e.handle(InputEvent::PointerUp { pos, button: Button::Primary, time_ms: t }),
                Out::Cancel => e.cancel_drag(),
                Out::Pan(d) => e.handle(InputEvent::Scroll { delta: v2(-d.x, -d.y), pos: v2(0.0, 0.0) }),
                Out::Zoom { factor, pos } => e.handle(InputEvent::Zoom { factor, pos }),
                Out::Undo => e.undo(),
                Out::LongPress(_) => {}
            }
        }
        e.touch = false;
    }

    #[test]
    fn a_tap_selects_and_the_next_finger_on_it_moves_it() {
        let (mut e, a) = ed();
        let mut r = Recognizer::default();
        assert!(!e.touch_grabs(v2(150.0, 150.0)), "unselected: a drag there would pan");
        apply(&mut e, r.down(1, v2(150.0, 150.0), 0.0, false), 0.0);
        apply(&mut e, r.up(1, v2(150.0, 150.0), 60.0), 60.0);
        assert_eq!(e.selection, vec![a]);

        assert!(e.touch_grabs(v2(150.0, 150.0)));
        apply(&mut e, r.down(2, v2(150.0, 150.0), 1000.0, true), 1000.0);
        apply(&mut e, r.moved(2, v2(190.0, 170.0)), 1016.0);
        apply(&mut e, r.up(2, v2(190.0, 170.0), 1100.0), 1100.0);
        assert_eq!(e.doc.world_rect(a).unwrap().min(), v2(140.0, 120.0));
    }

    #[test]
    fn a_pinch_reverts_the_layer_the_first_finger_was_moving() {
        let (mut e, a) = ed();
        e.selection = vec![a];
        let undo_before = e.history.undo_len();
        let mut r = Recognizer::default();
        apply(&mut e, r.down(1, v2(150.0, 150.0), 0.0, true), 0.0);
        apply(&mut e, r.moved(1, v2(200.0, 200.0)), 16.0);
        assert_eq!(e.doc.world_rect(a).unwrap().min(), v2(150.0, 150.0), "mid-drag");
        apply(&mut e, r.down(2, v2(300.0, 200.0), 100.0, false), 100.0);
        assert_eq!(e.doc.world_rect(a).unwrap().min(), v2(100.0, 100.0), "put back");
        let zoom = e.camera.zoom;
        apply(&mut e, r.moved(2, v2(400.0, 200.0)), 130.0);
        assert!(e.camera.zoom > zoom, "the pinch zooms");
        apply(&mut e, r.up(1, v2(200.0, 200.0), 500.0), 500.0);
        apply(&mut e, r.up(2, v2(400.0, 200.0), 510.0), 510.0);
        assert_eq!(e.doc.world_rect(a).unwrap().min(), v2(100.0, 100.0));
        assert_eq!(e.history.undo_len(), undo_before, "nothing was left on the undo stack");
    }

    #[test]
    fn a_double_tap_on_text_edits_it() {
        let (mut e, _) = ed();
        let t = e.add_node(
            Node::new(NodeId(0), NodeKind::Text { text: "Hello world, a long line".into(), font_size: 16.0, auto_width: true, align: Default::default() }, Rect::new(300.0, 300.0, 80.0, 30.0)),
            None,
        );
        let mut r = Recognizer::default();
        for (id, t0) in [(1, 0.0), (2, 200.0)] {
            apply(&mut e, r.down(id, v2(340.0 + id as f64 * 2.0, 310.0), t0, false), t0);
            apply(&mut e, r.up(id, v2(340.0 + id as f64 * 2.0, 310.0), t0 + 50.0), t0 + 50.0);
        }
        assert_eq!(e.text_edit.as_ref().map(|t| t.id), Some(t));
    }

    #[test]
    fn handles_are_grabbed_from_a_finger_width_away() {
        let (mut e, a) = ed();
        e.selection = vec![a];
        // The bottom-right handle sits at (200, 200): 18 px off is a miss for a mouse, a hit for a finger.
        let near = v2(218.0, 218.0);
        let bs = e.camera.world_rect_to_screen(&e.selection_bounds().unwrap());
        assert!(hit::handle_at(&bs, near).is_none());
        assert!(e.touch_grabs(near));
    }
}
