# Screens — plan

Built against `spec.md` (decisions resolved 2026-10-27).

## Model
- `Node::screen` and `Node::overlay` (bools, omitted when false). Shipped in project format v3, which is not released yet, so no extra version bump is needed: older builds already refuse v3.

## Editor (`src/editor/screens.rs`)
- `Role`, `refusal`, `declared`, `screen_of`, `candidates`, `device_of`, and `Editor::set_role`, which changes roles as one undo step.
- `coverage`: inside the layer's screen, it finds layers painted after it that are visible and either overlays or opaque (fill at 95% opacity or more), clips them to the screen, and returns the union area as a visible share. A layer under 1% visible counts as hidden; under 99% visible, as partly covered.
- `Editor::screens()` returns the declared screens, or the top-level frames when none are declared. "Screens scroll" therefore stops toggling sections.
- Frame labels: declared screens nested in sections get labels. A frame's own ancestors don't count as covering its label.

## MCP
- `set_screens {nodeIds, role}`.
- The guard runs on every tool in `GUARDED`. It reads `nodeId`, `nodeIds`, `parentId` and `targetNodeId` at any depth in the arguments.
  - It refuses hidden targets unless `allowHidden` is set.
  - It appends a `partlyCovered` or `editedWhileHidden` note to the result.
- `get_basic_info.screens`, `get_node_info.screen`, `lint_design.skippedHidden` (hidden layers are dropped from the findings).

## UI
- The frame inspector has Screen and Overlay toggles, shown only where the role is allowed.
- The page inspector has a "Declare N screens" button listing the candidates, as one undo step.
- "On click" lists `ed.screens()`.

## Known ceilings
- Covers are clipped to the screen only, not to frames in between.
- Lint autofix still edits hidden layers.
- Export and design-to-code don't use screens yet (planned for later).
