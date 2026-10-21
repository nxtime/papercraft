# Roadmap

PaperCraft is an early alpha: a local-first infinite-canvas design app that agents can drive. This
file lists what is still missing, in priority order, with an honest status. Done items are checked
against `src/mcp/tools.rs` and the UI code (2026-10-12); rows 7, 8, 12, 15, 23, 25, 32, 34, 39, 40, 45, 46 and section 6 were updated 2026-10-21. Milestone history is in
[PROGRESS.md](PROGRESS.md); known rendering and import limits are in the README.

Status: **done** (works today), **partial** (some of it works), **todo**.

## 1. Design systems (the current focus)

Agents can already build a design system through the tools; people cannot yet manage one in the UI.

| # | Item | Status | Notes |
|---|---|---|---|
| 1 | Inspector UI for variables | **done** | Design tab lists/renames/adds/deletes variables & modes; Inspector binds/unlinks solid fills to variables. |
| 2 | Inspector UI for components | **done** | Design tab searches components, inserts instances, swaps/resets/detaches instances; Inspector header swaps/resets instances. |
| 3 | Design lint | **done** | `lint_design`: WCAG contrast, 4px grid, type scale, tiny text, 44px targets, default names, hard-coded colors. |
| 4 | Lint auto-fix | **done** | `lint_design {fix: true}` and UI `Fix` / `Fix all`: snaps spacing to 4px grid, snaps type scale, enforces 44px targets, links matching colors to variables. |
| 5 | Lint in the UI | **done** | Live Lint section in Design tab; click issue to select & zoom, or click `Fix` / `Fix all`. |
| 6 | Variable modes | **done** | Mode selector + create/delete modes in Design tab and via `set_variables {mode}` / `set_variable_mode`. |
| 7 | Number variables (spacing, radius, font size) | **done** | Typed number, string and boolean variables in collections with groups and aliases; fields bind (size, gap, padding, radius, font size, line height, opacity, visible) in the inspector and `bind_variable`. |
| 8 | Stroke and gradient variables | partial | Strokes and text color bind to variables. Gradients keep fixed colors. |
| 9 | Text styles | todo | Named type styles (Heading / L, Body / M) that update every text layer using them. |
| 10 | Component variants and properties | todo | Variants are separate components named `Button / Primary`. No variant sets, boolean or text properties. |
| 11 | Swap instance, detach by tool | **done** | `swap_instance` and `detach_instance` MCP tools + UI controls in Design tab and Inspector. |
| 12 | Cross-page and shared libraries | **done** | Instances of a main on any page; publish a project as a library, enable, update and disable it in other projects (Libraries modal, `*_library` tools). |
| 13 | Export design tokens | **done** | `get_variables` with `format: "css"`, `"json"` (W3C DTCG + modes), or `"tailwind"` (`@theme`), plus Copy CSS / Copy JSON in Design tab. |
| 14 | Apply a design system to existing work | partial | `lint_design {fix: true}` links matching literal colors to variables and snaps spacing/type scale. |

## 2. Agent workflow

| # | Item | Status | Notes |
|---|---|---|---|
| 15 | MCP and CLI tool surface | **done** | 61 tools, one undo step per call, `papercraft-cli list / schema / call`. |
| 16 | Agent skills | **done** | `skills/papercraft` and `skills/papercraft-design`. |
| 17 | Accessibility annotations | todo | Reading order, roles and alt text on layers, checked by the linter. |
| 18 | Code export from frames | **done** | `get_jsx` supports `format: "inline"`, `"tailwind"` (Tailwind utility classes), and `"html"` (HTML + CSS rules with `:root` variables). |
| 19 | Plugin and skill marketplace | todo | Skills are copied by hand today. |
| 20 | Screenshot diffing | **done** | `diff_frames` tool (`src/mcp/diff.rs`) for structural/property frame diffing + `diff_rgba` pixel diffing with highlight PNG. |
| 21 | Moving nodes between pages | todo | `move_nodes` stays on one page. |
| 22 | Remote images in `write_html` | todo | Only local paths and `data:` URIs. |
| 46 | Design from the CLI without the app | **done** | `papercraft-cli call --offline` and `shell` run the tools in-process on the project store; screenshots use a GPU or the software adapter. |

## 3. Design features

| # | Item | Status | Notes |
|---|---|---|---|
| 23 | Prototyping links and flows | **done** | Prototype tab in the inspector; Present mode follows links with a camera glide. Smart transitions and hover states todo. |
| 24 | Responsive constraints and breakpoint preview | partial | Constraints (left, right, stretch, center, scale) exist in the editor; there is no breakpoint preview or per-size variants. |
| 25 | Shadows, blur and effects | partial | Drop shadow is an inspector effect. Layer and background blur todo (needs a renderer pass). |
| 26 | Margin and grid layout | partial | Wrapped-line cross alignment, per-line fill/align, and `layout_grid` track layout (`src/layout.rs`) implemented; CSS margin still ignored. |
| 27 | Font choice | todo | Inter only. |
| 28 | Rotation | todo | Nodes are axis-aligned. |
| 29 | Overlay, dodge, burn, light blend modes on the canvas | todo | Exact in SVG and PDF export, approximated as Normal on the canvas. |
| 30 | Rounded frame clipping | todo | Clipping is a rectangle. |
| 31 | Non-destructive booleans | todo | The result is a flattened path. |

## 4. Files and collaboration

| # | Item | Status | Notes |
|---|---|---|---|
| 32 | Version history | **done** | Named and automatic saved versions per page: rename, restore, delete (`src/versions.rs`). Restore now includes variables and tokens; compare todo. |
| 33 | Figma import | **done** | `.fig` binary (Kiwi wire decoder + zip container) and Figma REST API JSON import in `src/io/fig.rs`. |
| 34 | Multiplayer and cloud sync | partial | Optional shared projects through the relay, with merge of concurrent edits to one page. Presence (cursors, selections) todo. Stays optional. |
| 35 | Release builds for Windows and Linux | todo | The code targets them (wgpu, winit); packaging today is the macOS DMG and the `papercraft-cli` zip. |

## 5. Quality

| # | Item | Status | Notes |
|---|---|---|---|
| 36 | Pen pressure on macOS tablets | todo | winit 0.30 does not expose pressure for mouse-like pen events. |
| 37 | Text shaping, ligatures, RTL | todo | No shaping. IME preedit is not shown. |
| 38 | Performance beyond 10k visible nodes | todo | Cached frames take about 3 ms at 10k nodes; not tuned for 100k. |

## 6. Agent runs (next)

Spec: [specs/agent-runs/spec.md](specs/agent-runs/spec.md), tasks: [specs/agent-runs/tasks.md](specs/agent-runs/tasks.md). Design: `PaperCraft` file, page `Agent runs`.

| # | Item | Status | Notes |
|---|---|---|---|
| 39 | Agent runs and auto-versions | **done** | Editing calls group into runs; edited pages get auto-versions; runs persist, revert per page, and prune to 50. |
| 40 | Review agent changes | **done** | Id-paired diff, grouped review, canvas overlay, per-change revert and accept all; version compare UI remains todo. |
| 41 | Agent presence on the canvas | todo | Outline the node a tool call touches, label it, pause or stop the run. |
| 42 | `get_context` tool | todo | Selection, variables, nearby components and lint summary in one call, so an agent orients in one round trip. |
| 43 | Comments assigned to agents | todo | `@agent` in a thread triggers an edit and a reply. Needs threaded comments. |
| 44 | In-app prompt bar | todo | Run a prompt on the selection through the user's own key or local CLI. Largest scope: provider config, key storage, cost. |
| 45 | Correct tool annotations | **done** | `lint_design` no longer sends `readOnlyHint`; `fix: true` edits are classified. |
