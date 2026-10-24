# Chat ↔ canvas: spec

What and why for (a) the gaps in `gaps-found.md`, (b) a chat that acts and points at the canvas, and (c) chat grounded in the project's design system. Research and the current-state map are in `research.md`. Cost: **S** < 1 day, **M** 1–3 days, **L** > 3 days. Everything here uses egui primitives already in the tree (Area overlays, painter, popups) and no new crates.

## Principles

* **Two lanes.** Deterministic actions run in-process with no model. Intent actions send a scoped prompt. Both go through `runs::Tracker::call`, so each lands as one reviewable, revertable run.
* **Identity + pixels + region.** What the user points at goes to the agent as node ids, a crop image, and a world rect.
* **Reuse before draw.** The agent sees the design system every turn, and a lint flags work that should have been an instance or a token.
* **Never steal the camera.** Agent camera moves obey the existing `SELF_FOLLOW_QUIET` rule (`src/ui/chat.rs`).

## P0

### P0.1 Annotations in screenshots (gap 1) — S/M
`get_screenshot {annotations?: bool}`. When true, the image includes note cards, badges and leader lines for notes whose target is inside the captured roots.
* How: clone the page doc, add each card as temporary frame + text nodes at `annotation_layout` placement (`card_size`, the placement and routing functions), add leader lines as path nodes, then call the existing `io::png::render`. Captured bounds grow to include the cards. No second renderer.
* Accept: a test renders a frame with 2 notes, and the PNG bounds include both cards. With `false` or omitted, the output is byte-identical to today. `docs/annotations.md` and `skills/papercraft/references/tools.md` are updated.

### P0.2 Viewport: read, set, switch page (gaps 2, 3) — S
* `get_basic_info` and `get_selection` gain `viewport: {pageId, rect:[x,y,w,h] world, zoom}` from `ed.camera` and `ed.viewport`.
* New tool `set_viewport {pageId?, nodeIds? | rect?, zoom?, padding?}`. It switches the active page when `pageId` differs (as the page list does) and fits the nodes or rect. `zoom` sets an explicit level within the canvas's min/max, instead of the 1.0 cap in `select_nodes`. It is refused with `"user is moving the canvas"` when the camera moved within `SELF_FOLLOW_QUIET`, unless `force:true`.
* `select_nodes` keeps its behaviour. Its error on another page points to `set_viewport`.
* Accept: tests for the page switch, fit, explicit zoom and the quiet refusal. The tool count in `docs/agents.md` is updated.

### P0.3 Runs per agent client + per-run review (gap 7, run-card follow-ups) — M
* `PaperServer::call_tool` reads `context.peer.peer_info()` (rmcp `clientInfo.name`/`title`) and the session id, and passes `client` on `Call`. Chat turns pass their chat id. `Tracker` keeps one open run per client key, instead of one global `open`. `Run` gains `client: String` (serde default `""`).
* Agent view and chat: each run shows its client ("Claude Code · subagent 2"). Revert of one run is already in `runs::revert`; refuse it when a later run touched the same nodes, and offer "Restore to here" (`revert_all`).
* Fix the run card going stale after undo (refresh `ReviewDiff` on `history.generation()` change). The "Working" pill clears on turn end. Stop shows as "Stopped", not an error.
* Accept: two MCP sessions editing at once give two runs with the right counts. Reverting one leaves the other's nodes. The `tests/mcp.rs` and `runs.rs` tests pass.

### P0.4 Design-system context in every chat turn (user request) — M
Today `system_prompt()` (`src/ui/chat.rs:602`) is the skill text plus the project name. Add a **DS digest**, built in-process from data the tools already expose:
* **Variables/tokens**: collection → group → names, with modes. Values only for colors, listed as `name=#hex/#hex(dark)`. Tokens (space, radius, type) as scales. Source: same as `get_variables`.
* **Components**: page mains and enabled-library mains (`get_components`, `ed.library_docs`), as `componentKey · name · w×h · uses`. Sorted by uses, then name.
* **Libraries**: enabled libraries with their update status (`libraries::status`).
* **Annotation template**: the active template's enabled types (`get_annotation_template`).
* **Rules line**: "Reuse: use `create_instance`/`swap_instance` for anything matching a component; colors must be `var(--…)`; spacing on tokens."

Budget and caching:
* Cap the digest at ~1,500 tokens (≈6 KB, a named constant). When over the cap, keep the top components by use, collapse variable groups to counts, and append "call get_components / get_variables for the rest".
* Cache it on `ChatState`, keyed by a hash of (variables, tokens, component keys + names, enabled library versions, template id). Rebuild only on a key change.
* API backend: the digest goes in the system prompt, in its own cache-breakpoint block after the skill (Claude host). CLI/ACP backends: they get the system prompt once per session, so when the hash changes between turns, prepend `[Design system changed] <digest>` to the next user message.
* Settings → Chat gets a toggle, "Send design-system summary" (default on), because it costs tokens.

**Reuse check** (the Figma Make failure mode):
* New lint rule `not-an-instance`: a frame whose structure matches a component main (same child kinds/count, size within 4 px, same text roles) but that is not an instance. Its fix is "swap_instance with componentKey X". Computed with the same structure walk `diff_frames` uses.
* `lint_design {fix:true}` does **not** auto-swap; swapping is a user action (P0.5).

Accept: a unit test of the digest for a fixture project, checking it is under the cap with top-N kept. A test that the hash is unchanged when unrelated nodes move. A lint test where a hand-made copy of a button is flagged and an instance is not. The context meter shows the added tokens.

### P0.5 Scoped actions on a selection: "Optimize" menu (user request) — M
An **Actions** menu on the selection. It opens from the canvas right-click menu (extend `canvas_menu` in `src/ui/annotations.rs:438`, which today has only "Annotate"), from an "Actions" chip next to `@ Selection` in the composer, and from the command palette (`src/ui/palette.rs`). It works in Design and Chat modes.

Deterministic lane (in-process, no model, one run each, instant):
| Action | Runs | Exists |
|---|---|---|
| Fix lint here | `lint_design {nodeId, fix:true}` | yes |
| Bind to tokens | for each hard-coded color, space or radius in the subtree, `bind_variable` to the nearest variable/token within a tolerance; the rest are listed in the result | bind exists; nearest-match is new (S) |
| Use components | for each `not-an-instance` hit, `swap_instance` | needs P0.4 lint |
| Rename layers | `rename_nodes` for default names (`Frame 12`, `Text`) from text content or role | new heuristic (S) |
| Annotate | opens the existing popover | yes |

Intent lane (sends a prompt to chat with a selection chip and crop; switches to Chat mode like "Annotate with AI" does, but **sends**, because the action is explicit):
* **Optimize**: "Tighten layout and spacing to the token scale, fix contrast to AA, keep the content. Prefer existing components."
* **Make consistent with DS**: "Replace hand-made parts with instances of the listed components, bind every color and space to variables."
* **Annotate with AI** (existing), **Write states** (empty, loading, error), **Variations ×3** (P1.3).
Prompts live in one const table in `src/ui/chat_msgs.rs`, next to `SUGGESTIONS`.

Review: every action, in either lane, ends as a run, so the existing review overlay, Run card and Revert/Accept apply. A deterministic action shows a toast "Bound 14 colors · Review · Revert".

Accept: each deterministic action has a test on a fixture (counts, one undo step, one run). The intent actions put the right prompt and chips into a sent message (pattern: `ai_prompt_and_chat_hand_off_do_not_send`).

### P0.6 CLI/app version check (gap 4) — S
* `papercraft-cli call`/`shell` compares its own `CARGO_PKG_VERSION` with the app's `serverInfo.version` (`src/mcp/server.rs:39`). On mismatch it prints once to stderr: `papercraft-cli 0.6.1 ≠ app 0.6.2; reinstall the CLI`.
* `list`/`schema` use the app's `tools/list` when the app is running and fall back to the built-in list offline, so new tools show even from a stale CLI.
* Accept: a test with a fake server version prints the warning. `list` against a running app includes a tool absent from the built-in list.

## P1

### P1.1 Point at the canvas from chat: pick and area — M
* The composer gets **Pick** (crosshair; click a layer or Shift+drag an area; Esc cancels) next to `@ Selection`. It does not change the user's selection.
* `SelChip` (`src/ui/chat.rs:201`) gains `rect: Option<[f64;4]>` (world) and `image: Option<path>`. An area chip is sent as `[Area] page P, rect x,y,w,h, contains: <ids…>` plus the crop image. Layer chips also attach a crop.
* Crop: `get_screenshot`'s render of the intersecting roots, clipped to the rect (S; add a `clip` rect to `io::png::render`), with annotations per P0.1. Images go through the existing attachment path (image blocks for API; `.papercraft-chat-tmp/` paths for CLI/ACP).
* ⌥-click on the canvas in Chat mode adds a layer chip directly (Cursor pattern). Chips are inserted at the composer cursor.
* Accept: an area chip lists the contained node ids and attaches a PNG. Several picks give several chips.

### P1.2 Agent presence and live outlines — M
* While a run is open, outline the nodes from the last N calls' `follow_node` keys (`chat_msgs::follow_node`) on the canvas in the accent color, fading over 3 s. Label the newest outline with the verb (`agent_verb`) and the client name: an **agent cursor**. Draw it as one `egui::Area` like `review_overlay`, on any page; a chip "Agent on page X →" shows when the agent works on another page.
* This works for MCP clients too (not only chat), using the per-client run from P0.3.
* After the turn: today's review overlay. During the turn: cheap touched-outlines only (no full diff per call).
* Accept: a UI test shows the overlay present while a run is open and gone 3 s after the last call. There is no repaint loop when idle (respect `AGENT_PILL_FRAME_MS`).

### P1.3 Variations side by side — S/M
* Action "Variations ×3" duplicates the frame 3 times to the right (`duplicate_nodes`, offset by width + 80), names them `<Frame> · Variant A/B/C`, and sends the agent one prompt per variant with distinct directions (compact, spacious, bold). One run.
* Each variant gets an on-canvas tag with **Keep**. Keep deletes the other variants and the original, or replaces the original with the kept one when "replace original" is on. It is one undo step.
* Accept: Keep leaves exactly one frame with the original's name and position.

### P1.4 On-canvas Ask popover — S
* ⌘K on a selection (or "Ask…" in the Actions menu) opens a small prompt box anchored above the selection, using the annotation popover's placement. Enter sends to chat with the chip and crop, Chat mode stays as it is, and the reply streams in the chat column. This gives the Stitch/v0 "type on the element" feel without leaving Design mode.
* Accept: sending from the popover yields the same message as chip + composer.

### P1.5 Progress for long batches (gap 6) — S
* The run tracker exposes `calls` and `added/changed/removed` live. The pill shows `Styling · 212 calls · 1,340 changes` for any client, and the chat's step group shows the count while running.

### P1.6 Notes to prompt — S
* "Send open notes to chat" on a frame: open feedback + annotations of type `todo`/`question` become one batched prompt with a chip (the v0 annotations pattern). It reuses `selection_notes`.

## P2

* **True ghost preview** (L): apply agent edits to a shadow `Document` and draw it translucent over the canvas with Accept/Reject before commit. Costly: every tool would have to target the shadow doc and the renderer composites two docs. P1.2 + P1.3 + run revert cover most of the value; revisit only if users ask.
* **Dev reload** (gap 5, M): `papercraft --reopen` saves, re-execs the new binary and reopens the project and page. It is a menu item in debug builds.
* **Chat modes** (S): "Critique" (read-only tools only, returns a list of issues as annotations) vs "Build". It filters `tool_specs()` per mode, as tldraw does.
* **Sketch to UI** (M): draw on a whiteboard page, then "Make real" sends the crop with the DS digest and the prompt to build a frame next to it.
* **Voice** (M): push-to-talk into the composer through the OS dictation API. It is out of scope until the other items ship.
* **Docs** (S, gap 9): document `get_tree_summary` depth with an example; consider `depth` counting from the artboard.

## Out of scope

* New model backends, in-app agent install, web build chat.
* Gap 8 (fixed). The 0.6.2 platform items (GPU panic, Wayland clipboard, email ellipsis) are tracked in their own specs.

## Order

P0.2, P0.1, P0.6 (small, unblock agents), then P0.4 (digest + lint), then P0.5 (actions, needs the lint), then P0.3. After that P1.1 and P1.2, then the rest.
