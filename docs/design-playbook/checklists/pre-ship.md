# Pre-ship gate for a UI change

Run before saying a UI change is done. Items cite the rule that explains them.

1. Built from tokens; no raw hex, no off-scale spacing (SYS-01, VIS-04).
2. All component states render in light and dark (FBK-02, VIS-13).
3. Keyboard-only walkthrough of the changed flow works (A11Y-07, A11Y-08).
4. Contrast numerically checked for new color pairs (A11Y-01, A11Y-02).
5. Reduced-motion tested; no animation on high-frequency actions
   (MOT-03, MOT-07).
6. Targets sized (A11Y-09); focus visible and not obscured (A11Y-10).
7. Loading, empty, error, long-content states looked at (VIS-14, FBK-03..05).
8. Destructive actions use undo, or confirm with consequences (INT-06/07).
9. 320 px wide and 200% zoom checked (A11Y-04).
10. Looked at the real rendering (screenshot or filmstrip), not just tests.
    Transitions especially: automated runs passed with visible defects before
    (SHARED_AGENT_MEMORY).
11. No emoji, no gradient-text/glassmorphism by default (BAD-17, AI-slop list).
12. Report what was not verified.
