//! What a mobile shell (the Android app, `apps/android`) adds around the windowed app: the soft
//! keyboard's text, the area the system bars leave free, the clipboard and opening links. The
//! desktop runs without a host; `app::run_in` takes one.

use crate::ime::ImeStep;
use crate::io::clipboard::Provider;

pub trait Host {
    /// Called with each frame's input before the layout runs. Sets what only the platform knows
    /// (the safe-area insets) and answers what the soft keyboard did since the last frame.
    /// `typing`: a text field or a text layer had the keyboard in the last frame.
    fn frame_input(&mut self, input: &mut egui::RawInput, pixels_per_point: f32, typing: bool) -> Vec<ImeStep>;
    /// Puts text on the system clipboard (a text field's copy or cut).
    fn copy_text(&mut self, text: &str);
    /// Opens a link in the app the person picked for it.
    fn open_url(&mut self, url: &str);
    /// The system clipboard for layers; `None` when the platform has none.
    fn clipboard(&self) -> Option<Box<dyn Provider>>;
}

/// The soft keyboard's steps as egui input: composed words as pre-edit, final text as commits and
/// Enter as a key press.
pub fn egui_events(steps: &[ImeStep]) -> Vec<egui::Event> {
    let mut events = Vec::new();
    for step in steps {
        match step {
            ImeStep::Commit(text) => events.push(egui::Event::Ime(egui::ImeEvent::Commit(text.clone()))),
            ImeStep::Preedit(text) => events.push(egui::Event::Ime(egui::ImeEvent::Preedit { text: text.clone(), active_range_chars: None })),
            ImeStep::Enter => {
                for pressed in [true, false] {
                    events.push(egui::Event::Key {
                        key: egui::Key::Enter,
                        physical_key: None,
                        pressed,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    });
                }
            }
        }
    }
    events
}

/// The area the system bars, the cutout and the keyboard cover, as egui safe-area insets: pixels
/// (left, top, right, bottom) in points.
pub fn safe_area(insets: [i32; 4], pixels_per_point: f32) -> egui::SafeAreaInsets {
    // Bar and keyboard sizes are a few hundred pixels; anything outside i16 is treated as no inset.
    let points = |pixels: i32| i16::try_from(pixels.max(0)).map_or(0.0, f32::from) / pixels_per_point.max(f32::EPSILON);
    let [left, top, right, bottom] = insets;
    egui::SafeAreaInsets(egui::epaint::MarginF32 { left: points(left), top: points(top), right: points(right), bottom: points(bottom) })
}

/// A text field asks for the keyboard only while it is painted, and the window hides the keyboard
/// on any frame without that request; the keyboard then resizes the window, the field shows again,
/// and the keyboard flickers open and shut. This keeps the last request while that field holds
/// `focused`, and drops egui's request to interrupt a composition (carried out by turning the
/// keyboard off and on, which on Android hides and reshows it).
pub fn keep_keyboard(focused: Option<egui::Id>, ime: &mut Option<egui::output::IMEOutput>, last: &mut Option<(egui::Id, egui::output::IMEOutput)>) {
    if let Some(request) = ime {
        request.should_interrupt_composition = false;
    }
    *last = match (focused, *ime) {
        (Some(id), Some(request)) => Some((id, request)),
        (Some(id), None) => match *last {
            Some((kept, request)) if kept == id => {
                *ime = Some(request);
                Some((kept, request))
            }
            _ => None,
        },
        (None, _) => None,
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enter_is_a_press_and_a_release() {
        let events = egui_events(&[ImeStep::Commit("a".into()), ImeStep::Enter]);
        assert_eq!(events.len(), 3);
        assert!(matches!(&events[0], egui::Event::Ime(egui::ImeEvent::Commit(t)) if t == "a"));
        assert!(matches!(events[1], egui::Event::Key { key: egui::Key::Enter, pressed: true, .. }));
        assert!(matches!(events[2], egui::Event::Key { key: egui::Key::Enter, pressed: false, .. }));
    }

    #[test]
    fn insets_are_points_and_never_negative() {
        let area = safe_area([0, 96, -4, 300], 3.0);
        assert_eq!((area.0.top, area.0.right, area.0.bottom), (32.0, 0.0, 100.0));
    }
}
