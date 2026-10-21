# Accessibility

Target: WCAG 2.2 Level AA, plus the AAA items listed here as house defaults.
Sources: S01 (W3C new in 2.2), S02 (contrast), S03 (target size), S07 (APG
"read me first"), S08 (COGA), S16 (inclusive design principles), V04 (Chrome
A11ycasts focus), V06 (Hawkins on accessible design systems), S11 (Vercel).
WCAG 2.2 is backward compatible: meeting it also meets 2.0 and 2.1. SC 4.1.1
Parsing was removed in 2.2.

Automated tools catch only a fraction of defects (one vendor cites roughly 57%
from a 2024 Deque study: treat as indicative). Always add keyboard and screen
reader passes (checklists/a11y-audit.md).

## Perceivable

### A11Y-01 Text contrast >= 4.5:1; large text >= 3:1 (SC 1.4.3, AA)
Large = 18 pt (24 px) or 14 pt bold (about 18.5 px). No rounding: 4.499 fails.
Hover and focus text states count. Brand guidelines are not an exemption;
logotypes and inactive controls are. Compute with `scripts/contrast.py`.

### A11Y-02 Non-text contrast >= 3:1 (SC 1.4.11, AA)
Input borders, icon-only controls, focus indicators, chart marks against their
adjacent colors. A pale gray input border on white usually fails.

### A11Y-03 Do not rely on color alone (SC 1.4.1, A)
Add text, icon, shape or thickness (VIS-10).

### A11Y-04 Reflow and zoom (SC 1.4.4, 1.4.10, AA)
Content works at 200% text zoom and at 320 CSS px width without two-dimensional
scrolling. Never disable browser zoom. Use `rem`, flex/grid, no fixed heights
on text containers.

### A11Y-05 Text alternatives (SC 1.1.1, A)
Informative images get meaningful `alt`; decorative get `alt=""` or
`aria-hidden`; icon-only buttons get an accessible name (A11Y-14).

### A11Y-06 Captions and transcripts for media (SC 1.2.x)
Caption speech and meaningful sound; transcripts for audio-only; essential
visuals described; media controls keyboard-operable.

## Operable

### A11Y-07 Everything works with a keyboard (SC 2.1.1, A) and no trap (2.1.2)
Tab/Shift-Tab to move, Enter/Space to activate, Esc to dismiss, arrows inside
composite widgets (menus, tabs, radios, listboxes) per the APG patterns.
Native elements get this free: `button`, `a[href]`, `input`, `select`.

### A11Y-08 Logical focus order, from DOM order (SC 2.4.3)
Tab order follows DOM order (V04). CSS float, absolute positioning, flex
`order` and grid placement can desync visual and tab order: fix by moving the
element in the DOM, not by positive `tabindex`. Never use `tabindex` > 0. Only
interactive elements are tab stops; do not add `tabindex="0"` to static text.
Test by tabbing the whole page.

### A11Y-09 Target size: 24x24 CSS px minimum (SC 2.5.8, AA); 44x44 is the house default
2.5.8 passes smaller targets only if a 24 px circle around each does not
overlap another target, or an equivalent control exists, or the target is inline
in text, user-agent default, or essential. 2.5.5 (AAA) asks 44x44, matching
Apple's 44 pt and Vercel's mobile guidance: use 44 on touch, expand the hit
area with padding or a pseudo-element without enlarging the visual. 44 px
figure comes from 2.5.5 and platform guides, not from the S03 page. Measured
values by screen position (edges need more): PAT-06.

### A11Y-10 Visible, unobscured focus (SC 2.4.7 AA, 2.4.11 AA new in 2.2)
Every focusable element shows a visible indicator. Sticky headers, cookie bars
and chat widgets must not fully hide the focused element: add
`scroll-padding-top`/`scroll-margin` equal to the sticky height. Use
`:focus-visible` (not `:focus` alone) and style it.

### A11Y-11 Focus indicator quality (SC 2.4.13, AAA, house default)
At least a 2 CSS px perimeter and 3:1 contrast between focused and unfocused.
A 2 px outline with 2 px offset, double ring (inner light, outer dark) so it
works on any background. Prefer `outline`; if using `box-shadow` for radius
reasons, add a transparent outline so forced-colors mode still shows a ring.

### A11Y-12 Dragging has an alternative (SC 2.5.7, AA, new in 2.2)
Reorder, sliders, resize, map pan must work with single taps/clicks or buttons
(move up/down, +/-, numeric input) unless dragging is essential.

### A11Y-13 Respect user settings
Honor `prefers-reduced-motion` (MOT-07), `prefers-color-scheme`,
`prefers-contrast`, `forced-colors`, font-size and zoom, system language.
Do not use `px` for font sizes in a way that blocks scaling.

### A11Y-14 Accessible name, role, value (SC 4.1.2, A)
Every control has a name that includes its visible label text (SC 2.5.3 Label
in Name). Icon-only: `aria-label` or visually hidden text. Decorative SVG:
`aria-hidden="true"`. State is exposed (`aria-expanded`, `aria-pressed`,
`aria-current`, `aria-invalid`).

### A11Y-15 Skip link, landmarks, headings (SC 2.4.1, 1.3.1, 2.4.6)
"Skip to content" first in the tab order; one `main`; hierarchical headings,
no level skipping for styling; accurate `<title>` per view; anchor targets have
`scroll-margin-top`.

### A11Y-16 Time and motion (SC 2.2.1, 2.2.2, 2.3.1)
No hard session timeouts without warning and extension; pause/stop for moving
content; no flashing more than three times per second.

### A11Y-17 Links are links, buttons are buttons
Navigation uses `<a href>` so Cmd/Ctrl-click, middle-click and right-click work.
Actions use `<button>`. A link styled like a button stays a link, document it
(V06). Never `div onClick`.

### A11Y-18 Gesture alternatives
Swipe, pinch, long-press, path gestures have tap/click or keyboard equivalents
(S11). Multi-finger and motion (shake) gestures are never the only way.

## Understandable

### A11Y-19 Labels, instructions, errors (SC 3.3.1, 3.3.2, 3.3.3, A/AA)
Visible label per field (placeholder is not a label). Errors in text, next to the
field, programmatically associated (`aria-describedby`), say what is wrong and
how to fix it, and are not color-only (V06 postal-code example). On submit, move
focus to the first error and announce with a polite live region.

### A11Y-20 Consistent navigation and help (SC 3.2.3, 3.2.4, 3.2.6 new, A/AA)
Repeated navigation and identification stay in the same order and wording.
Help (contact, chat, support link) keeps the same relative place across pages.

### A11Y-21 No redundant entry (SC 3.3.7, A, new in 2.2)
Do not ask for information already given in the same process; prefill or let
the user select it.

### A11Y-22 Accessible authentication (SC 3.3.8, AA, new in 2.2)
No cognitive function test (memorize, transcribe, solve) unless an alternative
exists. Allow paste in password and OTP fields and password managers; support
passkeys and email links; CAPTCHAs need a non-cognitive alternative.

### A11Y-23 Plain language and cognitive load (COGA, S08)
Short sentences, literal language, no double negatives, state costs up front,
keep controls from moving unexpectedly, let users go back, minimize
interruptions, ask only for needed information, accept different input
formats, avoid data loss and timeouts.

### A11Y-24 Language (SC 3.1.1, 3.1.2)
`lang` on `<html>` and on foreign phrases; `translate="no"` on brand names and
code.

## Robust

### A11Y-25 Native first; no ARIA is better than bad ARIA (S07)
A role is a promise: adding `role="button"` obliges you to deliver Enter/Space,
focus and state. ARIA cannot make a div behave. Use `button`, `dialog`
(`<dialog>` with `showModal()`), `details`, `input type=checkbox`, `select`
before custom widgets. If building a custom widget, copy the APG pattern's
keyboard map and roles exactly and test with a screen reader.

### A11Y-26 Dialogs and overlays
Move focus in on open, trap it while modal (use `inert` on the background or
native modal `<dialog>`), restore focus to the trigger on close, Esc closes,
label via `aria-labelledby`. Non-modal popovers do not trap.

### A11Y-27 Live regions
Toasts, async validation and result counts use `aria-live="polite"`
(`assertive` only for urgent). The live region must exist in the DOM before the
text changes. Undo toasts must be reachable by keyboard and stay long enough
(INT-06).

### A11Y-28 Do not break assistive tech or extensions
Do not block paste, do not disable zoom, keep the accessibility tree accurate
(verify in browser devtools), leave password managers working, support
personalization and translation (COGA objective 8).

## Inclusive design principles (S16)
Comparable experience; consider situation (glare, one hand, noise, stress);
be consistent; give control; offer choice; prioritise content; add value.
Design for permanent, temporary and situational limits together: one-handed
use, bright sunlight, a broken wrist, a loud room.

## Accessible design systems: who owns what (V06, Hawkins)
- Design tokens carry many accessibility decisions (contrast pairs, border
  thickness, focus color, target sizes) but not all.
- Design every state of every component (default, hover, focus, active,
  disabled, error, loading, selected) in the system, so organisms inherit them.
- UX owns structure: error text placement, reading and focus order, wayfinding,
  alternative interactions, redundant coding (never color alone).
- UI owns contrast, type, focus visuals, motion.
- Content owns labels, error wording, alt text, reading level.
- Documentation must include keyboard interactions, pointer interactions, screen
  reader expectations and component rules (a button looks and acts like a
  button; a link-styled-button is a link). Keyboard docs are the biggest gap in
  the industry (V06).
- Child themes inherit accessibility tokens from the parent; they may change
  values (fuchsia vs green focus color) but must keep the contrast and
  thickness invariants. Encode invariants as tests, not prose.
