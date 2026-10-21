# Platform: native Rust UI (egui on wgpu)

For Rust apps and shared UI crates built on egui. The principles are the same; the mechanisms
differ. Confidence is stated per claim: "verified" means fetched or read in this
research; "to check" means consult egui/AccessKit docs or the code first.

## What is verified
- egui re-exports AccessKit (`egui::accesskit`, `egui::Role`) and has an
  `accessibility` module (egui `lib.rs` read 2026-10-21). That is the hook for
  screen-reader support. Whether the `accesskit` Cargo feature is enabled in
  your app is not verified: check `Cargo.toml`.
- A typical egui UI kit has a motion module (easing, spring, hover, appear,
  reduced-motion switch) and a widget module (button kinds, chip, toggle,
  checkbox, select, slider, banner, empty state, key cap). Role-named variables
  with modes (Light, Dark), exported as CSS or a DTCG-style tokens object, work
  well as the token source.

## Mapping the rules to egui

| Rule | Native approach |
|---|---|
| A11Y-14 accessible name/role/value | Give every widget an AccessKit node: set the label and role (`Response::widget_info`, `ui.ctx().accesskit_node_builder`). Icon-only buttons need a label. To check against the egui version in use. |
| A11Y-07/08 keyboard | egui focus moves with Tab; make custom widgets `Sense::focusable_noninteractive()` or interactive senses so they enter the focus order; draw a visible focus ring from `response.has_focus()`. Provide arrow-key navigation inside lists, tabs and menus. |
| A11Y-09 target size | Minimum interactive size 24 x 24 logical px; 44 for touch. In egui set `style.spacing.interact_size` and `button_padding`; for icon buttons allocate an expanded rect larger than the glyph. |
| A11Y-01/02 contrast | Run the token pairs through `scripts/contrast.py` for every theme. Egui `Visuals` colors derive from tokens, never raw literals. Disabled state still meets 3:1 for non-text where it conveys information. |
| A11Y-10/11 focus ring | Draw a 2 px ring offset by 2 px, color from `focus` token with 3:1 against both the widget and the surface; do not rely on the default faint egui hover/focus stroke. |
| MOT-02/04 durations, interruptibility | `ctx.animate_value_with_time` and springs retarget naturally: always animate toward a target value rather than replaying a timeline. Request repaints only while animating. |
| MOT-07 reduced motion | Read the OS preference where available (not provided by egui by default: to check) and expose an app setting that sets a single motion scale to 0; all easing/spring helpers multiply by it. |
| INT-06 undo over confirm | A toast/banner with Undo; keep a command history in the document model; autosave plus project history. |
| FBK-02 states | Each `kit` widget implements default, hover, focus, pressed, disabled, selected, error. Add a gallery screen that renders every state in every theme and is screenshot-tested. |
| A11Y-13 user settings | Honor system font scale / UI scale, dark mode, high contrast (offer a high-contrast theme built from tokens). |
| INT-03 menu-aim | Nested menus and context menus need diagonal tolerance; egui's default submenus close on leaving the item (to check): implement a safe-triangle timer. |
| A11Y-12/18 gesture alternatives | Canvas pan/zoom/drag must have keyboard and button alternatives (arrow nudge, +/- zoom, numeric inspector fields). |

## Painter-drawn widgets own their accessibility (from the 2026-10-21 audit)
A widget drawn with `painter().text()` and `allocate_exact_size` gets none of egui's built-in
behavior: no focus stroke, no accessibility role, name or state. Every such widget must (1) draw a
focus ring when `response.has_focus()` (A11Y-10, A11Y-11), (2) call `response.widget_info(...)` with
role, label and state (A11Y-14), (3) handle its keyboard map (A11Y-07), and (4) show a non-fill
boundary at 3:1 when its resting fill is close to the surface (A11Y-02). Keep these in one shared
helper so a new widget cannot forget them. `Cargo.lock` must contain `accesskit_winit` for the
native screen-reader bridge to exist; check it.

## Rendering notes
- Pixel snapping: align strokes and text to physical pixels at the current
  `pixels_per_point` to avoid blurry 1 px borders (VIS-05, optical alignment).
- Shadows: layered (two) soft shadows from tokens, not one hard shadow (VIS-11).
- Text: 16 px equivalent body, tabular figures for numeric fields and the
  inspector (VIS-06).
- Web (wasm) build: WebGPU shader rule: no
  `textureSample` inside non-uniform control flow; use `textureSampleGrad`.
  Browser a11y of a canvas app is limited: AccessKit on web requires a
  parallel DOM tree (to check) so offer an accessible text/keyboard path for
  critical flows.

## Gap analysis to run next (not yet done)
Audit your widget kit against `checklists/a11y-audit.md` and
`checklists/design-review.md`; produce a ranked fix list per widget. Output
goes in your UI library repo, referencing rule IDs.
