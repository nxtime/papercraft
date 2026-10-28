# Screens

## Problem
PaperCraft has no notion of a "screen": a screen, a section that groups screens, a slot that labels one, and a component main are all just frames. Big projects like Interview (42k layers) nest screens three or four frames deep (Section > Screens > Row > Slot > Screen), so every frame-based feature guesses wrong:
- The canvas shows labels only for top-level frames, so the nested screens get none.
- The "On click" link list offers sections and labels instead of screens.
- Scrolling picks whichever scroll-enabled frame is deepest.
- Agents cannot tell which frame is the device viewport.

The owner reports a second, related problem. When a drawer or modal is open over a screen, agents still edit, lint and "fix" the content hidden underneath it. The owner can't see that work, it can't be checked in a screenshot, and it often breaks the open-state design. Evidence: owner reports in this session (2026-10-27) about agents working behind open drawers and screen scroll caught by the section frame.

## Goal
A frame can be declared a Screen. Agents and the editor then treat it as one device viewport, and agent edits to layers fully hidden inside it are refused with a clear reason.

## Non-Goals
- Prototyping or interactive preview (flows, transitions, overlays opening on click).
- Responsive variants or auto-generating breakpoints from one screen.
- Occlusion between separate screens on the canvas, or between sections. Only layers inside one screen count.
- Pixel-exact occlusion through transparency, blur or blend modes. Translucent covers (scrims) are judged by a rule, not by rendered pixels.
- Changing how frames that are not screens behave.
- Migrating existing projects automatically without the owner choosing to.

## User Stories
- As a designer, I want to mark a frame as a Screen (with a device size), so that PaperCraft and agents know it is a page of the product, not a container.
- As a designer, I want screens to show their name and device on the canvas even when nested in sections, so that I can read a large page at a glance.
- As a designer, I want "On click" and other screen pickers to list only screens, so that I can find the target among hundreds of frames.
- As a designer working with an agent, I want the agent stopped from editing layers that are fully hidden behind an open drawer or modal, so that it can't change what I can't see.
- As an agent, I want each layer's visibility in its screen (visible, partly covered, hidden) and what covers it, so that I can choose the right target or open the right state first.
- As a designer, I want scrolling over a screen to scroll that screen's scroll area, not its section, so that I can review long screens in place.
- As a designer, I want to convert existing screen-like frames into Screens in one step, so that big projects get the benefit without hand-marking each one.

## Acceptance Criteria
Declaring screens:
- [ ] Given a frame, when the owner or an agent declares it a Screen, then it is saved as a Screen with a device size, survives save and reload, and the change is one undo step.
- [ ] Given a frame that is a component main, an instance, or inside another Screen, when someone tries to declare it a Screen, then the request is refused with a message that names the reason.
- [ ] Given a Screen, when it is undeclared, then it goes back to an ordinary frame and its layers and layout do not change.
- [ ] Given a project file written before this feature, when it opens, then it loads with no Screens and nothing else changes.
- [ ] Given a project that has Screens, when an older PaperCraft build opens it, then the behaviour matches the project's existing version-compatibility rules (refused or loaded without screen data), never silent data loss.

Agent tools:
- [ ] Given a page with Screens, when an agent asks for the page summary, then Screens are listed as screens (name, device, size, section they sit in), separate from other frames.
- [ ] Given a Screen where an opaque drawer covers a layer entirely, when an agent edits that layer (styles, text, delete, move, write content into it), then the edit is refused. The reason names the covering layer and is returned as an error, not a silent no-op.
- [ ] Given the same layer only partly covered, when an agent edits it, then the edit succeeds and the result says the layer is partly covered, by which layer, and the visible fraction.
- [ ] Given the owner, not an agent, edits a fully hidden layer in the app, then the edit is allowed (the guard applies to agent tools only).
- [ ] Given an agent that passes an explicit override on a refused edit, then the edit succeeds and the result records that it was hidden. (Pending Open Question 3.)
- [ ] Given a layer that is hidden only because its parent clips it or it scrolled out of view, then it is not treated as covered. Scroll and clipping are reported separately from covering.
- [ ] Given a Screen with a hidden layer, when an agent lints that Screen, then hidden layers are skipped (or reported once as "hidden") instead of producing fix-me findings.
- [ ] Given an agent reads a layer's info, then the response includes its visibility state in its Screen: visible, partly covered, hidden, or not in a screen.

Editor:
- [ ] Given Screens nested in sections, when the canvas is zoomed out, then each Screen shows a name label (subject to the existing label overlap rules), and sections keep theirs.
- [ ] Given the "On click" picker, when a project has Screens, then it lists Screens (grouped by section) and no other frames.
- [ ] Given the pointer over a Screen whose own scroll area can still move, when the owner scrolls, then that area scrolls and the section around the Screen does not.
- [ ] Given a page with screen-like frames (same size as a known device, or named "Screen / ..."), when the owner runs "Convert to Screens", then the owner sees the candidates and confirms, and the declaration is one undo step.

Performance:
- [ ] Given the Interview Admin page (19k layers), when an agent edits a layer inside a Screen, then the hidden check adds no more than 5 ms to the call.

## Constraints
- Stack: Rust, egui 0.36, wgpu; existing MCP tools and `papercraft-cli` (online and offline) must behave the same.
- Performance: no extra cost per canvas frame; the hidden check runs only when an agent tool targets layers.
- Security: none new. The refusal is a safety rail for agents, not a permission system.
- Compat: the project format version bump follows the existing format migration rules. Web (wasm) build keeps parity.

## Open Questions
None open. Resolved 2026-10-27 (owner: "go with your recommendations"):
1. Cover: a layer later in paint order inside the same Screen that is opaque (solid or image fill at 95%+ effective opacity), or any layer marked as an overlay (drawer, modal, sheet), even if translucent.
2. Partly covered: an edit passes when at least 1% of the layer's area is visible; the result reports the visible share.
3. Override: yes, agents can pass an explicit flag; the result records that the layer was hidden.
4. Overlays: a direct child of a Screen can be marked as an overlay. To work underneath it, agents hide it (an ordinary undo step); no separate toggle.
5. Device: presets (mobile 390×844, tablet 768×1024, desktop 1440×900, large 2560×1440) or free size. The Screen's height is the viewport; taller content scrolls.
6. Labels: top-level frames keep labels, and nested Screens gain them. Other nested frames get none.
7. Convert to Screens: candidates are frames named "Screen / …", frames that are the screen inside a "Slot / …" frame, or top-level frames at a preset size. The owner confirms in the app; agents declare by id only.
8. Never inside a component main or another Screen.
9. First release: On click, page summary, lint scope, layer info. Export and design-to-code follow later.
