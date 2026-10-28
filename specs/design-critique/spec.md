# Design critique and lessons

## Problem
When an agent finishes a design it says it is done; nobody reviews the work unless the person asks,
and the `papercraft-critique` skill only reaches agents that load skills (not the in-app chat). The
same mistakes come back in the next design (a page that grows instead of scrolling, gendered copy,
clipped labels) because nothing the person corrected is remembered.

## Goal
After a design is finished the person is offered a critique; the corrections they approve are
remembered and shown to every agent before its next design, so the same mistake is not repeated.

## Non-Goals
- Scoring by a model inside the app: the agent does the judging, the tool gathers the evidence.
- Saving anything the person did not approve.
- A settings screen for lessons (the tool lists and removes them).
- Syncing lessons between machines.

## User Stories
- As a designer, I want to be offered a critique when an agent finishes, so that I don't have to remember to ask.
- As a designer, I want to choose which critique findings become lessons, so that only what I agree with sticks.
- As a designer, I want agents to follow my lessons in later designs, so that I don't correct the same thing twice.
- As a designer, I want lessons for one product to stay in that product, so that a brand rule does not leak elsewhere.
- As a designer, I want to remove a lesson, so that a rule I no longer want stops applying.

## Acceptance Criteria
- [ ] Given edits since the last critique, when an agent calls `finish_working_on_nodes`, then the result tells it to offer the person a critique (`ask_user`), and it does not after a critique with no edits since.
- [ ] Given a page or artboard, when `critique_design` runs, then it returns the objective (or that it is missing and must be asked), the chosen reviewers (vision, ux, flow, qa, visual, accessibility, copy, end-user) with their methods, the finding format and 0-4 severity scale, the lint, the flow between screens and the lessons that apply.
- [ ] Given screens joined by arrows, when `critique_design` runs, then the flow lists the links, the dead ends (reached, leading nowhere) and the screens nothing leads to or from.
- [ ] Given several reviewers, the steps have them review independently (one subagent each, or one pass each) and a coordinator merge, never debate.
- [ ] Given the person approved a finding, when `design_lessons` adds it, then it is stored in the project store and listed by `get_basic_info` and `critique_design` from then on, in every session.
- [ ] Given a lesson saved with scope `project`, when another project is open, then it is not listed.
- [ ] Given a lesson id, when `design_lessons` removes it, then it is no longer listed.
- [ ] The tool descriptions say a lesson is saved only after the person approved it.

## Constraints
- Stack: Rust, MCP tools in `src/mcp`; lessons in a JSON file at the project store root, written atomically.
- Compat: missing or unreadable lessons file means no lessons, never an error for other tools.
- Security: lessons are plain text the person approved; no secrets.

## Open Questions
- None.
