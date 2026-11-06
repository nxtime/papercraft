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


### `delete_pages`

Delete one or more pages. The last page of a project cannot be deleted. A page that still has layers is only deleted with `confirm: true`; check what is on it first (get_basic_info lists node counts).

| Argument | Type | Required | Notes |
|---|---|---|---|
| `confirm` | boolean |  | Required to delete a page that has layers. |
| `pageIds` | array of string | yes | Pages to delete. |


## Read

### `get_basic_info` (read-only)

Essential context about the open project: project name, the page, node count, artboards (top-level frames) with ids, sizes and world positions, other top-level layers, fonts, `pages` with the active one marked isActive, and `viewport` ({pageId, rect: [x, y, w, h] world, zoom}: what the user sees).
- worldX/worldY are world positions; x/y are relative to the parent.
- Pass pageId to inspect another page without switching the user's view.


### `get_selection` (read-only)

The selected layers on the page the user is viewing: ids, names, types, sizes, positions and the artboard each belongs to, plus `viewport` ({pageId, rect: [x, y, w, h] world, zoom}).


### `get_context` (read-only)

The one compact read to make before editing (about 2k tokens): the `page`, the `target` (`id`, else the first selected layer, else the whole page) and its `screen`, the `selection`, an `outline` of the target two levels deep with child counts and an `estimatedTokens` per layer (what a `get_tree_summary` of it costs, so you know what is cheap to read next), the `variables` used inside it (value in the active mode, collection), every collection with its modes and the active one, the `components` used with instance counts, and a `lint` summary of the target only (counts per rule and the first 5 findings). Read deeper nodes afterwards, one at a time.

| Argument | Type | Required | Notes |
|---|---|---|---|
| `id` | string |  | A layer or screen to describe; omit for the selection, else the page. |


### `get_node_info` (read-only)

Details of one node: type, name, size, visibility, lock state, parent, children ids, text content, x/y (relative to parent) and worldX/worldY. Errors if it doesn't exist. Reports `estimatedTokens`; over `maxTokens` the child ids are cut (`truncated`).

| Argument | Type | Required | Notes |
|---|---|---|---|
| `maxTokens` | number |  | Token budget of the result (default 20000). Over it the result is cut at a whole node or line, `truncated` is true and the result says how to read the rest. |
| `nodeId` | string | yes | The node to inspect. |


### `get_bounds` (read-only)

Rects of a whole subtree in one read, instead of one get_node_info per layer. Returns `columns` [id, parent, x, y, w, h, absX, absY, clip] and `rows` (one array per layer, parents before children): x/y/w/h are relative to the parent, absX/absY are page coordinates, clip is true when the layer clips its content (clipping or scrolling frame), parent is null for a top-level layer. At most 2000 rows per call: then `truncated` is true and `continueWith` lists the node ids to pass as nodeIds. Use it to verify nothing leaves its artboard, and `lint_design`'s `overflow` rule to list the offenders.

| Argument | Type | Required | Notes |
|---|---|---|---|
| `depth` | number |  | Levels returned, counting each given node as 1 (default: all). |
| `nodeIds` | string[] |  | Subtrees to measure; omit for all artboards on the page. |
| `pageId` | string |  | Page to use (default: the active page). |


### `get_children` (read-only)

Direct children of a node (or of the page root when nodeId is "root"), with ids, names, types, child counts and positions. Reports `estimatedTokens`; over `maxTokens` the list is cut (`truncated`).

| Argument | Type | Required | Notes |
|---|---|---|---|
| `maxTokens` | number |  | Token budget of the result (default 20000). Over it the result is cut at a whole node or line, `truncated` is true and the result says how to read the rest. |
| `nodeId` | string | yes | The parent node, or "root". |


### `get_tree_summary` (read-only)

Compact indented text tree of a subtree: type, name, id and size per node. Much cheaper than get_jsx for orientation. nodeId "root" summarizes the whole page. A second result block reports `estimatedTokens`; over `maxTokens` the tree is cut at a whole line and the block carries `truncated`, `nextOffset` (pass it as `offset`) and a hint.

| Argument | Type | Required | Notes |
|---|---|---|---|
| `maxTokens` | number |  | Token budget of the result (default 20000). Over it the result is cut at a whole node or line, `truncated` is true and the result says how to read the rest. |
| `depth` | number |  | Levels shown, counting nodeId (or each top-level layer for "root") as level 1 (default 3, max 10). Deeper levels collapse to "… N children". |
| `fullText` | boolean |  | Show each text layer's complete text instead of its first 40 characters (for copy review). |
| `nodeId` | string | yes | Root of the summary, or "root". |
| `offset` | number |  | Line to start at: the `nextOffset` of a truncated result. |

Depth counts from the node you pass, not from the page. For an artboard `Home` with a `Screens` frame holding rows, `{"nodeId": "<Home id>", "depth": 2}` stops at `Screens`:

```
frame "Home" [3fa9c1:1] 1440×900
  frame "Screens" [3fa9c1:2] fill(1440)×hug(820)
    … 3 children
```

The rows inside `Screens` are level 3, so they need `depth: 3` from `Home` (the default), or `depth: 2` from `Screens` itself. With `nodeId: "root"` every artboard is level 1, so the same rows also need `depth: 3`.


### `get_jsx` (read-only)

Export a node and its descendants to code: "inline" (default, React JSX with inline styles), "tailwind" (React JSX with Tailwind CSS utility classes), "react" (React TSX component), "angular" (Angular v17+ standalone component), "html" (self-contained <style> + HTML markup), or "egui" (Rust egui 0.36 code with tokens). A second result block reports `estimatedTokens`; over `maxTokens` the code is cut at a whole line and the block carries `truncated`, `nextOffset` (pass it as `offset`) and a hint to export a narrower node.

| Argument | Type | Required | Notes |
|---|---|---|---|
| `maxTokens` | number |  | Token budget of the result (default 20000). Over it the result is cut at a whole node or line, `truncated` is true and the result says how to read the rest. |
| `format` | `inline` \| `inline-styles` \| `tailwind` \| `react` \| `angular` \| `html` \| `egui` |  | Output format: "inline" (default), "tailwind", "react", "angular", "html", or "egui". |
| `nodeId` | string | yes | The node to convert. |
| `offset` | number |  | Line to start at: the `nextOffset` of a truncated result. |


### `get_design_spec` (read-only)

Export a deterministic AI Design-to-Code specification for a frame or subtree. Includes all design tokens/variables, component instances, exact auto-layout tree (sizing, flex/grid/egui flow, gaps, paddings, bound `--var` colors), mandatory enforcement rules, and reference target code. Always call this before implementing a design in React, Tailwind, Angular, HTML/CSS, or Rust egui instead of guessing from a screenshot. Reports `estimatedTokens` (json format: a field; other formats: a second result block); over `maxTokens` the output is cut at a whole line (json: whole array elements) and flagged `truncated` with how to read the rest.

| Argument | Type | Required | Notes |
|---|---|---|---|
| `maxTokens` | number |  | Token budget of the result (default 20000). Over it the result is cut at a whole node or line, `truncated` is true and the result says how to read the rest. |
| `format` | `markdown` \| `json` |  | Output format: "markdown" (default) or "json". |
| `nodeId` | string | yes | Root frame or node id to export. |
| `offset` | number |  | Line to start at: the `nextOffset` of a truncated result. |
| `target` | `react` \| `tailwind` \| `angular` \| `html` \| `egui` |  | Target UI framework (default "react"). |


### `verify_code` (read-only)

Verify that AI-generated UI code (`react`, `tailwind`, `angular`, `html`, or `egui`) accurately implements a design subtree. Checks for missing text strings, hardcoded hex colors where `--var` or `tokens::*` must be used, forbidden `position: absolute` on auto-layout flow children, missing flex/grid/egui layout containers, unreferenced component instances, and spacing drift. Returns `{passed, score, errors, warnings, violations}`.

| Argument | Type | Required | Notes |
|---|---|---|---|
| `code` | string | yes | The generated UI source code to verify against the design. |
| `nodeId` | string | yes | Root frame or node id that the code implements. |
| `target` | `react` \| `tailwind` \| `angular` \| `html` \| `egui` | yes | Target UI framework. |


### `diff_frames` (read-only)

Compare two frames or subtrees (`beforeId` and `afterId`) and report property-level differences (size, position, fills, stroke, corner radius, opacity, text, layout, added/removed children).

| Argument | Type | Required | Notes |
|---|---|---|---|
| `afterId` | string | yes | Second frame or node id (comparison). |
| `beforeId` | string | yes | First frame or node id (baseline). |


### `get_computed_styles` (read-only)

CSS styles (camelCase) of one or more nodes, as {nodeId: styles}.

| Argument | Type | Required | Notes |
|---|---|---|---|
| `nodeIds` | array of string | yes | Nodes to read. |


### `find_nodes` (read-only)

Find nodes by text content and/or computed style. Searches the whole project by default; pass pageId for one page or nodeId for a subtree. `textValue` finds text containing it, ignoring case and line breaks; with * wildcards it must match the whole value ("Get *"). `filters` are {styleName, styleValue} pairs (AND), values may use * wildcards; colors match by value.

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


### `install_font`

Add a font and keep it in the open project, so it travels with the project. Takes one .ttf or .otf per call (one style of a family; a variable font gives its default style); call again for its bold and italic files. Returns the family; use it as font-family in write_html and update_styles. Check the font's licence allows the use.

| Argument | Type | Required | Notes |
|---|---|---|---|
| `source` | string | yes | A local file path, or an http(s) URL of the font file itself (not a web page, zip or WOFF). |

### `get_font_family_info` (read-only)

Whether font families are available and their weights/styles. Inter (400, 700, normal and italic) is built in and families added with install_font follow; others fall back to Inter.

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

Render a node (or the whole page when nodeId is omitted or "root") to a PNG and return it as an image. Scale 1 (default) is enough for layout checks; use 2 to read small text. Large images are scaled down to fit API limits.

| Argument | Type | Required | Notes |
|---|---|---|---|
| `annotations` | boolean |  | Also draw the note cards, badges and leader lines of notes on the captured layers; the image grows to include the cards. |
| `mode` | string |  | Render in this variable mode (e.g. "Light") without switching the document: preview every theme of one artboard while other agents keep working in the current mode. |
| `nodeId` | string |  | The node to capture; omit for the whole page. |
| `out` | string |  | Optional file path to save the PNG to (also used by papercraft-cli). |
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


### `export_presentation`

Export pages as a presentation PDF, the way a designer sends work to a client: a cover, contents, a divider per page, one 1920×1080 slide per screen (its name and size; narrow side panels share a slide) and a closing slide, numbered. Screens are the page's declared screens, else its top-level frames of 300×300 or more; a section holding no screen (a design-system board) gets a slide of its own. They stay vector. Returns the file path.

| Argument | Type | Required | Notes |
|---|---|---|---|
| `pageIds` | array of string |  | Pages to include, in this order (default: every page). |
| `path` | string |  | Output file (default: Downloads/<title>.pdf). |
| `title` | string |  | Deck title (default: the project name). |
| `subtitle` | string |  | Cover subtitle (default: screen and page counts). |
| `author` | string |  | Shown on the cover after the date. |
| `language` | string |  | Language of the deck's own words and date: en, pt, es, fr, de, it (default: the one the screens are written in). |


### `mark_handoff`

Mark a handoff point: a named snapshot of every page, taken when screens go to development. get_handoff_changes and export_handoff `since` then report what changed against what the developers received. Marking a name again moves it to now. Saved in the project (handoffs/). Returns {name, pages, screens}.

| Argument | Type | Required | Notes |
|---|---|---|---|
| `name` | string | yes | Name of the point, e.g. "design v1" or a release tag. |


### `list_handoffs` (read-only)

The project's handoff points (mark_handoff), newest first: [{name, saved}].


### `get_handoff_changes` (read-only)

What changed since a handoff point, sorted the way code acts on it, in few tokens: token values once (regenerate the theme), main components once with the screens using them (instances follow), then screens: added, removed, renamed, and per changed layer its kind (copy, binding, style, layout, structure, instance, note) and `field: old → new` with token names. Sizes that follow content, positions set by auto layout and instance fields that follow their main are left out. Markdown shows 8 changes per screen; pass `nodeId` (a screen or component key from it) for all of one. Layer keys work with get_design_spec {nodeId, format: compact} to fetch just that subtree.

| Argument | Type | Required | Notes |
|---|---|---|---|
| `format` | `markdown` \| `json` |  | markdown (default, an index) or json (everything). |
| `nodeId` | string |  | Only this screen or main component, every change. |
| `since` | string | yes | Handoff point name (list_handoffs). |


### `export_handoff`

Write the handoff bundle a code base builds from into a folder: index.md (one line per screen with its spec file and size in tokens), inventory.json, tokens.json (DTCG with modes), tokens.css, tokens.tailwind.css, components.json, open-questions.md (`? (team)` notes and the table on a page named Handoff), and per screen a compact spec (screens/<page>/<screen>.txt) and a reference PNG at scale 1 (a scrolling screen shown whole). With `since`, also changes.md and changes.json (get_handoff_changes). manifest.json lists each file's hash and tokens. Only files whose bytes changed are written, and files a previous export wrote that no longer exist are removed; nothing else in the folder is touched. Returns {dir, files, written, removed, screens, images}.

| Argument | Type | Required | Notes |
|---|---|---|---|
| `dir` | string | yes | Folder to write into (created if missing), e.g. docs/design. |
| `images` | boolean |  | Write reference PNGs (default true). |
| `since` | string |  | Handoff point to report changes against. |


### `export_site`

Write frames of a page as a website in one call; returns a short summary, never the markup. Before the first export ASK THE PERSON which `framework`, `render` and `styling` they want, and recommend from what they have: the framework their repo or team uses (read package.json), else html for a finished site nobody will develop or astro for a content site they will grow. Client JavaScript per page (gzip): html and astro 0; static vue ~71 KB, angular ~77 KB, react ~135 KB; spa vue ~40 KB, angular ~63 KB, react ~97 KB. render static (prerendered) for anything public that needs search or link previews, spa only for an app behind a login. styling: what their project uses, else css; scss and tailwind work everywhere; CSS-in-JS is react only: vanilla-extract (recommended, zero runtime), styled-components or emotion. Defaults html, static, css. One page per frame (the first is the home page) with semantic tags and one h1, <img> alt and link labels from layer names, self-hosted fonts, Open Graph and robots.txt; `base_url` makes canonical and hreflang absolute and adds sitemap.xml. Prototype links to exported frames and Url links become links. With `modes` (a collection whose modes are languages such as en and pt-BR) each mode is a locale: text that differs becomes a message per locale, styles that differ become :lang() rules. A project (any framework, scss, tailwind) needs a build: follow `next_steps`. The folder must be new, empty or an earlier export_site folder; a re-export removes only what the previous one wrote. Per framework: [export-site.md](export-site.md). Returns {dir, framework, render, styling, pages, files, bytes, written, removed, links: {resolved, unresolved}, urls, next_steps, warnings}.

| Argument | Type | Required | Notes |
|---|---|---|---|
| `path` | string | yes | Folder to write into (created if missing). |
| `frames` | array |  | Frame ids, in page order. Default: the page's screens (set_screens), else its top-level frames. |
| `modes` | object |  | `{collection, modes: [..]}`: collection name or id and the mode names to export, e.g. ["en", "pt-BR"]. |
| `base_url` | string |  | Where the site will be served (https://example.com): absolute hreflang and canonical URLs, and sitemap.xml. |
| `framework` | string |  | Output: html (static pages, no build step), or a project for react, vue, angular or astro. Ask the person; default html. |
| `render` | string |  | static: pages pre-rendered for search (landing, marketing, docs). spa: client-rendered app (behind a login). Only for react, vue and angular. Ask the person; default static. |
| `styling` | string |  | css (default, no build step), scss, tailwind, or CSS-in-JS for react only: vanilla-extract (recommended, zero runtime), styled-components, emotion. Ask the person; use what their project uses. |
| `lang` | string |  | Language tag of a single-language site (en, pt-BR): `<html lang>` and og:locale. |
| `title` | string or object |  | Site title: the home page title and the suffix of the others. Default: the frame names. With `modes`, an object keyed by mode name (`{"en": "...", "pt-BR": "..."}`) gives each language its own; a language without an entry takes the base language's. |
| `description` | string or object |  | Meta description for every page. Default: each page's first line of copy. With `modes`, an object keyed by mode name gives each language its own, as for `title`. |
| `pageId` | string |  | Page to export from (default the active page). |


## Write

### `create_artboard`

Creates an artboard (top-level frame) and returns its id; then add content with write_html({mode: "insert-children"}). `styles` are camelCase CSS; width and height are required (e.g. "1440px"). Artboards default to display:flex, flexDirection:column and a white background, and are placed in an empty spot unless left/top are given. Default sizes: desktop 1440×900, tablet 768×1024, mobile 390×844. At a device width, height "fit-content" becomes the device height; content taller than a screen scrolls in its main column (else the screen). overflow "visible" lets the board grow instead.

| Argument | Type | Required | Notes |
|---|---|---|---|
| `name` | string | yes | Layer name. |
| `styles` | object | yes | CSS styles, e.g. {"width": "390px", "height": "844px", "backgroundColor": "#f5f5f5", "padding": "24px", "gap": "16px"} |


### `write_html`

Parse HTML with inline styles into layers. Write incrementally: each call should add one visual group (a header, one list row, a card) so the user sees progress.
- mode "insert-children" appends the HTML inside targetNodeId ("root" = the page); "replace" replaces targetNodeId with it.
- Inline styles only. Flexbox (display:flex, flex-direction, gap, padding, align-items, justify-content, align-self, flex-grow, flex-wrap) is the layout model; block elements stack vertically. position:absolute with left/top is supported. Margins become gaps or spacers, display:grid becomes rows of cells, text inputs show their placeholder (each reported in warnings). No tables.
- Text: font-size, font-weight (≥600 = bold), font-style, color, line-height, letter-spacing, text-align, text-decoration (including wavy and text-decoration-color); <b>/<i>/<u>/<s>/<span style> make rich text. font-family applies to a whole text layer: the first installed family in the list, Inter by default.
- Boxes: background(-color/-image: colors, linear/radial gradients, url()), border (and border-top/right/bottom/left), border-radius, opacity, overflow:hidden, mix-blend-mode, width/height (px, %, fit-content), min-width/max-width/min-height/max-height.
- <img src> takes paper-asset:///abs/path, file:// URLs, absolute paths, data: URIs or public http(s) URLs (PNG, JPEG, GIF or WebP up to 10 MB; private and loopback addresses are refused and a failed download is a warning). <svg> markup is imported as vector layers.
- <x-paper-icon name="set:name" size="20" color="var(--accent)"/> places an installed icon (search_icons); <x-paper-instance component="Name" component-id="ID" layer-name="..."/> places a component instance; colors may be var(--name) of set_variables.
- layer-name="..." names layers; <x-paper-clone node-id="ID" style="..."/> clones an existing node (supports cross-page `<page>:<id>`).
Returns the ids of the created top-level layers and the styles that were ignored.

| Argument | Type | Required | Notes |
|---|---|---|---|
| `html` | string | yes | HTML to parse into layers. |
| `mode` | `insert-children` \| `replace` | yes |  |
| `targetNodeId` | string | yes | Target node id, or "root" for the page. |

### `import_html`

Open an HTML page exported by a prompt-to-design tool as layers: a .html file or a .zip with its stylesheets and images. `<style>` blocks and linked stylesheets are inlined (tag, .class and #id selectors; media queries and combinators are reported), images in the zip become assets, and each HTML file is written with write_html into its own new top-level frame on the page (side by side, named after the file). Returns the frames per file, `ignoredStyles` grouped by property and `warnings`.

| Argument | Type | Required | Notes |
|---|---|---|---|
| `path` | string | yes | A local .html or .zip file. |
| `pageId` | string | no | The page for the frames. Default: the active page. |


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
| `nodes` | array of object | yes |  |


### `move_nodes`

Move nodes keeping their ids (or onto another page via `toPageId` or a cross-page `parentId` / `before` / `after`). Each move is {nodeId, before: siblingId}, {nodeId, after: siblingId} or {nodeId, parentId, toPageId?, index?} (parentId "root" = the page root). Moves apply in order. In auto-layout parents this changes the visual order; in free parents it changes stacking and keeps the world position. Moving to another page copies the layers there (new ids; each page has its own undo history) and keeps component links and links between the moved layers; prototype links and connector ends that would cross pages are cleared and reported in `warnings`.

| Argument | Type | Required | Notes |
|---|---|---|---|
| `moves` | array of object | yes |  |


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
| `route` | `straight` \| `curved` \| `elbow` |  |  |
| `sides` | integer |  | Polygon sides / star points. |
| `startHead` | string |  |  |
| `startNodeId` | string |  |  |
| `styles` | object |  | CSS and hand-drawn styles. |
| `text` | string |  | Text for sticky and text. |
| `type` | `rectangle` \| `ellipse` \| `diamond` \| `triangle` \| `polygon` \| `star` \| `line` \| `arrow` \| `sticky` \| `text` | yes |  |
| `width` | number |  |  |
| `x` | number |  |  |
| `y` | number |  |  |


## Design system

### `get_variables` (read-only)

Variables of the page's project. Default JSON: `variables` (name -> hex), `tokens`, and `collections` (id, name, modes, defaultMode, variables with id, name, cssName, group, type color|number|string|boolean, values per mode name, aliases as {"alias": name}, description, hidden) plus `pageModes` (active mode per collection on this page). Fills that follow a variable were written with `var(--name)`; set_variables moves all of them at once. `format` css returns a `:root { --name: #hex }` block (plus `[data-theme]` rules for modes), json a W3C DTCG object (groups nest on `/`, aliases are {group.name}), and tailwind a Tailwind v4 `@theme` block.

| Argument | Type | Required | Notes |
|---|---|---|---|
| `format` | `css` \| `json` \| `tailwind` |  |  |


### `get_strings` (read-only)

The page text for translation, with no styles or geometry. Returns `strings` (text layers bound to a string variable, by variable name, with the value per mode name), `unbound` (layer id -> text of text layers bound to nothing; set_strings {extract} binds them), `missing` (per mode name, the keys with no value of their own, which show the base text) and `collections`. `missing` set to a mode name returns only {mode, base, missing: {key: base text}}, so a translation is done in steps. Scope: `frame` (a layer id; default every layer of the page), `pageId`; `collection` keeps one collection.

| Argument | Type | Required | Notes |
|---|---|---|---|
| `collection` | string |  | Collection name or id to keep. |
| `frame` | string |  | Layer id to read inside. Omitted = the whole page. |
| `missing` | string |  | A mode name: only the keys that mode lacks, with their base text. |


### `set_variables`

Define or change variables (names like `surface`, `accent-text`, or grouped `color/brand/primary`), on every page. Plain {"name": "#hex"} defines a color. An entry may be an object {value, type, collection, mode, alias, description, hidden}: `type` color|number|string|boolean (numbers bind as px), `collection` a name or id (created if missing), `alias` the name of another variable of the same type. Use them as `var(--name)` in write_html / update_styles / create_artboard colors (backgroundColor, background, color) instead of hex literals, so a theme change is one call. Fills already following a changed variable are updated. Variables apply to fills and text color; strokes and gradients keep fixed colors.

| Argument | Type | Required | Notes |
|---|---|---|---|
| `collection` | string |  | Default collection (name or id) for entries that name none. |
| `mode` | string |  | Store the values for this mode (e.g. "Light", "Dark"); they apply when it is the active mode. The first mode you define becomes active. |
| `type` | `color` \| `number` \| `string` \| `boolean` |  | Default type for entries that name none. |
| `variables` | object | yes | {"accent": "#cba6f7"} or {"space/md": {"value": 16, "type": "number"}, "accent-hover": {"alias": "accent"}} |


### `create_collection`

Create a variable collection (a set of variables with modes), e.g. `Brand` with modes ["Light", "Dark"]. Without `modes` it has one mode, `Default`. Then fill it with set_variables {collection}.

| Argument | Type | Required | Notes |
|---|---|---|---|
| `modes` | array of string |  |  |
| `name` | string | yes |  |


### `delete_variables`

Delete variables by name. A variable another one aliases is refused (the error names the dependents) unless `force` is true, which gives the dependents its resolved value.

| Argument | Type | Required | Notes |
|---|---|---|---|
| `force` | boolean |  |  |
| `names` | array of string | yes |  |


### `set_tokens`

Define non-color design tokens (spacing, radius, font size, shadows, widths), on every page: {"spacing-md": "16px", "radius-md": "8px", "shadow-md": "0 4px 12px rgba(0,0,0,0.08)"}. Use `var(--spacing-md)` in any style value (padding: var(--spacing-sm) var(--spacing-md), boxShadow: var(--shadow-md)); it is replaced by the value when written, so tokens are a shared vocabulary, not live links. Listed by get_variables.

| Argument | Type | Required | Notes |
|---|---|---|---|
| `tokens` | object | yes |  |


### `bind_variable`

Bind a field of layers to a variable (or detach it). `field`: stroke, text-color, width, height, gap, padding, padding-top/right/bottom/left, radius, font-size, line-height, opacity, visible, text (a text layer's content, from a string variable; typing into the layer detaches it). `variable` is a name like color/brand/primary or its css name; null or omitted detaches (the current value stays). The value applies at once; on an instance the field becomes an override.

| Argument | Type | Required | Notes |
|---|---|---|---|
| `field` | string | yes |  |
| `nodeIds` | array of string | yes | Layers to bind. |
| `variable` | string \| null |  |  |


### `set_strings`

Translate a page in one undoable call; languages are modes of a string collection, never copies. Does not switch the active mode (set_variable_mode does). One of: (a) `extract` true with `frame` (default the page) and `collection` (default `Strings`, created with a base mode named `mode`, default `Default`): every unbound text layer gets a string variable named from its layer path (`hero/title`, repeats numbered), holding its text in the base mode, and is bound to it; returns the keys with their texts. (b) `collection`, `mode` and `strings` {key: text}: sets those values, creating the mode if missing; an unknown key, a non-string variable or an unknown collection is an error that lists the offenders and changes nothing. (c) `collection`, `mode` and `pseudo` true: fills the mode with the base text accented, about 35% longer and in brackets, to show overflow before a real translation. Bound text re-fits when the mode is switched, so run lint_design and get_screenshot per mode.

| Argument | Type | Required | Notes |
|---|---|---|---|
| `collection` | string |  | Collection name or id. |
| `extract` | boolean |  |  |
| `frame` | string |  | extract: layer id to read inside. Omitted = the whole page. |
| `mode` | string |  | Mode name (the language); extract: the base mode's name when the collection is created. |
| `pseudo` | boolean |  |  |
| `strings` | object |  | {key: text}, keys as returned by get_strings. |


### `set_variable_mode`

Switch the active variable mode (defined with set_variables {mode}): every variable takes that mode's value and all fills following them change, on every page. One call turns a whole design from light to dark.

| Argument | Type | Required | Notes |
|---|---|---|---|
| `mode` | string | yes |  |


### `import_design_system`

Import an OpenDesign design-system package: a folder with manifest.json, tokens.css and (optional) DESIGN.md. The colors of the `:root` block become variables, the colors of the `[data-theme="name"]` blocks become modes and every other value becomes a token. Replaces the variables, modes and tokens of every page in one undo step. Only hex colors become variables; oklch(), rgb() and sizes stay tokens.

| Argument | Type | Required | Notes |
|---|---|---|---|
| `path` | string | yes | Folder of the package to import. |


### `export_design_system`

Write the page's variables, modes and tokens as an OpenDesign design-system package (manifest.json, tokens.css and DESIGN.md) in `path`, so it can be read back with import_design_system or by other tools. Returns the files written.

| Argument | Type | Required | Notes |
|---|---|---|---|
| `path` | string | yes | Folder to write the package into. |


### `get_components` (read-only)

Main components of every page (or one with `pageId`): id, name, `componentKey` (stable), `pageId`, `pageName`, size, and `uses` (instances across all pages). Reports `estimatedTokens`; over `maxTokens` the list is cut (`truncated`).

| Argument | Type | Required | Notes |
|---|---|---|---|
| `maxTokens` | number |  | Token budget of the result (default 20000). Over it the result is cut at a whole node or line, `truncated` is true and the result says how to read the rest. |


### `import_components`

Copies mains onto this page (detached from the source); only for moving a design system, not for using components: create_instance and swap_instance take a main on any page or an enabled library directly, with no import. Copies the main components anywhere on `fromPageId` (including inside artboards); components with the same name already here are skipped. Variables and tokens are already shared by every page.

| Argument | Type | Required | Notes |
|---|---|---|---|
| `fromPageId` | string | yes |  |
| `names` | array of string |  | Only these components (default all). |


### `create_component`

Turn a frame or artboard into a main component. Build reusable parts (button, row, chip, tab) once as components, then place them with create_instance or <x-paper-instance>. Instances may live on any page and follow the main.

| Argument | Type | Required | Notes |
|---|---|---|---|
| `name` | string |  | Optional new layer name. |
| `nodeId` | string | yes | The frame to convert. |


### `create_instance`

Place an instance of a component. componentKey may be a component on any page or an enabled library; no import needed (the instance stays linked to that main). Edit text and styles on the instance's descendants with set_text_content / update_styles (they become overrides); edits to the main component flow to every instance. Returns the instance id and its text layers.

| Argument | Type | Required | Notes |
|---|---|---|---|
| `componentId` | string |  | A main on any page ("pageId:num"). |
| `componentKey` | string |  | Instead of componentId: the key from get_components. |
| `index` | number |  | Position among the parent's children. |
| `name` | string |  |  |
| `parentId` | string |  | Parent frame; omit or "root" for the page. |
| `x` | number |  |  |
| `y` | number |  |  |


### `swap_instance`

Swap an instance root to point at a different main component, preserving overrides on descendants matched by name and layer type. componentKey may be a component on any page or an enabled library; no import needed.

| Argument | Type | Required | Notes |
|---|---|---|---|
| `componentId` | string |  | The new main component id. |
| `componentKey` | string |  | Instead of componentId: a key from get_components, including a library component. |
| `nodeId` | string | yes | The instance root node to swap. |


### `detach_instance`

Detach one or more component instances into plain frames, breaking their link to the main component while keeping their current appearance and children.

| Argument | Type | Required | Notes |
|---|---|---|---|
| `nodeIds` | array of string | yes | Instance root nodes to detach. |


### `set_prototype_links`

Sets what clicking a layer does in Present mode. Per link give at most one of `to`, `overlay`, `url` or `action`; none clears the layer's link. Every link is checked before any is applied. Links never cross pages.

| Argument | Type | Required | Notes |
|---|---|---|---|
| `links` | array | yes | Objects `{nodeId, to?, overlay?, url?, action?}`. `to`: a frame on the same page to go to (null removes). `overlay`: a frame opened centred over the current one with a dimmed backdrop (taller than the viewport scrolls; backdrop click or Escape closes). `url`: https, http, mailto or tel only. `action`: `back` (previous frame) or `close` (close the overlay). |

### `search_icons` (read-only)

Search installed open icon sets (Lucide, Tabler, Phosphor, Heroicons, Material). Returns `set:name` references for <x-paper-icon name="lucide:check" size="20" color="var(--text)"/> in write_html. Without a query it lists installed and installable sets; install with `papercraft-cli icons add lucide`.

| Argument | Type | Required | Notes |
|---|---|---|---|
| `limit` | number |  | Default 20. |
| `query` | string |  |  |
| `set` | string |  |  |


### `critique_design`

Critique finished work the way a design team does (offer it with ask_user when finish_working_on_nodes says the design changed). Judges against the person's objective, through independent reviewers: vision (the person's brief and notes), ux (Nielsen's heuristics), flow (coherent journeys across screens), qa (states and edge cases), visual, accessibility (W3C POUR), copy and end-user (a walkthrough as the audience). Returns the reviewers' briefs, the finding format and 0-4 severity scale, lint, the arrows between screens (dead ends, unreached screens), the person's lessons and the steps: review each persona independently, merge, fix severity 3-4, report, then ask which findings to keep as lessons.

| Argument | Type | Required | Notes |
|---|---|---|---|
| `nodeId` | string |  | Artboard or subtree to critique; omit for the whole page. |
| `objective` | string |  | What the design must achieve, for whom, and its top tasks (from the brief or the person). |
| `audience` | string |  | Who the end-user reviewer plays ("first-time shopper on a phone"). |
| `personas` | array of string |  | Reviewers to run (default: all): vision, ux, flow, qa, visual, accessibility, copy, end-user. |


### `design_lessons`

The person's design lessons: rules from past critiques that every design must follow (get_basic_info lists them too; read them before designing). Without arguments, lists those that apply in the open project. `add` stores one ONLY after the person approved it in this conversation (ask_user); never on your own. `remove` deletes one by id.

| Argument | Type | Required | Notes |
|---|---|---|---|
| `add` | object |  | `{rule, why, scope}`: rule is one line, an instruction; why is what went wrong; scope `all` (default) or `project`. |
| `remove` | string |  | Id of a lesson to delete ("L3"). |


### `lint_design`

Check a page or subtree for common design mistakes and get a fix for each: text contrast (WCAG 4.5:1, 3:1 for large text), text under 12px, font sizes off the type scale, gap/padding off the 4px grid, buttons and tabs under 44px tall, default layer names (Frame, Rectangle), layers or text overflowing their artboard or clipping parent (rule `overflow`, never auto-fixed; scrolling frames are exempt), text too narrow for its words (text-collapsed), and hard-coded colors that equal a variable. Run it before finish_working_on_nodes and fix errors and warnings; `info` items are suggestions. Pass `fix: true` to automatically snap spacing/padding to 4px, relink matching color variables, snap text sizes, and enlarge small touch targets.

| Argument | Type | Required | Notes |
|---|---|---|---|
| `fix` | boolean |  | Apply safe automatic fixes before reporting remaining issues. |
| `nodeId` | string |  | Subtree to check; omit for the whole page. |
| `scale` | array of number |  | Allowed font sizes. Default 12, 14, 16, 20, 24, 32, 40, 48, 64. |


## Libraries

### `publish_library`

Publish this project's variables (not hidden) and components (not named with a leading `_` or `.`) as a library other projects can enable. Writes library.json with the next version and returns what changed since the last publish.

| Argument | Type | Required | Notes |
|---|---|---|---|
| `description` | string |  | What the library is for (kept from the last publish when omitted). |


### `list_libraries` (read-only)

Libraries other projects published (`available`, with `enabled`) and the ones this project uses (`enabled`: version, sourceVersion, sourceFound, updateAvailable).


### `enable_library`

Use a published library: its variables become read-only, bindable and aliasable, and its components can be placed with create_instance componentKey. Refused when a CSS name is already defined here.

| Argument | Type | Required | Notes |
|---|---|---|---|
| `libraryId` | string | yes | From list_libraries. |


### `update_library`

Apply the library's newest publish to this project: variables and components are replaced and every instance of it re-synced, overrides kept. Returns the changes.

| Argument | Type | Required | Notes |
|---|---|---|---|
| `libraryId` | string | yes |  |


### `disable_library`

Stop using a library: bindings to its variables are detached (values kept), its instances become plain frames, its variables and components leave this project.

| Argument | Type | Required | Notes |
|---|---|---|---|
| `libraryId` | string | yes |  |


## Annotations

### `annotate`

Add notes to node ids or exact layer names on a page in one undo step. First text line is label; remaining lines are body. Type is an allowed category key (default rule); put fields as "Name: value" lines in text. Placement is automatic (tidy re-lays-out). Returns [number, node, type, label] rows; tidy alone returns {tidied: moved}.

| Argument | Type | Required | Notes |
|---|---|---|---|
| `frame` | string |  | With tidy: only the notes on this frame. |
| `notes` | array of object |  | `{node, text, type?}`. |
| `pageId` | string |  |  |
| `tidy` | boolean |  | Re-layout all notes (of frame, else the page) as part of this undo step; notes may then be omitted. |

Check the placement with `get_screenshot {"nodeId": "<frame>", "annotations": true}`.

### `get_annotation_template` (read-only)

Read the page's annotation template: [type key, guidance, field names] for each allowed type. Call once before annotate.

| Argument | Type | Required | Notes |
|---|---|---|---|
| `pageId` | string |  |  |


### `set_annotation_template`

Switch the annotation template by id or name, for one page or the whole project. Annotate only accepts the template's types.

| Argument | Type | Required | Notes |
|---|---|---|---|
| `pageId` | string |  |  |
| `scope` | string |  | `page` or `project`. |
| `template` | string | yes |  |


### `upsert_annotation_template`

Create or replace an annotation template, optionally defining new categories (key, name, #hex color, icon name) first. Reuse existing keys freely. One undo step for the edit; setAs page adds a second. Returns {id, name, types}.

| Argument | Type | Required | Notes |
|---|---|---|---|
| `categories` | array of object |  | Category styles `{key, name, color, icon}`; an existing key is updated, a new key needs all four. |
| `pageId` | string |  |  |
| `setAs` | string |  | `project` or `page`. |
| `template` | object | yes | `{id?, name, display?: full or badges, categories: [{key, enabled?, guidance?, fields?}]}`. |


### `delete_annotation_template`

Delete an annotation template by id or name. The last template cannot be deleted; a project default pointing at it reverts to Product spec.

| Argument | Type | Required | Notes |
|---|---|---|---|
| `template` | string | yes |  |


## Session

### `get_feedback` (read-only)

Notes the user left on layers (the Feedback box in the inspector, shown as amber dots on the canvas): which layer, where it sits, what to change. Omitting `pageId` queries all pages across the project; `openCount` and `resolvedCount` are returned alongside per-page counts in `get_basic_info`. Call it at the start of a session and after each round of edits; fix every open note, then resolve_feedback. `get_selection` also includes the selected layers' notes.

| Argument | Type | Required | Notes |
|---|---|---|---|
| `includeResolved` | boolean |  |  |
| `nodeId` | string |  | Only this layer. |


### `add_feedback`

Leave a note on a layer (author `agent` by default), e.g. a question or a problem you could not fix. The user sees it in the inspector and as a dot on the canvas.

| Argument | Type | Required | Notes |
|---|---|---|---|
| `author` | string |  |  |
| `nodeId` | string | yes |  |
| `text` | string | yes |  |


### `resolve_feedback`

Mark notes on a layer resolved, with a short reply saying what you changed. Without commentId every open note on the layer is resolved. `delete` removes instead.

| Argument | Type | Required | Notes |
|---|---|---|---|
| `commentId` | number |  |  |
| `delete` | boolean |  |  |
| `nodeId` | string | yes |  |
| `reply` | string |  |  |


### `select_nodes`

Select nodes on the page the user is viewing so they can see what you mean; zoom: true also frames them in the viewport (at most 100%). An empty list clears the selection. For another page or a closer zoom use set_viewport.

| Argument | Type | Required | Notes |
|---|---|---|---|
| `nodeIds` | array of string | yes | Nodes to select. |
| `zoom` | boolean |  |  |


### `set_viewport`

Move the user's view: switch to pageId (as clicking it in the page list does) and fit nodeIds or a world rect, optionally at an explicit zoom (0.01 to 256). With only zoom it zooms about the view's centre. Refused while the user is moving the canvas (within 10 s of their last pan or zoom) unless force: true. Returns the new viewport ({pageId, rect: [x, y, w, h] in world units, zoom}); get_basic_info and get_selection report the current one.

| Argument | Type | Required | Notes |
|---|---|---|---|
| `force` | boolean |  | Move the view even if the user is panning or zooming. |
| `nodeIds` | array of string |  | Nodes to fit in the view. |
| `padding` | number |  | Screen margin when fitting, in px (default 64). |
| `pageId` | string |  | Page to show; defaults to the page of nodeIds, else the current one. |
| `rect` | array of number |  | World rect [x, y, width, height] to fit, instead of nodeIds. |
| `zoom` | number |  | Explicit zoom level (1 = 100%); centres the nodes or rect at this zoom instead of fitting. |


### `undo`

Undo a change. An agent's undo (scope `mine`, its default) takes back its own newest step and leaves later steps by others in place; it is refused (`undo_blocked`, with the steps in the way) when one of them touched the same layers. Scope `page` undoes the page's last step, whoever made it.

| Argument | Type | Required | Notes |
|---|---|---|---|
| `scope` | string |  | `mine` or `page`. |


### `redo`

Redo the last undone change on a page. With scope `mine` (an agent's default) only when that step was the caller's own.

| Argument | Type | Required | Notes |
|---|---|---|---|
| `scope` | string |  | `mine` or `page`. |


### `claim_nodes`

Several agents on one project: claim the layers you will rewrite (each with everything under it) or a shared resource, so other agents' writes there are refused while you work. All or nothing; a denial names the holder and when it expires (never wait in a loop: do other work). The claim lasts ttlSeconds after your last call and goes with finish_working_on_nodes or release_claims. The person is never blocked. Needs the running app.

| Argument | Type | Required | Notes |
|---|---|---|---|
| `ids` | array of string |  | Layer ids. |
| `resources` | array of string |  | `variables`, `components`, `pages`. |
| `ttlSeconds` | integer |  | Default 300 (10 to 1800); renewed by each of your calls. |
| `note` | string |  | What you are doing, shown to others. |


### `release_claims`

Release your claims: one by claimId, those on ids or resources, or all of them when nothing is given.

| Argument | Type | Required | Notes |
|---|---|---|---|
| `claimId` | string |  |  |
| `ids` | array of string |  |  |
| `resources` | array of string |  |  |


### `list_claims` (read-only)

Who is working where: every live claim with its holder, layers, resources, note and seconds left.


### `finish_working_on_nodes`

Call when done creating or editing. Saves the project.

| Argument | Type | Required | Notes |
|---|---|---|---|
| `nodeIds` | array of string |  | Optional nodes you worked on. |

### `merge_variants` (variant sets)

Variant names of the form `Size=M, State=hover` (component names or `values`) build a variant set: one enum property per axis, each read from the variants that differ from the base in that axis alone, so axes combine additively. `set_instance_props` picks a value per axis; overrides survive the swap. The result lists `axes`.
### `set_accessibility`

Sets or clears accessibility annotations on layers: alt text, decorative mark, role, heading level and reading order. `get_node_info` shows them as `accessibility`; `lint_design` checks them (`image-alt`, `interactive-name`, `heading-skip`, `reading-order`); `export_site` writes them instead of inferring. All items are checked before any is applied.

| Argument | Type | Required | Notes |
|---|---|---|---|
| `items` | array | yes | Objects `{nodeId, alt?, decorative?, role?, level?, order?, clear?}`. `alt`: text alternative (or the name of an icon button). `decorative`: ornament, needs no alt. `role`: button, link, heading, list, nav, main, header, footer, aside or section. `level`: 1-6, only with role heading. `order`: reading position among siblings (frames without auto layout), lower first. null clears a field; `clear: true` removes all first. |
