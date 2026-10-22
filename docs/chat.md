# Chat mode

Chat mode puts a conversation with an agent next to the canvas. Press ⌘J, or use the `Design | Chat` switch in the top bar, to turn it on. The chat column sits on the left (420 to 640 px, 480 by default, resizable) and the live canvas fills the rest. The layers and inspector are hidden; the inspector can be opened as a narrow overlay on the canvas's right edge. Design mode works as before, and a chat keeps running in the background when you switch back.

The mode is remembered per project. Entering a project that was last left in Chat mode opens in Chat mode and shows its newest chat.

Chat mode is native only. The web build shows the switch disabled with a note.

## Backends

Chat drives the same tools as an agent connected over MCP, so the edits, runs, undo and revert behave the same whichever backend you use. There are three ways in: two CLIs that PaperCraft talks to directly, any agent that speaks ACP, and API endpoints.

**Claude Code** runs `claude -p` with streaming JSON in and out. PaperCraft writes an MCP config that points the CLI at the app's own server on `127.0.0.1`, allows the `mcp__papercraft__*` tools, and passes `--strict-mcp-config`, so the CLI sees only PaperCraft's server. The PaperCraft skill text goes in through `--append-system-prompt-file`. The working directory is the project folder. Follow-up messages resume the CLI session by its id, so the agent keeps its context.

**Codex** runs `codex exec --json` in the project folder. The PaperCraft server is added with a `-c mcp_servers.papercraft.url=...` override, and its tools are pre-approved (`default_tools_approval_mode="approve"`), so Codex does not stop to ask for each one. The PaperCraft instructions are prepended to the first prompt of a chat; resumed sessions do not repeat them. Follow-ups resume the Codex session the same way.

**ACP agents** run any agent that speaks the Agent Client Protocol (https://agentclientprotocol.com) over stdio. PaperCraft starts the agent once per turn, sends `initialize` and `session/new` with the project folder as the working directory, and passes PaperCraft's MCP server: over HTTP when the agent supports it, otherwise through the bundled `papercraft-mcp` stdio bridge. The prompt goes out as `session/prompt`, and the agent's streamed updates become the same thread as the other backends: text, thinking, tool cards and the plan. Stop sends `session/cancel`. Follow-ups resume with `session/load` when the agent supports it. ACP has no system prompt field, so the PaperCraft instructions go in front of the first prompt.

The ACP agents PaperCraft knows about, and what was checked on the development Mac:

* **OpenCode** (`opencode acp`): works.
* **Gemini CLI** (`gemini --acp`): found and started, but its server currently refuses ACP clients, so turns fail with the agent's error until Google changes that.
* **Cursor** (`cursor-agent acp`): sign in first with `cursor-agent login`.
* **Pi** (`pi-acp`, an adapter run through `npx`): enable it once in the agent panel or Settings, then it works like the others.
* **Goose, Kimi, Devin, Junie, Mistral Vibe, Hermes, OpenHands, Stakpak, VT Code, Poolside, Droid**: started with the command their own docs give. They are not all verified here.
* **Claude Code and Codex ACP adapters** (`npx`): listed under Enable via npx. Claude Code and Codex themselves use the direct backends above.

Agents that run through `npx` are off until you enable them, because enabling runs code that npx fetches from npm.

ACP permissions: PaperCraft's own tools run without asking, except the destructive ones listed under Approval below. Any other tool the agent asks to run (its own shell or file tools, for example) shows an approval card with Approve or Deny in the thread, and Deny is sent back to the agent.

**Custom agents** are your own ACP agents: a name, a command (a program name on the resolved `PATH`, or a full path) and arguments. The arguments are split on spaces, with no quoting. Custom agents appear under Custom in the agent panel and in Settings → Chat.

**API endpoints** call a model over HTTPS with your own key. Settings → Chat holds a list of endpoints, each with a name, a format, a base URL, a model and an optional key. The app's tool list is sent as the API tools, and every tool call runs through the same code path as an MCP call on the UI thread. The loop continues until the model ends its turn, for up to 40 tool rounds per message.

* **Format**: `Anthropic` is the Messages API (`POST {base}/v1/messages`, `x-api-key`). `OpenAI` is Chat Completions (`POST {base}/chat/completions`, `Authorization: Bearer`, streaming `choices[].delta`, tools as `{"type":"function",...}`, tool results as `tool` messages). Both give the same thread: text, tool cards, approvals, Stop, the usage line.
* **Base URL and model** are free text, for example `https://api.anthropic.com`, any Anthropic-compatible URL, or `http://127.0.0.1:11434/v1`. The key is optional, so local servers need none.
* A default endpoint named `Anthropic` (`https://api.anthropic.com`, `claude-opus-5-5`) exists from the start, so the first run needs only a key.
* The agent panel lists Detected agents, API endpoints and Custom agents (see Setup). Each chat remembers its backend.
* **Claude-only fields** (effort, the thinking summary, the refusal fallback and its beta header, eager tool-input streaming, prompt-cache breakpoints) are sent only when the base URL host is `api.anthropic.com`. For that host the model menu offers `claude-opus-5-5`, `claude-sonnet-5-5` and `claude-haiku-5-5`, effort is `medium` by default (`low` and `high` in the menu), and the Thinking row shows the summary. Every other endpoint gets a plain request. Missing `usage`, thinking or cache fields in the reply are fine.
* **OpenAI format and images**: attached images are sent as `image_url` data URIs. A tool message in this format carries text only, so a tool result with an image (such as `get_screenshot`) goes back as the text `[image omitted]`, and the image follows as an extra user message in the same round.
* **Usage line**: input, output and cached tokens, as the server reports them; nothing when it reports none. A cost estimate shows only for the Claude models in the small price table in `src/chat/endpoint.rs`. It is not a bill.
* `ANTHROPIC_API_KEY` in the environment is used for `api.anthropic.com` when no key is stored. `PAPERCRAFT_ANTHROPIC_BASE_URL` points Anthropic-format requests at another URL (for tests and proxies).

## Setup

### Finding agents

Chat finds agents the way your terminal does. At startup PaperCraft reads your login shell's `PATH` once, off the UI thread (`$SHELL -l`, 3 s limit), and merges it with the app's own `PATH` and the usual folders (`~/.local/bin`, `/opt/homebrew/bin`, `/usr/local/bin`, `~/.cargo/bin`, `~/.bun/bin`, `~/.npm-global/bin`, Volta and nvm). Detection and the agent processes both use that merged `PATH`. This is why an agent installed under `~/.local/bin` or Homebrew is found even when PaperCraft was opened from the Dock.

A row shows a version when the agent reports one. A program with the right name that is not the agent (for example the `goose` database migration tool, which shares its name with Block's Goose agent) shows `Wrong program` with its path, not a false Ready.

PaperCraft never shows install commands. An agent that is not found shows `Not found`, with `Set path…` (opens Settings) and `Docs` (the agent's own documentation). Install the agent with its own installer, then press Detect again.

### Agent panel

Click the agent chip in the chat header. The panel opens over the chat column with a shadow and a dimmed backdrop, sized like the column. It has a search field (name or id), and these sections:

* **Detected**: agents found on your `PATH`, with the version and path. Click one to use it for this chat.
* **Enable via npx**: agents that start through `npx` (Pi, Qwen Code, Kilo, Cline, Auggie, Copilot CLI, Qoder, CodeBuddy and the Claude Code and Codex ACP adapters). Each row has an `Enable` chip. The section says that it runs code fetched by npx.
* **Not found**: registry agents that are not on this computer, folded by default.
* **API endpoints**: your endpoints, with an `Add` action that opens Settings.
* **Custom**: `Add custom agent` opens Settings.

Each row has a status chip: `Ready`, `Enable`, `Not found`, `Wrong program`, `No key` (an endpoint without a key) or `Set up` (an endpoint without a URL or model). Arrow keys move through the rows, Enter picks one, Escape closes the panel.

Settings → Chat has the same list, so an agent can also be hidden from the panel there (the `Show in the chat agent picker` switch on its row).

### Setup card

A chat with no backend chosen shows one card, "Connect an agent". It lists the agents detected on this computer, up to four, with `Show all agents` for the rest. Below them are `Add an API endpoint` and `Add custom agent`. The card never lists install steps.

### Sign-in

When an agent says it needs you to sign in (ACP's "Authentication required" and similar messages), the failed turn shows "<Agent> needs you to sign in" with the command from the agent's docs, then Retry. For Cursor that is `cursor-agent login`, for OpenCode `opencode auth login`, for Codex `codex login`, for Claude Code's ACP adapter `claude /login`. For agents without a known command the card says to sign in with the agent's own app and retry. Chat does not sign in for you.

### Settings → Chat

The page scrolls and keeps one grid for its labels. Its sections:

* **Agents**: one row per agent, with its status, version and path. A details disclosure holds the **Path override** (for a binary that is not on `PATH`, or a specific install; empty means the first match on the merged `PATH`), the **Arguments override** (replaces the registry's ACP arguments, for example `--acp --debug`), and for npx agents the **Enable** button. The **Show in the chat agent picker** switch hides an agent from the panel. Custom agents have Name, Command and Arguments fields and a Remove button. `Detect again` re-runs detection.
* **API endpoints**: add, edit and remove endpoints (name, format, base URL, model) and set the default one. Each has a key field with Save and Remove. Keys are stored in the macOS Keychain, Windows Credential Manager or the Linux Secret Service, under the service name `PaperCraft` and the user `api-key:<base URL>`. An older single key (`anthropic-api-key`) is still read for `api.anthropic.com` and moves to the new entry the next time you save it. Removing a key makes that endpoint stop until a key is saved again (for `api.anthropic.com`; other endpoints work without one).
* **Defaults for new chats**: the backend, the endpoint, and the effort for `api.anthropic.com`.

Settings from before endpoints existed (`api_model`) move into the default endpoint on first load. The `claude_path` and `codex_path` settings from earlier versions still win for their agent until you save the Agents list. These defaults, the agent settings and the endpoint list (never the keys) are saved in `chat_settings.json` in your config folder (`~/Library/Application Support/papercraft/` on macOS). They are not part of any project.

## The thread

The header has the chat title (rename), the chat list (new chat, switch, delete), the agent chip (for example `Claude Code · Opus 5.5`) and the context meter. The meter shows the tokens in the context at the last model call against a 200k window, and turns amber past 75%. It is hidden for Codex, which reports no per-call usage.

Replies are markdown: paragraphs, lists, code blocks with a copy button, inline code and links. While a turn runs, the reply streams in.

* **Thinking**: when the backend reports reasoning, a row shows "Thinking…" while it runs and collapses to a summary when it ends. Expand it to read the text.
* **Tool cards**: one card per tool call with the verb and target (for example "Drawing layout · Card"), a spinner while it runs, and a failed state with the error. Args and result are collapsible. `get_screenshot` results show a thumbnail.
* **Step groups**: consecutive tool calls in one reply fold into one line ("Built the card · 5 steps") that expands to the cards.
* **Plan**: when the agent keeps a todo list (Claude Code's `TodoWrite`, or an ACP plan), the latest list is pinned above the composer with a "done of total" count.
* **Approval**: on the API backend and for ACP agents, the destructive tools (`delete_pages`, `disable_library`, `import_design_system`, and `delete_nodes` on more than 20 layers) wait for Approve or Deny before they run. Deny returns an error to the model, which can then carry on. For ACP agents, any non-PaperCraft tool the agent asks for shows the same card. The Claude Code and Codex CLIs do not show this card. They use the CLI's own permission rules, and in non-interactive mode a tool that needs permission is denied. The papercraft tools are pre-allowed on both CLIs.
* **Checkpoint**: when an agent run starts, the thread shows "Checkpoint · before run N". "Restore to here" reverts that run and every run after it, through the same revert as the Agent view.
* **Run card**: when a run ends, "Agent finished, N changes" with Review, Revert and Accept. These are the same actions as the run review on the canvas.
* **Errors**: a failed turn shows a red row with Retry. A sign-in problem shows the sign-in card (see Setup).
* **Canvas follow**: when a tool call targets a node, the canvas pans to it, unless you moved the camera in the last 10 seconds.

## Composer and attachments

Type in the composer, ⏎ sends and ⇧⏎ adds a line. The empty state offers suggestions built from the project, such as "Design a sign-in screen using this project's variables" or "Lint this page and fix warnings".

* **Selection**: the `@ Selection` chip adds the current selection to the message as a chip (also ⌘L). Each click adds another chip, labelled with the layer name or `<first> · N layers`; an identical selection is added once, and each chip is removed on its own. Every chip is sent as a short line with the layer name and node ids.
* **Start failures**: if a turn cannot start (agent missing, bad path), an error row with Retry shows in the thread and your text stays in the composer.
* **Images**: the image button opens a file picker. You can also paste a clipboard image or drop image files on the column (PNG, JPEG, GIF, WebP).

For the API backend, images are sent as image blocks, so the model sees them. For the CLIs and ACP agents, each image is saved under `.papercraft-chat-tmp/` in the project and its path is added to the prompt for the agent to read.

While a turn runs, the send button becomes **Stop**. Stop asks the backend to interrupt: the CLIs get an interrupt, or SIGINT, then a kill after 3 seconds; ACP agents get `session/cancel`, then a kill if they do not exit. Every backend ends within 3 seconds. Tools that already ran stay applied, and the partial run can be reviewed or reverted like any other.

## Storage

A chat is one file in the project: `chats/<id>.json`, holding the title, the backend (kind, model, effort, CLI or ACP session id, agent) and the messages with their tool calls, usage and run ids. The API backend also keeps `chats/<id>.api.json`, the raw messages array (Messages format with thinking blocks, or Chat Completions format, whichever the endpoint speaks), so a follow-up replays exactly. The chat records its endpoint id. Deleting a chat asks first and removes both files, and its saved images.

Projects without a `chats/` folder load unchanged. The newest chat opens when you enter Chat mode.

## Privacy

PaperCraft never writes an API key into a project file, a chat or a log. The key lives in the OS credential store. The CLIs and ACP agents run with your own logins and do not get the API keys from the environment: `ANTHROPIC_API_KEY`, `ANTHROPIC_AUTH_TOKEN`, `OPENAI_API_KEY` and `CODEX_API_KEY` are removed from their environment, so Claude Code and Codex bill your account, not a key you set for PaperCraft. The MCP server listens on loopback only, as it does for any other client.

Prompts, project content and tool results go to whichever backend you chose: the CLI's or agent's model service, or the base URL of the endpoint you picked. Nothing goes to any other service. An npx agent downloads its package from npm the first time it runs. If a server echoes your key in an error message, the app hides it before showing the message.

## Limits

* One turn per project at a time. Sending while a turn runs is refused with "A turn is already running"; use Stop first. Switching to another project stops a running turn.
* The web build has no Chat mode.
* The canvas shows the run's outlines after the turn ends, when the run is reviewed. They are not drawn live while the agent works.
* The API backend stops after 40 tool rounds in one message and says so.
* Codex reports no per-call usage, so the context meter is hidden for it.
* Agents need to be installed and signed in with their own tools. Chat does not install them or sign them in for you.
* ACP terminal and file-system requests from an agent are answered as not supported. Agents use their own file tools.
