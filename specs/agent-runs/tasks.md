# Tasks

Each task is independently testable. Stop on failure. Tasks in the same wave touch different files and can run in parallel; a wave starts when the previous one passes `cargo test`.

## Wave 1: foundations (parallel)

- [x] 1. **Full restore** (`src/editor/pages.rs`). `restore_snapshot` also restores `variables`, `modes`, `mode`, `tokens` inside the same undo step (use the variable edit path so it undoes too). Test: restore after changing a variable, a token and the mode gives a doc equal to the snapshot; one undo brings the changes back. Fixes History restore today.
- [x] 2. **Classify tools** (`src/mcp/tools.rs`). `pub fn edits_doc(name: &str, args: &Value) -> bool`: false for `read_only` tools, `select_nodes`, `export`, `export_combined_pdf`, `export_design_system`, `open_file`, `create_file`, `finish_working_on_nodes`; `lint_design` only with `fix: true`; true for the rest. Also drop `readOnlyHint` from `lint_design` (its `fix` mode edits). Test: every tool in the list is classified explicitly, so adding a tool without a class fails.
- [x] 3. **Id-paired diff** (`src/mcp/diff.rs`). `diff_pages(before: &Document, after: &Document) -> PageDiff`: pair nodes by id, reuse `compare_node_props`; group by top-level frame of `after` (removed nodes by their `before` frame); report variable/token/mode changes as their own list. Test: added, changed, removed, renamed (one changed row), reordered root, variable change.
- [x] 4. **Run records** (new `src/runs.rs`, register in `src/lib.rs`). `Run`, `RunState { Running, Review, Accepted, Reverted }`, `RunPage { page_id, version: Option<String> /* None = created in run */ }`, `RUN_IDLE_SECS = 30`, `RUNS_KEEP = 50`; `save`, `list` (newest first), `delete` (also deletes its auto-versions), `prune`. Follows `src/versions.rs` style and `crate::vfs`. Test: roundtrip, project without `runs/`, prune keeps 50 and removes only the pruned runs' versions.

## Wave 2: tracking (after wave 1)

- [x] 5. **Run tracker** (`src/runs.rs` `Tracker`, hooked in `App::run_mcp_calls` in `src/app.rs` and the web loop in `src/web.rs`). Before an editing call: open a run if none, snapshot the target page(s) not yet in the run (`Before agent run N`). After: count calls; failed calls do not open runs. End on idle (`tick(now)` from the frame loop), `finish_working_on_nodes`, project change. On end: compute diff; zero changes drops the run and its versions; else state `Review` and save. Clock injected. Test: 6 calls = 1 run + 1 version; gap gives run N+1; read-only only gives none; `lint_design {fix}` opens one; `create_page` records `created`; project switch ends the run.

## Wave 3: actions (after wave 2)

- [x] 6. **Revert run** (`src/runs.rs` + `Editor`). Per page: full restore (task 1) as one undo step; delete pages created in the run; re-create pages deleted in the run from their snapshot; undo renames. State `Reverted`. Test: two pages equal snapshots; undo on each returns the agent's result; created page gone; deleted page back.
- [x] 7. **Revert one change** (`src/runs.rs` + `Editor`). Changed: copy snapshot props. Removed: re-insert subtree under old parent/index, or root if parent gone. Added: delete. One undo step. Test each kind; other rows unchanged.

## Wave 4: UI (after wave 3; 8 and 11 parallel, then 9, then 10)

- [x] 8. **Agent view runs list** (`src/ui/panels.rs`). Runs list, finish banner, state chips, kit components and icons, same panel size. Use `AGENT_WORKING_SECS` for the live chip (it hard-codes 8 today). Screenshot check against `Agent / Runs`.
- [x] 9. **Review view** (`src/ui/panels.rs`). Changes grouped by frame, `Variables` group, diff rows with per-row revert, Revert run (confirm modal if the user edited a run page after the run), Accept all, click to select (switch page) and zoom, error row for a missing version, "Includes changes synced from others" note. Screenshot check against `Agent / Run review`.
- [x] 10. **Canvas overlay** (`src/ui/mod.rs` or the canvas painter). Green/yellow outlines, red ghost for removed nodes, per-frame count tags, finish toast with Review and Revert. Screenshot check against `Canvas / Agent finished`.
- [x] 11. **Command palette** (`src/ui/palette.rs`). Review last agent run, Revert last agent run.

## Wave 5: finish

- [x] 12. **Web build**. Same tracker through `src/web.rs`; `cargo check --lib --target wasm32-unknown-unknown`; check runs list and revert in the browser build.
- [x] 13. **Docs and verify**. `docs/agents.md` (runs and review; tool count is 53, it says 40), `docs/feature-ideas.md` (items 2 and 3), `PROGRESS.md`, `ROADMAP.md` rows 39 and 40. Full `cargo test --no-fail-fast`, wasm and Windows checks.

## Model per task (subagents)

Claude Sonnet: 3, 4, 5, 7, 8, 9, 10. Claude Haiku: 1, 2. Codex `gpt-6.1-sol` (coding workhorse): 6, 12, and a read-only review after each wave. Codex `gpt-6-luna` (fast, easier tasks): 11, 13.

Codex runs: `codex exec -m <model> --dangerously-bypass-approvals-and-sandbox "<prompt>" < /dev/null` for edits, `--sandbox read-only` for reviews. Only one editing agent per file at a time.
