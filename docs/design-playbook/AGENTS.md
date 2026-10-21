# Agent guide: design-playbook

You are in the design reference. Use it before designing, building or reviewing
any user interface, and cite rule ids.

## Route by task

| Task | Do this |
|---|---|
| Plan a screen, flow or component | Skill `playbook-design`, or read `templates/screen-brief.md` |
| Build or change UI code | Skill `playbook-implement`; read `platforms/web.md` or `platforms/native-egui.md` |
| Review or audit UI | Skill `playbook-audit`; `checklists/design-review.md`, `checklists/a11y-audit.md` |
| Answer a specific question | `python3 scripts/search.py "<question>"` or `--id <ID>` |
| CSS/animation/shadow detail | `principles/09-css-craft.md` |
| Build tooltip, toast, toggle | `principles/10-component-patterns.md` |
| Check colors | `python3 scripts/contrast.py FG BG` |
| Ship gate | `checklists/pre-ship.md` |

## Rules for using it

1. Look rules up; do not answer thresholds from memory. If a rule says GK or
   appears in `sources/UNVERIFIED.md`, call it a default, not a requirement.
2. Cite ids (`A11Y-10`, `INT-06`) in specs, reviews and commit messages.
3. If the playbook is silent, research with primary sources (protocol in
   `sources/SOURCES.md`), then add the rule and source here.
4. Report what you did not verify. A green automated check is not proof of
   accessibility or of good transitions.
5. House rule: no emoji (use SVG, `currentColor`).

## Editing this repo

- Rule headings `### ID Title`; ids unique and stable; every cited id must exist
  (`python3 -I -m unittest discover -s tests` enforces this).
- Keep videos as notes in our words; do not commit transcripts.
- Keep files plain Markdown and JSON; scripts use only the Python standard library.

## Remote access (no filesystem)

Fetch `llms.txt` from the raw file URL of the host serving this repo, then the
linked Markdown files.
