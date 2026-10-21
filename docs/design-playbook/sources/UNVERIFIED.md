# Unverified claims

Things the playbook mentions that could not be confirmed from a fetched source.
Do not harden these into rules; check them and move them out.

- "Safe triangle" attribution to Ben Kamens (2013, Amazon mega-dropdown post):
  the post returned HTTP 403. V01 says the pattern was documented in 2013 by a
  person it names (the auto-captions spell it "Cammons"). Pattern itself is
  real and shipped in several products.
- Raymond Chen (2009) on the double-click delay: V01 attributes it; the Old New
  Thing page could not be fetched.
- Material 3 easing and duration token values (MOT-01, MOT-02, tokens.base.json
  `standard`): the M3 page returned no content. Values are from memory.
- Kowalski-style curves `cubic-bezier(0.23, 1, 0.32, 1)` and
  `cubic-bezier(0.77, 0, 0.175, 1)`: widely used community easings, not from a
  standard. Tune by feel (MOT-12).
- 44 x 44 px target (A11Y-09): WCAG 2.5.5 AAA and Apple/Google platform guides;
  not on the fetched 2.5.8 page.
- 1.4.11 Non-text Contrast = 3:1 and 1.4.13 / text-spacing numbers: from the
  standard as remembered; the fetched page did not state them.
- Typography ranges (45 to 75 characters per line, line-height 1.5, 4 px
  spacing base): industry convention (GK).
- Stagger timings (30 to 80 ms) and exit-is-70%-of-enter (MOT-02, MOT-11): GK.
- Deque 2024 study "about 57%" automated detection: from a vendor blog found
  by search; indicative only.
- Whether Craft apps enable the egui `accesskit` feature, whether egui submenus
  tolerate diagonal travel, whether egui exposes the OS reduced-motion
  preference: not checked (platforms/native-egui.md).

## Added in the second research pass
- Adrian Roselli (avoid the title attribute) and Sara Soueidan target-size URLs
  were guessed and returned 404; their content is not in the playbook. PAT-01
  makes the title-attribute claim from Inclusive Components (S29) instead.
- Inclusive Components index page did not state Pickering's guiding principles
  (progressive enhancement, semantics first); do not attribute them to him from
  this research.
- Josh Comeau browser-support percentages (anchor positioning 81%, linear() 88%,
  @starting-style 86%, SVG path transitions 79%) are as of each article's date
  and are stale by definition.
- Josh Comeau's CSS-vs-JS conclusions come from his demos, not independent
  benchmarks (his own caveat).
- S21 timing figures (125/450 ms hover, 100/400 ms dropdown, 250 ms baseline) are
  examples in a tutorial, not recommendations from a standard.
- web.dev Learn Accessibility: only the module index was read.
- Smashing optimistic-UI article (2016): the 1 to 3% failure rate and 2 s window
  are the author's estimates.
