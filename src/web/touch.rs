//! Touch input for the browser shell: fingers go through the gesture recogniser instead of
//! acting as a mouse. Mouse and pen events never come here.

use super::{Web, now_ms, window};
use crate::editor::shortcuts::Action;
use crate::editor::{Button, InputEvent};
use crate::geom::{V2, v2};
use crate::gestures::{Out, Recognizer};
use web_sys::PointerEvent;

#[derive(Default)]
pub(super) struct Touch {
    rec: Recognizer,
    /// Fingers the recogniser owns; the others are pressing the interface.
    canvas_ids: Vec<i32>,
}

impl Web {
    fn time(&self) -> f64 {
        now_ms() - self.start_ms
    }

    /// Whether a point is on the sheet, the tool strip or a menu rather than the bare canvas.
    /// egui's own hover state is a frame behind a finger that has just landed.
    fn touch_on_chrome(&self, p: egui::Pos2) -> bool {
        self.st.phone.menu.is_some() || self.st.phone.rects.iter().any(|(_, r)| r.contains(p))
    }

    pub(super) fn touch_event(&mut self, e: &PointerEvent, pressed: Option<bool>) {
        let p = egui::pos2(e.offset_x() as f32, e.offset_y() as f32);
        let id = e.pointer_id();
        let t = self.time();
        self.pointer = Some(p);
        self.dirty = true;
        let pos = self.to_canvas(p);
        let owned = self.touch.canvas_ids.contains(&id);
        let outs = match pressed {
            Some(true) if self.canvas_rect.contains(p) && !self.touch_on_chrome(p) => {
                self.ctx.memory_mut(|m| {
                    if let Some(f) = m.focused() {
                        m.surrender_focus(f)
                    }
                });
                self.touch.canvas_ids.push(id);
                let _ = self.canvas.set_pointer_capture(id);
                let grab = self.st.phone.select_mode || self.ed.touch_grabs(pos);
                self.touch.rec.down(id, pos, t, grab)
            }
            Some(true) => {
                self.egui_touch(p, true);
                Vec::new()
            }
            Some(false) if owned => {
                self.touch.canvas_ids.retain(|i| *i != id);
                self.touch.rec.up(id, pos, t)
            }
            Some(false) => {
                self.egui_touch(p, false);
                self.events.push(egui::Event::PointerGone);
                Vec::new()
            }
            None if owned => self.touch.rec.moved(id, pos),
            None => {
                self.events.push(egui::Event::PointerMoved(p));
                Vec::new()
            }
        };
        self.apply_touch(outs, t);
    }

    fn egui_touch(&mut self, p: egui::Pos2, pressed: bool) {
        self.events.push(egui::Event::PointerMoved(p));
        self.events.push(egui::Event::PointerButton {
            pos: p,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: self.egui_mods(),
        });
    }

    /// The browser took the finger (scrolling, a system gesture): drop what it started.
    pub(super) fn touch_cancel(&mut self, e: &PointerEvent) {
        if e.pointer_type() != "touch" {
            return;
        }
        self.touch.canvas_ids.clear();
        let outs = self.touch.rec.cancel();
        let t = self.time();
        self.apply_touch(outs, t);
        self.events.push(egui::Event::PointerGone);
        self.dirty = true;
    }

    /// Fires a long-press that no event has reported yet.
    pub(super) fn touch_tick(&mut self, now: f64) {
        if !self.touch.rec.active() {
            return;
        }
        let t = now - self.start_ms;
        let outs = self.touch.rec.tick(t);
        if !outs.is_empty() {
            self.apply_touch(outs, t);
            self.dirty = true;
        }
        // Keep the frame loop awake so the timer can expire while the fingers are still.
        self.dirty |= self.touch.rec.active();
    }

    fn apply_touch(&mut self, outs: Vec<Out>, t: f64) {
        self.ed.touch = true;
        for out in outs {
            match out {
                Out::Down(pos) => self.ed.handle(InputEvent::PointerDown {
                    pos,
                    button: Button::Primary,
                    pressure: None,
                    time_ms: t,
                }),
                Out::Move(pos) => self.ed.handle(InputEvent::PointerMove { pos, pressure: None, time_ms: t }),
                Out::Up(pos) => self.ed.handle(InputEvent::PointerUp { pos, button: Button::Primary, time_ms: t }),
                Out::Cancel => self.ed.cancel_drag(),
                Out::Pan(d) => self.ed.handle(InputEvent::Scroll { delta: v2(-d.x, -d.y), pos: V2::ZERO }),
                Out::Zoom { factor, pos } => self.ed.handle(InputEvent::Zoom { factor, pos }),
                Out::Undo => self.ed.run(Action::Undo),
                Out::LongPress(pos) => self.long_press(pos),
            }
        }
        self.ed.touch = false;
        self.captured = !self.touch.canvas_ids.is_empty();
    }

    fn long_press(&mut self, pos: V2) {
        let Some(id) = self.ed.touch_pick(pos) else { return };
        if !self.ed.selection.contains(&id) {
            self.ed.select(vec![id]);
        }
        let at = egui::pos2(pos.x as f32 + self.canvas_rect.min.x, pos.y as f32 + self.canvas_rect.min.y);
        self.st.phone.menu = Some((at, id, self.ed.doc_stamp));
    }

    /// Safe-area insets (a hidden element padded with `env()`) and the soft keyboard's height.
    pub(super) fn read_insets(&mut self) {
        let w = window();
        let inset = |side: &str| -> f32 {
            w.document()
                .and_then(|d| d.get_element_by_id("safe-area"))
                .and_then(|el| w.get_computed_style(&el).ok().flatten())
                .and_then(|s| s.get_property_value(&format!("padding-{side}")).ok())
                .and_then(|v| v.trim_end_matches("px").parse::<f32>().ok())
                .unwrap_or(0.0)
        };
        self.st.phone.safe = [inset("top"), inset("right"), inset("bottom"), inset("left")];
        let height = w.inner_height().ok().and_then(|v| v.as_f64()).unwrap_or(0.0);
        self.st.phone.keyboard = w
            .visual_viewport()
            .map_or(0.0, |vv| (height - vv.height() - vv.offset_top()).max(0.0) as f32);
    }
}
