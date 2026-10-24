# Interview redesign: two candidate types, SSO, deeper UX

## Why (user, 2026-10-23)

The cleanup made the Interview design consistent; now the product itself needs a deeper pass. Asked for:

* Two kinds of candidates: **external** (applicants from outside) and **internal** (Netcracker employees moving roles or being assessed).
* **Sign in with SSO**; remove the Google and Microsoft buttons.
* Look at every page, improve the complex parts for UX, recreate sections or pages where that is worth more than patching, following the design playbook (`/Volumes/Storage/Projects/design-playbook`, `playbook` CLI).
* Hand off to the Interview codebase (`/Volumes/Storage/Projects/interview`) so its agent starts on it.

## Product decisions (defaults; open questions are annotated "? (team)")

### Candidate types

| | External | Internal |
|---|---|---|
| Who | applicant, no company account | employee with a company account |
| Admin creates session with | name + personal email | pick from the employee directory (name, email, department, employee id) |
| Invite | single-use link, email OTP (today's flow) | link opens **Sign in with SSO**; no OTP; identity comes from the IdP |
| Join | name and email confirmed by OTP | "Signed in as name@netcracker.com" (read-only), wrong account → "Use a different account" |
| Shown to staff | `External` tag | `Internal` tag + department; results visible only to the hiring panel, never to the candidate's current manager (? team) |
| Re-use | one session per invite | same rules; past internal sessions listed on the person (? team) |

The type is chosen once, at session creation, and shown as a tag everywhere a candidate appears (sessions list, session detail, entry approval, live session, review). Filters on the sessions list include type.

### Sign in

* Staff (admin, interviewer) sign in with **company SSO** as the primary action; email and password stays as a secondary, admin-configurable fallback (? team: remove entirely?).
* Candidate web root: no generic "Sign in to your workspace" with social buttons. External candidates enter only through the invite link (spec `interview/specs/candidate-access`); internal candidates through the invite link then SSO. The root page explains that and offers "Continue with SSO" for internal candidates who lost the email.
* Google and Microsoft buttons, logos and their literal colours are removed from every screen.
* SSO states: redirecting, cancelled at the IdP, account not allowed (not in directory / disabled), session expired, IdP unavailable.

### Deeper UX (agents decide per section, with the playbook)

Complex parts to rethink rather than patch: session creation (type first, then the fields for that type), entry approval and live session (what the interviewer must decide, at a glance, under time pressure), challenge authoring (long forms: sectioning, validation, preview), the candidate challenge IDE (timer, autosave state, run and submit, what happens at 00:00), every empty, loading, error and permission state.

## Design deliverables

* Design System page: new components — `Button / SSO`, `Tag / Candidate type` (Internal, External), `Directory picker` (search + result row with avatar, name, email, department), `Identity chip` (signed-in account), `SSO status` states. Built on existing variables; lint clean.
* Admin and Candidate pages: updated or recreated sections; new SSO screens; every screen keeps the section layout (screens 640 apart, route label above each screen).
* Annotations: rules for the new flows with the "Interview flows" template, tidy per section.
* Reports per agent in `specs/interview-redesign/`.

## Acceptance

* No Google or Microsoft anywhere (`find_nodes` text and layer names).
* Every candidate appearance shows the type tag; session creation branches by type.
* Each new or changed screen: lint 0 errors on its section, built from DS instances and variables, states covered (empty, loading, error, permission).
* Annotations on the new flows; open questions listed.
* Handoff doc in `interview/specs/candidate-types-sso/` and the Interview agent asked to start.
