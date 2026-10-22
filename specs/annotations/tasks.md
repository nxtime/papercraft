# Tasks

Stop on failure. UI tasks go to Sonnet, never Haiku. Agents clean up after themselves.

## Wave 1 (with chat-agents wave 1)

- [x] 1. **Mock-ups** on page `Annotations`. Sonnet.

## Wave 2 (parallel with chat-agents wave 2)

- [x] 2. **Model and storage**: `Annotation`, categories in `design.json`, page field (serde default, byte-identical save for old pages), anchoring relative to target, numbering per frame in reading order, undoable edits through `Editor`. Tests. Sonnet.
- [x] 3. **MCP tools** `add_annotation`, `get_annotations`, `update_annotation`, `delete_annotations`; `get_design_spec` / `get_jsx` include them as comments; classified in `edits_doc`. Codex Sol (Sonnet if unavailable).

- [x] 3b. **Templates**: model in `design.json` + page override, built-ins, `get_annotation_template` / `set_annotation_template`, `annotate` validates against the page's template. Sonnet (after 3).

## Wave 3 (after wave 2 and the mock-ups)

- [x] 4. **Canvas**: note cards, leader lines (connector router), drag, toggle ⇧A, category filter, hidden in Present and exports unless asked. Sonnet.
- [x] 5. **Annotate tool, side view (template picker, Customize modal, Annotate with AI), inspector section, popover**. Sonnet.

## Wave 4

- [x] 6. **Review** (Codex Sol, read-only) and fixes (Sonnet); chat `@ Selection` context includes annotations.
- [x] 7. **Docs** (Haiku): `docs/annotations.md`, README, `docs/agents.md`, ROADMAP, PROGRESS.
