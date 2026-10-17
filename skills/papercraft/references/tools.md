# Tool reference

Generated from `papercraft-cli schema <tool>`; run that command for the authoritative, current text. `fileId` and `pageId` are accepted by almost every tool: omit `fileId` to use the open project, pass `pageId` to work on a page without switching the user's view. Node ids are opaque strings such as `3fa9c1:14`; never show them to the user.


## Projects and pages


### `list_resources` (read-only)

Lists the user's papercraft projects (local folders), most recently modified first. Each item has id, name, path, page count, updatedAt, and `open` for the project open in the app.


| Argument | Type | Required | Notes |
|---|---|---|---|
| `limit` | integer |  | Maximum projects to return. Defaults to 50. |


### `open_file`

Opens a project in the app (saving the current one) and returns a get_basic_info response. Optional pageId switches the user to that page.


### `create_file`

Creates a new empty project and returns its id. Call open_file with the id to work in it.


| Argument | Type | Required | Notes |
|---|---|---|---|
| `name` | string |  | Optional project name. |


### `create_page`

Creates a new page at the end of the page list and returns its id, without switching the user's page. Use the returned pageId in later calls.


| Argument | Type | Required | Notes |
|---|---|---|---|
| `name` | string |  | Optional page name. Defaults to "Page N". |


### `rename_pages`

Rename one or more pages. Does not switch the user's page.


| Argument | Type | Required | Notes |
|---|---|---|---|
| `updates` | array of object | yes | Page renames. |


## Read


### `get_basic_info` (read-only)

Essential context about the open project: project name, the page, node count, artboards (top-level frames) with ids, sizes and world positions, other top-level layers, fonts, and `pages` with the active one marked isActive.
- worldX/worldY are world positions; x/y are relative to the parent.
- Pass pageId to inspect another page without switching the user's view.


### `get_selection` (read-only)

The selected layers on the page the user is viewing: ids, names, types, sizes, positions and the artboard each belongs to.


### `get_node_info` (read-only)

Details of one node: type, name, size, visibility, lock state, parent, children ids, text content, x/y (relative to parent) and worldX/worldY. Errors if it doesn't exist.


| Argument | Type | Required | Notes |
|---|---|---|---|
| `nodeId` | string | yes | The node to inspect. |


### `get_children` (read-only)

Direct children of a node (or of the page root when nodeId is "root"), with ids, names, types, child counts and positions.


| Argument | Type | Required | Notes |
|---|---|---|---|
| `nodeId` | string | yes | The parent node, or "root". |


### `get_tree_summary` (read-only)

Compact indented text tree of a subtree: type, name, id and size per node. Much cheaper than get_jsx for orientation. nodeId "root" summarizes the whole page.


| Argument | Type | Required | Notes |
|---|---|---|---|
| `depth` | number |  | Maximum depth (default 3, max 10). |
| `nodeId` | string | yes | Root of the summary, or "root". |


### `get_jsx` (read-only)

Export a node and its descendants to code: `"inline"` (default, React JSX with inline styles), `"tailwind"` (React JSX with Tailwind CSS utility classes), `"react"` (typed React TSX component with Tailwind classes), `"angular"` (Angular standalone `@Component` with template and CSS), `"html"` (self-contained `<style>` + HTML markup), or `"egui"` (Rust `egui` layout code with a `tokens` module).


| Argument | Type | Required | Notes |
|---|---|---|---|
| `format` | `inline` \| `inline-styles` \| `tailwind` \| `react` \| `angular` \| `html` \| `egui` |  | Output format: `"inline"` (default), `"tailwind"`, `"react"`, `"angular"`, `"html"`, or `"egui"`. |
| `nodeId` | string | yes | The node to convert. |


### `get_design_spec` (read-only)

Extract a comprehensive AI design-to-code specification (`ai_spec_markdown` and structured JSON `spec`) for a node and its descendants. Includes the hierarchy, exact auto-layout flow/sizing (`fixed`, `hug`, `fill`), bound design tokens (`--var`), component instances, text content, implementation rules, and reference code for the chosen `target`. Always call this before implementing a design in code.


| Argument | Type | Required | Notes |
|---|---|---|---|
| `nodeId` | string | yes | The root frame or node to extract the spec for. |
| `target` | `react` \| `tailwind` \| `angular` \| `html` \| `egui` |  | Target framework/format (defaults to `"react"`). |


### `verify_code` (read-only)

Enforce design-to-code fidelity by checking written `code` against the source PaperCraft node (`nodeId`) for the given `target`. Reports a `score` (0–100), `passed` boolean, and structured violations (`missing-text`, `hardcoded-color`, `forbidden-absolute`, `missing-layout`, `missing-component`, `spacing-drift`) with actionable fixes.


| Argument | Type | Required | Notes |
|---|---|---|---|
| `code` | string | yes | The generated or hand-written implementation code to verify. |
| `nodeId` | string | yes | The PaperCraft node id the code implements. |
| `target` | `react` \| `tailwind` \| `angular` \| `html` \| `egui` | yes | Target language/framework of `code`. |


### `diff_frames` (read-only)

Compare two frames or subtrees (`beforeId` and `afterId`) and report property-level differences (size, position, fills, stroke, corner radius, opacity, text, layout, added/removed children).


| Argument | Type | Required | Notes |
|---|---|---|---|
| `beforeId` | string | yes | First frame or node id (baseline). |
| `afterId` | string | yes | Second frame or node id (comparison). |


### `get_computed_styles` (read-only)

CSS styles (camelCase) of one or more nodes, as {nodeId: styles}.


| Argument | Type | Required | Notes |
|---|---|---|---|
| `nodeIds` | array of string | yes | Nodes to read. |


### `find_nodes` (read-only)

Find nodes by text content and/or computed style. Searches the whole project by default; pass pageId for one page or nodeId for a subtree. `textValue` matches text case-insensitively with * wildcards anchored to the whole value. `filters` are {styleName, styleValue} pairs (AND), values may use * wildcards; colors match by value.


| Argument | Type | Required | Notes |
|---|---|---|---|
| `filters` | array of object |  |  |
| `nodeId` | string |  | Optional subtree root; takes priority over pageId. |
| `textValue` | string |  |  |


### `get_fill_image` (read-only)

The image used by a node's image fill, as image content.


| Argument | Type | Required | Notes |
|---|---|---|---|
| `nodeId` | string | yes | A node with an image fill. |


### `get_font_family_info` (read-only)

Whether font families are available and their weights/styles. papercraft renders text with Inter (400, 700, normal and italic); other families fall back to Inter.


| Argument | Type | Required | Notes |
|---|---|---|---|
| `familyNames` | array of string | yes |  |


### `get_guide` (read-only)

Read a guide. Topics: "papercraft-mcp-instructions" (read first), "html" (what write_html supports), "whiteboard" (shapes, arrows, stickies, hand-drawn style).


| Argument | Type | Required | Notes |
|---|---|---|---|
| `topic` | string | yes |  |


## See and export


### `get_screenshot` (read-only)

Render a node (or the whole page when nodeId is omitted or "root") to a PNG and return it as an image. Scale 1 (default) is enough for layout checks; use 2 to read small text. Large images are scaled down to fit API limits. In `papercraft-cli`, pass `-o`/`--out FILE` or `{"out": "FILE"}` to choose the output file path.


| Argument | Type | Required | Notes |
|---|---|---|---|
| `nodeId` | string |  | The node to capture; omit for the whole page. |
| `out` | string |  | Optional file path to save the PNG to (also used by `papercraft-cli`). |
| `scale` | number |  | Render scale: 1 (default) or 2. |


### `export`

Export nodes to files. `nodes` maps node ids to export settings ([{format: png|svg|pdf, scale: "2x" | "512w" | "512h" | "720p"}]; [] = 2x PNG). Files go to `directory` (default: the Downloads folder). Returns the paths written.


| Argument | Type | Required | Notes |
|---|---|---|---|
| `directory` | string |  | Output folder (absolute path). |
| `nodes` | object | yes |  |


### `export_combined_pdf`

Export several nodes into one PDF, one page per node, ordered top-to-bottom then left-to-right. Returns the file path.


| Argument | Type | Required | Notes |
|---|---|---|---|
| `nodeIds` | array of string | yes | Nodes to put on the pages. |
| `path` | string |  | Output file (default: Downloads/<project>.pdf). |


## Write


### `create_artboard`

Creates an artboard (top-level frame) and returns its id; then add content with write_html({mode: "insert-children"}). `styles` are camelCase CSS; width and height are required (e.g. "1440px"). Artboards default to display:flex, flexDirection:column and a white background, and are placed in an empty spot unless left/top are given. Default sizes: desktop 1440×900, tablet 768×1024, mobile 390×844.


| Argument | Type | Required | Notes |
|---|---|---|---|
| `name` | string | yes | Layer name. |
| `styles` | object | yes | CSS styles, e.g. {"width": "390px", "height": "844px", "backgroundColor": "#f5f5f5", "padding": "24px", "gap": "16px"} |


### `write_html`

Parse HTML with inline styles into layers. Write incrementally: each call should add one visual group (a header, one list row, a card) so the user sees progress.
- mode "insert-children" appends the HTML inside targetNodeId ("root" = the page); "replace" replaces targetNodeId with it.
- Inline styles only. Flexbox (display:flex, flex-direction, gap, padding, align-items, justify-content, align-self, flex-grow, flex-wrap) is the layout model; block elements stack vertically. position:absolute with left/top is supported. No margin, grid or tables.
- Text: font-size, font-weight (≥600 = bold), font-style, color, line-height, letter-spacing, text-align, text-decoration (including `wavy` and `text-decoration-color`); <b>/<i>/<u>/<s>/<span style> make rich text. Font family is always Inter.
- Boxes: background(-color/-image: colors, linear/radial gradients, url()), border (and border-top/right/bottom/left), border-radius, opacity, overflow:hidden, mix-blend-mode, width/height (px, %, fit-content), min-width/max-width/min-height/max-height.
- <img src> takes paper-asset:///abs/path, file:// URLs, absolute paths or data: URIs. <svg> markup is imported as vector layers.
- <x-paper-icon name="set:name" size="20" color="var(--accent)"/> places an installed icon (search_icons); <x-paper-instance component="Name" component-id="ID" layer-name="..."/> places a component instance; colors may be var(--name) of set_variables.
- layer-name="..." names layers; <x-paper-clone node-id="ID" style="..."/> clones an existing node (supports cross-page `<page>:<id>`).
Returns the ids of the created top-level layers and the styles that were ignored.


| Argument | Type | Required | Notes |
|---|---|---|---|
| `html` | string | yes | HTML to parse into layers. |
| `mode` | insert-children/replace | yes |  |
| `targetNodeId` | string | yes | Target node id, or "root" for the page. |


### `update_styles`

Update styles on one or more nodes (camelCase CSS, like React.CSSProperties). Setting left/top on an artboard moves it. Hand-drawn keys: sketch, roughness, fillStyle, strokeStyle, seed. Styles with no effect are returned under ignoredStyles.


| Argument | Type | Required | Notes |
|---|---|---|---|
| `updates` | array of object | yes | Each item applies styles to nodeIds. |


### `set_text_content`

Set the text of one or more Text or Sticky nodes (batch).


| Argument | Type | Required | Notes |
|---|---|---|---|
| `updates` | array of object | yes |  |


### `rename_nodes`

Rename layers (batch). Names are truncated to 50 characters.


| Argument | Type | Required | Notes |
|---|---|---|---|
| `updates` | array of object | yes |  |


### `delete_nodes`

Delete nodes and their descendants. Verify parents with get_node_info first when unsure.


| Argument | Type | Required | Notes |
|---|---|---|---|
| `nodeIds` | array of string | yes | Nodes to delete. |


### `duplicate_nodes`

Deep-clone nodes, including across pages (via `toPageId` or a cross-page `parentId`; `<x-paper-clone node-id="<page>:<id>"/>` also works across pages). Each copy goes under parentId when given, else next to its source; duplicated artboards are placed in an empty spot. Returns new ids and a descendantIdMap from every original descendant id to its copy.


| Argument | Type | Required | Notes |
|---|---|---|---|
| `nodes` | array of object | yes | Items accept `{id, parentId?, toPageId?}`. |


### `move_nodes`

Move nodes keeping their ids (or onto another page via `toPageId` or a cross-page `parentId` / `before` / `after`). Each move is {nodeId, before: siblingId}, {nodeId, after: siblingId} or {nodeId, parentId, toPageId?, index?} (parentId "root" = the page root). Moves apply in order. In auto-layout parents this changes the visual order; in free parents it changes stacking and keeps the world position.


| Argument | Type | Required | Notes |
|---|---|---|---|
| `moves` | array of object | yes | Items accept `{nodeId, before?, after?, parentId?, toPageId?, index?}`. |


### `create_shape`

Create a whiteboard element: rectangle, ellipse, diamond, triangle, polygon, star, line, arrow, sticky (sticky note with text) or text. Coordinates are world units, or relative to parentId when given. Lines and arrows take `points` ([[x,y],...], at least two) and arrows may bind to layers with startNodeId/endNodeId (the arrow then follows them). `styles` takes the same CSS as update_styles plus the hand-drawn keys sketch, roughness (0 architect, 1 artist, 2 cartoonist), fillStyle (hachure, cross-hatch, solid, zigzag, dots), strokeStyle (solid, dashed, dotted). Arrows also take route (straight, curved, elbow) and startHead/endHead (none, arrow, triangle, dot, bar, diamond).


| Argument | Type | Required | Notes |
|---|---|---|---|
| `endHead` | string |  |  |
| `endNodeId` | string |  |  |
| `height` | number |  |  |
| `name` | string |  |  |
| `parentId` | string |  | Frame to place the shape in. |
| `points` | array of array |  |  |
| `route` | straight/curved/elbow |  |  |
| `sides` | integer |  | Polygon sides / star points. |
| `startHead` | string |  |  |
| `startNodeId` | string |  |  |
| `styles` | object |  | CSS and hand-drawn styles. |
| `text` | string |  | Text for sticky and text. |
| `type` | rectangle/ellipse/diamond/triangle/polygon/star/line/arrow/sticky/text | yes |  |
| `width` | number |  |  |
| `x` | number |  |  |
| `y` | number |  |  |


## Design system


### `get_variables` (read-only)

Color variables and design tokens of the page's project (name -> hex/value). Fills that follow a variable were written with `var(--name)`; set_variables moves all of them at once. `format` `css` returns a `:root { --name: #hex }` block (plus `[data-theme]` rules for modes), `json` returns a W3C DTCG design-tokens object, and `tailwind` returns a Tailwind v4 `@theme` block.


| Argument | Type | Required | Notes |
|---|---|---|---|
| `format` | `css` \| `json` \| `tailwind` |  | Output format: `css`, `json` (W3C DTCG), or `tailwind` (`@theme`). |


### `set_variables`

Define or change color variables (names like `surface`, `accent-text`), on every page. Use them as `var(--name)` in write_html / update_styles / create_artboard colors (backgroundColor, background, color) instead of hex literals, so a theme change is one call. Fills already following a changed variable are updated. Variables apply to fills and text color; strokes and gradients keep fixed colors.


| Argument | Type | Required | Notes |
|---|---|---|---|
| `mode` | string |  | Store the values for this mode (e.g. "Light", "Dark"); they apply when it is the active mode. The first mode you define becomes active. |
| `variables` | object | yes | {"accent": "#cba6f7", "surface": "#313244"} |


### `set_variable_mode`

Switch the active variable mode (defined with set_variables {mode}): every variable takes that mode's value and all fills following them change, on every page. One call turns a whole design from light to dark.


| Argument | Type | Required | Notes |
|---|---|---|---|
| `mode` | string | yes |  |


### `get_components` (read-only)

Main components on a page (id, name, size) with how many instances each has.


### `import_components`

Copy the main components anywhere on `fromPageId` (including inside artboards, typically on a `Design System` page) onto this one, so you can place instances here with `create_instance` or `<x-paper-instance component="Name" component-id="ID"/>`. Components with the same name already here are skipped. Variables and tokens are already shared by every page; components are copies, not live links.


| Argument | Type | Required | Notes |
|---|---|---|---|
| `fromPageId` | string | yes | Source page id. |
| `names` | array of string |  | Only these components (default all). |


### `create_component`

Turn a frame or artboard into a main component. Build reusable parts (button, row, chip, tab) once as components, then place them with create_instance or `<x-paper-instance component="Name" component-id="ID"/>`. Components and instances live on one page.


| Argument | Type | Required | Notes |
|---|---|---|---|
| `name` | string |  | Optional new layer name. |
| `nodeId` | string | yes | The frame to convert. |


### `create_instance`

Place an instance of a component. Edit text and styles on the instance's descendants with set_text_content / update_styles (they become overrides); edits to the main component flow to every instance. Returns the instance id and its text layers.


| Argument | Type | Required | Notes |
|---|---|---|---|
| `componentId` | string | yes |  |
| `index` | number |  | Position among the parent's children. |
| `name` | string |  |  |
| `parentId` | string |  | Parent frame; omit or "root" for the page. |
| `x` | number |  |  |
| `y` | number |  |  |


### `swap_instance`

Swap an instance root to point at a different main component on the same page, preserving overrides on descendants matched by name and layer type.


| Argument | Type | Required | Notes |
|---|---|---|---|
| `componentId` | string | yes | The new main component id. |
| `nodeId` | string | yes | The instance root node to swap. |


### `detach_instance`

Detach one or more component instances into plain frames, breaking their link to the main component while keeping their current appearance and children.


| Argument | Type | Required | Notes |
|---|---|---|---|
| `nodeIds` | array of string | yes | Instance root nodes to detach. |


### `search_icons` (read-only)

Search installed open icon sets (Lucide, Tabler, Phosphor, Heroicons, Material). Returns `set:name` references for <x-paper-icon name="lucide:check" size="20" color="var(--text)"/> in write_html. Without a query it lists installed and installable sets; install with `papercraft-cli icons add lucide`.


| Argument | Type | Required | Notes |
|---|---|---|---|
| `limit` | number |  | Default 20. |
| `query` | string |  |  |
| `set` | string |  |  |


### `lint_design` (read-only)

Check a page or subtree for common design mistakes and get a fix for each: text contrast (WCAG 4.5:1, 3:1 for large text), text under 12px, font sizes off the type scale, gap/padding off the 4px grid, buttons and tabs under 44px tall, default layer names (Frame, Rectangle), and hard-coded colors that equal a variable. Run it before finish_working_on_nodes and fix errors and warnings; `info` items are suggestions. Pass `fix: true` to automatically snap spacing/padding to 4px, relink matching color variables, snap text sizes, and enlarge small touch targets.


| Argument | Type | Required | Notes |
|---|---|---|---|
| `fix` | boolean |  | Apply safe automatic fixes before reporting remaining issues. |
| `nodeId` | string |  | Subtree to check; omit for the whole page. |
| `scale` | array of number |  | Allowed font sizes. Default 12, 14, 16, 20, 24, 32, 40, 48, 64. |


## Session


### `get_feedback` (read-only)

Notes the user left on layers (the Feedback box in the inspector, shown as amber dots on the canvas): which layer, where it sits, what to change. Omitting `pageId` queries all pages across the project; `openCount` and `resolvedCount` are returned alongside per-page counts in `get_basic_info`. Call it at the start of a session and after each round of edits; fix every open note, then `resolve_feedback`. `get_selection` also includes the selected layers' notes.


| Argument | Type | Required | Notes |
|---|---|---|---|
| `includeResolved` | boolean |  | Include resolved notes. |
| `nodeId` | string |  | Only this layer. |
| `pageId` | string |  | Omit to query all pages across the project. |


### `add_feedback`

Leave a note on a layer (author `agent` by default), e.g. a question or a problem you could not fix. The user sees it in the inspector and as a dot on the canvas.


| Argument | Type | Required | Notes |
|---|---|---|---|
| `author` | string |  | Defaults to `agent`. |
| `nodeId` | string | yes |  |
| `text` | string | yes |  |


### `resolve_feedback`

Mark notes on a layer resolved, with a short reply saying what you changed. Without `commentId` every open note on the layer is resolved. `delete` removes instead.


| Argument | Type | Required | Notes |
|---|---|---|---|
| `commentId` | number |  |  |
| `delete` | boolean |  |  |
| `nodeId` | string | yes |  |
| `reply` | string |  |  |


### `select_nodes`

Select nodes on the page the user is viewing so they can see what you mean; zoom: true also frames them in the viewport. An empty list clears the selection.


| Argument | Type | Required | Notes |
|---|---|---|---|
| `nodeIds` | array of string | yes | Nodes to select. |
| `zoom` | boolean |  |  |


### `undo`

Undo the last change on a page (the user's or an agent's).


### `redo`

Redo the last undone change on a page.


### `finish_working_on_nodes`

Call when done creating or editing. Saves the project.


| Argument | Type | Required | Notes |
|---|---|---|---|
| `nodeIds` | array of string |  | Optional nodes you worked on. |

