# Swarm safety: several agents on one project

## Why

One project can now have many writers at once: the in-app Chat agent, one or more external MCP clients (Claude Code, Codex), `papercraft-cli` calls online and offline, `delegate`d helpers, the person at the canvas, and (through the relay) other machines. Today the *transport* is safe: every online call is serialized on the UI thread and is one undo step, so nothing tears. What is missing is the *semantics*: an agent that read a frame, thinks for 40 s and then overwrites it does not learn that another agent rebuilt it meanwhile; two agents can both edit `variables`; `undo` undoes whoever was last; an offline CLI and a second process can race on the folder. This spec makes those cases visible, mostly non-blocking, and recoverable.

Design stance: **cooperative and fail-fast**. No tool call ever waits on another agent (no deadlock is possible). Conflicts are reported by default and rejected only on request. The human always wins and is never blocked.

## Builds on (do not redesign)

A change log with a document revision is being added by another change. This spec assumes exactly this interface and names it `rev`:

* `rev: u64`, bumped once per committed write. Every write result returns `rev`. It is **not** `Editor::revision` (`src/editor/mod.rs:253`), which is a redraw counter that `App::run_mcp_calls` bumps after every call.
* A bounded in-memory log of entries `{rev, client, tool, page, nodes[], fields[], variables?, at}`; `client` is the label of the caller; the human is `You`.
* `get_changes {since, exclude:"self"}` and `lint_design {since | changedOnly}`.

Two small additions are needed from that log and are tracked as Phase 0 follow-ups below: entries must say **which fields** of a node changed and keep `children` as its own field name (so "another agent added a child" is distinguishable from "another agent restyled the frame"), and the log must survive restart (see 6.4). If the other change lands differently, only `src/mcp/swarm/conflicts.rs` adapts.

## What exists today (checked 2026-10-28)

* **One chokepoint, online.** `server.rs::call_tool` builds a `Call {name, args, client, reply}`, `Dispatch` sends it down an mpsc channel and wakes the loop with `UserEvent::Mcp`; `App::run_mcp_calls` (`src/app.rs:1562`) runs each call in order via `Tracker::call` (`src/runs.rs:522`), then `UiState::mark_agent`, `log_agent`, and `steer::attach`. The chat's API backend uses the same channel (`App::tool_dispatch`), and its CLI backends reach the same HTTP server with `?chat=<id>` (`server.rs::client_of`, `chat_of`). `refs`, `imagegen` and `delegate` are the exception: they run on a spawned thread (`app.rs:1573`), because the helper calls back into the app through `papercraft-cli`.
* **Identity.** `Client {name, session}` (`src/mcp/mod.rs`). `name` is the MCP `clientInfo` title or name, or `CHAT_CLIENT`; `session` is the `mcp-session-id` header or the chat id. `Tracker` keeps one open run per `Client` (`Tracker.open`, `open_of`) and labels the second session of a name "Claude Code · 2" (`Tracker::label`). `papercraft-cli` connects with `().serve(...)` (`bin/papercraft-cli.rs:309`): default client info, a **new MCP session per process**, so each `papercraft-cli call` is a different `Client` and opens its own run. A delegated helper is just another such CLI process; nothing links it to its parent.
* **Touched layers.** `Editor.touch_log` is filled by `log_touched` on every commit while `Tracker::call` has it open; `take_touched` hands (page id, node) pairs to `UiState::mark_agent`, which keeps the topmost ones as `AgentMark {page_id, node, verb, client, at}` (cap `AGENT_MARKS_MAX`, fade `AGENT_MARK_SECS`). `RunPage.nodes` persists the node set per run and page; `runs::blocker` / `keep_own` / `revert` already use it to refuse or narrow a revert when a *later run* touched the same nodes.
* **Layer `lock`** is a human flag (`Node.locked`, `is_effectively_locked`): it stops canvas hit-testing and canvas editing. No tool in `src/mcp/exec.rs` checks it, so agents write through locked layers today. This spec does not change that (open question 4).
* **Undo** is per page (`ed.with_page(page, ..)`, `History.undo/redo` stacks of `Change`), global across authors. The `undo` / `redo` tools (`exec.rs::undo_redo`) pop the top `Change` of the page whoever made it. A `Change` has no author. A design-system edit puts the same `design` before/after in a step on **every** page (`history.rs`, `Change.design`).
* **Offline.** `cli::Offline` (`src/cli.rs`) wraps `SessionHost` and calls `exec::call` directly: no `Tracker`, no runs, no versions, identity-less. It already has two locks: `APP_LOCK` (`.papercraft-app.lock` in the projects root, held by the app for its whole life from `app.rs:1714`) and `PROJECT_LOCK` (`.papercraft-cli.lock` in the project folder, taken by `lock_for_write` before the first editing call, with a 5 s wait, then a **reload** of the project from disk). `refuse_if_open` refuses `--offline` on a project the app reports as `open` in `list_resources`. All are OS advisory locks (`File::try_lock`), so a crash releases them. Gaps are listed in section 6.
* **Saving.** `Session::save` (`src/session.rs:217`) writes every dirty page, `design.json` and the manifest atomically (`io::atomic::write`, temp + rename) but **never looks at what is on disk**: it is last-writer-wins per file. Only shared projects have a three-way page merge (`sync.rs` + `merge::merge`, base copy in `.sync-base/`).
* **Skills.** `skills/papercraft/SKILL.md` teaches single-agent use; nothing about sharing a project.

## Scope

### 1. Identity

Every call is attributed to an `AgentId`:

```rust
pub struct Client {            // src/mcp/mod.rs, extended; stays Eq + Hash (Tracker key)
    pub name: String,          // "Claude Code", "Codex", "Chat", "You", "Actions", or --as value
    pub session: String,       // MCP session id, chat id, or stable agent session (below)
    pub parent: Option<String>,// session of the agent that delegated this one
}
```

`run` is not a field of `Client`: the run is derived (`Tracker.open_of(client)`), and the change-log entry stores `run_id` when one is open so Review can group them.

**Sources of identity**

| Caller | name | session | parent |
|---|---|---|---|
| MCP client | `clientInfo` title or name (as today) | `mcp-session-id` | none |
| Chat turn (API or CLI backend) | `Chat` | chat id | none |
| `papercraft-cli` online | `--as <name>` > `$PAPERCRAFT_AGENT` > `cli` | `agent:<name>` (stable across invocations) | `--parent` / `$PAPERCRAFT_PARENT` |
| `papercraft-cli` offline / `shell` | same as online | same | same |
| `delegate` helper | `<parent label> / <role>` e.g. `Chat / build` | `d<n>-<chat id>` | the caller's session |
| Actions menu | `Actions` | `""` | none |
| The person at the canvas | `You` (reserved, cannot be claimed by `--as`) | `""` | none |

* **Why a query string, not `clientInfo`.** `papercraft-cli` has no per-call client info and a fresh MCP session per invocation. The chat already proves the pattern: `?chat=<id>`. The CLI appends `?agent=<name>&parent=<session>` to the MCP URL; `server.rs::client_of` reads it (`agent_of(query)` next to `chat_of`) and builds the stable session `agent:<name>`. Names are sanitized (`[A-Za-z0-9 ._/-]{1,40}`); `You`, `Chat` and `Actions` are rejected for `--as`. An unnamed CLI is `cli` and prints one stderr hint: "set --as <name> so other agents can tell you apart".
* **Delegation.** `delegate::run` sets `PAPERCRAFT_AGENT` and `PAPERCRAFT_PARENT` on the helper process (it already sets `PAPERCRAFT_PROMPT` and `PATH`), and the preamble tells the helper to pass nothing else: `papercraft-cli` reads the env. Grandchildren inherit the env, so chains work (`parent` is the immediate delegator).
* **Offline.** `Offline::new(store, project, client)` takes the same `Client`; the CLI binary builds it from the flags and env above. Offline gets a `Tracker` (Phase 6) so offline work also produces runs, versions and attributed log entries, not an anonymous blob.
* **Run labels.** `Tracker::label` keeps its numbering for MCP clients with no stable name. A named CLI agent is not numbered (its session is the name).

**Where it shows**

* *Canvas.* `mark_agent` already carries `client` (the label). The outline and its tag are drawn in the agent's colour: `agent_color(label) -> theme slot`, a stable hash of the label into 8 theme tokens `agent_1..agent_8` (added to `src/theme.rs` next to the other palette entries; both themes; no hex in UI code). The tag reads `Codex . Styling` (label, then verb); a delegate reads `Chat / build`. A *presence pin* (a small coloured dot plus the label, no text editing cursor exists for agents) sits on the top-left corner of the last touched layer for `AGENT_MARK_SECS`. The human's own edits get no mark.
* *Agent view.* One group per `Client` with its colour dot, label, state (working per `AGENT_WORKING_SECS`, idle, finished), runs, and its claims (section 3). Delegates are nested under their parent. Rows are one line, truncate with an ellipsis, and have 44 px targets for actions.
* *Change log and `get_changes`.* Entries carry `{client, session, parent, run}`; `exclude:"self"` matches on `(name, session)`.

### 2. Optimistic concurrency

**Reference rev.** The point in history the caller's picture of a layer is from.

* Explicit: any write tool accepts `ifRev` (integer, the `rev` the caller last synced to).
* Implicit (default, no `ifRev`): the server remembers `seen[(client, page, artboard)] = rev at the end of the client's last call that read or wrote anything under that artboard` (any tool whose resolved node ids sit under it; `get_screenshot`, `get_tree_summary`, `get_jsx`, writes, ...). A client with no history on an artboard has reference = the rev when its session first called. Tradeoff: coarse (artboard, not node) so a read of the header does not hide a stale body, in the direction of *over*-reporting; it costs one small map per client, dropped when the client has been silent for an hour.

**The rule.** A write call has a target set `T` (resolved before the call from `nodeId`, `nodeIds`, `targetNodeId`, parent, page and variable arguments by `swarm::targets::of(name, args, ed)`, the same resolution `runs::target_pages` already does for pages). It conflicts when the log has an entry with `rev > ref` by a **different client** (including `You`) whose node set matches one of:

| relation | an entry conflicts when it ... | applies to |
|---|---|---|
| `self` | changed a node in `T` (any field) | all writes |
| `ancestor` | changed the **own fields** (not `children`) of an ancestor of a node in `T`, or deleted or moved one | all writes (size, layout, clip, visibility of a parent change what a child edit means) |
| `descendant` | changed any node under a node in `T` | writes with `reach: subtree` (see below) |
| `variable` | changed (value, rename, delete) a variable the call **sets**, or deleted/renamed one the call **binds** with `var(--x)` or `bind_variable` | variable tools; node writes that bind variables |
| `component` | changed the main component of an instance in `T`, or an instance's main when `T` is a main | writes to instances and mains |
| `page` | created, deleted, renamed or reordered pages | page tools |

Per-tool `reach` (table in `src/mcp/swarm/targets.rs`, one test asserts every tool in `tools::definitions()` is classified, like the `edits_doc` test):

* `node`: `update_styles`, `set_text_content`, `rename_nodes`, `set_code_link`, `bind_variable`, ... (own fields only; descendants are not affected).
* `subtree`: `write_html` in replace mode, `delete_nodes`, `move_nodes` (also its source and destination parents' `children`), `detach_instance`, `apply_ui_kit`, anything that rebuilds children.
* `children`: `write_html` `insert-children`, `create_shape` under a parent, `duplicate_nodes`: adds below the target; conflicts only through `self` / `ancestor` (target deleted or moved), so two agents appending to the same frame do not conflict.
* `none`: `create_artboard`, `create_page` (no existing target; page relation only).

**Reading the result.** Conflicts are computed *before* the call runs, against the log, on the UI thread, so there is no check-then-act gap.

```json
// warn (default): the call is applied; trailing content block of the result
{"swarm": {"rev": 58, "refRev": 51, "applied": true,
  "conflicts": [
    {"node": "1:12", "name": "Header", "relation": "self",
     "by": {"client": "Codex", "session": "agent:Codex", "label": "Codex"},
     "rev": 55, "tool": "update_styles", "at": "2026-10-28T10:02:11Z",
     "changed": ["fill", "paddingTop"]}
  ],
  "hint": "Codex changed 1 layer you just wrote. get_changes {since: 51, exclude: \"self\"}, re-read those layers, adjust."}}
```

```json
// strict: nothing applied; isError result, text is this JSON
{"error": "conflict", "applied": false, "rev": 57, "refRev": 51,
 "conflicts": [ ... same objects ... ],
 "hint": "Re-read the layers above (get_changes {since: 51, exclude: \"self\"}), then retry with ifRev 57."}
```

The result stays a `ToolResult = Result<Vec<Out>, String>`: strict errors are the JSON string above (stable `error` codes: `conflict`, `claimed`, `needs_claim`, `undo_blocked`); warnings ride in a trailing `Out::Text` block, exactly how `steer::attach` appends its note, so tools' own JSON is untouched and image results keep working. `conflicts` is capped at 20 with `"more": n`.

**Modes**

| Setting | How set | Behaviour |
|---|---|---|
| `warn` (default) | nothing | apply; report conflicts against the implicit or explicit reference |
| `strict` per call | `ifRev` present (and no `onConflict: "warn"`) | reject if any conflict against `ifRev`; nothing applied |
| `strict` global | Settings, Agents, "Reject conflicting writes"; env `PAPERCRAFT_CONFLICTS=strict` | like strict per call, using the implicit reference when `ifRev` is absent |
| `warn` per call | `onConflict: "warn"` | override the above for this call |

Precedence: `onConflict` > presence of `ifRev` > env > setting > default.

`ifRev` newer than the current `rev` (the log was reset, for example the app restarted without a persisted log) is reported as one conflict `{"relation": "history_reset"}`: reported in warn, rejected in strict.

**Recovery protocol** (what the tool hint and the skill say): on `conflict` or a non-empty `conflicts`, call `get_changes {since: <refRev>, exclude: "self"}`, `get_tree_summary` / `get_screenshot` on only the listed nodes, decide whether the other change is compatible, redo your edit if not, and continue with `ifRev` = the `rev` you got back. Never retry the same call blindly.

**The human.** Canvas edits are recorded in the log as `You` through the editor commit path (a `HUMAN` author when no tool call is running), never checked, never blocked, and reported to agents by the same `conflicts` structure (`by.client == "You"`). An agent write is not blocked by a selection or an open text edit; the in-progress text edit is committed on Esc/blur as a normal human write and conflicts back in the other direction.

### 3. Soft claims

A claim is a short lease that says "I am working here". It is advisory by default and enforced for other agents (not the human) when the setting says so.

```json
// claim_nodes
{"name": "claim_nodes", "inputSchema": {"type": "object", "properties": {
  "ids":        {"type": "array", "items": {"type": "string"}, "description": "Layer ids (\"<page>:<n>\"). Each claims the layer and everything under it."},
  "resources":  {"type": "array", "items": {"enum": ["variables", "components", "pages", "settings"]}, "description": "Shared resources that cannot be split by layer."},
  "ttlSeconds": {"type": "integer", "minimum": 10, "maximum": 1800, "default": 300},
  "note":       {"type": "string", "maxLength": 120, "description": "Shown to the person and to other agents."}},
  "anyOf": [{"required": ["ids"]}, {"required": ["resources"]}]}}
// -> {"claimId": "c7", "claimed": ["1:12"], "resources": [], "expiresAt": "...", "ttlSeconds": 300, "rev": 58}
// release_claims   {"claimId"?: "c7", "ids"?: [...], "resources"?: [...]}   (nothing = all of mine)
// -> {"released": ["c7"]}
// list_claims      {}  (read-only)  -> {"claims": [{claimId, holder, ids, resources, note, expiresInSeconds}]}
```

Resource names are the "named shared resource" form of a claim: `variables` (collections, variables, modes, tokens, the design system), `components` (creating or editing main components), `pages` (create, delete, rename, reorder, group), `settings` (project-level settings, libraries).

**Semantics**

* **Subtree.** A claim on a layer covers it and all descendants. It does not cover ancestors' own fields.
* **Overlap.** Another agent's claim `X` overlaps a request `Y` when `X` is the same layer, an ancestor, or a descendant of `Y`. A request overlapping *another client's* live claim is denied. Overlap with the caller's own claims merges (the existing claim is extended and renewed).
* **Atomic.** `claim_nodes` takes all of `ids` and `resources` or none. A denial is a normal result, not a thrown error:

```json
{"claimed": [], "denied": [
  {"id": "1:12", "relation": "ancestor", "heldBy": "Codex", "claimId": "c3", "ids": ["1:3"],
   "note": "rebuilding Settings screen", "expiresInSeconds": 212, "retryAfterSeconds": 212}]}
```

* **Limits.** At most 16 live claims per client and 64 per project (`MAX_CLAIMS_PER_CLIENT`, `MAX_CLAIMS`), so a runaway agent cannot fence off the canvas.
* **Lease, not lock.** The expiry is `now + ttl` and is renewed by any call from the holder (read or write) while the claim lives, so an active agent never loses a claim and a stalled one loses it `ttl` after its last call. Expiry is checked lazily on every claim check and by `Tracker::tick`; there is no timer thread.
* **What a claim does for others** (Settings, Agents, "Claimed layers": `Block` (default) or `Warn`; per call `onClaim` is not offered, the holder decides by claiming):
  * `Block`: a write by another agent whose resolved targets (with `reach`) overlap a claim is rejected before it runs: `{"error": "claimed", "applied": false, "claims": [{claimId, heldBy, ids, note, expiresInSeconds}], "hint": "Work elsewhere or retry after expiresInSeconds; ask the holder through your orchestrator."}`.
  * `Warn`: applied, with `claimWarnings` in the trailing `swarm` block.
  * Reads and screenshots are never blocked. The parent chain of a delegate (delegator, its delegator, ...) is never blocked by the delegate's claim, only warned, so an orchestrator can always step in.
  * The human is never blocked. A human edit inside a claim is logged as `You`, so the holder sees it as a conflict on its next call, and the canvas shows the claim as "edited".
* **Resources.** Writes that touch a shared resource map to a resource name by `swarm::targets::resources_of(name, args)`: `set_variable*`, `create_variable*`, `set_variable_mode`, token tools -> `variables`; component tools and writes whose target is a main component or inside one -> `components`; `create_page`, `delete_page`, `rename_page`, reorder and group tools -> `pages`; project settings and library tools -> `settings`. A resource claim held by someone else blocks (or warns) like a layer claim. Setting "Shared resources" (Settings, Agents): `Open` (default: no claim needed) or `Claim required` (a write to a shared resource without holding its claim is rejected with `needs_claim`; this is how an orchestrator reserves them for itself: it claims and holds them, and helpers cannot).
* **Auto-release**

| Event | Effect |
|---|---|
| `release_claims` | released |
| `finish_working_on_nodes {nodeIds}` | the caller's claims on those layers (or all of its claims when no ids) are released, then the run ends as today |
| Chat turn ends (`Tracker::finish`, `app.rs:772`) | all claims of that chat |
| `delegate` helper process ends, times out or is stopped | all claims of the helper |
| Run ends by idle (`RUN_IDLE_SECS` = 30) | **kept**: an agent thinking for 45 s is not finished; the TTL covers a dead one |
| MCP session closes (transport DELETE) | all claims of that session, if the session manager reports it (open question 3); else TTL |
| TTL passes with no call from the holder | expired |
| Project switch (`Tracker::sync_project` sees another dir) | all claims |
| App restart | gone (claims are memory-only online; MCP session ids do not survive a restart) |
| Human "Break claim" | removed; the holder's next result carries `notices: [{"type": "claim_broken", "by": "You", "claimId": "c7"}]` |

* **State machine of a claim**

| State | On | Next |
|---|---|---|
| (none) | `claim_nodes`, no overlap with others | Active |
| (none) | `claim_nodes`, overlap with another client | denied, no state created |
| Active | holder calls any tool | Active (expiry renewed) |
| Active | holder `claim_nodes` overlapping its own | Active (merged, renewed, one id) |
| Active | `release_claims`, `finish_working_on_nodes`, turn/helper end, project switch | Released |
| Active | no call from holder for `ttl` | Expired |
| Active | human Break | Broken |
| Expired / Released / Broken | any | removed at once; the change log keeps `claim_*` entries |

* **Deadlock freedom by construction.** No tool waits. A claim attempt returns immediately (granted or denied with holder and expiry). A claim is all-or-nothing, so no agent holds a partial set while asking for more; claims always expire, so no chain of holders can be unbounded. The remaining failure is *starvation*, not deadlock; the denial tells the caller `retryAfterSeconds` and the protocol (section 4) tells it to do other work first.
* **Delegation.** The `delegate` tool gains `scope` (`{"ids": [...], "resources": [...]}`), required for role `build`, ignored for `qa` and `read` (they do not edit). `run_mcp_calls` takes the claim **on the UI thread before spawning the helper** (the spawn happens at `app.rs:1573`), with holder = the helper's `Client` (name `<parent> / build`, `parent` = caller session) and a TTL of 15 min (the helper's timeout is `TIMEOUT` = 900 s), released when the process ends. If the scope is denied, the delegate call returns the `denied` list and no process starts. The parent can read and write inside the child's claim (warned, not blocked).

**Seeing and breaking claims (human)**

* *Canvas:* a claimed layer gets a dashed outline in the holder's colour and a small tag `Codex . 4:12 left` (a painted lock-and-clock icon, truncated note in the tooltip). Right-click on a claimed layer: "Claimed by Codex (4:12 left, rebuilding Settings)", "Break claim".
* *Agent view:* a `Claims` section per agent: target names, note, remaining time, a "Break" button (44 px). Resources show as chips (`variables`).
* *Editing under a claim:* allowed; the first human edit inside a claim shows a one-line status ("Codex has claimed this; it will be told") and no modal.

### 4. Swarm partitioning guidance

For the orchestrator (a person's prompt, or a lead agent). Prefer to split by things that do not share state, in this order:

| Split by | Why it is safe | Notes |
|---|---|---|
| **Page** | pages have their own `Document`, history and file; the only shared state is the design system and component sources | best default for large work |
| **Artboard** | one artboard = one subtree; claims and conflicts are cheap | the usual unit inside a page |
| **Component** | a main component and its instances | claim `components` unless every agent owns a different main |
| **Variables** | one agent owns the design system for the swarm | do this *first*, then fan out |
| **Annotations** | stored per page | partition by page like layers |

Shared resources that cannot be partitioned (each of these is a single value in the project: `design.json`, the manifest page list, component sources shared across pages via `flush_cross_pages`, project settings):

| Resource | Rule |
|---|---|
| Variables, modes, tokens | Orchestrator defines them up front. Later changes: `claim_nodes {resources:["variables"]}`, change, release, in a short window. Never switch the document mode (`set_variable_mode`) while others take screenshots: it changes what every agent sees. Do it last or have the orchestrator do it |
| Components | One owner per main component. Anyone may place instances; only the owner edits the main. Use `components` claim for new components |
| Page list and order | Orchestrator creates pages first (`pages` claim for later changes) |
| Project settings, libraries | Orchestrator only (`settings` claim held for the swarm, setting "Claim required") |

**Recommended agent protocol** (the block for `skills/papercraft/SKILL.md`, about 20 lines):

```
Sharing a project with other agents
1. Identify yourself: papercraft-cli --as <short name> (or set PAPERCRAFT_AGENT). MCP clients are named by the client.
2. Read before you write: note `rev` (get_basic_info). Work only in the layers you were assigned.
3. Claim what you will rewrite: claim_nodes {ids:[<your artboard>], ttlSeconds:600, note:"<what you do>"}.
   If denied, do other work or report back; never wait in a loop.
4. Edit with ifRev = the rev your last result returned.
5. If a result has `conflicts` or error `conflict`: get_changes {since: <refRev>, exclude:"self"}, re-read only those layers, redo what is no longer right. Do not blindly retry.
6. After a batch: lint_design {changedOnly:true}, fix findings in your layers only.
7. Shared things (variables, components, pages, settings): claim the resource, change, release in the same minute.
8. Finish: finish_working_on_nodes {nodeIds:[...]} (releases your claims) and report which layers you changed, which conflicts you saw and how you resolved them.
9. Never undo with scope "page"; use undo {scope:"mine"}.
Orchestrators: give each helper a disjoint `scope` in `delegate`; create pages and variables before fanning out.
```

### 5. Undo and revert under concurrency

**Per-client undo.** `undo` and `redo` get `scope`:

```json
{"name": "undo", "inputSchema": {"type": "object", "properties": {
  "pageId": {"type": "string"},
  "scope": {"enum": ["mine", "page"], "default": "mine",
    "description": "mine: undo only your latest step, if no later step by someone else touched the same layers. page: undo the page's latest step whoever made it."}}}}
// -> {"done": true, "what": "write_html", "step": "s41", "rev": 61}
// -> refused: {"done": false, "reason": "undo_blocked", "step": "s41", "blockedBy": [{"client": "Codex", "rev": 59, "tool": "update_styles", "nodes": ["1:12"]}]}
```

* **Authorship.** `History` stores per `Change` an `author: Arc<str>` (the client label, `You` for canvas edits), its `rev` and a step id. Coalesced canvas steps (`coalesce_key`) keep the human author.
* **Algorithm** (`History::undo_by(doc, author) -> Result<Option<Undone>, Blocked>`): find the newest `Change` `m` by the caller on the page. Let `later` be the changes above it. `m` is *independent* when, for every `l` in `later` by anyone: entry ids of `l` and `m` are disjoint, `m.design`/`m.variables`/`m.page_modes`/`m.annotations` are `None` or the same field is `None` in `l`, and `m` touched no main component (a `touched_mains` step may rewrite instances on other pages). If independent, apply `m` backwards, splice it out of the undo stack, and push it to the redo stack. If not, return `Blocked` with the offending `l`s. Conservative on purpose: if both changed `children` of the same parent they both have an entry for the parent, so it is refused. The root list is handled by set difference (remove ids `m` added, re-insert ids `m` removed after their old predecessor), so two agents each creating an artboard do not block each other's undo.
* **Redo** `scope: "mine"` re-applies the caller's last undone step if the same independence holds in reverse; as always, any new commit on the page clears the redo stack, so an agent cannot redo across someone else's later edit (documented, not worked around).
* **Default.** `scope` defaults to `mine` for every client except `You`, because "undo" from an agent should never remove a person's or another agent's work. The old behaviour is `scope: "page"`. This is a behaviour change for single-agent use when the human edits between agent steps; it is deliberate (open question 2).
* **The human's Ctrl+Z / Ctrl+Shift+Z** is unchanged: it undoes the newest step on the page whatever its author, because that is what the canvas shows and what the person expects. The status line says whose step it was ("Undid Codex: write_html"). The Agent view run rows get "Undo this step" using the same independence check, with the refusal explained in a toast.
* **Cross-page steps.** A design-system edit adds a step on every page; its independence check is against later `design` changes only, so undoing one's variable edit works while others edit layers.

**Run revert** (`runs::revert`, `revert_change`, "Restore to here") under concurrency:

* The existing `blocker(runs, run)` refuses when a *later run* touched the same nodes. Extend it with the change log: a node of `RunPage.nodes` that has a log entry by another client (including `You` and clients with no run, like offline CLI before Phase 6) with `rev` after the run's first call is **foreign-changed**.
* Outcomes of "Revert run":
  1. *No foreign-changed nodes:* as today (full page restore when the run is the only writer, node-level when `shared`).
  2. *Some:* revert only the clean nodes (the existing node-level path used for `shared` pages, now chosen whenever *any* foreign entry exists on the page since the run started, not only when another run exists), skip the foreign-changed ones, and say so: "Reverted 14 layers. Kept 3 that Codex or you changed afterwards." with a list and a per-row "Revert anyway" (restores that node to the snapshot, overwriting the later change; one undo step, shows what will be lost).
  3. *Everything is foreign-changed:* nothing is reverted; the state stays `Review`.
* An agent-driven revert (a future tool) is never forced; only the person can use "Revert anyway".
* The auto-version `Before agent run N` is a whole-page snapshot taken when *that client's* run starts, so it contains other agents' in-flight work. `RunPage.nodes` / `keep_own` already restrict review and revert to the run's own nodes; the change log makes that precise for clients that never had a run.

### 6. Offline CLI and multi-process

**6.1 What races today**

| Race | Today | Verdict |
|---|---|---|
| Offline A vs offline B, same project | `PROJECT_LOCK` taken at first edit, held until the process exits; B waits 5 s then fails; A reloads at lock time | Safe but unfair: a long `shell` starves everyone (G1) |
| Offline vs running app, project open in the app | `--offline` refused via `list_resources.open` for `open_file` / `--project` | Safe |
| Offline vs running app, project **not** open in the app, app opens it later | the app's `Session::open` never looks at `PROJECT_LOCK`; it loads mid-edit state, then saves over the CLI's later edits | Race (G2) |
| Save over a changed file (Dropbox, a second machine's copy, a manual copy) | blind overwrite | Last writer wins (G3) |
| Offline writes carry no identity, runs, versions or log entries | `Offline::call` bypasses `Tracker` | Gap (G4) |

**6.2 Per-call lease (G1).** `Offline::call` takes the project lock **around each editing call**, not for the process lifetime: `lock -> refresh -> exec -> save -> unlock`. Waits stay `PROJECT_LOCK_WAIT` (5 s), but now a held lock lasts one call (milliseconds to a second), so shells and scripts interleave. `refresh` compares the on-disk stamp (`manifest.modified` plus `len + mtime` of each page file and `design.json`) with the stamp recorded at the last load/save; for each page whose stamp changed it reloads **that page only** (replacing its `Document` and resetting only its history); unchanged pages keep their history, so `:undo` in a shell works across interleaving as long as that page was not changed meanwhile. Because every offline editing call saves before unlocking, the process has no dirty pages at refresh time, so the refresh never needs a merge.

**6.3 The app takes the lock too (G2).** `Session::open` (native) takes `PROJECT_LOCK` with a 2 s wait before loading and keeps it while the project is open (released on close or switch, and by the OS on crash). An offline process that finds it held by the app reports "PaperCraft has this project open" (same text as `refuse_if_open`) instead of "another papercraft-cli is writing". The lock file's content records the holder (`{"pid", "client", "since"}`) so errors can say "held by papercraft-cli --as Codex (pid 4120, 12 s)". The windows between app open and offline lease are one call long, thanks to 6.2.

**6.4 Identity and the log offline (G4).** `Offline` gets a `Client` and a `Tracker`; the run closes when the process ends or after `RUN_IDLE_SECS`, and the auto-version rules apply, so Review/Revert cover offline work. The change log (Phase 0) is persisted as `activity.json` in the project folder (bounded: `ACTIVITY_KEEP` = 500 entries, plus `epoch` and `rev`), written atomically by the process holding the lock after each editing call, and loaded by the app on open and by offline processes at lease time. This is what makes `ifRev` and `get_changes` meaningful across offline processes and across an app restart. A project without the file starts at `rev = 0` with a new epoch (`history_reset` for older `ifRev`). Format: additive, optional, ignored by older builds.

**6.5 Claims offline.** With no app running, claims live in `claims.json` in the project folder, read and written under the lease (expired ones are dropped on load); the holder is the stable `agent:<name>` session, so a claim survives across that agent's CLI invocations. With the app running, offline is only used on projects the app does not hold, and the app discards `claims.json` when it opens the project (agents using the app are on MCP sessions that the file cannot represent). Every offline process reads the file at lease time, so claims are enforced between offline processes too.

**6.6 Save-time check, merge or refuse (G3).** `Session::save` keeps per page the packed bytes it last read or wrote (`PageBase`, zstd, about the size of the file) and the file stamp. Before writing a dirty page it compares the stamp on disk. Unchanged (the normal case): write. Changed: three-way merge with `merge::merge(base, local, disk)` (decoded JSON), write the result atomically, and install it in the editor as one undo step "Merged changes from disk" (via `Editor::restore_snapshot`, which already exists for versions). `merge` is extended to return a report (`Merged { value, notes }`, notes for each scalar where both sides changed and local won, and each renumbered id) because RULES.md says a merge that cannot keep something must say so; the notes go to the status line and the Agent view. Refuse (save skipped, status "Disk changed and cannot be merged: ...") when either side fails to decode or the page file has a newer format version. `design.json` and the manifest are small objects; they use the same merge with `base` = last written. The shared-project sync engine keeps its own `.sync-base/` merge; this check runs first and does not replace it.

**6.7 App running, offline requested.** Unchanged: plain `call` goes through the app; `--offline` on a project the app has open is refused; `--offline` on another project works under the lease. A CLI that finds `APP_LOCK` held but no server yet keeps waiting (`app_starting`).

### 7. Chat vs MCP

Both reach `App::run_mcp_calls`, so identity, conflicts, claims, rev, runs and the log are identical. Differences, all unchanged by this spec:

| Aspect | Chat | External MCP / CLI |
|---|---|---|
| Identity | `Chat` + chat id, from `?chat=` or the API backend's dispatch | client info / `--as` |
| Steering | `steer::attach` appends the user's mid-turn messages to the next tool result | none |
| Run end | `Tracker::finish` when the turn ends; also releases claims | idle 30 s or `finish_working_on_nodes` |
| Auto mode | answers `ask_user` with the first option; unaffected | not applicable |
| Sub-agents | `delegate` (off the UI thread) | an MCP client's own sub-agents are not visible; they appear as the same or new sessions |

The swarm block must surface in chat the same way: `swarm::attach(client, r)` runs for every client right before `steer::attach`, so the chat model sees `conflicts`, `claimWarnings` and `notices` in its tool results. In the chat UI, `chat_msgs` renders a result with `swarm.conflicts` or a `claimed` / `conflict` error as a warning chip on the tool card ("Codex changed Header at rev 55") with a "Show" action that selects the node; the chat system prompt gets the protocol block of section 4 (the chat agent is a swarm member, and its `delegate` calls carry `scope`).

### 8. Relay and other machines

Out of scope: real-time cross-machine claims, and cross-machine `rev` (each machine has its own counter). In scope by reference only:

* A relay sync is **page granularity** (`sync.rs`): when both sides changed a page it merges it with `merge::merge` against `.sync-base/` and the folder wins ties; the open project reloads (`Session::sync_poll`) unless it has unsaved edits (those win and go up on the next save).
* A synced merge lands in the editor as a reload, so it is recorded in the log as one entry by `client: "Sync"`, `nodes` = the pages' changed nodes if cheap to compute, else `page: <id>, whole: true`. Agents then see it as a conflict (`by: Sync`) on their next write to that page. `Tracker` already flags "Includes changes synced from others" in Review; this reuses that signal.
* Claims are local to the machine that holds the app; a remote editor is like the human: never blocked, always reported (as `Sync`).
* Remote agents on the relay (`mcp/relay.rs`) arrive through the same MCP path on the host machine and are ordinary clients there.

## Not in scope

* Real-time collaborative cursors for humans.
* Automatic semantic merge of conflicting edits on the same property (the agent decides).
* Enforcing the human `Node.locked` flag against agents (open question 4).
* Blocking a read while another agent is writing (reads see the committed state).
* Per-property claims; layer granularity only.
* Cross-machine claims or `rev`.
* Cross-page atomic undo.
* Locking on network filesystems where advisory locks are unreliable: documented as unsupported, not detected.

## Acceptance

* Two simulated clients editing disjoint artboards produce no `conflicts`; overlapping edits produce exactly the conflicts the brute-force oracle computes (section 2 table), in `warn` and `strict`, for every relation.
* A strict rejection leaves the document, the log (`rev`), the undo stack and runs unchanged.
* `claim_nodes` on a layer makes another client's `update_styles` on it or a descendant fail with `claimed` (`Block`) or succeed with `claimWarnings` (`Warn`); the holder is unaffected; the human is never blocked. A claim expires after `ttl` of holder silence (fake clock), is released by `finish_working_on_nodes`, by turn end, by helper end, by project switch, and by Break; overlapping and ancestor/descendant requests are denied atomically.
* `delegate` of role `build` without `scope` is an error; with `scope` the helper's claim exists before the process starts and is gone after it ends.
* `undo {scope:"mine"}` undoes the caller's newest step when later steps by others touch other layers and returns `undo_blocked` with `blockedBy` when they touch the same ones; Ctrl+Z still undoes the newest step.
* Run revert with a later foreign edit to one of its nodes reverts the others and reports the kept one; revert never changes a layer the run did not touch.
* Two offline `Offline` instances on one folder (threads, real locks) each doing N edits end with 2N edits on disk and in a fresh load; the shell with an idle prompt does not block the other.
* `Session::open` on a project whose `PROJECT_LOCK` is held waits, then reports the holder. Saving a page whose file changed on disk merges and reports notes, or refuses; a layer is never silently dropped.
* A project without `activity.json`, `claims.json` or the new run fields loads unchanged; an older build ignores them.
* Tool schemas and descriptions exist for `claim_nodes`, `release_claims`, `list_claims`, the new `ifRev` / `onConflict` / `scope` arguments, each tested in `tests/mcp.rs`; README and ROADMAP tool counts are updated.
* Screenshot inspection at scale 2 in both themes of: agent colours on outlines, claim outline and tag, Agent view with two agents and a delegate, Claims section, a long agent name and a long note (no overflow, 44 px targets).
* `cargo test --no-fail-fast`, `cargo check --lib --target wasm32-unknown-unknown` (swarm logic is target-agnostic; file locks are native only) and the Windows check pass. 80% line coverage on the new `src/mcp/swarm/` modules and the changed lines elsewhere.

## Open questions for the maintainer

See `plan.md`.
