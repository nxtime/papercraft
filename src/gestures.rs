//! Touch gesture recogniser: turns raw finger down/move/up events (canvas px, ms) into the
//! editor's pointer events plus pan, pinch-zoom, undo and long-press. It knows nothing about the
//! browser or the document: the shell says at touch-down whether the finger grabs something
//! (a selected layer, a handle, or any tool other than Select) and applies the outputs.
//!
//! One finger that grabs behaves like a mouse. One finger that does not pans; a tap (no
//! movement) is a click, and a second tap close in time and place is a double-click. Two fingers
//! pan and zoom about their midpoint and cancel whatever the first finger had started.

use crate::geom::{V2, v2};

/// A finger that moves less than this (px) is still tapping.
pub const TAP_SLOP: f64 = 10.0;
/// Two taps this close (px) and this soon (ms) after each other are a double-tap. The editor
/// treats clicks under 400 ms and 5 px apart as double clicks, so the second tap is moved onto
/// the first one's position.
pub const DOUBLE_TAP_DIST: f64 = 30.0;
pub const DOUBLE_TAP_MS: f64 = 350.0;
/// A finger held still this long (ms) is a long-press.
pub const LONG_PRESS_MS: f64 = 500.0;
/// Two fingers that land and lift within this time (ms) without moving are a two-finger tap.
pub const TWO_FINGER_TAP_MS: f64 = 300.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Out {
    /// Primary-button pointer events for the editor.
    Down(V2),
    Move(V2),
    Up(V2),
    /// Revert the drag the editor has in progress (a second finger arrived, or a long-press).
    Cancel,
    /// Pan the canvas by this many px (the content follows the fingers).
    Pan(V2),
    Zoom { factor: f64, pos: V2 },
    Undo,
    LongPress(V2),
}

#[derive(Clone, Copy, Debug)]
struct Finger {
    id: i32,
    start: V2,
    pos: V2,
}

#[derive(Clone, Copy, Debug, Default)]
enum Phase {
    #[default]
    Idle,
    One {
        grab: bool,
        /// The editor has a pointer-down that is still waiting for its up or cancel.
        down_sent: bool,
        moved: bool,
        long_fired: bool,
        start_ms: f64,
        /// Where the last pan delta was taken from.
        anchor: V2,
    },
    Two {
        start_ms: f64,
        moved: bool,
        mid: V2,
        dist: f64,
    },
    /// A gesture that ended early (long-press, pinch with a finger lifted): ignore the
    /// remaining fingers until all are up.
    Spent,
}

#[derive(Debug, Default)]
pub struct Recognizer {
    fingers: Vec<Finger>,
    phase: Phase,
    /// Where and when (ms) the editor last saw a tap's pointer-down.
    last_tap: Option<(V2, f64)>,
}

fn mid_and_dist(a: V2, b: V2) -> (V2, f64) {
    (v2((a.x + b.x) / 2.0, (a.y + b.y) / 2.0), a.dist(b))
}

impl Recognizer {
    /// A finger lands. `grab`: it would move, resize or draw rather than pan.
    pub fn down(&mut self, id: i32, pos: V2, t: f64, grab: bool) -> Vec<Out> {
        if self.fingers.iter().any(|f| f.id == id) {
            return vec![];
        }
        self.fingers.push(Finger { id, start: pos, pos });
        let mut out = vec![];
        match self.phase {
            Phase::Idle => {
                let at = self.snap_to_last_tap(pos, t);
                let down_sent = grab;
                if grab {
                    out.push(Out::Down(at));
                    self.last_tap = None;
                }
                self.phase = Phase::One { grab, down_sent, moved: false, long_fired: false, start_ms: t, anchor: pos };
            }
            Phase::One { down_sent, .. } => {
                if down_sent {
                    out.push(Out::Cancel);
                }
                let (a, b) = (self.fingers[0].pos, self.fingers[1].pos);
                let (mid, dist) = mid_and_dist(a, b);
                self.last_tap = None;
                self.phase = Phase::Two { start_ms: t, moved: false, mid, dist };
            }
            Phase::Two { .. } | Phase::Spent => {}
        }
        out
    }

    pub fn moved(&mut self, id: i32, pos: V2) -> Vec<Out> {
        let Some(i) = self.fingers.iter().position(|f| f.id == id) else {
            return vec![];
        };
        self.fingers[i].pos = pos;
        let mut out = vec![];
        match &mut self.phase {
            Phase::One { grab, moved, long_fired, anchor, .. } if !*long_fired => {
                let start = self.fingers[0].start;
                if !*moved && pos.dist(start) > TAP_SLOP {
                    *moved = true;
                }
                if *moved {
                    if *grab {
                        out.push(Out::Move(pos));
                    } else {
                        out.push(Out::Pan(v2(pos.x - anchor.x, pos.y - anchor.y)));
                        *anchor = pos;
                    }
                }
            }
            Phase::Two { moved, mid, dist, start_ms: _ } if self.fingers.len() >= 2 => {
                let (m, d) = mid_and_dist(self.fingers[0].pos, self.fingers[1].pos);
                if m.dist(*mid) > 0.0 || d != *dist {
                    let first = self.fingers[0];
                    let second = self.fingers[1];
                    if m.dist(v2(
                        (first.start.x + second.start.x) / 2.0,
                        (first.start.y + second.start.y) / 2.0,
                    )) > TAP_SLOP
                        || (d - first.start.dist(second.start)).abs() > TAP_SLOP
                    {
                        *moved = true;
                    }
                    out.push(Out::Pan(v2(m.x - mid.x, m.y - mid.y)));
                    if *dist > 0.0 && d > 0.0 {
                        out.push(Out::Zoom { factor: d / *dist, pos: m });
                    }
                    *mid = m;
                    *dist = d;
                }
            }
            _ => {}
        }
        out
    }

    pub fn up(&mut self, id: i32, pos: V2, t: f64) -> Vec<Out> {
        let Some(i) = self.fingers.iter().position(|f| f.id == id) else {
            return vec![];
        };
        self.fingers.remove(i);
        let mut out = vec![];
        let all_up = self.fingers.is_empty();
        match self.phase {
            Phase::One { grab, moved, long_fired, .. } if !long_fired => {
                if grab {
                    out.push(Out::Up(pos));
                    if !moved {
                        self.last_tap = Some((pos, t));
                    }
                } else if !moved {
                    // A tap: click now, at the first tap's spot when this is the second one.
                    let at = self.snap_to_last_tap(pos, t);
                    let double = at != pos;
                    out.push(Out::Down(at));
                    out.push(Out::Up(at));
                    self.last_tap = if double { None } else { Some((pos, t)) };
                }
                self.phase = Phase::Idle;
            }
            Phase::Two { start_ms, moved, .. } => {
                // Fingers rarely lift together: the first lift decides.
                if !moved && t - start_ms < TWO_FINGER_TAP_MS {
                    out.push(Out::Undo);
                }
                self.phase = if all_up { Phase::Idle } else { Phase::Spent };
            }
            _ => {}
        }
        if all_up {
            self.phase = Phase::Idle;
        }
        out
    }

    /// The system took the touch away: drop everything, reverting a drag in progress.
    pub fn cancel(&mut self) -> Vec<Out> {
        let sent = matches!(self.phase, Phase::One { down_sent: true, long_fired: false, .. });
        self.fingers.clear();
        self.phase = Phase::Idle;
        if sent { vec![Out::Cancel] } else { vec![] }
    }

    /// Call every frame; fires the long-press.
    pub fn tick(&mut self, t: f64) -> Vec<Out> {
        let Phase::One { down_sent, moved: false, long_fired: false, start_ms, grab, anchor } = self.phase else {
            return vec![];
        };
        if t - start_ms < LONG_PRESS_MS {
            return vec![];
        }
        let pos = self.fingers[0].pos;
        self.phase = Phase::One { grab, down_sent: false, moved: false, long_fired: true, start_ms, anchor };
        let mut out = vec![];
        if down_sent {
            out.push(Out::Cancel);
        }
        out.push(Out::LongPress(pos));
        out
    }

    /// Whether a finger is down.
    pub fn active(&self) -> bool {
        !self.fingers.is_empty()
    }

    fn snap_to_last_tap(&self, pos: V2, t: f64) -> V2 {
        match self.last_tap {
            Some((p, lt)) if t - lt < DOUBLE_TAP_MS && p.dist(pos) < DOUBLE_TAP_DIST => p,
            _ => pos,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: f64, y: f64) -> V2 {
        v2(x, y)
    }

    #[test]
    fn a_tap_on_empty_canvas_clicks_without_panning() {
        let mut r = Recognizer::default();
        assert!(r.down(1, p(50.0, 60.0), 0.0, false).is_empty(), "nothing until it is known to be a tap");
        assert!(r.moved(1, p(53.0, 62.0)).is_empty(), "under the slop");
        let out = r.up(1, p(53.0, 62.0), 80.0);
        assert_eq!(out, vec![Out::Down(p(53.0, 62.0)), Out::Up(p(53.0, 62.0))]);
        assert!(!r.active());
    }

    #[test]
    fn a_second_tap_lands_on_the_first_so_the_editor_sees_a_double_click() {
        let mut r = Recognizer::default();
        r.down(1, p(100.0, 100.0), 0.0, false);
        r.up(1, p(100.0, 100.0), 60.0);
        r.down(2, p(112.0, 104.0), 200.0, false);
        let out = r.up(2, p(112.0, 104.0), 260.0);
        assert_eq!(out, vec![Out::Down(p(100.0, 100.0)), Out::Up(p(100.0, 100.0))]);
        // A third tap right after is a fresh single tap, not a triple.
        r.down(3, p(100.0, 100.0), 300.0, false);
        let out = r.up(3, p(100.0, 100.0), 340.0);
        assert_eq!(out[0], Out::Down(p(100.0, 100.0)));
    }

    #[test]
    fn taps_far_apart_in_time_or_space_stay_single() {
        let mut r = Recognizer::default();
        r.down(1, p(100.0, 100.0), 0.0, false);
        r.up(1, p(100.0, 100.0), 50.0);
        r.down(2, p(100.0, 100.0), 900.0, false);
        assert_eq!(r.up(2, p(100.0, 100.0), 950.0)[0], Out::Down(p(100.0, 100.0)));
        r.down(3, p(300.0, 300.0), 1000.0, false);
        assert_eq!(r.up(3, p(300.0, 300.0), 1040.0)[0], Out::Down(p(300.0, 300.0)));
    }

    #[test]
    fn one_finger_on_empty_canvas_pans() {
        let mut r = Recognizer::default();
        r.down(1, p(100.0, 100.0), 0.0, false);
        let out = r.moved(1, p(100.0, 130.0));
        assert_eq!(out, vec![Out::Pan(p(0.0, 30.0))], "the whole move from the start, no lag");
        assert_eq!(r.moved(1, p(105.0, 140.0)), vec![Out::Pan(p(5.0, 10.0))]);
        assert!(r.up(1, p(105.0, 140.0), 200.0).is_empty(), "a pan is not a click");
    }

    #[test]
    fn one_finger_on_a_selected_layer_drags_like_a_mouse() {
        let mut r = Recognizer::default();
        assert_eq!(r.down(1, p(10.0, 10.0), 0.0, true), vec![Out::Down(p(10.0, 10.0))]);
        assert!(r.moved(1, p(12.0, 12.0)).is_empty(), "jitter under the slop moves nothing");
        assert_eq!(r.moved(1, p(40.0, 40.0)), vec![Out::Move(p(40.0, 40.0))]);
        assert_eq!(r.up(1, p(40.0, 40.0), 100.0), vec![Out::Up(p(40.0, 40.0))]);
    }

    #[test]
    fn holding_still_fires_a_long_press_and_swallows_the_release() {
        let mut r = Recognizer::default();
        r.down(1, p(70.0, 80.0), 0.0, false);
        assert!(r.tick(400.0).is_empty());
        assert_eq!(r.tick(500.0), vec![Out::LongPress(p(70.0, 80.0))]);
        assert!(r.tick(900.0).is_empty(), "fires once");
        assert!(r.up(1, p(70.0, 80.0), 950.0).is_empty(), "no click after a long-press");
    }

    #[test]
    fn long_press_on_a_grabbed_layer_reverts_the_press_first() {
        let mut r = Recognizer::default();
        r.down(1, p(70.0, 80.0), 0.0, true);
        assert_eq!(r.tick(520.0), vec![Out::Cancel, Out::LongPress(p(70.0, 80.0))]);
        assert!(r.up(1, p(70.0, 80.0), 600.0).is_empty());
    }

    #[test]
    fn moving_before_the_timer_means_no_long_press() {
        let mut r = Recognizer::default();
        r.down(1, p(70.0, 80.0), 0.0, false);
        r.moved(1, p(70.0, 120.0));
        assert!(r.tick(800.0).is_empty());
    }

    #[test]
    fn two_fingers_pinch_about_their_midpoint() {
        let mut r = Recognizer::default();
        r.down(1, p(100.0, 200.0), 0.0, false);
        r.down(2, p(200.0, 200.0), 10.0, false);
        let out = r.moved(2, p(300.0, 200.0));
        assert_eq!(
            out,
            vec![Out::Pan(p(50.0, 0.0)), Out::Zoom { factor: 2.0, pos: p(200.0, 200.0) }],
            "distance 100 to 200 about the new midpoint"
        );
    }

    #[test]
    fn two_fingers_moving_together_pan() {
        let mut r = Recognizer::default();
        r.down(1, p(100.0, 200.0), 0.0, false);
        r.down(2, p(200.0, 200.0), 10.0, false);
        let mut out = r.moved(1, p(100.0, 240.0));
        out.extend(r.moved(2, p(200.0, 240.0)));
        let pan: f64 = out.iter().map(|o| if let Out::Pan(d) = o { d.y } else { 0.0 }).sum();
        let zoom: f64 = out.iter().map(|o| if let Out::Zoom { factor, .. } = o { *factor } else { 1.0 }).product();
        assert!((pan - 40.0).abs() < 1e-9, "{out:?}");
        assert!((zoom - 1.0).abs() < 1e-9, "fingers moving as one do not zoom: {out:?}");
    }

    #[test]
    fn a_pinch_cancels_the_drag_the_first_finger_started() {
        let mut r = Recognizer::default();
        r.down(1, p(10.0, 10.0), 0.0, true);
        r.moved(1, p(50.0, 50.0));
        assert_eq!(r.down(2, p(150.0, 50.0), 100.0, false), vec![Out::Cancel]);
        // Lifting the fingers afterwards sends no pointer-up for the reverted drag.
        let out = [r.up(1, p(50.0, 50.0), 400.0), r.up(2, p(150.0, 50.0), 410.0)].concat();
        assert!(!out.iter().any(|o| matches!(o, Out::Up(_) | Out::Down(_))), "{out:?}");
    }

    #[test]
    fn a_quick_two_finger_tap_undoes_once() {
        let mut r = Recognizer::default();
        r.down(1, p(100.0, 200.0), 0.0, false);
        r.down(2, p(200.0, 200.0), 20.0, false);
        let a = r.up(2, p(200.0, 200.0), 120.0);
        let b = r.up(1, p(100.0, 200.0), 140.0);
        assert_eq!([a, b].concat(), vec![Out::Undo], "the second lift adds nothing");
    }

    #[test]
    fn a_slow_or_moving_two_finger_touch_does_not_undo() {
        let mut r = Recognizer::default();
        r.down(1, p(100.0, 200.0), 0.0, false);
        r.down(2, p(200.0, 200.0), 20.0, false);
        let a = r.up(2, p(200.0, 200.0), 900.0);
        let b = r.up(1, p(100.0, 200.0), 910.0);
        assert!([a, b].concat().is_empty());

        r.down(1, p(100.0, 200.0), 2000.0, false);
        r.down(2, p(200.0, 200.0), 2010.0, false);
        r.moved(2, p(260.0, 200.0));
        let a = r.up(2, p(260.0, 200.0), 2050.0);
        let b = r.up(1, p(100.0, 200.0), 2060.0);
        assert!(!a.iter().chain(b.iter()).any(|o| *o == Out::Undo));
    }

    #[test]
    fn cancel_reverts_a_grabbed_drag_and_forgets_the_fingers() {
        let mut r = Recognizer::default();
        r.down(1, p(10.0, 10.0), 0.0, true);
        assert_eq!(r.cancel(), vec![Out::Cancel]);
        assert!(!r.active());
        assert!(r.cancel().is_empty());
    }
}
