# PaperCraft redesign: handoff

Last updated 2026-10-21. Branch `debrand`, pushed to `origin` (`masi/papercraft`). Latest commit when this was written: `3f8ef2c`.

## The goal

Port the PaperCraft redesign (the `PaperCraft` design file) into the egui app and remake all the UI. A Stop hook keeps judging that goal "not met"; the list below is what is actually left, so the next session does not have to re-derive it.

## What is done

* Design file `~/Documents/paper-rs/PaperCraft` (pages `Design System`, `Redesign`, `Redesign (from AI spec)`): built earlier. The dead "+" next to the Layer and Code & AI Handoff headings was removed from all three pages on 2026-10-21. Backup of the file before that edit: `/private/tmp/claude-501/pc-work/PaperCraft.backup` (temp, may be gone).
* App UI: floating rounded panels, shared component kit (`src/ui/kit.rs`), Lottie icon motion (`src/lottie`, `src/ui/lottie_icon.rs`, `src/ui/motion.rs`), home, trash, settings (theme, interface scale, animations), command palette, confirm modals, updater, Present mode (arrow keys, prototype links, camera glide between frames), History/Comments/Agent side views, inspector with Design/Prototype tabs, Effects (drop shadow), export presets, icons in the Share menu and every context menu.
* Agent feedback: animated "working" pill in the top bar and a pulsing ring around the canvas while an agent has called a tool in the last 6 seconds (`agent_pill`, `agent_glow` in `src/ui/mod.rs`).
* Sharing: file sync through the relay (`src/sync.rs`) now merges a page when both sides changed it (`src/merge.rs`, base copies in `.sync-base/`). Verified between two browser sessions and the native app against `papercraft-relay`.
* Web build: `sh web/build.sh`, serve with `papercraft-relay --static web --data <dir>`. Fixed the library probe hiding Share/Join after reload, the share dialog is a modal, the menu bar folds into one "Menu" when the canvas is under 1100px, saved versions work in the browser.
* Skills: `craft-skills/skills/papercraft` (new `references/compact-workflow.md`), `skills-papercraft/skills/*` (project layout, design-to-code pointers). Both repos pushed to `main`.
* Plans: `docs/shared-libs.md` (what to move into `craft-libs`), `docs/feature-ideas.md` (ranked ideas).

## What is not done, and why

1. **`/Applications/PaperCraft.app` is the older build.** The user has PaperCraft open for other work and said not to close it. `tools/install-local.sh` asks a running app to quit, so it was never run against `/Applications`. The final build is installed at `~/Applications/PaperCraft.app` (installed with `PAPERCRAFT_APP_PATH="$HOME/Applications/PaperCraft.app" bash tools/install-local.sh --no-launch`). To finish: the user closes PaperCraft, then `bash tools/install-local.sh`.
2. **Linux rounded corners:** not built. Needs a transparent surface, a paint callback that clears the four corners with replace blending, and `drag_resize_window` edge resizing; none can be tested without a Linux desktop. Reasoning is in `docs/feature-ideas.md` item 9. Windows is done (`with_corner_preference(Round)` and undecorated shadow, compile-checked only).
3. **History, Agent, Comments** show only the project's own data. Richer content needs backend features that do not exist (server-side history, an agent-activity feed, threaded comments with replies from people). See `docs/feature-ideas.md`.
4. **Layer and Code "+"**: the user chose to remove them from the design, so design and app match.
5. Smart transitions that animate matching layers between frames, layer blur, presence on shared projects, version compare: listed in `docs/feature-ideas.md`, not built.
6. **Shared libraries:** nothing moved. `docs/shared-libs.md` has the plan; it touches `craft-libs` and NotesCraft and needs the user's go-ahead.

## Rules the user set

* Never close, quit or reinstall over the user's running PaperCraft. Memory: `dont-close-users-papercraft`. Launch test builds with their own `PAPERCRAFT_HOME` and `PAPERCRAFT_MCP_PORT`, keep the PID from `$!`, kill only that PID. Never `pkill` by name.
* Commit and push only when asked (they have asked for pushes of this work). No `--no-verify`.
* Do not click macOS permission prompts for the user. Launching the app binary directly with `PAPERCRAFT_HOME` set to a temp folder avoids the Documents prompt.
* Caveman terse chat style; normal prose in files and commits.

## How to verify

* `cargo test --no-fail-fast`. `ten_thousand_elements_cache_and_pan_without_rework` (tests/perf.rs) is a timing test and fails when the machine is busy; run it alone.
* `cargo check --lib --target wasm32-unknown-unknown` and `cargo check --lib --target x86_64-pc-windows-msvc` for the other platforms.
* On screen: launch `target/debug/papercraft` (or the installed binary) with `PAPERCRAFT_HOME=<tmp> PAPERCRAFT_MCP_PORT=<free port>`, find its window with `osascript ... get {position, size} of window 1` for that PID, and use the helpers in `/private/tmp/claude-501/pc-work/` (`click`, `drag` Swift binaries; may be gone). Drive the app with `papercraft-cli` and `PAPERCRAFT_MCP_PORT`. Note `papercraft-cli call <tool> -` reads stdin; pass `'{}'` for no arguments.
* Browser: Playwright's Chromium (installed under `~/Library/Caches/ms-playwright`), script in `/private/tmp/claude-501/pc-work/pw/` (`lib.mjs` launches with `--use-angle=swiftshader --enable-unsafe-swiftshader --ignore-gpu-blocklist --enable-unsafe-webgpu`). The browser MCP tool's own Chrome link is broken.

## Gotchas

* egui tests need `out.textures_delta.clear()` after each frame.
* `cargo fmt` reformats the whole crate, including files with the user's uncommitted edits.
* The relay port 8787 may be taken by another local service; use `--listen 127.0.0.1:18787`.
* A `.share` file in a project folder marks it shared; `.sync-base/` holds merge bases and never syncs.
* Home cards open on double-click.
