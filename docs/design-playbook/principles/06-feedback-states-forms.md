# Feedback, states, errors, forms

Sources: S04 (heuristics 1, 5, 9), S05 (timings), S09 (undo), S10, S11,
V01. Interface honesty: the interface may show a result before the server
confirms (optimism), but must never present a falsehood it cannot repair.

### FBK-01 Every action gets feedback near where it happened
Inline checkmark after "Copy", highlighted field on error, row-level error on a
failed save. Avoid generic top-of-page banners for local problems. Blocking
popups detached from the triggering action are the worst place for errors.

### FBK-02 Define every state for every component
default, hover, focus-visible, active/pressed, disabled, loading, selected,
error, success, empty. A component spec without these is unfinished
(templates/component-spec.md). Disabled controls: no tooltip, explain why
elsewhere, or keep enabled and validate on submit (FBK-09).

### FBK-03 Loading
Buttons that start work keep their label, show an inline spinner, and block
duplicates (idempotency key). Spinners and skeletons use the delay/minimum
pattern in PSY-02. Skeleton mirrors final content. Over 10 s: real progress and
cancel. Offline and timeouts have a retry.

### FBK-04 Empty states are onboarding
Say what belongs here, why it is empty, and the one action to fill it
(optionally templates). Never a blank panel.

### FBK-05 Error messages: what, why, how to fix
Plain language, no codes alone, no blame, specific ("Postal code is too short:
use 6 characters"), next to the field, with a recovery path. Every screen has a
next step. Preserve user input on error. Heuristic 9 (S04).

### FBK-06 Optimistic UI: when yes, when no
Numeric preconditions (failure rate 1 to 3%, under 2 s): PAT-05.
Yes: likes, toggles, reorder, rename, add to list, mark as read. Apply
immediately, send in background, reconcile, roll back with inline error and
retry or Undo. No: payments, balances, irreversible sends, anything legally
significant, or anything where the user acts on the result at once. There show
"Pending", then confirmed. Persist the optimistic state across reload only if
the queue persists. POST/PATCH/DELETE should complete in under about 500 ms
for this to feel honest (S10).

### FBK-07 Undo, history, soft delete
Provide at least one undo level for destructive in-app edits; soft delete with
retention (30 days) for user content; toast with Undo for quick deletes.
Shared or collaborative data raises local-vs-global undo questions: decide and
document (S09 leaves this open).

### FBK-08 Toasts
Informational vs actionable toasts differ: PAT-03.
Short, specific, one action at most (Undo/View). Auto-dismiss only when no
action is required; pause on hover and focus; keyboard reachable; announced
politely; stack limit; position consistent. Never use a toast for an error
that needs fixing.

### FBK-09 Forms
- Label on every control, visible; click on label focuses the input.
- Right `type`, `inputmode`, `autocomplete` and meaningful `name`; spellcheck off
  for emails, codes, usernames.
- Do not block keystrokes; validate and explain instead. Do not pre-disable
  Submit; let users submit and see validation.
- Enter submits single-input forms; in textareas Cmd/Ctrl+Enter submits.
- On submit failure focus the first invalid field.
- Warn before leaving with unsaved changes (or autosave, INT-08).
- Allow paste and OTP autofill; ask only what you need; do not ask twice
  (A11Y-21).
- Placeholders show an example and end with an ellipsis; they never replace
  labels.
- Group long forms into steps; show progress and allow going back.

### FBK-10 State lives in the URL
Filters, tabs, pagination, expanded panels, selected item are deep-linkable so
share, refresh and Back/Forward work; restore scroll on Back (S11).

### FBK-11 Idle, stale and offline
Show last-updated time on cached data, queue writes offline with visible
pending state, never silently drop input.

### FBK-12 Copy tone for states
Sentence case, verbs on buttons ("Save changes", "Delete 3 files"), no
"Oops", no exclamation marks for errors, consistent terminology (A11Y-20).
