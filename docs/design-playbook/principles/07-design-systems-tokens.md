# Design systems and tokens

Sources: V06 (Hawkins, accessible design systems), V07 (design-tokens talk,
tool-specific parts dated), S17 (W3C Design Tokens Community Group format, a
preview draft, "not to be implemented yet"), S12 (Radix scale).

A design system = a complete set of standards that manages design at scale
using reusable components and patterns (NN/g definition, quoted in V06):
reusable design elements, reusable coded elements, documentation for both.

### SYS-01 Tokens are the source of truth
A token is a named design decision (color role, space step, radius, duration,
easing, font size, shadow, z-index). Store tokens once in a platform-agnostic
data file and generate each platform's variables (CSS custom properties,
Tailwind theme, Rust constants for egui). Do not hand-copy values (V07: the
drift between SASS, JS, SVG and mobile is the problem tokens solve).

### SYS-02 Two layers: primitive and semantic
- Primitive (global): `blue.9 = #0066cc`, `space.4 = 16px`.
- Semantic (alias): `color.accent = {blue.9}`, `color.on-accent = {white}`.
- Component (optional): `button.primary.bg = {color.accent}`.
Components reference semantic or component tokens, never primitives and never
raw hex. Modes (light, dark, high-contrast) swap what semantics point to.

### SYS-03 Name by role and purpose, not value
`text-muted`, not `gray-500` in component code; `space-section`, not `24`.
A mode must be able to change a value without changing the meaning.

### SYS-04 Standard file format
Follow the DTCG draft shape so tools interoperate (S17): JSON,
`.tokens.json`, a token is an object with `$value`; optional `$type`,
`$description`, `$extensions`, `$deprecated`; aliases with `{group.token}`;
groups organize and may set `$type`; types include color, dimension (`px` or
`rem`), fontFamily, fontWeight (1 to 1000), duration (`ms`/`s`), cubicBezier,
number, and composites (shadow, border, transition, typography, gradient).
The spec is a draft; pin the shape in this repo's `templates/` and re-check
before adopting a tool. Theming and modes are not covered by the part read, so
model modes as separate token files or a documented `$extensions` key.

### SYS-05 Accessibility invariants live in the tokens and are tested
Encode checkable rules: every `text-*`/`bg-*` pair listed in
`contrast.pairs` meets 4.5:1 (3:1 large), border/focus tokens meet 3:1,
`target-min` is 44, `focus-ring-width` >= 2, `motion.reduce` exists. CI fails
when a theme breaks an invariant (`scripts/contrast.py --tokens`). Child
themes may change hue, never the invariants (V06).

### SYS-06 Atomic structure is a thinking tool, not a folder rule
Tokens (subatomic) -> atoms (label, box) -> molecules (button, text field) ->
organisms (form, header) -> templates -> pages (Brad Frost, via V06). Design all
states and variants at the molecule level so organism work can focus on
structure, order, and dependencies.

### SYS-07 Component contract
Each component documents: purpose and when not to use; anatomy; props/variants;
all states (FBK-02); tokens consumed; keyboard map; pointer behavior; screen
reader name/role/value; focus behavior; motion (with reduced-motion variant);
content rules (length, tone); responsive behavior; do/don't examples. Use
`templates/component-spec.md`.

### SYS-08 Build native elements first
Compose from the platform's accessible primitives (`button`, `dialog`,
`details`, `input`) and style them with tokens. Headless primitive libraries
(Radix, React Aria, Ark, Headless UI, Bits UI) own focus and keyboard logic;
reimplementing it per component is the main source of regressions.

### SYS-09 Governance
One owner per component; a "promote to the system" path from a product's local
component; deprecations via `$deprecated` and a changelog; visual regression
screenshots per state; versioned releases. Do not let a product fork a
component to change behavior silently.

### SYS-10 Documentation is a deliverable
Keyboard interaction docs, pointer docs and screen reader expectations are
the part most systems skip (V06). A component without them is incomplete.

### SYS-11 Inheritance for multi-brand
Parent system defines structure and invariants; each child overrides token
values (color, radius, font) within bounds. Fixes in the parent reach all
children.

### SYS-12 Platform parity without identical pixels
Same tokens and behaviors across web and native (egui); idiomatic layout per
platform. See platforms/web.md and platforms/native-egui.md.
