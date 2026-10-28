---
name: papercraft-critique
description: Critique finished PaperCraft work the way a design team does - against the person's objective, through independent reviewers (the person's vision, UX heuristics, flow across screens, QA states, visual, accessibility, copy, an end user from the audience), each finding rated 0-4 and backed by tool evidence, then fixed, reported and, when the person approves, kept as a lesson. Use when finish_working_on_nodes offers a critique, when asked to review or critique a design, or before handing over a screen or flow.
---

# papercraft-critique

A critique judges a design against what it is for, not against taste. The tool `critique_design`
hands you the evidence and the reviewers; this skill is how to run them. Mechanics live in the
`papercraft` skill, taste in `papercraft-design`. Per-reviewer checklists:
[personas.md](references/personas.md).

## 1. When

* `finish_working_on_nodes` returns `next`: ask the person (`ask_user`) whether to critique now.
* The person asks "review this", "is it ready?", "critique it".
* Before handing over a whole screen or flow. Not for one component, an early draft or a copy tweak.

## 2. Objective first

Critique without an objective is opinion. Before judging, have:

* **Objective**: what the work must achieve ("a returning customer reorders in two taps").
* **Audience**: who uses it, in what context (phone, in a hurry, first visit, low vision).
* **Top tasks**: the two to five things people come to do.

Take them from the brief, the spec, the person's notes (`get_feedback`) and references. Missing?
Ask with `ask_user` and pass them to `critique_design {objective, audience}`.

## 3. The reviewers

| Persona | Asks | Method |
|---|---|---|
| `vision` | Does it achieve the objective, as the person imagined it? | Objective, element, effect, why (Connor and Irizarry) |
| `ux` | Can people use it without help? | Nielsen's 10 heuristics |
| `flow` | Do the screens make one coherent journey per task? | Task walkthrough across screens, 4 questions per step |
| `qa` | What breaks with real data and states? | State and edge-case matrix, pass/fail/n.a. |
| `visual` | Does the eye land right, does it hold together? | Hierarchy, balance, contrast, gestalt, rhythm, tokens |
| `accessibility` | Can everyone use it? | W3C POUR applied to a design |
| `copy` | Is every word clear and specific? | Plain language, labels, errors, empty states |
| `end-user` | Where would this user get stuck or give up? | Persona walkthrough of the top task |

`vision` always runs: the person's intent is the yardstick for the others. Pick the rest with
`personas` (default: all). The person's own words outrank any reviewer.

## 4. Independence

Several reviewers find far more than one, but only when they work apart (Nielsen: one
evaluator finds about a third of the problems, three to five most of them). Reviewers who
discuss or debate converge and amplify each other's bias.

* **With subagents**: one subagent per persona, given only its persona brief, the objective and
  the evidence. No reviewer sees another's findings before rating.
* **Alone**: one pass per persona, in order, writing that persona's findings before reading the
  next brief. Do not revise earlier personas' findings during later passes.
* **You coordinate**: you merge and judge; you never let reviewers argue a finding out.

## 5. Evidence

From `critique_design`: lint (fix every error and warning first), the `flow` graph (arrows between
screens, `deadEnds`, `unreached`), the lessons. Then `get_screenshot` each artboard at scale 1
(2 for small text) in every variable mode, `get_bounds` for overflow, `get_feedback` for the
person's notes. Every finding cites one of these: a tool result or a screenshot region. No
evidence, no finding.

## 6. A finding

`persona`, `heuristic or check`, `element` (layer name), `seen` (the observation), `effect` (on
the objective), `evidence`, `severity`, `suggestion`. Keep the observation and the suggestion
apart: critique is analysis, not brainstorming. Each persona also names at least one thing that
works and should stay.

Severity (Nielsen), from frequency, impact and persistence:

| | |
|---|---|
| 0 | not a problem |
| 1 | cosmetic: fix if time allows |
| 2 | minor: low priority |
| 3 | major: fix before handing over |
| 4 | catastrophe: blocks the task, fix now |

Every broken lesson (`lessons`) is a finding of at least 3.

## 7. Merge, fix, report

1. **Merge** into one list: join duplicates, count the personas that found each (two or more is
   confirmed), severity is the mean of their independent ratings.
2. **Fix** every 3 and 4 with the smallest edit (`update_styles`, `set_text_content`), then check
   that finding again with the same evidence. The work is ready when no 3 or 4 is open and lint is
   clean. Three fix rounds at most; then hand over with the open findings stated.
3. **Report** to the person: what works, then findings by severity, each with what was seen, the
   evidence, the suggestion and whether it was fixed. Phrase open ones as questions about the
   objective ("How does a first-time user find Reorder from here?"), not verdicts.

## 8. Lessons

Ask the person with `ask_user` which findings to remember, one question per finding: for every
project, for this project, or not. Save only what they approve, with
`design_lessons {add: {rule, why, scope}}`: the rule is one line written as an instruction
("Screens keep their device height; the main column scrolls"), `why` is what went wrong. Never
save a lesson the person did not approve, and never save praise. Lessons come back in
`get_basic_info` and in the next `critique_design`; `design_lessons {remove: "L3"}` drops one.

## Sources

Adam Connor and Aaron Irizarry, *Discussing Design* (objective, element, effect, rationale);
Nielsen Norman Group: 10 usability heuristics, severity ratings, cognitive walkthroughs, design
critiques, visual design principles; W3C WAI accessibility principles (POUR); work on panels of
independent LLM judges (debate amplifies bias, a coordinating judge resists it).
