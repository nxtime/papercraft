# What `write_html` supports and ignores

`write_html` parses an HTML fragment with inline `style` attributes into layers. It is a forgiving parser (unquoted attributes, void tags, entities such as `&middot;` and `&rarr;`, mismatched closing tags), not a browser: there is no cascade, no `<style>`, no classes, no scripts. Write each element's styles inline. Anything it cannot map comes back in the result as `ignoredStyles`: read that list after every call.

## Call shape

```json
{"targetNodeId":"<artboard or frame id, or \"root\">","mode":"insert-children","html":"<div ...>...</div>"}
```

`insert-children` appends inside the target. `replace` swaps the target for the new HTML. The result lists `createdNodeIds` (top-level layers created), so write one visual group per call and keep the ids.

## Elements

| Markup | Becomes |
|---|---|
| `<div>`, `<section>`, `<p>`... with child elements | a frame (auto layout when `display:flex`; block elements stack vertically) |
| an element containing only text | a text layer |
| `<b>`, `<i>`, `<u>`, `<s>`, `<span style="...">` inside text | rich-text runs (bold, italic, underline, strike, size, color) |
| `<img src>` | an image fill. `src` is an absolute path, a `file://` URL, `paper-asset:///abs/path` or a `data:` URI. `http(s)` is not fetched |
| `<svg>...</svg>` | vector layers (paths, gradients, strokes) |
| `<x-paper-icon name="set:name" size="20" color="var(--x)"/>` | an icon from an installed set (`phosphor:gear`) |
| `<x-paper-instance component="Name" component-id="ID" overrides='{"Label":"Save"}'/>` | an instance of a component (lookup by `component="Name"` or `component-id="ID"`) |
| `<x-paper-clone node-id="ID" style="..."/>` | a deep copy of an existing node (supports cross-page `<page>:<id>`) |
| `layer-name="..."` on any of the above | the layer's name (names are truncated to 50 characters) |

## Layout

Supported: `display:flex` (and `inline-flex`), `flex-direction`, `flex-wrap`, `gap`, `row-gap`, `column-gap`, `padding` (shorthand, per side, `padding-inline`, `padding-block`), `align-items`, `justify-content` (including `space-between`), `align-self` (`start`, `center`, `end`, `stretch`), `flex-grow` / `flex: 1`, `width`, `height` (px, %, `fit-content`), `min-width`, `max-width`, `min-height`, `max-height`, `overflow:hidden` (clips), `position:absolute` with `left`/`top`/`right`/`bottom`, `display:none`, `visibility`.

Rewritten before layers are made, each reported in `warnings`:
- `margin` between siblings becomes the parent's `gap` when every gap is the same, else spacer layers; the margin before the first and after the last child becomes the parent's `padding` (spacers when its padding is not plain px). `margin-*:auto` is a growing spacer and `margin:0 auto` centres the item across the flow. Margins across the flow, on absolute boxes and on `auto` sides become a padded wrapper frame (named `<layer> (margin)`) or an offset. Negative and percentage margins are dropped with a warning; parent and first-child margin collapse is not modelled.
- `display:grid` becomes rows of cells: `repeat(N, 1fr)`, px, `fr`, `minmax(px, 1fr)` and `repeat(auto-fill, minmax())` (columns counted from the width) tracks; `gap`, `row-gap`, `column-gap`; `grid-column: span N` (also `1 / 3`) makes a cell cover N columns. `grid-template-rows`, areas, `grid-row` and explicit placement are not supported: cells flow row by row, with a warning.
- `transform: translate()`, `translateX()` and `translateY()` move a free-positioned layer by that offset (px). Inside auto layout the offset cannot stay, so use margin or gap. `rotate(<n>deg|rad|turn|grad)` turns the layer clockwise around its centre (visual only: layout, `rect` and constraints stay unrotated; several `rotate()` add up; `none` resets it). Computed styles and exports write it back as `transform: rotate(<n>deg)`, Tailwind `rotate-[<n>deg]` and SVG/PDF rotation. `scale`, `skew` and `matrix` are reported in `ignoredStyles`.
- Inline boxes (chips, badges) become boxes in a wrapping row; `<input>` and `<textarea>` become a box showing their placeholder or value.

`ignoredStyles` lists what was dropped, one entry per property (`transform: rotate, scale`).

Prefer `gap` and `padding` anyway: the result is the same and shorter. To open a whole exported page (a `.html` or a `.zip` with its stylesheets and images), use `import_html`: it inlines `<style>` and linked CSS (tag, `.class` and `#id` selectors, `:root` custom properties; media queries and combinators are reported), registers the zip's images, and writes each HTML file into its own new frame on the page.

Not supported (check `ignoredStyles` to be sure): tables, `float`, `calc()`, viewport units, rotation.

Tips:

* Artboards are `display:flex; flex-direction:column`. Children of a column stretch to the full width unless they have a width or `align-items` / `align-self` says otherwise.
* To center something, put it in a flex parent with `align-items:center; justify-content:center`.
* To make a row fill the remaining space, give the child `flex-grow:1`.
* A screen keeps its device's size; at a device width (390, 1440...) `height:"fit-content"` becomes the device height. Content taller than the screen scrolls by itself: the main column under a fixed sidebar and header, else the screen (write_html says which). Pass `overflow:"visible"` to let a board grow (a landing page drawn in full).

## Text

Supported: `font-size` (px), `font-weight` (600 and above renders bold; otherwise regular), `font-style:italic`, `color`, `line-height` (px or unitless), `letter-spacing`, `text-align`, `text-decoration` (including `text-decoration-color` and `text-decoration-style: wavy`, e.g. `text-decoration: underline wavy var(--warning)`). The font is **Inter** unless `font-family` names a family added with `install_font` (it applies to the whole text layer, not a span). There are only regular and bold weights (plus italic), so hierarchy comes from size, weight, color and space.

## Boxes

Supported: `background` and `background-color` (hex, `rgb()`, `hsl()`, `var(--name)`), gradients (`linear-gradient`, `radial-gradient`), `background-image: url()`, `background-size` (`contain`, `100% 100%`), `border` and per-side `border-top`, `border-right`, `border-bottom`, `border-left` (width, style, color), `border-radius` (shorthand and per corner; `999px` makes a pill), `opacity`, `mix-blend-mode`. Strokes and gradients keep their resolved colors; they do not follow variables later.

Not supported (reported as ignored): `box-shadow`, `filter`, `backdrop-filter`, `text-shadow`, `outline-offset`, CSS animations and transitions.

## Colors and variables

Write colors as `var(--name)` for `background`, `background-color` and `color` and the layer stays linked to the variable. Define the variable with `set_variables` first. Hex is fine for one-off colors in an image or an illustration, but `lint_design` will flag a hex that equals a variable.

## `update_styles`

Takes the same properties as camelCase JSON (`{"backgroundColor":"var(--accent)","paddingTop":16}`; numbers become px for lengths). Extra keys for the whiteboard: `sketch`, `roughness`, `fillStyle`, `strokeStyle`, `seed`. Setting `left` / `top` on an artboard moves it.
