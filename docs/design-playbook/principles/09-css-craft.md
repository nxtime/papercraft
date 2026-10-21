# CSS craft: shadows, transitions, resets, defensive layout

Practitioner rules for the details that separate polished from default.
Sources: S20 to S28 (Josh Comeau's articles on shadows, transitions, the CSS
reset, CSS vs JavaScript animation, squash and stretch, `@starting-style`,
`linear()` springs, anchor positioning; Ahmad Shadeed on defensive CSS). All
read 2026-10-21 through a summarizing fetch. Browser-support percentages are as
stated in each article on its date and go stale: re-check on caniuse.

## Shadows and depth

### CSS-01 One light source, one offset ratio
Every shadow on a page shares a light direction (above, slightly left) and the
same offset ratio, for example vertical offset always 2x the horizontal. Shadows
chosen per element look like unrelated blurry borders (S20). Higher elevation:
larger offset, larger blur, lower opacity.

### CSS-02 Layered shadows with doubling offsets
Stack 3 to 5 low-opacity shadows whose offset and blur double each layer
(1, 2, 4, 8, 16 px; blur equal to offset; alpha about 0.075 each for a large
elevation) (S20). Provide small, medium and large tiers as tokens
(`elevation.1..3`). Layers cost render time on old mobile devices and should not
be animated: animate opacity of a pseudo-element carrying the shadow instead.

### CSS-03 Tint shadows toward the background hue
Pure black at low alpha looks gray and dirty. Use the background's hue with
lower lightness and saturation, held in a `--shadow-color` token set by the
surface (example: background `hsl(220 100% 80%)`, shadow `hsl(220 60% 50%)`).
Too dark or saturated reads as a glow (S20). Use `filter: drop-shadow()` when
the shadow must follow a shape (tooltip tail, transparent PNG); Safari may lag
with inputs inside filtered elements.

## Transitions and motion mechanics

### CSS-04 Choose easing by what the element does
`ease-out` for things entering or appearing (modal); `ease-in` only when the
element ends offscreen or invisible; `ease-in-out` for symmetrical loops; the
default `ease` for movement that stays in view; `linear` rarely (S21). Easing
changes the shape of motion, not its length. This refines MOT-01: for on-screen
moves either `ease` or `ease-in-out` is fine; pick one per product and token it.

### CSS-05 Enter and exit are different actions
Define separate transitions for enter and exit. Comeau's hover examples use a
quick enter (125 ms) and a slower exit (450 ms); a dropdown fades in over 100 ms
and out over 400 ms (S21). The slow exit forgives pointer overshoot on hover
UI. This is the opposite of the exits-are-shorter default for dismissals
(MOT-02, a GK default): use slow exits for hover-revealed UI, short exits for
modal and toast dismissals.

### CSS-06 Bridge gaps with `transition-delay`
A 300 ms delay before a dropdown fades out keeps it open while the cursor
crosses a gap, and re-entering inside the window cancels it
(`transition: opacity 250ms 300ms`) (S21). This is the cheap version of INT-03;
use the safe triangle when submenus are large.

### CSS-07 Separate trigger from effect
Put `:hover` on a stable wrapper and animate a child. Animating the element under
the cursor can flip hover on and off repeatedly (flicker) (S21).

### CSS-08 Property cost ladder
`transform`, `opacity` and `filter` are cheap; `background-color` repaints;
`height`, `width`, `margin` trigger layout for siblings and are expensive (S21,
S23). Name properties in `transition`, never `all`. `will-change: transform` only
on elements that animate, because it holds video memory. Test on the lowest-end
target device.

### CSS-09 CSS first, WAAPI second, libraries last
CSS transitions and keyframes run off the main thread and stay smooth when
JavaScript blocks. A `requestAnimationFrame` loop or GSAP freezes with the main
thread. Libraries built on the Web Animations API (Motion) stay smooth. Order:
CSS, then WAAPI or a WAAPI-based library, then others; use a library only for
what CSS cannot do (SVG morphing, advanced sequencing). Scroll-linked effects use
`animation-timeline`, not scroll listeners. Compute dynamic values once into a
custom property, not per frame (S23). Bundle sizes quoted: Motion about 48 kB,
GSAP about 27 kB gzipped.

### CSS-10 Springs in CSS with `linear()`
`linear()` with 40 or more points reproduces spring and bounce curves; generate
them (Jake Archibald and Adam Argyle's generator, Easing Wizard). Store in CSS
variables, gate with `@supports`, keep a cubic-bezier fallback. Limits: duration
is fixed so a zero-damping spring cannot be modeled, and an interrupted
transition does not carry momentum, so reversals look abrupt next to a JS spring.
If custom curves exceed about 20% of your animations, review the tokens (S26).

### CSS-11 `@starting-style` gotchas
It lets a transition run on first render, but a more specific rule or an inline
style that sets the final value cancels it silently (no change, no animation).
Keyframes are simpler for most enter animations; choose `@starting-style` when a
dismissal may interrupt the enter, because a transition fades from the current
opacity where keyframes can flicker. The element must still render correctly
without support (S25).

### CSS-12 Micro-interactions: subtle, event-based, accessible
Squash and stretch on small icons works when subtle (use 25 to 50% of the value a
demo uses). Triggering on the hover event rather than state (stretch then snap
back after about 150 ms) feels playful. Wrap in `prefers-reduced-motion:
no-preference` and also check it in JS; trigger on `:focus-visible` as well as
hover; mark the decorative icon `aria-hidden="true"` and give the button visible
or hidden text. SVG path `d` transitions did not work in Safari (about 79%
support in April 2026), so decorative-only effects can accept that gap (S24).

### CSS-13 Reduced motion: two implementations
Comeau sets `transition: none` for cosmetic transitions inside
`prefers-reduced-motion: reduce`; MDN advises replacing large movement with a
gentler effect and keeping essential state information (S21, S15). Both are
valid: remove decorative, replace spatial, keep informational. View transitions
still use `animation-duration: 1ms` (MOT-07).

## Foundation

### CSS-14 Reset essentials worth keeping
`box-sizing: border-box` everywhere; `margin: 0` except `dialog` (its
`margin: auto` centers it); `body { line-height: 1.5 }` (WCAG expects users can
reach 1.5 without breakage; headings override smaller); media
`display: block; max-width: 100%`; `input, button, textarea, select { font: inherit }`
(browsers default form controls to about 13 px and iOS zooms inputs below
16 px); `overflow-wrap: break-word` on text; `text-wrap: pretty` for paragraphs and
`balance` for headings (progressive enhancement); `isolation: isolate` on the app
root so overlays sit above content without z-index wars;
`interpolate-size: allow-keywords` inside `no-preference` to animate to `auto`
(Chromium only at the article date) (S22).

### CSS-15 Defensive CSS: design for the worst content
From S28, the most valuable defaults: `flex-wrap: wrap` for rows that can
overflow; `gap` instead of margins or `space-between`; `min-height` not `height`
and `min-width` not `width`; `min-width: 0` on flex children and
`minmax(0, 1fr)` on grid tracks so long words do not blow out layout;
`object-fit: cover` for user images; `overscroll-behavior: contain` on
modals; `scrollbar-gutter: stable` to stop layout shift; a background color
under text-on-image so text survives an image failure; wrap fixed-width grid
columns in a `min-width` media query; `align-self: start` on sticky grid
children; `repeat(auto-fill, minmax(250px, 1fr))` so a lone item does not
stretch; truncate long user-generated names with ellipsis; a fallback in
`var(--x, fallback)` for JS-fed variables. These implement VIS-14.

### CSS-16 Anchored UI with CSS anchor positioning
`anchor-name` on the anchor, `position-anchor` and `position-area` on the
target, `position: fixed` (so overflow fallbacks trigger),
`position-try-fallbacks: flip-block` to flip near edges; spacing via margin
(S27). Support about 81% for the base API in July 2026; gate with `@supports` and
fall back to a small JS positioner. The article covers layout only: the
semantics, focus and keyboard behavior of the tooltip, popover or menu still
come from PAT-01, PAT-02 and A11Y-26.
