# Component patterns and field-tested numbers

Accessible component behavior from Heydon Pickering's Inclusive Components
(S29 to S31), the Smashing optimistic-UI article (S32) and Steven Hoober's
touch-target research (S33), plus a curriculum map from web.dev (S34). Read
2026-10-21 via a summarizing fetch. Inclusive Components has 11 case studies; only
tooltips/toggletips, notifications and toggle buttons were read.

### PAT-01 Tooltips are a last resort
Prefer visible permanent labels. If space forces a tooltip: show on hover and
focus, name the control with `aria-labelledby` (replaces its content as the
name) or add supplementary text with `aria-describedby`, keep a hidden label for
icon-only controls, never put interactive content (links, close buttons) inside,
and never rely on the `title` attribute (not keyboard-accessible, unsupported by
many screen reader setups). Touch is a known gap: focus and active fire together,
so the tip shows only while pressed; give first-use inline text instead (S29).
Disabled buttons get no tooltip (S11).

### PAT-02 Toggletip for on-demand info
A `<button type="button">` (usually an "i") reveals information on click, which
works for mouse, keyboard and touch. Put an empty `role="status"` live region next
to it; on click clear it, then fill it after about 100 ms so repeated content is
re-announced. Do not use `aria-describedby` on it (users would hear the content
before choosing to open it). Close on Escape, outside click or blur (S29).

### PAT-03 Notifications (informational toasts)
Use a live region (`role="status"`, optionally with `aria-live="polite"`) that is
already in the DOM before the message is added. Do not move focus to a
notification: focus belongs to things requiring action. Show only what is
discoverable elsewhere. Differentiate message types with content, not style
(prefix "Error:", "Info:"), icons with hidden text and `currentColor`. Pickering
argues for no manual close button and auto-removal (his sample defaults to 10 s)
(S30).
Tension with FBK-08 and INT-06: an Undo toast is actionable, so it must be
keyboard-reachable and stay long enough (WCAG 2.2.1 timing); treat it as a
different component from an informational notification, and do not auto-dismiss
anything the user must act on.

### PAT-04 Toggle buttons
Use `<button type="button" aria-pressed="true|false">` with a constant label that
names the thing toggled ("Notify by email"); always set the attribute explicitly.
Never change the label and the pressed state together (label changes are not
announced until refocus). `role="switch"` with `aria-checked` suits on/off
settings but support is uneven (ChromeVox does not recognize `switch`). Checkboxes
are fine when users will not think they are submitting data. State must not be
color alone, and focus must remain visible without layout shift (S31).

### PAT-05 Optimistic UI preconditions (numbers)
Use it when the server almost always succeeds (user-initiated failure rate about
1 to 3%), the API is stable, responses stay under about 2 s, the action is simple
and binary (like, toggle), and it is not critical (check analytics: if users often
close the tab inside the 2 s window, restrict it). Validate on the client first.
Surface failure inside the same 2 s flow, scaled to stakes: quietly revert a
like; clearly explain a consequential failure; avoid pop-ups for minor actions.
Optimism is a prediction; it becomes deception when failures are hidden (S32,
from 2016). Refines FBK-06 and INT-11.

### PAT-06 Touch target sizes by screen position
Hoober's measurements (S33): top of screen 11 mm (about 31 pt, 42 px), bottom 12
mm (about 34 pt, 46 px), center as low as 7 mm (about 20 pt, 27 px). Use 44 to 46
px or more for sticky and edge controls, about 27 px minimum for links inside
content, 30 px as the practical floor and 48 px for critical targets. Add
padding so icons reach the target, use full-width bars for rows and accordions.
Consistent with A11Y-09; the 44 px default stands, and the center-of-screen
exception never goes below 24 px (WCAG 2.5.8).

### PAT-07 Patterns that cause rage taps
Hover-revealed icons in tables (replace with an "Actions" button), tiny color
swatches, edge-hugging controls, slow responses, pinch-zoom for precision (offer
a handle or a second-tap confirm instead), bottom tab bars with more than five
items (use a bottom sheet) (S33). Links BAD-11.

### PAT-08 Read the pattern before building the component
Inclusive Components case studies: cards, data tables, notifications, content
slider, collapsible sections, tabbed interfaces, theme switcher, tooltips and
toggletips, menus and menu buttons, todo list, toggle buttons. Also read the
matching WAI-ARIA APG pattern (A11Y-25). Add a rule here when one is digested.

### PAT-09 Accessibility curriculum map (web.dev Learn Accessibility, S34)
Modules to consult by task: ARIA and HTML; content structure; the document;
keyboard focus; JavaScript; images; color and contrast; animation and motion;
typography; video and audio; forms; patterns and design systems; design and UX;
automated, manual and assistive-technology testing. Only the module list was
read; fetch the module before quoting it.
