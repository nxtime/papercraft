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
/// Reach of the arrow head's strokes, along and across the middle bar.
const ARROW_HEAD: f32 = 4.5;
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

/// Where the three bars end, as `[start, end]` offsets from the button's centre, with the strip
/// `open` (0 = hidden, 1 = shown). Hidden they are three bars; shown they have folded into a
/// back arrow, pointing the way the strip slides away. The middle bar never moves.
fn bars(open: f32) -> [[Vec2; 2]; 3] {
    let lerp = |a: Vec2, b: Vec2| a + (b - a) * open;
    let (h, g) = (BAR_HALF, BAR_GAP);
    let tip = Vec2::new(-h, 0.0);
    let head = Vec2::new(-h + ARROW_HEAD, ARROW_HEAD);
    let head_up = Vec2::new(head.x, -head.y);
    [
        [lerp(Vec2::new(-h, -g), tip), lerp(Vec2::new(h, -g), head_up)],
        [Vec2::new(-h, 0.0), Vec2::new(h, 0.0)],
        [lerp(Vec2::new(-h, g), tip), lerp(Vec2::new(h, g), head)],
    ]
}

/// The menu button. Clicking toggles the strip: the caller flips and saves the choice. `open`
/// is the strip's eased state, which also drives the icon's morph.
pub fn button(ui: &mut Ui, open: f32) -> Response {
    let tip = if open < 0.5 { "Show the menu bar" } else { "Hide the menu bar" };
    icon(ui, open).on_hover_text(tip)
}

/// The same three bars as a plain menu button, for windows too narrow for the strip.
pub fn menu_button(ui: &mut Ui) -> Response {
    icon(ui, 0.0).on_hover_text("Menu")
}

fn icon(ui: &mut Ui, open: f32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(SIZE), Sense::click());
    if ui.is_rect_visible(rect) {
        let p = theme::pal(ui);
        let hover = crate::ui::motion::hover(ui.ctx(), resp.id.with("hover"), &resp);
        let fill = if resp.is_pointer_button_down_on() { p.pressed } else { p.hover.gamma_multiply(hover) };
        ui.painter().rect_filled(rect, theme::radius_md(), fill);
        let stroke = Stroke::new(BAR_STROKE, p.text);
        let c = rect.center();
        for [a, b] in bars(open) {
            ui.painter().line_segment([c + a, c + b], stroke);
        }
    }
    resp
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bars_are_flat_when_hidden_and_an_arrow_when_shown() {
        let hidden = bars(0.0);
        assert!(hidden.iter().all(|[a, b]| a.y == b.y), "three flat bars");
        let shown = bars(1.0);
        assert_eq!(shown[0][0], shown[2][0], "the head strokes meet at the tip");
        assert_eq!(shown[0][0], shown[1][0], "and the tip is the shaft's end");
        assert!(shown[0][1].y < 0.0 && shown[2][1].y > 0.0);
    }

    #[test]
    fn only_the_word_hidden_hides_the_strip() {
        assert!(parse_hidden("hidden"));
        assert!(parse_hidden("hidden\n"), "a trailing newline is fine");
        assert!(!parse_hidden("shown"));
        assert!(!parse_hidden(""), "no file yet: shown");
        assert!(!parse_hidden("garbage"), "an unreadable choice shows it rather than losing the menus");
    }
}
