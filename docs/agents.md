# Driving PaperCraft with agents

PaperCraft exposes one set of tools (40 at the time of writing) three ways. They all run the same code inside the app, so an agent sees what you see and every call is one undo step.

| Way | Use it when | Needs the app running |
|---|---|---|
| MCP over HTTP, `http://127.0.0.1:29980/mcp` | the agent client speaks MCP (Claude Code, Codex, pi) | yes |
| MCP over stdio, `papercraft-mcp` | the client only starts stdio servers | for calls; `tools/list` works offline |
| Shell, `papercraft-cli` | the agent has a terminal and you want no tool schemas in its context | for `call`; `list`, `schema` and `icons` work offline |

## Install

1. Get the app and the tools: the DMG and the `papercraft-cli` zip from the [releases](https://git.marcospaulo.dev.br/masi/papercraft/releases), or `cargo build --release` (binaries `papercraft`, `papercraft-cli`, `papercraft-mcp` in `target/release/`).
2. Put `papercraft-cli` and `papercraft-mcp` on your `PATH` (for example in `~/.local/bin`).
3. Start PaperCraft and open or create a project. The server starts with the app. `PAPERCRAFT_MCP_PORT` changes the port (the CLI and the bridge read it too); `PAPERCRAFT_MCP=0` turns the server off.

Check the connection:

```sh
papercraft-cli call get_basic_info     # project, page, artboards
```

Exit status 1 with "isn't running" means the app is closed or the port differs.

## Register with an agent

Claude Code:

```sh
claude mcp add --transport http --scope user papercraft http://127.0.0.1:29980/mcp
# or through the stdio bridge
claude mcp add --scope user papercraft -- /path/to/papercraft-mcp
```

Codex:

```sh
codex mcp add papercraft --url http://127.0.0.1:29980/mcp
codex mcp add papercraft -- /path/to/papercraft-mcp      # stdio alternative
```

pi (and other agents without built-in MCP): use the shell. Tell the agent to run `papercraft-cli list`, then `papercraft-cli schema <tool>` for the tools it needs, and install the skills below.

Any other MCP client: point it at the HTTP URL, or run `papercraft-mcp` as a stdio server.

## Install the skills

Two agent skills in [`skills/`](../skills) teach the workflow and the design rules:

* `papercraft`: how to drive the tools (connection check, build, screenshot, lint, fix loop, naming, layout recipes, known limits).
* `papercraft-design`: how to make the result look good (a direction first, 4/8pt spacing, a type scale, contrast, states, components and tokens).

Link them where your agent looks for skills, for example `ln -s "$PWD/skills/papercraft" ~/.agents/skills/papercraft` (same for `papercraft-design`).

## First prompts

* "In PaperCraft, create a 390×844 mobile artboard and design a sign-in screen. Use variables for every color, build the button as a component, and run `lint_design` before you finish."
* "Add a dark mode to this project: define Light and Dark values for every variable and switch with `set_variable_mode`."
* "Draw a flowchart of our deploy process on a new page, hand-drawn style."
* "Lint this page and fix every warning."

## A session by hand

```sh
export PATH="$PWD/target/release:$PATH"
papercraft-cli call get_basic_info
papercraft-cli call set_variables '{"variables":{"bg":"#1e1e2e","text":"#cdd6f4","accent":"#cba6f7"}}'
papercraft-cli call create_artboard '{"name":"Hello","styles":{"width":"390px","height":"844px","backgroundColor":"var(--bg)","padding":"24px"}}'
# -> {"id":"abc123:1",...}
echo '{"targetNodeId":"abc123:1","mode":"insert-children","html":"<div layer-name=\"Title\" style=\"font-size:32px;font-weight:700;color:var(--text)\">Hello</div>"}' | papercraft-cli call write_html -
papercraft-cli call lint_design '{"nodeId":"abc123:1"}'
papercraft-cli call get_screenshot '{"nodeId":"abc123:1"}' --out hello.png
papercraft-cli call finish_working_on_nodes '{"nodeIds":["abc123:1"]}'
```

Pass JSON inline, or `-` to read it from stdin (best for HTML, which is full of quotes; `jq -Rs` can wrap a file into the right JSON). Text results go to stdout. Images are saved to `--out` (default `<tool>.png`) and the path is printed.

## Safety

The server listens on loopback only, rejects requests that carry an `Origin` header (browsers) and foreign `Host` headers. Anything on your machine that can reach `127.0.0.1:29980` can edit the open project, so do not forward the port. Undo (⌘Z, or the `undo` tool) reverts any agent edit.

## Reference

* [Tool reference](../skills/papercraft/references/tools.md), generated from `papercraft-cli schema`.
* [Design systems](design-system.md): variables, components, icons, lint.
* [Design-system packages](design-system-packages.md): importing and exporting an OpenDesign package.
* `papercraft-cli --help`-style usage: run `papercraft-cli` with no arguments.
