# AL report: Admin Live Session, Challenges, new Sign in

Page Admin `96e0de09bfd69d1e`. Screenshots: `specs/interview-redesign/AL/`. Lint (my 3 sections): 0 errors; Sign in 0, Live Session 0, Challenges 2 touch-target warnings (old Templates Modal "Add file/Add test" buttons, accepted in B report).

## Offline note
The user's PaperCraft stopped mid-work. One call, the Challenges v2 `write_html` (node `96e0de:46700`), ran offline against the file. Everything was re-read after the app returned: all 3 sections, 46700, Live v2 and Sign in present and intact; all text overrides, moves, deletes, lint fixes, annotations and screenshots ran after the app was back and confirmed. Nothing else was lost as far as I can tell.

## 1. Sign in (new) `Section / Admin / Sign in` `96e0de:27174`, y 52080 (Entry Approval ended at 50480, +1600)
8 screens, 2 per row, route label above each: Sign in (SSO primary), Email Open (disclosure), Email Error, SSO Redirecting, SSO Cancelled, Session Expired, Not Allowed, IdP Unavailable.
- Button / SSO / Default, SSO status / Redirecting|Cancelled|Not allowed|IdP unavailable (texts re-worded for staff), Form Field-style fields from Input instances, Alert / Error|Warning. No Google/Microsoft (find_nodes: 0).
- Why: primary action single and above fold (`playbook` not queried per rule id, see open items); email fallback behind a disclosure; IdP down surfaces the fallback as recovery; error copy generic for credentials (no account enumeration).

## 2. Live Session `96e0de:19328`
New "Screens Live v2" `96e0de:34668` (10 screens): Live Code, Code Disconnected, Feed Error, Diagram, Whiteboard, Theory, Waiting, Time Up, Loading, End Session confirm.
Deleted old slots (recorded): Live Diagrams `96e0de:19330`, Live Whiteboard `19481`, Live Whiteboard Waiting `19590`, Live Code `19681`, Live Theory `19796`, Live Theory Waiting `19894`.
Changes: one fixed hierarchy: candidate + type tag (Internal shows department) + timer with bar, live status, Notes, End session; challenge strip (which of N, type tag, limits); work area (code, diagram, whiteboard or theory answers; changed lines highlighted, "read-only mirror, updated Ns ago"); right rail with tests (pass/fail with icon and word), candidate activity, private notes. Removed the global 4-tab bar: workspace tabs sit in the work area, dot = new activity. End session opens a consequence modal (workspace locks, drafts saved, goes to scoring, not undoable); at 00:00 the primary action becomes "End and score". Staff-side error (feed paused) and candidate-side (offline) are separate states.
Note: Sessions section (other agent) overlaps this section's y range in the file (Sessions height 17306 vs Live y 17336 is fine now, but it was 22248 mid-run); re-check.

## 3. Challenges `96e0de:15868`
New "Screens Challenges v2" `96e0de:46700` (17 screens + the old Templates Modal slot `96e0de:17345`, moved into the last row). Deleted old "Screens" `96e0de:15870` with its 8 rows and 15 slots (List, List Filtered, List With Archived, List Empty, List Loading, New Blank, New Template Applied, New Validation Errors, Edit Mcq, Edit Mcq Validation, Edit Theory, Edit Theory Empty, Edit Sql, Edit Fullstack, Edit Loading).
- List states: list, no match, archived (Restore), empty (Empty / Illustrated + CTAs), loading (skeleton rows), load error (retry), archive confirm (consequences).
- Authoring: vertical section stepper (state: todo, active, done, error with hint) + one expanded section at a time, others collapsed to a summary + Edit; sticky candidate preview; action bar with unsaved/saved text, error count, "Preview as candidate". Screens: New basics errors, New template applied (prompt editor), Edit Algo test cases (table editor, inline error, weights), Scoring weights (total must be 100), Full-stack files, MCQ questions (exactly one correct), Theory questions, SQL schema and expected result, Full preview as candidate, Edit loading, Edit archived (read-only).
- Type names: one set, matching the API enum ALGO/FULLSTACK/MCQ/SQL/THEORY, shown as Algo, Full-stack, MCQ, SQL, Theory (Tag colours blue, purple, cyan, gold, green as before). CODE and DIAGRAM exist nowhere in the Interview code as challenge types; they are workspace tabs (Code, Diagram, Whiteboard, Answers). The DS `Challenge Option / *` components (Sessions create) still carry a "Code" tag: the Sessions agent should relabel to Algo/Full-stack etc.

## Components and clutter
`import_components` put ~220 DS mains on the Admin page, parked as top-level frames at y=0, x>5824 (needed for instances; the other Admin agent also imports). Same names are skipped, no duplicates.

## Playbook ids
Playbook pass run afterwards, see the table below.

## Playbook pass (rule ids verified with `playbook rule <ID>`)
Searches run: sso sign in, disclosure secondary option, stepper long form, inline validation, destructive confirm undo, live dashboard at a glance, empty state, loading skeleton. No direct hit for "live dashboard at a glance"; closest are PSY-04 and A11Y-27, used below.

| Change | Rule ids | Verdict / fix |
|---|---|---|
| SSO primary, email and password in a disclosure; fallback shown as recovery when the IdP is down | PSY-04, PSY-05 | pass (one recommended default, rarely used option behind disclosure) |
| Sign-in password: paste and password managers allowed, SSO and email alternative to any test | A11Y-22, A11Y-21 | pass; no CAPTCHA, no re-asking identity (note for dev) |
| Sign-in placeholders were typed values | FBK-09 | fixed: example placeholders ending with an ellipsis (`name@netcracker.com…`), labels stay visible |
| Wrong credentials: one alert with what, count and recovery | FBK-05, A11Y-19 | fixed: "Email or password is incorrect. 2 attempts left. Try again or use SSO." Icon plus text, never colour only |
| SSO redirecting replaced the button | FBK-03 | fixed: Button / SSO / Loading keeps its label with spinner, status card below |
| Authoring stepper: sections, progress, back, summary, one open section | FBK-09, PSY-04 | pass; unsaved text in the action bar, leave-warning noted in annotations |
| Inline validation: field-level text next to field, section and footer counts, input preserved | FBK-05, A11Y-19, FBK-09 | pass; test-case error now says what to do ("Enter the expected output"); Submit never pre-disabled |
| Archive: was a confirm modal for a reversible action | INT-06, FBK-07 | fixed: no modal; row leaves, toast with Undo for 10 s; screen renamed `Challenges Archive Undo`, annotation updated |
| End session: irreversible, high blast radius | INT-07 | pass; modal names the consequence and the time left (38:12), button repeats the verb ("End session"), "Keep interviewing" as cancel |
| Empty list | FBK-04 | fixed: says what challenges are, why empty, actions (New challenge, templates) |
| Loading: list skeleton rows, edit form skeleton, live work-area skeleton | FBK-03, PSY-02 | fixed live loading to mirror code lines (was two generic blocks); list and edit already mirror layout; show-delay/min-time is a dev note |
| Every state drawn: empty, no match, loading, error, archived, offline, paused, waiting, time up | FBK-02 | pass |
| Live view: tag, timer, status always visible; stale and offline stated in text | PSY-04, A11Y-27 | pass; status changes need `aria-live="polite"` (dev note) |
| Colour contrast fixes | SYS-05 | done earlier (0 lint errors) |
| Copy: gendered pronouns about interviewer or candidate | n/a | checked all text in the 3 sections (find_nodes he/she/his/her/him): none; copy uses names or "they" |

## Open questions (annotated "? (team)")
Remove email/password sign-in entirely; lockout limit; offline threshold (60 s); per-challenge weight override vs rubric only; internal candidate notes visibility; Type labels in Sessions create.

## Annotations
Template "Interview flows": 8 on Sign in, 8 on Live Session, 9 on Challenges, tidied per section. Temp files in /tmp/al can be deleted.
