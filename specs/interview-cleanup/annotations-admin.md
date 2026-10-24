# Annotations: Admin page (template "Interview flows")

Date 2026-10-23. Project `Interview`, page Admin (`96e0de09bfd69d1e`). 50 notes, one `annotate` call per section, then `annotate {frame, tidy: true}`. Only annotations changed. `finish_working_on_nodes` done. All labels are 60 characters or fewer. Each note attaches to the exact element it governs (button, field, card or banner).

## Notes per section
**Sessions (13)**: New session is an Admin action (role), six statuses and Cancelled has no filter pill (data), status pills plus search (behaviour), list loading spinner, empty state, Title required, invite expiry blank = 7 days, create failed keeps form, invite code single-use (rule), expiry shown as ISO UTC (data), challenges change only on Save challenges, Open live vs Open review, detail load error with Retry.
**Live Session (9)**: read-only for admin (role), tabs mirror challenge types, Live badge, Waiting badge and empty hint, End & score closes session, template chips (question), Mermaid source (data), code pane mirror (data), theory answers keyed by question id (data).
**Entry Approval v3 (14)**: single-use link per candidate, challenge picker with total minutes (data), When: start on join or schedule, Create link result, invite message template (content), Admit result, Deny asks to confirm, Decline kills link (rule), waiting candidates at top of list, entry request data, activity timeline, Revoke access, re-entry request banner.
**Challenges (14)**: five types, scope Active/All, Archive, Type locked on edit, Time limit and Memory required, Language only for code types, test weight and sample, hidden files, starter templates, inline validation, MCQ one correct option, MCQ no correct warning, Theory needs 1+ question, live preview, empty/loading.

## Open questions for the team (marked "?" in the notes)
- Roles: can an Interviewer create sessions, admit, end or archive? The design shows only the ADMIN role.
- Session statuses: confirm order Draft, Open, In progress, Submitted, Graded; how Cancelled is reached; no filter pill for it. Session detail labels types CODE, DIAGRAM, MCQ while Challenges has Algo, Fullstack, MCQ, SQL, Theory: mapping?
- Entry: request time-out, camera or mic not ready when admitting, what the candidate sees after Decline or Revoke, session status after decline, link reuse after expiry or decline, re-entry limit or auto-admit window, whether work is kept on re-entry.
- Create: title max length, earliest expiry date, Schedule fields (date, time, timezone), minimum challenges selected, invite message editable, local vs UTC display.
- Live: refresh interval, confirm dialog on End & score, resulting status (Submitted or Graded), whether candidate loses access immediately, what the template chips do in a read-only view, invalid Mermaid state, list load error state.
- Challenges: where the per-challenge minutes (20, 25, 15) come from (form has only Time limit (s) and Memory), min and max for limits, allowed languages, MCQ option limits and points, sample test and visible file meaning (assumed visible to candidate), weight meaning (assumed score share), Archive confirm and restore, effect on sessions already using an archived challenge, whether Save is blocked by the MCQ warning.
- Inferred, not in design: timer starts on Admit when "Start when candidate joins"; challenge picker lists active challenges only; Rubric scores theory answers. Marked "confirm" in the notes.

## Design observations
- Sessions list design has a "Cancelled" row but no pill; the v3 Sessions list differs (Needs you block, 3 rows). Old Sessions flow (title and expiry) and v3 flow (candidate name, challenges, When) describe two different New session forms: team to pick one.
- Challenge Edit forms show Time limit 5 s and Memory 256 MB for MCQ, Theory and SQL where they probably do not apply.

## Placement
Could not verify visually: `get_screenshot` omits the annotation overlay (exports never include it), so `specs/interview-cleanup/annotations/admin-{sessions,live,entry-approval,challenges}.png` show the design only. Tidy ran on all four frames without error. Needs a look in the live app: cards beside targets, no card over a component, no crossing lines. Frame ids: Sessions `96e0de:14778`, Live Session `96e0de:19328`, Entry Approval `96e0de:23776`, Challenges `96e0de:15868`. The sections are far apart on the canvas and Challenges is 10786 high, so leader lines on long frames may be long. Several notes anchor to Text layers inside dense forms (for example `96e0de:17235`, `96e0de:18179`), which may be overlapped.
