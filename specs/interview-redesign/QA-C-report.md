# QA-C report: Candidate page (pageId 943a1809e1077b81)

Status: done except annotations. App recovered; fixes 4, 5 and 6 were re-screenshotted and look right (33911, 21994, 21090 PNGs in QA-C/ are post-fix). Lint re-run: 0 errors, 0 not-an-instance, 6 touch-target.

## Lint
- `not-an-instance`: 17 before, 0 after the new build (all were false positives; no frames replaced).
- `touch-target`: 6 warnings left, not fixed. 40px tall: `Button / Previous` (943a18:37024, 37129, 37234), `Button / Primary / LG / Disabled` (943a18:29084, 29179), `Button` (943a18:28779).

## Screenshots
63 screens at scale 1 (1440x900), saved in `specs/interview-redesign/QA-C/` before the fixes. All 63 were reviewed through 2x2 contact sheets, not one by one at full size.

## Fixes applied (1-3 not individually re-screenshotted)
1. Gendered copy "the moment she lets you in" changed to "the moment you are let in" in the "Check your setup" subheading on 5 Join screens (943a18:20966, 21059, 21147, 21507, 21423).
2. Join waiting screens: "Leave and cancel my request" was shifted right by a stale text width. Set `width: fit-content; text-align: center` (943a18:20961, 21054, 21142, 21418). The widths now read as expected.
3. Same stale-width fix: "Send a new code" on Access / Verify error (943a18:21807), "Retry now" on Join / Connection lost (943a18:21414), "Notify Elena again" on Join / Waiting no response (943a18:21049).
4. Access / SSO no invite: the 3-line message overlapped the "Use a different account" button. `Message` (children of 943a18:22009) and card 943a18:22005 set to height fit-content. The card grew from 220 to 242 and Message from 44 to 66.
5. Challenge / Code / running tests (943a18:33911): the last test row ("cleans up on unmount") was clipped at the bottom. Code area 472 to 416 (943a18:34009), test panel 272 to 328 (943a18:34084). The screenshot after this edit was not seen.

6. Status chips ("Camera needs attention", "All checks passed") had a stale narrow width, so the border cut through the text. Set fit-content on label and chip root (943a18:21149/21150, 20968/20969, 21061/21062, 21509/21510, 21425/21426).

## Seen but not fixed (minor)
- SSO redirecting/checking cards (943a18:21860, 21883): the Cancel button sits slightly off the text left edge, and in "checking" it is tight under the body text.
- Whiteboard and Diagrams screens have a large empty right-rail gap. This looks intentional.
- Annotations were not checked (`get_screenshot {annotations:true}` and `tidy`), because the app became unresponsive.

## Cleanup
/tmp/qac deleted. The app was never closed or restarted by me.
