# Design handoff: tasks

Defaults taken for the spec's open questions: a handoff point covers the whole project; `since` is passed per call; old points are kept until marked again under the same name.

## Done
- [x] Compact spec marks scrolling frames: `scroll+N` (N px past the bottom).
- [x] Handoff points: `mark_handoff`, `list_handoffs` (`src/handoffs.rs`, project folder `handoffs/`).
- [x] Change report: `get_handoff_changes` (`src/mcp/handoff/changes.rs`): tokens once, mains once with the screens using them, screens by kind with the action; derived sizes, auto-layout positions and instance fields that follow their main left out; 8 lines per screen, `nodeId` for the rest.
- [x] Bundle: `export_handoff` (`src/mcp/handoff/bundle.rs`): index, inventory, DTCG/CSS/Tailwind tokens, components, open questions (with the Handoff page table), compact spec and unrolled reference PNG per screen, changes; manifest with hash and tokens; unchanged files not rewritten, stale ones removed.

## Next
- [ ] Per-screen "implemented from" record, so each screen diffs from what was built for it.
- [ ] Canvas status badges: draft, ready, changed since handoff, implemented.
- [ ] Before/after images of each changed screen, cropped to the change.
- [ ] "What changed" deck from a change report.
- [ ] `verify_code` against a bundle on disk (no app).
- [ ] shipcraft `bin/pc-freeze.py` → one `export_handoff` call.
