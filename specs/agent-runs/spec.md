# Agent runs: auto-versions and change review

## Why

Agents edit the open project through MCP, the CLI or the relay link. Every call is one undo step, but a run of 20 calls is 20 undo steps, and nothing tells the user what changed. The Agent view shows only a tool-call log, held in memory and lost on restart. Users cannot trust a run they cannot review or roll back as one step.

## Design

Mock-ups live in the `PaperCraft` design file, page `Agent runs`, artboards `Agent / Runs`, `Agent / Run review` (both 344x1440, same panel size as the other states) and `Canvas / Agent finished`.

## What exists today (checked 2026-10-21)

* Every agent call, native or web, goes through one place: `App::run_mcp_calls` (`src/app.rs`, calls `mcp::exec::call` then `UiState::log_agent`) and the web loop in `src/web.rs` (also `log_agent`). The run tracker hooks in there, not in each tool.
* `src/mcp/tools.rs` already flags each tool with `read_only` (sent as `readOnlyHint`). Two cases are wrong for this feature: `lint_design` is flagged read-only but `fix: true` edits the document; `open_file`, `create_file`, `export`, `export_combined_pdf`, `export_design_system` and `select_nodes` are flagged non-read-only but do not edit any page.
* `src/versions.rs` saves a page's whole `Document` under `versions/<pageId>__<ms>.json`, with an `auto` flag. There is no pruning: auto-versions grow without limit.
* `Editor::restore_snapshot` (`src/editor/pages.rs`) is one undo step on the **active page** and replaces nodes only. It does not restore `variables`, `modes`, `mode` or `tokens`, so History restore today silently keeps the agent's variable changes. Undo history is per page.
* `diff_subtrees` (`src/mcp/diff.rs`) pairs nodes by `(name, kind, occurrence)` because it compares two different frames. A snapshot and the live page share node ids, so pairing by name would report a renamed node as removed + added and a per-row revert would act on the wrong node.

## Scope (v1)

1. **Run.** A run is the sequence of document-editing tool calls from the agent connection, ending after `RUN_IDLE_SECS` (30) with no editing call, when `finish_working_on_nodes` is called, or when the open project changes (`open_file`, `create_file`, the user opening another project). Calls that do not edit a page (the `read_only` tools, `lint_design` without `fix`, `select_nodes`, `export*`, `open_file`, `create_file`, `finish_working_on_nodes` itself) neither start nor extend a run. Failed calls do not start a run.
2. **Auto-version.** Before the first editing call of a run touches a page, that page is saved as an `auto` version named `Before agent run N` (`versions::save(.., auto: true)`). Pages first touched mid-run are saved at that moment. The target page of a call is the `pageId` argument, else the page of the node ids, else the active page; a call whose page cannot be known up front snapshots the active page. A page created in the run (`create_page`) has no snapshot; it is recorded as created.
3. **Runs list.** The Agent side view lists runs, newest first: title (the `nodeIds` names given to `finish_working_on_nodes`, else the first call's activity verb from `agent_verb`, e.g. "Drawing layout"), call count, change count, age, state chip (`Running`, `Review`, `Accepted`, `Reverted`). Connection status and Recent activity stay below.
4. **Finish banner and toast.** When a run ends with at least one change, the Agent view shows a banner and the canvas a toast: "Agent finished, N changes", with Review and Revert. A run with zero changes (for example write then undo) is dropped with its versions.
5. **Review.** Review diffs each page of the run against its auto-version, pairing nodes **by id** (a new id-paired walk in `src/mcp/diff.rs` that reuses `compare_node_props` and `FrameDiff`). Rows are grouped by top-level frame: added, changed, removed, each with a one-line detail from the first `PropChange` (`fill #fff → #111`, `+2 more`). Variable/token changes show as one group `Variables`. Clicking a row selects the node (switching page if needed) and zooms to it. While a review is open the canvas draws green (added), yellow (changed) and red ghost (removed, from the snapshot bounds) outlines and a count tag per frame.
6. **Revert run.** Restores every snapshotted page of the run, nodes **and** variables, modes, mode and tokens, as one undo step per page (undo is per page; one step overall is not possible without a cross-page history, out of scope). Pages created in the run are deleted; page renames are undone. Pages deleted in the run are re-created from their snapshot. Before reverting, if the user edited a run page after the run ended, a confirm modal says those edits will be lost too. The run becomes `Reverted`.
7. **Revert one change.** Per row: a changed node gets its snapshot properties back, a removed node is re-inserted under its old parent at its old index (or the page root if the parent is gone), an added node is deleted. One undo step. Other rows stay. Rows in the `Variables` group have no per-row revert in v1; Revert run covers them.
8. **Accept.** "Accept all" clears the overlay and marks the run `Accepted`. The auto-versions stay and show in History as `auto`.
9. **Persistence and pruning.** Run records (id, number, title, start/end ms, call count, per page: page id, version id or `created`, state, counts) are saved as `runs/<id>.json` in the project folder and survive restart. The open run is closed on load (an app crash mid-run leaves it `Review`). Pruning: keep the newest `RUNS_KEEP` (50) runs; deleting a run record deletes its auto-versions. Named (non-auto) versions are never pruned.
10. **Shared projects.** A sync merge from the relay during a run is not part of the run; the diff may then show remote edits. v1 states this in the review header ("Includes changes synced from others") when a merge landed on a run page during the run.

## Not in scope

* Per-property accept/reject (row level only).
* Telling several agents apart (a second agent's calls join the open run).
* One cross-page undo step.
* Version compare UI in History (the id-paired diff makes it cheap; separate item).
* A prompt box inside the app.

## Acceptance

* A scripted sequence of 6 editing calls on one page, then the idle timeout, produces one run, one auto-version, and a change count equal to the id-paired diff totals (`added + removed + modified`).
* Calls to every read-only tool, `lint_design` without `fix`, `select_nodes` and `export` alone never create a run or a version. `lint_design {fix: true}` does.
* A test fails when a tool is added to `tools.rs` without a classification.
* Idle for `RUN_IDLE_SECS`, `finish_working_on_nodes`, or a project switch ends the run exactly once; the next editing call starts run N+1. Tested with an injected clock.
* `rename_nodes` shows one changed row (`name`), not removed + added.
* Revert run on two pages leaves each page equal to its snapshot (nodes, `root` order, variables, modes, mode, tokens); one undo on each page brings the agent's result back. A page created in the run is gone; a page deleted in the run is back.
* History restore of a version also restores variables and tokens (fixes today's bug).
* Per-row revert of an added, a changed and a removed node each leaves the other changes intact.
* Run records reload after restart; a project without `runs/` loads unchanged; pruning past `RUNS_KEEP` removes the oldest run and its auto-versions only.
* Opening a review of a run whose version file is missing shows an error row and does not panic.
* Screenshot inspection of the Agent view (runs, review) and the canvas overlay at 1440p and 1280x800 matches the design.
* `cargo test --no-fail-fast`, `cargo check --lib --target wasm32-unknown-unknown` and `cargo check --lib --target x86_64-pc-windows-msvc` pass.
