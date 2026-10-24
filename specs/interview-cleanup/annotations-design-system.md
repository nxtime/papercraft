# Annotations: Design System (page 64e1d4aaf56fe6a4, template Dev handoff)

Result: 29 notes, all attached to the exact component cell (Section / Components, 64e1d4:7475). Old pass had 14 notes (not 11).

## Old notes
- Rewritten in place (3): #10808 Button / Primary, #10809 Button / Primary / LG, #10815 Data Table / Row Hover.
- Deleted (11): #10810 (history: "Replaces Ant Button"), and 10 notes that sat on whole group frames (Tag, Submission Status Badge, Pill, Segmented, Page headers, Data Table, Input, Form Field, Alert, Tabs). Their useful rule was re-added on a single cell.

## Notes by group
- Buttons: One Primary per view; Primary fill #0958d9 not #1677ff; Danger only destructive + confirm; SM sizes under 44 px; Bell icon-only needs aria-label; states beyond default (question).
- Card: hover only on clickable cards.
- Tag / Submission badge / Pill / Segmented: status colour map; badge for code runs only; Pill filter behaviour; Segmented variant by option count.
- Score Bar: cut-offs (question); number beside bar.
- Loading / Empty / Alert rule; Headers (Admin vs Candidate); Exit Session (question).
- Data Table: Header Row + Rows + Footer, zero rows = Empty; Row states (Selected missing).
- Input / Checkbox / Radio / Switch / Form Field: placeholder contrast 3.4:1; focus/error/disabled; 44 px row hit area; Switch vs Checkbox; Form Field wrapper; error linked to field.
- Overlays: Modal button order; focus trap + Esc; Tooltip on focus.
- Alert: icon + text, role alert/status. Tabs: 48/50 px, active not colour alone. Sidebar: variant = active route.

## Open questions for the team (marked "? (not in design)")
1. Button hover, pressed, focus, disabled, loading states.
2. Pill: single or multi select; hover and disabled.
3. Score Bar High/Mid/Low thresholds.
4. Exit Session: confirmation and what happens to unsaved answers and the timer.
5. Tag Purple and Cyan: which status.
6. Data Table row Selected state.

## Placement
- annotate tidy on frame 64e1d4:7475 applied. get_screenshot cannot render notes (they sit outside the frame; no annotations option), so verification was geometric: card rect (target + position, 280 wide, estimated height) vs every Cell frame rect: 0 overlaps for all 29. Cards are in the left gutter (x = -280) or in whitespace inside the Buttons/Header/Table areas.
- Not annotated: foundation sections (Colors, Typography, Spacing, Radius, Page Layout; being renamed by the token package) and the loose component copies at page root.
- Layer name issue (not touched): Page headers cell 64e1d4:8256 is named "Cell / Header / Admin" but holds Header / Page (duplicate name with 8262).
