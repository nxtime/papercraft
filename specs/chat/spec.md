# Chat layout: design by conversation

## Why

Today an agent drives PaperCraft from outside: a terminal running Claude Code or Codex, pointed at the MCP server. The app shows a working pill, the agent runs and their review, but the conversation itself lives elsewhere. OpenDesign and Cursor's agent view put the conversation next to the result. PaperCraft already has everything behind that: 61 tools, agent runs with auto-versions and review, skills, design systems and libraries.

## Decisions (user, 2026-10-21)

* Backend: **both** local agent CLIs (Claude Code, Codex) and a bring-your-own-key Anthropic API path.
* Layout: a separate **Chat layout**, chat column left, live canvas right, switched from the top bar (`Design | Chat`).
* Conversations live **in the project** (`chats/`), linked to the agent runs they produce.
* Mock-ups first.

## What exists today

* MCP server on `127.0.0.1:<PAPERCRAFT_MCP_PORT>` (default 29980), loopback only, rejects `Origin` headers (`src/mcp/server.rs`). Every tool runs on the UI thread through `App::run_mcp_calls` and `mcp::exec::call`.
* Agent runs (`src/runs.rs`): editing calls grouped into runs with auto-versions, review, revert, the finish toast.
* Tool list and schemas: `mcp::server::tool_list()`, `crate::mcp::tools::edits_doc`.
* Skills: `skills/papercraft`, `skills/papercraft-design`, `skills/papercraft-design-system` (in repo) and the installed copies under `~/.agents/skills/`.
* No HTTP client for outbound API calls except `reqwest` (already in `Cargo.lock` through rmcp) and `curl` used by the CLI's icon download.

## Layout

* **Mode switch** in the top bar: `Design | Chat` (segmented control), shortcut ⌘J. The mode is per window and remembered per project.
* **Chat mode**: left column 420–640 px wide (resizable, default 480), canvas on the right with the normal top bar, zoom and a slim tool strip; the layers, inspector and side views are hidden (a toggle brings the inspector back as a narrow overlay).
* **Chat column**, top to bottom:
  * header: chat title (rename), chat list menu (new chat, switch, delete with confirm), agent picker chip (`Claude Code · Opus 5.5`, `Codex · gpt-6.1-sol`, `API · Opus 5.5`) and a settings icon;
  * message list: user messages; assistant text streamed as it arrives (markdown: paragraphs, lists, code, inline code, links); thinking/progress shown as a collapsed one-line "Thinking…" row when the backend provides it; **tool-call cards**: verb + target ("Drawing layout · Card", from `agent_verb`), status (running spinner, done, failed with the error), collapsible args/result JSON, a thumbnail for `get_screenshot` results; a **run card** when an agent run ends (`Agent finished, N changes` with Review, Revert, Accept, reusing the run review); errors as a red row with Retry;
  * composer: multi-line input (⏎ sends, ⇧⏎ newline), attachment chips (current selection as `@Frame name` chips added by a button or ⌘L, images by paste/drag, the active page), a Stop button while a turn runs, a model/effort menu for the API backend;
  * empty state: suggestions built from the project ("Design a sign-in screen using this project's variables", "Lint this page and fix warnings", "Make a dark mode").
* **Patterns from shadcn AI Elements** (ui.shadcn.com/docs/ai-elements; React components, so only their patterns and names are reused, built from `src/ui/kit.rs`):
  * **Chat** thread with a message scroller that sticks to the bottom while streaming and keeps your place when you scroll up (a "Jump to latest" pill appears).
  * **Message**: user and assistant turns; assistant actions on hover (copy, retry).
  * **Reasoning**: collapsible, streams while open, collapses to "Thought for 12s" when done.
  * **Tool**: one card per call, states pending / running / done / error, input and output sections collapsible; a screenshot output shows inline.
  * **Task / Chain of Thought**: consecutive tool calls in one assistant turn group into a step list ("Built the card · 5 steps") that expands.
  * **Plan / Queue**: when the agent keeps a todo list (Claude Code `TodoWrite`, or the API backend's plan), show remaining and completed items pinned above the composer.
  * **Confirmation**: approve or deny before destructive tools run (`delete_pages`, `delete_nodes` on more than 20 layers, `disable_library`, `import_design_system`); API backend pauses the loop; CLI backends rely on the CLI's own permission prompt where it exists, else the tool stays allowed and the run can be reverted.
  * **Checkpoint**: each agent run's auto-version shows as a checkpoint in the thread; "Restore to here" reverts the runs after it (reuses `runs::revert`).
  * **Context**: a small meter in the header with tokens used of the model's context window (API and Claude Code report usage).
  * **Model Selector**: backend + model menu and an effort menu.
  * **Prompt Input**: multi-line field, attachments (selection, images, page), send / stop.
  * **Suggestion**: one-click prompts in the empty state and after a run ("Lint this", "Make a dark mode", "Review changes").
  * **Subagent**: a nested, collapsible block when a CLI agent spawns a sub-agent.
  * **Code Block**: fenced code in replies with a copy button.
  * Not used: Sources, Inline Citation, File Tree, Terminal, Stack Trace, Test Results (not design work).
* **Canvas side**: follows the agent: when a tool call targets a node, the canvas pans to it unless the user moved the camera in the last 10 s; the run overlay (green/yellow/red outlines) shows while a run is open.
* Design mode keeps working as today; the chat keeps running in the background when you switch.

## Backends

All backends drive the same tools, so runs, undo, review and revert work the same.

### Local agent CLIs

* **Claude Code**: spawn `claude -p` with streaming JSON in and out (`--input-format stream-json --output-format stream-json --verbose`), an MCP config pointing at the app's server (`--mcp-config` with `{"mcpServers":{"papercraft":{"type":"http","url":"http://127.0.0.1:<port>/mcp"}}}`), the papercraft tools allowed (`--allowedTools "mcp__papercraft__*"`), working directory = the project folder, and the system prompt appended with the PaperCraft skill text. Resume with `--resume <session_id>` (stored on the chat). Exact flags are checked against `claude --help` at implementation time and wrapped in one adapter.
* **Codex**: spawn `codex exec --json` with the server added through `-c mcp_servers.papercraft.url=...` (or the current equivalent, checked against `codex exec --help`), resume with its session id. Same adapter shape.
* **Detection**: Settings → Chat lists the CLIs found on `PATH` (plus a path override) with their version; a missing CLI shows install hints, never an error dialog.
* **Events**: each adapter turns the CLI's JSON lines into one internal event stream: `Text(delta)`, `Thinking(delta)`, `ToolStart{id,name,args}`, `ToolEnd{id,result|error}`, `TurnEnd{usage?}`, `Error`. Tool calls also arrive through the MCP server, so the card status is confirmed by the app itself (`log_agent`), not only by the CLI's report.
* **Stop**: sends an interrupt (stdin control message where supported, else SIGINT, then kill after 3 s). The run so far stays reviewable.
* **Security**: the CLI runs with the user's own permissions and logins; PaperCraft passes no keys. Only papercraft tools are pre-allowed; other tools follow the CLI's own permission settings (non-interactive: denied).

### Anthropic API (bring your own key)

* Raw HTTPS to `POST https://api.anthropic.com/v1/messages` with `stream: true` (no Rust SDK exists), using `reqwest` (already a dependency).
* **Key**: entered in Settings → Chat, stored in the macOS Keychain (Linux: Secret Service; Windows: Credential Manager) through the `keyring` crate, never in project files or logs. `ANTHROPIC_API_KEY` in the environment is used when no key is stored.
* **Model**: default `claude-opus-5-5`; menu also offers `claude-sonnet-5-5` and `claude-haiku-5-5`. Thinking adaptive (default on these models), `output_config.effort` set explicitly (default `medium`, menu low/medium/high), `thinking.display: "summarized"` so the Thinking row has text. Server-side refusal fallback on (`fallbacks: "default"` with beta `server-side-fallback-2026-07-01`; for Haiku, show the refusal instead).
* **Tools**: the app's own tool list (`tool_list()` converted to Messages API `tools`: name, description, input_schema), with `eager_input_streaming: true`; inputs are parsed tolerantly, validated against the schema before running, and run through `mcp::exec::call` on the UI thread like any MCP call (so runs and undo behave the same). Several `tool_use` blocks in one turn run in order and all `tool_result` blocks go back in one user message; failures go back as `is_error: true`. No forced `tool_choice` (not supported on these models).
* **Loop**: stream → collect `tool_use` → run → append `response.content` unchanged (thinking blocks included) + tool results → next request, until `stop_reason` is `end_turn`; `max_tokens` → continue once with a note; `refusal` → show the category; `pause_turn` → continue. Max 40 tool rounds per user message (then stop and say so).
* **Caching**: `cache_control` breakpoints on the tool list and on the system prompt (skill text + project context), stable ordering so they hit; per-turn volatile context (selection, active page) goes in the user message, not the system prompt. `usage.cache_read_input_tokens` is logged to verify.
* **Images**: `get_screenshot` results go back as image blocks so the model sees what it made; user-attached images too.
* **Cost line**: per turn input/output/cache tokens and an estimated cost from a small price table, shown under the turn in a muted row.
* Implementers read the bundled claude-api skill files `curl/examples.md`, `shared/tool-use-concepts.md`, `shared/prompt-caching.md`, `shared/model-migration.md` (Opus 5.5 section) before writing this adapter.

### Custom endpoints (generic, no provider list)

The API backend is not tied to Anthropic. Settings → Chat holds one or more **endpoints**, each `{ name, format, base_url, model, key }`:

* `format`: `anthropic` (Messages API, `POST {base}/v1/messages`, `x-api-key`) or `openai` (Chat Completions, `POST {base}/chat/completions`, `Authorization: Bearer`, streaming SSE `choices[].delta`, tools as `{"type":"function","function":{name,description,parameters}}`, `tool_calls` with `arguments` streamed as string fragments, results as `role: "tool"` messages).
* `base_url`: free text (e.g. `https://api.anthropic.com`, any Anthropic-compatible URL, `http://127.0.0.1:11434/v1`). `model`: free text. `key`: optional (local servers need none), stored in the OS keychain under the endpoint's base URL, never in settings files.
* Claude-only request fields (`output_config.effort`, `thinking.display`, `fallbacks` and its beta header, `eager_input_streaming`) are sent only when the base URL host is `api.anthropic.com`; other Anthropic-format endpoints get a plain request (adaptive thinking omitted), and are tolerant of missing `usage`, `thinking` blocks, or `cache_*` fields.
* The cost line shows a price only for models in the built-in Claude price table; otherwise tokens only (or nothing when the server reports no usage).
* No provider names, presets or logos in the app. A default endpoint `Anthropic` (`anthropic`, `https://api.anthropic.com`, `claude-opus-5-5`) exists so the first-run path stays one key away.
* The agent picker lists every configured endpoint by its name.

### Not in scope (v1)

* Named provider presets.
* Multiple chats running at once in one project (one active turn per project).
* Voice, file uploads other than images.

## Storage

* `chats/<id>.json` in the project folder: `{ id, title, created, updated, backend: {kind, model?, effort?, cli_session?}, messages: [ {role, text, tools:[{id,name,args,result,error,ms}], run_id?, usage?, at} ] }`. API backend also keeps the raw `messages` array (for exact replay, including thinking blocks) in `chats/<id>.api.json`.
* Chats sync with the project (relay). Deleting a chat deletes both files after a confirm.
* The project's newest chat opens when entering Chat mode.

## MCP and CLI

* No new tools needed. `papercraft-cli` is unchanged.

## Acceptance

* Chat mode toggles from the top bar and ⌘J; layout matches the mock-ups at 1440p and 1280x800.
* With Claude Code installed: "Create a 390x844 artboard named Test with a centred title" streams text, shows tool cards for `create_artboard` and `write_html`, the canvas follows, a run card appears at the end, Review/Revert work; a follow-up message continues the same CLI session.
* Same with Codex.
* With an API key: the same prompt works through the API backend; thinking row shows a summary; cost line shows tokens; a second turn reads from cache (`cache_read_input_tokens > 0`).
* Stop during a long turn ends it within 3 s; tools already applied stay and are revertible.
* Missing CLI and missing key show setup states, not errors; a bad key shows the API error message.
* Chats persist in `chats/` and reopen after restart; a project without `chats/` loads unchanged.
* The key never appears in project files, logs or crash output (test greps the project folder and the log after a run).
* Tests: event parsing for each adapter from recorded JSON fixtures (Claude Code stream-json, Codex --json, Anthropic SSE incl. thinking, tool_use with input_json_delta, refusal, max_tokens); tool-schema conversion; chat file round trip; the API loop with a fake HTTP server (two tool rounds then end_turn).
* `cargo test --no-fail-fast`, wasm and Windows checks pass (chat backends are native-only; the web build shows Chat mode disabled with a note).
