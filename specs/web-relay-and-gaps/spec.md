# Web build: remote agent relay + closing the browser gaps

Audience: a coding agent (pi, MiniMax) with no prior context. Everything needed is here.
Work in the git worktree you were started in. Do not commit; leave changes in the working tree.

## 1. What PaperCraft is

PaperCraft is a native infinite-canvas design tool in Rust (edition 2024, rustc >= 1.95).
Crate `papercraft` = library (`src/lib.rs`) + bins (`papercraft`, `papercraft-cli`, `papercraft-mcp`).
UI is egui 0.36 + egui-wgpu; the canvas is drawn by our own wgpu renderer (`src/render/`).
AI agents drive the app over MCP (`src/mcp/`): tool catalogue in `mcp/tools.rs`, executor in
`mcp/exec.rs` (trait `Host`), rmcp HTTP server in `mcp/server.rs` (native only).

## 2. What exists on this branch (already working, do not redo)

A browser build exists (`wasm32-unknown-unknown`):

- `src/web.rs`: browser shell. Canvas + wgpu surface (WebGPU, WebGL fallback), DOM events
  translated to egui `RawInput` and to editor `InputEvent`s, requestAnimationFrame loop, home
  screen (projects), autosave, `pagehide` save. `Web` struct owns `Editor`, `UiState`, `Session`.
  `#demo` in the URL opens the sample board.
- `src/vfs/`: file-system shim. Native = `std::fs` re-export. Wasm (`vfs/mem.rs`) = in-memory tree
  mirrored to IndexedDB (`idb_put`/`idb_delete` fire-and-forget; `vfs::load().await` at startup).
  Projects live under `/papercraft` (`WEB_ROOT` in web.rs). All code paths that touch files must go
  through `crate::vfs`, never `std::fs` / `image::open` / `File::open` (those do not work in wasm).
- Native-only things are gated with `#[cfg(not(target_arch = "wasm32"))]`: `app.rs` (winit shell),
  `mcp::server`, arboard clipboard, `rmcp`/`axum`/`tokio` net features (see `Cargo.toml` target tables).
- `web/index.html`, `web/build.sh` (builds `web/pkg`). `web_time::Instant` is used instead of std.

Build/verify commands (ALWAYS put rustup first: Homebrew's cargo is too old):

    export PATH="$HOME/.cargo/bin:$PATH"
    cargo +stable test --lib                                   # must stay 112+ passing
    cargo +stable check --all-targets                          # native
    cargo +stable check --lib --target wasm32-unknown-unknown  # browser
    cargo +stable check --target x86_64-pc-windows-msvc        # windows
    ./web/build.sh                                             # -> web/pkg (needs wasm-bindgen-cli 0.2.129, installed)
    cargo +stable fmt -- <only the files you changed>          # do NOT run plain `cargo fmt`; then `git checkout build.rs`

No browser is available in your environment, so you cannot run the web build. Verify with the
checks above plus native tests. Write the logic so it is unit-testable natively where possible
(pure functions, protocol structs, zip writer, byte-based importers).

## 3. Goal

A. Let a remote AI agent drive a PaperCraft tab running in someone's browser (hosted deployment).
B. Close the browser gaps listed in section 5.

## 4. Part A: relay for remote agents

### Architecture

    agent --(MCP streamable HTTP)--> relay <--(WebSocket)-- browser tab
                                      POST /s/{token}/mcp     GET /tab/{token}

The tab already contains the full executor (`mcp::exec::call(host, name, args)`), so the relay
only forwards tool calls; it holds no document state.

### A1. `src/bin/papercraft-relay.rs` (native binary)

- Add it to `Cargo.toml` as a `[[bin]]`. Needs axum `ws` feature (extend the existing axum entry
  in the NON-wasm target table; keep default-features = false). Prefer reusing `mcp::server`
  (`PaperServer`, `StreamableHttpService`, `Dispatch`, `Call`) so the tool list and instructions
  stay identical to the desktop server. Study `mcp/server.rs::spawn` first; if the service cannot be
  mounted under a dynamic path `/s/{token}/mcp`, create one service per token lazily and route by
  token, or expose a small refactor in `mcp/server.rs` (keep desktop behaviour unchanged).
- CLI: `papercraft-relay [--listen 0.0.0.0:8787] [--static <dir>]` (env `PAPERCRAFT_RELAY_LISTEN`).
  `--static` serves the built web app (`web/index.html`, `web/pkg/*`) from the same origin with
  correct MIME types (`.wasm` = `application/wasm`, `.js`, `.html`, `.css`, `.png`, `.svg`), so one
  process hosts everything. Implement with `tokio::fs` + a path-traversal check; no new crate unless
  unavoidable.
- `GET /tab/{token}`: WebSocket for the browser. One tab per token; a new connection replaces the
  old one (close the old with a reason).
- `POST|GET|DELETE /s/{token}/mcp`: streamable-HTTP MCP endpoint for agents.
- Token = 32..64 lowercase hex chars chosen by the tab. Reject anything else with 404. It is a
  bearer secret: never log it, never put it in error messages. The relay stores nothing on disk.
- Origin policy: `/tab/*` accepts browser origins (optionally restricted by `--allow-origin`);
  `/s/*` rejects requests that carry an `Origin` header (agents are not browsers).
- A tool call is forwarded to the tab, awaited with a 120 s timeout (same as `CALL_TIMEOUT`).
  No tab connected => the tool result is an error text:
  "No PaperCraft tab is connected for this link. Open PaperCraft and use Agent > Connect."
- Limits: max WebSocket message 16 MiB; ping every 20 s; drop dead sockets; max 8 concurrent
  in-flight calls per token (others wait).

### A2. Wire protocol (JSON text frames, UTF-8)

relay -> tab:  `{"id": 7, "name": "get_screenshot", "args": { ... }}`
tab -> relay (success): `{"id": 7, "ok": true, "out": [{"type":"text","text":"..."},{"type":"image","mime":"image/png","data":"<base64>"}]}`
tab -> relay (failure): `{"id": 7, "ok": false, "error": "message"}`
Keep-alive: WebSocket ping/pong. Define the frame structs once in a module compiled for both
native and wasm (e.g. `src/mcp/relay_proto.rs`, serde only) and unit-test the (de)serialisation.

### A3. Tab side (`src/web.rs`; split into `src/web/mod.rs` + `src/web/relay.rs` if it helps)

- `web-sys` WebSocket (add features `WebSocket`, `MessageEvent`, `CloseEvent`, `ErrorEvent`,
  `Storage`, plus whatever else you need) with auto-reconnect (backoff 1 s -> 15 s).
- Token: generated once with `crypto.getRandomValues` (16 bytes -> 32 hex), stored in
  `localStorage["papercraft.relay.token"]`; "Regenerate" action replaces it and reconnects.
- Relay base URL: query param `?relay=wss://host` > `window.PAPERCRAFT_RELAY` > same origin
  (`wss://` when the page is https, else `ws://`).
- Incoming call => run `mcp::exec::call(&mut web_host, name, &args)` on the main thread inside the
  frame loop (queue them; process at the start of `frame()` so edits and repaint stay in one place;
  set `dirty = true`). Implement `mcp::exec::Host` for `Web` mirroring `impl Host for App` in
  `src/app.rs` (open_project, save, project, canvas_open, store, editor).
- `Host::gpu()` returns `Option<Headless>` and the native renderer blocks on GPU readback, which is
  impossible in wasm. Tools that need pixels (`get_screenshot`, PNG export) must work on the web:
  design an async path (e.g. `Host::gpu()` returns None on web and the executor returns a
  "needs render" marker; the shell then renders the requested frame/region offscreen, reads it back
  with `wgpu::Buffer::map_async` + a JS Promise or via `canvas.toBlob`, and answers the call
  later). Keep the desktop code path and its tests unchanged. Document the design in a comment.
- UI: new top-level menu "Agent" (web only) with: "Connect…" opening a small egui window showing the
  full agent URL `https://<host>/s/<token>/mcp` with a Copy button, connection state
  (Connected / Connecting / Offline), "Regenerate link", and ready-to-paste snippets for
  Claude Code (`claude mcp add --transport http papercraft <url>`) and a generic MCP JSON config.
  A small status dot in the toolbar when connected is enough. Menus are built in
  `src/ui/mod.rs::menu_bar` / `editor/shortcuts.rs::menus()`; the tab needs a way to add an item
  without touching the native menus (e.g. a `UiState` flag set only by `web.rs`).

### A4. Acceptance for Part A

- `cargo +stable test` has an integration test (`tests/relay.rs`) that starts the relay on port 0,
  connects a fake tab (tokio-tungstenite as dev-dependency is fine), calls a tool through the MCP
  HTTP endpoint (use rmcp client as `tests/` already does for the desktop server if present, else raw
  JSON-RPC), and asserts the round trip, the no-tab error, token validation, and Origin rejection.
- Wasm and native and windows checks pass; no new warnings.
- `docs/` or `web/README.md` is NOT required (do not create README files).

## 5. Part B: close the browser gaps

Currently these show "Not available in the browser yet" or are broken in the browser:

B1. **Import** (`HomeAction::Import`, `AppRequest::PlaceImage`, `AppRequest::ImageFill`, drag-drop):
    use a hidden `<input type=file>` and `dragover/drop` on the canvas. `io::import::import_as_pages`
    takes a `&Path`; add a bytes-based entry (`import_bytes(name, bytes)`) used by both the native
    wrapper and the web, supporting `.papercraft`, `.excalidraw`, `.svg`, and images (png/jpg/gif/webp).
    Native behaviour must not change. Also add importing a project backup zip (see B6).
B2. **Export** (`AppRequest::Export`, `ExportFormat`, inspector Export section): PNG, SVG, PDF built
    by the existing `io::export` code, delivered as a browser download (Blob + temporary
    `<a download>`). PNG needs the async GPU readback from A3: share that implementation.
B3. **Clipboard**: `Editor.clipboard_io: Option<Box<dyn io::clipboard::Provider>>` is a sync trait.
    Implement `WebClipboard` as an in-memory mirror: on copy/cut write text to the mirror AND
    `navigator.clipboard.writeText` (fire and forget); on the DOM `paste` event (don't
    preventDefault on Ctrl/Cmd+V) fill the mirror from `clipboardData` (text/plain, and image files
    decoded with the `image` crate), then trigger the editor's paste action. `paste` text already
    reaches egui text fields via `egui::Event::Paste`; keep that working. Also wire `copy`/`cut` DOM
    events. `AppRequest::CopyAsPng` = render + `navigator.clipboard.write(ClipboardItem)`.
B4. **Thumbnails**: `Session::write_thumbnail(&Headless, &Editor)` blocks on GPU readback. Produce
    the home-screen thumbnail on the web with the async path, write `thumbnail.png` into the project
    folder via `vfs`. Also fix `ui/home.rs::thumb` (line ~64) which uses `image::open(path)`: read
    bytes with `vfs::read` and decode with `image::load_from_memory`. Grep the whole tree for other
    `image::open`, `File::open`, `std::fs`, `std::path::Path::{exists,is_file,is_dir}` uses on
    paths that the web build reaches, and route them through `vfs`.
B5. **Settings persistence**: `theme::Choice::path()` returns None on wasm (`dirs::config_dir`). On
    wasm use the fixed vfs path `/config/theme` so the theme choice survives reloads. Likewise the
    shape library: native loads `Library::load(Library::path_in(&store.root))` in `App::new`; the web
    shell currently never loads or saves it (`st.library.library` stays default). Do the same in web.rs
    (load at start; save when `st.library.dirty`, see `app.rs`).
B6. **"Open projects folder"** has no meaning in a browser. On wasm, `HomeAction::RevealRoot` becomes
    "Download backup (.zip)": a zip of the whole `/papercraft` tree built in memory. Write a small
    store-only (no compression) zip writer (pure function over `Vec<(String, Vec<u8>)>`, CRC-32
    implemented locally or via an installed crate; `miniz_oxide` is already a dependency) and a
    matching reader used by B1's backup import. Label the sidebar item per platform
    (`ui/home.rs`: "Open projects folder" native, "Download backup" web).
B7. **Errors**: the web shell only logs and sets `ed.status`. Show errors to the user (the status
    bar already renders `ed.status`; make sure it is visible and cleared sensibly), and surface
    "Not saved" persistence failures (IndexedDB write errors currently ignored in `vfs/mem.rs`:
    keep a shared "last persistence error" and show it).
B8. **Bundle size**: release wasm is ~17 MB. In `web/build.sh` run `wasm-opt -Oz` when available
    (skip silently otherwise) and enable `lto = true`, `codegen-units = 1`, `panic = "abort"`
    only for a `[profile.web]` that inherits release (do not change the native release profile).

### Acceptance for Part B

- Everything above works through code paths that compile for wasm; native behaviour and all
  existing tests unchanged (`cargo +stable test --lib` passes, same or higher count).
- New native unit tests for the pure parts: bytes importer, zip writer/reader round trip, relay
  protocol frames, any path/format helpers.
- No remaining user-visible "Not available in the browser yet" for B1-B6 features. Features that
  still cannot work (`HomeAction::RevealRoot` native-only semantics) must be explained in code
  comments only where non-obvious.

## 6. Constraints (project rules)

- Do exactly this spec. No unrelated refactors, renames, formatting sweeps, or new README/docs.
- Smallest correct diff; reuse existing code (`mcp::exec`, `io::export`, `io::import`, `Session`,
  `HomeView`) before writing new code. New dependencies only if unavoidable; list each in your report
  with the reason. Never add dependencies to the native-only table that the wasm build needs, and
  vice versa.
- Rust style of the surrounding code: named items, early returns, no `unwrap()` on user/IO input,
  no magic numbers (named consts), comments only for non-obvious "why".
- `std::time::Instant/SystemTime` panic on wasm: use `web_time` / `crate::project::now_ms`.
- Blocking calls (`pollster::block_on`, `device.poll(Wait)`, `std::thread`, sockets) are forbidden in
  code that compiles for wasm.
- No secrets in code or logs. The relay token is a secret.
- Write files in chunks of at most ~300 lines per tool call (your output limit per turn is small).
- Do not touch `build.rs` formatting; do not edit `Cargo.lock` by hand (cargo updates it).
- Do not commit.

## 7. Work order (stop and report on failure)

1. A2 protocol module + tests. 2. A1 relay + `tests/relay.rs`. 3. A3 tab side (without async
screenshot). 4. Async GPU readback (shared by A3, B2, B4). 5. B1, B3, B5, B6, B7. 6. B2, B4.
7. B8. After each step run the three `cargo check` commands and `cargo +stable test --lib`.

## 8. Final report (print at the end)

List: files changed/added, new dependencies and why, each acceptance item with pass/fail evidence
(command output summaries), anything not done and why, and what must be tested manually in a real
browser (exact steps).
