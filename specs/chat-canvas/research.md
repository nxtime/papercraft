# Chat ↔ canvas: research

How canvas + AI products let a user act through chat and point at the canvas, and what PaperCraft already has. Researched 2026-10-23. Secondary sources (forums, reviews) are marked; treat their details as approximate.

## 1. What PaperCraft does today (code)

* **One path to the canvas: tool calls.** Every backend (Claude Code, Codex, ACP, API endpoints) edits only through the 69 MCP tools. CLI/ACP agents reach them over HTTP MCP (`src/mcp/server.rs`); API endpoints call `crate::mcp::exec` on the UI thread (`src/chat/tools.rs`, `ToolDispatch`). The user can't trigger anything in-process from chat except sending text.
* **Context the agent gets:** `skills/papercraft/SKILL.md` (14 KB) plus the project name and folder (`ChatState::system_prompt`, `src/ui/chat.rs:602`). There is no design-system digest, no viewport and no screenshot of what the user sees.
* **Pointing:** the `@ Selection` chip (⌘L), `add_selection` in `src/ui/chat.rs`. It sends `[Selection] <label> (<node ids>)` plus the annotations on those layers (`selection_notes`). It has no image, no region and no point. Annotations' "Annotate with AI" (`src/ui/annotations.rs:557`) is the one canvas-to-chat hand-off: it drafts a prompt, adds a chip and switches to Chat mode, but does not send.
* **Showing agent work:** the activity pill (`agent_pill`, `AGENT_PILL_FRAME_MS`, `agent_verb` in `src/ui/mod.rs:310-360`) shows a verb only, with no location. Canvas follow (`follow`/`pan_to`, `src/ui/chat.rs:1056`) pans to the node in the call args, on the active page only. The review overlay (`review_overlay`, `src/ui/mod.rs:1929`) draws added/changed/removed outlines, but only after the run ends and is opened for review (docs/chat.md, Limits).
* **Runs:** `src/runs.rs` has one global `Tracker`. A run is closed by a 30 s idle, `finish_working_on_nodes` or the end of a chat turn. Each run has auto-versions per page, plus `revert`, `revert_all` (checkpoint "Restore to here") and `revert_change` (one change). The chat has a Checkpoint row and a Run card (Review/Revert/Accept). MCP calls carry no client identity (`call_tool` ignores `RequestContext`), so parallel subagents land in one run (gap 7).
* **Screenshots:** `get_screenshot` renders nodes with `io::png::render`. Annotations are an egui overlay (`src/ui/annotations_canvas.rs`), so they never appear (gap 1). `src/model/annotation_layout.rs` (new) already gives pure card sizes, placement and leader routing in world units.
* **Camera tools:** `select_nodes {zoom}` works only on the active page and fits with a 1.0 zoom cap (`src/mcp/exec.rs:5266`). There is no page switch and no viewport read.
* **Design-system data that already exists:** `get_variables` (vars, tokens, modes; css/json/tailwind), `get_components` (page mains with use counts plus `ed.library_docs` mains), `get_annotation_template`, `lint_design {fix}` with `lint::autofix` (rules: text-size, type-scale, spacing-grid, touch-target, layer-name, hard-coded-color), `get_design_spec`, `diff_frames`, `swap_instance`, `bind_variable`. There are no component variants in the model.
* **Rendering stack:** custom `egui_winit` + `egui_wgpu` (`src/app.rs`, `src/render/frame.rs`), not eframe, so `ViewportCommand::Screenshot` is not handled. `render/gpu.rs` already has `start_readback` (texture to buffer).

## 2. Products

### tldraw: agent starter kit, Make Real, fairies
* Agent context: the user message, the **selection, the viewport**, a screenshot of the agent's view, a simplified shape list in view, **off-screen shapes summarised as clusters (bounds + count)**, recent user actions, and lint warnings. The user can add shapes, **a point or an area** as extra context. ([starter kit](https://tldraw.dev/starter-kits/agent), [AI docs](https://tldraw.dev/docs/ai))
* The agent **moves its own viewport** to look at parts of the canvas. Actions stream in and apply one at a time. Each action shows in chat with an icon and a one-line description. **Modes** (working, critique) limit the tool set per task. ([v4.4 release](https://tldraw.dev/releases/v4.4.0))
* Make Real: select shapes, click one button, and the selection's screenshot goes to the model. You iterate by drawing on top of the result. ([talk summary](https://gitnation.com/contents/make-real-tldraws-accidental-ai-play))
* Fairies (Dec 2025 experiment): agents shown as small characters on the canvas. **Where an agent is shows what it is working on**, and several run concurrently on shared space. ([talk](https://gitnation.com/contents/agents-on-the-canvas-with-tldraw), [tldraw offline](https://tldraw.dev/blog/tldraw-offline))
* Works: viewport and region as first-class context, a cheap off-screen summary, visible presence.

### Figma: AI, Make, MCP
* Quick actions (⌘K): Rename layers (only default-named layers), Make prototype, Make designs. These are **one-click scoped actions on a selection**, with no chat. ([quick actions](https://help.figma.com/hc/en-us/articles/23570416033943-Use-quick-actions), [Figma AI](https://www.figma.com/blog/introducing-figma-ai/))
* First Draft can't use your own design system; it uses Figma-built libraries. ([help](https://help.figma.com/hc/en-us/articles/23955143044247))
* **Make kits**: components and styles bundled with **usage guidelines**, so the model learns *how* to use the system, not just what is in it. Attachments (PDF, CSV/JSON, SVG) ground prompts in real data. ([blog](https://www.figma.com/blog/introducing-make-kits-and-make-attachments/))
* Best practices for AI to read a DS: semantic slash names, larger reusable blocks (cards, headers) over atoms, auto layout with sizing intent, full variant and property sets, variables for every color, space, radius and type, and a usage-examples page. ([help](https://help.figma.com/hc/en-us/articles/38978644498199-Best-practices-to-help-Figma-AI-understand-your-design-system))
* Failure mode reported by users: Make "only recognises a random subset of components" and "created new ones that looked like my components". (forum, 2025: [1](https://forum.figma.com/suggest-a-feature-11/figma-make-not-using-my-library-components-42235/index1.html), [2](https://forum.figma.com/share-your-feedback-26/figma-make-library-support-42423)) The lesson: putting the library "in context" is not enough. Reuse must be checked.
* Dev Mode MCP: `get_variable_defs` returns the variables used **in the selection**, Code Connect maps nodes to code components, and `create_design_system_rules` writes a rules file for agents. Selection-based prompting works only with the desktop server. ([guide](https://github.com/figma/mcp-server-guide))
* Works: selection-scoped context, DS rules as a compact digest, one-click actions.

### Cursor Design Mode (browser)
* Click an element to target it. **Shift+drag selects an area.** You can draw circles or boxes over a **frozen frame** of the viewport, and voice is accepted. ⌘L adds to chat, ⌥+click adds without the comment step. The agent gets element identity (xpath, component, computed styles) **plus a screenshot**. You can queue several edits while agents run. ([docs](https://cursor.com/docs/agent/design-mode))
* Agent checkpoints: a timeline in the chat, preview at a point, restore. Per-file Keep/Undo appears in the review panel. Reject bugs that deleted whole files show the risk of coarse undo. ([forum](https://forum.cursor.com/t/per-change-keep-undo-buttons-missing-after-agent-edits-only-undo-all-available/158983), [forum](https://forum.cursor.com/t/cursor-deletes-entire-file-when-changes-are-rejected/151760))
* Pain points (forum): a new pick overwrote the previous one, and the chip always lands at the end of the prompt. ([1](https://forum.cursor.com/t/new-visual-editor-prevents-adding-multiple-elements-in-chat-box/145962), [2](https://forum.cursor.com/t/insert-browser-tab-element-selection-at-cursor-in-composer-not-always-at-end/167268))
* Works: identity + pixels together, area selection, multiple chips.

### v0 design mode / Lovable visual edits
* v0: hover highlights and click selects. The **selected element shows as a badge in the prompt**, and the prompt is scoped to it. Direct panel edits are free; prompts cost tokens. A later version added annotations on elements **sent as one batched prompt**. ([v0 docs](https://v0.app/docs/design-mode), [changelog summary](https://www.createwith.com/tool/v0/updates/v0-launches-design-mode-with-visual-editing-and-layer-management))
* Lovable: select one or more elements (⌘-click for multi), then describe the change. The selection is the context. ([docs](https://docs.lovable.dev/features/preview-toolbar))
* Works: two lanes, deterministic edits for free and prompts for intent. Comments become a batch prompt.

### Google Stitch
* **Annotate**: draw a box around an element, type in a bubble, and only that element changes. You can also circle or write notes on the screen. **Variations**: select a screen and generate N (default 3) side by side, with a creativity range. "Create variant" keeps the original. ([review](https://designproject.io/blog/google-stitch-review/), [guide](https://tutorialsdojo.com/what-google-stitch-actually-does-and-where-it-falls-short/), secondary)
* Works: on-canvas prompt bubble, variations as siblings on the canvas, never overwriting.

### Canva Magic Edit
* Brush the area, then prompt one change per pass. Canva advises against giving size in words; the brushed area carries it. ([help](https://www.canva.com/help/using-magic-edit/))
* Works: the region carries the spatial intent, so the prompt stays short.

### Miro Sidekicks, Framer, Excalidraw, Penpot
* Miro: select clusters, then ask a task-specialised "Sidekick" (persona/mode) to transform them, for example into sticky notes. It has an MCP server for outside agents. ([TrustRadius](https://tr-web.dev.trustradius.com/products/miro/details), [MCP](https://miro.com/pl/ai/mcp/), secondary)
* Framer Wireframer: prompt a layout, then keep iterating in chat or jump to the canvas. Workshop builds components that match the site's style. ([update](https://www.framer.com/updates/wireframer))
* Excalidraw: AI is in the paid tier (text-to-diagram, wireframe-to-code). Community MCP servers give agents a live canvas. ([listing](https://www.stork.ai/de/excalidraw), [mcp_excalidraw](https://docsearch.algolia.com/mcp/docs/repo/yctimlin/mcp_excalidraw))
* Penpot: experimenting with MCP for agents that read and edit design data. ([summary](https://mcpmarket.com/es/news/9fad1b76-0f9d-4411-b557-348e8f85888c), secondary)

## 3. Patterns, ranked by value for PaperCraft

| Pattern | Seen in | PaperCraft today | Gap |
|---|---|---|---|
| Selection as scoped context, with pixels | Cursor, v0, Lovable, Figma MCP | `@ Selection` ids + notes | no image, no region |
| Area/lasso or point as context | Cursor, tldraw, Stitch, Canva | none | missing |
| One-click scoped actions on selection | Figma ⌘K, Lovable panel | Annotate menu only | add Optimize, Bind tokens, Rename, Fix lint |
| Deterministic lane vs model lane | v0, Lovable | `lint_design {fix}` exists, not in UI | expose without the LLM |
| DS digest in context + reuse check | Figma Make kits, `create_design_system_rules` | only the skill text | add a digest and a reuse lint |
| Agent viewport + presence | tldraw agent, fairies | pan follow + a verb pill | show where; let the agent read and set the view |
| Live change outlines during a run | tldraw (streamed), Cursor inline diff | after the run only | draw live |
| Per-run timeline + revert one | Cursor checkpoints | yes (runs.rs) | per-client runs; fix stale card |
| Variations side by side | Stitch | none | duplicate + keep one |
| Comments batched into one prompt | v0 annotations | feedback + annotations exist | "Send open notes to chat" |
| Voice | Cursor, Stitch | none | P2 |

## 4. Conclusions

1. The best tools send **identity + pixels + region**. PaperCraft sends identity only. Adding a screenshot crop to chips and an area pick is the biggest lever for "point at the canvas".
2. Split actions into **two lanes**. Deterministic actions (lint autofix, bind hard-coded colors to the nearest variable, swap copies for instances, rename default layers) run in-process through `runs::Tracker::call`, so they land as a reviewable run with no model. Intent actions (Optimize layout, Variations, Make consistent) send a scoped prompt from a template.
3. A DS in context is necessary but not enough (the Figma Make complaints). Pair the digest with a **reuse check**: lint "looks like component X but isn't an instance".
4. Presence beats a pill. Outline the nodes the current run touched, live, and say where the agent is (page, frame).
5. Ghost previews in egui are costly (a second document render path). The cheaper equivalent already exists: apply live, show the run outlines, Accept/Revert per run or per change. Variations give "preview" by construction: siblings on the canvas, keep one.
