# AI-report: AI integration design (done)

Date 2026-10-24. Agent AI. Screenshots in `specs/interview-redesign/AI/`. Code implications: `/Volumes/Storage/Projects/interview/specs/candidate-types-sso/handoff.md`, section "AI integration".

## Real capabilities used (Interview repo)
`services/api/internal/ai` (Adapter, Registry, Stub, Anthropic), `ai-grading` spec, `review_criteria` (Problem solving 30, Code correctness 25, Communication 20, Technical depth 15, Speed 10), `AiAnalysisPanel` with the 30 s anchoring guard. Today the AI returns one overall analysis; per-criterion scores, evidence, confidence, decisions, authoring and live notes are new (listed in handoff).

## Playbook rules (AIX, `playbook rule AIX`)
AIX-01 suggest not decide: every score written only on a human action; final score is the reviewer's.
AIX-02 per-item Accept/Edit/Dismiss, Undo, accepted items quiet origin mark; Accept all only when all shown (test cases).
AIX-03 confidence as label (High/Medium/Low), evidence chips linking to code lines or answer spans.
AIX-04 low confidence marked (dashed, muted, "Noticed" wording, sorted lower).
AIX-05 disclosure: AI-suggested badge for staff, candidate notice and after-submit disclosure.
AIX-06 settings: per-feature switches, live notes off by default, payload preview before first send, model change notice.
AIX-07 per-item "Wrong because..." line on suggestions.
AIX-08 states: running, partial, failed, disabled by workspace, AI off, manual path always works.

## Design System page (64e1d4aaf56fe6a4), mains at x 1560.., y 14100..14800
AI badge / Suggested 64e1d4:11070, / Reviewed :11077, / Edited :11084. Confidence / High :11089, Medium :11095, Low :11101. Evidence row / Code :11107, / Answer :11123, / Test :11135. AI suggestion card / Default :11143, / Low confidence :11187, / Accepted :11227. AI state / Unavailable :11269, / Disabled by workspace :11281, / Running :11289. 10 Dev handoff annotations (dev, behaviour, a11y, data). Lint 0 errors 0 warnings on each. A stray empty frame from the freeze was deleted.

## Admin page (96e0de09bfd69d1e), new sections below the lowest, 640 gaps
* Section / Admin / AI review 96e0de:55740 (y 60032): 10 screens: AI hidden until you write, AI suggestions, Decided (final score yours), Audit trail drawer, AI running, AI partial, AI unavailable, AI disabled by workspace, Edit score, Live session / AI notes (own screen in this section; the Live Session section was not touched).
* Section / Admin / AI authoring 96e0de:58705 (y 67984): 6 screens: prompt, generating, preview diff, test cases, MCQ distractors, AI unavailable (manual form).
* Section / Admin / AI settings 96e0de:60413 (y 72808): 6 screens: settings, payload preview before first send, turn AI off confirm, test failed, model change confirm, read only for interviewer.
* 38 notes (Interview flows template), tidy run; unknowns marked "? (team)". Lint: 0 errors, 0 warnings on all three.

## Candidate page (943a1809e1077b81)
Section / Candidate / AI policy 943a18:38262 (y 33518): 6 screens: not allowed, allowed, acknowledge required (error), disclosure after submit, in-challenge toolbar reminder, "How AI is used" drawer. 7 notes. Lint 0/0.

## Notes and limits
* Cards on screens are built from DS atoms (badge, confidence, evidence, buttons, tags) rather than instances of the card mains, because card text differs per criterion.
* Sidebar is the DS `Sidebar / Admin` instance (Sessions, Challenges or Configuration active); the Admin QA agent's local sidebars are separate.
* Score Dots labels audited: all 31 instances in the AI sections checked against the DS mapping (1 Poor, 2 Below bar, 3 At bar, 4 Above bar, 5 Exceptional). Only two show a label (both score 4: Edit score screen, Your score and Problem solving row) and "Above bar" is correct for 4. Unscored rows (0) have no label. No override was needed; set_text_content would be used per instance if a score other than 4 is ever shown.
* Clipping fixed: screen heights were checked (header + content) for every AI Admin screen. Two exceeded 900 px: Review / AI partial (944) and Review / Edit score (918). Both now use height fit-content with min-height 900 on Screen and Main, and the sidebar stretches to full height. All other AI screens fit in 900 px; Candidate screens are well under. Screenshots re-taken.
* Annotation cards are not visible in get_screenshot (known); placement tidied by `annotate tidy`.
* Layer-name text overrides truncated at 50 chars; long labels were reset with set_text_content.

## Open questions (? team)
Confidence cut-offs; reason required when scoring 2+ points from the AI; how an AI-not-allowed policy is checked; whether candidates can ask for human-only review; live notes refresh interval and provider/retention/region; who owns the API key; per-criterion scale (0-5 assumed).
