---
name: papercraft
description: Drive the PaperCraft design app (native infinite-canvas design tool) through its MCP tools or the `papercraft-cli` shell. Use when asked to design screens, landing pages, dashboards or whiteboards in PaperCraft, to edit an open PaperCraft project, to build or change its variables, components and icons, or to lint and screenshot a design. Covers the build, screenshot, lint, fix loop, naming, layout recipes, tool cheat sheet, limits and troubleshooting. Pair with the papercraft-design skill for visual quality.
---

# papercraft

PaperCraft is a native Rust design app. You edit the project that is open in the running app: every tool call is one undo step and the window repaints live. Same tools over MCP (`http://127.0.0.1:29980/mcp` or the `papercraft-mcp` stdio bridge) and over the shell (`papercraft-cli`). Look and taste live in the `papercraft-design` skill; this one is the mechanics.

References: [tools.md](references/tools.md) (every tool and argument), [html-subset.md](references/html-subset.md) (what `write_html` accepts), [recipes.md](references/recipes.md) (mobile flow, landing page, dashboard, design-system page).

## 1. Connect and orient

```sh
papercraft-cli call get_basic_info      # fails with exit 1 if the app is not running
```

* Not running: ask the user to start PaperCraft and open a project (or `create_file` then `open_file`). `PAPERCRAFT_MCP_PORT` must match if the port was changed. Do not try to launch the GUI yourself unless asked.
* Read `get_basic_info` (pages, artboards, sizes, fonts), then `get_selection` (what the user is looking at) and `get_variables` and `get_components`. Reuse what exists before creating anything.
* `pageId` defaults to the page the user is viewing, which can change between calls: pass it explicitly. Node ids look like `3fa9c1:14`; never show them to the user.
* With MCP tools you do not need the CLI; with a shell, `papercraft-cli list` and `papercraft-cli schema <tool>` give the schemas on demand. Pass big JSON on stdin: `... | papercraft-cli call write_html -`.

## 2. The loop: build, look, lint, fix

1. **Tokens first.** `set_variables` once (roles: `bg`, `surface`, `border`, `text`, `text-muted`, `accent`, `on-accent`, `danger`). Add a second mode for dark or light. No hex in the design afterwards.
2. **Components first.** Anything that appears twice (button, field, chip, row, tab bar) is built once as an artboard, `create_component`, then placed with `create_instance`.
3. **Build in small steps.** `create_artboard`, then one `write_html` per visual group (header, card, row) so the user sees progress. Name every layer with `layer-name`.
4. **Look.** `get_screenshot` of the artboard after each group or two. Scale 2 to read small text. Read it as a designer: hierarchy, alignment, spacing rhythm, clipping, wrapping.
5. **Lint.** `lint_design` on the artboard. Fix every error and warning; consider the `info` items. Run it again until errors and warnings are zero.
6. **Fix with the smallest edit.** `update_styles` and `set_text_content` on the existing nodes, not a rewrite. Rewrite a group with `write_html` mode `replace` only when its structure is wrong.
7. **Finish.** `finish_working_on_nodes` (saves). Tell the user what you made in a sentence or two, with artboard names, not ids.

Lint passes are necessary, not sufficient: contrast and grids say nothing about composition. The screenshot is the real test.

## 3. Naming

* Layers: `Noun` or `Component / Variant` (`Button / Primary`, `Tab / Today`, `Card / Task`, `Icon / gear`). Slashes group the layers panel.
* Screens (artboards): the screen's name (`Sign in`, `Task detail`), not `Frame 12`. Add a size suffix only when you have several breakpoints (`Landing / 1440`).
* Variables: roles, not hues (`surface`, `text-muted`, `on-accent`), kebab-case.
* Lint flags default names (`Frame`, `Rectangle`, `Group`) on layers that have children.

## 4. Layout recipes (flexbox is the only layout)

```html
<!-- row, vertically centered, 12px gap -->
<div layer-name="Row" style="display:flex;align-items:center;gap:12px">…</div>
<!-- column with 16px rhythm and 24px padding -->
<div layer-name="Stack" style="display:flex;flex-direction:column;gap:16px;padding:24px">…</div>
<!-- label left, value right -->
<div layer-name="Meta Row" style="display:flex;justify-content:space-between;align-items:center">…</div>
<!-- fill the remaining width or height -->
<div layer-name="Main" style="flex-grow:1">…</div>
<!-- pill button, 44px tall: 12 + 20 line height + 12 -->
<div layer-name="Button / Primary" style="display:flex;justify-content:center;padding:12px 24px;background:var(--accent);border-radius:12px"><div layer-name="Label" style="font-size:16px;font-weight:600;line-height:20px;color:var(--on-accent)">Save</div></div>
<!-- icon + label -->
<div layer-name="Nav / Home" style="display:flex;align-items:center;gap:12px"><x-paper-icon name="phosphor:house" size="20" color="var(--text-muted)"/><div layer-name="Label" style="font-size:16px;color:var(--text-muted)">Home</div></div>
```

Artboards: mobile 390×844, tablet 768×1024, desktop 1440×900. Create with `height:"fit-content"` when the content decides the height. Replace margin with `gap` and `padding`.

Full flows: [recipes.md](references/recipes.md).

## 5. Tools cheat sheet

| I want to | Tool |
|---|---|
| Know the project, pages, artboards | `get_basic_info`, `get_tree_summary`, `get_children` |
| Know what is selected | `get_selection` |
| Inspect one node or export code | `get_node_info`, `get_computed_styles`, `get_jsx` (`inline` \| `tailwind` \| `react` \| `angular` \| `html` \| `egui`) |
| Extract AI design-to-code spec | `get_design_spec` (`nodeId`, `target`: `react` \| `tailwind` \| `angular` \| `html` \| `egui`) |
| Verify generated code against design | `verify_code` (`nodeId`, `target`, `code`) |
| Compare two frames or subtrees | `diff_frames` (`beforeId`, `afterId`) |
| Find nodes by text or style | `find_nodes` |
| See it | `get_screenshot` (`nodeId`, `scale` 1 or 2) |
| New screen | `create_artboard` (`width` and `height` required) |
| Add content | `write_html` (`insert-children` or `replace`) |
| Change styles or text | `update_styles`, `set_text_content`, `rename_nodes` |
| Whiteboard shapes, sticky notes, arrows | `create_shape` |
| Copy, move, delete | `duplicate_nodes`, `move_nodes`, `delete_nodes` |
| Tokens | `get_variables` (`format`: `css` \| `json` \| `tailwind`), `set_variables` (`mode`), `set_tokens`, `set_variable_mode` |
| Reuse & components | `create_component`, `create_instance`, `swap_instance`, `detach_instance`, `import_components`, `get_components`, `<x-paper-instance>` |
| Icons | `search_icons`, `<x-paper-icon name="set:name">`; CLI: `papercraft-cli icons add\|list\|search` |
| Check & auto-fix quality | `lint_design` (`fix: true` to auto-fix grid/scale/variables/targets) |
| Export | `export` (PNG, SVG, PDF to files), `export_combined_pdf` |
| Point at things for the user | `select_nodes` (`zoom: true`) |
| Take back | `undo`, `redo` |
| Save | `finish_working_on_nodes` |

## Design-to-Code Handoff & Enforcement (React, Angular, HTML/CSS, Rust egui)

When implementing a PaperCraft design in production code (React/TSX, Tailwind JSX, Angular standalone components, HTML + CSS, or Rust `egui`), follow this mandatory three-step protocol:

1. **Never implement from a screenshot alone.** Always call `get_design_spec` (`nodeId`, `target: "react" | "tailwind" | "angular" | "html" | "egui"`) to extract the structured layout tree, exact auto-layout flow/sizing (`fixed`, `hug`, `fill`), design tokens (`--var`), component instances, and reference code.
2. **Write the target code** following the spec's layout flow (Flexbox/Grid for web; `egui::Frame` + `ui.horizontal`/`ui.vertical` + `tokens` module for Rust `egui`), binding all color variables instead of hardcoding hex values, and never using `position: absolute` on flow children.
3. **Enforce with `verify_code`**: Pass the written code to `verify_code({nodeId, target, code})`. Fix any reported `missing-text`, `hardcoded-color`, `forbidden-absolute`, `missing-layout`, `missing-component`, or `spacing-drift` violations until `passed: true` (`score: 100`).

## 6. Known limits (design around them)

* **Font is Inter only**, regular and bold plus italic. No other families, no weights between. Build hierarchy from size, weight, color and space.
* **Color variables (`set_variables`) apply to fills and text color**; strokes and gradients keep fixed colors. Non-color tokens (`set_tokens`) provide shared spacing, radius, and shadow values expanded on write; `lint_design` (`fix: true`) enforces scale and 4px grid discipline.
* **No CSS margin, grid or tables in `write_html`; no CSS filters or transforms** (reported under `ignoredStyles`). Replace margin with `gap` and `padding`.
* **No rotation.** Nodes are axis-aligned.
* **Components and their instances live on one page** (use `import_components` to copy main components from another page). Variants are modeled as separate components (`Button / Primary`, `Button / Secondary`) and can be swapped in place with `swap_instance` (preserving overrides) or unlinked with `detach_instance`.
* **Supported imports, diffs, and code handoff**: `.fig` file import is supported, `diff_frames` compares two frames/subtrees at the property level, `get_jsx` exports `inline`, `tailwind`, `react`, `angular`, `html`, or `egui`, and `get_design_spec` + `verify_code` provide full AI design-to-code handoff and enforcement.
* **Remote images are not fetched.** Use a local path or `data:` URI.
* **One project open at a time**; moving layers between pages is not supported.
* **Text boxes with fixed widths can wrap earlier than in a browser.** Check headlines in the screenshot and widen.
* **Instances are skipped by lint**; lint the main component.

## 7. Troubleshooting

| Symptom | Cause and fix |
|---|---|
| `papercraft-cli` exits 1, "isn't running" | The app is closed, no project is open, or the port differs. Start it and open a project; check `PAPERCRAFT_MCP_PORT`. |
| Call fails with an unknown node id | The id is from another page or was deleted. Re-read with `get_children`; pass `pageId`. |
| `write_html` reports `ignoredStyles` | You used a CSS property that has no equivalent (`margin`, `transform`...). Replace it, see [html-subset.md](references/html-subset.md). |
| Content is cut off | The artboard height is fixed. `update_styles` with `height: "fit-content"` or a larger number. |
| A color did not change with the theme | It was written as hex, or it is a stroke or gradient. Use `var(--name)` on fills and text; strokes need `update_styles` after a mode switch. |
| `create_instance` fails | The component is on another page. Instances must be on the component's page (or run `import_components` first). |
| Icon does not appear | The set is not installed: `papercraft-cli icons add phosphor`, then `papercraft-cli icons search <words>` for the exact `set:name`. |
| Layer panel is a pile of `Frame` | You skipped `layer-name`. Fix with `rename_nodes` (batch) before finishing. |
| Text looks tiny in the screenshot | Screenshots default to scale 1. Use `scale: 2`. |
| The user's view jumped | You switched pages. Always pass `pageId` instead of opening other pages. |
