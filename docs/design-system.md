# Design systems in PaperCraft

A PaperCraft project can hold a design system: collections of typed variables with modes, components usable from any page, icon sets, and libraries that other projects can enable. The editor has a Variables modal, a Libraries modal and an Assets view for them (see [ROADMAP.md](../ROADMAP.md)); the same things are available through the tools (MCP or `papercraft-cli`).

## Variables

Variables live in **collections**. A collection has one or more **modes** (`Light`, `Dark`) and each variable has one value per mode. The project stores them in `design.json` next to `project.json`; every page keeps a derived copy that the renderer, lint and exports read, rebuilt on every change. A change from the UI or a tool updates every page in one undo step per page.

```sh
papercraft-cli call set_variables '{"variables":{"surface":"#313244","accent":"#cba6f7","on-accent":"#11111b"}}'
papercraft-cli call set_variables '{"collection":"Spacing","type":"number","variables":{"space/md":16,"space/lg":{"value":24}}}'
papercraft-cli call set_variables '{"variables":{"accent-hover":{"alias":"accent"}}}'
```

* **Types.** `color`, `number`, `string`, `boolean`. Numbers have no unit and bind as px. Strings are substituted as text (a font family, a shadow). Booleans bind to layer visibility. An alias points at a variable of the same type, in any collection or enabled library, and resolves per mode. A cycle is refused.
* **Groups.** A name is split on `/`: `color/brand/primary` sits in the group `color/brand`. Renaming a group in the Variables modal renames every variable under it and the bindings follow.
* **CSS names.** The name's segments joined with `-`, sanitized: `--color-brand-primary`. Use `var(--color-brand-primary)` in `write_html`, `update_styles` and `create_artboard` for `background`, `backgroundColor` and `color`. Two variables with the same CSS name across the collections of a project (library ones included) are refused when written.
* **Modes.** The active mode is chosen per page and per collection (Design tab), not per project; a new page inherits the active page's choice. `set_variables {"mode":"Dark", ...}` stores values for a mode, and the first mode you define is active. `set_variable_mode {"mode":"Light"}` switches the whole project. `create_collection {"name":"Brand","modes":["Light","Dark"]}` makes a collection with its modes up front.
* **Reading and exporting.** `get_variables` returns the collections (id, groups, type, value per mode, aliases, description, hidden), `pageModes`, and the older `variables` and `tokens` maps. With `"format":"css"` it returns a `:root { --name: #hex }` block plus `[data-theme]` rules for modes, with `"json"` a design-tokens object where groups nest on `/` and aliases are `{group.name}`, with `"tailwind"` an `@theme` block. Copy CSS, JSON and Tailwind are in the Variables modal too.
* **Deleting.** `delete_variables {"names":[...]}` refuses a variable that another one aliases and names the dependents; `"force":true` detaches them to the resolved value first. In the UI the same asks for confirmation. A removal affects every page.
* **Hidden.** A variable marked `hidden` stays in the file and is left out of published libraries.

Naming that works: roles, not hues (`surface`, `text-muted`, `accent`, `on-accent`, `danger`), so a mode can change the value without changing the meaning.

### Bindings

A fill that was written with `var(--name)` stays linked. Other fields bind with `bind_variable {"nodeIds":[...],"field":"gap","variable":"space/md"}` or with the variable button next to the field in the inspector. A bound field shows the variable as a chip with a detach button; detaching keeps the current value. `variable` may be the name or the CSS name; `null` detaches.

| Field | Type | Applies to |
|---|---|---|
| `stroke`, `text-color` | color | stroke color, text color |
| `width`, `height` | number | size in px |
| `gap` | number | auto-layout gap |
| `padding`, `padding-top`, `padding-right`, `padding-bottom`, `padding-left` | number | auto-layout padding (`padding` sets all four) |
| `radius` | number | corner radius |
| `font-size` | number | text size |
| `line-height` | number | below 4 it is a multiplier (`1.5`), from 4 up it is px |
| `opacity` | number | 0 to 1, values outside are clamped |
| `visible` | boolean | layer visibility |

A bound field is applied again when the variable, an alias target or the page's mode changes. Instances take bindings from their main; changing a bound field on an instance is an override for that instance only. Gradients cannot be bound yet.

### Migration

Projects made before collections open as project version 2: the first page's colors and modes become the collection `Colors`, its tokens become `Tokens` (a `16px` value becomes a number, anything else, including a unitless `1.5`, stays a string). CSS names do not change, so existing fill bindings keep working. PaperCraft 0.4.0 does not know `design.json`, so it refuses a migrated project with "made by a newer PaperCraft" instead of writing pages that would drift from it. Update before opening such a project on another machine.

### Variables modal

Open it from the Design tab ("Open variables"), the command palette (`Variables…`) or ⌘⌥⇧V. Collections of this file are on the left, then each enabled library (read-only), each with its group tree. The table has one row per variable in the selected group and one column per mode; cells edit in place (swatch and hex, number, text, toggle) and an alias picker links to another variable. Add a variable (choose its type), a mode or a collection from the header; rename, delete, search and a details pane (description, hidden, CSS name, how many layers use it) are there too.

## Components and instances

```sh
papercraft-cli call create_component '{"nodeId":"<frame>","name":"Button / Primary"}'
papercraft-cli call create_instance  '{"componentId":"<id>","parentId":"<frame>","name":"Button / Sign in"}'
papercraft-cli call set_text_content '{"updates":[{"nodeId":"<label layer id from create_instance>","textContent":"Sign in"}]}'
```

* Build the main component as a frame (usually its own artboard), then convert it. Name it `Component / Variant`.
* `create_instance` returns the instance id and its layers, so you can set text and styles on them. Changes made through `set_text_content` or `update_styles` on an instance are recorded as overrides; everything else follows the main component.
* In `write_html`, `<x-paper-instance component-id="<id>" layer-name="Button / Save"/>` places an instance inline.
* `get_components` lists the main components of every page (and library components) with `componentKey`, page and how many instances each has.
* **Across pages.** Instances can be placed from a main on any page: `create_instance` and `swap_instance` take a `componentId` (`pageId:num`) or a `componentKey` (stable, from `get_components`). Editing a main on page A updates its instances on every other page, one undo step per page. If the main is deleted its instances stay as they are, marked "Main missing" in Assets, where you can detach them. `import_components` still copies a main onto a page, but reuse no longer needs it.
* **Limits.** No variants or properties: model variants as separate components (`Button / Primary`, `Button / Secondary`). `detach_instance` and `swap_instance` exist as tools; in the editor detach with ⌘⌥B.

## Icons

`papercraft-cli icons add phosphor lucide` downloads sets from the npm registry into the icons folder (`<data dir>/papercraft/icons/<set>/`, or `$PAPERCRAFT_ICONS`), each with its license file. Sets: Lucide (ISC), Tabler (MIT), Phosphor (MIT), Heroicons (MIT), Material Symbols (Apache-2.0). Find names with `papercraft-cli icons search arrow left` (offline) or the `search_icons` tool, then place one in `write_html`:

```html
<x-paper-icon name="phosphor:gear" size="20" color="var(--accent)"/>
```

Fill-based sets (Phosphor, Material) follow variables; Lucide is stroke-based and keeps a fixed color.

> **Known limitation.** An icon's *stroke* color is frozen at import time: the value of the
> variable is baked into the stroke, so a stroke-based icon (Lucide, Tabler, Heroicons) does
> not follow the variable when it changes later. A layer's `stroke` can follow a variable with
> `bind_variable`; the icon's own stroke does not. A `var(--x)` that does not exist is reported as a warning
> and the icon keeps the color it inherits.

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

## Libraries

Any project can publish itself as a library, and other projects can enable it. This is a published snapshot, as in Figma, not a live link.

* **Publish.** Libraries modal, tab "This file", or `publish_library`. It writes `library.json` in the project folder with a version counter, the non-hidden variables, and every main component whose name does not start with `_` or `.`, each with its layers and a 256px preview. Before publishing the modal shows what changed since the last publish ("3 variables changed, 1 component added"); the tool returns the same list.
* **Enable.** Every project in the store with a `library.json` is available (projects synced from other devices through the relay show up too). `enable_library {"libraryId":...}` or the toggle in the Libraries modal copies the snapshot into the project (`libraries/<id>.json`). Its variables show read-only in the Variables modal and can be bound and aliased; its components show in Assets and are placed with `create_instance {"componentKey":...}`. Enabling is refused if a CSS name already exists in the project.
* **Updates.** When the published version is newer than the copy, the Libraries modal tab "Updates" and a dot on the Assets tab say so, and `list_libraries` reports `updateAvailable`. The review lists changed variables and components. `update_library` (or Update in the modal) replaces the snapshot, rebuilds every page and re-syncs the library's instances; instance overrides are kept. Updates are never applied by themselves.
* **Missing source.** If the library's project is deleted or not in the store, the copy keeps working. The modal shows "Source not found" and `list_libraries` shows `sourceFound: false`.
* **Disable.** `disable_library` or the toggle (after a confirm when it is in use) stops using a library: bindings to its variables are detached and keep the resolved value, local variables that aliased it keep the resolved value, its instances become plain frames, and its variables and components leave the project. One undo step per page.

Open the Libraries modal from the Share menu (`Libraries…`), the command palette, or the Assets header.

## Assets

The Assets view (side panel tab) replaces the old Library tab. It has a search box and sections for this file (by page), each enabled library, and My shapes (the personal shape library). Components are grouped by the part of the name before the first ` / ` (`Button / Primary` is in group `Button`) and shown as previews. Click one to place it at the viewport center, or drag it onto the canvas. Instances whose main was deleted are listed as "Main missing" with a detach action.

## Packages

A design system can travel as an **OpenDesign design-system package**: a folder with
`manifest.json`, `tokens.css` and `DESIGN.md`. `import_design_system` reads one into the
project (colors become variables, `[data-theme]` blocks become modes, everything else becomes
a token) and `export_design_system` writes one back. The format, the rules and the tools are
in [design-system-packages.md](design-system-packages.md).

## Worked example

[`examples/taskflow.py`](../examples/taskflow.py) builds an 8-screen app: a Catppuccin theme as variables (the only place hex appears), components for buttons, fields, chips, task rows, toggles and tab bars, Phosphor icons, and a design-system page of swatches. Run it against the running app after `papercraft-cli icons add phosphor`.
