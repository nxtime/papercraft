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

## Next
1. **Presence on shared projects**: show who else is in the file (coloured cursors and selection outlines). NotesCraft `crates/collab` already has rooms and presence; reuse once it is a shared crate.
2. **Version compare**: pick two saved versions, show a diff overlay (added, removed, moved layers) using `diff_frames` logic.
3. **Auto versions**: snapshot on a timer or before an agent's batch of edits, so every agent run can be rolled back as one step.
4. **Smart transitions in Present mode**: animate matching layers (same name) between frames instead of cutting.
5. **Hover and pressed states for components**: variants (`Button / Primary / Hover`) switched in Present mode on pointer events.
6. **Layer blur and background blur**: needs a renderer pass; unlocks glass effects and the Layer "+" in the design.
7. **Comment threads with replies from people, mentions, and a canvas pin that opens the thread**.
8. **Flatten menus into icon toolbars**: the Share menu and the folded Menu are flat lists; next is an icon rail for the most used actions with labels on hover.
9. **Window**: custom rounded frame with edge resizing on Linux (needs a transparent surface); Windows already asks the system for rounded corners.
10. **One design-system package per brand** imported by every app's design file, plus a token export that generates the theme code (see `docs/shared-libs.md`).
