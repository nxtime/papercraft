---
name: playbook-implement
description: Implement or modify UI code (web HTML/CSS/React/Tailwind or native Rust egui) so it follows the personal design-playbook - tokens, native elements, keyboard and focus, contrast, target sizes, reduced motion, all component states. Use when building a component or screen from a spec, restyling an existing one, fixing an accessibility or interaction defect, or adding motion. Pair with playbook-design for the spec and playbook-audit to review.
---

# Implement with the playbook

Playbook path: `$PLAYBOOK_DIR` (the root of your design-playbook clone). Look rules up instead of
recalling them:

```bash
cd "$PLAYBOOK_DIR"
python3 scripts/search.py "focus trap dialog"
python3 scripts/search.py --id MOT-07
```

## Before writing code

1. Read the repo's own conventions first (its design-system docs, tokens,
   existing components). Extend what exists; do not add a parallel component.
   Web: the project's component library and token files. Native egui: the
   project's design-system docs and shared UI crates.
2. Pick the platform file and read it: `platforms/web.md` or
   `platforms/native-egui.md`.
3. For CSS/animation detail read `principles/09-css-craft.md`; for tooltips, toasts, toggles and touch sizes read `principles/10-component-patterns.md`.
4. Have a spec (from `playbook-design`) or write a minimal one from
   `templates/component-spec.md`: states, keyboard map, tokens, motion.

## Build order

1. **Semantics and keyboard** with native elements: `button`, `a[href]`, `dialog`,
   `details`, `input` (A11Y-25). Custom widget only if no native element fits, and
   then copy the APG keyboard map exactly. Tab order from DOM order, never
   positive `tabindex` (A11Y-08).
2. **Tokens.** No raw hex, no off-scale spacing, radius or duration (SYS-01,
   VIS-04). Components reference semantic tokens (SYS-02).
3. **States.** Implement default, hover (pointer-fine only), focus-visible,
   active, disabled, loading, error, selected (FBK-02).
4. **Focus and targets.** Visible focus ring 2 px, offset 2 px, 3:1 contrast;
   scroll padding for sticky UI; hit area >= 24 px, 44 px on touch by expanding
   the hit area, not the visual (A11Y-09..A11Y-11).
5. **Intent.** Buttons commit on release; menu buttons open on press; destructive
   reversible actions use undo toast, not a modal (INT-06, INT-09).
6. **Feedback.** Loading delay/minimum, skeleton mirrors content, inline errors
   with `aria-describedby`, polite live region for toasts (FBK-03, FBK-05,
   A11Y-19, A11Y-27).
7. **Motion last.** `transform` and `opacity` only, tokens for duration and
   easing, interruptible transitions, no animation on high-frequency actions, and
   a reduced-motion path. For view transitions use `animation-duration: 1ms`,
   never `animation: none` (MOT-03, MOT-06, MOT-07).

## Verify before saying done

- Run the checks that exist: project tests, type check, lint.
- New or changed color pairs: `python3 scripts/contrast.py FG BG`, or add them to
  the product's `contrastPairs` and run `python3 scripts/contrast.py --tokens <files>`.
- Keyboard-only run of the changed flow. 200% zoom and 320 px width for web.
- Turn on the OS reduced-motion setting and re-run the flow.
- Actually look at the rendering (screenshot; filmstrip via CDP
  `Page.startScreencast` for transitions). Green automated checks have missed
  visible defects before.
- Walk `checklists/pre-ship.md`.
- Report exactly what was and was not verified.

## Do not

- Do not use emoji or Unicode pictographs as icons (SVG, `currentColor`).
- Do not disable zoom, block paste, remove focus outlines without replacement.
- Do not add dependencies for what the platform provides natively.
- Do not copy values from a design image into code: derive tokens, then use them.

If the task reveals a missing rule or a wrong one, fix the playbook in the same
sitting (`principles/`, a source row in `sources/SOURCES.md`, then
`python3 -I -m unittest discover -s tests`).
