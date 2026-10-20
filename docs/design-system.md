# Design systems in PaperCraft

A PaperCraft project can hold a small design system: color variables, components with instances, and icon sets. Today these are built and managed through the tools (MCP or `papercraft-cli`); the editor shows the results but has no panel for them yet (see [ROADMAP.md](../ROADMAP.md)).

## Variables

Named colors per project. Define them once, use them as `var(--name)`:

```sh
papercraft-cli call set_variables '{"variables":{"surface":"#313244","accent":"#cba6f7","on-accent":"#11111b"}}'
```

* Use `var(--name)` in `write_html`, `update_styles` and `create_artboard` for `background`, `backgroundColor` and `color`. The fill stays linked and the link is saved in the file.
* Calling `set_variables` again with a new value moves every linked fill, on every page, in one undo step.
* **Modes.** `set_variables {"mode":"Dark", ...}` stores one set of values for a mode; define another mode the same way. The first mode you define is active. `set_variable_mode {"mode":"Light"}` switches the whole project.
* `get_variables` lists the variables and modes. With `"format":"css"` it returns a `:root { --name: #hex }` block, with `"json"` a design-tokens object (`{"accent":{"$type":"color","$value":"#cba6f7"}}`).
* **Limits.** Colors only. Fills and text color follow variables; strokes and gradients get the resolved color at write time and do not follow later changes. No spacing, radius or font-size variables yet.

Naming that works: roles, not hues (`surface`, `text-muted`, `accent`, `on-accent`, `danger`), so a mode can change the value without changing the meaning.

## Components and instances

```sh
papercraft-cli call create_component '{"nodeId":"<frame>","name":"Button / Primary"}'
papercraft-cli call create_instance  '{"componentId":"<id>","parentId":"<frame>","name":"Button / Sign in"}'
papercraft-cli call set_text_content '{"updates":[{"nodeId":"<label layer id from create_instance>","textContent":"Sign in"}]}'
```

* Build the main component as a frame (usually its own artboard), then convert it. Name it `Component / Variant`.
* `create_instance` returns the instance id and its layers, so you can set text and styles on them. Changes made through `set_text_content` or `update_styles` on an instance are recorded as overrides; everything else follows the main component.
* In `write_html`, `<x-paper-instance component-id="<id>" layer-name="Button / Save"/>` places an instance inline.
* `get_components` lists main components and how many instances each has.
* **Limits.** Components and their instances must be on the same page. No variants or properties: model variants as separate components (`Button / Primary`, `Button / Secondary`). In the editor, detach with ⌘⌥B; there is no detach or swap tool.

## Icons

`papercraft-cli icons add phosphor lucide` downloads sets from the npm registry into the icons folder (`<data dir>/papercraft/icons/<set>/`, or `$PAPERCRAFT_ICONS`), each with its license file. Sets: Lucide (ISC), Tabler (MIT), Phosphor (MIT), Heroicons (MIT), Material Symbols (Apache-2.0). Find names with `papercraft-cli icons search arrow left` (offline) or the `search_icons` tool, then place one in `write_html`:

```html
<x-paper-icon name="phosphor:gear" size="20" color="var(--accent)"/>
```

Fill-based sets (Phosphor, Material) follow variables; Lucide is stroke-based and keeps a fixed color.

## Lint

`lint_design` checks a subtree or the page and returns issues with a rule, a severity and a fix:

| Rule | Severity | Checks |
|---|---|---|
| `contrast` | error | WCAG 4.5:1 for text, 3:1 for large text (24px+, or 18.66px+ bold), against the nearest opaque fill behind it |
| `text-size` | warning | text under 12px |
| `type-scale` | info | font size not on the scale (default 12, 14, 16, 20, 24, 32, 40, 48, 64; pass `scale` to change) |
| `spacing-grid` | warning | gap or padding not a multiple of 4 |
| `touch-target` | warning | a frame named `Button…`, `Tab /…` or `Tab Bar /…` under 44px tall |
| `layer-name` | warning | default names (Frame, Rectangle, Group...) on layers with children |
| `hard-coded-color` | warning, info | a literal color that equals a variable (warning), or a literal used repeatedly when variables exist (info) |

Instances are skipped (their main component is linted). The rules are conventions, not law: pass `scale` to match your own type scale and ignore `info` items you disagree with. Lint does not judge taste; always look at `get_screenshot` too.

## Packages

A design system can travel as an **OpenDesign design-system package**: a folder with
`manifest.json`, `tokens.css` and `DESIGN.md`. `import_design_system` reads one into the
project (colors become variables, `[data-theme]` blocks become modes, everything else becomes
a token) and `export_design_system` writes one back. The format, the rules and the tools are
in [design-system-packages.md](design-system-packages.md).

## Worked example

[`examples/taskflow.py`](../examples/taskflow.py) builds an 8-screen app: a Catppuccin theme as variables (the only place hex appears), components for buttons, fields, chips, task rows, toggles and tab bars, Phosphor icons, and a design-system page of swatches. Run it against the running app after `papercraft-cli icons add phosphor`.
