# Roadmap

PaperCraft is an early alpha: a local-first infinite-canvas design app that agents can drive. This
file lists what is still missing, in priority order, with an honest status. Done items are checked
against `src/mcp/tools.rs` and the UI code (2026-10-12); rows 7, 8, 12, 15, 23, 25, 32, 34, 39, 40, 44, 45, 46 and section 6 were updated 2026-10-21; rows 15, 47 and 48 on 2026-10-22. Milestone history is in
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
| 8 | Stroke and gradient variables | **done** | Strokes, text color and gradient stops (`bind_variable` `gradient-stop-N`) bind to variables. |
| 9 | Text styles | **done** | `set_text_style`: named type styles (Heading / L, Body / M) that update every text layer using them, in one undo step. |
| 10 | Component variants and properties | **done** | Main components carry text, bool, icon and enum properties; `merge_variants` folds per-state components into one, with one enum per axis for names like `Size=M, State=hover`. |
| 11 | Swap instance, detach by tool | **done** | `swap_instance` and `detach_instance` MCP tools + UI controls in Design tab and Inspector. |
| 12 | Cross-page and shared libraries | **done** | Instances of a main on any page; publish a project as a library, enable, update and disable it in other projects (Libraries modal, `*_library` tools). |
| 13 | Export design tokens | **done** | `get_variables` with `format: "css"`, `"json"` (W3C DTCG + modes), or `"tailwind"` (`@theme`), plus Copy CSS / Copy JSON in Design tab. |
| 14 | Apply a design system to existing work | partial | `lint_design {fix: true}` links matching literal colors to variables, snaps spacing/type scale, binds literal gap, padding, radius and font-size numbers to number variables of the same value, and applies an exactly matching text style; each change is listed in `fixes`. |

## 2. Agent workflow

| # | Item | Status | Notes |
|---|---|---|---|
| 15 | MCP and CLI tool surface | **done** | 107 tools, one undo step per call, `papercraft-cli list / schema / call`. |
| 16 | Agent skills | **done** | `skills/papercraft` and `skills/papercraft-design`. |
| 17 | Accessibility annotations | **done** | `set_accessibility`: alt text, decorative, roles, heading levels and reading order on layers, checked by `lint_design` and used by `export_site`. |
| 18 | Code export from frames | **done** | `get_jsx` supports `format: "inline"`, `"tailwind"` (Tailwind utility classes), and `"html"` (HTML + CSS rules with `:root` variables). |
| 19 | Plugin and skill marketplace | todo | Skills are copied by hand today. |
| 20 | Screenshot diffing | **done** | `diff_frames` tool (`src/mcp/diff.rs`) for structural/property frame diffing + `diff_rgba` pixel diffing with highlight PNG. |
| 21 | Moving nodes between pages | **done** | `move_nodes {toPageId}` keeps instances linked, notes attached, and remaps prototype links and connectors. |
| 22 | Remote images in `write_html` | **done** | http(s) images are fetched from public addresses only, with redirects re-checked, a 10 MB cap and a 15 s deadline. |
| 46 | Design from the CLI without the app | **done** | `papercraft-cli call --offline` and `shell` run the tools in-process on the project store; screenshots use a GPU or the software adapter. |

## 3. Design features

| # | Item | Status | Notes |
|---|---|---|---|
| 23 | Prototyping links and flows | **done** | Prototype tab in the inspector; Present mode follows links with a camera glide. While-hovering links (reverted on leave) and smart transitions (layers with the same name tween position, size, opacity and fill) are in; pressed states for components todo. |
| 24 | Responsive constraints and breakpoint preview | partial | Constraints exist in the editor; `export_site` merges frames named `Home @mobile` or `Home @768` into one page with media queries. View > Breakpoint Preview shows the selected screen at 375 / 768 / 1280 or a variant's width from a copy of the document (desktop app; not on the web build). |
| 25 | Shadows, blur and effects | **done** | Drop shadow, layer blur and background blur are inspector effects (`filter: blur()` and `backdrop-filter: blur()` in CSS, Tailwind and SVG; PDF export drops the blurs). Rendered through offscreen layers (`src/render/layers.rs`). |
| 26 | Margin and grid layout | partial | Wrapped-line cross alignment, per-line fill/align, and `layout_grid` track layout (`src/layout.rs`) implemented; `write_html` honours CSS margin (shorthand, per side, `auto`; block siblings collapse, flex margins add). |
| 27 | Font choice | **done** | `install_font` adds a .ttf or .otf to the project; `font-family` applies it to a text layer. |
| 28 | Rotation | **done** | `rotation` in degrees around the centre (CSS `rotate()`; layout and `rect` stay unrotated), page format v9. Rendered, hit-tested and clipped along the rotated outline; the canvas has a rotate knob (Shift snaps to 15°) and the inspector a Rotate field; `transform: rotate()` in HTML, CSS, Tailwind `rotate-[..]`, SVG and PDF. Known limits: resizing a turned layer by handles (use W/H), snapping, guides, connectors and drop targets ignore the turn, and a multi-selection has no knob. |
| 29 | Overlay, dodge, burn, light blend modes on the canvas | **done** | All 16 modes draw on the canvas; the ones the GPU cannot blend in fixed function read the backdrop through an offscreen layer. |
| 30 | Rounded frame clipping | **done** | A clipping frame with a corner radius (per corner too) masks its children to the rounded shape on the canvas; nested clips work. Layers nest three deep, deeper ones draw into their parent unmasked. |
| 31 | Non-destructive booleans | **done** | Union, subtract, intersect and exclude make a live group (page format v10); operands stay editable, the result is cached until one changes; Flatten bakes it into a path. |

## 4. Files and collaboration

| # | Item | Status | Notes |
|---|---|---|---|
| 32 | Version history | **done** | Named and automatic saved versions per page: rename, restore, delete (`src/versions.rs`). Restore now includes variables and tokens; right-click a version to compare it with the current page, or pick two versions (read-only, list only for two saved versions). |
| 33 | Figma import | **done** | `.fig` binary (Kiwi wire decoder + zip container) and Figma REST API JSON import in `src/io/fig.rs`. |
| 34 | Multiplayer and cloud sync | **done** | Optional shared projects through the relay, with merge of concurrent edits to one page. Presence: other devices' cursors and selections, each with the device name and a colour, fading when idle and gone when the device leaves; in memory only (never in the document, the files or undo), native and browser alike. Stays optional. |
| 35 | Release builds for Windows and Linux | **done** | Releases ship Windows (MSI and portable zip), macOS and Linux builds. |

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
| 40 | Review agent changes | **done** | Id-paired diff, grouped review, canvas overlay, per-change revert and accept all; version compare reuses the same diff and overlay. |
| 41 | Agent presence on the canvas | **done** | Layers an agent touches get a fading outline and a cursor labelled with the verb and client; the chat can stop its own run. An external MCP client that called lately gets a Pause control on the canvas: its editing calls wait (reads go on) until you resume, or are refused with a "paused by the user" error after 100 s; none is lost or half applied. |
| 42 | `get_context` tool | **done** | Selection, outline with token estimates, variables, components and lint summary in one call, so an agent orients in one round trip. Big reads take `maxTokens` and report `estimatedTokens` and `truncated`. |
| 43 | Comments assigned to agents | todo | `@agent` in a thread triggers an edit and a reply. Needs threaded comments. |
| 44 | In-app prompt bar | **done** | Chat mode: Claude Code, Codex or your own Anthropic key; see `docs/chat.md`. |
| 45 | Correct tool annotations | **done** | `lint_design` no longer sends `readOnlyHint`; `fix: true` edits are classified. |

## 7. Annotations and chat agents

| # | Item | Status | Notes |
|---|---|---|---|
| 47 | Design annotations | **done** | Notes with leader lines and categories, templates per project and page, view modes (⌘⇧A), Annotate tool (N), side view, inspector section, six MCP tools; see `docs/annotations.md`. |
| 48 | ACP agents in Chat | **done** | Any ACP agent on the PATH (OpenCode works; Cursor needs `cursor-agent login`), npx agents after enabling, custom agents; see `docs/chat.md`. |
| 48b | Free agents | **done** | OpenCode and Pi on free models with `Free (auto)` rotation, Pi provider keys, an install dialog per OS; see `specs/free-agents`. |
