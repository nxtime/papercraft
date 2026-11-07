//! Rotating the selection: a knob above the only selected layer turns it around its centre.
//! Rotation is visual only (see `model::rotation`), so a drag changes `style.rotation` and
//! nothing else.

use super::{Editor, Gesture, V2};

/// How far above the top edge the knob sits, in screen px: past the quick-connect knobs (22 px).
const KNOB_OFFSET_PX: f64 = 44.0;
/// Shift snaps the angle to multiples of this many degrees.
pub const SNAP_STEP_DEG: f64 = 15.0;

/// The angle of the vector from `centre` to `p`, in degrees (clockwise, y down).
fn angle_deg(centre: V2, p: V2) -> f64 {
    (p.y - centre.y).atan2(p.x - centre.x).to_degrees()
}

/// `deg` folded into [-180, 180) and rounded to hundredths, so drags leave tidy values.
fn tidy(deg: f64) -> f64 {
    let folded = deg - 360.0 * ((deg + 180.0) / 360.0).floor();
    (folded * 100.0).round() / 100.0
}

impl Editor {
    /// Whether any selected layer is turned; resize handles are for upright boxes only.
    pub fn selection_rotated(&self) -> bool {
        self.top_selection()
            .iter()
            .any(|id| self.doc.get(*id).is_some_and(|n| n.style.rotation != 0.0))
    }

    /// The only selected layer and its outline centre as drawn, when it can be rotated.
    fn rotatable(&self) -> Option<(crate::model::NodeId, V2)> {
        if self.text_edit.is_some() {
            return None;
        }
        let ids = self.top_selection();
        let [id] = ids[..] else { return None };
        if self.doc.is_effectively_locked(id) {
            return None;
        }
        let q = self.doc.world_outline(id)?;
        Some((id, (q[0] + q[2]) * 0.5))
    }

    /// World position of the rotate knob: past the middle of the top edge, turned with the layer.
    pub fn rotate_knob(&self) -> Option<V2> {
        let (id, centre) = self.rotatable()?;
        let q = self.doc.world_outline(id)?;
        let top = (q[0] + q[1]) * 0.5;
        Some(top + (top - centre).normalized() * (KNOB_OFFSET_PX / self.camera.zoom))
    }

    /// Starts a rotation when `pos` (canvas px) is on the knob.
    pub(super) fn start_rotate(&mut self, pos: V2, world: V2) -> bool {
        let (Some((id, centre)), Some(knob)) = (self.rotatable(), self.rotate_knob()) else {
            return false;
        };
        if self.camera.world_to_screen(knob).dist(pos) > self.handle_radius() {
            return false;
        }
        let base = self.doc.get(id).map_or(0.0, |n| n.style.rotation);
        self.begin("Rotate");
        self.gesture = Gesture::Rotate { id, centre, from: angle_deg(centre, world), base };
        true
    }

    /// Follows the pointer: the layer turns by the angle the pointer swept around the centre.
    pub(super) fn rotate_to(&mut self, id: crate::model::NodeId, centre: V2, from: f64, base: f64, world: V2) {
        let mut deg = base + angle_deg(centre, world) - from;
        if self.mods.shift {
            deg = (deg / SNAP_STEP_DEG).round() * SNAP_STEP_DEG;
        }
        let deg = tidy(deg);
        let mut tx = self.tx();
        if let Some(n) = tx.node_mut(id) {
            n.style.rotation = deg;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::{Button, InputEvent, Modifiers};
    use crate::geom::{Camera, Rect, v2};
    use crate::model::{Color, Node, NodeId, NodeKind};

    fn editor_with_rect() -> (Editor, NodeId) {
        let mut e = Editor {
            viewport: v2(1000.0, 800.0),
            camera: Camera { pan: V2::ZERO, zoom: 1.0 },
            ..Editor::default()
        };
        let mut n = Node::new(NodeId(0), NodeKind::Rect, Rect::new(100.0, 100.0, 200.0, 100.0));
        n.style.set_fill(Some(Color::BLACK));
        let id = e.add_node(n, None);
        e.select(vec![id]);
        (e, id)
    }

    fn pointer(e: &mut Editor, ev: fn(V2) -> InputEvent, p: V2) {
        e.handle(ev(p));
    }

    fn down(p: V2) -> InputEvent {
        InputEvent::PointerDown { pos: p, button: Button::Primary, pressure: None, time_ms: 0.0 }
    }
    fn moved(p: V2) -> InputEvent {
        InputEvent::PointerMove { pos: p, pressure: None, time_ms: 20.0 }
    }
    fn up(p: V2) -> InputEvent {
        InputEvent::PointerUp { pos: p, button: Button::Primary, time_ms: 40.0 }
    }

    #[test]
    fn dragging_the_knob_turns_the_layer_in_one_undo_step() {
        let (mut e, id) = editor_with_rect();
        let knob = e.rotate_knob().unwrap();
        assert_eq!(knob, v2(200.0, 100.0 - KNOB_OFFSET_PX));
        pointer(&mut e, down, knob);
        // The pointer sweeps from straight above the centre (200, 150) to straight right of it.
        pointer(&mut e, moved, v2(400.0, 150.0));
        pointer(&mut e, up, v2(400.0, 150.0));
        assert_eq!(e.doc.get(id).unwrap().style.rotation, 90.0);
        assert_eq!(e.doc.get(id).unwrap().rect, Rect::new(100.0, 100.0, 200.0, 100.0), "the box stays upright");
        e.undo();
        assert_eq!(e.doc.get(id).unwrap().style.rotation, 0.0);
    }

    #[test]
    fn shift_snaps_to_fifteen_degrees() {
        let (mut e, id) = editor_with_rect();
        let knob = e.rotate_knob().unwrap();
        e.handle(InputEvent::Modifiers(Modifiers { shift: true, ..Modifiers::NONE }));
        pointer(&mut e, down, knob);
        // About 40 degrees clockwise of straight up.
        let p = v2(200.0 + 100.0 * 40f64.to_radians().sin(), 150.0 - 100.0 * 40f64.to_radians().cos());
        pointer(&mut e, moved, p);
        pointer(&mut e, up, p);
        assert_eq!(e.doc.get(id).unwrap().style.rotation, 45.0);
    }

    #[test]
    fn a_turned_layer_hides_resize_handles_and_the_knob_follows_it() {
        let (mut e, id) = editor_with_rect();
        e.edit("turn", |tx| {
            if let Some(n) = tx.node_mut(id) {
                n.style.rotation = 90.0;
            }
        });
        assert!(e.selection_rotated());
        // A quarter turn: the top edge is now the right edge, 50 px from the centre.
        let k = e.rotate_knob().unwrap();
        assert!((k.x - (250.0 + KNOB_OFFSET_PX)).abs() < 1e-9 && (k.y - 150.0).abs() < 1e-9, "{k:?}");
    }

    fn turned(e: &mut Editor, id: NodeId, deg: f64) {
        e.edit("turn", |tx| {
            if let Some(n) = tx.node_mut(id) {
                n.style.rotation = deg;
            }
        });
    }

    #[test]
    fn a_click_hits_the_turned_outline_and_not_the_old_box() {
        let (mut e, id) = editor_with_rect();
        turned(&mut e, id, 90.0);
        // Turned, the 200x100 box covers x 150..250, y 50..250.
        assert_eq!(crate::editor::hit::hit_test(&e.doc, v2(200.0, 70.0), 1.0), Some(id));
        assert_eq!(crate::editor::hit::hit_test(&e.doc, v2(120.0, 150.0), 1.0), None);
        // Inside the bounding box of a 45 degree turn, but past the diamond's edge.
        turned(&mut e, id, 45.0);
        assert_eq!(crate::editor::hit::hit_test(&e.doc, v2(200.0, 150.0), 1.0), Some(id));
        assert_eq!(crate::editor::hit::hit_test(&e.doc, v2(100.0, 50.0), 1.0), None);
    }

    #[test]
    fn a_marquee_selects_by_the_bounds_of_the_turned_layer() {
        let (mut e, id) = editor_with_rect();
        turned(&mut e, id, 90.0);
        e.select(Vec::new());
        // Touches y 230..300, which only the turned box (y 50..250) reaches.
        pointer(&mut e, down, v2(400.0, 300.0));
        pointer(&mut e, moved, v2(200.0, 230.0));
        pointer(&mut e, up, v2(200.0, 230.0));
        assert_eq!(e.selection, vec![id]);
    }

    #[test]
    fn angles_fold_into_half_open_range() {
        assert_eq!(tidy(190.0), -170.0);
        assert_eq!(tidy(-180.0), -180.0);
        assert_eq!(tidy(360.0), 0.0);
    }
}
