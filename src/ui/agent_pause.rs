//! Pause and resume controls for external MCP clients, drawn over the canvas corner.
//!
//! The shell fills [`UiState::pause_chips`] each frame and applies the keys pushed to
//! [`UiState::pause_toggles`]; holding the calls back is `crate::pause`.

use super::{UiState, kit, theme};
use egui::Align2;

/// Longest client name shown, so a long one cannot push the control off the canvas.
const LABEL_MAX: usize = 24;
const MARGIN: f32 = 14.0;

/// The words on a client's control.
pub fn text(label: &str, paused: bool, waiting: usize) -> String {
    let label: String = if label.chars().count() > LABEL_MAX {
        label.chars().take(LABEL_MAX - 1).chain(std::iter::once('\u{2026}')).collect()
    } else {
        label.to_string()
    };
    match (paused, waiting) {
        (false, _) => format!("Pause {label}"),
        (true, 0) => format!("{label} paused \u{b7} Resume"),
        (true, n) => format!("{label} paused \u{b7} {n} waiting \u{b7} Resume"),
    }
}

/// One control per external client that called lately, top right of the canvas.
pub fn chips(ctx: &egui::Context, st: &mut UiState, canvas: egui::Rect) {
    if st.pause_chips.is_empty() {
        return;
    }
    let warning = theme::pal_from_ctx(ctx).warning;
    let mut clicked = Vec::new();
    egui::Area::new("agent-pause".into())
        .order(egui::Order::Middle)
        .pivot(Align2::RIGHT_TOP)
        .fixed_pos(canvas.right_top() + egui::vec2(-MARGIN, MARGIN))
        .show(ctx, |ui| {
            ui.with_layout(egui::Layout::top_down(egui::Align::Max), |ui| {
                for c in &st.pause_chips {
                    let ink = c.paused.then_some(warning);
                    let hint = if c.paused { "Let this agent edit again" } else { "Hold this agent's edits until you resume it; it can still read" };
                    if kit::chip_ink(ui, &text(&c.label, c.paused, c.waiting), ink).on_hover_text(hint).clicked() {
                        clicked.push(c.key.clone());
                    }
                }
            });
        });
    st.pause_toggles.extend(clicked);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_control_says_what_a_click_does_and_stays_short() {
        assert_eq!(text("Claude Code", false, 3), "Pause Claude Code");
        assert_eq!(text("Claude Code", true, 0), "Claude Code paused \u{b7} Resume");
        assert_eq!(text("Claude Code", true, 2), "Claude Code paused \u{b7} 2 waiting \u{b7} Resume");
        let long = text(&"x".repeat(100), false, 0);
        assert_eq!(long.chars().count(), "Pause ".len() + LABEL_MAX);
    }
}
