# Project rules

The standards a change to PaperCraft is reviewed against. [CONTRIBUTING.md](CONTRIBUTING.md) says
*how* to contribute; this file says what "good" means once you do.

## Code

* **Immutability by default.** Return new values rather than mutating shared ones. The document
  changes only through `Editor::edit` transactions, so every change is one undo step and is
  logged for the renderer's cache.
* **Small, cohesive files.** 200 to 400 lines is typical, 800 is the ceiling. Split a module
  when it grows past that. Organise by feature, not by type.
* **Small functions** (under about 50 lines) and **shallow nesting** (4 levels at most); prefer
  early returns.
* **Handle errors explicitly.** No `unwrap` or `expect` on user data, file contents or network
  responses; return an error with context. User-facing messages are plain language. Never swallow
  an error silently.
* **Validate at the boundaries**: MCP and CLI arguments, imported files (Figma, Excalidraw, SVG),
  project folders and anything from the relay. Treat all of it as untrusted.
* **No magic numbers.** Name thresholds, sizes and delays as constants; UI measures come from
  the theme, not literals.
* **No dead code, debug prints or commented-out blocks.**
* **Match the surrounding code**: naming, comment density and idiom. Comments say *why*, not
  *what*.
* **Cross-platform.** Code must build on Windows, macOS and Linux and for the web build. Gate
  platform code with `cfg`, never assume a path separator, and keep
  `cargo check --lib --target wasm32-unknown-unknown` passing.
* **No new heavy dependencies** without discussion. Licenses must be compatible with
  MIT OR Apache-2.0.

## Data safety

PaperCraft stores people's work. Treat that as the top priority.

* Writes to projects are **atomic** (`src/io/atomic.rs`); never write a page in place.
* A format change bumps the project format version and ships a **migration**. Older data must
  keep loading; if a newer build writes something an older build cannot read, the older build
  must refuse it rather than corrupt it.
* Agent and sync paths must never silently drop a layer. If a merge cannot keep something, it
  says so.
* Destructive operations are undoable or confirmed.

## The machine you build on

A full build links dozens of test binaries and can take every core and most of the memory of a
desktop. The person is still using that machine, often with PaperCraft open.

* **Look before you start a heavy job.** Check free memory, CPU load and what is already running
  (the person's app, another build, other agents) before a build, a test run or clippy. If memory
  is short or another build is running, wait or ask.
* **Leave room.** Cap Cargo at no more than half the logical cores (`-j 8` on a 16-core machine,
  or `CARGO_BUILD_JOBS`) and the test harness too (`-- --test-threads=8`); go lower when memory is
  tight, since linking is the peak. Run heavy jobs at low priority (`nice` on Unix, Below Normal
  on Windows).
* **One heavy job at a time.** Never run two builds side by side; each target folder is a full
  build's worth of compiling and linking. Parallel agents share one build slot.
* **Run only what your change touches**: `cargo check` before `cargo build`, and the test
  targets that cover the change (`cargo test --test incremental`). The full suite runs on GitHub
  before a release (`.github/workflows/test.yml`), not on a desktop.
* **Stop at once when the person says the machine is slow**, then check that no compiler, linker
  or test process of yours is left over.
* **Clean up.** Delete scratch target folders and worktrees when the work is merged; they grow to
  tens of gigabytes.

## UI

* **Use the shared kit.** Controls, spacing, radii and colours come from `craft-ui`,
  `src/ui/kit.rs` and the theme, not one-off values.
* **Themes are modes.** Never branch on a theme to pick a colour; read it from the palette.
* **Nothing overflows.** Text truncates with an ellipsis, rows keep their controls inside, and
  panels step aside when the window is narrow. Test a narrow window and a long name.
* **Targets are at least 44 px** where a person taps or clicks something primary, and everything
  stays reachable from the keyboard.
* **Animation is quick**: eased and under about 200 ms.
* **Look at it.** Run the app or take a headless screenshot at scale 2 in each theme before
  asking for review.

## MCP tools and the CLI

* Every tool has a precise one-paragraph description, a JSON schema, and a test in
  `tests/mcp.rs`. Descriptions are what agents read; write them for that reader.
* One tool call is **one undo step**.
* Names follow the existing style (`verb_noun`); reuse Paper-compatible names and argument shapes
  where a tool has an equivalent.
* Reads are cheap and bounded. Apply limits, say when you truncated, and never dump the whole
  document by default.
* The server listens on `127.0.0.1` only and rejects browser origins. Do not weaken that.
* Update the tool count in the README and ROADMAP when you add or remove tools.

## Designs made with PaperCraft

These apply to example projects, templates, screenshots and skill content, and are what the
skills teach agents.

* **Themes are modes, never copies.** One artboard per screen. Give each variable a value per
  mode and switch with `set_variable_mode`. Do not duplicate artboards as light and dark
  versions, and do not rebuild a screen with other hex values.
* **Every colour on a themed screen is a variable** (`var(--name)` on fills and text,
  `bind_variable` for strokes, opacity and visibility).
* **No overflow.** Nothing extends past its artboard or clipping parent. Scrolling content is
  clipped inside its viewport and ends above floating bars with a fade and enough bottom padding.
  Check each artboard programmatically *and* by screenshot in each mode.
* Run `lint_design` and fix what it finds: contrast, the 4 px grid, type scale and 44 px targets.
* Release design locks when you are done so the next person can inspect.

## Skills and docs

* Skills in [skills/](skills) are instructions for agents: keep them short, concrete and tested
  against a real agent run.
* Docs describe what exists. Do not document a feature that is not built; put plans in `specs/`
  and `ROADMAP.md`.
* Screenshots come from the real app, in a generic project (no personal data, local paths or
  real names), at scale 2.

## Branding

The tree carries no third-party product branding; `.debrand` lists the forbidden patterns and
the check runs in review. Describe PaperCraft on its own terms.

## Review

A pull request is approved when it has no unresolved **critical** (security, data loss) or
**high** (bug, significant quality) issues. Medium and low comments are suggestions the author
may take or decline with a reason.
