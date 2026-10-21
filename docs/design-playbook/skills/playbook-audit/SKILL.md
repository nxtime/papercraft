---
name: playbook-audit
description: Audit an existing UI, component library or screenshot against the personal design-playbook and report ranked findings with rule ids - accessibility (WCAG 2.2 AA), keyboard and focus, contrast, target sizes, interaction patterns (undo vs confirm), states, motion and reduced motion, visual hierarchy, tokens. Use for "review this UI", "is this accessible", "audit our component library", "what is wrong with this screen", or before shipping UI changes.
---

# Audit with the playbook

Use the `playbook` CLI (rules are embedded in the binary). This skill is for review;
use `playbook-design` to plan and `playbook-implement` to fix.

## Inputs to establish first

- Target: URL, running app, code path, screenshot, or component library.
- Scope: which flows and which themes (light, dark, high contrast).
- Method available: can you run it and drive it, or only read code/images? Say
  which, because it bounds what you can claim.

## Procedure

1. Choose checklists: screens and flows use `checklists/design-review.md`;
   accessibility depth uses `checklists/a11y-audit.md`; a library also needs the
   component contract in `principles/07-design-systems-tokens.md` (SYS-07).
2. Gather evidence, in this order of reliability: run it and use the keyboard;
   inspect the accessibility tree; measure contrast
   (`playbook contrast FG BG`, or `--tokens` on token files); read the
   code for semantics (div buttons, outline none, positive tabindex, transition
   all, hover-only); look at screenshots.
3. For web, an automated pass (axe, Lighthouse) is a floor; never report
   "accessible" from it alone.
4. Scan `principles/08-anti-patterns.md` and list every BAD-xx hit.
5. For each finding record: rule id, location, evidence, impact, fix, severity.
   - blocker: prevents a task or fails WCAG A/AA
   - major: hurts clarity, trust or efficiency
   - minor: polish
6. Keep what is good: list strengths worth preserving, so fixes do not regress them.

## Native Rust (egui) targets

Use `platforms/native-egui.md`. Check the accesskit feature in `Cargo.toml`,
focus ring drawing, interact sizes, a reduced-motion switch, and that each widget
has all states. Do not claim screen-reader support without running one.

## Output format

```
Verdict: ship / fix first / redesign
Scope and method: ...
Blockers:  [ID] where - evidence - fix
Majors:    [ID] where - evidence - fix
Minors:    [ID] where - evidence - fix
Strengths to keep: ...
Not verified: ... (what you could not test, and why)
```

Do not pad with generic advice. Every finding has a rule id and evidence; if you
cannot cite either, it is an opinion and should be labeled as one. If a finding
needs a rule the playbook lacks, say so and propose the rule.
