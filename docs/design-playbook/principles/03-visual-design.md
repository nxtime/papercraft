# Visual design

Hierarchy, space, type, color, depth. Sources: V02 (Schoger, Refactoring UI
"Bad About" redesign, partly read), S12 (Radix 12-step color scale), S13
(Utopia fluid type), S11 (Vercel guidelines), S14 (Refactoring UI book via
secondary summaries, low trust until checked against the book).

### VIS-01 Hierarchy comes from size, weight, color and position together
Do not make everything bold or large to be important. De-emphasize the
secondary instead: lighter weight or a muted color for meta text, lighter and
smaller icons beside text (solid icons next to text look heavy; V02 lightens
them), a text link instead of a heavy button for "show more".

### VIS-02 Start with generous space, then remove
Cramped panels were the first fix in V02: widen the panel, give it more padding,
raise line-height for comfortable reading. Added vertical spacing still reduced
total height because the wider reading width stopped long titles wrapping.
Define spacing from a scale (VIS-04), never ad hoc pixel values.

### VIS-03 Space between groups > space within groups
The gap that separates two groups must be clearly larger than any gap inside a
group, or proximity reads as one blob. Group related meta on one line with a
separator dot, then align that line to the title's baseline.

### VIS-04 Use a constrained scale for everything
Spacing, type sizes, radii, shadows and durations come from short lists
(tokens, SYS-01). A 4 px base (4, 8, 12, 16, 24, 32, 48, 64) works; 8 px steps
for large layout. Constraints are what make an interface look consistent.

### VIS-05 Align to a grid, and align to baselines
Left-align text and numbers that share a column; centered blocks float. A
12-column grid divides cleanly into halves, thirds, quarters (V02 splits cards
into two-thirds title and one-third votes, nav one quarter and content three
quarters). Align the first line of adjacent blocks on a shared baseline.
Optical alignment: correct by +/- 1 px when geometry looks wrong (S11).

### VIS-06 Typography
Sourced: 16 px inputs, weights, tabular-nums, clamp, typographic polish (S11, S13). Line length, line-height and letter-spacing ranges are [GK].
- Body text 16 px or larger; never below 12 px for anything readable; input text
  16 px or larger on mobile to prevent iOS zoom (S11).
- Line length 45 to 75 characters; line-height about 1.5 for body, tighter
  (1.1 to 1.3) for large headings.
- Weights: 400 for body, 500 to 600 for emphasis and headings; do not use below
  400 and do not change weight on hover or selection (causes layout shift).
- Use `font-variant-numeric: tabular-nums` wherever numbers align or update.
- Uppercase text needs added letter-spacing (about 0.05 to 0.1 em) and is for
  short labels only.
- Fluid sizing with `clamp()` between a small-screen scale and a large-screen
  scale (Utopia: for example ratio 1.2 at 320 px, 1.333 at 1500 px) removes
  breakpoint guesswork. Use `rem` in the clamp so user font-size settings and
  zoom still scale text (A11Y-13). The Utopia article itself does not discuss
  this caveat; it is our requirement.
- Typographic polish: curly quotes, the single ellipsis character, non-breaking
  spaces between value and unit, avoid widows and orphans (S11).

### VIS-07 One primary action per screen
One filled, high-contrast primary button; secondary actions are outlined or
text; destructive actions are not the primary color unless they are the point.
Multiple primaries make the screen's purpose unclear (V02).

### VIS-08 Color: define roles, not hues
Name by role (`surface`, `text-muted`, `accent`, `on-accent`, `danger`) so
modes change values not meaning. Build
each hue as a 12-step scale (Radix, S12):

| Step | Role |
|---|---|
| 1-2 | app background, subtle background |
| 3-5 | component background: normal, hover, pressed/selected |
| 6-8 | borders: subtle, interactive, strong/focus ring |
| 9-10 | solid fill, solid hover |
| 11-12 | low-contrast text, high-contrast text |

Steps 11 and 12 are designed to reach APCA Lc 60 and Lc 90 on a step-2
background of the same scale. Radix designs step 9 for white text except sky,
mint, lime, yellow, amber (dark text).

### VIS-09 Hard colors (orange, yellow, light green): flip the text
White text on orange often fails contrast; darkening it turns brown. Use a
lighter, more saturated fill with dark text, and tint the dark text toward the
fill's hue instead of neutral gray (V02). Always verify numerically
(`playbook contrast`).

### VIS-10 Never rely on color alone
Pair color with text, icon, shape or border thickness. Karen Hawkins (V06):
focus changes color and border 1 px to 3 px; error adds an icon and message
text. Charts: use color-blind-safe palettes plus direct labels or patterns.

### VIS-11 Depth: layered shadows, soft edges
Two or more shadow layers (a tight dark one for direct light, a wide faint one
for ambient). Prefer a subtle shadow over a border to lift cards, optionally a
semi-transparent border for edge clarity. Tint shadows and borders toward the
background hue on non-neutral backgrounds. Light page background with white
cards makes the content stand forward (V02 flips gray-on-white to white-on-
gray). Hover, active and focus states need more contrast than rest. Mechanics
(one light source, doubling-offset layers, hue-tinted shadow color): CSS-01..CSS-03.

### VIS-12 Radius: concentric and consistent
Child radius <= parent radius, ideally parent minus padding so curves are
concentric. One radius family for the whole product (sharp, soft or pill).
Apply the same radius to the panels and the buttons that sit with them (V02).

### VIS-13 Dark mode is its own design
Not an inversion. Use lighter surface for higher elevation, desaturate accents,
check contrast again, set `color-scheme: dark`, set `<meta name="theme-color">`.
Do not animate the theme switch (S11).

### VIS-14 Design the unhappy states
Empty (invite the first action, offer templates), sparse, dense, very long
text, error, loading, offline. Layouts must survive short, average and very
long user content, and right-to-left or longer translations.

### VIS-15 Icons
One family, one stroke width. Icons inherit `currentColor` (house rule: SVG
only, no emoji or Unicode pictographs). Icon-only controls need an accessible
name (A11Y-14). Size to match text cap height; lighten when next to text.
