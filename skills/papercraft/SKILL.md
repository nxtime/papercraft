---
name: papercraft
description: Drive the PaperCraft design app (native infinite-canvas design tool) through its MCP tools or the `papercraft-cli` shell. Use when asked to design screens, landing pages, dashboards or whiteboards in PaperCraft, to edit an open PaperCraft project, to build or change its variables, components and icons, or to lint and screenshot a design. Covers the build, screenshot, lint, fix loop, naming, layout recipes, tool cheat sheet, limits and troubleshooting. Pair with the papercraft-design skill for visual quality.
---

# papercraft

PaperCraft is a native Rust design app. You edit the project that is open in the running app: every tool call is one undo step and the window repaints live. Same tools over MCP (`http://127.0.0.1:29980/mcp` or the `papercraft-mcp` stdio bridge) and over the shell (`papercraft-cli`). Look and taste live in the `papercraft-design` skill; this one is the mechanics.

References: [compact-workflow.md](references/compact-workflow.md) (pages and groups, compressing, design-code sync, few tokens, shared work), [tools.md](references/tools.md) (every tool and argument), [html-subset.md](references/html-subset.md) (what `write_html` accepts), [recipes.md](references/recipes.md) (mobile flow, landing page, dashboard, design-system page).

## 1. Connect and orient

```sh
papercraft-cli call get_basic_info      # fails with exit 1 if the app is not running
```

* **Check the project is the right one.** `get_basic_info` returns `fileName` and `path`. Build only in a project that belongs to the work (a NotesCraft design file is for NotesCraft screens, not for redesigning PaperCraft). For a different product: `create_file {name}`, then `open_file {fileId}`, and say so. `open_file` swaps the project the user is looking at, so tell them. Pass `fileId` on later calls when more than one project is in play.
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

## 3b. Scripting big builds

For a screen with dozens of parts, drive the CLI from a short Python helper (`call(tool, **args)` that pipes JSON to `papercraft-cli call <tool> -` and parses stdout). Rules that saved time:
* One helper builds components from a list `[(name, html)]`: write them into a scratch row, `create_component` each, `move_nodes {moves:[{nodeId, parentId:"root"}]}`, then lay them out with real sizes (shelf packing, a 28px gap, 44px between rows) and delete the scratch row. Never space rows by a guessed height: tall components (a 250px card) overlap the next row.
* Generate variants in a loop (states, icons) instead of copying HTML by hand.
* Override text by walking the instance subtree and matching the nearest named ancestor, then batch one `set_text_content`.
* `update_styles` takes `updates:[{nodeIds:[...], styles:{...}}]`; `move_nodes` takes `moves:[...]`; `create_artboard` takes `name` and `styles:{width,height,left,top,...}`.
* Look at one artboard per screenshot, scale 1 for layout, 2 for small text. Crop with ImageMagick for details.

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

Artboards: mobile 390×844, tablet 768×1024, desktop 1440×900, large desktop 2560×1440 (a 1440p display; ask which the user uses and design that first). A narrow window (1280×800) is its own artboard: collapse the sidebar to an icon rail, make secondary actions icon-only with a tooltip, keep one labelled primary action.
Dot-grid canvas (no pattern fill exists): one `<svg>` with a single path of tiny round-capped dashes, absolute at 0,0 inside the canvas frame: `<svg style="position:absolute;left:0px;top:0px" width="W" height="H" viewBox="0 0 W H"><path d="M16 16h0.01M48 16h0.01..." stroke="var(--surface-2)" stroke-width="2.5" stroke-linecap="round" fill="none"/></svg>`. One vector layer, about 3,000 dots at a 32px step. Its colour is frozen at write time. Create with `height:"fit-content"` when the content decides the height. Replace margin with `gap` and `padding`.

Full flows: [recipes.md](references/recipes.md).

## 5. Tools cheat sheet

| I want to | Tool |
|---|---|
| Know the project, pages, artboards | `get_basic_info`, `get_tree_summary`, `get_children` |
| Know what is selected | `get_selection` |
| Inspect one node | `get_node_info`, `get_computed_styles`, `get_jsx` |
| Find nodes by text or style | `find_nodes` |
| See it | `get_screenshot` (`nodeId`, `scale` 1 or 2) |
| New screen | `create_artboard` (`width` and `height` required) |
| Add content | `write_html` (`insert-children` or `replace`) |
| Change styles or text | `update_styles`, `set_text_content`, `rename_nodes` |
| Whiteboard shapes, sticky notes, arrows | `create_shape` |
| Copy, move, delete | `duplicate_nodes`, `move_nodes`, `delete_nodes` |
| Tokens | `get_variables` (`format` css or json), `set_variables` (`mode`), `set_variable_mode` |
| Reuse | `create_component`, `create_instance`, `get_components`, `<x-paper-instance>` |
| Icons | `search_icons`, `<x-paper-icon name="set:name">`; CLI: `papercraft-cli icons add\|list\|search` |
| Check quality | `lint_design` |
| Export | `export` (PNG, SVG, PDF to files), `export_combined_pdf` |
| Point at things for the user | `select_nodes` (`zoom: true`) |
| Take back | `undo`, `redo` |
| Save | `finish_working_on_nodes` |

## 6. Known limits (design around them)

* **Font is Inter only**, regular and bold plus italic. No other families, no weights between. Build hierarchy from size, weight, color and space.
* **Variables are colors only**, for fills and text color. Strokes and gradients keep fixed colors. Spacing, radius and type sizes are plain numbers: keep them on the scale by discipline and `lint_design`.
* **No margin, grid or tables; no shadows, filters or transforms** (reported under `ignoredStyles`). Fake elevation with a border or a lighter surface.
* **No rotation.** Nodes are axis-aligned.
* **Components and their instances live on one page.** Put shared components on the page where they are used, or duplicate the component per page.
* **No variants or properties**: one component per variant (`Button / Primary`, `Button / Secondary`). No swap or detach tool.
* **Remote images are not fetched.** Use a local path or `data:` URI.
* **One project open at a time**; moving layers between pages is not supported.
* **Text boxes with fixed widths can wrap earlier than in a browser.** Check headlines in the screenshot and widen.
* **Instances are skipped by lint**; lint the main component.
* **Icons inside an instance cannot be swapped**, only text can (`swap_instance` swaps to another main). Need the same button with a different glyph? Generate one component per icon in a loop (`Icon button / undo-2`, `Icon button / redo-2`...), not one generic button.
* **An instance keeps the size of its main.** Overriding text with a longer string clips it (a `Saved` chip made from a `Chip` main loses its right padding; a `Share` button from a wider/narrower main sits off-centre). After `set_text_content` on instance text, `update_styles {width:"fit-content"}` on the instance root and on its text layers, or pass `style="width:fit-content;justify-content:center"` on `<x-paper-instance>`.
* **Text inside nested frames**: `set_text_content` needs the Text node id. Walk `get_children` down from the instance; instances copy the main's structure (`Meta > Label`, not a flat list).
* **No icon-colour follow for stroke sets** (Lucide, Tabler): the colour is frozen at write time, so a mode switch (Mocha to Latte) leaves icons as they were. Use a fill set (Phosphor, Material) when a design must switch modes.
* **Pages**: `create_page`, `rename_pages`, `delete_pages` (needs `confirm: true` when the page has layers; the last page stays). `move_nodes` accepts `toPageId` to copy between pages.
* **`get_screenshot` of `root` on a wide page** is scaled to fit and unreadable. Screenshot one artboard at a time. Hoisted components are not inside an artboard screenshot.
* **CLI saves images to the cwd** as `<tool>.png`; newer builds take `-o FILE`. Run the CLI from a scratch directory, or pass `-o`, and rename after.

## 7. Troubleshooting

| Symptom | Cause and fix |
|---|---|
| `papercraft-cli` exits 1, "isn't running" | The app is closed, no project is open, or the port differs. Start it and open a project; check `PAPERCRAFT_MCP_PORT`. |
| Call fails with an unknown node id | The id is from another page or was deleted. Re-read with `get_children`; pass `pageId`. |
| `write_html` reports `ignoredStyles` | You used a CSS property that has no equivalent (`margin`, `box-shadow`, `transform`...). Replace it, see [html-subset.md](references/html-subset.md). |
| Content is cut off | The artboard height is fixed. `update_styles` with `height: "fit-content"` or a larger number. |
| A color did not change with the theme | It was written as hex, or it is a stroke or gradient. Use `var(--name)` on fills and text; strokes need `update_styles` after a mode switch. |
| `create_instance` fails | The component is on another page. Instances must be on the component's page. |
| Icon does not appear | The set is not installed: `papercraft-cli icons add phosphor`, then `papercraft-cli icons search <words>` for the exact `set:name`. |
| Layer panel is a pile of `Frame` | You skipped `layer-name`. Fix with `rename_nodes` (batch) before finishing. |
| Text looks tiny in the screenshot | Screenshots default to scale 1. Use `scale: 2`. |
| The user's view jumped | You switched pages. Always pass `pageId` instead of opening other pages. |
