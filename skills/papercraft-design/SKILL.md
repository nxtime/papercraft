---
name: papercraft-design
description: Design-quality rules for work made in PaperCraft (or any flexbox-based design tool): commit to an aesthetic direction, tokens before pixels, a 4/8pt spacing scale, a type scale, contrast and touch-target minimums, visual hierarchy, consistent radii, components, and every screen state. Use when designing or reviewing screens, landing pages or dashboards in PaperCraft, when a design looks generic, or before finishing a design. Use together with the papercraft skill, which has the tool mechanics.
---

# papercraft-design

Rules for making designs that look intentional, adapted to what PaperCraft can do: flexbox layout, Inter as the only font, color variables, components, `lint_design`. Mechanics (tools, loop, limits) are in the `papercraft` skill. Sources: Figma's agent skills (variables, auto layout, components, design-system audits, spacing snapped to a scale), Anthropic's frontend-design guidance (commit to a direction, avoid generic AI aesthetics) and common UI practice (4/8pt grid, WCAG contrast, 44px targets).

## 0. Before you draw anything: commit to a direction

Write down, in a few lines, before the first tool call:

* **Purpose and audience.** What is this screen for, who uses it, what is the one thing they must do?
* **Tone.** Pick one and mean it: calm and quiet, dense and technical, playful, editorial, luxe, brutalist, soft and friendly. "Clean and modern" is not a direction.
* **The memorable thing.** One idea a person would remember: a huge number, a bold color block, an unusual rhythm of whitespace, a strong grid.
* **Palette.** One dominant neutral family, one accent used sparingly, one semantic set (success, warning, danger). Define them as variables (section 2). A confident accent on a restrained neutral beats five evenly weighted colors.

Avoid the default AI look: purple-to-blue gradient on white, a row of three identical rounded cards with an icon and two lines, centered everything, generic stock phrases ("Seamless. Powerful. Effortless."), emoji as icons, drop shadows everywhere. If your draft would pass for any SaaS template, change one structural thing (layout, scale contrast, color block) until it would not.

**Inter is the only font.** You cannot pair typefaces, so hierarchy has to come from size, weight, color and space. Use real scale contrast (a 40 or 64px headline against 16px body), bold only for what matters, and `text-muted` for secondary text. Tight negative letter-spacing on large headlines and uppercase 12px labels with positive letter-spacing give Inter more character. Do not fake a second font.

## 1. Spacing: a 4/8pt scale

Allowed spacing values: **4, 8, 12, 16, 24, 32, 48, 64** (and 96 for page sections). Everything is a multiple of 4: `gap`, every side of `padding`, sizes of boxes and icons.

* Related things are close, unrelated things are far. Inside a group: 4 to 12. Between groups: 24 to 48. Between page sections: 64 to 96. If everything has the same gap, nothing reads as a group.
* Padding of a container is at least its inner gap. Card padding 16 (mobile) to 24 (desktop); screen padding 16 or 24.
* Use `gap` on the parent instead of per-child spacing (there is no margin).
* Icons 16, 20, 24 (32 for feature icons). Avatars 32, 40, 48.
* `lint_design` warns `spacing-grid` on any gap or padding off the 4px grid. Fix by snapping to the nearest allowed value.

## 2. Tokens first (variables)

* All colors are variables (`var(--name)`), named by role: `bg`, `surface`, `border`, `text`, `text-muted`, `accent`, `on-accent`, `success`, `warning`, `danger`. Two to three surface levels at most (`bg` < `surface` < `surface-2`).
* Define at least two modes if the product may be dark or light (`set_variables` with `mode`), and check both: lint contrast in each mode after `set_variable_mode`.
* Semantic colors carry meaning only: red means error or destructive, green means success. Do not use them decoratively.
* If a color you need is not a variable, add the variable, then use it. `lint_design` reports hard-coded colors that equal a variable.
* Spacing, radius and type sizes are number variables (`space/md`, `radius/sm`, `font/body`): define them once, in a collection of their own, and bind fields with `bind_variable`. Keep the values on the lists in this skill. Group names with `/` and alias semantic names to base ones (`color/button/bg` to `color/brand/primary`).

## 3. Type scale

Sizes: **12, 14, 16, 20, 24, 32, 40, 48, 64.** `lint_design` flags anything else as `type-scale` and under 12px as `text-size`.

| Use | Size / weight | Line height |
|---|---|---|
| Display, hero | 48 to 64 / 700 to 800 | 1.1 to 1.2 (set `line-height` in px) |
| Page title | 32 to 40 / 700 | 1.2 |
| Section title | 20 to 24 / 600 | 1.3 |
| Body | 16 / 400 (mobile never below 16) | 1.4 to 1.6 (24px) |
| Secondary, captions, table text | 14 / 400 | 20px |
| Labels, overlines, footnotes | 12 / 600, uppercase with letter-spacing 1px | 16px |

* One or two weights per screen (400 and 600/700). Do not use a different size for every element: three to five sizes per screen.
* Line lengths: 45 to 75 characters for paragraphs (set a `width`, around 560 to 720px on desktop).
* Left-align body text. Center only short headings and hero blocks.
* Numbers that matter (stats) get size, not just weight: 32 to 48.

## 4. Color and contrast

* Text contrast **4.5:1** for normal text, **3:1** for large text (24px, or 18.66px bold) and for UI components (borders of inputs, icons that carry meaning). `lint_design` reports `contrast` errors; fix the color, not the check.
* `text-muted` must still pass 4.5:1 on its background. Placeholder text too.
* Never rely on color alone: pair error red with an icon or text.
* Accent is for the primary action, the active state and one or two highlights. If more than about 10% of the screen is accent, it stops meaning anything.
* Dark themes: raise surfaces by lightening (`bg` < `surface` < `surface-2`), not by shadows (PaperCraft has none).

## 5. Hierarchy and composition

* **One primary action per screen.** One filled accent button; others are secondary (outline or `surface`) or text links.
* Squint test: blur the screenshot mentally. You should see three levels: the thing that matters, the supporting content, the chrome. If everything has equal weight, enlarge one thing and quiet the rest.
* Align to a few strong edges. Every element should line up with something. Prefer a left edge and consistent column widths over centering everything.
* Group with containers (surface + padding + radius) or with space, not both everywhere. Not everything needs a card.
* Use whitespace as a material. Generous outer padding makes a screen feel designed; cramped content feels like a form dump.
* Consistent radii: pick one family and stay in it, for example 8 (inputs, small), 12 (buttons), 16 (cards), 999 (pills, avatars). Nested corners: inner radius = outer radius minus padding.
* Borders: 1px `border` color, used consistently on inputs and dividers.
* Icons: one set per design, one size per context, `text-muted` unless active. Text labels beat icon-only buttons.

## 6. Touch targets and density

* Interactive targets are **at least 44px** tall and wide on touch (mobile, tablet). Make them 44 with padding (12 + 20 + 12), not just a bigger font. On dense desktop UIs 32 to 40 is acceptable; say so, and keep spacing between targets at 8 or more.
* `lint_design` checks frames named `Button…`, `Tab /…` and `Tab Bar /…` for height under 44, so **name interactive layers that way**.
* Mobile: body 16px or larger, side padding 16 to 24, bottom navigation within thumb reach, safe areas (status bar 44, home indicator 34 at the bottom).

## 7. Components and states

* Anything that appears twice is a component, built once, named `Component / Variant`, placed with `create_instance`. Change the main component, not the copies.
* Components first, screens second: build the button, field, chip, row, card, tab bar; then compose screens from instances; override only text.
* **Design every state, not only the happy path.** For each interactive component make a variant per state: default, hover, pressed, focused, disabled, selected, error. For each screen: empty, loading, error, partial, full, and long text (names that wrap, big numbers).
  * Empty states explain what goes here and offer the next action.
  * Disabled controls stay legible (contrast of text still 3:1 or better) but look inert.
  * Error: message under the field in `danger`, with an icon or text, not color alone.
  * Loading: skeleton blocks in `surface-2` in the shape of the content.
* Variants are separate components in PaperCraft (`Button / Primary`, `Button / Disabled`); keep the names parallel so they sort together.
* Keep a design-system page (swatches from the variables, the type scale, every component with its states). See the recipe in the `papercraft` skill.

## 8. Naming

* Layer names are real: `Hero`, `Stat Card / Revenue`, `Nav / Settings`, `Icon / gear`. No `Frame 12`. `lint_design` warns on default names.
* The name says what it is, not how it looks (`Primary Action`, not `Purple Box`).

## 9. Finish checklist

Run it before `finish_working_on_nodes`:

1. Direction written down; the screen has one memorable idea and one primary action.
2. Every color is a variable; both modes checked if there are two.
3. Spacing only 4, 8, 12, 16, 24, 32, 48, 64, 96; type only from the scale; radii from one family.
4. `lint_design` has zero errors and zero warnings, and the `info` items were considered.
5. Screenshot at scale 2 looked at: hierarchy, alignment, wrapping, clipping, empty areas, contrast by eye.
6. Interactive things at least 44px on touch and named `Button…` / `Tab /…`.
7. Repeated parts are components; states exist for every interactive component and for empty, loading and error screens.
8. Layers named; design-system page current.
