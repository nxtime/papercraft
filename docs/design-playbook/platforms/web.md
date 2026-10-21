# Platform: web (HTML, CSS, JS)

Implementation recipes for the rules in `principles/`. Rule IDs in brackets.

## Baseline CSS

```css
:root {
  color-scheme: light dark;            /* [VIS-13] */
  --focus-ring: 2px solid var(--color-focus);
  --ease-out: cubic-bezier(0.23, 1, 0.32, 1);
  --dur-fast: 120ms; --dur-base: 200ms;
}
html { -webkit-text-size-adjust: 100%; scroll-padding-top: var(--sticky-h, 0px); } /* [A11Y-10] */
body { font: 1rem/1.5 var(--font-body); }                                         /* [VIS-06] */

:focus-visible {                                                                  /* [A11Y-10, A11Y-11] */
  outline: var(--focus-ring);
  outline-offset: 2px;
}
:focus:not(:focus-visible) { outline: none; }

button, [role="button"], a { touch-action: manipulation; }                       /* [BAD-10] */
.hit { position: relative; }
.hit::after { content: ""; position: absolute; inset: -8px; }                    /* expand target, not visual */

@media (hover: hover) and (pointer: fine) { .item:hover { background: var(--color-hover); } } /* [BAD-11] */

@media (prefers-reduced-motion: reduce) {                                         /* [MOT-07] */
  *, *::before, *::after {
    animation-duration: 1ms !important;   /* not "none": see MOT-07 */
    animation-iteration-count: 1 !important;
    transition-duration: 1ms !important;
    scroll-behavior: auto !important;
  }
}
@media (forced-colors: active) { :focus-visible { outline: 2px solid CanvasText; } }
```

Prefer replacing motion with a fade over blanket 1 ms where the motion carries
information.

## Tailwind v4 focus trap
`outline-none` sets `--tw-outline-style: none` and `focus-visible:outline-2` reuses that variable, so the
ring never draws (compiled and checked with Tailwind 4.3.3, audit 2026-10-21). Write
`focus-visible:outline-2 focus-visible:outline-solid focus-visible:outline-(--focus-color)` and omit `outline-none` (BAD-19).

## Native elements first [A11Y-25, SYS-08]

| Need | Use | Not |
|---|---|---|
| Action | `<button type="button">` | `<div onclick>` |
| Navigation | `<a href>` | `<button>` that calls `router.push` |
| Modal | `<dialog>` + `showModal()` | custom overlay div |
| Disclosure | `<details><summary>` | custom accordion without semantics |
| Toggle | `<input type=checkbox role=switch>` or `button aria-pressed` | clickable span |
| Select | `<select>` unless search/multi demands combobox | custom listbox first |
| Popover | `popover` attribute / anchor positioning where supported | JS positioning libs by default |

Custom widgets: copy the WAI-ARIA APG keyboard map and roles exactly and test with
VoiceOver/NVDA/Orca.

## Dialog and focus recipe [A11Y-26]
```js
dialog.showModal();          // traps focus, makes background inert, Esc closes
dialog.addEventListener('close', () => trigger.focus());
```
Non-native: set `inert` on siblings, move focus into the dialog, restore on close.

## Forms [FBK-09, A11Y-19]
```html
<label for="zip">Postal code</label>
<input id="zip" name="postal-code" autocomplete="postal-code" inputmode="text"
       aria-describedby="zip-hint zip-err" aria-invalid="true">
<p id="zip-hint">Six characters</p>
<p id="zip-err">Postal code is too short: use 6 characters</p>
<div aria-live="polite" id="status"></div>
```
Error states change border width (1 px to 3 px) and add icon plus text, not only
color (V06).

## Motion recipes [MOT-*]
```css
.sheet { transition: transform var(--dur-base) var(--ease-out), opacity var(--dur-base) var(--ease-out); }
.popover { transform-origin: var(--radix-popover-content-transform-origin, top left); }
.btn:active { transform: scale(0.97); }
```
Only `transform` and `opacity`. Interactive state uses transitions, not keyframes
(retargetable, MOT-04). Never `transition: all`.

View transitions (route change) must use `animation-duration: 1ms` for
reduced motion, never `animation: none`.

## Optimistic update skeleton [INT-11, FBK-06]
```ts
const prev = state;
setState(optimistic);
try { await api(); } catch (e) { setState(prev); showInlineError(item, retry); }
```
Add an idempotency key on mutations; reconcile with server response.

## Performance guardrails [S11]
Explicit image dimensions; `content-visibility: auto` or virtualization for long
lists; preload only above-the-fold images and critical fonts; subset fonts;
`will-change` only during an animation; large `backdrop-filter` is expensive;
test with 4x CPU throttle. Mutations under about 500 ms.

## Fluid type [VIS-06]
```css
:root {
  --step-0: clamp(1rem, 0.96rem + 0.2vw, 1.125rem);
  --step-1: clamp(1.2rem, 1.1rem + 0.5vw, 1.5rem);
  --step-3: clamp(1.728rem, 1.4rem + 1.6vw, 2.5rem);
}
```
Generate real values with Utopia (utopia.fyi); keep `rem` terms so zoom works.

## Testing on web
1. Tab through the whole page, no mouse. 2. 200% zoom and 320 px width.
3. OS reduce-motion on. 4. Contrast via `scripts/contrast.py` or devtools.
5. axe/Lighthouse as a floor, not proof. 6. Screen reader pass on the main flow.
7. Throttled CPU/network. 8. Filmstrip (CDP screencast) for transitions.
