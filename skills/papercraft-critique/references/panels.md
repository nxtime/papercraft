# Panel rubrics

Five panels, one artifact, one set of evidence. A panel is a pass with a fixed rubric: what it inspects, which tool output backs it, what it does **not** judge, and the band each finding lands in. The panels share the band legend in [../SKILL.md](../SKILL.md) (Blocker 0–2.9, Poor 3–4.9, Weak 5–6.9, Acceptable 7–7.9, Good 8–9.4, Excellent 9.5–10).

## How to run one panel

1. Read the evidence you were handed: the lint payload, the screenshot, the design spec, the variables, the code result.
2. Write findings. Every finding cites its evidence — the lint rule and node, the region of the screenshot, the spec's MUST rule, the `verify_code` violation.
3. Score the panel 0–10. The score is the panel's verdict on the whole screen, not an average of its findings: two serious defects on one region cost more than two trivial ones on two regions, but a screen with three medium defects is not "Good" either.
4. Apply the caps. Hard evidence caps the owning panel (SKILL.md section 4); degraded evidence caps it at 6.9 (SKILL.md section 8).

Evidence a panel may not use: taste, guesses about a user's preferences, or a claim about something no tool showed. When a panel wants to judge what no tool exposes — whether an image is meaningful, whether a flow makes sense — it raises the question as an open item instead of scoring it as a defect.

Tool arguments used below are the current names. Note that `lint_design`'s `scale` is the list of allowed font sizes, not a zoom; the zoom argument of `get_screenshot` is `scale: 1 | 2`.

---

## Designer — weight 0 in v1

**Judges:** layout and composition, alignment, spacing rhythm, hierarchy, the type scale in use, how things are grouped, screen states, overflow, density.

**Evidence it uses**

| Tool | How |
|---|---|
| `get_screenshot` (`nodeId`, `scale: 2`) | the primary evidence: alignment, edges, rhythm, hierarchy, clipping, empty areas |
| `lint_design` (`nodeId`, `pageId`) | `spacing-grid`, `type-scale`, `layer-name` findings |
| `get_design_spec` (`nodeId`, `target`) | flow and sizing (`fixed`, `hug`, `fill`), grouping, where the spacing rules were broken |
| `get_tree_summary` (`nodeId`, `depth`) | structure depth, unnamed frames, accidental nesting |

**Does not judge:** contrast and WCAG (Accessibility), token and palette choices (Brand), wording (Copy).

**Bands**

| Band | What the Designer saw |
|---|---|
| 0.0–2.9 | Broken layout: frames overlap, content sits outside the artboard, text is clipped, the screen reads as a pile |
| 3.0–4.9 | Poor composition: everything on the default grid, equal weight everywhere, no grouping, ragged edges |
| 5.0–6.9 | Weak: spacing off the 4px grid in places, sizes off the type scale, mixed grouping (space *and* containers everywhere), a state missing |
| 7.0–7.9 | Acceptable: coherent, but one region is crowded or one rhythm is broken |
| 8.0–9.4 | Good: clear hierarchy, aligned to a few strong edges, consistent rhythm, states present |
| 9.5–10.0 | Excellent: deliberate rhythm, one memorable idea, whitespace used as a material |

**Fixes that move the score:** snap spacing to the 4/8 scale (`lint_design` with `fix: true`), pull one size step per level, give each group either a container or space — not both, align to one left edge, add the missing empty/loading/error state.

---

## Critic — weight 0.4

**Judges:** whether the screen meets the brief (the request this run is executing), whether visual weight matches the priority, readability as a human experiences it, and whether the real cases are covered.

**Evidence it uses**

| Tool | How |
|---|---|
| the brief | the user's request and the `papercraft-design` section 0 direction, read against the screenshot |
| `get_screenshot` (`nodeId`, `scale: 2`) | one primary action, reading order, what is above the fold, legibility by eye |
| `lint_design` (`nodeId`, `pageId`) | `contrast`, `text-size`, `type-scale` findings as the objective part of readability |
| `get_design_spec` (`nodeId`, `target`) | what the screen actually contains, against what was asked for |
| `verify_code` (`nodeId`, `target`, `code`) | when the design is code-bound: whether the implementation kept the intent |

**Does not judge:** which palette (Brand), line-level wording (Copy), implementation details beyond fidelity to the design.

**Bands**

| Band | What the Critic saw |
|---|---|
| 0.0–2.9 | Misses the brief: wrong screen or wrong content, the primary action is absent or ambiguous |
| 3.0–4.9 | Fights the brief: right elements, wrong order or emphasis, or hard to read (contrast errors, tiny type) |
| 5.0–6.9 | Weak: it does the job, but the weight is flat and something important sits below the fold or is buried |
| 7.0–7.9 | Acceptable: meets the brief; one area stays unclear or the priority is not obvious |
| 8.0–9.4 | Good: the brief is met, the primary action is obvious, readability is solid |
| 9.5–10.0 | Excellent: unmistakable hierarchy, the screen reads in one pass, the edge cases are covered |

**Fixes that move the score:** make one thing clearly primary and quiet the rest, move the important content up, cut a competing element, re-read the brief line by line and check each requirement against the screenshot.

---

## Brand — weight 0.2

**Judges:** token compliance, on-brand color, semantic color use, mode coverage, product voice consistency, component reuse.

**Evidence it uses**

| Tool | How |
|---|---|
| `get_variables` (`pageId`, `format`) | the variables, tokens, active `mode` and available `modes` — the reference palette |
| `lint_design` (`nodeId`, `pageId`) | `hard-coded-color` findings |
| `get_computed_styles` (`nodeIds`) / `find_nodes` (`filters`) | fills and text colors that are not following a variable |
| `get_design_spec` (`nodeId`, `target`) | which tokens the design already binds |
| `get_components` / `get_tree_summary` | components built and instanced versus copies |
| `verify_code` (`nodeId`, `target`, `code`) | `hardcoded-color`, `missing-component` in the implementation |

**Does not judge:** composition (Designer), wording line by line (Copy), whether the layout serves the brief (Critic).

**Bands**

| Band | What the Brand saw |
|---|---|
| 0.0–2.9 | Off-system: hard-coded colors where tokens belong, a foreign palette, no modes |
| 3.0–4.9 | Off-brand: technically on-token but the accent is used everywhere, or the screens read as different products |
| 5.0–6.9 | Weak: one or two hard-coded colors, the accent covers much of the screen, components copied instead of instanced |
| 7.0–7.9 | Acceptable: on-system, with one token or component inconsistency |
| 8.0–9.4 | Good: every color is a token, the accent marks the primary action only, modes defined and checked, components reused |
| 9.5–10.0 | Excellent: coherent across screens and modes, the palette carries meaning |

**Fixes that move the score:** `lint_design` with `fix: true` relinks hard-coded colors that match a variable, add the missing variable before using the color, cut accent back to the primary action, build the repeated part once and place instances, check the second mode with `set_variable_mode`.

---

## Accessibility — weight 0.2

**Judges:** WCAG contrast, the text-size floor, touch-target size, focus and hover states, semantic structure, alt text, and signalling that does not rely on color alone.

**Evidence it uses**

| Tool | How |
|---|---|
| `lint_design` (`nodeId`, `pageId`) | `contrast`, `text-size`, `touch-target` findings |
| `get_screenshot` (`nodeId`, `scale: 2`) | whether a visible focus or hover state was designed at all, and how the error state reads |
| `get_design_spec` (`nodeId`, `target`) | which text is a title, how the screen is grouped, what each control is |
| `find_nodes` (`textValue`, `filters`) | image and icon nodes, unlabeled icon buttons |
| `get_tree_summary` (`nodeId`, `depth`) | semantic structure: heading levels, groups, control order |

**Does not judge:** visual taste (Designer), palette taste (Brand), voice (Copy).

**Bands**

| Band | What Accessibility saw |
|---|---|
| 0.0–2.9 | Fails WCAG: contrast errors, text under 12px, targets under 44px, meaning carried by color alone |
| 3.0–4.9 | Poor: borderline contrast, unlabeled controls, an error state that is red text and nothing else |
| 5.0–6.9 | Weak: passes on the desktop artboard but fails on the mobile one, one image without alt text |
| 7.0–7.9 | Acceptable: compliant baseline, one gap (no visible focus state, one icon-only button) |
| 8.0–9.4 | Good: WCAG clean, targets pass, semantics and labels present, focus and hover states designed |
| 9.5–10.0 | Excellent: every state specified, sensible keyboard order, no meaning by color alone |

**Fixes that move the score:** fix the color, not the check, until `lint_design` reports no contrast error, name interactive layers `Button…` / `Tab /…` so the target rule can see them and grow them to 44px, pair every error color with an icon or a word, design the focus and hover variants as components, give every image an alt text.

PaperCraft has no focus-ring or alt-text checker: judge those from the design itself (a `Button / Focused` variant, alt text in the spec) and report what is missing as a gap, not as a tool finding.

---

## Copy — weight 0.2

**Judges:** voice and tone against the product's voice, terseness, labels and microcopy, error-message quality, empty states, placeholders versus labels, the absence of filler.

**Evidence it uses**

| Tool | How |
|---|---|
| `get_design_spec` (`nodeId`, `target`) | the authoritative copy: every text node's content, in order |
| `get_screenshot` (`nodeId`, `scale: 2`) | how the copy reads in context: line breaks, truncation, all-caps abuse |
| `find_nodes` (`textValue`) | repeated strings, leftover placeholders, "Lorem ipsum", inconsistent labels for the same action |
| `verify_code` (`nodeId`, `target`, `code`) | `missing-text` — copy that was lost between the design and the code |

**Does not judge:** typography and layout (Designer), color (Brand), whether the screen meets the brief (Critic).

**Bands**

| Band | What Copy saw |
|---|---|
| 0.0–2.9 | Wrong: placeholder text, lorem ipsum, or copy that contradicts the brief |
| 3.0–4.9 | Poor: filler adjectives, two voices on one screen, errors that blame the user |
| 5.0–6.9 | Weak: wordy, mixed formality, vague labels ("Submit" where "Save changes" is meant) |
| 7.0–7.9 | Acceptable: clear but plain; one error message says only "Error" |
| 8.0–9.4 | Good: terse, one voice, every action labeled with a verb, errors say what happened and what to do next |
| 9.5–10.0 | Excellent: reads like a product that shipped, not like a mockup |

**Fixes that move the score:** cut every word that does not inform, give each action a verb label ("Save changes", not "OK"), write errors as *what happened + what to do next*, fill empty states with an explanation and the next action, replace placeholder copy with real content, keep one formality level per screen.

PaperCraft has no spell-checker or copy linter: the copy panel reads the spec and the screenshot, and it flags what a human reviewer would flag — it does not invent grammar rules.

---

## Scoring discipline

* Half-point steps only: 7.5, 8.0, 8.5. Never 8.37.
* The score is a verdict on the screen, not an average of findings.
* A panel that finds nothing wrong and cites no evidence scores 7.0–7.9, not 10. A 10 means the screen is a reference for the next one.
* Caps bind: hard evidence (SKILL.md section 4) and degraded evidence (section 8) are applied after the verdict, never negotiated with.
* No panel may raise another panel's score. If the Critic likes the hierarchy, that stays a Designer finding.
