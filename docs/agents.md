# Driving PaperCraft with agents

PaperCraft exposes one set of 88 tools three ways. They all run the same code inside the app, so an agent sees what you see and every call is one undo step.

| Way | Use it when | Needs the app running |
|---|---|---|
| MCP over HTTP, `http://127.0.0.1:29980/mcp` | the agent client speaks MCP (Claude Code, Codex, pi) | yes |
| MCP over stdio, `papercraft-mcp` | the client only starts stdio servers | for calls; `tools/list` works offline |
| Shell, `papercraft-cli` | the agent has a terminal and you want no tool schemas in its context | for `call`; `list`, `schema` and `icons` work offline (with the app running, `list` and `schema` show the app's tools) |

If you would rather talk to the agent inside the app, Chat mode (⌘J, or `Design | Chat` in the top bar) is the in-app alternative. It runs Claude Code or Codex through their CLIs against the same MCP server, ACP agents such as OpenCode or Gemini CLI, or the Anthropic API with your own key, and keeps the conversation next to the canvas, with the same runs, review and revert. The setup and the thread are in [chat.md](chat.md). The rest of this guide covers driving the app from your own terminal or agent client.

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

## Without the app

`papercraft-cli` also works with PaperCraft closed (a server, CI, a headless box). When nothing answers on the port, or with `--offline`, tools run inside the CLI on the project store (`$PAPERCRAFT_HOME`, else the default folder) and every editing call is saved. Projects made this way open normally in the app.

```sh
papercraft-cli call create_file '{"name":"Demo"}'          # -> fileId
papercraft-cli call create_artboard '{"name":"Card"}' --project Demo
papercraft-cli call get_screenshot '{"nodeId":"abc123:1"}' --project Demo -o card.png
```

`--project <name|id|path>` opens that project first; without it the most recently modified one is used. With the app open on a project, plain `call` goes to the app, and `--offline` on that same project is refused ("PaperCraft has this project open") so two writers never share a folder. Screenshots need a GPU adapter; with none, the software adapter is tried, and if that fails the image tools return an error while all other tools work.

### Shell and scripts

When `call` or `shell` reaches the app and the app is another version, the CLI prints once on stderr, for example `papercraft-cli 0.6.1 ≠ app 0.6.2; reinstall the CLI`. `list` and `schema` read the running app's tool list, so tools newer than the CLI still show; with the app closed they use the CLI's built-in list.

`papercraft-cli shell [--project X] [--offline]` keeps one session open (offline, one in-process project; otherwise the app). A line is `<tool> <json>` (JSON may span lines until its braces close) or a command: `:help :tools :schema <tool> :open <project> :pages :page <name|id> :shot [nodeId] [file] :undo :redo :save :watch on|off :quit`. `:watch on` renders the artboard last edited to `preview.png` after each editing call, shown inline on iTerm2, WezTerm and kitty. Input is plain stdin (no line editing or history; use `rlwrap` for that). Piped input prints no prompt and exits 1 if any line failed, listing the line numbers on stderr.

```sh
papercraft-cli shell --offline --project Demo <<'EOF'
# build a card
create_artboard {"name":"Card","styles":{"width":"320px","height":"180px","backgroundColor":"#1e1e2e"}}
write_html {
  "targetNodeId": "abc123:1",
  "mode": "insert-children",
  "html": "<div style='font-size:28px;color:#cdd6f4'>Hello</div>"
}
:shot abc123:1 card.png
EOF
```

## Checking only what changed

On a big page a full `lint_design` after every edit repeats the same output, and a `get_node_info` call per layer is slow (about 53 s for 397 layers). Check the change, then check everything once.

* Every write tool result includes `rev`, the document revision after the call. `get_basic_info` reports the current `rev`. Reads never change it.
* `get_changes {since, page?}` returns `{rev, since, truncated, changes: [{page, ids}]}`: what the edits since `since` touched. `truncated: true` means the log no longer reaches that far back, so do a full pass.
* `lint_design {changedOnly: true}` lints since this client's last lint; `{since: rev}` lints since a revision you name. A scoped lint checks the touched layers plus their ancestors, descendants, siblings and every layer bound to a changed variable, and reports its coverage, for example `{checked: 12, total: 397, since: 40, partial: true}`. Plain `lint_design` is a full pass (`partial: false`). The `overflow` rule flags a layer that extends past its parent or clipping artboard (scroll frames are excluded).
* `get_bounds {nodeIds?, pageId?, depth?}` returns `[{id, parent, x, y, w, h, absX, absY, clip}]` for a subtree in one call (capped, with `truncated`). Use it instead of looping `get_node_info`.
* Always finish with one full `lint_design` (no arguments) and a screenshot per artboard in each theme mode before reporting. A partial lint speeds up the loop; it does not replace that pass.

In the app, read-only calls no longer draw per-layer outlines or keep the "Working" pill lit for long. The agent workflow, with an example, is in [incremental-checks.md](../skills/papercraft/references/incremental-checks.md).

## Design-system tools

* Variables: `get_variables` and `set_variables` take collections, groups (`color/brand/primary`), types (`color`, `number`, `string`, `boolean`), `mode`, `alias`, `description` and `hidden`; the old `{"name":"#hex"}` call still works and lands in the first collection. `create_collection` makes a collection with modes, `delete_variables` removes by name (refuses aliased ones unless `force`), `bind_variable` binds or detaches a field of layers (`stroke`, `text-color`, `width`, `height`, `gap`, `padding`, `radius`, `font-size`, `line-height`, `opacity`, `visible`).
* Components: `get_components` lists mains of every page and enabled library with a `componentKey`; `create_instance` and `swap_instance` accept a main from any page or library (`componentId` or `componentKey`).
* Libraries: `publish_library`, `list_libraries`, `enable_library`, `update_library`, `disable_library`. They work offline too, on the project store.

## Seeing what the user sees

* `get_basic_info` and `get_selection` include `viewport: {pageId, rect: [x, y, w, h], zoom}`: the page the user is viewing and the world rect on screen.
* `set_viewport {pageId?, nodeIds? | rect?, zoom?, padding?, force?}` switches the user's page (as the page list does) and fits the nodes or rect, or centres them at an explicit `zoom` (0.01 to 256; `select_nodes {zoom: true}` stops at 100%). While the user is panning or zooming (10 s after their last camera move) it is refused with "user is moving the canvas", unless `force: true`.
* `find_nodes {textValue}` finds text layers containing the phrase on every page (or `pageId`), ignoring case and line breaks; `*` wildcards match the whole value instead. `get_tree_summary {nodeId, fullText: true}` prints every text layer's complete text, for copy review across screens.
* `get_screenshot {nodeId, annotations: true}` draws the notes on the captured layers (cards, number badges, leader lines), so an agent can check note placement.

## Annotation tools

* `annotate {pageId?, notes: [{node, text, type?}]}` adds many notes in one undo step. `node` is a node id or an exact layer name; the first line of `text` is the label and the rest the body; `type` is a category key (default `rule`) that the page's template must enable. Placement is automatic. `annotate {tidy: true, frame?}` with no notes re-lays-out the existing notes and returns `{"tidied": n}`, the number of notes it moved.
* `get_annotation_template {pageId?}` returns the allowed types with their guidance and fields. Call it once before `annotate`.
* `set_annotation_template {template, scope?: "page" | "project", pageId?}` switches the template by id or name.
* `upsert_annotation_template {template: {id?, name, display?, categories: [{key, enabled?, guidance?, fields?}]}, categories?: [{key, name, color, icon}], setAs?: "project" | "page", pageId?}` creates or replaces a template and its category styles in one undo step. `delete_annotation_template {template}` removes one (the last stays).
* `get_annotations {nodeId?, pageId?, full?}` reads notes as `[number, node, type, text]` rows.
* `update_annotation {id | ref, text?, type?}` and `delete_annotations {ids}` take ids or references such as `Sign in #3`.

See [annotations.md](annotations.md) for categories, templates and what the notes look like.

## Safety

The server listens on loopback only, rejects requests that carry an `Origin` header (browsers) and foreign `Host` headers. Anything on your machine that can reach `127.0.0.1:29980` can edit the open project, so do not forward the port. Undo (⌘Z, or the `undo` tool) reverts any agent edit.

## Runs and review

Editing calls are grouped into an agent run. A run ends after 30 seconds without an editing call, when the project changes, or when `finish_working_on_nodes` is called. Before each page's first edit in a run, PaperCraft saves an auto-version named `Before agent run N`. Call `finish_working_on_nodes` at the end of your work so the run gets a useful title and ends right away.

The Agent view lists runs and lets you review changes, revert a whole run, undo one change, or accept all changes. Review also marks changes on the canvas; a toast offers Review and Revert when a run finishes. The command palette has `Review last agent run` and `Revert last agent run`. Run records live under `runs/` in the project folder; PaperCraft keeps the newest 50 runs.

## Reference

* [Tool reference](../skills/papercraft/references/tools.md), generated from `papercraft-cli schema`.
* [Design systems](design-system.md): collections, variables and bindings, components, libraries, icons, lint.
* [Design-system packages](design-system-packages.md): importing and exporting an OpenDesign package.
* `papercraft-cli --help`-style usage: run `papercraft-cli` with no arguments.
