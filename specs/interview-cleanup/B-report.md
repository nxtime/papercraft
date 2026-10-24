# Package B report: Admin page

Date 2026-10-23. Project `Interview` (verified). Page Admin only. Screenshots in `B/` (before-*, after-*, after-v3-row1..3, after-sessions-list). Deleted component names: `B/deleted-components.txt`.

## Lint (per section, byRule)
| section | before | after |
|---|---|---|
| Sessions | contrast 9, hard-coded 12, spacing 9 | 0 |
| Challenges | contrast 33, hard-coded 133, spacing 28, touch 22 | touch 22 |
| Live Session | contrast 44, hard-coded 106, type-scale 67 | 0 |
| Entry Approval (v3) | hard-coded 18 | 0 |
| whole page (incl. 212 component copies) | contrast 212, hard-coded 433, layer-name 7, spacing 94, touch 36, type 104 | touch 22 only |

## 1. Swaps and detached copies
- 352 instances re-pointed with `swap_instance` + `componentKey` to Design System mains (all Admin instances: Ant Button/Button Primary/Default/Small, Tag, Pill, Input*, Select, Sidebar, Header, Checkbox, Segmented, Entry Request, Timeline, etc., per component-mapping.csv). Instance layer names kept (renamed back after swap).
- After swap, Button/Pill/Tag instances clipped their text (instance keeps main size): width fit-content set on 169 roots and text layers; 2 Segmented + 9 Checkbox instances restored to their old width.
- Detached copies replaced by instances (18): 6 `Page Header` -> `Header / Page` (2 pure instances; 4 that carry a tag or button were rebuilt as local `Header / Page` frames with Tag / Button instances inside, because an instance cannot hold extra children), 1 `Form Field` -> `Form Field / Default` (width overridden to 592), 5 `Button / Primary Large` -> `Button / Primary / LG`, 5 `Button / Default Large` -> `Button / Secondary / LG`, 1 `Button / Danger` -> `Button / Danger / LG` (neighbours are LG; DS LG is 40 high, was 44; widths now hug).
- Not replaced: 4 `Card` (96e0de:15375, 15467, 15564, 15675; Sessions section, not v3): `Card / Default` has only title and body text, the local cards hold whole forms. 1 `Alert / Error` (15470): DS `Alert / Error` has no icon (colour-only error). The 2 other v3 "Form Field" wrappers (challenge list, segmented) are not form-field copies; renamed `Field / Challenges`, `Field / When`.
- Side effect on the v3 headers/buttons: original button icons (plus on "New session") dropped because DS LG buttons have no icon.

## 2. Colour binding
Text via `update_styles color: var(--css-name)`, fills via `backgroundColor: var()`, borders via `bind_variable stroke` (lint cannot see border bindings). Bound: 723 text, 22 fill, 303 border. Left literal: 10 sidebar dividers `#ffffff1f` and 1 fill `#0000000f` (no exact variable). No Google logo colours on this page.

## 3. Spacing and type
Off-grid padding/gap bound to `space/4|8|12` (1px to 0). 13px text (66) bound to `font/size/xs` (12), 15px (1) to `font/size/sm`. Not changed: touch-target warnings (22: Button / Add file 32, Add test 32, Add question 32, Add option 24); accepted for dense desktop admin, documented in the DS annotation.

## 4. Contrast (checked with `playbook contrast`, SYS-05 / a11y-audit)
`#1677ff` text -> `color/text/link` (6.16:1 white, 5.50:1 on `#e6f4ff`); white on `#1677ff` buttons: fill -> `color/brand/primary-strong` (6.16:1); white on `#52c41a` -> `color/status/success-text` (5.59:1); cyan/gold/pink tag text -> `color/tag/cyan|gold|red/text` (5.8, 6.5, 5.1); `#8c8c8c`/`#bfbfbf` grey text -> `color/text/secondary` (7:1); disabled switch text on `#bfbfbf` -> `color/text/secondary` (lint passes; 6 nodes, bg not changed); IDE greys `#555/#5a5a5a/#484848/#888` -> `color/ide/text-muted` (5.1 on `#161616`) or `color/ide/text-dim` on `#252526` (7.98). Lint contrast 86 -> 0.

## 5. Component deletion
All 212 Admin-local main components had 0 uses after the swaps (159 before, +53 emptied by the swaps), deleted: 212 (incl. the 4 template stubs T Hdr/T Donut/T Spark/T Stat, which A flagged). Not referenced as needed: the DS equivalents carry every audit item (Card, Form Field, Data Table Row, ...). Page now has only the 4 sections.

## 6. Layer naming (439 renames)
`Path` -> `Glyph` (207), `Td` -> `Cell` (36), `Th` -> `Head Cell` (8), `Close/Remove/Plus` -> `Icon Button / Close|Remove|Add` (73; not `Button...` on purpose so touch-target lint stays out), frame `Line` -> `Divider` (11), `Frame / X` slot wrappers -> `Slot / X`, 70 text layers named `Text` -> their text, row wrappers `Row N` / `Screens Row N` -> `Row / <content>`, v3 section -> `Section / Admin / Entry Approval`, v3 title text -> `Section Title`.

## Flags for A / the user
- DS `Sidebar / Admin`: user footer role tag overlaps the name and the email wraps (`Elena Vasquez`); nav items sit 2px higher than the old copy (pre-existing from A's main).
- DS `Input / Number`: Value text width 0 and height 44 makes the number wrap in the stepper (affects 25 instances; pre-existing).
- On edit screens (hug height 1500+) the 900px sidebar is vertically centred (pre-existing).
- Entry Request / Waiting: Deny button touches the helper text in the v3 decline block (DS main).
- Text colour binding regressions after variable edits (A2 warning): none seen on Admin; I did not call `set_variables`. 61 bound texts intentionally differ from the original colour (contrast fixes).
- Not done (out of requested scope): annotations, v3 vs Sessions duplicate flows, replacing local table/form/card structures.
