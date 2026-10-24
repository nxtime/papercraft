# Annotations: Candidate page (template "Interview flows")

Date 2026-10-23. Project `Interview`, page Candidate (`943a1809e1077b81`). 38 notes, one `annotate` call per section, then `annotate {frame, tidy: true}`. Only annotations changed. `finish_working_on_nodes` done.

## Notes per section
**Access (6)**: invite link validity (rule), Send code (behaviour), unknown/expired invite error (behaviour), 6-digit verify input (data), wrong code (behaviour), password sign-in off (behaviour).
**Join v3 (9)**: single-use link, recording consent, Request to join, 5 min no-response + notify again, one entry / re-approval, camera must work, declined (no retry), link expiry (30 min after start, inferred), "3 days" copy.
**Session (10)**: candidate nav (role), Enter session, session not found, supported browsers, camera/mic required, network 5 Mbps, Exit session, submission status only (role), runner 30 s limit, Submitted at UTC.
**Challenge (10)**: 45 min limit, low-time timer (<5 min), autosave (question), copy/paste limits (question), Run unlimited, Submit, diagram/whiteboard saved (question), theory word count, unanswered theory skipped, submission status lifecycle.

## Open questions for the team (not in design)
- Invite code length, invite expiry, email-send resend limit and cooldown, max verify attempts, verify code lifetime.
- Join name max length; max "Notify again" count; is the 30 min link window (10:00 start, 10:30 expiry) the real rule; is "3 days" configurable.
- Re-entry after Exit session; session availability window; does a slow network (<5 Mbps) block or only warn.
- Autosave interval and draft restore; what happens at 00:00 (auto-submit or lock); resubmit after submit; are diagrams/whiteboard part of the submission; copy/paste limits; theory min/max words.
- Whether "Submitted at" is shown local or UTC to the candidate.

## Placement
Could not verify visually: `get_screenshot` omits the annotation overlay (exports never include it), so screenshots in `specs/interview-cleanup/annotations/candidate-{access,session,join,challenge}.png` show the design only and do not prove card placement. Tidy ran on all four frames without error. Needs a look in the live app: cards beside targets, no card over a component, no crossing lines. Notes anchor to Text layers (labels, headings) inside components, so lines are short.
Note: get_annotations lists `Label`/`Title` layer names, not note targets by id; note 4 in Challenge is anchored to the first code line (`import { useEffect, ...`), which may be overlapped by the code pane.
