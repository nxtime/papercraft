# Variable collections, shared libraries and assets

## Why

Variables are a flat list of colors plus untyped tokens, copied into every page's `Document` and kept in step by hand (two writers fan out to every page, the remove buttons in `src/ui/confirm.rs` do not). There are no groups, no types beyond color, no aliases, and only fills can bind. Components work only on the page that holds the main: the Design tab lists the active page's components, so a `Design System` page's components are invisible everywhere else, and `create_instance` / `swap_instance` refuse other pages. Nothing can be shared between files except a one-off package import.

Goal: Figma's model, local-first. Collections with modes, slash groups, typed variables with aliases; components usable from any page; any file can publish itself as a library that other files enable and update; a proper place to see and edit all of it.

## Decisions (user, 2026-10-21)

* Libraries: **Figma-style publish**. A file publishes a versioned snapshot; consumers keep a copy and get "Updates available". Files keep working when a library is missing.
* Variables: **collections + groups + types + aliases**, number variables bindable to geometry and type.
* UI: a **Variables modal**, a **Libraries modal**, and an **Assets** side view. Design mock-ups first.

## What exists today (checked 2026-10-21)

* Storage: `Document.variables` (active colors), `modes`, `mode`, `tokens` (`src/model/document.rs:15`), one copy per page. Writers: `Tx::set_variable`, `remove_variable`, `rename_variable`, `set_token`, `remove_token`, `set_design_system`, `remove_mode`, `set_variable_in_mode`, `set_mode` (`src/history.rs`); UI `on_every_page` (`src/ui/design.rs:15`); MCP `set_variables`, `set_tokens`, `set_variable_mode`, `import_design_system`, `create_page` (`src/mcp/exec.rs`).
* Binding: `Paint::Solid { color, var: Option<String> }` holds the CSS name; the color is baked in and `apply_variable` rewrites it. Renderer, SVG and PDF read the baked color only. Tokens are substituted as text when CSS is written (`css::set_variables_in_scope`), no link kept.
* Readers: codegen (`:root`, `[data-mode]`, Tailwind, DTCG), `get_variables`, `tokens_css`, lint (`lint.rs` match and auto-link), `diff_pages`, inspector var picker.
* Components: `Node.component`, `InstanceLink { main: NodeId, overrides }`; `Editor::fixup` syncs instances on the same `Document`. `import_components` copies mains between pages with no link back.
* Project: `project.json` (version 2: v1 to v2 is a no-op manifest step; the version bump makes older apps refuse), `pages/<id>.json` (version 3, migration chain), `versions/`, `runs/`. Projects live in one store folder; relay library mode already syncs every project to every device.
* Figma import ignores variables and does not link instances.

## Model

### Variables (project level, `design.json` in the project folder)

```text
DesignSystem
  collections: [Collection]
Collection { id, name, modes: [Mode { id, name }], default_mode, variables: [Variable] }
Variable   { id, name: "color/brand/primary", kind: Color | Number | String | Boolean,
             values: { mode_id: Literal | Alias(variable id) }, description, hidden: bool }
```

* **Groups** are the `/` segments of the name (`color/brand/primary` sits in group `color/brand`). Renaming a group renames every variable under it.
* **CSS name**: segments joined by `-`, sanitized (`--color-brand-primary`). Two variables with the same CSS name across all enabled collections are refused at write time.
* **Aliases** point to a variable of the same kind in any collection, including a library's. Cycles are refused. A value resolves through aliases per mode.
* **Active mode** is per page per collection (`PageFile.modes: {collection_id: mode_id}`, default = `default_mode`). A new page inherits the active page's choice.
* **Numbers** carry no unit; they bind as px. **Strings** substitute as text (font family, shadow). **Booleans** bind to visibility.
* **One source, one cache.** `design.json` is the only stored truth. Each page's `Document.variables` / `modes` / `mode` / `tokens` become a derived cache rebuilt from it (resolved with that page's modes, keyed by CSS name), so the renderer, codegen, lint, diff and export keep reading what they read today. Every writer goes through one design-system API that updates `design.json`, rebuilds every page's cache and re-applies bindings in one undo step per page.
* **Migration** (project version 1 → 2): the first page's `variables` + `modes` become collection `Colors` (one mode per existing mode, else `Default`); `tokens` become collection `Tokens` (a px value such as `16px` becomes Number; anything else, including unitless `1.5`, stays String). Existing fill bindings keep working because CSS names do not change. Pages keep their fields as the cache. The manifest version is 2, so an older app refuses the project ("made by a newer PaperCraft") rather than write pages without bindings and never update `design.json`.

### Bindings

* Fills keep `Paint::Solid.var` (CSS name).
* New `Node.bindings: BTreeMap<field, css name>` for `stroke` (color), `text-color`, `width`, `height`, `gap`, `padding` (all four or per side), `radius`, `font-size`, `line-height`, `opacity`, `visible` (boolean). A bound field is re-applied when the variable, an alias target or the page's mode changes, exactly like fills today. Detaching writes the resolved value and drops the binding.
* Instances inherit bindings from mains; an overridden field drops it on the instance only.

### Components across pages

* Every main gets a stable `component_key` (created on `create_component`, migrated for existing mains).
* `InstanceLink` gains `source: Option<Source>`: `None` = same page (current behaviour), `Page(page_id)`, `Library(library_id)`. `main` keeps pointing at the node id inside that source document.
* Editing a main on page A re-syncs instances on every other page whose link is `Page(A)`, in the same commit (one undo step per touched page). Deleting a main leaves its instances as they are, marked "main missing" (Assets offers Detach or Restore).
* `create_instance` and `swap_instance` accept a component from any page or enabled library (`componentId` as `pageId:num`, or `componentKey`).
* `import_components` stays (copy) but is no longer needed for reuse.

### Libraries

* **Publish** (Libraries modal, or MCP `publish_library`): writes `library.json` in the file's project folder: `{ library_id (= project id), name, description, version (counter), published (ms), collections (non-hidden variables), components (every main not named with a leading `_` or `.`, with its subtree and a 256px PNG preview) }`. A diff against the previous publish is shown first ("3 variables changed, 1 component added").
* **Available libraries** = every project in the store with a `library.json` (relay library mode makes other devices' libraries appear with no extra work).
* **Enable** in a consumer copies the snapshot to `<consumer>/libraries/<library_id>.json` and records `{library_id, version}` in `design.json`. Library collections show read-only in the Variables modal (aliasable, bindable); library components show in Assets.
* **Updates**: when the library's `version` is newer than the copy, the Libraries modal and a dot on the Assets tab say "Updates available". Review lists changed variables and components; **Update** replaces the snapshot, rebuilds caches and re-syncs `Library(id)` instances (instance overrides kept). Updates are never applied silently.
* **Missing library**: the copy keeps everything working; the modal shows "Source not found" with Disable (bindings are detached to resolved values, instances detached) after a confirm.
* **Disable** with uses left: confirm, then detach as above.

## UI

* **Variables modal** (from Design tab "Open variables", palette "Variables…", ⌘⌥V): left column collections (local, then each library, read-only) with their group tree; main area a table, one row per variable in the selected group, one column per mode; cells edit inline (swatch + hex, number, text, toggle) with an alias picker ("→ color/brand/primary"); add variable (type menu), add mode, add collection, rename and delete (confirm), search, a details pane (description, hidden from publishing, CSS name, where used count). Copy CSS / JSON / Tailwind move here.
* **Libraries modal** (Share menu "Libraries…", palette, Assets header): "This file" (publish state, description, changes since last publish, Publish), "Libraries" (available libraries with Enable toggles, version, source status), "Updates" (per library review and Update).
* **Assets side view** (replaces the Library tab icon slot, keeps My shapes as its last section): search; sections This file (grouped by page), each enabled library, My shapes; previews in a grid; click to place at the viewport centre, drag onto the canvas; context menu: Go to main, Detach all, Rename (local only). Instances whose main is missing show a warning row.
* **Design tab** slims to: modes per collection for this page, a short variables summary with "Open variables", lint. The components list moves to Assets.
* **Inspector**: every bindable field gets the variable picker (filtered by kind), a bound field shows the variable chip with Detach.

Mock-ups: `PaperCraft` design file, new page `Design libraries`, artboards `Modal / Variables`, `Modal / Libraries`, `Modal / Library updates`, `Panel / Assets` (344x1440 like the other panel states), `Inspector / Bound fields` (344x1440). Modals drawn at 2560x1440 window size and checked at 1280x800.

## MCP and CLI

* `get_variables` returns collections, groups, kinds, per-mode values and aliases (current formats keep working; DTCG nests groups).
* `set_variables` gains `collection`, `type`, `mode`, `alias`, `description`, `hidden`; old calls (color by name) still work and land in the first collection.
* New: `create_collection`, `delete_variables`, `bind_variable` (node field ← variable), `publish_library`, `list_libraries`, `enable_library`, `update_library`, `disable_library`.
* `get_components` lists every page and enabled library with `componentKey` and `source`.
* Every new tool is classified in `edits_doc` (the existing test enforces it).

## Not in scope (v1)

* Text styles, effect styles, grid styles (roadmap row 9; next).
* Component variants and properties (row 10).
* Per-frame mode override (mode is per page).
* Live links between files without publish.
* Figma variable import (separate item; the model now has room for it).

## Acceptance

* Migration: a project with variables, modes and tokens opens as version 2 with collections `Colors` and `Tokens`; every existing fill binding still resolves; 0.4.0 refuses a migrated project with a clear "made by a newer PaperCraft" error instead of losing data.
* Groups: renaming group `color/brand` renames its variables and every binding follows.
* Aliases resolve per mode; a cycle is refused; deleting an aliased variable asks first and detaches dependents to resolved values.
* Number bindings: changing `space/md` from 16 to 24 updates the gap of every bound frame on every page in one undo step per page; switching a page's mode updates its bound fields only.
* Removing a variable or mode from the UI affects every page (fixes today's page-only removal).
* An instance on page B of a main on page A updates when the main changes; undo on A and B each restores its side.
* Publish → enable in another project → instance and binding work; change and republish → consumer shows Updates available → Update re-syncs instances and keeps overrides; deleting the library project leaves the consumer working with "Source not found".
* Two libraries defining `--color-bg` cannot both be enabled without a clear error.
* Variables modal, Libraries modal and Assets view match their mock-ups at 1440p and 1280x800 (screenshot check).
* `cargo test --no-fail-fast`, wasm and Windows checks pass; web build loads.
