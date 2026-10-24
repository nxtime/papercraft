# Package A4 report: Candidate page clipping fixes

Date 2026-10-23. Project `Interview` (verified). Screenshots in `A4/` (before-*/after-*).

Root cause: instance labels kept the stale main-measured text width (43 px), so fit-content parents clipped. Re-asserting `width: fit-content` / `flexShrink: 0` via `update_styles` forces relayout.

1. `Send code` (3 Access screens: Invite, Invite/loading, Invite/error): button + label set fit-content (75 -> 103 px). No more "Send co".
2. Verify-error alert (Access Invite/error, `Alert / Error` instance in Alert Wrap): width 100% + alignSelf stretch (336 px, matches card content width). In the render it already measured 336 px, so it was already inside the card; now it follows the card width instead of a fixed size.
3. `Enter session` (Session home hero actions): Primary LG and Secondary LG (`Practice problem`) buttons + labels fit-content (83 -> 117 px; secondary also fixed, it was clipped the same way).

No spacing added, so no `bind_variable` needed.

Lint: Access section 0 errors, 8 info (Google logo colours, intentional). Session home section: see below, no new issues from these edits.
