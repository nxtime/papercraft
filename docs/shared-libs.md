# Shared libraries for the Craft apps

`craft-libs` (`git.marcospaulo.dev.br/masi/craft-libs`) already holds `atomic`, `print`, `quit`,
`settings` and `ui`. PaperCraft and NotesCraft both depend on it. This note lists what else the two
apps carry twice (or that any new app would need) and the order to move it, so a new app starts from
crates instead of copies.

## Candidates

| Crate | Today | Why shared |
|---|---|---|
| `craft-lottie` | PaperCraft `src/lottie/` (about 2.5k lines, with fixtures); NotesCraft `crates/lottie/` (older, smaller). The two copies have diverged. | Renderer-independent Lottie-subset player for animated icons. Same format, same bugs fixed twice. First to move: no UI dependency. |
| `craft-motion` | PaperCraft `src/ui/motion.rs` (easing, spring, hover, appear, pop-in, wobble, progress, reduced-motion switch). | Every egui app wants the same eased hover and appear helpers. Only depends on `egui`. |
| `craft-kit` | PaperCraft `src/ui/kit.rs` (button kinds, chip, toggle, checkbox, select, slider, section header, setting row, banner, empty state, badge, nav item, key cap). | Overlaps `craft-ui` icon buttons and tabs. Merge into `craft-ui` behind a small palette trait so each app keeps its theme. |
| `craft-merge` | PaperCraft `src/merge.rs` (265 lines, 4 tests): three-way merge of two edited JSON files. | Generic. NotesCraft `crates/sync` has its own conflict code; one merge core, two storage layers. |
| `craft-sync` | PaperCraft `src/sync.rs` plus `src/vfs/` (native fs or in-memory/IndexedDB fs): relay file sync with a base copy for merges, library mode, share links. | NotesCraft `crates/sync`, `crates/collab` (presence, rooms) solve the same problem. Converge on one file-sync core and keep live presence in `collab`. |
| `craft-update` | PaperCraft `src/update.rs` (signature-checked updater, 394 lines, security tests). | Any shipped app needs it; the security tests should live in one place. |
| `craft-activity` | PaperCraft `agent_pill`, `agent_glow`, tool-name to verb map in `src/ui/mod.rs`. | "The agent is working" feedback is the same in every app with an MCP surface. Small; goes into `craft-ui`. |
| UI scale | PaperCraft `theme::apply_ui_scale`. | Belongs in `craft-settings` as a standard key (`ui.scale`), applied by `craft-ui`. |

## Order

1. `craft-lottie`: take the PaperCraft copy (more complete, has fixtures), port NotesCraft's tests,
   switch both apps to the crate.
2. `craft-merge`, then `craft-sync`: move with their tests; NotesCraft adopts the merge core first,
   sync second.
3. `craft-motion`, then `craft-kit` into `craft-ui`.
4. `craft-update`, `craft-activity`, UI scale key.

Each step is one PR to `craft-libs`, then one rev bump per app. The app keeps a thin adapter
(palette, icon set, file layout); nothing app-specific moves.

## Design side

The same split applies to designs: one OpenDesign design-system package (variables, modes, tokens,
components, exported by `export_design_system`) per brand, imported by every app's design file with
`import_design_system`, instead of a copy per file. Code reads the tokens from the same package
(`get_design_spec` token names), so a colour changes in one place.

## Not done yet

Nothing here has been moved: it touches `craft-libs` and NotesCraft, so it needs a go-ahead and a
rev bump in each repo.
