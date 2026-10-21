# Feature ideas

Ranked by value for the effort. Done items say where.

## Done
* Prototype links: click a layer in Present mode to jump to a frame (inspector, Prototype tab).
* Drop-shadow effect in the inspector.
* Saved versions with rename, restore, delete (also in the browser).
* Comments filters, reopen, delete; agent activity log with failures, copy agent URL.
* Agent activity pill and canvas glow while an agent works; layers pop in.
* Merge of concurrent edits to one shared page (`src/merge.rs`, `src/sync.rs`).
* Interface scale in Settings and the command palette (native).
* Export presets (inspector Export "+").
* Shared libraries: publish a project as a library, enable and update it in others (`src/libraries.rs`, Libraries modal); cross-page components; number variables.
* Design from the CLI with the app closed (`papercraft-cli --offline`, `shell`).
* Auto versions before agent edits: each agent run snapshots edited pages and can be reviewed or reverted (`src/runs.rs`, Agent view).

## Next
1. **Presence on shared projects**: show who else is in the file (coloured cursors and selection outlines). NotesCraft `crates/collab` already has rooms and presence; reuse once it is a shared crate.
2. **Version compare**: pick two saved versions and show a diff overlay (added, changed, removed layers) using the id-paired `diff_pages` in `src/mcp/diff.rs`; compare UI is still todo.
4. **Smart transitions in Present mode**: animate matching layers (same name) between frames instead of cutting.
5. **Hover and pressed states for components**: variants (`Button / Primary / Hover`) switched in Present mode on pointer events.
6. **Layer blur and background blur**: needs a renderer pass; unlocks glass effects and the Layer "+" in the design.
7. **Comment threads with replies from people, mentions, and a canvas pin that opens the thread**.
8. **Flatten menus into icon toolbars**: the Share menu and the folded Menu are flat lists; next is an icon rail for the most used actions with labels on hover.
9. **Window**: Windows asks the system for rounded corners and keeps the shadow. On Linux the window manager (or the Wayland client-side frame) draws the frame, and it is round on GNOME and KDE. A frame drawn by the app needs three things that cannot be checked without a Linux desktop: a transparent surface, a paint callback that clears the four corners with replace blending (egui cannot erase pixels it has already drawn), and its own edge resizing through `drag_resize_window`. Not built for that reason.
10. **One design-system package per brand** imported by every app's design file, plus a token export that generates the theme code (see `docs/shared-libs.md`). Publishing the brand project as a library covers the sharing; the generated theme code is still todo.
