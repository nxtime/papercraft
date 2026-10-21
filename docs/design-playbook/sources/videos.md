# Video notes

Notes in our words, not transcripts (transcripts are copyrighted and are not
stored). Each entry lists what to take away and which rules it feeds.

## V01 Kole Jain, "Why the best software must LIE to you" (9:23)
Thesis: every interaction guesses intent; guesses cost time, certainty or trust.
- AirPod stem waits about 700 ms to distinguish one, two and three taps; the
  phone's separate buttons need no wait. Same reason single-click open vs
  double-click edit needs a wait (documented by Raymond Chen, 2009).
- Table cell: first click highlights immediately because it is right either
  way: no wait needed. Fix by removing the competing action when the secondary
  convenience is not worth the delay.
- Desire paths: pave where people walk. Diagonal pointer travel to a submenu is
  one (Linear, Notion, Attio use an invisible triangle, speed-aware; origin 2013).
- Tooltips: wait once, then adjacent tooltips are instant; Notion's collapsed
  sidebar opens on dwell.
- Drag to reorder: show the guess of the drop position; scroll speed as a
  gradient toward the edge.
- Replace delete confirmation modals with Undo toast or "Recently deleted"
  (30 days). Gmail delays send by 5 s but says "sent" first. Settings pages and
  editors autosave; version history replaced Save/Cancel.
- Still warn for truly irreversible actions (bulk email to 10,000, sending
  money).
- Buttons commit on release (built-in cancel; like camera half-press); menu
  buttons open on press so press-drag-release works. Pull-to-refresh, swipe to
  delete, Cmd-Tab until release show proximity to commit.
Feeds: INT-01..INT-12, FBK-06, FBK-07.

## V02 Schoger, Refactoring UI: Bad About (14:29, partly read)
A live fix of a cramped list page. Techniques: widen the reading column;
give content two-thirds and voting one-third on a 12-column grid; lighter,
smaller icons; group meta with a dot separator; text link for "show more";
stack vote arrows, enlarge the value and raise its contrast; uppercase labels
need letter-spacing; left-align and share a baseline with the title; layered
shadows replace borders; flip gray page / white cards; hard colors (orange)
use light fill with dark, hue-tinted text; one primary action per page.
Feeds: VIS-01..VIS-12.

## V03 Todaro on Yablonski's Laws of UX (10:04, mostly read)
19 principles with origins: aesthetic-usability, Doherty threshold (400 ms),
Fitts, Hick, Jakob, common region, Pragnanz, proximity, similarity, uniform
connectedness, Miller, Occam, Pareto, Parkinson, Postel, serial position,
Tesler, von Restorff and others. Light on evidence for some; use as vocabulary.
Feeds: PSY-02..PSY-11.

## V04 Chrome A11ycasts #03, What is focus? (7:58)
Focus directs keyboard events to an element; Tab order follows DOM order;
only interactive elements belong in it; CSS (float, absolute positioning)
desyncs visual order from tab order, so fix by moving DOM nodes. Test: tab
through your page.
Feeds: A11Y-07, A11Y-08.

## V05 Tartarotti, The Ridiculous Engineering Of Animations (first 40%)
Five rules framework; first two covered: copy nature (spatial consistency,
Disney's 12 principles, easing = slow in slow out), then deliberately depart
from it. Emil Kowalski: interruptibility makes animation feel fast; easing is
the most important parameter. Changing the app-open animation origin changes
the user's mental model of the OS.
Feeds: MOT-01, MOT-04, MOT-05, PSY-13.

## V06 Hawkins, Creating Accessible Design Systems (45% read)
Design tokens carry many accessibility decisions (focus: color change plus
border 1 px to 3 px; error: color plus icon plus text) but not all. Design every
state at the molecule level. Child systems inherit from a parent but must keep
invariants. Responsibility split among UX (structure, error placement, reading
and focus order, alternatives, redundant coding), UI (brand, contrast, type,
motion) and content (copy). Document keyboard, pointer and screen-reader
behavior, and component rules (link vs button).
Feeds: A11Y accessible design systems section, SYS-05..SYS-10.

## V07 Design Tokens for Everyone (17:35, 70% read)
Why tokens: one platform-agnostic source (JSON/YAML) generates SASS, CSS, JS,
iOS and Android variables, ends copy-paste drift, shared vocabulary across
design and dev. Tooling shown is dated; keep the idea.
Feeds: SYS-01, SYS-04.
