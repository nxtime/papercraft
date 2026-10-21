//! Tool catalogue: names, descriptions and input schemas.
//!
//! Names and argument shapes follow Paper's MCP server where papercraft has an equivalent
//! (`analysis/paper-mcp.md` §2), so agent prompts written for Paper carry over. papercraft
//! additions: `create_shape` (whiteboard shapes and arrows), `select_nodes`, `undo`, `redo`.

use serde_json::{Value, json};

pub const INSTRUCTIONS: &str = "\
papercraft is a local design and whiteboard app with an infinite canvas. These tools read and \
edit the project open in the running app; every change is one undo step and shows up live.

- Start with get_basic_info (pages, artboards, sizes; `openFeedback` counts the user's notes) \
and get_selection (what the user is looking at); when openFeedback > 0 call get_feedback and \
fix each note, then resolve_feedback with a reply. Use list_resources / open_file / create_file to pick a project.
- For details read get_guide({ topic: \"papercraft-mcp-instructions\" }) once per session.
- pageId defaults to the page the user is viewing; pass it to work on another page without \
switching the user's view. Node ids are opaque strings; never show them to the user.
- Build UI with create_artboard, then write_html into it, one visual group per call (a header, \
a row, a card) so the user sees progress. Inline styles only; flexbox (display:flex, gap, \
padding, align-items, justify-content) is the layout model; no margin, grid or tables.
- Text uses Inter (regular, bold, italic). Use px for font sizes.
- Edit with update_styles, set_text_content, rename_nodes, duplicate_nodes, move_nodes, \
delete_nodes. Whiteboard shapes, sticky notes and arrows: create_shape. Hand-drawn style: \
update_styles with {\"sketch\": true, \"roughness\": 1, \"fillStyle\": \"hachure\"}.
- Don't write colors as hex in designs: set_variables once (surface, text, accent, ...), then \
use var(--name). Build repeated parts (button, row, chip) once as components \
(create_artboard + create_component) and place them with create_instance, overriding text \
with set_text_content. Icons: search_icons, then <x-paper-icon name=\"phosphor:gear\"/> in \
write_html. Name every layer (layer-name) like a real file.\n\
- Design-system packages: import_design_system reads an OpenDesign package folder \
(manifest.json + tokens.css) into the variables, modes and tokens of the project; \
export_design_system writes one back.\n\
- Check your work with get_screenshot. Artboard height is a starting point: when content \
clips set height: \"fit-content\" with update_styles.
- Call finish_working_on_nodes when done.";

fn file_id() -> Value {
    json!({"type": "string", "description": "The project to operate on (id, name or folder path from list_resources). When omitted the project open in the app is used."})
}

fn page_id() -> Value {
    json!({"type": "string", "description": "The page to operate on. When omitted the page the user is viewing is used. The user's page can change between calls; prefer explicit page ids."})
}

fn styles_schema(desc: &str) -> Value {
    json!({
        "type": "object",
        "propertyNames": {"type": "string"},
        "additionalProperties": {"anyOf": [{"type": "string"}, {"type": "number"}, {"type": "boolean"}]},
        "description": desc,
    })
}

fn tool(name: &str, desc: &str, props: Value, required: &[&str], read_only: bool) -> Value {
    let mut props = props;
    if let Some(m) = props.as_object_mut()
        && !m.contains_key("fileId")
        && !matches!(
            name,
            "list_resources" | "create_file" | "get_font_family_info" | "get_guide" | "open_file"
        )
    {
        m.insert("fileId".into(), file_id());
    }
    let ann = if read_only {
        json!({"readOnlyHint": true})
    } else if name == "delete_nodes" {
        json!({"destructiveHint": true, "readOnlyHint": false})
    } else {
        json!({"destructiveHint": false, "readOnlyHint": false})
    };
    json!({
        "name": name,
        "description": desc,
        "inputSchema": {
            "type": "object",
            "properties": props,
            "required": required,
        },
        "annotations": ann,
    })
}

/// Every tool, as MCP `Tool` JSON objects.
pub fn definitions() -> Vec<Value> {
    let node_id = |d: &str| json!({"type": "string", "description": d});
    let ids = |d: &str| json!({"type": "array", "items": {"type": "string"}, "description": d});
    vec![
        // ------------------------------------------------------------ projects and pages
        tool(
            "list_resources",
            "Lists the user's papercraft projects (local folders), most recently modified first. \
             Each item has id, name, path, page count, updatedAt, and `open` for the project open \
             in the app.",
            json!({"limit": {"type": "integer", "description": "Maximum projects to return. Defaults to 50."}}),
            &[],
            true,
        ),
        tool(
            "open_file",
            "Opens a project in the app (saving the current one) and returns a get_basic_info \
             response. Optional pageId switches the user to that page.",
            json!({
                "fileId": {"type": "string", "minLength": 1, "description": "Project id, name or folder path (see list_resources)."},
                "pageId": {"type": "string", "description": "Optional page to show."},
            }),
            &["fileId"],
            false,
        ),
        tool(
            "create_file",
            "Creates a new empty project and returns its id. Call open_file with the id to work in it.",
            json!({"name": {"type": "string", "description": "Optional project name."}}),
            &[],
            false,
        ),
        tool(
            "create_page",
            "Creates a new page at the end of the page list and returns its id, without switching \
             the user's page. Use the returned pageId in later calls.",
            json!({"name": {"type": "string", "description": "Optional page name. Defaults to \"Page N\"."}}),
            &[],
            false,
        ),
        tool(
            "rename_pages",
            "Rename one or more pages. Does not switch the user's page.",
            json!({"updates": {"type": "array", "description": "Page renames.", "items": {
                "type": "object",
                "properties": {"pageId": {"type": "string"}, "name": {"type": "string"}},
                "required": ["pageId", "name"],
            }}}),
            &["updates"],
            false,
        ),
        tool(
            "delete_pages",
            "Delete one or more pages. The last page of a project cannot be deleted. A page that \
             still has layers is only deleted with `confirm: true`; check what is on it first \
             (get_basic_info lists node counts).",
            json!({
                "pageIds": {"type": "array", "items": {"type": "string"}, "description": "Pages to delete."},
                "confirm": {"type": "boolean", "description": "Required to delete a page that has layers."},
            }),
            &["pageIds"],
            false,
        ),
        // ------------------------------------------------------------ reading
        tool(
            "get_basic_info",
            "Essential context about the open project: project name, the page, node count, \
             artboards (top-level frames) with ids, sizes and world positions, other top-level \
             layers, fonts, and `pages` with the active one marked isActive.\n\
             - worldX/worldY are world positions; x/y are relative to the parent.\n\
             - Pass pageId to inspect another page without switching the user's view.",
            json!({"pageId": page_id()}),
            &[],
            true,
        ),
        tool(
            "get_selection",
            "The selected layers on the page the user is viewing: ids, names, types, sizes, \
             positions and the artboard each belongs to.",
            json!({}),
            &[],
            true,
        ),
        tool(
            "get_node_info",
            "Details of one node: type, name, size, visibility, lock state, parent, children ids, \
             text content, x/y (relative to parent) and worldX/worldY. Errors if it doesn't exist.",
            json!({"nodeId": node_id("The node to inspect.")}),
            &["nodeId"],
            true,
        ),
        tool(
            "get_children",
            "Direct children of a node (or of the page root when nodeId is \"root\"), with ids, \
             names, types, child counts and positions.",
            json!({"nodeId": node_id("The parent node, or \"root\"."), "pageId": page_id()}),
            &["nodeId"],
            true,
        ),
        tool(
            "get_tree_summary",
            "Compact indented text tree of a subtree: type, name, id and size per node. Much \
             cheaper than get_jsx for orientation. nodeId \"root\" summarizes the whole page.",
            json!({
                "nodeId": node_id("Root of the summary, or \"root\"."),
                "depth": {"type": "number", "description": "Maximum depth (default 3, max 10)."},
                "pageId": page_id(),
            }),
            &["nodeId"],
            true,
        ),
        tool(
            "get_screenshot",
            "Render a node (or the whole page when nodeId is omitted or \"root\") to a PNG and \
             return it as an image. Scale 1 (default) is enough for layout checks; use 2 to read \
             small text. Large images are scaled down to fit API limits.",
            json!({
                "nodeId": node_id("The node to capture; omit for the whole page."),
                "scale": {"type": "number", "description": "Render scale: 1 (default) or 2."},
                "out": {"type": "string", "description": "Optional file path to save the PNG to (also used by papercraft-cli)."},
                "pageId": page_id(),
            }),
            &[],
            true,
        ),
        tool(
            "get_jsx",
            "Export a node and its descendants to code: \"inline\" (default, React JSX with inline \
             styles), \"tailwind\" (React JSX with Tailwind CSS utility classes), \"react\" (React \
             TSX component), \"angular\" (Angular v17+ standalone component), \"html\" \
             (self-contained <style> + HTML markup), or \"egui\" (Rust egui 0.36 code with tokens).",
            json!({
                "nodeId": node_id("The node to convert."),
                "format": {"type": "string", "enum": ["inline", "inline-styles", "tailwind", "react", "angular", "html", "egui"], "description": "Output format: \"inline\" (default), \"tailwind\", \"react\", \"angular\", \"html\", or \"egui\"."},
                "pageId": page_id(),
            }),
            &["nodeId"],
            true,
        ),
        tool(
            "get_design_spec",
            "Export a deterministic AI Design-to-Code specification for a frame or subtree. \
             Includes all design tokens/variables, component instances, exact auto-layout tree \
             (sizing, flex/grid/egui flow, gaps, paddings, bound `--var` colors), mandatory \
             enforcement rules, and reference target code. Always call this before implementing \
             a design in React, Tailwind, Angular, HTML/CSS, or Rust egui instead of guessing \
             from a screenshot.",
            json!({
                "nodeId": node_id("Root frame or node id to export."),
                "target": {"type": "string", "enum": ["react", "tailwind", "angular", "html", "egui"], "description": "Target UI framework (default \"react\")."},
                "format": {"type": "string", "enum": ["markdown", "json"], "description": "Output format: \"markdown\" (default) or \"json\"."},
                "pageId": page_id(),
            }),
            &["nodeId"],
            true,
        ),
        tool(
            "verify_code",
            "Verify that AI-generated UI code (`react`, `tailwind`, `angular`, `html`, or `egui`) \
             accurately implements a design subtree. Checks for missing text strings, hardcoded \
             hex colors where `--var` or `tokens::*` must be used, forbidden `position: absolute` \
             on auto-layout flow children, missing flex/grid/egui layout containers, unreferenced \
             component instances, and spacing drift. Returns `{passed, score, errors, warnings, violations}`.",
            json!({
                "nodeId": node_id("Root frame or node id that the code implements."),
                "target": {"type": "string", "enum": ["react", "tailwind", "angular", "html", "egui"], "description": "Target UI framework."},
                "code": {"type": "string", "description": "The generated UI source code to verify against the design."},
                "pageId": page_id(),
            }),
            &["nodeId", "target", "code"],
            true,
        ),
        tool(
            "get_computed_styles",
            "CSS styles (camelCase) of one or more nodes, as {nodeId: styles}.",
            json!({"nodeIds": ids("Nodes to read.")}),
            &["nodeIds"],
            true,
        ),
        tool(
            "get_fill_image",
            "The image used by a node's image fill, as image content.",
            json!({"nodeId": node_id("A node with an image fill.")}),
            &["nodeId"],
            true,
        ),
        tool(
            "find_nodes",
            "Find nodes by text content and/or computed style. Searches the whole project by \
             default; pass pageId for one page or nodeId for a subtree. `textValue` matches text \
             case-insensitively with * wildcards anchored to the whole value. `filters` are \
             {styleName, styleValue} pairs (AND), values may use * wildcards; colors match by value.",
            json!({
                "pageId": page_id(),
                "nodeId": node_id("Optional subtree root; takes priority over pageId."),
                "textValue": {"type": "string"},
                "filters": {"type": "array", "items": {"type": "object", "properties": {
                    "styleName": {"type": "string"}, "styleValue": {"type": "string"}}}},
            }),
            &[],
            true,
        ),
        tool(
            "get_font_family_info",
            "Whether font families are available and their weights/styles. papercraft renders text \
             with Inter (400, 700, normal and italic); other families fall back to Inter.",
            json!({"familyNames": {"type": "array", "items": {"type": "string"}}}),
            &["familyNames"],
            true,
        ),
        tool(
            "get_guide",
            "Read a guide. Topics: \"papercraft-mcp-instructions\" (read first), \"html\" (what \
             write_html supports), \"whiteboard\" (shapes, arrows, stickies, hand-drawn style).",
            json!({"topic": {"type": "string"}}),
            &["topic"],
            true,
        ),
        tool(
            "export",
            "Export nodes to files. `nodes` maps node ids to export settings \
             ([{format: png|svg|pdf, scale: \"2x\" | \"512w\" | \"512h\" | \"720p\"}]; [] = 2x PNG). \
             Files go to `directory` (default: the Downloads folder). Returns the paths written.",
            json!({
                "nodes": {"type": "object", "additionalProperties": {"type": "array", "items": {
                    "type": "object",
                    "properties": {
                        "format": {"type": "string", "enum": ["png", "svg", "pdf"]},
                        "scale": {"type": "string"},
                    },
                    "required": ["format"],
                }}},
                "directory": {"type": "string", "description": "Output folder (absolute path)."},
                "pageId": page_id(),
            }),
            &["nodes"],
            false,
        ),
        tool(
            "export_combined_pdf",
            "Export several nodes into one PDF, one page per node, ordered top-to-bottom then \
             left-to-right. Returns the file path.",
            json!({
                "nodeIds": ids("Nodes to put on the pages."),
                "path": {"type": "string", "description": "Output file (default: Downloads/<project>.pdf)."},
            }),
            &["nodeIds"],
            false,
        ),
        // ------------------------------------------------------------ writing
        tool(
            "create_artboard",
            "Creates an artboard (top-level frame) and returns its id; then add content with \
             write_html({mode: \"insert-children\"}). `styles` are camelCase CSS; width and height \
             are required (e.g. \"1440px\"). Artboards default to display:flex, \
             flexDirection:column and a white background, and are placed in an empty spot unless \
             left/top are given. Default sizes: desktop 1440×900, tablet 768×1024, mobile 390×844.",
            json!({
                "name": {"type": "string", "description": "Layer name."},
                "styles": {
                    "type": "object",
                    "properties": {"width": {"type": "string"}, "height": {"type": "string"}},
                    "required": ["width", "height"],
                    "additionalProperties": {"anyOf": [{"type": "string"}, {"type": "number"}]},
                    "description": "CSS styles, e.g. {\"width\": \"390px\", \"height\": \"844px\", \"backgroundColor\": \"#f5f5f5\", \"padding\": \"24px\", \"gap\": \"16px\"}",
                },
                "pageId": page_id(),
            }),
            &["name", "styles"],
            false,
        ),
        tool(
            "write_html",
            "Parse HTML with inline styles into layers. Write incrementally: each call should add \
             one visual group (a header, one list row, a card) so the user sees progress.\n\
             - mode \"insert-children\" appends the HTML inside targetNodeId (\"root\" = the page); \
             \"replace\" replaces targetNodeId with it.\n\
             - Inline styles only. Flexbox (display:flex, flex-direction, gap, padding, \
             align-items, justify-content, align-self, flex-grow, flex-wrap) is the layout model; block \
             elements stack vertically. position:absolute with left/top is supported. No margin, \
             grid or tables.\n\
             - Text: font-size, font-weight (≥600 = bold), font-style, color, line-height, \
             letter-spacing, text-align, text-decoration (including wavy and text-decoration-color); \
             <b>/<i>/<u>/<s>/<span style> make rich text. Font family is always Inter.\n\
             - Boxes: background(-color/-image: colors, linear/radial gradients, url()), border \
             (and border-top/right/bottom/left), border-radius, opacity, overflow:hidden, \
             mix-blend-mode, width/height (px, %, fit-content), min-width/max-width/min-height/max-height.\n\
             - <img src> takes paper-asset:///abs/path, file:// URLs, absolute paths or data: URIs. \
             <svg> markup is imported as vector layers.\n\
             - <x-paper-icon name=\"set:name\" size=\"20\" color=\"var(--accent)\"/> places an installed icon (search_icons); \
             <x-paper-instance component=\"Name\" component-id=\"ID\" layer-name=\"...\"/> places a component instance; \
             colors may be var(--name) of set_variables.\n\
             - layer-name=\"...\" names layers; <x-paper-clone node-id=\"ID\" style=\"...\"/> \
             clones an existing node (supports cross-page `<page>:<id>`).\n\
             Returns the ids of the created top-level layers and the styles that were ignored.",
            json!({
                "html": {"type": "string", "description": "HTML to parse into layers."},
                "targetNodeId": {"type": "string", "description": "Target node id, or \"root\" for the page."},
                "mode": {"type": "string", "enum": ["insert-children", "replace"]},
                "pageId": page_id(),
            }),
            &["html", "targetNodeId", "mode"],
            false,
        ),
        tool(
            "create_shape",
            "Create a whiteboard element: rectangle, ellipse, diamond, triangle, polygon, star, \
             line, arrow, sticky (sticky note with text) or text. Coordinates are world units, or \
             relative to parentId when given. Lines and arrows take `points` ([[x,y],...], at \
             least two) and arrows may bind to layers with startNodeId/endNodeId (the arrow then \
             follows them). `styles` takes the same CSS as update_styles plus the hand-drawn keys \
             sketch, roughness (0 architect, 1 artist, 2 cartoonist), fillStyle (hachure, \
             cross-hatch, solid, zigzag, dots), strokeStyle (solid, dashed, dotted). Arrows also \
             take route (straight, curved, elbow) and startHead/endHead (none, arrow, triangle, \
             dot, bar, diamond).",
            json!({
                "type": {"type": "string", "enum": ["rectangle", "ellipse", "diamond", "triangle", "polygon", "star", "line", "arrow", "sticky", "text"]},
                "x": {"type": "number"}, "y": {"type": "number"},
                "width": {"type": "number"}, "height": {"type": "number"},
                "points": {"type": "array", "items": {"type": "array", "items": {"type": "number"}}},
                "text": {"type": "string", "description": "Text for sticky and text."},
                "name": {"type": "string"},
                "parentId": {"type": "string", "description": "Frame to place the shape in."},
                "startNodeId": {"type": "string"}, "endNodeId": {"type": "string"},
                "route": {"type": "string", "enum": ["straight", "curved", "elbow"]},
                "startHead": {"type": "string"}, "endHead": {"type": "string"},
                "sides": {"type": "integer", "description": "Polygon sides / star points."},
                "styles": styles_schema("CSS and hand-drawn styles."),
                "pageId": page_id(),
            }),
            &["type"],
            false,
        ),
        tool(
            "update_styles",
            "Update styles on one or more nodes (camelCase CSS, like React.CSSProperties). Setting \
             left/top on an artboard moves it. Hand-drawn keys: sketch, roughness, fillStyle, \
             strokeStyle, seed. Styles with no effect are returned under ignoredStyles.",
            json!({"updates": {"type": "array", "description": "Each item applies styles to nodeIds.", "items": {
                "type": "object",
                "properties": {
                    "nodeIds": {"type": "array", "items": {"type": "string"}},
                    "styles": styles_schema("CSS properties."),
                },
                "required": ["nodeIds", "styles"],
            }}}),
            &["updates"],
            false,
        ),
        tool(
            "set_text_content",
            "Set the text of one or more Text or Sticky nodes (batch).",
            json!({"updates": {"type": "array", "items": {
                "type": "object",
                "properties": {"nodeId": {"type": "string"}, "textContent": {"type": "string"}},
                "required": ["nodeId", "textContent"],
            }}}),
            &["updates"],
            false,
        ),
        tool(
            "rename_nodes",
            "Rename layers (batch). Names are truncated to 50 characters.",
            json!({"updates": {"type": "array", "items": {
                "type": "object",
                "properties": {"nodeId": {"type": "string"}, "name": {"type": "string"}},
                "required": ["nodeId", "name"],
            }}}),
            &["updates"],
            false,
        ),
        tool(
            "get_feedback",
            "Notes the user left on layers (the Feedback box in the inspector, shown as amber dots \
             on the canvas): which layer, where it sits, what to change. Omitting `pageId` queries \
             all pages across the project; `openCount` and `resolvedCount` are returned alongside \
             per-page counts in `get_basic_info`. Call it at the start of a session and after each \
             round of edits; fix every open note, then resolve_feedback. `get_selection` also \
             includes the selected layers' notes.",
            json!({"nodeId": {"type": "string", "description": "Only this layer."},
                   "pageId": page_id(),
                   "includeResolved": {"type": "boolean"}}),
            &[],
            true,
        ),
        tool(
            "add_feedback",
            "Leave a note on a layer (author `agent` by default), e.g. a question or a problem you \
             could not fix. The user sees it in the inspector and as a dot on the canvas.",
            json!({"nodeId": {"type": "string"}, "text": {"type": "string"},
                   "author": {"type": "string"}, "pageId": page_id()}),
            &["nodeId", "text"],
            false,
        ),
        tool(
            "resolve_feedback",
            "Mark notes on a layer resolved, with a short reply saying what you changed. Without \
             commentId every open note on the layer is resolved. `delete` removes instead.",
            json!({"nodeId": {"type": "string"}, "commentId": {"type": "number"},
                   "reply": {"type": "string"}, "delete": {"type": "boolean"},
                   "pageId": page_id()}),
            &["nodeId"],
            false,
        ),
        tool(
            "lint_design",
            "Check a page or subtree for common design mistakes and get a fix for each: text \
             contrast (WCAG 4.5:1, 3:1 for large text), text under 12px, font sizes off the type \
             scale, gap/padding off the 4px grid, buttons and tabs under 44px tall, default layer \
             names (Frame, Rectangle), and hard-coded colors that equal a variable. Run it before \
             finish_working_on_nodes and fix errors and warnings; `info` items are suggestions. \
             Pass `fix: true` to automatically snap spacing/padding to 4px, relink matching color \
             variables, snap text sizes, and enlarge small touch targets.",
            json!({"nodeId": {"type": "string", "description": "Subtree to check; omit for the whole page."},
                   "scale": {"type": "array", "items": {"type": "number"},
                       "description": "Allowed font sizes. Default 12, 14, 16, 20, 24, 32, 40, 48, 64."},
                   "fix": {"type": "boolean", "description": "Apply safe automatic fixes before reporting remaining issues."},
                   "pageId": page_id()}),
            &[],
            false,
        ),
        tool(
            "get_variables",
            "Color variables of the page's project (name -> hex). Fills that follow a variable \
             were written with `var(--name)`; set_variables moves all of them at once. \
             `format` css returns a `:root { --name: #hex }` block (plus `[data-theme]` rules for modes), \
             json a W3C DTCG design-tokens object, and tailwind a Tailwind v4 `@theme` block.",
            json!({"pageId": page_id(),
                   "format": {"type": "string", "enum": ["css", "json", "tailwind"]}}),
            &[],
            true,
        ),
        tool(
            "set_variables",
            "Define or change color variables (names like `surface`, `accent-text`), on every \
             page. Use them as `var(--name)` in write_html / update_styles / create_artboard \
             colors (backgroundColor, background, color) instead of hex literals, so a theme \
             change is one call. Fills already following a changed variable are updated. \
             Variables apply to fills and text color; strokes and gradients keep fixed colors.",
            json!({"variables": {"type": "object", "propertyNames": {"type": "string"},
                "additionalProperties": {"type": "string"},
                "description": "{\"accent\": \"#cba6f7\", \"surface\": \"#313244\"}"},
                   "mode": {"type": "string", "description": "Store the values for this mode (e.g. \"Light\", \"Dark\"); they apply when it is the active mode. The first mode you define becomes active."}}),
            &["variables"],
            false,
        ),
        tool(
            "set_tokens",
            "Define non-color design tokens (spacing, radius, font size, shadows, widths), on every \
             page: {\"spacing-md\": \"16px\", \"radius-md\": \"8px\", \"shadow-md\": \"0 4px 12px \
             rgba(0,0,0,0.08)\"}. Use `var(--spacing-md)` in any style value (padding: \
             var(--spacing-sm) var(--spacing-md), boxShadow: var(--shadow-md)); it is replaced by the \
             value when written, so tokens are a shared vocabulary, not live links. Listed by \
             get_variables.",
            json!({"tokens": {"type": "object", "propertyNames": {"type": "string"},
                "additionalProperties": {"anyOf": [{"type": "string"}, {"type": "number"}]}}}),
            &["tokens"],
            false,
        ),
        tool(
            "set_variable_mode",
            "Switch the active variable mode (defined with set_variables {mode}): every variable \
             takes that mode's value and all fills following them change, on every page. One call \
             turns a whole design from light to dark.",
            json!({"mode": {"type": "string"}}),
            &["mode"],
            false,
        ),
        tool(
            "import_design_system",
            "Import an OpenDesign design-system package: a folder with manifest.json, tokens.css \
             and (optional) DESIGN.md. The colors of the `:root` block become variables, the \
             colors of the `[data-theme=\"name\"]` blocks become modes and every other value \
             becomes a token. Replaces the variables, modes and tokens of every page in one undo \
             step. Only hex colors become variables; oklch(), rgb() and sizes stay tokens.",
            json!({"path": {"type": "string", "description": "Folder of the package to import."},
                   "pageId": page_id()}),
            &["path"],
            false,
        ),
        tool(
            "export_design_system",
            "Write the page's variables, modes and tokens as an OpenDesign design-system package \
             (manifest.json, tokens.css and DESIGN.md) in `path`, so it can be read back with \
             import_design_system or by other tools. Returns the files written.",
            json!({"path": {"type": "string", "description": "Folder to write the package into."},
                   "pageId": page_id()}),
            &["path"],
            false,
        ),
        tool(
            "get_components",
            "Main components on a page (id, name, size) with how many instances each has.",
            json!({"pageId": page_id()}),
            &[],
            true,
        ),
        tool(
            "import_components",
            "Copy the main components anywhere on `fromPageId` (including inside artboards, \
             typically on a `Design System` page) onto this one, so you can place instances here \
             with create_instance or <x-paper-instance component=\"Name\"/> / component-id=\"ID\". \
             Components with the same name already here are skipped. Variables and tokens are \
             already shared by every page; components are copies, not live links.",
            json!({"fromPageId": {"type": "string"}, "pageId": page_id(),
                   "names": {"type": "array", "items": {"type": "string"},
                       "description": "Only these components (default all)."}}),
            &["fromPageId"],
            false,
        ),
        tool(
            "create_component",
            "Turn a frame or artboard into a main component. Build reusable parts (button, row, \
             chip, tab) once as components, then place them with create_instance or \
             <x-paper-instance>. Components and instances live on one page.",
            json!({"nodeId": {"type": "string", "description": "The frame to convert."},
                   "name": {"type": "string", "description": "Optional new layer name."},
                   "pageId": page_id()}),
            &["nodeId"],
            false,
        ),
        tool(
            "create_instance",
            "Place an instance of a component. Edit text and styles on the instance's descendants \
             with set_text_content / update_styles (they become overrides); edits to the main \
             component flow to every instance. Returns the instance id and its text layers.",
            json!({"componentId": {"type": "string"},
                   "parentId": {"type": "string", "description": "Parent frame; omit or \"root\" for the page."},
                   "index": {"type": "number", "description": "Position among the parent's children."},
                   "x": {"type": "number"}, "y": {"type": "number"},
                   "name": {"type": "string"},
                   "pageId": page_id()}),
            &["componentId"],
            false,
        ),
        tool(
            "swap_instance",
            "Swap an instance root to point at a different main component on the same page, \
             preserving overrides on descendants matched by name and layer type.",
            json!({"nodeId": node_id("The instance root node to swap."),
                   "componentId": node_id("The new main component id."),
                   "pageId": page_id()}),
            &["nodeId", "componentId"],
            false,
        ),
        tool(
            "detach_instance",
            "Detach one or more component instances into plain frames, breaking their link to \
             the main component while keeping their current appearance and children.",
            json!({"nodeIds": ids("Instance root nodes to detach."),
                   "pageId": page_id()}),
            &["nodeIds"],
            false,
        ),
        tool(
            "search_icons",
            "Search installed open icon sets (Lucide, Tabler, Phosphor, Heroicons, Material). \
             Returns `set:name` references for <x-paper-icon name=\"lucide:check\" size=\"20\" \
             color=\"var(--text)\"/> in write_html. Without a query it lists installed and \
             installable sets; install with `papercraft-cli icons add lucide`.",
            json!({"query": {"type": "string"}, "set": {"type": "string"},
                   "limit": {"type": "number", "description": "Default 20."}}),
            &[],
            true,
        ),
        tool(
            "delete_nodes",
            "Delete nodes and their descendants. Verify parents with get_node_info first when unsure.",
            json!({"nodeIds": ids("Nodes to delete.")}),
            &["nodeIds"],
            false,
        ),
        tool(
            "duplicate_nodes",
            "Deep-clone nodes, including across pages (via `toPageId` or a cross-page `parentId`; \
             `<x-paper-clone node-id=\"<page>:<id>\"/>` also works across pages). Each copy goes \
             under parentId when given, else next to its source; duplicated artboards are placed \
             in an empty spot. Returns new ids and a descendantIdMap from every original \
             descendant id to its copy.",
            json!({"nodes": {"type": "array", "items": {
                "type": "object",
                "properties": {
                    "id": {"type": "string"},
                    "parentId": {"type": "string"},
                    "toPageId": {"type": "string", "description": "Optional destination page id when copying/moving to another page."},
                },
                "required": ["id"],
            }}}),
            &["nodes"],
            false,
        ),
        tool(
            "move_nodes",
            "Move nodes keeping their ids (or onto another page via `toPageId` or a cross-page \
             `parentId` / `before` / `after`). Each move is {nodeId, before: siblingId}, {nodeId, \
             after: siblingId} or {nodeId, parentId, toPageId?, index?} (parentId \"root\" = the \
             page root). Moves apply in order. In auto-layout parents this changes the visual \
             order; in free parents it changes stacking and keeps the world position.",
            json!({"moves": {"type": "array", "minItems": 1, "items": {
                "type": "object",
                "properties": {
                    "nodeId": {"type": "string"}, "before": {"type": "string"},
                    "after": {"type": "string"}, "parentId": {"type": "string"},
                    "toPageId": {"type": "string", "description": "Optional destination page id when copying/moving to another page."},
                    "index": {"type": "integer", "minimum": 0},
                },
                "required": ["nodeId"],
            }}}),
            &["moves"],
            false,
        ),
        tool(
            "select_nodes",
            "Select nodes on the page the user is viewing so they can see what you mean; \
             zoom: true also frames them in the viewport. An empty list clears the selection.",
            json!({"nodeIds": ids("Nodes to select."), "zoom": {"type": "boolean"}}),
            &["nodeIds"],
            false,
        ),
        tool(
            "undo",
            "Undo the last change on a page (the user's or an agent's).",
            json!({"pageId": page_id()}),
            &[],
            false,
        ),
        tool(
            "redo",
            "Redo the last undone change on a page.",
            json!({"pageId": page_id()}),
            &[],
            false,
        ),
        tool(
            "diff_frames",
            "Compare two frames or subtrees (`beforeId` and `afterId`) and report property-level \
             differences (size, position, fills, stroke, corner radius, opacity, text, layout, \
             added/removed children).",
            json!({
                "beforeId": node_id("First frame or node id (baseline)."),
                "afterId": node_id("Second frame or node id (comparison)."),
                "pageId": page_id(),
            }),
            &["beforeId", "afterId"],
            true,
        ),
        tool(
            "finish_working_on_nodes",
            "Call when done creating or editing. Saves the project.",
            json!({"nodeIds": ids("Optional nodes you worked on.")}),
            &[],
            false,
        ),
    ]
}

pub const GUIDE_INSTRUCTIONS: &str = "\
# Designing in papercraft over MCP

1. Orient: get_basic_info → pages, artboards, sizes; get_selection → what the user points at.
2. Plan a brief before a new design: palette (3–5 colors), type scale (e.g. 12/14/16/20/28/40),
   spacing scale (4/8/12/16/24/32), corner radius.
3. Create an artboard sized for the device (create_artboard). Artboards are flex columns.
4. Fill it with write_html, one visual group per call, into the artboard id. Keep the returned
   ids: later calls can target them (insert-children) or replace them.
5. Iterate with update_styles / set_text_content / duplicate_nodes instead of rewriting.
6. Review with get_screenshot (scale 1; 2 for small text). Fix clipping with
   update_styles({height: \"fit-content\"}) on the artboard.
7. finish_working_on_nodes at the end (also saves the project).

Layout model: every container is an auto-layout frame (flexbox). display:flex with
flex-direction row/column, gap, padding, justify-content (start, center, end, space-between),
align-items (start, center, end; stretch is the default and makes children fill the cross axis
when the container has a definite size). flex-grow:1 fills the main axis; width:100% fills too.
height/width fit-content hug the content. position:absolute + left/top places freely.

Whiteboard: create_shape makes rectangles, ellipses, diamonds, stickies, text and arrows that
bind to shapes (they re-route when shapes move). Turn on the hand-drawn look with
{sketch: true, roughness: 1, fillStyle: \"hachure\"}.

Node ids look like \"3fa9c1:14\" (page prefix and node number). Ids stay valid across calls.";

pub const GUIDE_HTML: &str = "\
# write_html reference

Elements: div/section/header/footer/nav/main/article/aside/ul/ol/li/button/form → frames;
p/h1–h6/span/label/a/strong/em/code/pre (elements holding only text) → text layers (with a
frame around them when they have a background, border or padding); img → image rectangle;
svg → vector layers; br → line break;
<x-paper-instance component=\"Name\" component-id=\"ID\" overrides='{\"Label\":\"Save\"}'/> → component instance;
<x-paper-clone node-id=\"ID\"/> → copy of an existing layer (supports cross-page `<page>:<id>`).

Default sizes (like CSS): h1 32px bold, h2 24px bold, h3 20px bold, h4–h6 16px bold, p 16px,
small 13px, code/pre 14px; color is inherited, default black.

Styles: width, height (px, %, fit-content), min-width, max-width, min-height, max-height,
left/top/right/bottom with position:absolute, display (flex, block, none), flex-direction,
flex-wrap, gap, row-gap, column-gap, padding*, justify-content, align-items,
align-self (start, center, end, stretch), flex-grow, overflow, background, background-color,
background-image (url(), linear-gradient(), radial-gradient()), background-size/object-fit,
border, border-top/border-right/border-bottom/border-left, border-color/width/style,
border-radius (1–4 values), opacity, mix-blend-mode, color, font-size, font-weight, font-style,
line-height, letter-spacing, text-align, text-decoration (including `text-decoration-color` and
`text-decoration-style: wavy`, e.g. `text-decoration: underline wavy var(--warning)`).
Ignored (reported back): margin, grid, box-shadow, transform, filters, transitions.";

pub const GUIDE_WHITEBOARD: &str = "\
# Whiteboard elements

create_shape({type, x, y, width, height, styles}) for rectangle, ellipse, diamond, triangle,
polygon (sides), star (sides = points), sticky (text, background color), text (text).
Lines and arrows: create_shape({type: \"arrow\", points: [[x1,y1],[x2,y2]],
startNodeId, endNodeId, route: \"elbow\", endHead: \"triangle\"}). With start/end nodes the
points can be omitted: the arrow is routed between the shapes and follows them when they move.

Hand-drawn style (Excalidraw-like): styles {sketch: true, roughness: 0|1|2,
fillStyle: hachure|cross-hatch|solid|zigzag|dots, strokeStyle: solid|dashed|dotted, seed}.
Colors: backgroundColor (fill), borderColor/stroke, borderWidth/strokeWidth.";

/// Whether a tool call edits a page document, so it counts towards an agent run.
pub fn edits_doc(name: &str, args: &Value) -> bool {
    classify(name, args).unwrap_or(false)
}

/// `None` for a tool with no class, so a tool added without one fails the test below.
fn classify(name: &str, args: &Value) -> Option<bool> {
    Some(match name {
        "list_resources" | "get_basic_info" | "get_selection" | "get_node_info" | "get_children"
        | "get_tree_summary" | "get_screenshot" | "get_jsx" | "get_design_spec" | "verify_code"
        | "get_computed_styles" | "get_fill_image" | "find_nodes" | "get_font_family_info"
        | "get_guide" | "get_feedback" | "get_variables" | "get_components" | "search_icons"
        | "diff_frames" => false,
        "select_nodes" | "export" | "export_combined_pdf" | "export_design_system" | "open_file"
        | "create_file" | "finish_working_on_nodes" => false,
        "lint_design" => args["fix"] == true,
        "create_page" | "rename_pages" | "delete_pages" | "create_artboard" | "write_html"
        | "create_shape" | "update_styles" | "set_text_content" | "rename_nodes"
        | "add_feedback" | "resolve_feedback" | "set_variables" | "set_tokens"
        | "set_variable_mode" | "import_design_system" | "import_components" | "create_component"
        | "create_instance" | "swap_instance" | "detach_instance" | "delete_nodes"
        | "duplicate_nodes" | "move_nodes" | "undo" | "redo" => true,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tool_names() -> Vec<String> {
        definitions()
            .iter()
            .map(|def| def["name"].as_str().unwrap_or_default().to_string())
            .collect()
    }

    #[test]
    fn every_tool_has_an_explicit_class() {
        for name in tool_names() {
            assert!(classify(&name, &json!({})).is_some(), "unclassified tool: {name}");
        }
    }

    #[test]
    fn read_only_tools_never_edit() {
        for def in definitions() {
            if def["annotations"]["readOnlyHint"] != true {
                continue;
            }
            let name = def["name"].as_str().unwrap_or_default();
            assert!(!edits_doc(name, &json!({})), "read-only tool edits: {name}");
        }
    }

    #[test]
    fn lint_design_edits_only_with_fix() {
        assert!(!edits_doc("lint_design", &json!({})));
        assert!(!edits_doc("lint_design", &json!({"fix": false})));
        assert!(edits_doc("lint_design", &json!({"fix": true})));
    }

    #[test]
    fn unknown_tool_does_not_edit() {
        assert!(!edits_doc("no_such_tool", &json!({})));
    }
}
