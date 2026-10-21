# Component spec: <Name>

Copy this file per component. Every section is required; write "n/a: reason"
instead of deleting one. Rule IDs from `principles/`.

## Purpose
One sentence. When to use it. When not to (name the alternative).

## Anatomy
Parts (container, label, icon, helper, error). Which are optional.

## Variants and sizes
| Variant | Use |
|---|---|
| primary | the one main action on a screen (VIS-07) |
| secondary | ... |
| destructive | ... |

Sizes: hit area >= 44 px on touch, >= 24 px always (A11Y-09). Visual size may be
smaller if the hit area is expanded.

## States (all required, FBK-02)
default, hover (pointer fine only), focus-visible, active/pressed, disabled,
loading, selected/checked, error, success. Show each in light and dark.

## Tokens consumed
List semantic tokens only (SYS-02): `color.accent.default`, `radius.control`,
`space.3`, `motion.duration.fast`, `motion.easing.out`. No raw values.

## Behavior
- Pointer: press arms, release inside commits, drag off cancels (INT-09).
- Keyboard map (A11Y-07): Tab/Shift-Tab, Enter, Space, Esc, arrows as relevant.
- Touch: target size, gesture alternatives (A11Y-18).
- Intent handling: any delays, inference, optimistic update (INT-01..INT-11).
- Undo/confirm policy for destructive paths (INT-06/07).

## Accessibility contract
- Native element used: ...
- Name (source of accessible name), role, states exposed (A11Y-14).
- Focus behavior: where focus goes on open/close/error (A11Y-26).
- Live announcements (A11Y-27).
- Contrast pairs added to `contrastPairs` (A11Y-01/02).
- Screen reader expectation: "Delete, button" / "Email, edit text, invalid, ..."

## Motion
Enter/exit, durations, easing from tokens, origin (MOT-05). Reduced-motion
variant (MOT-07). Why this animation exists, or "none: high frequency" (MOT-03).

## Content
Label length limits, tone, truncation rule, empty/long text behavior, i18n
expansion (FBK-12, VIS-14).

## Responsive
320 px, tablet, wide. What reflows, what hides, what becomes a sheet.

## Do / Don't
Two or three short examples each.

## Acceptance tests
- [ ] Keyboard-only flow
- [ ] Screen reader announcement matches the contract
- [ ] Contrast pairs pass for all themes
- [ ] Reduced-motion variant
- [ ] All states screenshot-tested
