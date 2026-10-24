# Interview project audit (read-only)

Source: PaperCraft 0.6.1, project `Interview` (fileName confirmed). Date 2026-10-23. Only read tools used. Screenshots in `specs/interview-cleanup/audit/`. Full old->new colour map: `specs/interview-cleanup/variable-mapping.csv` (161 rows).
Page ids: Design System `64e1d4aaf56fe6a4`, Admin `96e0de09bfd69d1e`, Candidate `943a1809e1077b81`. Node id prefixes: `64e1d4:`, `96e0de:`, `943a18:`.

## 0. Headline numbers

| | Design System | Admin | Candidate |
|---|---|---|---|
| nodes | 4358 | 7699 | 5886 |
| top-level frames | 232 (211 components, 6 sections, 15 label stubs) | 236 (232 components, 4 sections) | 266 (262 components, 4 sections) |
| main components | 211 | 232 (211 copies + 21 local) | 262 (211 copies + 51 local) |
| components with 0 instances | 15 | 171 | 195 |
| real screens | n/a | 39 | 40 |
| lint (whole page): contrast / hard-coded / layer-name / spacing / touch / type | 128 / 11 / 7 / 57 / 16 / 42 | 217 / 49 / 9 / 102 / 39 / 105 | 261 / 19 / 47 / 170 / 24 / 152 |
| lint (screens only) | n/a | 86 / 35 / 0 / 37 / 22 / 67 | 17 / 8 / 0 / 0 / 0 / 24 |

Reading: the "236 artboards" are 98 percent main components parked as top-level frames. Real screens: 4 sections per page. Most lint noise sits in the component copies (Candidate-local IDE components: 244 contrast, 170 spacing, 47 layer-name), not in screens. `Admin Entry Approval v3` and `Candidate Join v3` are clean (0 lint issues, mostly instances) and are the quality reference.
Annotations: `get_annotations` returns `[]`, `get_feedback` count 0. Only business notes are `Route` text labels on Admin screen slots (e.g. `/sessions (create failed)`), 27 loose Text labels on DS, 12 on Candidate.
`lint_design` truncates its issue list at 200; counts above come from `byRule` totals, per section for screens.

## 1. Work package A: Design system (do first)

### A1. Variables, collection `Colors` (161 vars, 1 mode `Default`)
Findings:
- 0 groups (flat `color-*`), 0 descriptions, 0 aliases, 1 mode. Every name carries a redundant `color-` prefix.
- 38 hex values are shared by 2 to 4 variables (44 redundant vars). Examples: `color-ide-card`=`color-ide-code-pane`=`color-ide-input` `#161616`; `color-syntax-*` (7) vs `color-tok-*` (6) identical values; `color-wb-*` (10) vs `color-ide-wb-*` (5) identical; `color-brand-primary`=`color-info`=`color-text-link` `#1677ff`; `color-bg-elevated`=`color-text-inverse` `#ffffff`; `color-cam-hint`=`color-cam-label`=`color-login-card-head`=`color-login-desc` `#ffffff99`.
- 114 near-duplicate pairs (RGB distance <= 12, same alpha). Worst: `ide-card-border` vs `ide-panel` (1.0), `ide-chip` vs `ide-toolbar` (1.0), `brand-secondary` vs `wb-blue` (1.4), `button-dark`/`code-bg`/`ide-rail` (1.7), `diagram-bg`/`login-bg`/`ide-preview` (2.0), seven near-black IDE surfaces `#161616 #181818 #1a1a1a #1f1f1f` (3.5 to 8.7).
- Family sizes: `ide` 46, `tag` 18 (6 hues x bg/bd/fg), `login` 12, `wb` 10, `sidebar` 8, `syntax` 7, `syscheck` 6, `text` 6, `tok` 6, `avatar` 5. Domain families are 55 percent of the set; the core palette is about 30.
- Contrast defect at the root: `color-brand-primary #1677ff` with white text is 4.1:1 (needs 4.5). It causes most "contrast" errors on buttons and sidebar active item. `color-brand-action #0958d9` passes; use it (or darken primary) for filled buttons and text-on-white links. Also failing: `#8c8c8c` on white 3.4 (placeholders), `#5a5a5a` on `#1e1e1e` 2.4 and `#484848` on `#161616` 2.0 (IDE gutter/labels), `#888` on `#252526` 4.3, white on `#bfbfbf` 1.8 (disabled button).
- Binding gap (matters for A and for B/C): in screens, text `color` is hard-coded on every node (Admin 692 local text nodes, Candidate 348), borders hard-coded (317 / 94). Only `background-color` is bound (645 / 271). Of 1030 / 458 hard-coded colours, 1019 / 442 equal an existing variable exactly. Exceptions kept on purpose: Candidate Google logo colours (`#4285f4 #34a853 #fbbc05 #ea4335 #f25022 #7fba00 #00a4ef #ffb900`, 2 nodes each).

Target structure and exact names:
- Collection `Colors`, mode `Light` (rename `Default`). Add mode `Dark` only if the product wants a dark theme; sidebar, IDE, whiteboard, auth surfaces are fixed-dark domain sets, not modes. Do not add a mode without defining every variable in it.
- Groups with `/` (exact, Figma style, no `color-` prefix):
  `brand/{primary,pressed,hover,secondary,focus-ring}`, `text/{primary,secondary,disabled,placeholder,inverse,link}`, `bg/{layout,surface,subtle,selected,overlay,tooltip}`, `border/{default,subtle,strong,hover}`, `status/{success,success-text,warning,warning-text,error,info,star,score-poor,score-warn}`, `tag/{blue,cyan,gold,green,purple,red}/{bg,border,text}`, `avatar/{indigo,orange,pink,purple,teal}`, `sidebar/{bg,border,hover,text,text-muted,text-faint,badge,tag-bg}`, `ide/*` (31 after merge), `syntax/*` (7), `whiteboard/*` (9), `diff/{added,removed}`, `code/{bg,fg}`, `auth/*` (12, from `login-*`), `camera/{label,ring}`, `syscheck/*` (6), `hero/{chip,text}`, `button/dark`, `control/switch-off`.
- Result: 161 -> 136 variables (25 merged). Full mapping in `variable-mapping.csv`. Samples:

| old | new | note |
|---|---|---|
| color-text-primary | text/primary | `#000000e0` |
| color-bg-elevated | bg/surface | same hex as text/inverse; keep both, role differs |
| color-bg-sider | bg/subtle | |
| color-brand-action | brand/pressed | use for filled buttons (contrast) |
| color-button-primary-hover | brand/hover | |
| color-info | status/info | alias of brand/primary if aliases work, else keep |
| color-tag-blue-bd | tag/blue/border | 18 tag vars |
| color-login-* | auth/* | 12 |
| color-tok-* | syntax/* | merge into `color-syntax-*` (6 identical) |
| color-ide-wb-* / color-wb-* | whiteboard/* | merge, 5 identical |
| color-ide-fail / color-ide-pass | diff/removed / diff/added | merge, same hex |
| color-cam-hint | camera/label | merge with `cam-label` |
| color-diagram-bg | ide/preview | merge, `#0d1117` |

- Aliases: not exposed by `get_variables` (no alias field). Agent A tests once on a throwaway variable; if unsupported, merge duplicates and repoint instead.
- Renaming: `set_variables` writes by name, so rename = create new name with same value, repoint every `var(--old)` (use `find_nodes` + `update_styles`), then delete old. Never delete before repointing. Repointing is project-wide: coordinate so B and C start after A is finished.

### A2. Collection `Tokens` (32 vars) -> split into number collections
Findings:
- Mixed types: 26 `number`, 7 `string` (`shadow-sm/md/lg`, `z-base/dropdown/modal/toast`). Shadows are unsupported in PaperCraft (reported as `ignoredStyles`): delete 3. Z-index strings: make number or delete (no z-index in canvas).
- Duplicate values: `page-max-width-default`=`-lg`=900, `-md`=`-narrow`=720, `-wide`=`-xl`=1100 (3 redundant).
- Scale gaps: spacing has 4, 8, 16, 24, 32, 48 but no 12 (used constantly in `gap`/`padding`); `font-size-lg`=18 is off the lint scale (12,14,16,20,24,32,40,48,64); no 32/40/48/64; no line-height tokens; radius has 4, 8, 12 only while screens use 99 (Admin 51x), 999 (Candidate 9x), 5, 6, 7, 32.
- Tokens are substituted at write time (not live), so they only guide new work.

Target collections (all `number`, mode `Default`):
- `Spacing`: `space/1`=4, `space/2`=8, `space/3`=12, `space/4`=16, `space/6`=24, `space/8`=32, `space/12`=48, `space/16`=64 (from `spacing-xs..xxl`; `xs`->`space/1`, `sm`->`space/2`, `md`->`space/4`, `lg`->`space/6`, `xl`->`space/8`, `xxl`->`space/12`).
- `Radius`: `radius/sm`=4, `radius/md`=8, `radius/lg`=12, `radius/xl`=16, `radius/pill`=999.
- `Typography`: `text/xs`=12, `text/sm`=14, `text/md`=16, `text/lg`=20 (drop 18), `text/xl`=24, `text/2xl`=32, `text/3xl`=40, `text/4xl`=48 (old: `font-size-xs..xxl`; `font-size-lg` 18 -> `text/md`or 20, decide once). Add `leading/*` in px (16, 20, 24, 28, 32, 40).
- `Layout`: `page/padding`=24, `page/padding-lg`=48, `page/max-width/{xs=480,sm=420,md=720,lg=900,xl=1100}`; drop `-default`, `-narrow`, `-wide`.
- Colour vs number groups: slash-grouped names collide with colour group `text/`; to avoid confusion name typography tokens `font-size/*` instead if the app lists collections together.

### A3. Components (Design System page: 211)
Findings (counts from `get_components`):
- No group: 25 names (`Avatar`, `Card`, `Search Input`, `Donut`, `Spin`, `Loading Block`, `Empty State`, `Page Header`, `Admin Shell Header`, `Candidate Shell Header`, `Section Title`, `Pagination`, `Input`, `Input Number`, `Date Picker`, `Textarea`, `Select`, `Form Field`, `Modal`, `Drawer`, `Tooltip`, `Skeleton`, `Admin Sidebar`, `Candidate Sidebar`, `Link Field`). Base variant is unnamed while siblings are `Group / Variant`.
- Two parallel systems: `Button / *` (16: Primary, Default, Ghost, Danger, Dark x Small/Large + Notification Bell) heights 34/28/40 vs `Ant Button / *` (15: Primary, Default, Danger, Text, Text Danger x 3 sizes) heights 32/24/40. Same for `Tag`(7)+`Tag Dot`(7) vs `Ant Tag`(5); `Segmented`(9) vs `Ant Segmented`(3); `Data Table`(5) vs `Ant Table`(2). Admin uses both (`Ant Button / Primary` 16x, `Button / Primary` 7x).
- Combinatorial variants: `Segmented / 4 Options Active 1..4` (9+3), `Tabs / Bar 5 Large Active 1..5` + `Bar 3 Active 1..3` (8), `Score Dots / 0..5`, `Stars / 0..5`, `Stat / Blue..Orange Down` (6, 208x132), `Sparkline` (6), `Admin Sidebar` x8 and `Candidate Sidebar` x3 (full 248x900, about 58 nodes each, one per active item, e.g. `64e1d4:6906..7312`, `7370..7422`).
- Content baked into names: `Challenge Option / Two Sum On`, `... / URL shortener Off`, `... / JavaScript closures On` (6), `Timeline / Step 1..3`, `Syscheck Row`, `Tip Card / Read carefully`.
- Inconsistent naming: size as suffix word (`Primary Small`, `Input / Large Filled`), size as number (`Avatar / 30`, `/ 34`, `/ 36`, `/ 40`), `Empty / Simple` vs `Empty State`, `Spin`/`Spin / Small`/`Spin / Large`, `Alert / Info With Icon`, `Input / Focus` vs `Search Input / Focused`, `Tabs / Tab Active Large`.
- Overlap: `Page Header` (`64e1d4:6476`) and `Admin Shell Header` (`6480`) both 1192x102.
- Suspect geometry: `Candidate Shell Header` `64e1d4:6489` height 0 (6 instances on Candidate); `Alert / *` heights 456, 478, 522 for a one-line alert (`6710`, `6716`, `6728`) probably include a long text or a fixed height.
- Missing states (interactive parts with only default/some states): Button (no hover, pressed, focus, disabled, loading), Tabs (no hover/disabled), Checkbox and Radio (no disabled, focus, indeterminate), Switch (no disabled), Select (no focus/error/disabled), Textarea (no focus/error/disabled), Input Number and Date Picker (no states), Pill (no hover/disabled), Data Table Row (no selected), Pagination (no current/disabled), Modal/Drawer (no variants), Tag (no closable).
- Not in DS at all: 21 Admin-local (`T Hdr`, `T Donut`, `T Spark`, `T Stat`, `Funnel Row`, `Funnel Row / Inverse`, `Challenge Usage Row`, `Review Item`, `Upcoming Row / *`, `Donut / Score Distribution`, `Legend Row`, `Criterion Row / Weight`, `Setting Row / On|Off`, `ATS Tile / *`, `Pipeline Row / Dark`, `Message / Success|Error`, `Admin Shell Header / With Action`) and 51 Candidate-local (`IDE Toolbar` 4, `File Tree` 2, `Editor Tabs` 2, `Code Editor` 3, `Console` 4, `Problem Rail` 2, `Diagram Toolbar` 4, `Whiteboard Toolbar` 3, `Theory Toolbar/Chip/Button/Answer` 13, `Progress Tracker` 2, `Check Item` 2, `Tip Card` 3, `Syscheck Row/Step` 6, `Message / Success`). Violates "add to the system first". `T Hdr/Donut/Spark/Stat` are template stubs (names without group).
- Copy drift: `Timeline / Step 3` is 194 tall on DS, 216 on Admin and Candidate. DS-only components with 0 instances (15): `Link Field`, `Entry Request`, `Device Preview`, `Challenge Option`, `Timeline` families (`64e1d4:9635..9858`).
- Hoisted copies: Admin has 171 unused copies of 232, Candidate 195 of 262 (parked top-level frames, about 3000 nodes each). They are stale risk. Only 61 (Admin) and 67 (Candidate) components are placed at all.

Target component naming (rule `Group / Variant` or `Group / Variant / Size`; sizes `SM|MD|LG`):
- Merge Ant and plain sets into one: `Button / Primary`, `Button / Secondary` (old `Default`/`Ant Button / Default`), `Button / Ghost`, `Button / Text`, `Button / Danger`, `Button / Danger Text`, `Button / Dark`, each with `/ SM` and `/ LG` (md is the unsuffixed); `Button / Icon / Bell`. Decide one height set (recommend MD 40, SM 32, LG 48; touch rule 44 on touch only; admin desktop 32-40 ok, state it).
- Add `Button / <Variant> / Disabled` and `/ Loading`; `Input / Default|Focus|Error|Disabled|Filled` x `MD|LG`; same for `Select`, `Textarea`.
- Rename mapping (sample): `Button / Primary Small` -> `Button / Primary / SM`; `Ant Button / Text Danger Large` -> `Button / Danger Text / LG`; `Avatar` -> `Avatar / 32`; `Avatar / 30` -> `Avatar / 30` (keep numeric, all numeric); `Search Input` -> `Input / Search / Default`, `Search Input / Focused` -> `Input / Search / Focus`; `Input / Large Filled` -> `Input / Filled / LG`; `Empty / Simple` -> `Empty / Simple`, `Empty State` -> `Empty / Illustrated`; `Spin` -> `Spin / MD`; `Candidate Shell Header` -> `Header / Candidate`; `Admin Shell Header` -> `Header / Admin`, `Admin Shell Header / With Action` -> `Header / Admin / With Action`; `Page Header` merge into `Header / Admin`.
- Replace 8 `Admin Sidebar / X Active` + 3 `Candidate Sidebar / X Active` with `Sidebar / Admin` (shell, 248x900) composed of `Nav item / Default` and `Nav item / Active` instances; or keep as text-override-free single component and document active item as an override. Cuts about 600 nodes.
- Replace `Segmented / N Options Active K` and `Tabs / Bar ...` with `Segmented / Item Active|Default` and `Tab / Active|Default` (+ `Tab / Disabled`) building blocks; bars are screen-level compositions.
- Replace content-baked `Challenge Option / <title> On|Off` with `Option Row / On|Off`; `Timeline / Step N` with `Timeline Step / Done|Current|Upcoming`.
- Replace colour-named `Stat / Blue..`, `Sparkline / Blue..` by `Stat Card / Default` + accent override (6 -> 1) unless each colour is semantic.
- Move the 72 local components (21 + 51) to the DS page under `Components / Admin` and `Components / Candidate`, named `Group / Variant`, then re-import (do not leave local mains on feature pages).
- DS page: keep `Section / *` boards, delete the 15 `Label / *` stubs after re-layout, set `Section / Components` (1440x14512) to fit-content and re-pack with real heights.

### A4. Order of operations inside A
1. Variables rename/merge/restructure (A1) and token collections (A2).
2. Fix `brand/primary` contrast or switch filled buttons to `brand/pressed`; re-check both after A1.
3. Components rename, merge, add states (A3); fix `64e1d4:6489` height 0 and the Alert heights.
4. Re-import changed components to Admin and Candidate only after B/C owners confirm, because import creates copies and instances lose overrides on re-import (see papercraft-design-system). Publish a list: old name -> new name for `swap_instance`.
5. Write rename mapping files `specs/interview-cleanup/variable-mapping.csv` (done) and `specs/interview-cleanup/component-mapping.csv` (agent A produces).

## 2. Work package B: Admin page (after A)

Sections (all 1440 wide screens):
- `Section / Admin / Sessions` `96e0de:14778`, 10 slots (list, filtered, loading, empty, new, new error, detail, in progress, loading, error), labelled by `Route` text.
- `Section / Admin / Challenges` `96e0de:15868`, 16 screens in unnamed wrappers `Screens > Row 1..8` (`15871, 16182, 16508, 16827, 17344, 17965, 18464, 18873`): list x5, new x4 (blank, template applied, validation errors, templates modal), edit x7 (MCQ, MCQ validation, theory, theory empty, SQL, fullstack, loading). Heights up to 1710, no route labels.
- `Section / Admin / Live Session` `96e0de:19328`, 6 screens (diagrams, whiteboard, whiteboard waiting, code, theory, theory waiting).
- `Section / Admin Entry Approval v3` `96e0de:23776`, 7 blocks (`Block /`, 1440x930), 0 lint issues.
- Not drawn but present as components: Dashboard, Question bank, Schedule, Configuration, Rubric (sidebar items exist, `Funnel Row`, `Upcoming Row`, `Setting Row`, `ATS Tile`, `Criterion Row`, `Pipeline Row`, `Donut` components have no screen on this page).

Findings:
1. Flow structure and naming: 4 different conventions (`Screen Slot / ... > Screen /`, `Screens > Row N > Screen /`, `Block /`, `Section / Admin Entry Approval v3`). v3 duplicates 3 flows already in Sessions: `Block / New session` vs `Screen / New Session` (`96e0de:15304`), `Block / Sessions list` vs `Screen / Sessions List` (`14782`), `Block / Session, in progress` vs `Screen / Session Detail In Progress` (`15604`). Decide canonical (v3 is cleaner) and archive the other.
2. Hard-coded colours on local nodes (screens, excluding instance internals): 1030 (692 text, 317 border, 21 background); 1019 equal a variable exactly. Top: `rgba(0,0,0,0.878)` 295 -> `text/primary`; `#f0f0f0` 214 -> `border/subtle`; `rgba(0,0,0,0.651)` 137 -> `text/secondary`; `#d4d4d4` 65 -> `code/fg`; `#cf1322` 61 -> `status/error`; `#d9d9d9` 52 -> `border/default`; `#ffffff` 36; `#888888` 32 -> `ide/text-muted`; `#1677ff` 17 -> `brand/primary`; `#dddddd` 12, `#5a5a5a` 12, `#1a1a1a` 8, `#555555` 7. Examples: `96e0de:14886`, `14898` (text/primary), `14874`, `14877` (border/subtle), `14895` (brand/primary). No exact match: `rgba(255,255,255,0.122)` 10x (sidebar divider, `96e0de:16757`, `16971`; nearest `sidebar/border` `#ffffff12`) and `rgba(0,0,0,0.059)` 1x.
   Rule: text -> bind with `color:var()`; backgrounds -> bind; borders: check whether 0.6.1 accepts `var()` in border; if not, leave and list them.
3. Contrast (screens): 86 errors. Sessions 9 (`#1677ff` on white 4.1 x8 e.g. `96e0de:14895`; on `#e6f4ff` 3.7 `15083`); Challenges 33 (`#8c8c8c` on white 3.4 x10 `16821`; white on `#bfbfbf` 1.8 x6 `17027`; white on `#1677ff` 4.1 x4 `17057`); Live 44 (`#888` on `#252526` 4.3 x18 `19402`; `#5a5a5a` on `#1e1e1e` 2.4 x12 `19771`; `#484848` on `#161616` 2.0 x3 `19886`; `#555` on `#252526` 2.1 x2 `19419`).
4. Spacing off 4px grid (screens): 84 values. `padding 2px` 37 (tag/pill internals, `96e0de:14888`), `padding 10px` 22 (`17944`), `gap 6px` 11 (`17584`), `padding 6px` 6, `padding 1px` 4, `padding 7px` 4. Lint: 37 warnings (Sessions 9, Challenges 28).
5. Type: off-scale sizes in text nodes: `13px` 66 (Live, `96e0de:19393`), `15px` 1 (`19587`). Distribution of text sizes: 12px 219, 13px 66, 14px 353, 15px 1, 16px 14, 20px 11, 24px 27, 32px 1. Lint 67 `type-scale` (info). Radii in use: 4x40, 5x5, 6x2, 8x110, 12x95, 32x2, 99x51 (pill should be 999 via `radius/pill`).
6. Touch/size: 22 `touch-target` warnings, all in Challenges: `Button / Add file` 32px x10 (`96e0de:16800`), `Button / Add test` x5 (`16809`), `Button / Add question` x4 (`17918`). Decide: 32px accepted on dense desktop admin, or grow to 40. Document in DS.
7. Detached copies that should be instances (frames carrying a component name but not linked), Admin:
   - `Card` 4: `96e0de:15375, 15467, 15564, 15675` -> `Card / Default` (new name).
   - `Alert / Error` 1: `15470` -> `Alert / Error`.
   - `Page Header` 6: `23841, 23959, 24051, 24195, 24333, 24525` -> `Header / Admin`.
   - `Form Field` 3: `23847, 23851, 23885` -> `Form Field / Default`; `Input / Large` 1: `23849`.
   - `Button / Primary Large` 5: `23893, 23987, 24215, 24337, 24455` -> `Button / Primary / LG`.
   - `Button / Default Large` 5: `23895, 23983, 24219, 24453, 24552` -> `Button / Secondary / LG`.
   - `Button / Danger` 1: `24554` -> `Button / Danger`.
   All 26 are in the v3 section. Replace with `swap`/new `<x-paper-instance>` and override text.
8. Repeated local structure that should be components (not in DS): sessions and challenges tables built from locals (`Td` 36, `Cell / Type`/`Cell / Status`/`Cell / Actions` 15 each, `Data Table / Row` has 0 instances on Admin); form fields in Challenge New/Edit repeated 10x (`Field / Title`, `Field / Type`, `Field / Prompt`, `Field / Time limit`, `Field / Memory`) instead of `Form Field` instances; `Card Header` 44, `Card Body` 48, `Divider` 44, `Card / Editor` / `Card / Prompt` / `Card / Type & details` 10 each (Card has no header/body slots); `Markdown Editor` 10 with 11 toolbar icons x10 (`text-b`, `text-italic`, `link`, `quotes`...); `Tag` 14 local; `Toolbar` 15. Instance totals: 357 instances across 54 components on screens, 2277 local nodes.
9. Naming: vector icons named `Path` 202; table parts `Td` 36; icon buttons `Close` 26, `Remove` 25, `Plus` 22; `Row 1..8` wrappers; v3 title text node named `Text` (`96e0de:23777`, `24451`); default-named Line/Arrow: `96e0de:117..137` (6), `760`; section name `... v3`. Lint `layer-name` 9 (all in component copies).
10. Content inconsistency: sidebar user footer is `Elena Vasquez / elena@netcracker.com` on `14782` (email wraps to a second line and clips: `elena@netcracker.` / `com`) but `Admin / admin@test.local` on `24133`; table on `14782` has 1px cell borders and an empty header cell for Actions.
11. Missing annotations (business rules visible in the designs, none documented): session status lifecycle (Draft, Open, In progress, Submitted, Graded, Cancelled) and which actions each allows; entry approval (single-use link, re-entry request, decline dialog, revoke access); live view tabs and the waiting states; challenge types and per-type validation (MCQ, Theory, SQL, Fullstack), archived challenges, templates modal; time limit and memory limit units. Add `add_feedback` notes (or `Note` text blocks) on each section; create 6 to 8 notes.
12. Structure: sections sit at world x = 152072 and 156600 (Sessions, Live) and y = 0 / 12000 / 23000; page is not navigable as one board. Re-pack sections in a grid.

Admin tasks (assign to agent B):
- Re-bind colours (item 2) and fix contrast (item 3) after A1 lands.
- Replace the 26 detached frames with instances (item 7); replace local table, form, card structures with instances after A adds `Table / Row`, `Form Field`, `Card` slots (item 8).
- Normalise names, wrappers, v3 duplicates (items 1, 9); fix sidebar content (item 10).
- Snap spacing and type (items 4, 5) using the new token scale; then delete unused component copies (171) and re-import only the used ones (61) above the page content.
- Add annotations (item 11). Lint target: 0 errors, 0 warnings on screens.

## 3. Work package C: Candidate page (after A, parallel with B)

Sections:
- `Section / Candidate Challenge` `943a18:8084`, 15 blocks (`Block /`, 1440x930): code x5 (idle, running tests, passed, failed low time, submitted), diagrams, whiteboard x2, theory x2, loading, load error, MCQ form, MCQ submit error, text form + status badges. Wrapper `Screens` `943a18:8086`. Mostly instances (code blocks: 7 local + 7 instances each).
- `Section / Candidate Access` `943a18:16180`, 7 blocks (login, login error, invite, invite loading, invite error, verify, verify error); wrapper `943a18:16182`.
- `Section / Candidate Session` `943a18:16508`, 9 blocks (session home x3, system check x3, review x3); wrapper `943a18:16510`.
- `Section / Candidate Join v3` `943a18:20014`, 9 blocks, 0 lint issues; 3 rows `20016, 20191, 20354`.

Findings:
1. Duplicated flow: `Block / Invite` exists in Access (`943a18:16411`) and Join v3 (`943a18:20017`); `Block / Waiting*`, `Approved`, `Declined`, `Link used/expired` overlap with Access invite/verify states and with Session system check. Pick canonical (v3 clean), archive others.
2. Hard-coded colours on local nodes: 458 (348 text, 94 border, 16 background); 442 exact-match a variable. Top: `rgba(0,0,0,0.878)` 111 -> `text/primary` (`943a18:9863`, `9879`); `rgba(0,0,0,0.651)` 107 -> `text/secondary` (`8089`, `8312`); `#f0f0f0` 39 -> `border/subtle`; `#ffffff` 30; `rgba(255,255,255,0.6)` 20 -> `camera/label` (`16203`, `16210`); `#d9d9d9` 14; `#d4d4d4` 11; `#cf1322` 10; `#666666` 9 / `#cccccc` 9 / `#888888` 7 -> `ide/text-faint`, `ide/text`, `ide/text-muted`; `#ffccc7` 7 -> `tag/red/border`; whiteboard colours (`#e0e0e0 #6ab0ff #ff9f43 #c792ea` 4 each -> `whiteboard/*`, `943a18:9434..9452`). Google logo colours stay literal (16 nodes).
3. Contrast (screens): 17 errors: white on `#1677ff` 4.1 x13 (login, buttons: `943a18:16258`, `17038`, `16750`, `16190`); white on white 1.0 x4 (`943a18:16566`, hidden or invisible text; check); Candidate-local component copies add 244 more (IDE greys on dark, same as Admin Live).
4. Type: off-scale sizes `13px` 11, `15px` 2, `19px` 2 (`943a18:9670`), `22px` 4 (`16765`), `26px` 1 (`16555`), `30px` 4 (`16243`); lint 24 info. Text sizes: 12px 55, 13px 11, 14px 218, 16px 30, 20px 8, 24px 13, 30px 4. Radii: 4x24, 6x9, 7x5, 8x43, 12x21, 32x4, 99x18, 999x9 (mixed pill).
5. Spacing: 0 off-grid in screens (clean), but component copies on this page carry 170 `spacing-grid` and 47 `layer-name` warnings (IDE components). Agent A should fix these in the DS copies, agent C only re-imports.
6. Detached copies that should be instances (Candidate):
   - `Card` 21: `943a18:16415, 16433, 16452, 16473, 16491, 17035, 17133, 17200, 20030, 20080, 20107, 20146, 20166, 20206, 20224, 20270, 20294, 20332, 20368, 20394, 20420` -> `Card / Default`.
   - `Alert / Error` 4: `16358, 16465, 16497, 17207`.
   - `Button / Default Large` 5: `20158, 20252, 20377, 20403, 20429` -> `Button / Secondary / LG`; `Button / Primary Large` 2: `20063, 20291`.
   - `Textarea` 1: `10002`; `Pagination` 1: `17090`; `Avatar` 1: `20032`; `Form Field` 1: `20053`; `Input / Large` 1: `20055`.
   Total 37, 31 of them in Join v3 / Access.
7. Repeated local structure: `Block / Login` is 100 local nodes with 4 instances (twin `Login error` 105 nodes); `Brand`/`Mark` 14x, `Header` 12, `Th` 12, `Td` 18, `Label Row` 9, `Icon Disc` 8, `Feature Icon/Glyph/Text` 6 each, `Icon / phosphor:lifebuoy` 9x -> new components `Auth / Brand`, `Auth / Feature Row`, `Field / Labelled`. Candidate Join v3 blocks have 0-1 instances each and 23-44 local nodes (`Link used`, `Link expired`, `Declined`, `Done`: 23 local, identical structure -> one `Status Card` component with 4 variants).
8. Naming: `Block /` with punctuation and colons (`Block / Code: idle`, `Block / Session, request pending` style), `Screens` / `Screens Row N` wrappers, section `... v3`, `Block / Invite` twice. Admin uses `Screen /`; unify across pages (`Screen / <Flow> / <State>`). Lint `layer-name` in screens 0.
9. Missing annotations: join/entry states (waiting, no response, camera blocked, approved, declined, link used, link expired), time limit and low-time behaviour (`44:12` timer, `Code: tests failed, low time`), theory partial submit vs submit all, system check gating (camera/mic failed), review step empty/error, MCQ submit error; 12 loose Text labels exist, 0 annotations. Add 8 to 10 notes.
10. Dark IDE surfaces (46 `ide-*` vars) are used by few screens (Challenge section, 14 blocks); after A, bind IDE text/border/bg to `ide/*` and `whiteboard/*`.

Candidate tasks (agent C): same pattern as B: re-bind colours, replace the 37 detached frames, extract login/status/form patterns once A has added them, unify names and flow duplicates, annotate, re-pack, delete unused copies (195 of 262), re-import used ones (67).

## 4. Execution order and conflict rules

1. A first, alone: it changes variable names project-wide, token collections, component names and mains on the DS page. Nothing on Admin or Candidate should be edited before A signs off (rename/repoint is project-wide and would conflict).
2. After A: B (Admin page only) and C (Candidate page only) in parallel. They share no nodes. Each may only touch its own page, only call `import_components` for its own page, only `add_feedback` on its own nodes. Variable and token edits are forbidden for B and C: if a colour is missing, B/C list it in a request file and A adds it before the next pass.
3. Run `finish_working_on_nodes` after each work package. Expected end state: lint 0 errors / 0 warnings on screens; every hard-coded colour that has a variable exact match bound (text and background); 0 detached frames named after components.
4. Safety: the user's app is live (see memory: never close it); use `undo` only for the package's own steps; take a screenshot before and after each section.

Screenshots taken (`specs/interview-cleanup/audit/`): `admin-sessions-list.png`, `admin-challenge-templates-modal.png`, `admin-live-diagrams.png`, `admin-v3-session-in-progress.png`, `cand-diagrams-tab.png`, `cand-login.png`, `cand-v3-waiting.png`, `ds-colors.png`, `ds-components.png` (page-wide, unreadable by design).

## Design playbook (user, 2026-10-23)

All packages also follow the design playbook (`/Volumes/Storage/Projects/design-playbook`, CLI `playbook`): A uses principles/07 (tokens) and 10 (components); B and C use `checklists/design-review.md` and `checklists/a11y-audit.md` per screen, `playbook contrast` for every text/background pair, and cite rule ids in their reports.

## Annotations (decided 2026-10-23)

* **Admin and Candidate**: page template `Product spec` (`set_annotation_template {pageId, template: "Product spec", scope: "page"}`). Use:
  * `rule` – eligibility, approval and timing rules, limits (e.g. "Candidate can join 10 min before start", "Admin must approve entry before the session").
  * `behaviour` – states and transitions: pending / approved / rejected, empty, loading, error, what happens on each action.
  * `data` – where a field comes from and which role (Admin / Candidate) sees or edits it.
  * `content` – copy constraints: max length, tone, localisation.
* **Design System page**: template `Dev handoff`: `dev` usage rules (one primary button per screen, which Tag for which status), `a11y` (contrast pairs, 44 px targets), `behaviour` (component states), `data`.
* How: one batched `annotate` call per frame (or per section), notes short (first line = label, ≤ 60 chars; body ≤ 2 lines), only where a rule is not obvious from the UI. Read the template first with `get_annotation_template`. Accessibility defects on screens are fixed, not annotated.
* Custom templates cannot be created over MCP in 0.6.1 (no upsert tool), so built-ins are used; a project-specific "Interview flows" template is a 0.6.2 follow-up.

## Annotation content rules (user feedback, 2026-10-23)

* Objective and actionable: each note states a rule someone must follow or a fact they need, not history ("Replaces Ant Button" is not a note; put it in the changelog).
* Label = the rule itself, complete, ≤ 60 chars ("One primary button per screen"). Body = condition → outcome with concrete values ("Use Primary for the main action only; others use Secondary or Ghost. Danger only for destructive, irreversible actions."). Values, limits, states, roles; no filler.
* One note per distinct rule, attached to the exact component or element it governs (not the frame), so the line is short.
* Prefer fewer, high-value notes over many: rules that change behaviour, access, data or compliance; skip what the UI already shows.
* After adding notes, run Tidy (re-layout) and check the screenshot: cards next to targets, lines not crossing components, text complete.
