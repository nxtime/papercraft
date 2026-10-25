# Progress

Last updated: 2026-10-24.

## Phase 3: local-first design and whiteboard tool

Goal: grow paper-rs into a Paper / Figma / Excalidraw-class tool. Figma and Paper are studied
clean-room (behavior only, `analysis/FIGMA_NOTES.md`, `analysis/PAPER_ARCHITECTURE.md`).
Excalidraw is MIT and studied from source (`analysis/EXCALIDRAW_NOTES.md`); ported algorithms
are credited in `THIRD_PARTY.md`.

| Milestone | Status | Evidence |
|---|---|---|
| Analysis notes | ✅ done | `analysis/FIGMA_NOTES.md`, `analysis/EXCALIDRAW_NOTES.md`, fixtures in `tests/fixtures/excalidraw/` |
| P1: local projects, home screen, autosave, atomic writes, versioned format | ✅ done | `src/project/`, `src/session.rs`, `src/io/atomic.rs`, `tests/projects.rs::m1_*`, `milestone1_home.png`, `milestone1_trash.png` |
| P2: multiple pages, infinite canvas per page | ✅ done | `src/editor/pages.rs`, `src/ui/pages.rs`, `tests/projects.rs::m2_*`, `milestone2_pages.png` |
| P3: Figma core | ✅ done | `src/layout.rs`, `src/editor/{structure,vector_ops,text_edit}.rs`, `src/render/{draw,tess}.rs`, `src/ui/inspector.rs`, `tests/figma.rs`, `tests/perf.rs`, `milestone3_figma_core.png`, `milestone3_canvas.png` |
| P4: Excalidraw core | ✅ done | `src/render/rough.rs`, `src/connector.rs`, `src/editor/whiteboard.rs`, `src/library.rs`, `src/ui/library.rs`, `tests/excalidraw.rs`, `milestone4_whiteboard.png`, `THIRD_PARTY.md` |
| P5: inspector, layers drag-reorder, command palette, shortcuts | ✅ done | `src/ui/{inspector,palette}.rs`, `src/ui/mod.rs` (layers drag, help), `src/editor/{commands,shortcuts}.rs`, `src/macos.rs`, `tests/workflow.rs`, `milestone5_palette.png`, `milestone5_shortcuts.png`, `milestone5_inspector.png` |
| P6: import/export (PNG/SVG/PDF, .excalidraw, SVG), cross-project paste | ✅ done | `src/io/{pdf,export,excalidraw,svg_import,clipboard}.rs`, inspector Export section, CLI `--export*`/`--import`, `tests/io.rs`, `milestone6_*.png` |
| P7: MCP server for AI agents (Paper-compatible tools, HTTP + stdio bridge) | ✅ done on Linux; ⏳ Mac run pending (steps below) | `analysis/paper-mcp.md`, `src/mcp/`, `src/bin/paper-rs-mcp.rs`, `tests/mcp.rs`, `milestone7_mcp.png`, `milestone7_mcp_live_xvfb.png` |

### P7 notes: MCP server

* **Study** (`analysis/paper-mcp.md`): Paper's desktop app serves MCP over streamable HTTP on
  `127.0.0.1:29979/mcp` (hono + MCP SDK), forwards calls into the renderer's editor, guards
  against browsers and DNS rebinding, and ships a Go `paper mcp` stdio relay. 29 tools; the
  write path is `write_html` (HTML + inline CSS → layers) and `update_styles` (CSS maps).
  Studied with REA (behaviour only; raw dumps in gitignored `analysis/raw/`).
* **Server** (`src/mcp/server.rs`, rmcp 3.5): streamable HTTP on `127.0.0.1:29980/mcp`
  (`PAPER_RS_MCP_PORT`, `PAPER_RS_MCP=0` to disable), on its own Tokio thread. Calls are sent
  to the winit loop (`UserEvent::Mcp`) and run on the UI thread via `mcp::exec`, each as one
  `Editor::edit` transaction (one undo step, live repaint, autosave). Edits to other pages go
  to that page's own document and history (`Editor::with_page`) without switching the view.
* **Bridge** (`paper-rs-mcp`): rmcp stdio server that lists tools offline and relays calls to
  the app over HTTP, reconnecting after app restarts.
* **Tools** (`src/mcp/tools.rs`): 53 tools at the time (61 now), using Paper's names and argument shapes where there is an
  equivalent (see README), plus `create_shape`, `select_nodes`, `undo`, `redo`.
  `write_html` (`src/mcp/{html,css}.rs`, `exec.rs` `Builder`) maps flex/block layout onto auto
  layout with CSS stretch semantics, text styles and rich-text runs, backgrounds, gradients,
  borders, images, inline SVG and clones.
* **Verified**: `tests/mcp.rs` (12 tests: every tool on a headless session, rmcp client over
  HTTP and through the bridge, browser/Origin rejection, a raw 2026-07-28 request through the
  bridge, screenshots). End to end on Linux: the real app under Xvfb, driven by **Claude Code**
  (`claude -p` with the server added over HTTP, then over the stdio bridge) drew a frame with
  a heading, paragraph, button and sticky note that appeared live in the window
  (`docs/screenshots/milestone7_mcp_live_xvfb.png`). That run found a bug the rmcp-only tests
  missed (bridge results lacked `resultType` for 2026-07-28 clients), fixed in `01e0836`.
  The macOS target type-checks (`cargo check --target aarch64-apple-darwin --all-targets`).
* **Mac verification steps** (not run here: no access to the Mac from this machine). In
  `/Users/Shared/projects/paper-rs` on the Mac:

  ```sh
  export CARGO_TARGET_DIR=~/.cache/paper-rs-target   # keep build output out of the synced tree
  cargo build --release
  $CARGO_TARGET_DIR/release/paper-rs &               # app; log says "MCP server listening on http://127.0.0.1:29980/mcp"
  curl -s http://127.0.0.1:29980/x                   # → {"status":"not_found",...The MCP endpoint is /mcp."}

  # HTTP client
  claude mcp add --transport http --scope user paper-rs http://127.0.0.1:29980/mcp
  claude -p "Use the paper-rs MCP tools: create_file 'Mac check', open_file it, create a 480x320 \
    artboard 'Hello', and write_html a heading 'Hello from the Mac' into it." \
    --allowedTools "mcp__paper-rs__*"
  # stdio client (through the bridge)
  claude mcp remove paper-rs --scope user
  claude mcp add --scope user paper-rs -- $CARGO_TARGET_DIR/release/paper-rs-mcp
  claude -p "In paper-rs, add a yellow sticky note saying 'stdio works' next to the Hello artboard." \
    --allowedTools "mcp__paper-rs__*"
  # Codex works the same way:
  #   codex mcp add paper-rs --url http://127.0.0.1:29980/mcp
  #   codex mcp add paper-rs -- $CARGO_TARGET_DIR/release/paper-rs-mcp
  ```

  Done when the "Mac check" project is open in the window with the "Hello" frame and its
  heading (and the sticky note) visible, and ⌘Z removes each agent edit in one step.
* **Not carried over**: comments, design tokens, AI image generation, video export, teams;
  moving layers between pages; remote image URLs (see README Known gaps).

### 2026-10-25: unreleased (component properties)

* Main components carry properties (`Node.props`): `text`, `bool`, `icon` and `enum`, whose options are changes in the overrides format (a layer name, `$root` or a path `Item > Label`, to a text or styles that may hold `text`, `icon` and `visibility`), so they restyle, retext, show, hide or re-icon any layer. Instances keep their values (`prop_values`) as overrides, so main edits keep them; changing the definitions re-applies every instance on every page.
* Templates: `text` (a property per text layer), `active` (which item of a tab bar, nav or segmented row is active) and `status` (options with a colour tint the root and colour the label and icon).
* `set_component_props`, `set_instance_props`, `props='{...}'` on `<x-paper-instance>`, properties in `get_components`, `{Prop}` in code link templates, and an inspector section (a control per property on an instance; Texts and Active item templates on a main). Icon swaps keep the layer's name and link (a `children` override); recoloured icon shapes inside instances now keep their colour after a sync.
* Spec: `specs/component-props/spec.md`.

### 2026-10-25: unreleased (scrolling screens)

* Screens keep their size and scroll what overflows: a frame's `scroll` flag (CSS `overflow: auto|scroll`, `overflow-y-auto` in the Tailwind export) makes it fixed height and clipped, and the wheel over it scrolls its content on the canvas and in Present, then pans once the content ends. The offset is view state, not saved; hit-testing and overlays follow it.
* Toggles: a scroll button beside each screen's name (shown when it scrolls, overflows, or is hovered), Scroll overflow on a selected frame, and Screens scroll on the page for every screen at once. A fit-content screen that grew goes back to its min height.
* Agents are told to keep screens at their size and use overflow:auto for long content.

### 2026-10-25: unreleased (fixes #19, #20, #21)

* #19: a clipping frame draws its border after its children, so filled rows no longer hide it. Clipping stays rectangular: children's square corners still show past a rounded border.
* #20: a border written with a colour variable (`1px solid var(--border)`) binds the stroke, a literal one unbinds it, and reads show `var(--name)` for bound strokes. `find_nodes` says when it stopped at 500 matches.
* #21: hug (fit-content) frames grow around growing children (CSS flex-basis auto) instead of staying at their min height; with short content the min height stays and the growing child fills it, so a pinned footer works.

### 2026-10-25: unreleased (follow agents, skeletons)

* Follow agents (Settings, Chat; on by default): in Design and Chat mode alike, the canvas pans, and switches page, to the layer any agent changes (the chat, Claude Code over MCP, a helper). Moving the canvas yourself pauses it for a moment; off, a chip offers the agent's page.
* Skeletons: the moment the model starts writing a tool call (`ToolDraft` and `ToolInput` events from Claude Code's stream and the API), the canvas shows a shimmering skeleton over its target, or where a new artboard goes, labelled with what it is doing, until the call has run. Fresh agent content comes in through a skeleton that fades, so an outline never frames empty space.
* Top bar: the title and the agent pill leave room measured from the buttons on the right, so nothing overlaps at narrow widths.

### 2026-10-25: unreleased (fixes #14 to #18)

* #14: `write_html` fits an `<svg>` viewBox into its width and height like a browser (centred, aspect kept).
* #15: a note on something placed straight in its artboard treats the artboard as the screen: tidy keeps cards off it instead of in its free space.
* #16: `not-an-instance` no longer matches look-alikes with a different icon; its fix names the `<x-paper-instance>` to write (swap_instance refuses plain frames).
* #17: instances with their own text re-measure it when the main's text style changes, so hug buttons keep fitting their labels (also covers the size reset after a swap).
* #18: `lint_design` reports borders whose hex equals a color variable, and `fix` binds them (`stroke`), so a mode switch reaches them.

### 2026-10-25: 0.7.3 (QA sub-agent, helper agents, auto mode, mid-turn messages, isolated dev settings)

* QA sub-agent: Claude Code chats always get a Sonnet `qa` agent that lints and screenshots built frames and returns only findings, so screenshots stay out of the main context.
* Helper agents (`src/mcp/delegate.rs`, Settings, Chat, Helper): `delegate` hands `qa`, `read` or `build` tasks to a cheaper agent CLI (pi with MiniMax M3.1 Flash, opencode, an Ollama-backed agent, any command taking `{prompt}`). It drives PaperCraft through `papercraft-cli`, runs off the UI thread, and only its last 6,000 characters come back. With a helper set, the chat agent orchestrates: keeps the brief and taste, delegates review, bulk reading and fully specified edits.
* Dev config isolation: `PAPERCRAFT_HOME` also isolates the chat settings (`$PAPERCRAFT_HOME/.config`), so a second copy for testing never touches the installed app's settings.
* Live check in an isolated copy: `delegate` ran the pi helper (MiniMax M3.1 Flash) as QA, a message added mid-turn went Waiting then Seen, and the agent drafted #13.
* Fixes #10, #11, #12, #13:
  * #10: `move_nodes` to another page keeps mains' component keys and relinks every instance to moved nodes; new `rename_file` renames a project.
  * #11: swapped nested instances persist as swap overrides (`InstanceLink.swap`), preserving component identity across outer component updates and copies.
  * #12: `move_nodes` to another page takes the moved nodes' annotations along; detached notes resolve as `Detached #n`.
  * #13: `get_computed_styles` reports a text colour bound to a variable as `var(--name)`.
* Auto mode (the composer's Auto chip): when a turn ends on `ask_user` questions, the first (recommended) option of each is sent and the chat goes on, until Stop or a turn hits its budget. The agent is told to put its recommendation first, default what only the user can know, and end each turn with next steps.
* The Present button uses a play icon drawn like the icons beside it. Composer chips wrap instead of running under Stop and Add. Top bar: the agent pill drops its text when it would run under the buttons on the right.
* While a chat turn works, the composer's Add (or Enter) queues a message (`src/mcp/steer.rs`). It rides on the agent's next PaperCraft tool result for that chat, so every backend reads it mid-turn; the composer shows each as Waiting or Seen. What no tool call picked up is sent as the next message when the turn ends, or goes back into the box after Stop.

### 2026-10-24: 0.7.2 (UI kits, code links, issues from the app, design playbook)

* Code links (Figma Code Connect style): a main component carries its library import and a React and an egui template (`set_code_link`). The React, Tailwind, inline JSX and egui exports write linked instances as that code, with overrides filled in and imports gathered once; the AI spec lists them as Library Components with a "use it, do not rebuild it" rule and stops describing their inner layers.
* UI kits (`src/ui_kits.rs`, `apply_ui_kit`): shadcn/ui, Ant Design, React Bits and egui, each with Light and Dark variables, tokens and components in its look and density, all linked. The chat brief asks for the code library and starts from its kit.
* Issues from the app (`src/issues.rs`): the agent's `report_issue` draft card (Send or Discard) and a Report a PaperCraft issue form, filed on the repository in Settings (token in the keychain, else the `tea` login), de-duplicated by title against open and closed issues.
* design-playbook in the chat: with the `playbook` CLI installed, the brief asks whether to follow the playbook and the agent looks rules up with `design_rules`, citing ids in its report.
* Top bar: project name and page picker on one centred row; the two-row title had outgrown the bar and covered the canvas.
* Evidence: `src/ui_kits.rs`, `src/issues.rs`, `src/mcp/codegen.rs` (`linked_components_export_as_library_code`), `specs/ui-kits/spec.md`.

### 2026-10-24: 0.7.1 (rename fix)

* Enter, Escape and clicking away end a rename again (projects on the home screen, library assets, versions). The fields took focus back before checking whether they had lost it.

### 2026-10-24: 0.7.0 (images, research, plugins, fewer tokens)

* Images in chat: paste from the clipboard (Cmd/Ctrl+V with an image-only clipboard now reaches the composer), thumbnails in the composer and in sent messages, a lightbox, image-only messages. Attachments are sent at most 1568 px (originals kept); `zoom_image` reads a region of the original.
* Brief and finish: `ask_user` draws questions as a card of clickable answers (palette swatches, reference image tiles, the recommended answer marked). The chat rules ask for a brief before the first design, variables and components first, sizes from the agreed density, a short report and next steps as a card.
* Web research (the composer's Web chip): `fetch_reference_images` (public addresses only, icons, logos and backdrops dropped, social cards last), a References gallery that dims what the agent did not keep; on Claude Code a Haiku `researcher` sub-agent does the searching.
* Image generation, opt-in (Settings, Chat, Images): Codex (offered once when signed in with a ChatGPT plan), Google Flow through `gf`, or any command (`{prompt}`, `{aspect}`, `{ratio}`, `{out}`, passed as data).
* Plugins (Settings, Plugins): Avataaars, DiceBear, Iconify, Lorem Picsum, Flags, QR codes, placed with `<x-plugin use="…">`, cached on disk. SVG import draws masked groups as images (Avataaars).
* Tokens: lean Claude Code sessions (42.7k to 20.6k prompt tokens), deferred tool schemas and context editing on the API, a fresh session with a recap past 120k (or on demand from the meter), per-turn cost for resumed Claude Code sessions, a 1M context meter, a turn budget. Model and effort pickers for Claude Code.
* Also: page switcher in the Chat top bar, steps stay folded while a turn runs (no flicker), `lint_design` asks 24 px targets on desktop screens, the installer keeps `papercraft-mcp` current.
* Evidence: `src/mcp/{refs,imagegen}.rs`, `src/plugins.rs`, `src/io/svg_import.rs`, `src/ui/{chat,chat_msgs,chat_agents,settings}.rs`, `src/chat/{anthropic,claude_code}.rs`, `tests/chat_tools.rs`, `specs/chat-tokens/spec.md`.

### 2026-10-22: 0.6.1 (agents via ACP, annotations)

* Agents are found through the login shell's PATH (read once at startup, 3 s limit, merged with the app PATH and the usual folders), so an app opened from the Dock finds Claude Code, Codex and the rest. No install instructions anywhere; a missing agent shows Not found with Set path… and Docs.
* ACP backend (`src/chat/acp.rs`): any agent that speaks the Agent Client Protocol over stdio, with PaperCraft's MCP server, streamed updates, permission flow, cancel and load. Registry as data in `src/chat/agents.rs`: native agents found on PATH, npx agents off until enabled, custom agents by name, command and arguments. Verified here: OpenCode works; Gemini CLI's server currently refuses ACP clients; Cursor needs `cursor-agent login`; Pi works through `pi-acp` after enabling.
* Agent panel over the chat column (Detected, Enable via npx, Not found, API endpoints, Custom, search), a Connect an agent setup card, a sign-in card, and multiple `@ Selection` chips.
* Settings → Chat redesigned: one agents list with details, endpoints, defaults, scrolling page on one label grid.
* Send bug: a turn that cannot start keeps the typed text and shows an error row with Retry, never a silent no-op (`a_failed_start_keeps_the_text_and_shows_an_error`).
* Annotations: notes with leader lines on a page, six categories with keys (style comes only from the category), project and per-page templates with built-ins and a Customize modal, view modes (⌘⇧A), Annotate tool (N), side view, inspector section, Annotate with AI, six MCP tools (`annotate` batches), and comments in `get_design_spec` and the code exports. Exports and Present leave annotations out.
* Evidence: `src/chat/{path,agents,acp}.rs`, `src/ui/{chat_agents,annotations,annotations_canvas}.rs`, `src/model/annotation.rs`, `src/editor/annotations.rs`, `src/mcp/tools.rs`, `tests/annotations*.rs`, `specs/chat-agents/`, `specs/annotations/`, `docs/chat.md`, `docs/annotations.md`.

### 2026-10-21: Chat mode

* Chat layout next to the canvas (⌘J, `Design | Chat`), with three backends: Claude Code and Codex through their CLIs (`claude -p` stream-json, `codex exec --json`) against the app's MCP server, and the Anthropic API with a key in the OS credential store. One event stream for all three; tool calls run through the same MCP dispatch, so runs, review and revert behave as before.
* Thread: markdown replies, thinking row, tool cards, step groups, plan from `TodoWrite`, approval for destructive tools on the API backend, run checkpoints with Restore to here, usage line and context meter, `@ Selection` and image attachments, Stop.
* Chats live in the project's `chats/` folder. Settings → Chat holds CLI paths, the default backend and the API key.
* Offline CLI fix: an offline save writes no camera, and a top-level shape created without a position goes to the next free spot instead of the origin.
* Evidence: `src/chat/`, `src/ui/chat.rs`, `src/ui/chat_msgs.rs`, `src/ui/chat_md.rs`, `src/ui/settings.rs`, `src/mcp/exec.rs`, `tests/fixtures/chat/`, `specs/chat/`, `docs/chat.md`.

### 2026-10-21: Agent runs and change review

* Agent editing calls group into runs, snapshot each edited page as `Before agent run N`, and end on 30 seconds idle, `finish_working_on_nodes`, or project switch. Runs persist under `runs/` and keep the newest 50.
* Agent view lists runs and supports review, whole-run revert, per-change undo and accept all. Review adds canvas outlines and a finish toast; palette commands review or revert the last run.
* Id-paired page diffs power review; History version compare UI remains todo. Completed with full restore (including variables, modes and tokens), tool edit classification, and wasm support.

### 2026-10-21: Design libraries and offline CLI

* Variables are now collections with modes, slash groups, four types and aliases, stored in `design.json` (project version 2; 0.4.0 refuses migrated projects). Each page keeps a derived cache; every edit is one undo step per page, and the page-only removals are gone. Per-page active mode per collection.
* Fields bind to variables (stroke, text color, size, gap, padding, radius, font size, line height, opacity, visible) in the inspector and with `bind_variable`; gradients do not bind yet.
* Instances work across pages (`component_key`, source page or library); deleting a main flags its instances.
* Libraries: publish a snapshot (`library.json`), enable, update with review (overrides kept), disable, and keep working when the source is missing. Variables modal, Libraries modal and Assets view in the UI; `create_collection`, `delete_variables`, `bind_variable` and five `*_library` tools in MCP (61 tools).
* `papercraft-cli call --offline` and `papercraft-cli shell` run the tools in-process on the project store, refuse a project the app has open, and fall back to the software adapter for screenshots. See `docs/agents.md`.
* Evidence: `src/variables.rs`, `src/editor/design.rs`, `src/libraries.rs`, `src/cli.rs`, `tests/{libraries,cross_page_components,variables_ui,assets_panel,cli_offline}.rs`, specs in `specs/design-libraries/` and `specs/cli-offline/`.

### P6 notes

* **Export** (`io/export.rs`): scope = selection, each frame, page or all pages; format =
  PNG (1–4x, transparent or white), SVG or PDF. Several PNG/SVG outputs go into a chosen
  folder (named after the frames/pages); PDF is always one file with one page per item.
  Inspector Export section, ⌘⇧E / ⌘⌥⇧E / ⌘⌥P, copy as PNG (⌘⇧C) / SVG, and the CLI.
* **PDF** (`io/pdf.rs`): hand-written PDF 1.7 writer over the shared draw ops — paths
  (quadratics elevated), solid fills, axial/radial shadings (stitched functions for 3+
  stops), images as Flate-compressed RGB XObjects with an alpha soft mask, strokes with
  caps/joins/dashes, ExtGState opacity and all 16 blend modes, frame clipping. Verified by
  structure tests and by rasterizing with pdftoppm.
* **Excalidraw import** (`io/excalidraw.rs`): scenes and clipboard JSON (rectangles,
  ellipses, diamonds, text, lines, arrows with bindings/elbows/heads, freedraw with
  pressures, images from `files`, frames, sticky notes with their text, groups → group
  layers, rotation baked into paths, hand-drawn style kept with the element's seed and fill
  style); `.excalidrawlib` v1/v2 into the library.
* **SVG import** (`io/svg_import.rs`, usvg): paths with transforms, fills (solid, linear,
  radial), strokes, opacity, embedded raster images, nested SVG and text outlined with Inter.
* **Clipboard** (`io/clipboard.rs`): copy writes a `paper-rs/clipboard` JSON payload with
  image bytes to the system clipboard (arboard), so layers paste between pages, projects,
  windows and restarts; paste also takes Excalidraw JSON, SVG markup, images and plain text.
  Our own last copy pastes from the internal clipboard (offset on the same page).
* Drag-and-drop of `.svg`, `.excalidraw`, `.excalidrawlib` and images onto the canvas, and
  ⌘O imports into the current page.

### P5 notes

* **Inspector**: grouped macOS-style sections (header actions, alignment icons, position &
  size with per-corner radii, constraints / sizing, auto layout, text, fills with gradients
  and images, stroke, layer blend/opacity, hand-drawn, arrow, sticky). Number fields scrub
  and coalesce into one undo step; edits inside instances become overrides.
* **Layers tree**: drag rows to reorder — above / below a row, or into a frame or group (the
  middle of the row); the canvas position is kept and the move is one undo step.
* **Command palette** (⌘K, also ⌘⇧/): fuzzy search over every bound action, all tools, pages,
  library items and layers (select + zoom). Arrow keys, Enter, Esc.
* **Shortcuts**: one binding table drives dispatch, the grouped help panel (⌘/ or ?), the egui
  menu bar and the native macOS menu (⌘ chords become key equivalents; single keys are
  shown in titles). Added Tab/⇧Tab sibling selection, ⇧Enter parent, ⇧H/⇧V flip,
  ⌘⌥C/⌘⌥V copy/paste properties, ⌥-drag duplicate (one undo step), PageUp/PageDown pages,
  ⌘R rename, ⌘\ hide UI, ⇧0 zoom 100%, Excalidraw number keys. 100+ bindings, checked for
  conflicts by a test.
* Deferred: Figma's number-key opacity (the digits pick tools, as in Excalidraw), drag of
  multiple layers at once in the tree, customizable shortcuts.

### P4 notes

* **rough.js 4.6.4 port** (`render/rough.rs`): RNG, lines, curves, ellipses, SVG paths, solid,
  hachure, cross-hatch, zigzag and dots fills, plus Excalidraw's element → options mapping
  (`adjustRoughness`, dash patterns, fill weight, hachure gap). The test compares ten
  generator calls against real rough.js output (`tests/fixtures/rough/`, produced by
  running the MIT library under Node) to 1e-6, so the same seed draws the same wobble.
  Per-node toggle (⌘⌥H or the inspector), sloppiness 0/1/2, fill style, re-roll seed; with
  nothing selected the toggle sets the default for new shapes.
* **Arrows** (`connector.rs`): a `Connector` node with start/end bindings (`node`, normalized
  fixed point, inside/orbit). Orbit ends stop at the target outline + `5 + stroke/2`, aimed
  at the other end's focus, and re-route in the fixup pass whenever a bound layer (or an
  ancestor) moves, resizes or is deleted. Straight, curved (Catmull-Rom) and elbow routes;
  elbow uses a grid + A* with Excalidraw's bend penalty and segment-count heuristic.
  Heads: arrow, triangle, dot, bar, diamond. Endpoint handles rebind; the midpoint handle
  bends. Labels on shapes defer to the shape when binding.
* **Quick diagram shapes**: diamond tool (D), polygon/star, and quick-connect "+" knobs on a
  selected shape that duplicate it 80 px away and connect the two with a bound arrow.
* **Sticky notes** (S): colored note with padding 16, text wraps and the font shrinks
  (to 12) to fit, six colors.
* **Laser pointer** (K): a trail that fades over 1 s and shows the last 50 points, eased like
  Excalidraw; the event loop animates only while points are visible, then idles.
* **Library**: a shared `library.json` in the projects folder (starter flowchart items on
  first use), a Library tab with tessellated previews, add-selection, rename, remove, insert
  (fresh ids, arrow bindings and instance links remapped).
* Number keys 1–0 select tools like Excalidraw (1 select … 0 eraser).
* Deferred depth: Excalidraw's dynamic-AABB "side hack" in elbow routing, arrow labels and
  text bound inside shapes, multi-point arrows by clicking, freedraw with perfect-freehand
  (paper-rs keeps its own ink), rough rendering of text/ink, collaboration.

### P3 notes

* **Renderer rework.** Nodes become backend-neutral draw ops (fills with solid / linear /
  radial / image paints, strokes, glyph runs) in `render/draw.rs`. The GPU path tessellates
  them in world space and caches per node (keyed by content, world rect, inherited opacity and
  zoom octave). Unchanged frames reuse the item list; the world vertex buffer is only
  re-uploaded when the item list changes, so panning costs one uniform write. Gradients use a
  ramp atlas; images are mipmapped textures; multiply/screen/darken/lighten have their own
  blend pipelines (premultiplied output). The SVG exporter consumes the same ops.
* **Model v3**: fill layers, blend, stroke align/dash/caps/joins, per-corner radii, rich text
  spans, auto layout, layout-child sizing and constraints, component flag and instance links,
  groups, vector paths, polygons, stars. Page format v3 with a v2→v3 migration (`fill` →
  `fills`).
* **Fixup pass** (`Editor::fixup`) runs inside every transaction: normalizes group bounds,
  syncs instances of edited components (respecting per-property overrides recorded by the
  inspector, text edits and drags), and re-runs auto layout from the outermost auto-layout
  ancestor. It undoes together with the edit.
* **Resize** restores the drag-start snapshot each move and re-applies constraints/group
  scaling from it, so repeated resizes never drift.
* **10k elements**: `tests/perf.rs` builds 10 000 nodes (rects, ellipses, stars, text), all in
  view. Release: first frame ~140 ms (CPU tessellation of everything), cached frame ~3 ms,
  editing one node re-tessellates one node, panning re-uploads nothing.
* Deferred depth: rounded-corner frame clipping, live (non-destructive) booleans, on-canvas
  gradient handles, rotation, variants, layout grids, min/max sizes, baseline alignment, and
  exact GPU compositing for the remaining 11 blend modes and group opacity.

### P2 notes

* Each page is its own infinite canvas with its own undo history, selection and camera. The
  active page lives in the editor; the others are parked in their slot (`PageStash`).
* Pages panel at the top of the left sidebar: click to switch, + to add, double-click to rename,
  right-click for Rename / Duplicate / Delete, drag to reorder. The last page can't be deleted.
* Paste on another page (or in another project) keeps the original position; paste on the same
  page offsets by 16 px.
* Deleting a page isn't undoable (the page file is removed at the next save).

### P1 notes

* Projects live in `~/Documents/paper-rs/<name>/` (`PAPER_RS_HOME` overrides): `project.json`
  (manifest: id, name, created/modified, ordered pages, active page), `pages/<id>.json`, and
  `thumbnail.png`. Deleting moves the folder to `.trash/` with a `trashinfo.json`, restorable from
  the Trash view.
* Every write is temp file + fsync + rename + directory fsync (`io::atomic`). Stale temp files
  from a crash are removed when the project opens.
* Formats carry `format` + `version`; loading runs a chain of `Value -> Value` migrations.
  paper-rs 0.1 `.paper-rs` files are page format v1 and migrate to v2 (tested).
* Autosave: changes are written 1 s after the first unsaved edit (bounded latency even while
  editing, never during a drag), on page switch, when leaving the project and on quit. The event
  loop sleeps until the autosave deadline, so idle CPU stays at zero. Thumbnails refresh at most
  every 20 s and when leaving the project.
* Known gap: typing inside an open text edit is saved when the edit ends (it is one undo step).

### Phase 3 verification log (2026-10-11)

* `cargo build --release`: no warnings. `cargo clippy --all-targets`: clean. `cargo fmt`.
* `cargo test`: 99 unit tests + 42 integration tests (projects, figma, excalidraw, workflow,
  io, perf, headless), all passing; GPU tests on Mesa lavapipe.
* `cargo check --target aarch64-apple-darwin` and `--target x86_64-pc-windows-gnu` pass
  (the macOS menu is generated from the shared menu table and type-checks).
* Real window under Xvfb: idle CPU measured from `/proc/<pid>/stat` over 5 s after startup —
  0 ticks on the home screen and 0 ticks in the editor. Autosave, tooltips and the laser
  trail wake the loop only while they need to.
* 10k elements (`tests/perf.rs`, release): cached frame ~3 ms, first frame at a new zoom
  octave ~140 ms, one edited node re-tessellates one node, panning uploads nothing.
* Not yet run on Mac hardware (no Mac available to this session); see README Known gaps.

## Phase 2 (paper-rs 0.1) record

| Phase / milestone | Status | Evidence |
|---|---|---|
| Phase 1: reverse engineering | ✅ done | `analysis/PAPER_ARCHITECTURE.md`. REA scoped run is summarized in the appendix; raw output in `analysis/raw/` (gitignored). |
| Phase 2: design | ✅ done | `analysis/DESIGN.md` |
| M1: window + infinite canvas, smooth pan/zoom | ✅ done | `tests/headless.rs::m1_*`, editor scenario tests, `m1_*.png`, `x11_window_xvfb.png` |
| M2: freehand sketch with smoothing | ✅ done | `ink.rs` tests, `m2_freehand_sketch_with_smoothing`, `m2_sketch.png` |
| M3: shapes, frames, text | ✅ done | scenario tests, `m3_shapes_frames_and_text`, `m3_*.png` |
| M4: select/move/resize, multi-select, snapping | ✅ done | scenario tests, `m4_select_move_resize_snap`, `m4_*.png` |
| M5: layers panel, undo/redo | ✅ done | `history.rs` tests, `undo_redo_round_trips_a_whole_session`, `m5_layers_and_undo_redo`, `m5_*.png` |
| M6: save/load `.paper-rs`, PNG/SVG export | ✅ done | `io/*` tests, `m6_save_load_and_export`, `m6_svg_export_matches_gpu_render`, `m6_*` |
| M7: keyboard shortcuts | ✅ done | `editor/shortcuts.rs` (conflict test), README table, in-app `?` help |
| README (build/run, parity table, gaps) | ✅ done | `README.md` |

## Verification log

* `cargo build --release` passes with no warnings. `cargo clippy --all-targets` is clean.
* `cargo test` runs 51 unit/scenario tests and 8 headless GPU tests, all passing. The GPU tests
  ran on Mesa lavapipe (llvmpipe, Vulkan) on the headless VPS.
* The real windowed app was launched under Xvfb (X11) with the release binary and screenshotted.
  It needed `libxkbcommon-x11` and `libxcb-xkb` placed in `~/.local/lib`, since the VPS has no
  root access.
* `cargo check --release --target aarch64-apple-darwin` and `--target x86_64-pc-windows-gnu`
  both pass.
* The SVG export was rasterized with resvg and compared with the GPU PNG export: 0.004% of
  pixels differ.

## Notes

* Commit history: the core model, the editor and the app/renderer/I/O layer landed as three
  feature commits. Each one builds in release mode and has its own tests. Milestones 1–7 are
  spread across them because the layers depend on each other: the app needs I/O, and the
  overlay needs the editor.
* REA's full-asar analysis stalled on `node_modules/typescript/lib/_tsc.js` after 20+ minutes
  at 2.6 GB RSS. A scoped run on Paper's own sources finished in 65 s.

## Next steps (not started)

1. Image nodes (textured quads) and drag-and-drop of image files, to match `paper-asset:`.
2. Per-node mesh cache keyed by revision and zoom bucket.
3. Rotation, plus a bezier pen tool.
4. Multiple pages, and system clipboard interop (SVG/PNG/text).
5. Run and tune on macOS hardware, especially trackpad scroll feel and tablet pressure.
