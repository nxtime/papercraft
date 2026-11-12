//! The soft keyboard's text, turned into editor input.
//!
//! `GameActivity` keeps the keyboard's own buffer (`GameTextInput`): the keyboard writes text into
//! it and marks the word it is still composing. winit drops those updates, so each frame
//! [`ImeBridge::step`] compares the buffer with the last one seen and answers what changed: text
//! that is final (committed), the word being composed (shown as pre-edit, the way accents and
//! swipe typing work), and Enter. Once text is final the buffer is emptied (only the composing
//! word stays), so the next update carries only new input and Backspace on an empty buffer
//! reaches the app as a key press.
//!
//! Pure and platform independent, so it is tested on the desktop. The Android shell
//! (`apps/android`) reads the buffer; `app::App` turns the steps into egui and canvas input.

/// One thing the keyboard did, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImeStep {
    /// Final text to insert at the caret (replaces a pre-edit in progress).
    Commit(String),
    /// The word being composed; an empty string ends the composition without text.
    Preedit(String),
    /// The Enter key (a keyboard commits it as a line break).
    Enter,
}

/// The final text a step read out of the keyboard buffer, for the platform to take out of it.
///
/// The platform drops `before` from the start and `after` from the end of the buffer as it is
/// then, on the keyboard's own thread, so letters typed since the read stay in the buffer
/// (overwriting the buffer with what was left at the read would lose them).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reset {
    /// Final text before the composing word (all of it when nothing is composing).
    pub before: String,
    /// Final text after the composing word.
    pub after: String,
}

/// The last buffer seen and whether a pre-edit is showing.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ImeBridge {
    last_text: String,
    last_compose: Option<(usize, usize)>,
    composing: bool,
}

impl ImeBridge {
    /// Compares the keyboard buffer (`text`, composing region in bytes) with the last one seen.
    /// Answers the steps to send to the editor and, when final text left the buffer, what the
    /// buffer must be reset to. An unchanged buffer answers nothing.
    pub fn step(&mut self, text: &str, compose: Option<(usize, usize)>) -> (Vec<ImeStep>, Option<Reset>) {
        let compose = compose.filter(|(start, end)| start < end);
        if text == self.last_text && compose == self.last_compose {
            return (Vec::new(), None);
        }
        let mut steps = Vec::new();
        // A region that does not fall on character boundaries is read as the whole buffer composing.
        let parts = compose.and_then(|(start, end)| Some((text.get(..start)?, text.get(start..end)?, text.get(end..)?)));
        let (before, after, preedit) = match (compose, parts) {
            (None, _) => (text, "", None),
            (Some(_), Some((before, word, after))) => (before, after, Some(word.to_owned())),
            (Some(_), None) => ("", "", Some(text.to_owned())),
        };
        let committed = format!("{before}{after}");
        if !committed.is_empty() {
            push_committed(&mut steps, &committed, self.composing && preedit.is_none());
            self.composing = false;
        }
        match &preedit {
            Some(word) => {
                steps.push(ImeStep::Preedit(word.clone()));
                self.composing = true;
            }
            None if self.composing => {
                // The word was deleted or cancelled: clear what is shown.
                steps.push(ImeStep::Preedit(String::new()));
                self.composing = false;
            }
            None => {}
        }
        let reset = (!committed.is_empty()).then(|| Reset { before: before.to_owned(), after: after.to_owned() });
        if reset.is_some() {
            // What the buffer holds once the platform took the final text out (if nothing was typed since).
            self.last_compose = preedit.as_ref().map(|word| (0, word.len()));
            self.last_text = preedit.clone().unwrap_or_default();
        } else {
            text.clone_into(&mut self.last_text);
            self.last_compose = compose;
        }
        (steps, reset)
    }
}

/// Final text as steps: line breaks become Enter. `replaces_preedit` sends the first piece as a
/// commit even when it is empty, so a composed word turned final replaces its pre-edit.
fn push_committed(steps: &mut Vec<ImeStep>, committed: &str, replaces_preedit: bool) {
    let mut first = true;
    for (index, piece) in committed.split(['\n', '\r']).enumerate() {
        if index > 0 {
            steps.push(ImeStep::Enter);
        }
        if !piece.is_empty() || (first && replaces_preedit) {
            steps.push(ImeStep::Commit(piece.to_owned()));
        }
        first = false;
    }
}

#[cfg(test)]
mod tests {
    use super::{ImeBridge, ImeStep, Reset};

    fn commit(text: &str) -> ImeStep {
        ImeStep::Commit(text.to_owned())
    }

    fn preedit(text: &str) -> ImeStep {
        ImeStep::Preedit(text.to_owned())
    }

    #[test]
    fn a_composed_word_shows_as_preedit_then_commits_and_empties_the_buffer() {
        let mut bridge = ImeBridge::default();
        assert_eq!(bridge.step("ca", Some((0, 2))), (vec![preedit("ca")], None));
        assert_eq!(bridge.step("caf", Some((0, 3))), (vec![preedit("caf")], None));
        assert_eq!(bridge.step("café", Some((0, 5))), (vec![preedit("café")], None), "an accent is part of the word");
        let (steps, reset) = bridge.step("café ", None);
        assert_eq!(steps, vec![commit("café ")]);
        assert_eq!(reset, Some(Reset { before: "café ".to_owned(), after: String::new() }));
        assert_eq!(bridge.step("", None), (Vec::new(), None), "the emptied buffer is no news");
    }

    #[test]
    fn an_unchanged_buffer_answers_nothing() {
        let mut bridge = ImeBridge::default();
        bridge.step("wo", Some((0, 2)));
        assert_eq!(bridge.step("wo", Some((0, 2))), (Vec::new(), None));
    }

    #[test]
    fn text_before_the_composing_word_commits_and_the_word_stays_in_the_buffer() {
        let mut bridge = ImeBridge::default();
        let (steps, reset) = bridge.step("olá m", Some((5, 6)));
        assert_eq!(steps, vec![commit("olá "), preedit("m")]);
        assert_eq!(reset, Some(Reset { before: "olá ".to_owned(), after: String::new() }));
        assert_eq!(bridge.step("m", Some((0, 1))), (Vec::new(), None), "the reset buffer is what was expected");
        assert_eq!(bridge.step("mu", Some((0, 2))), (vec![preedit("mu")], None));
    }

    #[test]
    fn letters_typed_after_a_read_are_read_next_time_not_lost() {
        let mut bridge = ImeBridge::default();
        let (steps, reset) = bridge.step("ab", None);
        assert_eq!(steps, vec![commit("ab")]);
        assert_eq!(reset, Some(Reset { before: "ab".to_owned(), after: String::new() }));
        // The keyboard typed "c" before the platform took "ab" out: the buffer is now "c".
        assert_eq!(bridge.step("c", None).0, vec![commit("c")]);
    }

    #[test]
    fn a_line_break_becomes_enter() {
        let mut bridge = ImeBridge::default();
        let (steps, _) = bridge.step("one\ntwo", None);
        assert_eq!(steps, vec![commit("one"), ImeStep::Enter, commit("two")]);
        let mut bridge = ImeBridge::default();
        assert_eq!(bridge.step("\n", None).0, vec![ImeStep::Enter]);
    }

    #[test]
    fn a_deleted_composing_word_clears_the_preedit() {
        let mut bridge = ImeBridge::default();
        bridge.step("x", Some((0, 1)));
        assert_eq!(bridge.step("", None), (vec![preedit("")], None));
    }

    #[test]
    fn a_region_off_a_character_boundary_is_read_as_the_whole_word() {
        let mut bridge = ImeBridge::default();
        // 'é' is two bytes; 1 is inside it.
        assert_eq!(bridge.step("é", Some((1, 2))).0, vec![preedit("é")]);
        assert_eq!(bridge.step("é", Some((0, 99))).0, vec![preedit("é")], "a region past the end too");
    }

    #[test]
    fn a_composed_word_that_turns_final_replaces_its_preedit() {
        let mut bridge = ImeBridge::default();
        bridge.step("hi", Some((0, 2)));
        assert_eq!(bridge.step("hi", None).0, vec![commit("hi")]);
    }
}
