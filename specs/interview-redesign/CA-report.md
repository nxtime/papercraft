# CA report: Candidate Access + Join v3 (page Candidate 943a1809e1077b81)

Screenshots: specs/interview-redesign/CA/ (a-*.png Access, je/ji/w1..w5 Join, sec-*-after overviews). Lint 0 errors, 0 warnings on both sections. find_nodes "Google"/"Microsoft"/"Sign in to your workspace": 0 hits; no layer names with them in either section.

## Section / Candidate Access (943a18:16180), 14 screens in 4 rows (Row 1..4)
Row 1: Root `/` (new, 943a18:21545), Invite external (21614), Invite loading (21666), Invite error link invalid/expired (21695).
Row 2: Verify (21724), Verify error (21765), Invite internal (21810), SSO redirecting (21860).
Row 3: SSO checking invitation (21883), SSO cancelled (21905), SSO not allowed (21927), SSO expired (21949).
Row 4: SSO IdP unavailable (21972), SSO no open invitation from root (21994).
- Root: no sign-in form, no social buttons, no divider. Explains invite-only entry, two path cards (External tag with steps; Internal tag with steps + `Button / SSO / Default`).
- External: invite shows masked email (no typing), Send code; 6 separate digit boxes, auto-submit on 6th, attempts left, resend wait, "Send a new code".
- Internal: Continue with SSO, no code step. SSO status components used for redirecting, checking (text overrides), cancelled, not allowed (also "no open interview" via text override), expired, IdP unavailable.
- Deleted (recreated): Login 943a18:16184 "Block / Access / Login" (Google/Microsoft), Login error 16295, Invite 16411, Invite loading 16429, Invite error 16448, Verify 16469, Verify error 16487 (rows 16183, 16447).

## Section / Candidate Join v3 (943a18:20014), 12 screens in 4 rows
Row 1: Join external (20754), Join internal with Identity chip (20819), Join loading (new, in Row 1).
Row 2: Waiting, Waiting no response, Waiting camera blocked (rethought).
Row 3: Approved (rebuilt), Done, Declined (kept, cloned).
Row 4: Link used, Link expired (kept, cloned), Connection lost (new).
- Waiting rethought: headline says what is happening; "stay on this page, it updates by itself"; 3-step status (sent, Elena reviews, you start) with expected time; one-entry rule; Start disabled with reason; Leave; right card "Check your setup" with pass/fail chip, mic test hint, browser row. 5-min state adds Notify again + rate note. Camera blocked adds numbered fix steps + Retry camera, and says Elena can still admit. Connection lost keeps the request alive, Retry now.
- Deleted (recreated): Block/Join Invite 20017, Waiting 20066, Waiting no response 20132, Waiting camera blocked 20192, and old copies of Approved 20256, Done 20319, Declined 20355, Link used 20381, Link expired 20407 (rows 20016, 20191, 20354; replaced by clones/new).
- Candidate types: External/Internal tag on both invite screens; Internal shows "Internal · Platform Eng".

## Annotations ("Interview flows"): 17 on Access, 16 on Join, tidy run. 15 detached notes from deleted screens removed and the still-valid rules re-added.

## Playbook check (verified: `playbook search` + `playbook rule`, `checklists/a11y-audit.md`)
| Change | Rules |
|---|---|
| Code step: visible "6-digit code" label, paste allowed, one input with one-time-code autofill, Verify never pre-disabled (fixed in this pass: was a disabled button) | FBK-09, A11Y-19, A11Y-22 |
| Wrong code: inline text + icon, input kept, "Send a new code", focus to first digit + polite announce (annotated) | FBK-05, A11Y-19, VIS-10 |
| Masked prefilled email, confirmed email read only, name prefilled, no re-asking (internal: identity from SSO) | A11Y-21, A11Y-22 |
| Waiting: status headline, 3-step progress, expected time, reason shown inline next to the disabled Start (no tooltip), polling every 3 s, announced via live region (annotated) | PSY-12 (1 status visibility), FBK-02, FBK-03, PSY-01, A11Y-27 |
| Notify again, Retry now, Retry camera, Email Elena, Use a different account: every error/stall screen has a next step | FBK-05, PSY-12 (9) |
| Skeleton loading screens mirror the final layout | FBK-03, PSY-02 |
| Buttons 44 px; "Use a different account" link raised to 24 px; chips are non-interactive | A11Y-09, BAD-10 |
| Icons paired with text, error never colour-only | A11Y-03, VIS-10 |

a11y-audit on the Join waiting and code screens (design-level only, no AT run): B (keyboard order follows reading order, A11Y-08), D contrast and colour, E target size, G plain language, errors with fixes pass in the design. Not testable in a design file: focus ring, screen reader output, 200% zoom, forced colors; they are written as annotations for the build.
Copy: no pronouns for the interviewer anywhere ("when Elena lets you in", "Notify Elena again", "Ask Elena for a new link"); texts checked in all 26 screens.

## Open questions (? team)
- Masked email enough or retype? Max attempts, code lifetime, resend cooldown.
- SSO: protocol, claims, match on email vs employee id, window length, IdP timeout, two open invitations on one account.
- "Usually within 2 minutes" and "Elena may be in another interview" are placeholder copy; no measured answer time.
- Server hold time for a request while candidate offline; link expiry 30 min (inferred).
- Brand shows "Hireloop" in the header (kept from before).
- Not drawn: mobile layout, internal-candidate Waiting variant (same as external).
- SSO status cards sit alone on the page; consider an invite summary beside them.
