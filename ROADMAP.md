# Roadmap

PaperCraft is an early alpha: a local-first infinite-canvas design app that agents can drive. This
file lists what is still missing, in priority order, with an honest status. Done items are checked
against `src/mcp/tools.rs` and the UI code (2026-10-12). Milestone history is in
[PROGRESS.md](PROGRESS.md); known rendering and import limits are in the README.

Status: **done** (works today), **partial** (some of it works), **todo**.

## 1. Design systems (the current focus)

Agents can already build a design system through the tools; people cannot yet manage one in the UI.

| # | Item | Status | Notes |
|---|---|---|---|
| 1 | Inspector UI for variables | todo | Variables exist only through `get_variables` / `set_variables`. No panel to list, rename, edit or bind them. |
| 2 | Inspector UI for components | todo | Create component, detach and go-to-main are shortcuts (⌘⌥K / ⌘⌥B); there is no Components panel or asset browser. |
| 3 | Design lint | **done** | `lint_design`: WCAG contrast, 4px grid, type scale, tiny text, 44px targets, default names, hard-coded colors. |
| 4 | Lint auto-fix | todo | `lint_design` suggests a fix per issue; nothing applies it. Safe candidates: snap spacing, rename layers, relink colors to variables. |
| 5 | Lint in the UI | todo | Show issues on the canvas or in a panel, click to select. |
| 6 | Variable modes | partial | Done through tools: `set_variables {mode}` stores values per mode and `set_variable_mode` switches the whole project. Not yet in the UI, and not per frame or page. |
| 7 | Number variables (spacing, radius, font size) | todo | Variables are colors only, and only for fills and text color. |
| 8 | Stroke and gradient variables | todo | Strokes and gradients keep fixed colors. |
| 9 | Text styles | todo | Named type styles (Heading / L, Body / M) that update every text layer using them. |
| 10 | Component variants and properties | todo | Variants are separate components named `Button / Primary`. No variant sets, boolean or text properties. |
| 11 | Swap instance, detach by tool | partial | Detach is a shortcut in the UI. No `swap_instance` or `detach_instance` tool. |
| 12 | Cross-page and shared libraries | todo | Components and instances must live on one page. |
| 13 | Export design tokens | partial | `get_variables` with `format: "css"` or `"json"` returns tokens. No Tailwind or Style Dictionary output, no file export from the UI. |
| 14 | Apply a design system to existing work | todo | Map hard-coded colors and sizes onto variables and components in one pass. |

## 2. Agent workflow

| # | Item | Status | Notes |
|---|---|---|---|
| 15 | MCP and CLI tool surface | **done** | 40 tools, one undo step per call, `papercraft-cli list / schema / call`. |
| 16 | Agent skills | **done** | `skills/papercraft` and `skills/papercraft-design`. |
| 17 | Accessibility annotations | todo | Reading order, roles and alt text on layers, checked by the linter. |
| 18 | Code export from frames | todo | `get_jsx` returns inline-style JSX only. No React / HTML+CSS with variables, no Tailwind (the `tailwind` format is accepted and ignored). |
| 19 | Plugin and skill marketplace | todo | Skills are copied by hand today. |
| 20 | Screenshot diffing | todo | Compare two renders of a frame to confirm a change did what was asked. |
| 21 | Moving nodes between pages | todo | `move_nodes` stays on one page. |
| 22 | Remote images in `write_html` | todo | Only local paths and `data:` URIs. |

## 3. Design features

| # | Item | Status | Notes |
|---|---|---|---|
| 23 | Prototyping links and flows | todo | No click-through between frames. |
| 24 | Responsive constraints and breakpoint preview | partial | Constraints (left, right, stretch, center, scale) exist in the editor; there is no breakpoint preview or per-size variants. |
| 25 | Shadows, blur and effects | todo | CSS `box-shadow` and filters are reported as ignored. |
| 26 | Margin and grid layout | todo | Flexbox only. |
| 27 | Font choice | todo | Inter only. |
| 28 | Rotation | todo | Nodes are axis-aligned. |
| 29 | Overlay, dodge, burn, light blend modes on the canvas | todo | Exact in SVG and PDF export, approximated as Normal on the canvas. |
| 30 | Rounded frame clipping | todo | Clipping is a rectangle. |
| 31 | Non-destructive booleans | todo | The result is a flattened path. |

## 4. Files and collaboration

| # | Item | Status | Notes |
|---|---|---|---|
| 32 | Version history | todo | Undo per page and crash-safe autosave only; no named versions or restore. |
| 33 | Figma import | todo | SVG and Excalidraw import exist; `.fig` does not. |
| 34 | Multiplayer and cloud sync | todo | Local-first by design. If added, it must stay optional. |
| 35 | Release builds for Windows and Linux | todo | The code targets them (wgpu, winit); packaging today is the macOS DMG and the `papercraft-cli` zip. |

## 5. Quality

| # | Item | Status | Notes |
|---|---|---|---|
| 36 | Pen pressure on macOS tablets | todo | winit 0.30 does not expose pressure for mouse-like pen events. |
| 37 | Text shaping, ligatures, RTL | todo | No shaping. IME preedit is not shown. |
| 38 | Performance beyond 10k visible nodes | todo | Cached frames take about 3 ms at 10k nodes; not tuned for 100k. |
