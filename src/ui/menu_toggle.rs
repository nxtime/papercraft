//! The toolbar's menu button: shows or hides the File / Edit / ... strip on platforms without a
//! native menu bar (macOS has its own). The choice is kept across runs.

use crate::theme;
use egui::{Response, Sense, Stroke, Ui, Vec2};

/// Matches the "All projects" button next to it.
const SIZE: f32 = 26.0;
/// Half the length of a bar, and the gap from the middle bar to the outer ones.
const BAR_HALF: f32 = 6.0;
const BAR_GAP: f32 = 4.0;
const BAR_STROKE: f32 = 1.25;
const HIDDEN: &str = "hidden";

fn path() -> Option<std::path::PathBuf> {
    crate::project::config_dir().map(|d| d.join("menu_bar"))
}

/// `"hidden"` hides the strip; no file, or anything else, shows it.
fn parse_hidden(s: &str) -> bool {
    s.trim() == HIDDEN
}

pub fn load_hidden() -> bool {
    path().and_then(|p| crate::vfs::read_to_string(p).ok()).is_some_and(|s| parse_hidden(&s))
}

pub fn save_hidden(hidden: bool) {
    let Some(p) = path() else { return };
    if let Some(dir) = p.parent() {
        let _ = crate::vfs::create_dir_all(dir);
    }
    let _ = crate::vfs::write(p, if hidden { HIDDEN } else { "shown" });
}

/// The three-bar button. Clicking toggles the strip: the caller flips and saves the choice.
pub fn button(ui: &mut Ui, hidden: bool) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(SIZE), Sense::click());
    if ui.is_rect_visible(rect) {
        let p = theme::pal(ui);
        let hover = crate::ui::motion::hover(ui.ctx(), resp.id.with("hover"), &resp);
        let fill = if resp.is_pointer_button_down_on() { p.pressed } else { p.hover.gamma_multiply(hover) };
        ui.painter().rect_filled(rect, theme::radius_md(), fill);
        let stroke = Stroke::new(BAR_STROKE, p.text);
        let c = rect.center();
        for dy in [-BAR_GAP, 0.0, BAR_GAP] {
            ui.painter().hline(c.x - BAR_HALF..=c.x + BAR_HALF, c.y + dy, stroke);
        }
    }
    resp.on_hover_text(if hidden { "Show the menu bar" } else { "Hide the menu bar" })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_word_hidden_hides_the_strip() {
        assert!(parse_hidden("hidden"));
        assert!(parse_hidden("hidden\n"), "a trailing newline is fine");
        assert!(!parse_hidden("shown"));
        assert!(!parse_hidden(""), "no file yet: shown");
        assert!(!parse_hidden("garbage"), "an unreadable choice shows it rather than losing the menus");
    }
}
