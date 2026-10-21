---
name: playbook-design
description: Design a UI screen, flow, component or interaction using the personal design-playbook (accessibility, interaction intent, motion, visual hierarchy, tokens). Use before building or redesigning any user interface, when asked how something should behave (confirm vs undo, loading, empty state, animation, focus), when planning a design-system component, or whenever the output must be accessible and non-generic. Produces a screen brief and component specs, not code.
---

# Design with the playbook

The playbook is the `playbook` CLI (build it from the repo with
`go build -o ~/.local/bin/playbook ./cmd/playbook`; the rules are embedded). Rules have stable ids (`A11Y-10`, `INT-06`);
cite ids in your output so choices are auditable.

## Look things up, do not recall them

```bash
playbook search "undo vs confirm delete"     # ranked sections
playbook rule A11Y-09                 # one rule, full text
playbook list                       # every rule id
```
Thresholds (contrast 4.5:1, targets 24/44 px, durations) must come from the
playbook or a fetched primary source, never from memory. A rule tagged GK or
listed in `sources/UNVERIFIED.md` is a default to test, not a requirement.

## Procedure

1. **Brief.** Copy `templates/screen-brief.md` and fill sections 1 to 3 from the
   request. If the job to be done or the primary action is unclear and cannot be
   inferred, ask one specific question; otherwise state your assumption.
2. **Point of view.** Pick three feel adjectives, one thing it must not feel like,
   one memorable commitment (type, space or motion). This is what prevents
   generic output (`principles/08-anti-patterns.md`, "AI slop" list). For
   aesthetic direction also use the `frontend-design` skill; this playbook
   supplies the rules that direction must obey.
3. **Intent and risk map** (brief section 6). For each action: frequency,
   reversibility, then choose wait / infer / act-and-undo / ask
   (`principles/01-interaction-intent.md`). Default: reversible means act and
   offer undo; irreversible and high blast radius means confirm with
   consequence.
4. **Structure.** Layout, hierarchy, one primary action (VIS-01..VIS-07). Reading
   order equals DOM order equals focus order (A11Y-08).
5. **States.** Every state of every component: default, hover, focus-visible,
   active, disabled, loading, error, success, empty, long content (FBK-02,
   VIS-14).
6. **Tokens first.** Reuse the product's existing tokens (web: the project's
   token files; native: its palette module). Propose new
   tokens by role name only when needed (SYS-02, SYS-03). Check any new color
   pair with `playbook contrast FG BG`.
7. **Accessibility plan** (brief section 8): keyboard map, names/roles, live
   announcements, target sizes, contrast pairs, reduced-motion variant,
   cognitive load. Use `checklists/a11y-audit.md` as the source of items.
8. **Motion.** Only where it serves spatial consistency, feedback or attention;
   none on high-frequency actions; tokens for duration and easing; reduced-motion
   variant (MOT-01..MOT-07).
9. **Deliver** a brief plus a `templates/component-spec.md` per new component,
   with rule ids next to decisions, and a "Risks and open questions" list.

## Rules of engagement

- Native elements first; custom widgets copy the WAI-ARIA APG contract (A11Y-25).
- No emoji or Unicode pictographs as icons: SVG with `currentColor` (house rule).
- Prefer real content over lorem ipsum, including the longest and the empty case.
- If the playbook has no answer, follow "Research protocol" in
  `sources/SOURCES.md`, prefer W3C/MDN/NN/g/platform guidelines, then record
  the finding as a new rule so the next agent gets it.
- State what you did not verify.

Hand off to `playbook-implement` to build and `playbook-audit` to review.
