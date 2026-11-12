# Android app (sideload APK)

## Why

The phone browser experience is poor (ROADMAP row 69). The owner approved a native Android app on 2026-11-11, Android first and iOS later. The app has to run the same PaperCraft, with the phone layout that `ui::phone::is_phone(width)` already picks, and work offline.

## Scope

1. **Shell.** `apps/android` is a cdylib (`libpapercraft_android.so`) with its own `[workspace]` and lockfile, so the Android-only dependencies stay out of the desktop and web builds. `android_main` builds a winit event loop on `android-game-activity` and calls `app::run_in(event_loop, Start::Home, Some(host))`. `app::run` (the desktop) builds its loop and calls the same function with no host.
2. **Host.** `app::Host` (`src/host.rs`) is what only a mobile shell knows: the safe-area insets (system bars, cutout, keyboard), the soft keyboard's text, the clipboard and opening links. `src/ime.rs` turns the `GameActivity` text buffer into commits, pre-edit and Enter (winit drops those events); the app sends them to egui while a field has the keyboard, else to the text layer being edited. Copy, paste and links go through JNI (`apps/android/src/platform.rs`, `PaperActivity.java`); only http, https, mailto and tel links are opened.
3. **Gated for Android, as for wasm.** File and message dialogs (`dialogs.rs` answers as if dismissed), the clipboard crate, the updater and its restart, the MCP TCP port, the app lock, the custom title bar and window-edge resizing. `reqwest` has no TLS on Android, so the relay needs plain http until a TLS stack is added.
4. **Surface.** Android takes the window away when the app leaves the front: `suspended` drops the wgpu surface, `resumed` makes a new one from the same window and keeps the device, so the open project and the GPU caches survive.
5. **Storage.** `android_main` sets `PAPERCRAFT_HOME` to `<internal data dir>/papercraft` and `HOME` to the internal data dir (the theme and other settings use `HOME/.config`). Home, new, open, edit, save and trash work offline; writes are the same atomic writes as on the desktop.
6. **Packaging.** `packaging/android/package.sh [--abi arm64-v8a|x86_64|all]` builds the library with the NDK, compiles `PaperActivity` and the `GameActivity` library, and signs `PaperCraft-<version>.apk` with the debug key. App id `dev.marcospaulo.papercraft`, minSdk 26, targetSdk 35.

## Not in scope (next steps, in order)

* Syncing the shared library through the relay, with a TLS stack for https.
* Opening and saving files outside the app folder (Storage Access Framework), and the share-sheet import.
* A real signing key and the Play Store; a release build with a smaller GPU limit profile for old phones.
* iOS.

## Acceptance

* `packaging/android/package.sh` produces `PaperCraft-<version>.apk` and `apkanalyzer`/`aapt2 dump badging` shows the app id and the arm64 library.
* On a device: the home screen shows in the phone layout; a new project can be created, edited with touch and the soft keyboard, saved on leaving, and trashed; leaving the app and coming back keeps the project.
* Desktop and wasm are unchanged: `cargo check --lib` and `cargo check --lib --target wasm32-unknown-unknown` pass.
* Tests: `host::tests` (Enter and commit events, insets in points) and `ime::tests` (the buffer bridge).
