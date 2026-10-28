# Tasks

Each task is independently testable. Tests first. Stop on failure. Tasks in the same wave touch different files and can run in parallel; a wave starts when the previous one passes `cargo test`. Phase 0 (change log, `rev`, `get_changes`, scoped lint) must be merged first.

## Wave 1: foundations (parallel)

- [ ] 1. **Client identity** (`src/mcp/mod.rs`, `src/mcp/server.rs`). `Client.parent: Option<String>`; `agent_of(query) -> Option<AgentQuery{name,parent}>` next to `chat_of`; `client_of` builds `Client{name, session:"agent:<name>", parent}`; sanitize names, reject `You`/`Chat`/`Actions`. Test: parsing, sanitizing, reserved names, equality, `chat=` still wins.
- [ ] 2. **CLI identity** (`src/bin/papercraft-cli.rs`, `src/cli.rs`, `src/mcp/delegate.rs`). `--as`, `--parent`, `PAPERCRAFT_AGENT`, `PAPERCRAFT_PARENT` (flag > env > `cli`); online appends the query to the MCP URL; `Offline::new(store, project, client)`; `delegate::run` sets both env vars on the helper; one-time stderr hint for unnamed. Test: precedence, URL, offline carries the client, helper env visible (unix test), shell keeps one identity across lines. Depends on 1.
- [ ] 3. **Gate skeleton** (new `src/mcp/swarm/{mod,gate}.rs`, `src/lib.rs` or `src/mcp/mod.rs` registration; call sites in `src/app.rs::run_mcp_calls` and `cli::Offline::call`). `Swarm`, `Gate::{Allow, Reject(String)}`, `swarm::call(host, tracker, swarm, client, name, args, now)`, `swarm::attach(client, r)` (before `steer::attach`). Empty check chain; behaviour unchanged. Test: all existing `tests/mcp.rs` and `tests/cli_offline.rs` pass; a stub check can reject and annotate.
- [ ] 4. **Harness skeleton** (`tests/common/swarm.rs`, `tests/swarm.rs`). `Sim`, `SimClient`, `Step`, `FakeClock`, `Oracle` stub, `disjoint(n)` scenario: N clients write own artboards, assert every write present and ordered `rev`s. Depends on 3.
- [ ] 5. **Save-time stamp check and merge** (`src/session.rs::save`, `src/merge.rs`, `src/project/packed.rs`, `src/editor/pages.rs`). `PageBase`, stamp per page file, `merge::merge_report -> Merged{value, notes}` (keep `merge` as a wrapper), changed file -> merge -> write -> `restore_snapshot` as "Merged changes from disk"; undecodable or newer format -> refuse with a message. Test: unchanged writes; changed merges and reports; refusal; one undo step; property test: no kept id lost.

## Wave 2: behaviour (after wave 1; 6, 8, 10, 11 parallel)

- [ ] 6. **Targets and reach** (`src/mcp/swarm/targets.rs`). `of(name, args, ed) -> Targets{nodes, pages, variables, resources, reach}`, `Reach::{None,Node,Children,Subtree}`, `resources_of`. Test: every tool in `tools::definitions()` classified (fails on a new unclassified tool); resolution of `nodeId`, `nodeIds`, `targetNodeId`, parent, page, `var(--x)` in html/styles.
- [ ] 7. **Conflict rule** (`src/mcp/swarm/conflicts.rs`, `tools.rs` schemas, settings). `ifRev`, `onConflict`, `seen[(client,page,artboard)]`, relations table, warn trailing block, strict `conflict` error before execution, `history_reset`, cap 20, Settings entry + `PAPERCRAFT_CONFLICTS`. Test: each relation +/-, mode precedence, strict changes nothing (doc, rev, history, runs), human reported, oracle comparison in `tests/swarm.rs` (`overlap`, `churn` 200 seeds). Depends on 6.
- [ ] 8. **Claims core** (`src/mcp/swarm/claims.rs`). State machine, overlap (same/ancestor/descendant), atomic all-or-nothing, own-claim merge, sliding TTL, limits, lazy expiry plus `tick`, parent-chain exemption, `Block`/`Warn`, resources, `Claim required`. Pure logic over ids and a clock, no UI. Test: the full state table and each auto-release row except those needing the app (project switch is a `Swarm::reset`). Depends on 3.
- [ ] 9. **Claim tools and delegate scope** (`src/mcp/tools.rs`, `src/mcp/exec.rs`, `src/app.rs`, `src/mcp/delegate.rs`). `claim_nodes`, `release_claims`, `list_claims`; `finish_working_on_nodes` releases; turn end (`app.rs:772` neighbourhood) releases chat claims; delegate `scope` required for `build`, claim taken before spawn on the UI thread, released when the helper ends; `claimed` / `needs_claim` errors. Update README and ROADMAP tool counts. Test in `tests/mcp.rs` per tool; stub-helper test for claim lifetime. Depends on 8.
- [ ] 10. **Per-client undo** (`src/history.rs`, `src/editor/mod.rs`, `src/mcp/exec.rs::undo_redo`, `tools.rs`). `Change.author/rev/step`, `History::undo_by/redo_by`, independence check, root set-difference, `scope` argument, default `mine` for agents, `undo_blocked` result, author in the status text for Ctrl+Z. Test: independence matrix, redo, defaults, Ctrl+Z unchanged, cross-page design steps.
- [ ] 11. **Offline lease and app lock** (`src/cli.rs`, `src/session.rs::open`, `src/project/mod.rs`, `src/bin/papercraft-cli.rs`). Lock around each editing call; stamp-based per-page refresh; lock file records holder; `Session::open` takes the project lock with a 2 s wait and keeps it; error text distinguishes app from CLI. Test: 3-thread `OfflineSim` (3N + M edits), idle shell does not block, app open waits then reports the holder, other pages keep history.
- [ ] 12. **Offline runs, log and claims file** (`src/cli.rs`, change-log persistence, `src/runs.rs`). `Tracker` in `Offline`, run closed at exit; `activity.json` (bounded, epoch) written under the lease and loaded by app and CLI; `claims.json` offline claims; app discards the file on open. Test: offline run and version exist and revert; roundtrip; bound; absent file. Depends on 2, 8, 11.

## Wave 3: runs and UI (after wave 2)

- [ ] 13. **Run revert under concurrency** (`src/runs.rs`, `src/ui/panels.rs`). `foreign_changed` from the log, outcomes 1 to 3, "Revert anyway" row action, kept-layers list, "Undo this step" in run rows. Test: partial revert, all-foreign, offline-client edits count as foreign, other layers untouched. Depends on 10.
- [ ] 14. **Agent colours and presence pin** (`src/theme.rs`, `src/ui/mod.rs`, canvas painter). Theme tokens `agent_1..8` in both themes, `agent_color(label)`, tinted outlines and tags, pin, `Chat / build` label. Screenshot at scale 2 in each theme (`tests/agent_view.rs` style), long label truncates. Depends on 2.
- [ ] 15. **Claims UI** (`src/ui/panels.rs`, canvas painter, `src/ui/palette.rs`). Dashed claim outline and tag, right-click "Break claim", Agent view `Claims` section and delegate nesting, resource chips, notice to the holder on Break, "Break all claims" palette entry. Screenshots both themes, 1440p and 1280x800, long note and name, 44 px targets. Depends on 9, 14.
- [ ] 16. **Chat surface** (`src/ui/chat_msgs.rs`, `src/ui/chat.rs`). Warning chip for `swarm.conflicts` / `claimed` / `conflict` on tool cards with Show; protocol block added to the chat system prompt. Test: parse of the `swarm` block, chip text, prompt contains the protocol. Depends on 7, 9.

## Wave 4: finish

- [ ] 17. **Skill and docs**. `skills/papercraft/SKILL.md` gets the section 4 protocol and the partitioning table (kept short; run it with two real agents against a test project on a private port, per AGENTS.md "Running the app safely"); `docs/agents.md` (identity, `--as`, conflicts, claims, undo scope, offline lease); `src/mcp/tools.rs::INSTRUCTIONS`; README and ROADMAP tool counts; `ROADMAP.md` row.
- [ ] 18. **Swarm scenarios complete** (`tests/swarm.rs`). Adds `shared_resources`, `chain` (delegate), claim expiry, per-client undo, offline sim, revert-with-foreign scenarios; `churn` 200 seeds default, 5000 `--ignored`. Coverage gate 80% on `src/mcp/swarm/` and changed lines.
- [ ] 19. **Verify**. Full `cargo test --no-fail-fast` (known `tests/update.rs` failure on Windows), `cargo check --lib --target wasm32-unknown-unknown`, Windows check, `cargo clippy --all-targets`, `cargo fmt --all`. Manual: two MCP clients and the chat on one project on a private `PAPERCRAFT_HOME` and `PAPERCRAFT_MCP_PORT`, never the person's running app.

## Independence summary

* Fully independent: 5 (save merge), 10 (undo), 11 (offline lease).
* Share only the gate skeleton: 7 and 8/9.
* UI tasks 14 to 16 touch `panels.rs` in separate sections; land 14 first.

## Model per task (subagents)

Claude Sonnet: 1, 2, 3, 4, 7, 8, 9, 10, 12, 13. Claude Haiku: 6 (table), 17. Codex coding model: 5, 11, 14, 15, 16, and a read-only review after each wave. Only one editing agent per file at a time (see the hot spots in `plan.md`).
