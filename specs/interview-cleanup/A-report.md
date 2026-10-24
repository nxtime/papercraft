# Package A report: Interview Design System

Date 2026-10-23. Project `Interview` (fileName verified). Backup: `/Volumes/Storage/Projects/Interview.backup-20261023-0112`. Screenshots in `A/` (before-*, after-*). Mapping files: `variable-mapping.csv`, `component-mapping.csv` (old name, new name, componentKey, action; 280 rows).

## 1. Variables (collection `Colors`)
- 161 -> 137 variables (25 merged, +1 `color/brand/primary-strong`). All names are `color/<group>/<name>` (csv `new` column with `color/` prefix). SYS-03: named by role, not value.
- Rename = create new + repoint every bound fill (`update_styles`) + delete old. The app has no rename tool, and `delete_variables` detaches fills, so repointing was required. 4293+ bound fills repointed on all 3 pages. No dangling `var(--old)` found afterwards.
- Merges (csv "merge" rows) repointed to the canonical variable then deleted (no leftover aliases needed because nothing binds them).
- SYS-02 (semantic alias over primitive value): `color/brand/pressed` and `color/text/link` are aliases of `color/brand/primary-strong` (#0958d9); `color/status/info` is an alias of `color/brand/primary`.
- Contrast: `#1677ff` + white = 4.10:1 FAIL, `#0958d9` + white = 6.16:1 pass (`playbook contrast`), `#0958d9` on `#e6f4ff` 5.50:1 pass (SYS-05). 40 DS fills (Button / Primary x3, Avatar x5, sidebar active items, dots, brand marks, OK button) rebound to `primary-strong`; 16 literal `#1677ff` text nodes now use `text/link` colour. DS contrast errors 128 -> 67 on the original components.
- Final groups (Colors): avatar 5, bg 6, border 4, brand 6 (primary, primary-strong, pressed, hover, secondary, focus-ring), button 1, camera 2, code 2, control 1, diff 2, hero 2, ide 31, status 9, sidebar 8, syntax 7, syscheck 6, tag 18 (tag/<hue>/{bg,border,text}), text 6, whiteboard 9, auth 12.
- Colours the page still uses as hard-coded literals (text colours, borders) are not bound; that is B/C work (tools bind fills only).

## 2. Tokens
- Old `Tokens` collection (32: 26 number, 3 shadow strings, 4 z-index strings) deleted; shadows and z-index dropped (unsupported in PaperCraft, unreferenced). Empty `Tokens` collection remains (no delete-collection tool; remove in the Variables modal).
- New collection `Spacing & Size` (34 Number variables): `space/{4,8,12,16,24,32,48,64}` (12 added), `radius/{sm4,md8,lg12,xl16,pill999}`, `font/size/{xs12,sm14,md16,lg20,xl24,2xl32,3xl40,4xl48}` (18 dropped), `font/leading/{xs16..2xl40}`, `layout/page-padding(-lg)`, `layout/page-width/{xs480,sm420,md720,lg900,xl1100}` (3 duplicate widths removed). SYS-01/SYS-02.
- Nothing is bound to them yet (tokens are substituted when written). Use `bind_variable` in later work.

## 3. Components (Design System page: 211 -> 260 mains)
- Consolidated: Ant Button Primary/Default/Danger (9), Ant Tag (5), Ant Segmented (3), Ant Table (2) dropped after `swap_instance` of their DS cells to the kept set (19 swaps, 19 mains and 19 cells deleted). Ant Text / Text Danger kept as `Button / Text` and `Button / Danger Text` (SM/MD/LG). AntD showcase groups removed, cells moved into `Group / Buttons`.
- 78 mains renamed to `Group / Variant [/ SM|LG]` (see csv). Button MD height 34 -> 32 (SM 28, LG 40). `Page Header` kept as `Header / Page` (no action slot; not equal to `Header / Admin`, so not merged).
- Fixed: `Header / Candidate` height 0 -> 134 (Header Card height 100% -> fit-content). 8 `Alert / *` 456/478/522 -> 38 high (message text width 0px -> fit-content).
- Moved 68 page-local components onto the DS page (17 Admin, 51 Candidate): each moved with `move_nodes toPageId`, instances on the source page re-pointed with `swap_instance` by componentKey, instance layer names updated. Packed at x=1560, y 8623..13765 on the DS page. `Candidate Message / Success` renamed `Banner / Success` (name clash with the Admin one, different design).
- Skipped (not moved): `T Hdr`, `T Donut`, `T Spark`, `T Stat` (Admin template stubs, 0 uses; equivalents exist in DS: Section Title, Donut, Sparkline, Stat). Delete them in package B.
- Not done: collapse of combinatorial variants (Segmented Active N, Tabs Bar N, 11 sidebars, content-baked Challenge Option/Timeline, colour-named Stat/Sparkline), missing states (hover/disabled etc.), Input/Select/Textarea LG state matrix (SYS-06/07 follow-ups), `Light` mode rename of `Default` (no tool).
- Side effect: while moving, Admin's own copies of `Button / Primary Small`, `Button / Primary Large`, `Button / Default` were moved by mistake (the copy filter used renamed names) and then deleted from DS; no Admin instance pointed at them (nested ones live inside other instances). B should re-import or swap to the DS mains below.
- Admin/Candidate still carry their old copies of the other 211 DS components under old names; B/C: use `component-mapping.csv` and `swap_instance componentKey`.

### Kept component keys (packages B and C)
`componentKey` for every kept or moved main is in `component-mapping.csv` (column 3). Frequently needed:
Button / Primary `64e1d4aaf56fe6a4:5980`, Primary / SM `:5982`, Primary / LG `:5984`, Secondary `:5986`, Secondary / SM `:5988`, Secondary / LG `:5990`, Ghost `:5992`, Danger `:5998`, Dark `:6004`, Icon / Bell `:6010`.
Header / Admin `64e1d4aaf56fe6a4:6480`, Header / Page `:6476`, Header / Candidate `:6489`, Header / Admin / With Action `1c5e5918`.
Form Field / Default `:6663`, Card / Default `:6054`, Alert / Error `:6728`, Input / Default `:6575`, Sidebar / Admin `:6906`, Sidebar / Candidate `:7370`.
(Key prefix is the DS page id `64e1d4aaf56fe6a4`.) Moved components have short keys (e.g. `bd7f34e5`), see csv.

## 4. Annotations
Template `Dev handoff` set on the DS page. 11 notes (dev, a11y, behaviour): one Primary per screen and contrast pair, tag colour by status, Data Table single set, Segmented, Input states, Alert, Tab hit area, Form Field, Header variants, Pill.

## 5. Lint (DS page, byRule)
Before: contrast 128, hard-coded 11, layer-name 7, spacing 57, touch 16, type-scale 42 (list truncated at 200; counts exact).
After (whole page): contrast 194, hard-coded 366, layer-name 49, spacing 180, touch 30, type 135. The rise is the 68 moved local components (contrast 127, hard-coded 225, layer-name 42, spacing 123, touch 9, type 93). The original components: contrast 67 (-61), hard-coded 146 (more literals now match a single variable after merges, so lint flags them), layer-name 7, spacing 57, touch 21 (Button MD 32 < 44, desktop density accepted, documented in annotation), type-scale 42. Remaining contrast: `#52c41a`/white 2.3, `#8c8c8c` 3.4 (placeholders), `#bfbfbf` 1.8 (disabled), tag text pairs, IDE greys: design decisions left for follow-up. `lint-ds-before.json` saved in `A/`.

## Leftovers for the user
Empty `Tokens` collection; mode `Default` not renamed; 15 `Label / *` stubs at x=30000 not deleted; Colors board has no swatch for `brand/primary-strong`; `brand/hover` (#4096ff) with white text fails contrast (hover state unused in DS).
