# Interaction and intent

How software decides what the user meant, and what that costs. Source: V01
(Kole Jain, read in full), plus S09 (undo over warnings) and S11 (Vercel
guidelines).

Core model: every interaction guesses user intent. There are only four ways to
guess, and each pays in a different currency.

| Strategy | Pays in | Example |
|---|---|---|
| Wait | time | AirPod stem waits ~700 ms to tell pause from double/triple tap; single-click-opens vs double-click-edits waits for a second click |
| Infer | certainty | Submenu stays open while the pointer travels a "safe triangle"; tooltip warm-up; sidebar that opens on dwell |
| Act first, offer a way back | trust | Delete then Undo toast; Gmail send with 5 s undo; autosave plus version history |
| Ask the user how sure they are | effort | Release-to-commit buttons, pull to refresh, swipe to delete, Cmd-Tab until release |

Pick the cheapest strategy that is safe for the action's reversibility.

### INT-01 Do not delay a primary action unless two actions truly compete
A delay is only needed when the first input is ambiguous between two actions
(click vs double-click on the same target). Selecting a table cell highlights
instantly on the first click, so a double click adds no delay: the first click
is correct either way. Before adding a wait, ask whether the first action is
still right if a second input follows.

### INT-02 If you must wait, remove the ambiguity instead
The cleaner fix is usually to move the secondary action elsewhere (edit title
only in the detail page). Keep the secondary action in place only when its
convenience is worth the delay on the primary (V01: skipping a track without
the phone justifies the AirPod delay). Document the trade in the component spec.

### INT-03 Use intent inference where the pointer path already tells you
Nested menus must tolerate diagonal travel: draw an invisible triangle from the
pointer to the submenu edges and keep the submenu open while the pointer stays
inside it and moves fast enough. Too slow means the user is aiming at an item
in the current menu. Linear, Notion and Attio do this; it is the pattern
Vercel calls a "prediction cone" (S11). Origin: Ben Kamens, 2013 (menu-aim;
the original post returned 403 when fetched, so treat the attribution as
unverified, see sources/UNVERIFIED.md).

### INT-04 Warm-up once, then stay warm
First tooltip in a group waits; adjacent tooltips appear with no delay once one
has shown. Same for hover-opened sidebars: require dwell, and make the
threshold speed-aware so a pointer sweeping past does not trigger it.

### INT-05 Show your guess when the guess can be wrong
During drag-to-reorder, render where the item will land, and make auto-scroll a
gradient (faster the closer to the edge) instead of a binary edge zone. A
visible guess lets the user correct it before commit.

### INT-06 Prefer undo to confirmation for reversible actions
A confirmation modal for delete makes 99% of users confirm something they meant
to protect the 1% who did not, and habituation turns it into a reflex click
(S09: "Never use a warning when you mean undo"). Instead: perform the action,
show a toast with Undo (10 s or more is kind, and it must be reachable by
keyboard and announced politely), or move to "Recently deleted" with 30-day
retention.

### INT-07 Keep the confirmation for the truly irreversible or high-blast-radius
Sending bulk email to thousands, moving money, revoking shared access, deleting
data the system cannot restore. The confirmation names the consequence and the
count ("Send to 10,432 recipients"), and the destructive button repeats the
verb, not "OK". Rare plus costly means friction is acceptable (S09 allows
type-to-confirm only for rare actions).

### INT-08 Autosave replaces Save/Cancel; version history replaces Cancel
Settings and editors apply changes immediately. Toggles take effect with no
confirm (S11). The safety net is undo and history, not a dialog. Show saved
state ("Saved", "Saving...") so autosave is not a silent promise.

### INT-09 Commit on release, not on press, for ordinary buttons
Down-press arms the control, up-inside commits, drag-off cancels. That is the
built-in cancel every button has. Exception: controls that open a menu or
popover act on press so that press-drag-release selecting works in one motion.
Custom controls must reproduce both behaviors; do not fire on `pointerdown`
for plain buttons.

### INT-10 Offer progressive commitment where the gesture allows it
Pull to refresh, swipe to delete, hold-to-confirm and Cmd-Tab-until-release all
show proximity to commit and allow reversal (release early, Escape). Use for
gestures that are cheap to trigger by accident. Every gesture needs a plain
click or keyboard equivalent (A11Y-18).

### INT-11 Optimistic updates must reconcile
Apply the change at once, send it, and on failure roll back with feedback near
the failed item plus a retry, not a blocking popup detached from the action.
Do not use optimism where a wrong value is a trust failure (balances, payment
status, anything shown as final): show pending state there. See FBK-06.

### INT-12 Safe by default, fast on repeat
Frequent actions get shortcuts and no animation delay (MOT-03); rare
destructive actions get friction. Ask "how often per day" and "how reversible"
for every confirmation, delay and animation you add.
