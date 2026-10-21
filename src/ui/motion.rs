//! Motion: easing curves and animated values over egui's animation manager.
//!
//! Every animation goes through [`progress`] / [`value`], so one switch ([`set_enabled`],
//! driven by `UiState::animations`) turns all motion off: disabled animations jump straight
//! to their target.

use egui::{Context, Id};

fn narrow_f32(v: f64) -> f32 {
    v as f32
}

/// Duration of hover fades and small state changes, in seconds.
pub const FAST: f32 = 0.12;
/// Duration of most transitions (tabs, tree, panels), in seconds.
pub const MEDIUM: f32 = 0.18;
/// Duration of larger transitions (palette, note switch), in seconds.
pub const SLOW: f32 = 0.26;

fn flag_id() -> Id {
    Id::new("papercraft-motion-enabled")
}

/// Turns all animations on or off for this context.
pub fn set_enabled(ctx: &Context, enabled: bool) {
    ctx.data_mut(|d| d.insert_temp(flag_id(), enabled));
}

/// Whether animations are on (default: on).
#[must_use]
pub fn enabled(ctx: &Context) -> bool {
    ctx.data(|d| d.get_temp::<bool>(flag_id())).unwrap_or(true)
}

/// Cubic ease-out: fast start, soft landing. `t` is clamped to `0..=1`.
#[must_use]
pub fn ease_out_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

/// Cubic ease-in-out. `t` is clamped to `0..=1`.
#[must_use]
pub fn ease_in_out_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
    }
}

/// Ease-out with a small overshoot (the "pop"): passes 1.0 then settles back. Clamped input.
#[must_use]
pub fn ease_out_back(t: f32) -> f32 {
    const C1: f32 = 1.70158;
    const C3: f32 = C1 + 1.0;
    let t = t.clamp(0.0, 1.0) - 1.0;
    1.0 + C3 * t * t * t + C1 * t * t
}

/// Ease-in with anticipation: dips below 0 first (the "wind-up"), then accelerates to 1.
/// Used as `1.0 - ease_in_back(t)` it grows a little before it shrinks away.
#[must_use]
pub fn ease_in_back(t: f32) -> f32 {
    const C1: f32 = 1.70158;
    const C3: f32 = C1 + 1.0;
    let t = t.clamp(0.0, 1.0);
    C3 * t * t * t - C1 * t * t
}

/// Linear interpolation.
#[must_use]
pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// An eased 0..=1 progress toward `target` (`true` = 1). Jumps when animations are off.
pub fn progress(
    ctx: &Context,
    id: impl std::hash::Hash + std::fmt::Debug,
    target: bool,
    secs: f32,
    easing: fn(f32) -> f32,
) -> f32 {
    if !enabled(ctx) {
        return if target { 1.0 } else { 0.0 };
    }
    ctx.animate_bool_with_time_and_easing(Id::new(id), target, secs, easing)
}

/// A value easing toward `target`. Jumps when animations are off.
pub fn value(
    ctx: &Context,
    id: impl std::hash::Hash + std::fmt::Debug,
    target: f32,
    secs: f32,
) -> f32 {
    if !enabled(ctx) {
        return target;
    }
    ctx.animate_value_with_time(Id::new(id), target, secs)
}

/// Spring stiffness (per second squared) of [`spring`]: higher snaps faster.
pub const SPRING_STIFFNESS: f32 = 320.0;
/// Spring damping of [`spring`]; this pair overshoots by about 8% and settles in ~0.5 s.
pub const SPRING_DAMPING: f32 = 22.0;

/// A value that chases `target` like a damped spring, so it overshoots and settles: the
/// bounce. Jumps to the target when animations are off; the first call starts at the target.
pub fn spring(ctx: &Context, id: impl std::hash::Hash + std::fmt::Debug, target: f32) -> f32 {
    if !enabled(ctx) {
        return target;
    }
    let key = Id::new(id).with("spring");
    let (mut pos, mut vel) = ctx
        .data(|d| d.get_temp::<(f32, f32)>(key))
        .unwrap_or((target, 0.0));
    let dt = ctx.input(|i| i.stable_dt).clamp(0.0, 1.0 / 20.0);
    // Fixed sub-steps keep the integration stable at any frame rate.
    let steps = 4;
    let h = dt / 4.0;
    for _ in 0..steps {
        let accel = SPRING_STIFFNESS * (target - pos) - SPRING_DAMPING * vel;
        vel += accel * h;
        pos += vel * h;
    }
    if (target - pos).abs() < 0.05 && vel.abs() < 0.5 {
        pos = target;
        vel = 0.0;
    } else {
        ctx.request_repaint();
    }
    ctx.data_mut(|d| d.insert_temp(key, (pos, vel)));
    pos
}

/// A one-shot wobble: when `trigger` is true the wobble restarts; afterwards it returns a
/// damped oscillation (starts at 0, swings, decays to 0 within `secs`). `0.0` when idle or
/// when animations are off.
pub fn wobble(
    ctx: &Context,
    id: impl std::hash::Hash + std::fmt::Debug,
    trigger: bool,
    secs: f32,
) -> f32 {
    if !enabled(ctx) {
        return 0.0;
    }
    let key = Id::new(id).with("wobble");
    let now = ctx.input(|i| i.time);
    if trigger {
        ctx.data_mut(|d| d.insert_temp(key, now));
    }
    let Some(start) = ctx.data(|d| d.get_temp::<f64>(key)) else {
        return 0.0;
    };
    let elapsed = now - start;
    if !(0.0..f64::from(secs)).contains(&elapsed) {
        return 0.0;
    }
    ctx.request_repaint();
    let t = elapsed / f64::from(secs);
    narrow_f32((-5.0 * t).exp() * (t * std::f64::consts::TAU * 2.5).sin())
}

/// An eased 0..=1 hover amount for `response`'s widget (fades in and out over [`FAST`]) and,
/// when the widget can be clicked, the pointing-hand cursor while hovered (a hover-only widget keeps
/// whatever cursor is below it). `id` must be unique per widget.
pub fn hover(
    ctx: &Context,
    id: impl std::hash::Hash + std::fmt::Debug,
    response: &egui::Response,
) -> f32 {
    if response.hovered() && response.sense.senses_click() {
        ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    progress(ctx, id, response.hovered(), FAST, ease_out_cubic)
}

/// Blends two colours by `t` (`0.0` = `a`, `1.0` = `b`).
#[must_use]
pub fn mix(a: egui::Color32, b: egui::Color32, t: f32) -> egui::Color32 {
    a.lerp_to_gamma(b, t.clamp(0.0, 1.0))
}

/// An eased 0..=1 "appear" progress for something that was not drawn on the previous pass:
/// it restarts from 0 each time the item (re)appears, then eases to 1 over `secs`. Returns 1
/// right away when animations are off.
pub fn appear(
    ctx: &Context,
    id: impl std::hash::Hash + std::fmt::Debug,
    secs: f32,
    easing: fn(f32) -> f32,
) -> f32 {
    if !enabled(ctx) {
        return 1.0;
    }
    let key = Id::new(id);
    let seen = key.with("seen-pass");
    let pass = ctx.cumulative_pass_nr();
    let last = ctx.data(|d| d.get_temp::<u64>(seen));
    ctx.data_mut(|d| d.insert_temp(seen, pass));
    let fresh = last.is_none_or(|last| pass > last.saturating_add(1));
    if fresh {
        // Restart: store 0 immediately, then ease up from there.
        let _ = ctx.animate_value_with_time(key, 0.0, 0.0);
    }
    easing(ctx.animate_value_with_time(key, 1.0, secs))
}

/// Seconds between the start of one list row's pop-in and the next.
const STAGGER: f32 = 0.035;
/// Rows past this index share the same delay, so long lists do not crawl.
const STAGGER_ROWS: usize = 8;
/// How far (points) a row drops while it pops in.
const POP_DROP: f32 = 14.0;

/// Draws a list row that pops in when it (re)appears: it fades in, drops into place with a
/// small bounce, and starts `index` steps after the first row. Stable `id` per row.
pub fn pop_in<R>(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    index: usize,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    if !enabled(ui.ctx()) {
        return add(ui);
    }
    let delay = STAGGER * f32::from(u8::try_from(index.min(STAGGER_ROWS)).unwrap_or(0));
    let raw = appear(ui.ctx(), id, delay + SLOW, |t| t);
    let t = ((raw * (delay + SLOW) - delay) / SLOW).clamp(0.0, 1.0);
    if t < 1.0 {
        ui.ctx().request_repaint();
    }
    let drop = (1.0 - ease_out_back(t)) * POP_DROP;
    let rect = egui::Rect::from_min_size(
        ui.cursor().min + egui::vec2(0.0, drop),
        egui::vec2(ui.available_width(), ui.available_height()),
    );
    let out = ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
        ui.multiply_opacity(ease_out_cubic(t));
        add(ui)
    });
    out.inner
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn easings_hit_their_endpoints_and_clamp() {
        for f in [ease_out_cubic, ease_in_out_cubic, ease_out_back] {
            assert!(f(0.0).abs() < 1e-6);
            assert!((f(1.0) - 1.0).abs() < 1e-6);
            assert!((f(-5.0) - f(0.0)).abs() < 1e-6, "clamped below");
            assert!((f(9.0) - f(1.0)).abs() < 1e-6, "clamped above");
            assert!(f(0.5).is_finite());
        }
    }

    #[test]
    fn ease_out_cubic_is_monotonic_and_front_loaded() {
        let mut last = 0.0;
        for i in 0..=20_u8 {
            let v = ease_out_cubic(f32::from(i) / 20.0);
            assert!(v >= last);
            last = v;
        }
        assert!(ease_out_cubic(0.3) > 0.3);
    }

    #[test]
    fn ease_in_back_winds_up_before_it_goes() {
        let low = (0..=100_u8)
            .map(|i| ease_in_back(f32::from(i) / 100.0))
            .fold(0.0_f32, f32::min);
        assert!(low < -0.05 && low > -0.3, "wind-up {low}");
        assert!((ease_in_back(1.0) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn spring_overshoots_then_settles_on_the_target() {
        let ctx = Context::default();
        let mut peak = 0.0_f32;
        let mut last = 0.0;
        assert_eq!(spring(&ctx, "s", 0.0), 0.0);
        for frame in 0..120 {
            let input = egui::RawInput {
                time: Some(10.0 + f64::from(frame) / 60.0),
                predicted_dt: 1.0 / 60.0,
                ..egui::RawInput::default()
            };
            let mut out = ctx.run_ui(input, |ui| {
                last = spring(ui.ctx(), "s", 100.0);
            });
            out.textures_delta.clear();
            peak = peak.max(last);
        }
        assert!(peak > 100.0 && peak < 125.0, "overshoot {peak}");
        assert!((last - 100.0).abs() < 1.0, "settled at {last}");
    }

    #[test]
    fn wobble_swings_and_returns_to_zero() {
        let ctx = Context::default();
        let mut seen = Vec::new();
        for frame in 0..40 {
            let input = egui::RawInput {
                time: Some(5.0 + f64::from(frame) * 0.02),
                predicted_dt: 0.02,
                ..egui::RawInput::default()
            };
            let mut out = ctx.run_ui(input, |ui| {
                seen.push(wobble(ui.ctx(), "w", frame == 0, 0.5));
            });
            out.textures_delta.clear();
        }
        assert!(seen.iter().any(|v| v.abs() > 0.2), "it swings");
        assert!(
            seen.iter().any(|v| *v < 0.0) && seen.iter().any(|v| *v > 0.0),
            "both directions"
        );
        assert_eq!(
            seen.last().copied(),
            Some(0.0),
            "idle again after {} s",
            0.5
        );
        set_enabled(&ctx, false);
        assert_eq!(wobble(&ctx, "w", true, 0.5), 0.0);
        assert_eq!(spring(&ctx, "x", 7.0), 7.0);
    }

    #[test]
    fn ease_out_back_overshoots_then_settles() {
        let peak = (0..=100_u8)
            .map(|i| ease_out_back(f32::from(i) / 100.0))
            .fold(0.0_f32, f32::max);
        assert!(peak > 1.02 && peak < 1.2, "overshoot {peak}");
    }

    #[test]
    fn disabled_motion_jumps_to_the_target() {
        let ctx = Context::default();
        set_enabled(&ctx, false);
        assert!(!enabled(&ctx));
        assert_eq!(progress(&ctx, "a", true, MEDIUM, ease_out_cubic), 1.0);
        assert_eq!(progress(&ctx, "a", false, MEDIUM, ease_out_cubic), 0.0);
        assert_eq!(value(&ctx, "b", 42.0, MEDIUM), 42.0);
        set_enabled(&ctx, true);
        assert!(enabled(&ctx));
    }

    #[test]
    fn appear_restarts_after_a_gap_and_jumps_when_off() {
        let ctx = Context::default();
        let first = appear(&ctx, "item", SLOW, ease_out_cubic);
        assert!((0.0..=1.0).contains(&first));
        set_enabled(&ctx, false);
        assert_eq!(appear(&ctx, "item", SLOW, ease_out_cubic), 1.0);
    }

    #[test]
    fn enabled_progress_starts_at_the_old_value() {
        let ctx = Context::default();
        let first = progress(&ctx, "fade", false, SLOW, ease_out_cubic);
        assert_eq!(first, 0.0);
        let next = progress(&ctx, "fade", true, SLOW, ease_out_cubic);
        assert!((0.0..=1.0).contains(&next));
    }
}
