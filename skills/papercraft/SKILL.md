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
* App closed (server, CI)? `papercraft-cli call --offline ... --project <name>` runs the same tools on the project store and saves after every editing call; `papercraft-cli shell` keeps one session for many calls and reads scripts from stdin. It is refused while the app has that project open. See `docs/agents.md`.
* With MCP tools you do not need the CLI; with a shell, `papercraft-cli list` and `papercraft-cli schema <tool>` give the schemas on demand. Pass big JSON on stdin: `... | papercraft-cli call write_html -`.
* **Working in the user's open project: use `papercraft-cli --online call …`** (or `PAPERCRAFT_CLI_NO_OFFLINE=1`). Without it, a call made while the app is starting or stuck silently falls back to offline writes on the files. `stderr` warns when the CLI and app versions differ: tell the user, don't ignore it.
* **One call at a time per agent.** No background loops or parallel batches against the app; a backlog of screenshot calls makes the app unresponsive for every agent.
* **App stops answering** ("did not answer in time", 0% CPU): usually a macOS dialog (a privacy prompt such as "access files on a removable volume") or a modal in the app. Tell the user and wait; never kill, quit or restart their PaperCraft.
* **See what the user sees**: `get_basic_info` / `get_selection` include `viewport {pageId, rect, zoom}`. `set_viewport {pageId, nodeIds | rect, zoom?}` switches page and frames things for the user; it refuses while they are moving the canvas (pass `force` only when they asked you to show something).

## 2. The loop: build, look, lint, fix

1. **Tokens first.** `set_variables` once (roles: `bg`, `surface`, `border`, `text`, `text-muted`, `accent`, `on-accent`, `danger`). Add a second mode for dark or light. No hex in the design afterwards.
2. **Components first.** Anything that appears twice (button, field, chip, row, tab bar) is built once as an artboard, `create_component`, then placed with `create_instance`.
3. **Build in small steps.** `create_artboard`, then one `write_html` per visual group (header, card, row) so the user sees progress. Name every layer with `layer-name`.
4. **Look.** `get_screenshot` of the artboard after each group or two. Scale 2 to read small text. Read it as a designer: hierarchy, alignment, spacing rhythm, clipping, wrapping.
5. **Lint.** `lint_design` on the artboard. Fix every error and warning; consider the `info` items. Run it again until errors and warnings are zero.
6. **Fix with the smallest edit.** `update_styles` and `set_text_content` on the existing nodes, not a rewrite. Rewrite a group with `write_html` mode `replace` only when its structure is wrong.
7. **Finish.** `finish_working_on_nodes` (saves). Tell the user what you made in a sentence or two, with artboard names, not ids.

Lint passes are necessary, not sufficient: contrast and grids say nothing about composition. The screenshot is the real test.

## 2b. Big jobs and several agents on one project

Lessons from redesigning a 3-page, 130-screen project with up to six agents at once:

* **Split by section, not by page.** Give each agent named sections (frame ids) and forbid the rest. New sections go below the current lowest one; re-read positions right before creating, since another agent may have just added one.
* **Section layout**: a section frame holds a title and rows of screen slots (a route label above a fixed-size screen). Keep 640 px between screens and rows and 1600 px between sections, so notes fit beside screens. Stack sections in one column.
* **Recreate, then delete.** Build the new screen next to the old one, finish it, then delete the old one, and record the deleted ids and names in the report. A cleanup agent once deleted buttons and lost icons by editing in place.
* **Reuse the design system across pages**: `create_instance` / `swap_instance` / `<x-paper-instance>` take a main from any page and stay linked. Never `import_components` to use a component: that copies mains onto the page, and instances made from the copies stop following the design system. `lint_design` flags hand-made look-alikes as `not-an-instance` (a different icon is a different component); fix with `write_html` mode replace and the `<x-paper-instance>` it names.
* **QA pass after a build** (screen by screen at scale 1, one call at a time): text layers with stale fixed widths clip labels ("In prog", "New sess"): set `width: fit-content`. Sidebars and panels fixed at 900 px stop short on taller screens: make them fill the height and let the screen hug with a 900 px minimum. Tags and chips overflowing their box, cards overlapping buttons, footers not pinned, leftover `Text` / `Frame` names, gendered copy about people whose gender is unknown (use their name or neutral wording).
* **Report per agent** in `specs/<feature>/<agent>-report.md`: what changed (ids), why (playbook rule ids from `playbook search`), open questions. Unknown product facts are written as questions for the team, never invented.

## 2c. Annotations

* Read the page's template first (`get_annotation_template {pageId}`), then one batched `annotate {pageId, notes:[{node, text, type}]}` per section, then `annotate {pageId, frame: <section id>, tidy: true}` (returns `{tidied: n}`; 0 means already placed).
* Content: the first line is the complete rule in 60 characters or fewer; the body is condition → outcome with values as `Field: value` lines; attach to the exact element, not the screen; few high-value notes; unknowns as `? (team)`. Rules: `playbook rule ANN`.
* Check placement with `get_screenshot {nodeId: <screen slot>, annotations: true, scale: 0.5}`; plain screenshots do not show notes. Cards must sit beside their target off any screen, lines must not cross other screens. Below 60% zoom the canvas shows notes as badges.

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
| Find nodes by text or style | `find_nodes` (text contains the phrase, any case, every page) |
| Read all copy in a subtree | `get_tree_summary {nodeId, fullText: true}` (depth counts from the node: rows under a "Screens" frame need depth 3) |
| See it | `get_screenshot` (`nodeId`, `scale` 1 or 2, `annotations: true` to include notes) |
| Show the user something | `set_viewport` (page switch, fit, zoom) |
| Document rules on the design | `annotate`, `get_annotation_template`, `get_annotations`, `update_annotation`, `delete_annotations` |
| New screen | `create_artboard` (`width` and `height` required) |
| Add content | `write_html` (`insert-children` or `replace`) |
| Change styles or text | `update_styles`, `set_text_content`, `rename_nodes` |
| Whiteboard shapes, sticky notes, arrows | `create_shape` |
| Copy, move, delete | `duplicate_nodes`, `move_nodes`, `delete_nodes` |
| Tokens | `get_variables` (`format` css, json or tailwind), `set_variables` (`collection`, `type`, `mode`, `alias`), `create_collection`, `delete_variables`, `set_variable_mode`, `bind_variable` |
| Reuse | `create_component`, `create_instance`, `swap_instance` (also a nested badge inside a row instance: a swap that survives main edits), `get_components`, `<x-paper-instance>` |
| Component properties | One component with properties instead of one per state: `set_component_props` (`text`, `bool`, `icon`, `enum` options in the overrides format; templates `text`, `active` for tab bars and navs, `status` with [{value, color}]), then `set_instance_props {nodeIds, values}` or `<x-paper-instance component="Badge" props='{"Status":"Rejeitada"}'/>`; `get_components` lists them |
| Merge per-state components | `merge_variants {componentIds, name}` turns `Tab bar / Hoje`, `Tab bar / Lojas`... into one `Tab bar` with an enum, moves every instance and deletes the emptied variants |
| Screens | Fixed size (390x844, 1440x900...); content longer than the screen: `overflow: auto` on the screen frame, so it keeps its size and the overflow scrolls on the canvas and in Present (inspector: Scroll overflow; page: Screens scroll) |
| Project | `rename_file {name}` (open project stays open), `move_nodes {toPageId}` keeps instances linked and notes attached |
| Shared libraries | `publish_library`, `list_libraries`, `enable_library`, `update_library`, `disable_library` |
| Icons | `search_icons`, `<x-paper-icon name="set:name">`; CLI: `papercraft-cli icons add\|list\|search` |
| Design in a library's look, hand off its code | `apply_ui_kit` (shadcn, antd, reactbits, egui), then `<x-paper-instance>` of its components; `set_code_link` for a component of your own; `get_jsx` / `get_design_spec` then write `<Button variant="outline">` instead of divs |
| Avatars, more icons, photos, flags, QR | `<x-plugin use="avataaars\|dicebear\|iconify\|picsum\|flags\|qr" …/>` inside `write_html` (plugins the user turned on) |
| References and art | `fetch_reference_images`, `zoom_image`, `generate_image` (when the user turned image generation on) |
| Ask the user | `ask_user` (clickable answers; end your turn after it) |
| Hand work to the helper agent | `delegate` (`role` qa, read or build, a self-contained `task`; only when Settings, Chat, Helper is set) |
| Design rules (design-playbook) | `design_rules` (`query` or `rule` id; needs the `playbook` CLI; cite ids in the report) |
| Report a PaperCraft problem | `report_issue` (a draft the user sends; synthetic repro only) |
| Check quality | `lint_design` |
| Export | `export` (PNG, SVG, PDF to files), `export_combined_pdf` |
| Point at things for the user | `select_nodes` (`zoom: true`) |
| Take back | `undo`, `redo` |
| Save | `finish_working_on_nodes` |

## 6. Known limits (design around them)

* **Font is Inter only**, regular and bold plus italic. No other families, no weights between. Build hierarchy from size, weight, color and space.
* **Variables** are typed (`color`, `number`, `string`, `boolean`), live in collections with modes, group with `/` (`space/md`, `color/brand/primary`) and can alias each other. Colors work as `var(--name)` on fills and text; every other field binds with `bind_variable` (stroke, text-color, width, height, gap, padding, radius, font-size, line-height, opacity, visible). Numbers bind as px; line-height below 4 is a multiplier. Gradients keep fixed colors.
* **No margin, grid or tables; no shadows, filters or transforms** (reported under `ignoredStyles`). Fake elevation with a border or a lighter surface.
* **No rotation.** Nodes are axis-aligned.
* **Components work across pages**: `create_instance` takes a main from any page or enabled library (`componentId` or `componentKey` from `get_components`) and follows later edits. To share with other projects, `publish_library` there and `enable_library` here; `update_library` pulls a newer publish.
* **No variants or properties**: one component per variant (`Button / Primary`, `Button / Secondary`). `swap_instance` and `detach_instance` exist.
* **Remote images are not fetched.** Use a local path or `data:` URI.
* **One project open at a time**; moving layers between pages is not supported.
* **Text boxes with fixed widths can wrap earlier than in a browser.** Check headlines in the screenshot and widen.
* **Instances are skipped by lint**; lint the main component.
* **Icons inside an instance cannot be swapped**, only text can (`swap_instance` swaps to another main). Need the same button with a different glyph? Generate one component per icon in a loop (`Icon button / undo-2`, `Icon button / redo-2`...), not one generic button.
* **An instance keeps the size of its main.** Overriding text with a longer string clips it (a `Saved` chip made from a `Chip` main loses its right padding; a `Share` button from a wider/narrower main sits off-centre). After `set_text_content` on instance text, `update_styles {width:"fit-content"}` on the instance root and on its text layers, or pass `style="width:fit-content;justify-content:center"` on `<x-paper-instance>`.
* **Text inside nested frames**: `set_text_content` needs the Text node id. Walk `get_children` down from the instance; instances copy the main's structure (`Meta > Label`, not a flat list).
* **No icon-colour follow for stroke sets** (Lucide, Tabler): the colour is frozen at write time, so a mode switch (Mocha to Latte) leaves icons as they were. Use a fill set (Phosphor, Material) when a design must switch modes.
* **Pages**: `create_page`, `rename_pages`, `delete_pages` (needs `confirm: true` when the page has layers; the last page stays). `move_nodes` accepts `toPageId` to copy between pages.
* **`get_screenshot` of `root` on a wide page** is scaled to fit and unreadable. Screenshot one artboard (or one screen slot) at a time. Hoisted components are not inside an artboard screenshot.
* **CLI saves images to the cwd** as `<tool>.png`, or to `-o FILE`. The CLI writes the file itself; on older builds an `out` path made the app write it and could freeze the app behind a macOS privacy prompt, so update the CLI with the app (`tools/install-local.sh` installs both).
* **`swap_instance` renames the instance root to the new main's name** unless the name was overridden. Re-apply custom layer names after a batch swap (`rename_nodes`).

## 7. Troubleshooting

| Symptom | Cause and fix |
|---|---|
| `papercraft-cli` exits 1, "isn't running" | The app is closed, no project is open, or the port differs. Start it and open a project; check `PAPERCRAFT_MCP_PORT`. |
| Call fails with an unknown node id | The id is from another page or was deleted. Re-read with `get_children`; pass `pageId`. |
| `write_html` reports `ignoredStyles` | You used a CSS property that has no equivalent (`margin`, `box-shadow`, `transform`...). Replace it, see [html-subset.md](references/html-subset.md). |
| Content is cut off | The artboard height is fixed. `update_styles` with `height: "fit-content"` or a larger number. |
| A color did not change with the theme | It was written as hex, or it is a stroke or gradient. Use `var(--name)` on fills and text; strokes need `update_styles` after a mode switch. |
| `create_instance` fails | Wrong `componentId` / `componentKey` (read `get_components`). Mains on other pages and enabled libraries work; no import needed. |
| A page is full of copied components at y=0 | Something imported mains onto the page (`import_components`, or old `write_html` builds). Swap their instances to the design-system mains (`swap_instance {componentKey}`), check a screenshot, then delete the unused copies. |
| `papercraft-cli --online` says "did not answer in time" | The app is blocked: look for a macOS dialog or an in-app modal and ask the user to answer it. Don't retry in a loop and don't restart the app. |
| A tool name is unknown, or the docs list tools the CLI lacks | The CLI is stale. `papercraft-cli list` reads the app's live list; reinstall the CLI with the app. |
| Icon does not appear | The set is not installed: `papercraft-cli icons add phosphor`, then `papercraft-cli icons search <words>` for the exact `set:name`. |
| Layer panel is a pile of `Frame` | You skipped `layer-name`. Fix with `rename_nodes` (batch) before finishing. |
| Text looks tiny in the screenshot | Screenshots default to scale 1. Use `scale: 2`. |
| The user's view jumped | You switched pages. Always pass `pageId` instead of opening other pages. |
