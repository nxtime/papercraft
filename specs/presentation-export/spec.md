# Presentation export

## Problem
Exporting a project as PDF gives one bare page per frame: no cover, no titles, no order beyond the canvas, and boards whose content sits on top of them as sibling layers came out empty. Sending designs to a client or team means rebuilding the deck by hand in another tool.

## Goal
One export produces a client-ready PDF deck of a project's screens, the way a freelance designer presents work.

## Non-Goals
- Authored narrative (problem statements, research, rationale paragraphs): the deck uses what the project knows (names, pages, sizes).
- Editable slide documents (Keynote, PowerPoint) or slide editing in the app.
- Theming the deck from the project's variables.

## User Stories
- As a designer, I want to export a page or the whole project as a presentation, so that I can send it to a client without rebuilding slides.
- As an agent, I want a tool and a CLI flag for the same export, so that I can deliver designs as a deck.
- As a reviewer, I want each screen on its own slide with its name, so that I can refer to it in feedback.

## Acceptance Criteria
- [ ] Given the Export section, when the format is Presentation and the scope is All pages, then one PDF is written with a cover, a contents slide, a divider per page that has screens, one slide per screen, and a closing slide.
- [ ] Given a page with declared screens, then the deck shows those screens; otherwise its top-level frames of at least 300 × 300.
- [ ] Given narrow, tall screens (side panels), then up to five share a slide, each with its name.
- [ ] Given a board frame with layers laid over it as siblings, then its slide shows those layers (as the canvas does).
- [ ] Every slide but the cover has the deck title and a page number; screen slides show the page name, the screen name and its size.
- [ ] Screens stay vector in the PDF (scaled, not rasterised).
- [ ] `papercraft --export deck.pdf --format presentation --scope all <project>` and the MCP tool `export_presentation {pageIds?, path?, title?, subtitle?, author?}` produce the same deck.

## v2: presentable like a designer's deck
Research (client-presentation guides, the pptx skill's slide rules): show the big picture before details, annotate key decisions, keep filler (states, look-alike screens) out of the main flow, close on open questions, keep one visual system.

- [x] Given a page, after its divider, then a map slide shows its key screens as a captioned thumbnail grid in reading order.
- [x] Given a screen with notes on it (or on its layers), then its slide shows the screen with numbered markers on the noted layers and a column of the numbered notes (title, first line of the body, the category's colour); more notes than fit continue on a second slide of the same screen.
- [x] Given states of one screen (`Sessions / List`, `Sessions / Empty`), then the main flow shows the first, captioned with how many states it has, and the others are in an Appendix section at the end, two per slide.
- [x] Given notes that ask a question (a `?`), then an Open questions slide before the end lists them with their screen.
- [x] Given a page with boards, then the contents and its divider count screens and boards apart.
- [x] Detached notes (their layer was deleted) are left out.

## Constraints
- Stack: Rust, the existing PDF writer (`io/pdf.rs`); no new dependencies, no browser.
- Performance: a 120-screen project exports in seconds.
- Compat: PDF 1.7, readable in Preview, Acrobat and browsers.

## Open Questions
None: defaults chosen (dark slides, 1920 × 1080, deck title from the file name in the app and CLI).
