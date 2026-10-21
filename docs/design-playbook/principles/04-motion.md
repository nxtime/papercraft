# Motion

Sources: V05 (Tartarotti with Emil Kowalski, first part read), S06 (Kowalski
"great animations"), S11 (Vercel), S10 (Rauno), S15 (MDN reduced motion).
Material 3 token values are marked [GK]: the M3 page could not be fetched.

Motion exists to (1) preserve spatial consistency, (2) give cause-and-effect
feedback, (3) direct attention to a change. If it does none, delete it.

### MOT-01 Easing: ease-out for responses, ease-in-out for on-screen moves
Curve values below are [GK] community defaults; tune by feel (MOT-12).
Things that respond to input start fast and settle (`ease-out`). Things that
travel between two on-screen positions use `ease-in-out`. Linear only for
progress and continuous loops. Easing matters more than duration: it can make a
bad animation good or the reverse (Kowalski in V05). Never use default `ease`
by accident; choose a curve per intent. Suggested tokens
(`templates/tokens.example.json`): `ease-out: cubic-bezier(0.23, 1, 0.32, 1)`,
`ease-in-out: cubic-bezier(0.77, 0, 0.175, 1)`; M3 [GK]:
standard `cubic-bezier(0.2, 0, 0, 1)`, emphasized-decelerate
`(0.05, 0.7, 0.1, 1)`, emphasized-accelerate `(0.3, 0, 0.8, 0.15)`.

### MOT-02 Durations
Only "200 ms or less" (S11) and "under 300 ms" (S06) are sourced; the finer ranges are [GK]. Hover-revealed UI is the exception to short exits: see CSS-05.
- Micro feedback (hover, press, toggle): 100 to 150 ms.
- Interaction animations: 200 ms or less (S11); under 300 ms in general (S06).
- Enter/exit of modals, sheets, popovers: 200 to 300 ms; exits shorter than
  enters (about 70%).
- Large full-screen transitions: up to 400 to 500 ms, rarely.
- A 500 ms enter on a frequently opened tool is a defect (S06).
- M3 [GK]: short 50 to 200 ms, medium 250 to 400, long 450 to 600.

### MOT-03 Do not animate high-frequency or keyboard-driven actions
Anything the user does hundreds of times a day (command palette open,
keyboard navigation, list add/delete, right-click menus, tooltip repeat) gets
no animation or a near-zero one. Raycast has none and feels right (S06).
Ask "how often will one user see this?" before animating.

### MOT-04 Interruptible and retargetable
The user can change their mind mid-motion: a drawer swiped half-down can be
swiped back up. Use CSS transitions (which retarget smoothly) or spring
animations that carry velocity; avoid keyframe animations for interactive state
because they restart from the beginning. "Sluggish" often just means
non-interruptible (Kowalski, V05).

### MOT-05 Origin and direction carry meaning
Scale/translate from where the element physically came from
(`transform-origin` at the trigger, popovers growing from the button, not the
center). Entering and leaving along the same axis. Dialogs scale from about 0.8
to 1 with fade, not from 0 (S11). Press feedback scales to about 0.96, not 0.8.
Disney's "slow in, slow out" is the origin of easing; use squash/stretch and
overshoot sparingly (V05).

### MOT-06 Animate only compositor properties
`transform` and `opacity` skip layout and paint. Never animate `width`, `height`,
`top`, `left`, `margin`, `padding`. Never write `transition: all`; list
properties. Prefer CSS, then Web Animations API, then a JS library. JS libraries
that animate on the main thread drop frames under load (S06). Keep 60 fps; test
with CPU throttling. Avoid large `blur()` and `backdrop-filter` values on
animating layers (S11).

### MOT-07 Respect `prefers-reduced-motion`
`reduce` means remove, reduce or replace motion-based animation (S15).
- Replace large movement (scale, pan, parallax, slide) with a fade or color
  change.
- Keep essential state information (a progress change) in a gentler form.
- Remove decorative loops and autoplay.
- Never `animation: none` on view transitions: it leaves the old page frozen
  over the new one forever. Use `animation-duration: 1ms` (house lesson,
  SHARED_AGENT_MEMORY "page transitions").
- Test with the OS setting on, not by reading the CSS.

### MOT-08 Autoplay and looping
No autoplay except muted, non-essential loops. Anything moving longer than 5 s
beside other content needs a pause/stop/hide control (WCAG 2.2.2). Pause loops
that are off-screen. Do not flash more than three times per second (WCAG 2.3.1).

### MOT-09 Shared-element / route transitions: the best-liked pattern
The clicked card's image becomes the hero on the next page (view transitions
plus `view-transition-name`), then destination-only content rises in reading
order, one block at a time. Verify with a
filmstrip (CDP `Page.startScreencast`), not by assertion.

### MOT-10 Loading motion
A spinner that conveys effort feels faster than a static one at equal time.
Skeleton shimmer is subtle and respects reduced motion. Avoid fake progress
bars that lie about completion.

### MOT-11 Stagger and choreography [GK]
Reveal sequences 30 to 80 ms apart, total sequence under about 400 ms, in
reading order. A single page-load orchestration beats many scattered micro
animations.

### MOT-12 Test by feel, the next day
Tune curve and duration by trial. Review with fresh eyes after a day (S06).
Record the final values as tokens so they are reused, not retuned.
