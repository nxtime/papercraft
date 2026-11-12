//! The activity side of the app: `android_main`, and the [`Host`] that gives the shared app the
//! soft keyboard, the clipboard, links and the safe area.

use papercraft::app::{Host, Start, UserEvent};
use papercraft::ime::{ImeBridge, ImeStep};
use papercraft::io::clipboard::Provider;
use winit::event_loop::EventLoop;
use winit::platform::android::EventLoopBuilderExtAndroid;
use winit::platform::android::activity::AndroidApp;

const LOG_TAG: &str = "PaperCraft";
/// Folder name under the app's private storage, so `PAPERCRAFT_HOME` is not the bare data dir.
const HOME_FOLDER: &str = "papercraft";

struct AndroidHost {
    android: AndroidApp,
    bridge: ImeBridge,
    /// Whether the keyboard buffer can be read (it is null until the keyboard first writes).
    keyboard_ready: bool,
}

impl AndroidHost {
    /// The soft keyboard's changes since the last frame.
    fn keyboard_steps(&mut self) -> Vec<ImeStep> {
        // The buffer still holds text already read until the UI thread takes it out; reading it
        // now would send it twice.
        if crate::platform::input_pending(&self.android) {
            return Vec::new();
        }
        let state = self.android.text_input_state();
        let compose = state.compose_region.map(|span| (span.start, span.end));
        let (steps, reset) = self.bridge.step(&state.text, compose);
        if let Some(reset) = reset {
            crate::platform::consume_input(&self.android, &reset.before, &reset.after);
        }
        steps
    }

    /// The system bars, cutout and keyboard in pixels: the window minus the content rectangle
    /// (API 26-29, where the window fits inside the bars) plus the insets `PaperActivity` reports
    /// (API 30+, edge to edge). `None` until the activity has a window.
    fn insets(&self) -> Option<[i32; 4]> {
        let window = self.android.native_window()?;
        let content = self.android.content_rect();
        let [left, top, right, bottom] = crate::platform::safe_insets(&self.android).unwrap_or_default();
        Some([
            content.left.saturating_add(left),
            content.top.saturating_add(top),
            window.width().saturating_sub(content.right).saturating_add(right),
            window.height().saturating_sub(content.bottom).saturating_add(bottom),
        ])
    }
}

impl Host for AndroidHost {
    fn frame_input(&mut self, input: &mut egui::RawInput, pixels_per_point: f32, typing: bool) -> Vec<ImeStep> {
        input.safe_area_insets = self.insets().map(|insets| papercraft::host::safe_area(insets, pixels_per_point));
        if !self.keyboard_ready && typing {
            self.keyboard_ready = crate::platform::keyboard_wrote(&self.android);
        }
        if self.keyboard_ready { self.keyboard_steps() } else { Vec::new() }
    }

    fn copy_text(&mut self, text: &str) {
        crate::platform::copy_text(&self.android, text);
    }

    fn open_url(&mut self, url: &str) {
        crate::platform::open_url(&self.android, url);
    }

    fn clipboard(&self) -> Option<Box<dyn Provider>> {
        Some(Box::new(Clipboard(self.android.clone())))
    }
}

/// The system clipboard for layers: text only (an image would need a content URI).
struct Clipboard(AndroidApp);

impl Provider for Clipboard {
    fn read_text(&mut self) -> Option<String> {
        crate::platform::read_text(&self.0)
    }

    fn write_text(&mut self, text: &str) {
        crate::platform::copy_text(&self.0, text);
    }
}

/// Sends `log` lines and panics to logcat: stderr goes nowhere on Android.
fn init_logging() {
    android_logger::init_once(android_logger::Config::default().with_tag(LOG_TAG).with_max_level(log::LevelFilter::Info));
    std::panic::set_hook(Box::new(|info| log::error!("panic: {info}")));
}

/// Points the app's folders at its private storage: projects under `PAPERCRAFT_HOME`, settings
/// under `HOME/.config` (the desktop folders do not exist on a phone).
#[allow(unsafe_code, reason = "set_var is unsafe in edition 2024; this runs first thing in android_main, before the app starts threads")]
fn use_private_storage(data: &std::path::Path) {
    // SAFETY: only the activity's own glue threads exist yet, and none of them reads these variables.
    unsafe {
        std::env::set_var("HOME", data);
        std::env::set_var("PAPERCRAFT_HOME", data.join(HOME_FOLDER));
    }
}

/// Called by `GameActivity` once the library is loaded.
#[allow(unsafe_code, reason = "the loader looks the entry up by its unmangled name; the body is safe Rust")]
#[allow(clippy::no_mangle_with_rust_abi, reason = "android-activity calls `android_main` with the Rust ABI, as its docs require")]
#[unsafe(no_mangle)]
fn android_main(android: AndroidApp) {
    init_logging();
    let Some(data) = android.internal_data_path() else {
        log::error!("no app data folder");
        return;
    };
    use_private_storage(&data);
    papercraft::file_keys::install(&data.join("keys"));
    let event_loop = match EventLoop::<UserEvent>::with_user_event().with_android_app(android.clone()).build() {
        Ok(event_loop) => event_loop,
        Err(error) => {
            log::error!("event loop: {error}");
            return;
        }
    };
    let host = AndroidHost { android, bridge: ImeBridge::default(), keyboard_ready: false };
    if let Err(error) = papercraft::app::run_in(event_loop, Start::Home, Some(Box::new(host))) {
        log::error!("app: {error}");
    }
}
