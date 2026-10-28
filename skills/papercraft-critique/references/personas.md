# Reviewer checklists

Each reviewer works from its own list, alone, and cites evidence for every finding. Mark each
item pass, fail or n/a; a state or screen that was never designed is an open question, not a pass.

## vision: the person's vision

For each key element on each screen, four questions (Connor and Irizarry):

1. **Objective**: which part of the objective is this element for?
2. **Element**: which elements serve it?
3. **Effect**: do they achieve it, for this audience?
4. **Why** or why not?

Also: everything the brief, references and feedback notes asked for is there; nothing contradicts
an open or resolved note (`get_feedback`); the tone matches the references. Unknown product facts
are questions for the person, never guesses.

## ux: usability heuristics (Nielsen)

1. Visibility of system status: progress, saving, selection shown at once.
2. Match with the real world: the users' words and order, not the system's.
3. User control and freedom: a marked exit, undo, cancel.
4. Consistency and standards: one word and one look per thing, platform conventions.
5. Error prevention: risky actions confirmed, impossible choices removed.
6. Recognition rather than recall: options visible, no remembering across screens.
7. Flexibility and efficiency: shortcuts for experts that novices need not see.
8. Aesthetic and minimalist design: nothing that competes with what matters.
9. Recognize, diagnose, recover from errors: plain language, the problem, the fix.
10. Help and documentation: needed rarely, and then where the task is.

## flow: coherent journeys

Walk each top task across screens (the `flow` graph when arrows connect screens, else the screen
order and names; ask the person when the intended order is unclear). At each step, the four
walkthrough questions: will they try to do this, will they notice the control, will they connect
it to their goal, will they see progress afterwards?

* Every task has an entry point (including first run and empty state) and a clear end
  (success screen or state).
* Every screen has a way forward and a way back or out; no dead ends (`deadEnds`), no screen
  nothing leads to (`unreached`) unless it is an entry.
* Each action that can fail has its error state; each destructive one a confirmation and undo.
* Navigation, back and primary actions sit in the same place with the same labels on every screen.
* What the user entered or chose shows on the following screens (the cart count, the name typed).
* Steps a user must remember between screens are shown instead (recognition over recall).
* The number of steps fits the objective ("two taps" means two).

## qa: states and edge cases

Per screen and component, pass, fail or n/a:

* States: default, hover, focus, pressed, disabled, selected, loading, empty, error, success.
* Content: the longest realistic text and names, minimum content, zero, one and many items,
  missing images and avatars, large numbers, special characters.
* Modes and sizes: every variable mode (screenshot each); the narrow and wide screen sizes when the
  design has them.
* Layout: nothing overflows or clips (`get_bounds`, lint `overflow`), screens keep their device
  height and scroll inside, fixed bars stay fixed.
* Consistency: repeated pieces are instances of the same component (lint `not-an-instance`).

## visual

* One focal point per screen; the primary action is the most prominent control.
* Scale and hierarchy: size, weight and colour follow importance.
* Balance and alignment: shared edges, a steady grid.
* Gestalt proximity: related items closer together than unrelated ones.
* Rhythm: spacing from the 4px scale, the same gaps for the same relationships.
* Type: sizes on the scale, at most three levels per screen.
* Tokens: every colour a variable, consistent radii, components reused.

## accessibility (W3C POUR, for a design)

**Perceivable**
* Text contrast 4.5:1, 3:1 for large text (18px, or 14px bold) and for control borders and icons.
* Colour is never the only signal (errors also have an icon or text; links are underlined or bold).
* Text is real text, not pictures of text; icons that act have a label or tooltip.
* The layout holds at 320px wide and with text at 200% (a narrow artboard shows it).

**Operable**
* Touch targets 44px (24px minimum with spacing).
* A visible focus state is designed for every interactive component.
* Headings, a page title and a visible current location (active nav item).
* Destructive actions confirmed or undoable; every gesture has a button alternative.
* Nothing flashes; motion can be reduced.

**Understandable**
* Fields keep a visible label (a placeholder alone disappears).
* Errors say what went wrong and how to fix it, next to the field.
* Navigation and labels are the same across screens.
* Plain language; abbreviations explained.

**Robust**
* Controls are named for their role (layer names, code links to real components), so the code
  hand-off keeps them buttons and inputs, not divs.

## copy

* The users' words, not internal jargon; verbs on buttons ("Save changes", not "OK").
* No placeholder or lorem text, no "Item 1".
* Errors: what happened and what to do. Empty states: what this is and the first action.
* Neutral wording about people whose gender is unknown (their name, or they).
* Consistent terms: one word per concept across all screens.

## end-user: a walkthrough as the audience

Take the `audience` (else ask). Write as that person, in the first person, for each top task:
what I see first, what I think it does, what I try, where I hesitate, and the first point where I
would give up. One persona per run; launch another critique with a different `audience` for a
second user type.
