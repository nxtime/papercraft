# Screen / flow brief: <Name>

Fill this before designing or coding a non-trivial screen. A short, specific
brief prevents generic output.

## 1. Job to be done
Who is the user, what are they trying to finish, how often, under what
conditions (one-handed, glare, rushed)? (Inclusive principle: consider situation.)

## 2. Primary action and success
The one primary action. What does done look like? Time budget for the task.

## 3. Content inventory
Every piece of data and action on the screen, ranked: must-see, should-see,
available on demand. Real content, including the longest realistic and the empty case.

## 4. Point of view (avoids templated output)
- Three adjectives for the feel, and one thing it must NOT feel like.
- One memorable choice (typographic, spatial, or motion) that the whole screen
  commits to.
- Reference screens studied and what is borrowed (Mobbin-style research:
  note which product and which pattern, not a copy).

## 5. Layout plan
Grid, regions, reading order, focus order (same list). Mobile-first behavior at
320 px. Where hierarchy comes from (size, weight, color, position).

## 6. Intent and risk map
| Action | Frequency | Reversible? | Strategy (wait / infer / act+undo / ask) | Rule |
|---|---|---|---|---|

## 7. States
Loading, empty, error, partial, offline, long content, permission denied.

## 8. Accessibility plan
Landmarks and headings; keyboard map; announcements; target sizes; contrast
pairs; reduced motion; cognitive load (steps, wording).

## 9. Tokens needed
New tokens required (propose names by role). Anything that is a one-off must be
justified or removed.

## 10. Verification plan
Which checks from `checklists/pre-ship.md` and how the real rendering will be
looked at.
