# Guidance for coding agents

PaperCraft is a native Rust design and whiteboard canvas (egui, wgpu, winit) that AI agents drive
over MCP and a CLI. Read [CONTRIBUTING.md](CONTRIBUTING.md) and [RULES.md](RULES.md) first; this
file is the short version for agents, and [AI_POLICY.md](AI_POLICY.md) says what the person using
you is accountable for.

## Commands

```sh
cargo build --release
cargo fmt --all
cargo clippy --all-targets
cargo test --no-fail-fast                       # prefer one test: cargo test --test mcp name
cargo check --lib --target wasm32-unknown-unknown
```

`tests/update.rs` cannot run on Windows (it needs elevation); that failure is known.

## Running the app safely

Never run a test copy against the person's real projects or MCP port, and never close or restart
their running PaperCraft. Give yours its own folder and port, and keep its PID so you can stop only
your own process:

```sh
PAPERCRAFT_HOME=/tmp/pc PAPERCRAFT_MCP_PORT=29998 ./target/release/papercraft
```

Do not stop processes by name.

## Layout

`src/model` (document), `src/editor` (tools and edits), `src/render` (wgpu), `src/mcp` (tools,
HTML/CSS mapping, lint), `src/ui` (egui panels), `src/io` (import and export), `src/project`
(store and formats), `tests/` (scenario tests), `specs/` (feature specs), `skills/` (agent
skills). The README's project layout lists the rest.

## Conventions

* Tests first; match the style of the tests next to yours, and check the behaviour is not already
  covered.
* Changes to the document go through `Editor::edit`: one transaction, one undo step.
* Handle errors; do not `unwrap` user data and do not discard errors with `let _ =`.
* Comments explain *why*, never summarise the code. No dead code, debug prints or
  commented-out blocks.
* Files stay under about 800 lines; split by feature.
* Reuse the shared UI kit and theme instead of one-off values. Nothing may overflow its container.
* Writes to projects are atomic and format changes ship a migration (see RULES.md, "Data safety").
* Adding or removing an MCP tool means a test in `tests/mcp.rs`, a precise description, and the
  tool count updated in the README and ROADMAP.
* Commits are conventional (`feat(scope): summary`) and focused. Do not commit, push or tag unless
  the person asked you to.

## Designing with PaperCraft (skills, examples, screenshots)

* Themes are modes, never copies: one artboard per screen, a value per mode on each variable, and
  `set_variable_mode` to switch. Never duplicate artboards for light and dark.
* Every colour on a themed screen is a variable.
* Check each artboard for overflow, programmatically and by screenshot in each mode, and run
  `lint_design`.
* Release design locks when you finish.

## Avoid

* Creative additions nobody asked for; stay inside the task.
* Trivial tests, wide or optional types added to appease the compiler, and swallowed errors.
* Editing generated files, lockfiles or release artifacts by hand, or touching files unrelated to
  the task. Leave other people's uncommitted work alone.
