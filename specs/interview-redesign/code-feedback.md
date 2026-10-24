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

## Applied in the design (2026-10-24)

Copy follows the built code (`apps/web/src/app/page.tsx`, `apps/admin/src/components/sessions/IdentityLine.tsx`).

Candidate page:
- Deleted Access / Invite external (943a18:21612, masked email + Send code), Verify (21722) and Verify error (21763). External invites now go straight to Join v3 / External.
- Join v3 / External: removed the "Email · Confirmed" field (20793). New note on the name field: typed by the candidate, not verified, interviewer admits.
- Root external steps: "Enter your name and check your camera", "Ask to join. Your interviewer lets you in". Session step "Email verified" is now "Joined with link".
- Section title: "Candidate / Access (invite only: external name + approval, internal SSO)".
- Notes: deleted 7 OTP notes (send code, 6 digits, wrong code x2, resend wait, masked email, email confirmed). Answered: IdP protocol (generic OIDC, auth code + PKCE, dev IdP Logto), invite matching (employee_id claim, else verified email).

Admin page:
- External identity everywhere: "Name as entered, not verified" + "Joined with the link. Check it matches who you invited before you admit.", neutral colour, no seal-check icon; "Matches the email you entered" lines removed. Internal: "Verified by SSO".
- Removed the "Email confirmed by code" activity step (7 screens). Type descriptions, Summary card steps, invite message, email hint and the Entry request route label no longer mention a code.
- Notes: identity method, identity at decision, invite template, internal match rule, external identity updated; staff sign-in note now says email + password is a fallback an admin can turn off.

Design System page: "invite/verify card" width label renamed "invite/join card".

Not changed: Access / Invite loading and Invite error still use the `/invite/[code]` route caption; the code uses `/join/[code]`. Lint: 0 errors, 0 warnings on every changed screen.
