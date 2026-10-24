# QA-A report: Admin page (96e0de09bfd69d1e)

Screenshots (scale 1, final state, 69 screens): specs/interview-redesign/QA-A/. All screens reviewed visually via 2x2 montages; spot-checked individually after fixes.

## Fixed
- Short sidebars: Edit MCQ sidebar 96e0de:48939, Edit Theory 96e0de:49204 (were fixed 248x900) -> height 100%. Same applied to all 60 Sidebar frames (New session/Session detail screens taller than 900 had footer floating mid-screen: now pinned).
- Lint not-an-instance: 20 `Header` frames -> `Header / Admin` instances (ids 96e0de:55560 + 9k, k=0..19), titles/subtitles copied. Lint now: 0 errors, 2 touch-target warnings (Button / Add file 96e0de:17529, Add test 96e0de:17538, 32px desktop buttons, left).
- Clipped/overflowing text (stale fixed widths on ~700 text nodes in pills, buttons, tags, alerts, header actions, dialogs): set to fit-content/nowrap and re-measured; filters, "New session", status tags, "Admit/Decline <name>", "Try admitting", alerts now full width. Sign in email error message set to wrap inside its box (27300).
- Sidebar footer (all 60 + modal screen): removed ADMIN role tag (overlapped gear), padding 12, gap 6, email 11px one line; Challenges/Live screens said "Admin / admin@test.local / A": now same user as Sessions (Elena Vasquez, elena@netcracker.com, EV).
- Page Header inconsistency (heights 88/112 vs Header/Admin 102; bell first vs last): 17 Page Header frames set to 102 tall, bell moved last (Challenges + Live screens).
- Templates Modal screen 96e0de:17347: Main overflowed to 1375 tall and overlay was invisible; Main clipped to 900, Overlay now absolute 1440x900 scrim with the modal centered; ADMIN tag removed.
- Revoke/Decline confirm dialogs (96e0de:54175, 53846): width fixed 480 so consequence lines no longer overflow.
- Edit Fullstack file rows: Visible/Hidden tag no longer overlaps trash icon.

## Leftovers pass (second round)
- Edit Algo Tests (96e0de:48178): Input column 124, Expected 132 (header + 5 rows), "negative numbers" no longer wraps, "[99998,99999]" fits.
- Edit Fullstack file rows: filename width 428 so Visible/Hidden tags sit at the right edge next to the trash icon (fill sizing did not stretch the text; fixed width used).
- New session challenge option rows (12 frames, were 640) set to 100% so they fill the card.
- Layer names: 86 generic "Text"/"Frame" layers renamed (Challenge name, Tag label, Duration, Link url, Copy link label, Step text, Banner text, Spacer, Md Editor group).
- Sign in: all 8 Form Panels padding-top 200 + flex-start, so the form starts at the same y in every state.
- Annotations: checked 6 slots (Challenges List, Edit MCQ, Live Code, Sign in, New session External, Link created External) with get_screenshot annotations:true scale 0.5. Cards sit outside the screens (above, below, left), none over a component. annotate tidy on the 5 sections: nothing to tidy.

## Remaining
- Leader lines for in-screen targets cross the screen by design: New session "External identity" (#12) line crosses the sidebar to the email field; Link created "Invite message template" line runs up through the content to Copy message. No card placement avoids this without a new card position per target (left/right side cards would cross other content); left as is.
- Lint: 2 touch-target warnings (Button / Add file 96e0de:17529, Add test 96e0de:17538, 32px desktop buttons).
