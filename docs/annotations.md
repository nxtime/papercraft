# Annotations

An annotation is a note that explains a design in place: a business rule, a behaviour, where the data comes from, an accessibility gap. Each note is a small card with a number, a category, a label and an optional body, joined to its layer by a leader line. Teams use them for review and handoff, and turn them off for presenting.

Annotations live on a page and are saved with it (the `annotations` list in the page file). Pages without any load and save exactly as before. Categories and templates are project-wide and live in `design.json`.

## Categories

Every note has one category. The category is the only thing that sets how a note looks: its icon, colour and badge come from the category, and a note stores none of them. Change a category's colour or icon and every note in it changes.

Each category has a short type key. Agents and the Annotate popover use the key; the UI shows the name.

| Key | Name | Icon | Colour | Use for |
|---|---|---|---|---|
| `rule` | Business rule | scale | violet `#7c5cff` | the rule or constraint the part enforces |
| `behaviour` | Behaviour | zap | blue `#2f80ed` | states, transitions, edge cases |
| `content` | Content | type | green `#1fa87a` | copy, tone, length |
| `data` | Data | database | amber `#e08a1e` | where a value comes from |
| `a11y` | Accessibility | accessibility | pink `#d94f8a` | focus, labels, contrast, reading order |
| `dev` | Dev note | code | grey `#6b7280` | implementation notes for engineers |

You can add categories, rename them, change their key, colour and icon, and write guidance for each one in the Customize modal. A custom category gets a key too.

## Templates

A template says which kinds of notes a page wants, and what each note should contain. The project has a default template; a page can use its own instead.

Built-in templates are editable copies, not locked. Each category in a template has enabled or not, guidance (a short prompt for what the note should say) and optional fields (structured lines such as `Owner`).

| Template | Categories | Fields |
|---|---|---|
| Product spec | rule, behaviour, content, data | none |
| Dev handoff | behaviour, data, dev, a11y | none |
| Accessibility review | a11y | Issue, WCAG, Fix |
| Content review | content | Tone, Max length |
| Minimal | rule | none |

Pick the project default or "This page" from the template chips at the top of the Annotations side view. Built-ins can be reset; custom templates can be deleted. Duplicate copies any template so you can change it.

**Customize…** (side view) opens the template editor: name, display (full notes or badges), the enabled categories, the guidance and fields of each. Changes to categories apply to the whole project.

The Annotate popover offers only the enabled categories of the active template, and shows each field as a labelled line. A field is stored as a `Name: value` line at the top of the note's body. Agents get the same rule: `annotate` refuses a type the template disables and lists the allowed keys.

## View modes

Three levels, all view state. They are not saved in the design and not undoable.

* **Full**: every note shows its label and the first lines of its body. Below 60% zoom notes show as badges, so cards never spill over the design; zoom in to read them.
* **Badges**: only the number badges are shown.
* **Hidden**: nothing is drawn.

`⌘⇧A` (View → Annotations) cycles Full → Badges → Hidden → Full. The starting mode follows the template's display setting until you change it.

Two finer controls:

* **Per note**: collapse a note to its number badge; click the badge to expand it again.
* **Per category**: show or hide a category from the category filter in the side view.

Annotations are drawn as an overlay on the canvas only. Exports (PNG, SVG, PDF) and Present mode never include them. There is no option to include them in an export yet.

## Annotate tool

Press `N` for the Annotate tool, then click a layer. The New annotation popover opens beside it with:

* Category, from the template's enabled categories.
* Label, up to 40 characters. The popover shows the count.
* The template's fields, if any.
* A note, in Markdown.

Save creates the note with its leader line to the layer's side; Cancel discards it. You can also select layers, right-click, and choose **Annotate**.

Each note's leader line starts at the note card and ends at the anchor on the target. The card and the anchor are stored relative to the target, so moving or resizing the layer keeps the note attached. Drag a card to move it; the line re-routes.

If the target is deleted, the note stays where it was in the side view's **Detached** group, with a warning badge.

## Annotations side view

The side view lists the page's notes:

* Grouped by top-level frame, plus **Page** (notes on no frame) and **Detached**.
* Each row shows the number, the category chip and the label. Click a row to select the layer and zoom to it.
* The category filter shows or hides categories.
* Edit a note inline, or delete it. Edits are one undo step each.
* **Add annotation** opens the popover for the current selection.
* **Annotate with AI** puts a prompt in the Chat composer, unsent, with the selected frame (or the page) as a chip. The prompt asks the agent to read the template with `get_annotation_template` and add all the notes in one `annotate` call. It needs a saved project and Chat mode.

Numbers run per frame in reading order (top to bottom, then left to right), so a frame reads 1, 2, 3. Notes on no frame are numbered after that, from 1.

## Inspector

When a single layer is selected, the inspector has an Annotations section. It lists that layer's notes, with edit and delete, and an Add annotation button. The note's anchor is one of the four sides of the target (Top, Right, Bottom, Left) with a position along it.

## Agents

Agents work with notes through six MCP tools, all in `src/mcp/tools.rs`. They are one undo step per call and appear in `edits_doc`, so a run that only adds notes still gets a Review card (a "Notes" group, counted as added, changed or removed), and Revert removes them.

| Tool | What it does |
|---|---|
| `get_annotation_template` | Reads the template: `[type key, guidance, field names]` for each enabled type. Call it once before `annotate`. |
| `set_annotation_template` | Switches the template by id or name, for one page (default) or the project (`scope: "project"`). |
| `upsert_annotation_template` | Creates or replaces a template, and creates or restyles categories in the same call (`categories: [{key, name, color, icon}]`). `setAs: "project"` or `"page"` makes it the default or the page override. Returns `{id, name, types}`. |
| `delete_annotation_template` | Deletes a template by id or name. The last template stays. |
| `annotate` | Adds many notes at once. Each note names a node id or an exact layer name on the page, a `text` and a `type` (default `rule`). |
| `get_annotations` | Reads notes as `[number, nodeName, type, text]` rows (text capped at 120 characters). `full: true` returns complete objects; `nodeId` limits to a subtree; `pageId` picks another page. |
| `update_annotation` | Changes the text or type of one note, by `id` or by reference such as `Sign in #3`. |
| `delete_annotations` | Deletes notes by ids or references such as `Sign in #3`. |

Custom template with a new category, used on the current page:

```json
{
  "template": {"name": "Legal review", "categories": [
    {"key": "legal", "guidance": "Legal risk this part carries.", "fields": ["Owner", "Law"]},
    {"key": "rule"}]},
  "categories": [{"key": "legal", "name": "Legal", "color": "#aa3355", "icon": "scale"}],
  "setAs": "page"
}
```

`icon` must be one of the built-in icon names (an unknown name lists them). Every key in the template must exist, either already or in `categories`. The template and category edit is one undo step; `setAs: "page"` adds a second.

For `annotate` and `update_annotation`, the first line of `text` is the label and the rest is the body. There are no positions, icons or colours: PaperCraft places each note beside its target, outside the frame, stacked so none overlap. `annotate` is all or nothing: if one note is refused (an unknown layer, or a type the template disables), none is added and the error lists the allowed keys.

Batched call, on a page whose template is Product spec:

```json
{
  "notes": [
    {"node": "Sign in", "type": "rule", "text": "Lock after 5 failed tries\nLocks for 15 minutes, then resets."},
    {"node": "Email field", "type": "data", "text": "Source: users.email\nValidated on blur"},
    {"node": "Submit", "type": "behaviour", "text": "Disabled until both fields are valid\nSpinner while the request runs"}
  ]
}
```

The reply is one row per note, `[number, node, type, label]`. Then `get_annotations {}` reads them back, and `delete_annotations {"ids":["Sign in #3"]}` removes one by its frame reference.

`annotate {"tidy": true}` with no notes re-runs placement for every note on the page (or on `frame`) in one undo step and returns `{"tidied": n}`, the number of notes it moved; `0` means the layout was already tidy.

To see the notes, `get_screenshot {"nodeId": "<frame>", "annotations": true}` draws the cards, number badges and leader lines of the notes whose target is inside the captured layers, placed and routed as on the canvas (full cards, scale 1). The image grows to include the cards. Without `annotations`, or with `false`, the screenshot is the same as before.

**Handoff.** `get_design_spec` and the code exports (`get_jsx`) include the notes of the frame as comments. Each note is one line, `// 1 Business rule: Lock after 5 failed tries`, with its body lines as more `//` lines, so code generated from a frame carries the rules next to the markup.

## Storage

* The page file has an `annotations` list. Each note holds its target (or a point, when it has none), its anchor, label, body, category, position, author (`user` or `agent`) and creation time. A note whose target is gone is marked `detached`.
* `design.json` holds the categories and the templates, with the project default. A page may name its own template.
* Old projects load unchanged. A page without annotations saves byte-identical to before.
