# OpenDesign integration

How much of [OpenDesign](https://github.com/nexu-io/open-design) (Apache-2.0) belongs in
PaperCraft, what has landed, and what is still on the table.

OpenDesign is a Next.js + Express/SQLite "collaborative AI design agent workspace": it spawns
your local coding agent CLI, binds a brand design system to it, and renders the artifacts it
writes. Its runtime is Electron/Next.js-bound and does not port. Its **file formats** do, and
PaperCraft already owns the surface most of them target — variables, tokens, modes, components,
codegen, MCP tools.

## OpenDesign feature inventory

| Subsystem | What it is | Portable? |
|---|---|---|
| `SKILL.md` protocol | Agent Skills convention; `od:` frontmatter adds mode/scenario/design_system refs | Yes — markdown |
| `open-design.json` | Plugin manifest: kind, taskKind, mode, context, pipeline, genui, inputs, capabilities | Yes — JSON |
| Plugin marketplace | Federated `open-design-marketplace.json` catalog; trust tiers + capability gating | Yes — JSON |
| Design-system packages | 151 packages: `manifest.json` + `DESIGN.md` + `tokens.css` (+ components fixtures) | **Yes — imported here** |
| `craft/*.md` | 13 brand-agnostic rulebooks injected into the prompt | Yes — markdown |
| Critique Theater | 5-panelist pre-emit gate: critic .4 / brand .2 / a11y .2 / copy .2, threshold 8.0, ≤3 rounds, `ship_best` | **Yes — landed as a skill** |
| Direction picker | 5 curated OKLch directions (editorial, minimal, human, tech-utility, brutalist) | Yes — data |
| `<question-form>` | Structured clarification JSON the assistant emits before generating | Yes — JSON |
| Tweaks panel | 5-knob contract (`--accent/--scale/--density/--mode/--motion`) | Yes — CSS |
| Atoms + pipeline DSL | `stages[{id,atoms,repeat,until,onFailure}]`, closed `until` vocabulary, ≤10 iterations | Yes — DSL |
| GenUI | 4 surface kinds (form/choice/confirmation/oauth-prompt) + persist tiers | Yes — JSON Schema |
| Claude Design ZIP import | Import an artifact bundle from Anthropic's Claude Design | Yes — the ZIP shape |
| BYOK proxy, runtime registry, SQLite schema, Electron sidecar, Next.js UI, HyperFrames MP4 | The product itself | **No — do not port** |

## Landed

### 1. Design-system package import/export (code)

`src/design_system.rs` reads and writes the OpenDesign package format:
`manifest.json` (`od-design-system-project/v1`) + `tokens.css` + `DESIGN.md`.

* `:root { --accent: #cba6f7 }` → a PaperCraft **variable**
* `[data-theme="Dark"] { … }` → a PaperCraft **mode**
* everything else (`--space-4: 16px`, `--font-body`, `color-mix(…)`) → a PaperCraft **token**

MCP tools: `import_design_system` (project-wide, one undo step) and `export_design_system`
(writes the three files). Format doc: `docs/design-system-packages.md`.

Verified against the real bundled OpenDesign package (`design-systems/agentic`): 12 variables,
37 tokens, and an import → export → re-import round-trip that is byte-stable.

### 2. Critique gate (agent skill)

`skills/papercraft-critique/` ports the 5-panel gate contract into PaperCraft's own tools:
`lint_design` and `verify_code` findings are hard evidence that veto a panel score, the
weighted composite must reach 8.0, three rounds maximum, `ship_best` fallback, and the status
vocabulary (`Shipped` / `Below threshold` / `Timed out` / `Interrupted` / `Degraded`).

## Not ported, and why

* **Plugin/marketplace runtime** — PaperCraft has no plugin host at all (ROADMAP #19). The
  format can be adopted later; the runtime is a project of its own.
* **Tokens as a typed system** — OpenDesign's `tokens.css` has four layers and a contract
  report. PaperCraft tokens stay untyped strings; the layer semantics would need a persisted
  format change (`PAGE_VERSION`).
* **OKLch colours** — the PaperCraft colour model is hex only. Non-hex values are tokens.

## Next candidates, ranked

1. **Direction picker as built-in presets** — the 5 OKLch directions become starter design
   systems. Data-only, no model change; needs a colour-space decision (OKLch → hex).
2. **Craft rulebooks as a prompt layer** — 13 markdown rulebooks the design-to-code path reads
   before generating code. Markdown only.
3. **`<question-form>` discovery** — structured clarification before codegen; the agent renders
   it, PaperCraft just needs to pass it through.
4. **Pixel diff wiring** — `src/mcp/diff.rs::diff_rgba` is implemented but unreachable. Not an
   OpenDesign feature, but OpenDesign's `visual-diff` evaluator wants the same thing.
5. **Plugin manifest (`open-design.json`) reader** — accepts a third-party plugin folder and
   maps its `context` onto PaperCraft skills. Only worth it with a plugin host.

## Attribution

OpenDesign is Apache-2.0. No upstream file is copied into this repo: the package format and
the critique contract are reimplemented from their published descriptions. Recorded in
`THIRD_PARTY.md`.
