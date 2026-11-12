# Threaded comments on the canvas (ROADMAP row 58)

## Why

The Feedback notes on layers (inspector box, Comments view, `get_feedback` / `add_feedback` /
`resolve_feedback`) are single notes with one optional reply. Row 43 (comments assigned to
agents) needs real threads: replies, resolve and reopen, and people mentioned by name.

## Model (extends `Comment`, no parallel system)

* `Comment` (on a node) gains, all serde-default and skipped when empty: `created` (unix ms),
  `point` (offset inside the layer where the pin sits; it follows moves and resizes),
  `mentions` (names `@mentioned` in the text) and `replies: [Reply { id, author, text, created,
  mentions }]`. The old single `reply` string still loads and is shown.
* Mentions are parsed from the text when a note or reply is written (`@` + letters, digits, `_`,
  `-`, after a non-name character) and stored as names; the UI highlights them from the text.
  No notifications and no accounts yet (row 54): a name is just data.
* The author is `user` for the UI (there is no local user name in the app yet; the machine name
  in presence is a device label, not an author) and `agent` or the given `author` over MCP.
* A thread pinned to a free canvas point (no layer) is not part of this change: threads live on
  layers, optionally at a point inside one.

## Format

A page that holds a timestamped, pinned, mentioning or replied-to comment is written as page
**v11** (`needs_v11`), the oldest version otherwise, so older apps only refuse pages that really
need the new fields instead of dropping replies on save. The v10 to v11 migration is the
identity (every field defaults). Plain old notes still save as before.

## Edits

Every change is one `Editor::edit` step: start a thread, reply, resolve, reopen, delete.
`Node::post_comment`, `reply_to` and `set_thread_resolved` are shared by the UI and MCP.

## UI

* Comments side view: each thread is a card (layer name, resolve/reopen and delete chips, author
  and age, text with highlighted mentions, replies, a reply box). Filter chips Open / Resolved /
  All already exist. Long text wraps.
* The inspector's Feedback section shows the same cards for the selected layer.
* The canvas dot sits at the thread's `point` when it has one, else on the layer's corner.

## MCP (no tool added or removed; count unchanged)

* `add_feedback`: `commentId` replies to that thread; `point {x, y}` pins a new thread.
* `resolve_feedback`: `reply` is added to the thread as a reply; `reopen: true` reopens.
* `get_feedback` / `get_selection` rows carry `point`, `mentions` and `replies`.

## Acceptance

* Old files open unchanged; a v11 page is refused by a v10 reader as too new.
* Tests: model (mentions, replies, resolve, serde defaults), format (v11 selection and
  TooNew), MCP round trip (`tests/mcp.rs`).
