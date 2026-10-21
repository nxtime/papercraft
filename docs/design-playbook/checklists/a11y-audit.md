# Accessibility audit (WCAG 2.2 AA plus house defaults)

Run in this order. Tools find a fraction of problems (principles/05); the manual
passes matter most. Record evidence (screenshot, command output) per failure.

## A. Automated floor (5 minutes)
- [ ] axe-core or Lighthouse on each key view in light and dark
- [ ] `python3 scripts/contrast.py --pairs <file>` on token pairs
- [ ] HTML validation for duplicate ids and invalid ARIA
Automated pass is not a pass.

## B. Keyboard (no mouse)
- [ ] Tab reaches every interactive element, in a logical order (A11Y-08)
- [ ] Nothing focusable is invisible; focus ring always visible (A11Y-10/11)
- [ ] Sticky header/footer never hides the focused element (SC 2.4.11)
- [ ] Enter/Space activate buttons; Enter follows links
- [ ] Esc closes dialogs, menus, popovers; focus returns to trigger (A11Y-26)
- [ ] Arrow keys work in tabs, menus, radios, listboxes per APG (A11Y-07)
- [ ] No keyboard trap (SC 2.1.2); skip link works (A11Y-15)
- [ ] Custom shortcuts do not clash with browser/OS or screen reader keys

## C. Screen reader (NVDA/Orca on Firefox or Chrome; VoiceOver on Safari)
- [ ] Page title and one `h1`; headings outline makes sense (A11Y-15)
- [ ] Landmarks: header, nav, main, footer
- [ ] Each control announced with name, role, state (A11Y-14)
- [ ] Form fields: label, hint, error announced; errors announced on submit (A11Y-19)
- [ ] Dynamic content (toasts, results) announced politely (A11Y-27)
- [ ] Images: meaningful alt or hidden; icons named
- [ ] Custom widgets match the APG role/keyboard contract (A11Y-25)

## D. Visual
- [ ] Text >= 4.5:1, large >= 3:1, UI/graphics >= 3:1, in every theme and state (A11Y-01/02)
- [ ] 200% zoom and 320 px reflow without loss or horizontal scroll (A11Y-04)
- [ ] Text spacing override (line 1.5, paragraph 2x, letter 0.12em, word 0.16em) causes no clipping
- [ ] Meaning never by color alone (A11Y-03)
- [ ] Forced-colors / high-contrast mode usable
- [ ] Prefers-contrast and color-scheme honored (A11Y-13)

## E. Pointer and touch
- [ ] Targets >= 24 px (AA), 44 px target on touch (A11Y-09)
- [ ] Dragging has a non-drag alternative (A11Y-12)
- [ ] Gestures have simple alternatives (A11Y-18)
- [ ] Hover content dismissible, hoverable, persistent (SC 1.4.13)
- [ ] Pointer cancel: commit on up, drop outside cancels (INT-09)

## F. Motion and time
- [ ] Reduced-motion tested with the OS setting (MOT-07)
- [ ] No flashing over 3/s; moving content over 5 s has pause (A11Y-16)
- [ ] No unannounced timeouts

## G. Content and cognition
- [ ] Plain language, literal labels, no double negatives (A11Y-23)
- [ ] Errors say how to fix (FBK-05)
- [ ] No redundant entry; accessible authentication (A11Y-21/22)
- [ ] Consistent navigation and help placement (A11Y-20)
- [ ] `lang` set; abbreviations explained where needed (A11Y-24)

## Report
```
Scope: ...   Tested with: browser/AT/versions
Fail   [SC x.y.z / A11Y-nn] location - impact - fix - severity
Pass   notable
Not tested: ... (state it; do not imply coverage)
```
