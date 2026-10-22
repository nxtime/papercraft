# Annotations: notes with leader lines that explain a design

## Why (user request, 2026-10-21)

Teams annotate designs in Figma with callouts and arrows that explain what each part does: business rules, behaviour, states, data sources, edge cases. They are toggled on for review and handoff and off for presenting. PaperCraft has comments (threads, open/resolved) but nothing that documents a design in place.

## Model

* An **annotation** belongs to a page: `{ id, target: NodeId (or a point), anchor: side or offset on the target, label: short title, body: markdown text, category, number, position: where the note sits on the canvas, author: user | agent, created }`. Stored in the page file under `annotations` (serde default; old pages load unchanged).
* **Categories** are project-level, editable: default `Business rule`, `Behaviour`, `Content`, `Data`, `Accessibility`, `Dev note`, each with a colour (theme palette) and icon.
* Annotations survive moves and resizes of the target (anchor is relative); deleting the target keeps the annotation detached with a warning badge.

## Classification drives the look (user, 2026-10-21)

* The **category is the only styling input**. Icon, colour, badge shape and default label prefix come from the category; an annotation stores no icon or colour of its own. Changing a category's colour or icon restyles every note in it.
* Categories carry a short **type key** used everywhere (`rule`, `behaviour`, `content`, `data`, `a11y`, `dev`); custom categories get a key too.
* **Hiding information**, three levels, all undo-free view state (not saved in the design):
  * per note: collapse to its number badge only (click the badge to expand);
  * per category: show/hide from the category filter;
  * global (⌘⇧A cycles; ⇧A is Add auto layout): full notes → badges only → hidden.
  * Exports and Present mode use "hidden" unless chosen otherwise.

## Templates (user, 2026-10-21)

People annotate differently per project and even per page (a product spec, a dev handoff, an accessibility review). A **template** says which kinds of notes are wanted and what each should contain.

* `AnnotationTemplate { id, name, categories: [ { key, enabled, prompt: short guidance for what this note should say, fields: [name] (optional structured lines, e.g. "Applies when", "Owner") } ], display: full | badges }`.
* Stored in `design.json` (project-level list); the **project default** is one template, and a **page can override** it (`page.annotation_template`, serde default). New categories added in a template become available project-wide.
* **Built-in templates** (editable copies, not locked): `Product spec` (rule, behaviour, content, data), `Dev handoff` (behaviour, data, dev, a11y), `Accessibility review` (a11y with fields "Issue", "WCAG", "Fix"), `Content review` (content with "Tone", "Max length"), `Minimal` (rule only).
* **UI**: the Annotations side view header has a template picker (project default or this page) and a "Customize…" modal: toggle categories, edit name/colour/icon/key, edit each category's guidance and fields, duplicate/reset a template. The Annotate popover only offers the template's enabled categories, and shows the fields as labelled lines.
* **Ask AI**: an "Annotate with AI" button in the side view (and a chat suggestion "Annotate this page") sends a prompt to the chat: annotate the selected frame or the page using the active template. The agent reads the template with one cheap call.
* MCP: `get_annotation_template {pageId?}` returns compact `[[key, guidance, fields]]` for the enabled categories (a few hundred tokens); `annotate` rejects types the page's template disables (error lists the allowed keys) and accepts `fields` as `"Name: value"` lines inside `text`. `set_annotation_template {pageId?, template}` switches it.

## Cheap for agents

* One tool call can add many notes: `annotate {notes: [{node, text, type}]}` where `node` is a node id or a layer name on the active page, `text` is the note (first line becomes the label, the rest the body), `type` is the category key (default `rule`). No positions, icons or colours: placement is automatic (beside the target, outside its frame, stacked without overlap).
* `get_annotations` returns compact rows `[n, node, type, text]`, not full objects; `{full: true}` for everything.
* `update_annotation` / `delete_annotations` take ids or numbers ("frame Sign in #3").
* The tool description stays under ~60 words.

## Canvas

* A note is a small card (number badge, label, first lines of the body) connected to its target by a **leader line**: elbow or straight, ending in a dot or arrow on the target's anchor, plus a thin outline around the target while hovered.
* Notes are draggable; the line re-routes (reuse the connector router, `src/connector.rs`).
* **Toggle**: View → Annotations (⌘⇧A) shows or hides all; a filter per category; never drawn in Present mode or exports (notes live only in the editor overlay; an export option to include them is a later item).
* Numbering is per frame, in reading order, so a frame reads "1, 2, 3…".

## UI

* **Annotate tool** (N; A is the Frame tool): click a layer, type the note; or select layers → right-click → Annotate.
* **Annotations side view**: list per frame with number, category chip, label; click to select and zoom; filter by category; edit inline.
* **Inspector**: an Annotations section for the selected layer.

## Agents and handoff

* MCP: `annotate` (batch, see "Cheap for agents"), `get_annotations`, `update_annotation`, `delete_annotations`; classified in `edits_doc`.
* `get_design_spec` and `get_jsx` include annotations as comments (`// 1 Business rule: …`), so code generated from a frame carries the rules.
* Chat: the `@ Selection` context includes the selected layers' annotations; the agent can be asked to "annotate this screen".

## Mock-ups

`PaperCraft` design file, page `Annotations`: `Canvas / Annotated frame` (several categories, leader lines, numbering), `Canvas / Annotations hidden`, `Panel / Annotations` (344x1440), `Inspector / Annotation` (344x1440), `Popover / New annotation`.

## Acceptance

* Add, edit, move, hide/show, filter, delete annotations; undo for each.
* Moving or resizing a target keeps the leader line attached; deleting it detaches the note with a badge.
* Old projects load unchanged; page files without `annotations` stay byte-identical on save.
* Exports and Present mode never include annotations.
* MCP tools round trip; `get_design_spec` includes them.
* Screenshot check against the mock-ups; tests for model, anchoring, numbering, serialization, MCP.
