# Plan

Read [spec.md](spec.md) first. Phase 0 is the change log and `rev` that another change is adding; nothing here starts before it merges (the harness skeleton and identity can start against its interface).

## Architecture in one paragraph

A new module tree `src/mcp/swarm/` holds the target-agnostic logic (compiles for wasm): `mod.rs` (the `Swarm` struct: claims, `seen` map, settings, and `Swarm::check(...) -> Gate` / `Swarm::finish(...)` / `swarm::attach(client, result)`), `targets.rs` (tool -> targets, `reach`, resources), `conflicts.rs` (the rule over the change log), `claims.rs` (state machine, TTL, overlap), `gate.rs` (the ordered checks, so claims and conflicts are separate files). `App::run_mcp_calls` and `cli::Offline::call` wrap their existing `Tracker::call` with `Swarm::check` before and `swarm::attach` after; nothing else in `exec.rs` changes, so the tools stay oblivious. Per-client undo lives in `history.rs`/`editor` with a thin change in `exec::undo_redo`. Offline and save-time work is in `cli.rs`, `session.rs`, `project/`, `merge.rs`.

## Phases

Size: S under 150 changed lines, M 150-500, L over 500, tests included. "Needs" lists real dependencies; phases with no overlap in files can run in parallel (wave column).

| # | Phase | Needs | Wave | Size | Files touched |
|---|---|---|---|---|---|
| 0 | Change log, `rev`, `get_changes`, scoped lint (other agent, external) | | - | | `src/mcp/changes*`, `exec.rs`, `tools.rs` |
| 1a | Identity (protocol) | 0 | 1 | M | `src/mcp/mod.rs` (`Client.parent`), `src/mcp/server.rs` (`agent_of`), `src/bin/papercraft-cli.rs` (`--as`, `--parent`, env, URL query), `src/cli.rs` (`Offline::new(.., client)`), `src/mcp/delegate.rs` (env), `src/runs.rs` (label for named sessions, run id in log entries) |
| 1b | Harness skeleton + gate skeleton | 0 | 1 | M | `tests/common/swarm.rs`, `tests/swarm.rs`, `src/mcp/swarm/{mod,gate}.rs` (empty `Gate` chain, `attach`), call sites in `src/app.rs` and `src/cli.rs` |
| 2 | Conflicts: `ifRev`, rule, warn/strict, settings | 1a, 1b | 2 | L | `src/mcp/swarm/{conflicts,targets}.rs`, `src/mcp/tools.rs` (`ifRev`/`onConflict` on write tools, `reach` test), `src/ui/settings.rs` + `src/settings`-side struct, `src/ui/chat_msgs.rs` (warning chip) |
| 3 | Claims: tools, state machine, auto-release, delegate scope | 1a, 1b | 2 | L | `src/mcp/swarm/claims.rs`, `src/mcp/tools.rs` (3 tools, `delegate.scope`), `src/mcp/exec.rs` (3 tool arms call `Swarm`), `src/app.rs` (claim before delegate spawn, turn end, project switch), `src/mcp/delegate.rs` (release on exit) |
| 4 | UI: agent colours, presence pin, claim outlines, Agent view, Break | 1a, 3 (claims part) | 3 | L | `src/theme.rs` (`agent_1..8`), `src/ui/mod.rs` (`mark_agent`, `AgentMark`), `src/ui/panels.rs` (Agent view), canvas painter, `src/ui/palette.rs` (Break all claims) |
| 5 | Per-client undo and run revert under concurrency | 0, 1a | 2 | M | `src/history.rs` (`author`, `undo_by`, root set-diff), `src/editor/mod.rs`, `src/mcp/exec.rs::undo_redo`, `src/mcp/tools.rs` (`scope`), `src/runs.rs` (`blocker`, `revert`, `foreign_changed`), `src/ui/panels.rs` (Undo this step, kept-layers list) |
| 6 | Offline: per-call lease, page refresh, app takes the lock, holder info, `Tracker` in `Offline`, `activity.json`, offline claims file | 0, 1a | 2 | L | `src/cli.rs`, `src/session.rs` (`open`), `src/project/mod.rs` (stamp), `src/bin/papercraft-cli.rs`, changes persistence file |
| 7 | Save-time stamp check, three-way merge, merge report | none (6 helps) | 1 | M | `src/session.rs::save`, `src/merge.rs` (`Merged`/notes), `src/project/packed.rs` (keep bytes), `src/editor/pages.rs` (`restore_snapshot` reuse) |
| 8 | Skill, docs, tool counts | 2, 3 | 4 | S | `skills/papercraft/SKILL.md`, `docs/agents.md`, `README.md`, `ROADMAP.md`, `src/mcp/tools.rs::INSTRUCTIONS`, chat system prompt (`src/ui/chat.rs`) |
| 9 | Swarm scenarios and coverage gate | per phase | with each | M | `tests/swarm.rs` grows with each phase |

**Order and parallelism.**

* Wave 1: 1a, 1b, 7 in parallel (disjoint files: 7 touches only session/merge/packed).
* Wave 2 (after 1a and 1b merge): 2, 3, 5, 6 in parallel. They share only `src/mcp/tools.rs` (different tool entries: add each in its own `fn` group to avoid conflicts) and `src/mcp/exec.rs` (3 and 5 add arms in different places). 2 and 3 plug into the `Gate` chain from 1b as separate files, so they do not edit each other's code. 5 and 6 are independent of 2 and 3.
* Wave 3: 4 (UI), once 3 exposes the claims API; the colour/pin half of 4 only needs 1a and can start in wave 2 if split (`4a` colours and pin, `4b` claims UI).
* Wave 4: 8.
* Merge conflict hot spots to coordinate: `tools.rs` (tool list and the `edits_doc` classification: new tools `claim_nodes`/`release_claims` edit nothing; `list_claims` read-only), `app.rs::run_mcp_calls` (1b owns the call-site edit; 3 adds the delegate claim in its own function), `panels.rs` (4 and 5 add different sections).

## Test plan

**Harness (Phase 1b, grows per phase).** `tests/common/swarm.rs`:

* `Sim { host: SessionHost, tracker: Tracker, swarm: Swarm, clock: FakeClock }` over a temp store (`temp_store`, as `tests/cli_offline.rs`), headless (no GPU unless a step needs a screenshot), single-threaded: determinism comes from scripted order, not threads.
* `SimClient { client: Client }`; `Step { at_ms, client, tool, args, expect }`; `Sim::run(script) -> Vec<Outcome>`. Every step goes through the **same entry the app uses** (`swarm::call(host, tracker, swarm, client, name, args, now)`, the function `App::run_mcp_calls` and `Offline::call` both call), so the harness cannot drift from production.
* `Sim::human(edit)` applies a canvas edit through `Editor::edit` with the human author, to test `You`.
* Scenario builders: `disjoint(n)` (N clients, own artboards), `overlap(n)` (shared frame), `shared_resources()`, `chain()` (delegate parent/child), `churn(seed)` (random ops from `fastrand::Rng::with_seed`).
* **Oracle.** A deliberately naive reference (`Oracle`) re-implements the section 2 table over the list of executed ops (per-node last-writer map, ancestor walk, per-tool `reach`) in O(n^2) and is compared with the product's `conflicts` and with the final document: every accepted write's value is present unless a later accepted write to the same property exists (no lost update). `churn` runs 200 seeds in CI, 5000 under `--ignored`.
* Offline: `OfflineSim` runs 3 `Offline` instances on threads against one folder with real locks (different `File` handles conflict on every platform), each doing N edits to its own artboard plus M to a shared one; asserts 3N + M edits in a fresh `Project::load_all`, no torn file, no panic, and that a held idle lock (shell between lines) does not block.

**Per-phase unit tests**

* 1a: `agent_of` parsing and sanitizing, reserved names, `Client` equality with `parent`, env -> flags precedence, `delegate::run` passes env (existing unix test extended with `env | grep`), stable session for `--as`, run label for named sessions.
* 2: each relation row positive and negative (self, ancestor own-fields vs `children`, descendant only for `subtree`, `children` reach not conflicting on sibling append, variable set vs bind, component, page); implicit reference per `(client, artboard)`; `ifRev` > current -> `history_reset`; strict leaves `rev`, doc, history, runs untouched (assert equality of all four); mode precedence table; human edit reported; conflict cap 20; every tool classified.
* 3: grant, deny with each `relation`, atomic denial (nothing claimed), own-claim merge, TTL renewal by reads, expiry with fake clock, each auto-release row, limits, `Block` vs `Warn`, parent-chain exemption, human exempt, resource names and `Claim required`, delegate scope required for `build`, claim exists before spawn and is gone after helper exit (stub command).
* 4: `agent_color` stable and in range; screenshot tests at scale 2 in both themes (`tests/agent_view.rs` style) for outlines, claims tags, Agent view, long names; Break removes the claim and the holder gets the notice.
* 5: `undo_by` independence matrix (disjoint ok; same node blocked; same parent children blocked; root set-diff ok; design/variables/page_modes/annotations; main component blocked); redo mine; default scope; human Ctrl+Z unchanged; run revert partial with a foreign edit, all-foreign, "Revert anyway".
* 6: lease around each call; interleaved shells; page refresh keeps other pages' history; app `Session::open` waits and reports the holder; holder info in the lock file; `activity.json` roundtrip, bounded, absent file; offline run/version created; `claims.json` roundtrip and expiry.
* 7: unchanged stamp writes; changed stamp merges and reports notes; undecodable or newer-format file refuses; merged result is one undo step; no layer dropped (property test over `merge::merge`: node count of merged >= union of kept ids).

**Coverage.** `cargo llvm-cov` (or `cargo tarpaulin` where the maintainer already uses it) on `src/mcp/swarm/`, `history.rs` changes and `cli.rs`; gate at 80% lines for the new modules. UI painting is covered by screenshots, not counted.

## Risks

| Risk | Mitigation |
|---|---|
| Conflict noise: agents ignore `conflicts` that fire on every call | `children` is a separate field so sibling appends do not fire; the implicit reference is per artboard; `reach` limits `descendant` to subtree tools; conflicts cap at 20; measure on the harness `churn` scenario (target: no conflict on pure disjoint work, by construction) |
| Under-reporting from the implicit reference (coarse, artboard-level) | `ifRev` is exact; the skill tells swarm agents to use it |
| Default `undo` scope change breaks single-agent habits | `scope: "page"` keeps the old behaviour; the tool description says so; only differs when someone else edited in between |
| Claims fence off the canvas (a stalled agent) | TTL with renewal only by the holder's calls, per-client and per-project limits, human Break, parent exemption |
| MCP session close not observable with rmcp's `LocalSessionManager` | TTL fallback; Phase 3 checks whether the manager can notify, else adds a `Drop`-guard on the per-session service handle |
| Per-call offline lease and page refresh cost on large projects | refresh is by stamp, only changed pages; `verify_large.rs`/`perf.rs` get a case |
| `PageBase` memory (compressed bytes of every saved page) | bytes already exist at save time; keep only for pages saved in this session; evict when the page is unloaded by `lazy_load` |
| Advisory locks on network folders | unsupported, documented; holder info and stamp check (Phase 7) still catch lost updates after the fact |
| Merge hides a semantic clash (both changed the same scalar, local wins) | the merge report surfaces each such scalar; the human decides |
| Change log assumptions differ from what Phase 0 ships | all dependence is in `conflicts.rs` and the entry struct; reconcile before wave 2 |
| `tools.rs` / `app.rs` merge conflicts between parallel agents | file ownership table above; each phase adds code in new files and one-line call sites |

## Open questions for the maintainer

1. **Default for claims:** `Block` (proposed) or `Warn`? Block makes claims meaningful but can stop an agent in a way the user sees as "stuck"; the denial message names the holder and time.
2. **`undo` default scope `mine`:** accept the behaviour change for single-agent use, or keep `page` as the default and make agents opt in through the skill? (Proposed: `mine`.)
3. **Session close hook:** is it acceptable to patch or wrap rmcp's session manager to learn of a closed session, or is TTL expiry enough?
4. **Human `locked` flag vs agents:** should agent writes honour `Node.locked` (reject with a clear error)? It is a one-line check in `swarm::targets` and would make "lock this for me" work with agents; it also changes today's behaviour where agents edit locked layers. Out of scope here unless you say otherwise.
5. **`delegate` `scope` required for `build`:** is a hard error acceptable, or should it default to the caller's current claims?
6. **Offline runs:** is it right that offline edits now create runs and auto-versions (disk cost per CLI invocation), or should they be opt-in (`--run`)?
7. **Merge on save (Phase 7):** worth its memory and complexity now, or defer until a sync-less second-writer case actually occurs? It is independent and can be dropped without affecting the rest.
8. **Notice/conflict placement:** trailing `swarm` content block (proposed) vs adding keys to the tool's JSON when it is an object. The block is uniform for text-only and image results; keys are easier for some clients to parse.
9. **Naming collision:** `rev` (log) vs `Editor::revision` (redraw counter). Rename the editor field (touches many files) or keep both and document?
10. **Where does the human's mid-edit text field count?** Proposed: nowhere (committed on blur). Confirm.
