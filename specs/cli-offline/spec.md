# Design from the CLI without the app

## Why

`papercraft-cli call` needs the app running with a project open. Agents on a server, in CI, or on a machine where the app is closed cannot design at all. The code that runs every tool already works without a window: `SessionHost` (`src/mcp/exec.rs:52`) holds an `Editor` and a `Session` and has an offscreen renderer (`Headless`) for screenshots and PNG export. Tests drive it today.

## Scope

1. **Offline calls.** `papercraft-cli call <tool> [JSON]` runs in-process on a `SessionHost` when the app is not reachable, or always with `--offline`. Projects come from the same store (`$PAPERCRAFT_HOME`, else the default folder). `--project <name|id|path>` opens a project first; without it, the most recently modified project is used, and `create_file` / `open_file` work as usual. Every editing call saves before exit.
2. **Safety.** When the app is reachable, plain `call` keeps going through the app (it is the one with the open file). `--offline` while the app has the same project open is refused ("PaperCraft has this project open; close it or drop --offline"), so two writers never race on one folder.
3. **Shell.** `papercraft-cli shell [--project X] [--offline]` is an interactive session that keeps one host for many calls:
   * a line is `<tool> <json>` (JSON may span lines until braces balance), or a command: `:help`, `:tools`, `:schema <tool>`, `:open <project>`, `:pages`, `:page <name|id>`, `:shot [nodeId] [file]`, `:undo`, `:redo`, `:save`, `:quit`;
   * `:watch on|off`: after every editing call, re-render the artboard last touched (or the page) to `preview.png` in the current folder; on iTerm2, WezTerm and kitty the image is also shown inline in the terminal;
   * history in `~/.papercraft_cli_history` when a line editor is available; plain stdin otherwise (works in pipes, so `papercraft-cli shell < script.pc` runs a script).
4. **Preview without a GPU.** If no adapter is found, retry with the fallback (software) adapter; if that fails too, image tools return a clear error and every other tool still works.
5. **Docs.** `docs/agents.md`: offline use, the shell, scripts. `papercraft-cli --help` updated.

## Not in scope

* A headless MCP server (`papercraft-mcp --offline`); the same host makes it a small follow-up.
* A TUI canvas.

## Acceptance

* With the app closed: `papercraft-cli call create_file '{"name":"Demo"}'`, then `call create_artboard`, `call write_html`, `call get_screenshot -o out.png` produce a saved project and a PNG; opening the project in the app shows the design.
* With the app open on project X: `call` goes to the app; `call --offline --project X` is refused; `--offline --project Y` works.
* `papercraft-cli shell < script` runs a multi-line script and exits 0; a failing tool line exits 1 at the end and prints which line failed.
* `:watch on` writes `preview.png` after each editing call.
* Tests: offline call round trip on a temp store, refusal when a fake "app has it open" check says so, shell script parsing (multi-line JSON, commands).
* `cargo test --no-fail-fast`, wasm `cargo check --lib` and Windows check pass (the CLI binary is native only).
