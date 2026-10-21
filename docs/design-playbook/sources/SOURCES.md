# Sources

Every rule cites these ids. Trust levels:

- **P**: primary (standard body, official docs, original author)
- **E**: expert practitioner writing or talk (credible, opinionated)
- **S**: secondary summary (verify before relying on specifics)
- **GK**: general knowledge, no fetched source (industry convention). Rules with no
  cited id are GK. Treat GK numbers as defaults to test, not requirements.

"Read" says what was actually consumed on 2026-10-21. A source listed as
fetched through a summarizing tool may have lost detail; quote nothing from
this list as a verbatim citation.

## Standards and official guidance
| id | Source | Level | Read |
|---|---|---|---|
| S01 | W3C, What's New in WCAG 2.2 https://www.w3.org/WAI/standards-guidelines/wcag/new-in-22/ | P | full |
| S02 | W3C, Understanding SC 1.4.3 Contrast (Minimum) https://www.w3.org/WAI/WCAG22/Understanding/contrast-minimum.html | P | full (1.4.11 threshold not stated on that page: 3:1 is from the standard, GK) |
| S03 | W3C, Understanding SC 2.5.8 Target Size (Minimum) https://www.w3.org/WAI/WCAG22/Understanding/target-size-minimum.html | P | full (44 px figure not on that page) |
| S07 | W3C APG, Read Me First https://www.w3.org/WAI/ARIA/apg/practices/read-me-first/ | P | full (the "five rules" and keyboard sections were not on this page) |
| S08 | W3C, Making Content Usable for People with Cognitive and Learning Disabilities (COGA) https://www.w3.org/TR/coga-usable/ | P | first ~100k of 470k chars (the 8 objectives) |
| S15 | MDN, prefers-reduced-motion https://developer.mozilla.org/en-US/docs/Web/CSS/@media/prefers-reduced-motion | P | first 100k of 130k |
| S16 | Inclusive Design Principles https://inclusivedesignprinciples.info/ | E | full |
| S17 | W3C DTCG, Design Tokens Format Module (2025.10 draft) https://www.designtokens.org/tr/drafts/format/ | P | first 100k of 115k; marked draft, "do not implement yet" |
| S18 | llms.txt spec https://llmstxt.org/ | E | full |
| S19 | egui lib.rs (AccessKit re-export) https://github.com/emilk/egui/blob/master/crates/egui/src/lib.rs | P | excerpt only |

## Practitioner articles and guidelines
| id | Source | Level | Read |
|---|---|---|---|
| S04 | NN/g, 10 Usability Heuristics https://www.nngroup.com/articles/ten-usability-heuristics/ | P | full |
| S05 | NN/g, Response Times: The 3 Important Limits https://www.nngroup.com/articles/response-times-3-important-limits/ | P | full |
| S06 | Emil Kowalski, Good vs great animations https://emilkowal.ski/ui/great-animations | E | full |
| S09 | A List Apart, Never Use a Warning When You Mean Undo https://alistapart.com/article/neveruseawarning/ | E | full |
| S10 | Rauno Freiberg, Web Interface Guidelines https://interfaces.rauno.me/ | E | full (via summary) |
| S11 | Vercel, Web Interface Guidelines https://vercel.com/design/guidelines | E | full (via summary) |
| S12 | Radix UI, Understanding the scale https://www.radix-ui.com/colors/docs/palette-composition/understanding-the-scale | E | full |
| S13 | Utopia, Designing with fluid type scales https://utopia.fyi/blog/designing-with-fluid-type-scales/ | E | full (no rem/zoom discussion on the page) |
| S14 | Refactoring UI (Wathan, Schoger), via third-party summaries (cdn.jsdelivr.net claude-code-skills, alexanderweichart.de) | S | search snippets only; verify against the book |

## Articles added in the second research pass (2026-10-21)
| id | Source | Level | Read |
|---|---|---|---|
| S20 | Josh Comeau, Designing Beautiful Shadows in CSS https://www.joshwcomeau.com/css/designing-shadows/ | E | full |
| S21 | Josh Comeau, An Interactive Guide to CSS Transitions https://www.joshwcomeau.com/animation/css-transitions/ | E | full (via summary; timing numbers are his examples, not mandates) |
| S22 | Josh Comeau, A Modern CSS Reset https://www.joshwcomeau.com/css/custom-css-reset/ | E | full |
| S23 | Josh Comeau, CSS vs. JavaScript (animation performance) https://www.joshwcomeau.com/animation/css-vs-javascript/ | E | full; conclusions from his demos, not benchmarks |
| S24 | Josh Comeau, Squash and Stretch https://www.joshwcomeau.com/animation/squash-and-stretch/ | E | full |
| S25 | Josh Comeau, The Big Gotcha With @starting-style https://www.joshwcomeau.com/css/starting-style/ | E | full |
| S26 | Josh Comeau, Springs and Bounces in Native CSS https://www.joshwcomeau.com/animation/linear-timing-function/ | E | full |
| S27 | Josh Comeau, Getting Started with Anchor Positioning https://www.joshwcomeau.com/css/anchor-positioning/ | E | full; no accessibility discussion |
| S28 | Ahmad Shadeed, Defensive CSS https://ishadeed.com/article/defensive-css/ | E | full |
| S29 | Heydon Pickering, Inclusive Components: Tooltips and Toggletips https://inclusive-components.design/tooltips-toggletips/ | E | full |
| S30 | Heydon Pickering, Inclusive Components: Notifications https://inclusive-components.design/notifications/ | E | full |
| S31 | Heydon Pickering, Inclusive Components: Toggle Buttons https://inclusive-components.design/toggle-button/ | E | full |
| S32 | Smashing Magazine, True Lies Of Optimistic User Interfaces (2016) https://www.smashingmagazine.com/2016/11/true-lies-of-optimistic-user-interfaces/ | E | full; old |
| S33 | Smashing Magazine, Accessible Target Sizes Cheatsheet (Steven Hoober, 2023) https://www.smashingmagazine.com/2023/04/accessible-tap-target-sizes-rage-taps-clicks/ | E | full |
| S34 | web.dev Learn Accessibility (module list) https://web.dev/learn/accessibility | P | index page only |

See `reading-list.md` for the wider map of blogs, magazines and design-system
sites, with read status.

## Videos (see videos.md for notes per video)
| id | Video | Level | Read |
|---|---|---|---|
| V01 | Kole Jain, Why the best software must LIE to you https://www.youtube.com/watch?v=S8DcTJm8pYM (9:23) | E | full transcript |
| V02 | Steve Schoger, Refactoring UI: Bad About https://www.youtube.com/watch?v=S6-q5BheEYU (14:29) | E | about 65% |
| V03 | Joseph Todaro, The Laws of UX, 19 Psychological Design Principles https://www.youtube.com/watch?v=fYs2Mdyasuc (10:04) | E/S | about 85% |
| V04 | Chrome for Developers, What is Focus? A11ycasts #03 https://www.youtube.com/watch?v=EFv9ubbZLKw (7:58) | P | full |
| V05 | Enrico Tartarotti, The Ridiculous Engineering Of Animations (with Emil Kowalski) https://www.youtube.com/watch?v=z3Lo5QT7nME (18:36) | E | first ~40% (before the sponsor read) |
| V06 | Karen Hawkins (Level Access), Creating Accessible Design Systems, WebAIM conference https://www.youtube.com/watch?v=S_uit5kCtP0 (1:00:09) | E | about 45% |
| V07 | Talk.CSS #39, Design Tokens for Everyone https://www.youtube.com/watch?v=Nk54CjdOulI (17:35) | E | about 70%; tooling is dated (PostCSS) |

## Still unread: pick up when needed
Transcripts downloaded but not digested: Schoger CSS Day 2019 talk
(7Z9rrryIOC4, 44 min), Marcy Sutton keyboard accessibility (wNunEYxZLPE),
Brad Colbow on contrast (b-PqO-ILcYo). No captions retrievable (rate-limited or
absent): Kole Jain "The secret behind weirdly perfect UI designs" (neE6wOuBIP8),
Kole Jain "Every UI/UX Concept Explained" (EcbgbKtOELY), Hatch Conference
"Design Systems in the Age of Agents" (JeJhPtsbIGA).

## Research protocol when the playbook has no answer
1. `python3 scripts/search.py "<question>"`; follow rule ids to sources.
2. Prefer P sources: W3C WAI, MDN, platform HIG/Material, NN/g. Fetch the page,
   do not rely on memory for thresholds.
3. For a pattern, read the WAI-ARIA APG pattern and Inclusive Components.
4. For taste and craft, study real products (Mobbin, Linear, Raycast, Vercel,
   Stripe) and note the mechanism, not the look.
5. Add the finding to the right principle with a new rule id, add the source row
   here, and run `python3 -I -m unittest discover -s tests`.
6. If a claim cannot be fetched, put it in `UNVERIFIED.md`, not in a rule.

## Other places to look (not yet read)
See `reading-list.md`: Apple HIG, Material 3, GOV.UK, remaining Inclusive
Components case studies, Laws of UX site, Deque, TPGi, Adrian Roselli, Sara
Soueidan, WebAIM, remaining Josh Comeau articles.
