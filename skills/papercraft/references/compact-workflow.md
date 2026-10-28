# Compact, synced, cheap

How to keep designs small, keep design and code in step, and spend few tokens doing both.

## Structure: pages, then groups, then artboards

* **One page per concern**, in this order: `Design System` (tokens + components), `Screens` (one artboard per screen), `States` (empty, loading, error, modals), `Flows` (a few connected screens, optional), `Archive`. A page over ~2000 nodes gets slow (save and screenshot time): split it.
* **Artboards are named `Area / Screen / State`** (`Editor / Inspector / Rectangle`). Slashes group the layers panel and make `find_nodes` queries cheap.
* **Group by intent, not by position**: one frame per section (`Header`, `Sidebar`, `List`, `Footer`) inside the artboard, so a section is one `write_html replace` and one `get_tree_summary` line.
* **Uniform sizes for states of one panel** (same width and height). Implementation then has no size variation and diffs stay readable.

## Compress the design

* Build every repeated thing once: `create_component`, then `create_instance` or `<x-paper-instance>`. Never paste markup twice. A screen is mostly instances plus layout frames.
* No raw colours, radii, spacing or shadows in `write_html`: `var(--token)` only. Changing a token then updates every page, and `get_design_spec` reports token names, which is what code needs.
* Reuse across pages with `create_instance {componentKey}` or `<x-paper-instance component="Name"/>` in `write_html`: a main on any page (or an enabled library, for `create_instance`) works directly and the instance stays linked to it. Do not `import_components` for that (it makes detached copies; it is for moving a design system). Do not rebuild a component per page.
* One design system per brand: `export_design_system` writes an OpenDesign package (variables, modes, tokens); `import_design_system` loads it into another project. That is the sync path between files and apps.
* Write one visual group per `write_html` (a header, a row), with `layer-name` on every box. Fewer, larger writes cost fewer round trips; groups that are too big make one bad style hard to find.

## Keep design and code in step

1. Design is the source of truth for structure, tokens and copy. Code is generated or verified from it, never the other way round without updating the design.
2. `get_design_spec {nodeId}` for each artboard: one deterministic JSON (layout, sizing, tokens, icons, `instanceOf`, `componentDefinitions`). Hand that to the implementer, not screenshots.
3. Implement from the spec. `verify_code {code, nodeId, language}` after each file; fix what it reports. Re-run `get_design_spec` only for artboards that changed (`diff_frames` between the old and new version of an artboard names the changed properties).
4. Name code pieces after components: `Button / Primary` becomes `button::primary` or `<ButtonPrimary>`. A 1:1 name map is the cheapest way to stay synced; keep it in a table in the feature spec (`specs/<feature>/design-map.md`).
5. Screenshots only for the final visual check (one per artboard, scale 1), and for diffing code output against the design (`get_screenshot` of both, compare).

## Spend few tokens

* **Look at structure before pixels**: `get_tree_summary` (indented text) and `get_node_info` cost a few hundred tokens; a 2x screenshot costs thousands. Screenshot after a group or two, not after every call.
* **Query, do not dump**: `find_nodes {textValue}` or style filters instead of reading a whole page; `get_children` on one frame; never `get_jsx` a whole page.
* **Batch**: `update_styles` with many `updates`, `set_text_content` with many nodes, `delete_nodes` and `move_nodes` with lists. One call, one undo step.
* **Script the repetitive part** with `scripts/pc.py` (`box/row/col/text/icon`, `Page.component`, `Page.inst`): a 10-line loop replaces 40 hand-written boxes and keeps output out of the conversation.
* **Lint once per artboard**, fix everything it names in one batch, lint again. Do not lint after each edit.
* **Brief subagents with the spec**, not the whole conversation: artboard name, `get_design_spec` output, token names, the file to write, the `verify_code` command to finish with.
* **Do not re-read** pages you just wrote; trust the returned ids and summaries.

## Sharing and several agents

* Several agents may work on one open project through one MCP server: calls run one at a time, each is one undo step, so they do not corrupt each other. Give each agent its own page or artboard prefix and say so in the brief, so they do not restyle each other's nodes. Each agent names itself (`papercraft-cli --as <name>`), claims what it rewrites (`claim_nodes`), reads the conflicts its writes report, and undoes only its own steps.
* Several people on a shared project: pages sync as files and are merged when both sides changed the same page (nodes by id, child lists as sets). Edits to different nodes both survive; the same property changed twice keeps the local value. Ask people to keep a page per person when restyling the same frame.
* While an agent works the app shows an animated "working" pill and a ring around the canvas; call `finish_working_on_nodes` when done so it stops.
