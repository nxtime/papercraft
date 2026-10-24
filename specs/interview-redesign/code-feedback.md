# Code-side feedback for the redesign (from the Interview agent)

Date: 2026-10-23. Source of these decisions: the user, while the Interview code work started. Full list in `interview/specs/candidate-types-sso/handoff.md` ("Code-side answers").

## Please correct in the design

- **External candidates do not use email OTP.** The code removed OTP (PR #54). External join is: invite link → candidate enters their name → "Waiting for the interviewer" → interviewer admits or denies. In Candidate Access, the external invite should lead straight to Join v3 (name form), and the Verify / Verify error screens and the "Send code" step should go. The CA build skips them.
- **Internal candidates: SSO, then the same approval.** No auto-admit. The Join v3 waiting screens apply to internal candidates too (name and email read-only from SSO).
- Migration numbering and Google/Microsoft removal are already handled in code; nothing needed in the design.

## Answers to open questions marked "? (team)" that are already decided

- SSO protocol: generic OIDC (authorization code + PKCE); dev IdP is the homelab Logto.
- Matching an internal invite: by an `employee_id` claim when the IdP provides one, otherwise by verified email.
- Staff sign-in: SSO primary, email + password kept as a fallback an admin can turn off.

## Built so far from the design

- DS components (from DS-report.md): SSO button, candidate type tag, directory picker (plus a selected-row state and an error state the design lacked), identity chip, SSO status. In `packages/ui`.
- Tokens the design uses but the code lacked (`bg-surface`, `border-subtle`, tag blue/purple/gold/red, `brand-primary-strong`, brand hover) are being added to the code tokens.

## Answer to AS-report "Product conflict" (OTP vs join approval)

Decided by the user: **join approval wins.** The redesign spec's email code for externals is outdated. The admin build uses: external entry request = name as entered (not verified), joined via link; internal = "Verified by SSO". Summary card copy for external: the candidate joins with the link and enters their name, then you admit them. Please drop "Verified by email code" and the code steps from Sessions, Entry Approval and Candidate Access when you next touch them.

Built from AL/AS (in progress): Admin Sign in, Sessions (list, filter, new session, detail), Entry Approval; type tag only in the Live and Review headers (Live and Challenges redesigns are not in this build).
