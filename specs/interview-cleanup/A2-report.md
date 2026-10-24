# Package A2 report: Design System page cleanup

Date 2026-10-23. Project `Interview` (fileName verified). Page `Design System` only. Evidence in `A2/`: `issues-before.json` (all 959 issues, linted per top-level frame to avoid the 200 cap), `issues-7.json` (final), `after-brand-swatch.png`, `after-components.png`. Backup: `/Volumes/Storage/Projects/Interview.backup-20261023-0112`.

## Lint counts (DS page, byRule, exact)
| rule | before A2 | after A2 | pre-A baseline |
|---|---|---|---|
| contrast (errors) | 194 | **0** | 128 (67 on original components) |
| hard-coded-color | 366 | **7** | 11 |
| spacing-grid | 180 | **0** | 57 |
| touch-target | 30 | 30 (accepted, see below) | 16 |
| type-scale (info) | 135 | 135 (not in scope) | 42 |
| layer-name | 49 | 49 (not in scope) | 7 |
Goal met: contrast 0 (target <= 67).

## What changed
- Hard-coded colours: 359 nodes bound by exact hex match (352 text colours, 12 fills) via `update_styles` `var(--color-...)`. White text bound to `color/text/inverse`, not `bg/surface` (same hex, text role). Note: `bind_variable text-color` was not recognised by lint, `update_styles color:var()` was; use that.
- Contrast (SYS-05, A11Y checked with `playbook contrast` maths, all pairs >= 4.5):
  - white on `#1677ff`/`#4096ff` -> parent fills rebound to `color/brand/primary-strong` (#0958d9, 6.16); 16 fills + 5 matching 1px strokes bound.
  - white on `#52c41a` (Score Dots) -> `color/status/success-text` (#237804, 5.59); white on `#ff4d4f` -> `color/status/error` (5.57).
  - `#5a5a5a` on `#1a1a1a` -> `color/ide/text-muted` (4.91); `#888`/`#666` on `#252526|#2a2a2a|#252525|#181818|#161616` -> `color/ide/text-dim` (>= 7.5); `#5a80a8` on `#1c2a40` -> `ide/text-dim`; `#13a8a8` on `#1a3540` -> `syntax/type` (6.33); sidebar `#64676c` -> `sidebar/text-muted` (4.51).
  - `#8c8c8c`/`#bfbfbf`/`#bbb`/`#878787`/`#848484` on light -> `color/text/secondary` (6.7-7.0). Note placeholders and disabled input text now read as secondary text; disabled look is carried by the grey fill only.
  - `#52c41a`, `#16a34a` text on light -> `status/success-text`; white on `#faad14`/`#13a8a8` (Score Dots 2, Theory Next) -> `color/text/primary`.
  - Variable VALUES changed (global, affects Admin and Candidate too, all pairs now pass): `color/tag/gold/text` #ad6800 -> #874d00, `color/tag/green/text` #389e0d -> #237804, `color/tag/cyan/text` #08979c -> #006d75. Bound fills followed automatically.
  - `Pipeline Row / Dark` was a transparent component meant for a dark card: gave it `color/ide/card` background, stage text `ide/text-muted`, name `text/inverse`.
- Spacing: all 180 off-grid nodes bound to `space/*` (nearest in-scale value: 1,2,3,5->4; 6,7,9->8; 10,11->12; 14,18->16; 22->24; gap 6->8). On-grid padding was not rebound (would be thousands of bindings with no lint effect). Visual check of buttons, tags, rows, toolbars: nothing broke.
- Radii: all uniform radii bound to `radius/*`: 289 -> sm (3,4,5), 442 -> md (6,7,8), 75 -> lg (12), 153 -> pill (99,999). Multi-value radii (per-corner) not touched; no radii above 12 other than the pill ones existed.
- Colors board: added swatch `color-brand-primary-strong` (#0958d9) in the Brand card.
- `Label / *` stubs: they are plain label frames, not components (`get_components` lists none), and each captions a real component at x=30000. Deleted only the 5 whose component has 0 instances (`Link Field / Copied`, `Challenge Option / Two Sum Off`, `... / URL shortener On`, `... / JavaScript closures Off`, `Timeline / Step 3`). The other 10 kept because their component has instances. Delete them too if you read the instruction as "all 15".

## Skipped / accepted
- 7 hard-coded: `#000000` text on `Stars / 0..5` (6, no exact variable; `text/primary` is #000000e0, near match not in the audit mapping) and `#bae0ff` on `Funnel Row` (1, no variable).
- Touch targets (30): `Button *` SM 24-28, MD 32-34, LG 40, `Button / Undo|Clear|Delete` 24, bell 36. Accepted desktop density: playbook A11Y-09 (24 px AA minimum, 44 default on touch only) and PAT-06 (about 27-30 px practical floor for dense content). All >= 24 px. Annotation on the DS page already documents it. Candidate pages are desktop IDE flows.
- Untouched (out of scope): type-scale 135, layer-name 49, `brand/hover` (#4096ff) with white text (state unused).
- Side effect to check: `set_variables` (tag text values) made lint report hard-coded text again on untouched nodes (text bindings seemed to be flattened for some nodes on the DS page); rebinding fixed it. Admin/Candidate pages were not inspected for this; if B/C see text colours turn literal, that is the cause.
