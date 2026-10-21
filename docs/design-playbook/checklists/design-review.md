# Design review checklist

Use on a screen, a flow or a component. Mark each item Pass, Fail (with rule ID
and fix) or N/A. Report failures ranked: blocker (breaks a task or WCAG A/AA),
major (hurts clarity/trust), minor (polish).

## 1. Purpose and hierarchy
- [ ] One primary action; its purpose is obvious in 3 seconds (VIS-07)
- [ ] Emphasis is earned: not everything bold/colored (VIS-01, PSY-08)
- [ ] Groups clear: gaps between groups > gaps within (VIS-03, PSY-06)
- [ ] Reading order matches visual order and DOM/focus order (A11Y-08)

## 2. Interaction and intent
- [ ] Reversible actions use undo, not confirmation (INT-06)
- [ ] Irreversible actions confirm with consequence and count (INT-07)
- [ ] Buttons commit on release; menus on press (INT-09)
- [ ] Ambiguity handled with the cheapest strategy (INT-01 to INT-05)
- [ ] Nested menus tolerate diagonal travel (INT-03)
- [ ] Autosave visible; no stray Save/Cancel (INT-08)

## 3. Feedback and states
- [ ] All states designed: default, hover, focus, active, disabled, loading, error, success, empty (FBK-02)
- [ ] Loading follows delay/minimum rule; skeleton mirrors content (PSY-02, FBK-03)
- [ ] Errors: specific, near the field, recoverable, input preserved (FBK-05)
- [ ] Optimistic only where wrong value is cheap (FBK-06)
- [ ] Empty/long/sparse/dense content tested (VIS-14)

## 4. Visual system
- [ ] Spacing, type, radius, shadows come from tokens (VIS-04, SYS-01)
- [ ] Type scale, line length, tabular numbers (VIS-06)
- [ ] Color roles and contrast verified numerically (VIS-08, A11Y-01/02)
- [ ] Concentric radii, layered shadows (VIS-11, VIS-12)
- [ ] Dark mode designed, not inverted (VIS-13)
- [ ] Icons: one family, SVG, currentColor, no emoji (VIS-15)

## 5. Motion
- [ ] Each animation has a purpose: spatial, feedback or attention (MOT intro)
- [ ] Durations/easing from tokens; under 200 to 300 ms (MOT-02)
- [ ] None on high-frequency or keyboard actions (MOT-03)
- [ ] Interruptible; compositor properties only (MOT-04, MOT-06)
- [ ] Reduced-motion variant exists and was tested (MOT-07)

## 6. Accessibility quick pass (full list: a11y-audit.md)
- [ ] Keyboard-only run completes the main task (A11Y-07)
- [ ] Focus visible and not obscured (A11Y-10)
- [ ] Targets >= 24 px, 44 px on touch (A11Y-09)
- [ ] No color-only meaning (A11Y-03)
- [ ] Labels and names present (A11Y-14, A11Y-19)

## 7. Heuristics sweep (Nielsen)
Status visible; real-world language; user control; consistency; error
prevention; recognition over recall; shortcuts; minimal; error recovery; help
(PSY-12).

## 8. Anti-patterns scan
Run through principles/08-anti-patterns.md; list every BAD-xx hit.

## Output format
```
Verdict: ship / fix first / redesign
Blockers: ID - what - fix
Majors:   ID - what - fix
Minors:   ID - what - fix
Strengths worth keeping: ...
Not verified: ... (say what you could not check)
```
