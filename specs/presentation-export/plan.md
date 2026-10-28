# Presentation export v2: plan

All in `src/io/deck.rs` unless noted; the PDF writer and tools stay as they are.

## Tasks (each with a test in `deck.rs`)
1. **Notes per screen.** `notes_of(doc, screen)`: attached notes whose target is the screen or inside it, in reading order of their targets. Category colour from the project's categories, else the built-ins.
2. **Annotated screen slide.** `Slide::Screen` with notes: the screen fitted in the left 64% of the stage, numbered accent markers at each noted layer's top-left, the note column on the right (number, title bold, first body line muted, wrapped). `NOTES_PER_SLIDE` = 6; more continue on a repeat slide titled `… (notes 2/2)`.
3. **States to an appendix.** Main flow: the first state, its caption `+ N states`; the rest collected per page into Appendix slides after the last page (a divider `Appendix`, rows of two titled `Page · Screen`).
4. **Map slide.** After each divider: up to 12 key screens (one per state group, boards excluded) as a grid of thumbnails with names; more pages of the map when needed.
5. **Open questions.** Notes containing `?`, listed with their screen's name, before the end slide; split at 8 per slide.
6. **Counts.** Contents and dividers say `N screens · M boards`.

## Risks
- Notes on tiny layers: the marker sits on the layer's top-left, clamped inside the screen.
- Long note bodies: one line in the column, cut with an ellipsis (text wrap at fixed width, max rows 2).
- Projects without notes: nothing changes for their slides.

## Data (needs the owner's go-ahead, not code)
- Interview: about 40 screens are named without ` / ` (`Challenges List No Match`), so they don't group as states; rename to `Screen / Challenges / No match`.
- Interview: 76 detached notes (their targets were deleted in the v1 cleanup).
