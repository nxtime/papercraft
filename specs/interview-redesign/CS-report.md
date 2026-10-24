# CS report: Candidate session and challenge (Candidate page 943a1809e1077b81)

Date 2026-10-23. Sections: "Section / Candidate Session" (943a18:16508) and "Section / Candidate Challenge" (943a18:8084). Both rebuilt from scratch (patching the old detached frames was worse). Screenshots: `specs/interview-redesign/CS/after/` (new), `CS/before/` (old).

Lint: 0 errors, 0 warnings except `touch-target` on 40px buttons (same as DS `Button / * / LG`) in both sections. `find_nodes` for google and microsoft: 0. `finish_working_on_nodes` done.

Note: the PaperCraft app was not running for the second half of the work (someone is rebuilding it). The CLI fell back to offline mode on the Interview project file and saved every call. Open the project and check the two sections; nothing was lost or reverted.

## Decisions (each with playbook ids)

| Change | Why | Rules |
|---|---|---|
| Candidate sidebar removed. Light top bar on session screens, dark focus header on every challenge. Both show Identity (avatar, name) and the Internal or External tag | One identity place; sidebar had 2 items and cost 248px of editor | PSY-04, VIS-07, spec (type tag everywhere) |
| Session home rebuilt: one primary action (Start challenge), session closing time, 1-of-4 progress, ordered challenge list with locked rows, setup strip | Answers "what is next, how far, how long" at a glance | VIS-07, PSY-04, PSY-08 |
| System check: Continue added (old screen had only Back), step 1 depends on type (SSO or email verified), camera-blocked fix steps and Retry, slow network warns and does not block, reassurance that no timer runs yet | Dead end removed, recoverable errors | FBK-05, FBK-02, A11Y-19 |
| Challenge header: Overview, title and "Challenge n of 4", Identity, Autosave status, Timer, Run tests, Submit. Timer is 48px tall, 20px bold, plus a 4px elapsed bar under the header | Timer and save state always in view, in every tab and type | INT-08, FBK-11, A11Y-16 |
| Autosave states Saving, Saved hh:mm, Offline (kept on device), Not saved with Retry; icon plus text, never colour alone | Current code swallows save errors (`.catch(() => {})`) | INT-08, FBK-11, VIS-10, A11Y-03 |
| Under 5 minutes: timer chip turns red, bar turns red, banner says "at 00:00 your work is submitted automatically" with Submit now | Old state only recoloured the number | A11Y-16, FBK-01, VIS-10 |
| Run tests (secondary, play icon, keeps label with spinner while running) vs Submit solution (the only primary). Rail checklist "Before you submit" | Run is unlimited and harmless, Submit is final | VIS-07, FBK-03, INT-12 |
| Submit confirmation: lists code saved, last test result, optional tabs, time left; repeats the verb; Keep working is the safe default. Theory and MCQ variants list unanswered questions | Irreversible action, summary instead of "Are you sure" | INT-07, A11Y-26, BAD-25 |
| MCQ validates on submit (error on the card plus summary) instead of disabling Submit | | FBK-09, FBK-05 |
| Test results: summary counts, segmented bar, failing test first with Expected and Received and file line, passes collapsed | Old console was one flat list of text lines | VIS-01, VIS-03, A11Y-03 |
| 00:00: dedicated screen with steps (draft saved, submitting, next challenge); offline variant keeps the draft, retries every 5 s, Copy my code | The code has no 00:00 behaviour at all today | A11Y-16, FBK-11, INT-11 |
| Offline and reconnect: amber banner (edits kept, Run and Submit pause, timer keeps running), green banner with change count on return | Never drop input silently | FBK-11, FBK-01, INT-11 |
| Diagram: syntax error keeps last valid preview and says so; whiteboard has selected state; theory has a question navigator with answered state | | FBK-05, FBK-02, PSY-07 |
| Submitted screens: per-challenge confirmation with Up next; final Submitted screen lists every challenge with status and what happens next; Review shows titles (not UUIDs), local time with zone, errored row with a next step | Old Review showed raw UUIDs and UTC | FBK-05, A11Y-23, FBK-12 |
| Empty, loading, error, permission states for home, review and challenge (not open yet, empty session, load error, locked challenge). Skeletons mirror final layout | | VIS-14, FBK-02, FBK-03, FBK-04, PSY-02 |
| Dark body keeps muted text at least 4.5:1: `ide/text/dim` on panels, `ide/text/muted` only on near-black | `playbook contrast` | A11Y-01 |

## Screens (new ids)

Session section, Rows 1 to 3 (4 per row, gap 640): Home `943a18:28641`, Home loading `28748`, empty `28808`, error `28841`, not open yet `28874`; System check ready `28907`, checking (External candidate) `28997`, needs action `29086`; Submitted `29182`; Review `29261`, empty `29324`, error `29361`.

Challenge section, Rows 1 to 9 (3 per row): Code idle `33687`, running tests `33911`, tests failed `34165`, tests passed `37973`, low time `34668`, confirm submit `34925`, time up submitting `35218`, time up offline `35498`, offline `35782`, back online `36038`, submitted `36295`; Diagrams `36335`, syntax error `36481`; Whiteboard `36634`, shape selected `36778`; Theory answered `36929`, last question `37030`, confirm submit `37135`; MCQ form `37275`, unanswered `37362`, confirm submit `37453`, submit error `37567`; Text answer writing `37921`, submitted `37661`; Loading `37733`, Load error `37835`, Locked `37878`.

External candidate (External tag) appears on System check checking and both Text answer screens; all others show Internal. All ids are `943a18:<n>`.

## Old screens deleted (recorded before deletion; all replaced)

Session: 16514 Home; 16650 Home/loading; 16698 Home/error; 16746 System check/ready; 16829 /checking; 16909 /failed; 16997 Review; 17095 Review/empty; 17162 Review/error.
Challenge: 8090 Code/idle; 8313 Code/running tests; 8504 Code/tests passed; 8733 Code/tests failed low time (split into "tests failed" and "low time"); 8962 Code/submitted; 9195 Diagrams/tab; 9338 Whiteboard/tab; 9473 Whiteboard/shape selected; 9613 Theory/answered; 9686 Theory/last question; 9759 Loading; 9798 Load error; 9831 MCQ form; 9898 MCQ form/submit error; 9970 Text form/status badges (split into Text answer writing and submitted).
Their old annotations went with them; 38 new ones replace them (17 session, 21 challenge) in the Interview flows template, tidied per section. Annotation overlay could not be seen in screenshots, so card placement is unchecked.

## Component use

Imported into the Candidate page from the DS: Identity-related `Tag / Candidate type / Internal|External`, `Avatar / 32`, `Button / Primary|Secondary|Ghost (SM, LG)`, `Tag / *`, `Alert / Warning|Error With Icon`, `Submission Status Badge / Queued`, `Button / Dark`, `Button / Text`. Variables only (no hex). `Identity chip` (360x88) is too big for a header, so the header uses Avatar + name + tag inline.

## DS requests (for the DS agent, not done here)

1. `Identity chip / Compact` (avatar, name, type tag) used in every header.
2. `Chip / Autosave` with 4 states; `Chip / Timer` normal and urgent; both are hand-built frames now.
3. `Button / Dark / Icon` (Run tests) and disabled variants of Primary LG.
4. `Modal / Confirm summary` (title, summary rows with status icon, 2 actions); `Banner / Offline|Online|Urgent` for the dark IDE.
5. `Test result row` and `Result bar`; `Question card` and `Option row` for MCQ.

## Code gaps found while reading apps/web (for the Interview agent)

* `session/[id]/page.tsx` redirects to the first challenge ("session home is gone"). With 2 or more challenges a home or overview is needed, so the design keeps it.
* `TechnicalSession` and `TheoryPhaseCandidate`: timer only recolours at under 5 or 3 min; nothing happens at 0; Submit has no confirmation; Run is simulated (hard-coded output).
* `challenge/[challengeId]/page.tsx`: every `saveArtifact` ends in `.catch(() => {})`, so there is no way to show Saved, Offline or Not saved. Artifacts are loaded per session, not per challenge (see whiteboard-diagram-rewrite).
* `done` page is not linked from submit (submit goes to `/review`).

## Open questions (annotated "? (team)")

Window clock running between challenges; challenges in fixed order or free; grace period after 00:00; autosave interval and draft restore; resubmit after Errored; minimum or maximum words for written answers; network below 5 Mbps block or warn; early entry before the start time; results visible to the current manager; reply time ("3 working days" is a placeholder); who sees the session closing time.

## Not drawn

Mobile or narrow layouts (editor needs 1280 or wider); practice problem (dropped from the home, was not in code); `/ended` page; Console and Terminal tabs (only Tests tab shown); keyboard focus states.
