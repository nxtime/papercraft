# Anti-patterns and generic ("AI slop") design

What reads as templated, careless or hostile. Mostly distilled from the other
principles; the "slop" section is [GK] (general knowledge and widely shared
critique, no single fetched source) and is a taste guide, not a standard.

## Hard failures (fix before shipping)

### BAD-01 Div buttons and click handlers on non-interactive elements (A11Y-17)
### BAD-02 `outline: none` with no replacement focus style (A11Y-10, A11Y-11)
### BAD-03 Placeholder as the only label (A11Y-19, FBK-09)
### BAD-04 Disabled zoom, blocked paste, `user-scalable=no` (A11Y-28)
### BAD-05 Confirmation modal for a reversible delete (INT-06)
### BAD-06 Positive `tabindex`, or visual order differing from DOM order (A11Y-08)
### BAD-07 Color-only status (red/green dots) (A11Y-03, VIS-10)
### BAD-08 `transition: all` and animating layout properties (MOT-06)
### BAD-09 `prefers-reduced-motion` ignored, or handled with `animation: none` on view transitions (MOT-07)
### BAD-10 Targets under 24 px, or 44 px missing on touch (A11Y-09)
### BAD-11 Hover-only functionality (no touch, no keyboard) (A11Y-07, S11 `@media (hover: hover)`)
### BAD-12 Error toast that vanishes before it can be read (FBK-08)
### BAD-13 Optimistic UI on money or final-looking status (FBK-06)
### BAD-14 Infinite scroll with no way to reach the footer or keep position (FBK-10)
### BAD-15 Modal that does not trap focus or restore it (A11Y-26)
### BAD-16 Text on images or gradients with no measured contrast (A11Y-01)
### BAD-17 Emoji or Unicode pictographs as icons (house rule; use SVG with `currentColor`)
### BAD-19 `outline-none` next to `focus-visible:outline-*` in Tailwind v4 (verified by compiling with 4.3.3: `outline-none` sets `--tw-outline-style: none`, which `outline-2` then reuses, so the ring never draws). Use `outline-hidden` only if you add a visible ring, or omit `outline-none` (A11Y-10, A11Y-11)
### BAD-18 Content jumping while loading (images without dimensions, late-inserted banners) (A11Y-23, S11)

## Soft failures (hurt quality and trust)

### BAD-20 Everything is emphasized: five accents, three bold levels (VIS-01, PSY-08)
### BAD-21 Inconsistent spacing: arbitrary pixel values off the scale (VIS-04)
### BAD-22 Equal gaps inside and between groups (VIS-03)
### BAD-23 Pill-shaped everything, or three radius families (VIS-12)
### BAD-24 Animations everywhere, every one 500 ms (MOT-02, MOT-03)
### BAD-25 Generic copy: "Oops!", "Something went wrong", "Submit" (FBK-05, FBK-12)
### BAD-26 A tooltip as the only explanation of an essential control (S11: prefer inline help)
### BAD-27 Empty panels and spinner-only loading with no structure (FBK-03, FBK-04)
### BAD-28 Dark mode made by inverting colors (VIS-13)
### BAD-29 Pixel-perfect on one screen size, broken at 320 px or ultrawide (A11Y-04, S10)
### BAD-30 Three primary buttons on one screen (VIS-07)

## "AI slop" tells (taste, not standard) [GK]
Models default to the same look unless steered. Avoid, unless the brief asks:
- Purple/indigo-to-blue gradient hero on white; gradient text on headings.
- Default system or Inter-only typography with no scale decision.
- Cards-in-cards, everything in a rounded 16 px card with a drop shadow.
- Three-up feature grid with icon, heading, one sentence, repeated.
- Glassmorphism and glows with no purpose; decorative blur behind everything.
- Generic stock hero copy ("Revolutionize your workflow").
- Emoji as bullets or icons (also banned by house rule).
- Perfect symmetry and no hierarchy: nothing is allowed to be large or small.
Instead: choose a point of view (see skills/playbook-design Step 2), set
tokens first, commit to one typeface pairing, a restrained palette with one
sharp accent, and let spacing and scale do the work.
Companion skill: `frontend-design:frontend-design` already installed for
aesthetic direction; this playbook supplies the rules it should obey.
