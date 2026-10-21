# Perception, cognition and timing

Established psychology and measured limits that explain why layouts and
timings work. Sources: V03 (Todaro on Yablonski's Laws of UX), S05 (NN/g
response times), S04 (NN/g heuristics), S10 (Vercel timings). Use these to
justify a decision, not to invent one: a "law" supports a choice, it does not
replace testing.

### PSY-01 Response time limits (NN/g, from Miller 1968 and Card 1991)
- 0.1 s feels instantaneous: show the result with no extra feedback.
- 1 s keeps flow: delays of 0.2 to 1 s are noticed; past 1 s show that the
  system is working.
- 10 s is the attention limit: need percent-done progress and a cancel control.
- Under about 2 s a spinner may flicker; 2 to 10 s use a busy indicator with a
  small changing counter; over 10 s use a real progress bar.
- Unknown total: list items as processed; a bare spinner is the last resort.

Doherty threshold (V03): keep system feedback under about 400 ms and users stay
in a productive rhythm.

### PSY-02 Loading indicators need a show delay and a minimum time
Do not show a spinner or skeleton for the first 150 to 300 ms, and once shown
keep it 300 to 500 ms, so quick responses do not flash (S11). Skeletons mirror
the final layout exactly.

### PSY-03 Fitts's law: size and distance of targets
Time to hit a target grows with distance and shrinks with size. Make primary
and frequent targets large and near the pointer's likely position; screen edges
and corners are effectively infinite targets on desktop. Minimums: A11Y-09.

### PSY-04 Hick's law: fewer choices, staged decisions
Decision time grows with the number and complexity of options. Break complex
tasks into steps, mark one recommended default, group rarely used options
behind progressive disclosure. Do not hide frequent actions to look clean.

### PSY-05 Jakob's law: be familiar
Users spend most of their time on other products. Reuse established patterns
(X closes, magnifier searches, logo goes home) and platform conventions unless
you can show the deviation pays for the learning cost.

### PSY-06 Gestalt grouping
Proximity, similarity, common region, uniform connectedness, continuity,
closure. In practice: spacing between groups must be visibly larger than
spacing inside groups (VIS-03); a shared card or background groups content
without a border; style same-function elements identically and different
functions differently.

### PSY-07 Miller's law is about chunking, not "7 items"
Working memory is small. Chunk long content into labeled groups, do not make
users remember information across screens (recognition over recall, S04
heuristic 6). Do not cite "7 plus or minus 2" as a menu-length limit.

### PSY-08 Serial position and von Restorff
First and last items in a list are remembered best, so put the most important
at the ends; the one visually distinct item in a set is remembered. This is why
a single primary action per screen (VIS-07) works, and why too many accents
cancel each other.

### PSY-09 Postel's law
Be liberal in what you accept: accept spaces, dashes and case in phone numbers,
card numbers, codes; normalize in code. Be conservative in what you send.

### PSY-10 Tesler's law: complexity is conserved
Some complexity cannot be removed, only moved. Push it to the system (defaults,
inference, smart parsing), not onto every user.

### PSY-11 Aesthetic-usability effect, with a warning
Polished interfaces are perceived as easier to use (Kurosu and Kashimura 1995),
and users tolerate minor faults more. It is also a bias: do not use polish to
substitute for usability testing, and do not dismiss accessibility defects
because the screen "looks right".

### PSY-12 Nielsen's 10 heuristics (S04)
1 Visibility of system status. 2 Match the real world. 3 User control and
freedom. 4 Consistency and standards. 5 Error prevention. 6 Recognition over
recall. 7 Flexibility and efficiency (shortcuts). 8 Aesthetic and minimalist.
9 Help users recognize, diagnose, recover from errors. 10 Help and
documentation. Use as the checklist order for a heuristic review
(checklists/design-review.md).

### PSY-13 Spatial consistency
Interfaces are spatial maps even though nothing is physical. An element should
appear to come from where it lives and return there (app opens from its icon,
volume slider grows from the buttons). Changing an animation origin changes
the mental model of the whole product (V05, Tartarotti). See MOT-05.
