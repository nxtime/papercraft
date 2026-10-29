# Design handoff and design updates

## Problem
Handing a PaperCraft design to code is a wrapper script today (`shipcraft/bin/pc-freeze.py`): one CLI call per screen (186 for Rota Fresca), the Handoff table parsed out of tree text, every file rewritten on each run, no picture of the design, and `verify_code` only against the running app. Worse, once code exists, a design edit has no handoff at all: nobody can tell which screens changed since the code was built, what kind of change it was (copy, a token, a component, the layout, a business rule), or what the code side has to do about it. Rota Fresca's `design-changes.md` shows the cost: nine design changes tracked by hand, each followed by "re-exported" of whole pages, and a stroke-colour question that took a manual check to settle. The same gap applies to any team that is not using shipcraft: there is no "what changed since you last built this" in PaperCraft.

## Goal
From any point the code was built from, PaperCraft says which screens and components changed, how, and what to do about each, in files a person or an agent can act on without opening the app.

## Non-Goals
- Generating or editing application code; PaperCraft describes, the code side implements.
- Git, issue-tracker or chat integrations; outputs are files and JSON others can feed into them.
- Merging two design branches; shared-project merge already exists.
- Replacing the presentation deck; it stays the owner-facing view.
- Pixel-diffing the running app against the design (shipcraft's `measure:route` keeps that job).

## User Stories
- As a developer, I want one command that writes the whole handoff bundle for a project (inventory, tokens, components, open questions, specs, reference images), so that I build from frozen files and never from the live app.
- As a designer, I want to mark a set of screens as handed off ("Ready for dev") at a named point, so that later edits are measured against what developers actually received.
- As a developer, I want a list of what changed since a handoff point, sorted by screen and by kind of change, each with the action it calls for, so that I update only what moved.
- As an agent building one screen, I want only the changed part of a screen's spec (the subtree and its path) plus before/after images, so that an update costs a few thousand tokens and not a whole-page rebuild.
- As a developer, I want to record which screens are implemented and from which handoff point, so that each screen's next diff starts from what was built for it, not from one global date.
- As a project owner, I want a "what changed" deck (before and after per changed screen, with the reason), so that I can approve an update without reading specs.
- As a team not using shipcraft, I want the bundle to be framework-neutral (DTCG tokens, CSS variables, Tailwind theme, SVG icons, named image assets, per-screen HTML reference), so that any stack can consume it.

## Acceptance Criteria

### Bundle
- [ ] Given a project, when the handoff export runs into a folder, then the folder holds the inventory, tokens (DTCG, CSS, Tailwind), components, open questions (including the Handoff table as structured rows), one compact spec per screen, one reference image per screen per mode, icons as SVG and images as named files, and a manifest listing every file with a content hash.
- [ ] Given a screen whose content scrolls, when it is exported, then its spec marks the scrolling frame and its full content height, and its reference image shows the whole content (as the deck does).
- [ ] Given an unchanged project, when the export runs twice, then the second run changes no file (byte-identical, same manifest).
- [ ] Given a renamed or deleted screen, when the export runs again, then the old files are removed and no stale file remains.
- [ ] Given the app is closed, when the export runs through the offline CLI on the same project, then the output is the same as with the app open.

### Handoff points
- [ ] Given selected screens or pages, when the designer marks them handed off under a name, then PaperCraft stores a baseline of those screens and the tokens and components they use, and the canvas shows each screen's status (draft, ready, changed since handoff, implemented).
- [ ] Given a screen marked ready, when anyone edits it, then its status becomes "changed since handoff" without any extra step.
- [ ] Given the code side reports a screen implemented from a named handoff point (and optionally a commit), when the next change report runs, then that screen is compared against that point.

### Change report
- [ ] Given a handoff point and later edits, when the change report runs, then it lists added, removed and renamed screens, and for each changed screen the changed nodes matched by id (not by position), each with the old and new value.
- [ ] Given a change, then it is classified as one of: copy, token value, token binding, style, layout, structure (nodes added, removed or moved), component main, instance swap, asset, annotation (behaviour or business rule), or screen added, removed or renamed.
- [ ] Given each class, then the report states the action it calls for. For example: copy → update the string key; token value → regenerate the theme, no screen work; component main → rebuild that component, and list the screens that use it with no screen work; layout or structure → rebuild that subtree; annotation → review the rule in the domain model; screen removed → remove the route.
- [ ] Given a token value or a component main changed, then the report lists every affected screen once, under that token or component, and does not repeat the change on each screen.
- [ ] Given a changed subtree, then the report includes that subtree's compact spec and its path from the screen root, and before and after images of the screen with the changed area outlined.
- [ ] Given a change report, then it is written as Markdown for people and JSON for scripts and agents, from the same data.
- [ ] Given a move or resize under 1 px, or a re-save with no content change, then the report lists nothing.

### Token cost
- [ ] Given a change report, then its first section is an index of one line per affected screen or component (name, change classes, action, file to read), and a reader reads detail files only for the items it acts on.
- [ ] Given a token or component change, then it appears once, as a single line naming the token or component, and is not repeated on each screen that uses it.
- [ ] Given a changed subtree, then its spec fragment holds only the changed nodes and the path from the screen root (names only, no styles). Unchanged siblings appear as one line each, or are left out.
- [ ] Given a change, then each value is written as `field: old → new`, using token names rather than resolved values. Values that did not change are not printed.
- [ ] Given a change that needs no screen work (token value, asset swap, annotation), then the report says so and points at no screen spec.
- [ ] Given the bundle, then every compact spec states its size in tokens in the manifest, so a brief can budget before it reads.
- [ ] Given before and after images, then they are cropped to the changed area plus a margin and kept at scale 1, so they are only fetched when needed.
- [ ] Given a one-node copy change on one screen, then the whole report for it (index line plus detail) is under 300 tokens.

### Verification and review
- [ ] Given a bundle on disk and source files, when verification runs against the bundle, then it gives the same verdict as `verify_code` against the live app, without the app running.
- [ ] Given a change report, when the "what changed" deck is exported, then it holds one slide per changed screen with before and after, the change classes, and the designer's note for the change, in the project's language.

## Constraints
- Stack: Rust (PaperCraft core, MCP tools, CLI); outputs are plain files (JSON, Markdown, PNG, SVG, CSS).
- Performance: a full export of 200 screens finishes in under a minute on the user's machine. A change report for one edited screen costs the reader under 2k tokens; the index for a 50-change update is under 3k.
- Token economy is a requirement, not polish: every output is read by agents billed per token. Fewer, smaller files read on demand beat one complete file.
- Data safety: baselines are stored in the project and migrate with it; exports write atomically; nothing is written outside the chosen folder.
- Compat: works offline (CLI) and online (app); the bundle format is versioned so a reader can refuse an unknown version.
- Tool count: each new MCP tool needs a test in `tests/mcp.rs`, and the README and ROADMAP counts updated.

## Open Questions
- Should a handoff point cover the whole project or only the screens selected? Per-screen baselines are more precise but heavier to store.
- How should "implemented" be reported back: an MCP tool the code side calls, a file in the bundle the export reads (for example `implemented.json` committed in the app repo), or both?
- Which counts as the action for a copy change in a team without a string table: edit the text in place, or always the string key?
- Should the canvas status badges ("changed since handoff") show to every viewer, or only in a "dev" view?
- Is a static handoff viewer (one HTML file in the bundle that browses screens, specs, values and changes) wanted, or are the Markdown and images enough?
- What retention applies to old baselines (keep all, keep the last N, keep the named ones)?
