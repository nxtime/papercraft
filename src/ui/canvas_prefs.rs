//! Settings > Canvas, kept between sessions. The grid and snapping can also be toggled from the
//! menus and shortcuts, so [`sync`] saves whatever the live values have become.

use super::UiState;
use crate::editor::Editor;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CanvasPrefs {
    pub dot_grid: bool,
    pub snapping: bool,
    pub wheel_zooms: bool,
}

impl Default for CanvasPrefs {
    fn default() -> Self {
        CanvasPrefs {
            dot_grid: true,
            snapping: true,
            wheel_zooms: false,
        }
    }
}

impl CanvasPrefs {
    fn path() -> Option<std::path::PathBuf> {
        Some(crate::project::config_dir()?.join("canvas"))
    }

    pub fn load() -> CanvasPrefs {
        Self::path()
            .and_then(|p| crate::vfs::read_to_string(p).ok())
            .map_or_else(CanvasPrefs::default, |s| Self::parse(&s))
    }

    /// `"<dot grid 0|1> <snapping 0|1> <wheel zooms 0|1>"`.
    fn parse(s: &str) -> CanvasPrefs {
        let mut flags = s.split_whitespace().map(|v| v == "1");
        let d = CanvasPrefs::default();
        CanvasPrefs {
            dot_grid: flags.next().unwrap_or(d.dot_grid),
            snapping: flags.next().unwrap_or(d.snapping),
            wheel_zooms: flags.next().unwrap_or(d.wheel_zooms),
        }
    }

    fn save(self) {
        let Some(p) = Self::path() else { return };
        let flags = format!(
            "{} {} {}",
            u8::from(self.dot_grid),
            u8::from(self.snapping),
            u8::from(self.wheel_zooms)
        );
        let saved = match p.parent() {
            Some(dir) => {
                crate::vfs::create_dir_all(dir).and_then(|()| crate::vfs::write(&p, flags))
            }
            None => crate::vfs::write(&p, flags),
        };
        if let Err(error) = saved {
            log::warn!("could not save the canvas settings: {error}");
        }
    }

    /// Puts saved settings on the live editor and interface.
    pub fn apply(self, ed: &mut Editor, st: &mut UiState) {
        ed.snapping = self.snapping;
        st.show_grid_off = !self.dot_grid;
        st.wheel_zooms = self.wheel_zooms;
        st.canvas_saved = self;
    }
}

pub fn sync(ed: &Editor, st: &mut UiState) {
    let now = CanvasPrefs {
        dot_grid: !st.show_grid_off,
        snapping: ed.snapping,
        wheel_zooms: st.wheel_zooms,
    };
    if now != st.canvas_saved {
        now.save();
        st.canvas_saved = now;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_keeps_each_flag_and_defaults_what_is_missing() {
        let flags = CanvasPrefs {
            dot_grid: false,
            snapping: true,
            wheel_zooms: true,
        };
        assert_eq!(CanvasPrefs::parse("0 1 1"), flags);
        assert_eq!(CanvasPrefs::parse(""), CanvasPrefs::default());
        assert_eq!(
            CanvasPrefs::parse("0"),
            CanvasPrefs {
                dot_grid: false,
                ..CanvasPrefs::default()
            }
        );
    }
}
