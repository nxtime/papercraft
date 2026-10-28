# Pending snippet for skills/papercraft/SKILL.md

Paste these three lines into section 2 ("The loop: build, look, lint, fix"), after step 5, then delete this file.

```md
   Between edits, lint only what changed: note `rev` from each write result, then `lint_design {changedOnly: true}` (or `{since: rev}`); use `get_changes {since}` to see what an edit touched and `get_bounds` instead of looping `get_node_info`.
   Always finish with one full `lint_design` (no arguments, `partial: false`) and a screenshot per artboard in each theme mode before reporting.
   Details and an example: [incremental-checks.md](references/incremental-checks.md).
```
