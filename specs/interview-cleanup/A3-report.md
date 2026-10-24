# Package A3 report: DS additions, DS defects, remaining swaps, layout fixes

Date 2026-10-23. Project `Interview` (verified). Screenshots in `A3/` (before-*/after-*). Undo is per page (use `pageId`).

## 1. Design System additions (DS page, lint 0 on each)
- `Card / Container` (64e1d4:10850): empty shell, padding 24, gap 16, radius 12, border, fill `color/bg/surface`. Title slot not added (children go straight in).
- `Button / Primary / LG / Icon` (10830), `Button / Secondary / LG / Icon` (10834): 40 px, leading plus icon (phosphor:plus, the set the old copies used; not Lucide). Instances cannot swap icons, so three extra Secondary variants for the icons the Candidate copies used: `... / Icon / Bell Ringing` (10838), `/ Retry` (10842), `/ Email` (10846). Placed right of `Button / Icon / Bell`, y=40.
- Alerts: `Alert / Error / With Icon` (6730) already existed (A naming); reused, no new `/ Icon` component. Info/Success/Warning icon variants exist too, not needed.
- `Pagination / Single` (10853): one active page, placed right of `Pagination / Default`.

## 2. DS defects fixed
- `Sidebar / Admin` user footer (main + the 16 Admin instances): User Meta now shrinks (width 100%, minWidth 0, clipped), name row wraps so the ADMIN tag drops under the name instead of overlapping, email sits in a new `User Email Clip` frame (16 px, overflow hidden) so it is cut, not wrapped. Clip is a hard cut (no ellipsis; text-overflow is unsupported). Instances do not follow structural main edits, so the clip frame was added to each of the 16 instances by hand.
- `Input / Number`: Value text width 0 -> 100% grow (main) and on the 25 Admin instances. No more wrap.

## 3. Swaps (instances now accept children: move_nodes into an instance works)
- Admin: 4 Cards (15375, 15467, 15564, 15675) -> `Card / Container` instances with their form content moved inside (Card Body wrapper flattened; padding/gap copied; heights unchanged, identical screenshots). Alert 15470 -> `Alert / Error / With Icon` (text "Create failed").
- Candidate: 21 Cards -> `Card / Container` (same flatten). 8 Join cards had fixed heights; fit-content made pairs unequal, so pairs set to the taller value (466/530). 3 icon Alerts (16358, 16465, 17207) -> `Alert / Error / With Icon` (text grows, wraps). 5 icon Secondary buttons -> the matching icon variants (width fill; 44 -> 40 px high). Pagination 17090 -> `Pagination / Single` (fill, right aligned, 16 px top padding).
- Extra: Admin v3 `New session` LG button (24627) got its plus icon back (`Button / Primary / LG / Icon`). Other LG buttons that lost an icon in package B were not identified; not restored.
- Components uses: Card / Container 25, Pagination / Single 1, icon buttons 5.

## 4. Layout fixes
- Candidate Session home: screen/main/content/page-center set fit-content (900 -> 936 px), sidebar stretches (100%), nav height fixed so footer sits at the bottom. Last card row no longer clipped. Other Session screens untouched.
- Admin 7 tall edit screens (Fullstack, Sql, Theory Empty, Theory, Mcq Validation, Mcq, New Template Applied): sidebar wrapper and instance stretch to screen height, nav sized so footer is at the bottom (no more vertical centring).
- v3 decline block (Entry Request / Waiting, 592 px variant on the Decline dialog screen): Who column shrinkable with 8 px right padding, Meta row wraps, so helper text no longer runs under Deny. Fixed in DS main and in the 592 instance (card 82 -> 106 px). Padding is a literal 8 (no bind tool in this build).

## Lint after
DS new components 0; Sidebar / Admin 2 info type-scale (13px, pre-existing). Admin: touch-target 22 (accepted by B), else 0. Candidate: hard-coded 8 (Google logo, intentional), else 0.

## Skipped / notes
- No Lucide icons (old copies used phosphor); no `/ Icon` alert rename; no title slot on Card / Container.
- Pre-existing, seen not fixed: Candidate Access `Send code` button clipped ("Send co") and verify-error alert wider than its card; Session home `Enter session` button clipped.
- Instances of `Entry Request / Waiting` at 656/1068 px were not changed (not overflowing).
