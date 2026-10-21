# Tasks

Each task is independently testable. Stop on failure. Tasks in the same wave touch different files and can run in parallel; a wave starts when the previous one passes `cargo test --lib`. After each code wave a read-only Codex `gpt-6.1-sol` review; real findings get fixed before the next wave.

## Wave 0: design (parallel with wave 1)

- [x] 0. **Mock-ups** in the `PaperCraft` design file, new page `Design libraries`: `Modal / Variables`, `Modal / Libraries`, `Modal / Library updates`, `Panel / Assets` (344x1440), `Inspector / Bound fields` (344x1440). Reuse the `Design System` page variables and components. Only through the user's running app after `get_basic_info` confirms the open file is `PaperCraft`; never `open_file` in the user's app.

## Wave 1: model (parallel)

- [x] 1. **Design-system model** (new `src/variables.rs`). Types from the spec (`DesignSystem`, `Collection`, `Mode`, `Variable`, `Kind`, `Value`), ids, CSS name, group helpers (list groups, rename group), alias resolution per mode with cycle check, duplicate CSS name check, `cache_for(page modes) -> (variables, modes, mode, tokens)` producing today's `Document` fields. Tests: groups, rename group, alias chain, cycle refused, duplicate refused, cache matches today's shape.
- [x] 2. **Storage and migration** (`src/project/format.rs`, `src/project/mod.rs`, `src/session.rs`). `design.json` load/save next to `project.json`; project version 2 with a `PROJECT_STEPS` step building collections `Colors` and `Tokens` from the first page; `PageFile.modes` per collection; `component_key` for existing mains. Tests: v1 project migrates, round trip, a v2 project's page files still deserialize in the old reader shape.
- [x] 3. **Bindings field** (`src/model/node.rs`, `src/model/paint.rs`). `Node.bindings`, `InstanceLink.source`, `Node.component_key`, all `serde(default)`. Tests: old pages load; serialization skips empty fields.

## Wave 2: wiring (after 1–3; 4 then 5 and 6 in parallel)

- [x] 4. **One write path** (`src/history.rs`, `src/editor/*`). Design-system edits update the project `DesignSystem`, rebuild every page's cache and re-apply fill and field bindings, one undo step per page; replace `on_every_page` and the page-only removals in `src/ui/confirm.rs`. Per-page mode switch. Tests: number binding change on two pages; mode switch only touches its page; undo per page.
- [x] 5. **Field bindings applied** (`src/editor/structure.rs`, layout). Apply bound stroke, text colour, size, gap, padding, radius, font size, line height, opacity, visible; instance sync carries bindings; detach writes resolved values. Tests per field.
- [x] 6. **MCP and exports** (`src/mcp/exec.rs`, `src/mcp/tools.rs`, `src/mcp/codegen.rs`, `src/design_system.rs`). Extended `get_variables` / `set_variables`, `create_collection`, `delete_variables`, `bind_variable`; DTCG nests groups; old calls still work. Tests: old and new call shapes, DTCG groups, `edits_doc` classification.

## Wave 3: components across pages (after wave 2)

- [x] 7. **Cross-page instances** (`src/editor/structure.rs`, `src/mcp/exec.rs`). `Source::Page` sync on main edit, create/swap from any page, "main missing" state. Tests: edit main on A updates B; undo per page; delete main leaves instance flagged.

## Wave 4: libraries (after wave 3; 8 then 9)

- [x] 8. **Publish and enable** (new `src/libraries.rs`). `library.json` writer with previews and publish diff, available list from the store, enable copies snapshot, library collections and components resolvable (`Source::Library`), duplicate CSS name refusal. Tests: publish, enable in second project, binding and instance work, conflict refused.
- [x] 9. **Updates and missing sources** (`src/libraries.rs`). Version compare, review diff, update keeps overrides, missing library keeps working, disable detaches after confirm. MCP `publish_library`, `list_libraries`, `enable_library`, `update_library`, `disable_library`. Tests per path.

## Wave 5: UI (after wave 4 and the mock-ups; 10, 11, 12 in parallel, then 13)

- [x] 10. **Variables modal** (new `src/ui/variables.rs`). Screenshot check against `Modal / Variables`.
- [x] 11. **Libraries modal** (new `src/ui/libraries.rs`). Screenshot check against `Modal / Libraries` and `Modal / Library updates`.
- [x] 12. **Assets view** (`src/ui/library.rs` becomes Assets). Screenshot check against `Panel / Assets`.
- [x] 13. **Design tab and inspector** (`src/ui/design.rs`, `src/ui/inspector.rs`, palette entries). Slim Design tab; variable picker on every bindable field. Screenshot check against `Inspector / Bound fields`.

## Wave 6: finish

- [x] 14. **Docs and verify**. `docs/design-system.md`, `docs/agents.md` (new tools, count), `ROADMAP.md` rows 7, 8, 12, `PROGRESS.md`; skills `skills/papercraft*` mention collections, bindings and libraries. Full `cargo test --no-fail-fast`, wasm and Windows checks, web build loads.

## Model per task

Claude Sonnet: 0, 1, 2, 4, 5, 7, 8, 10, 11, 12. Claude Haiku: 3. Codex `gpt-6.1-sol`: 9, 13, and the reviews (task 6 moved to Sonnet: Codex hit its usage limit on 2026-10-21). Codex `gpt-6-luna`: 14.
