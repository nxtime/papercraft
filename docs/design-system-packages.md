# Design-system packages

PaperCraft can read and write **OpenDesign design-system packages**: a folder that holds a
`manifest.json`, a `tokens.css` and (usually) a `DESIGN.md`. They are the exchange format for
a design system — one design system in, the same design system out.

```text
my-system/
    manifest.json     what the package is
    tokens.css        the colors, modes and tokens
    DESIGN.md         the prose about the brand
```

The two tools are `import_design_system` and `export_design_system`.

```sh
papercraft-cli call import_design_system '{"path":"/path/to/my-system"}'
papercraft-cli call export_design_system '{"path":"/path/to/my-system"}'
```

## The format

### `manifest.json`

```json
{
  "schemaVersion": "od-design-system-project/v1",
  "id": "my-system",
  "name": "My System",
  "category": "product",
  "description": "A small system",
  "source": { "type": "github", "url": "https://example.com/system" },
  "files": { "design": "DESIGN.md", "tokens": "tokens.css" },
  "importMode": "hybrid"
}
```

| Key | Meaning |
|---|---|
| `schemaVersion` | `od-design-system-project/v1`. Another value is refused. |
| `id` | The slug of the folder: lowercase letters, digits and dashes. |
| `name`, `category`, `description` | What the system is. |
| `source` | Where it came from: `bundled`, `local`, `github` or `shadcn`. |
| `files` | The files of the package. `design` and `tokens` are required; `designTokens`, `tailwind` and `components` are optional. |
| `assetsDir`, `previewDir`, `usage`, `componentsManifest`, `importMode`, `craft`, `fonts`, `preview`, `sourceFiles` | Optional extras; they are kept as they are. |

`importMode` is `normalized`, `hybrid` or `verbatim`.

### `tokens.css`

Plain CSS custom properties. The `:root` block holds the base values, every
`[data-theme="name"]` block the values of one mode:

```css
:root {
  --bg: #ffffff;
  --accent: #cba6f7;
  --spacing-md: 16px;
}

[data-theme="Dark"] {
  --bg: #1e1e2e;
  --accent: #89b4fa;
}
```

### `DESIGN.md`

Prose about the brand. PaperCraft reads it (the import reports that a package has one) and
writes a short summary of the variables, modes and tokens on export.

## What PaperCraft does with it

| Package | PaperCraft |
|---|---|
| a hex color in `:root` | a **variable** (`--bg`), the thing a fill follows |
| any other value in `:root` | a **token** (`--spacing-md`), substituted into styles when they are written |
| a hex color in `[data-theme="Dark"]` | the **mode** `Dark` |

The rules, in full:

* **Colors are hex only.** PaperCraft colors are `#rrggbb` / `#rrggbbaa`. A value PaperCraft
  cannot hold — an `oklch()`, an `rgb()`, a named color — is kept as a token with the value
  as it is, so nothing is lost.
* **The import replaces** the variables, the modes and the tokens of the project, on every
  page, in one undo step (like `set_variables`). Undo brings the old ones back.
* **The base theme stays live.** After an import no mode is active, so the `:root` values
  are the ones on the canvas. Pick another mode with `set_variable_mode`.
* **Names are made CSS-safe**: letters, digits, `-` and `_`. Everything else becomes `-`, so
  `--Brand Color` is imported as `--Brand-Color`.
* **A mode holds colors only.** A size inside a `[data-theme]` block cannot be stored per
  mode, so it becomes a plain token.
* **A package is identified by its folder.** The `id` in the manifest must be the slug of the
  folder it lives in (`my-system`). The export writes the slug of the target folder as the
  id.

## The tools

### `import_design_system`

```sh
papercraft-cli call import_design_system '{"path":"/path/to/my-system"}'
```

| Argument | Type | Notes |
|---|---|---|
| `path` | string | The folder of the package. Required. |
| `pageId` | string | Optional. The design system is project-wide, so it lands on every page. |

It answers with what came in:

```json
{
  "package": { "id": "my-system", "name": "My System", "category": "product" },
  "variables": 2, "modes": 1, "tokens": 1,
  "pages": 3, "fillsUpdated": 4, "design": true
}
```

A malformed package is an error, never a crash: a broken `manifest.json`, a missing
`tokens.css`, an id that is not a slug or a `schemaVersion` PaperCraft does not know all come
back as a message.

### `export_design_system`

```sh
papercraft-cli call export_design_system '{"path":"/path/to/my-system"}'
```

| Argument | Type | Notes |
|---|---|---|
| `path` | string | The folder to write the package into. Created if it is not there. Required. |
| `pageId` | string | Optional. The page whose design system is written. |

It writes `manifest.json`, `tokens.css` and `DESIGN.md` and answers with their paths. The
`tokens.css` is the same shape the import reads, so a package survives the round trip.

## Attribution

The package format is the one published by
[OpenDesign](https://github.com/nexu-io/open-design) (Apache-2.0). PaperCraft reimplements
the format from its description; no file of that project is copied. See
[THIRD_PARTY.md](../THIRD_PARTY.md).
