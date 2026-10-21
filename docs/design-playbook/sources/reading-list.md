# Where design knowledge lives

A map of recurring places to learn from, so agents and humans know where to look
and how much to trust it. "Status" says what has actually been read for this
playbook (2026-10-21). Unread entries are leads, not endorsements: the
descriptions come from general familiarity or search snippets and must be
verified on the page before a rule is based on them.

Trust: P primary or standards body; E credible expert; S secondary or aggregator.

## Practitioner blogs and course sites
| Source | Good for | Level | Status |
|---|---|---|---|
| Josh Comeau, joshwcomeau.com | CSS craft, animation, shadows, interactive explanations | E | 8 articles read (S20 to S27): shadows, transitions, CSS reset, CSS vs JS animation, squash and stretch, starting-style, linear() springs, anchor positioning. Unread: flexbox/grid guides, subgrid, scroll-driven animations, SVG paths, interop-2027 |
| Heydon Pickering, inclusive-components.design | Accessible component case studies | E | 3 of 11 read (S29 to S31) |
| Ahmad Shadeed, ishadeed.com | Layout, defensive CSS, component anatomy | E | defensive CSS read (S28) |
| Sara Soueidan, sarasoueidan.com | Accessibility, SVG, focus, target size | E | lead; her target-size URL guess returned 404, not located |
| Adrian Roselli, adrianroselli.com | Deep accessibility technical posts | E | lead; one guessed URL returned 404 |
| Emil Kowalski, emilkowal.ski and animations.dev | Animation craft for interfaces | E | one article read (S06) |
| Rauno Freiberg, interfaces.rauno.me | Interface details checklist | E | read (S10) |
| Vercel Web Interface Guidelines | Compact rules list | E | read (S11) |
| Steve Schoger / Adam Wathan, Refactoring UI | Visual design tactics | E | video only (V02); book not read |
| Stephanie Eckles, moderncss.dev | Modern CSS recipes | E | lead, unread |
| Lea Verou, Una Kravets, Bram Van Damme (bram.us), Manuel Matuzovic, Eric Bailey, Scott O'Hara, Eric Meyer | CSS and accessibility specifics | E | leads, unread |
| Every Layout, Utopia (utopia.fyi) | Layout primitives, fluid scales | E | Utopia type article read (S13) |

## Magazines and long-form
| Source | Good for | Level | Status |
|---|---|---|---|
| Smashing Magazine | Broad front-end and UX articles; quality varies by author | E/S | two articles read (S32, S33) |
| A List Apart | Durable UX essays | E | one article read (S09) |
| CSS-Tricks | CSS reference and tricks (check article date) | S | unread |
| Nielsen Norman Group | Research-backed UX | P/E | two articles read (S04, S05) |
| Baymard Institute | E-commerce and checkout UX research | E | lead, unread |

## Standards, vendors and design-system documentation
| Source | Good for | Level | Status |
|---|---|---|---|
| W3C WAI (WCAG, APG, COGA, Understanding docs) | Requirements and widget patterns | P | several read (S01 to S03, S07, S08) |
| MDN, web.dev | Platform behavior, Learn Accessibility course | P | S15 read; web.dev module list only (S34) |
| WebAIM, Deque, TPGi | Accessibility testing, research (e.g. WebAIM Million) | E | lead, unread |
| Apple HIG, Material 3, Microsoft Fluent, GOV.UK Design System | Platform conventions and tested patterns | P | leads, unread (Material page returned no content) |
| Radix, React Aria, Atlassian, GitHub Primer, IBM Carbon, Shopify Polaris | Component specs and tokens in production systems | E | Radix color scale read (S12) |

## Video
| Source | Good for | Status |
|---|---|---|
| Kole Jain | UI/UX concepts, interaction mechanics | 1 read in full (V01); 2 without captions |
| Steve Schoger (Refactoring UI) | Live visual redesigns | 1 partly read (V02) |
| Chrome for Developers, A11ycasts | Accessibility mechanics | 1 read (V04) |
| WebAIM conference talks | Accessible design systems | 1 partly read (V06) |
| Kevin Powell, Josh Comeau talks, Matt Pocock-style conference talks | CSS and engineering | leads, unread |

## Inspiration and pattern libraries
Mobbin (real app screens; has an MCP, per V01's sponsor segment, not verified
here), Linear, Raycast, Vercel, Stripe, Notion product UIs. Study the mechanism
(what happens on hover, drag, error), not the look; note the product and the
pattern in the screen brief.

## Keeping current
1. Prefer publication dates in the last 2 years for CSS and browser-support
   claims; support percentages and "Chromium only" notes expire.
2. Re-verify any number before adding it as a rule: fetch the page, record the
   date in `SOURCES.md`.
3. When two sources disagree, keep both in the rule and say why they differ
   (example: CSS-05 vs MOT-02; PAT-03 vs FBK-08).
4. Digest one source per sitting into a rule with an id; update `SOURCES.md`
   status. Unread entries above are the queue.
