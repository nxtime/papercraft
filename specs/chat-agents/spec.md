# Chat agents: any agent, found reliably, and a picker that scales

## Why (user report, 2026-10-21)

* Claude Code and Codex show "Not found on PATH" although both are installed in `~/.local/bin`. Root cause: an app opened from the Dock or Finder gets the minimal macOS PATH (`/usr/bin:/bin:/usr/sbin:/sbin`); `launchctl getenv PATH` is empty. The user's login shell also finds `pi`, `opencode`, `gemini`, `goose` (`/opt/homebrew/bin`) and `cursor-agent` (`~/.local/bin`).
* The setup state offers `npm install -g …` as the way to install agents. Claude Code and Codex have their own installers; PaperCraft should not tell people how to install agents at all.
* Only two agents are supported; the user wants every popular agent, generically.
* The agent picker popup has no shadow and overlaps the setup card; the Settings → Chat page has broken spacing, padding and height (fields not aligned, the page bottom cut off).
* Sending a message did nothing: no `chats/` folder was created, no error was shown.
* The composer allows one `@ Selection` chip; the user wants several.

## Scope

1. **Find agents like the shell does.** At startup (native, off the UI thread), resolve the login shell PATH once: `$SHELL -l -c 'printf %s "$PATH"'` with a 3 s timeout; merge with the process PATH and common folders (`~/.local/bin`, `/opt/homebrew/bin`, `/usr/local/bin`, `~/.cargo/bin`, `~/.bun/bin`, `~/.npm-global/bin`, Volta and nvm current bins). Use it for agent detection and for spawning agents (set `PATH` on the child). A path override in Settings still wins.
2. **ACP backend** (`src/chat/acp.rs`): a generic Agent Client Protocol client (JSON-RPC 2.0 over the agent's stdio): `initialize`, `session/new` with `cwd` = project folder and `mcpServers` = PaperCraft's server (HTTP entry when the agent advertises `mcpCapabilities.http`, else a stdio entry running the bundled `papercraft-mcp` bridge with `PAPERCRAFT_MCP_PORT`), `session/prompt` with text (and image) content blocks, streaming `session/update` notifications mapped to our `Event`s (agent message chunks → Text, thought chunks → Thinking, tool_call / tool_call_update → ToolStart/ToolEnd, plan → Plan items), `session/cancel` for Stop, `session/request_permission` answered from our approval flow (papercraft tools allowed; destructive ones and any non-papercraft tool ask the user), `session/load` to resume when supported. The PaperCraft skill text goes in as the first prompt's leading context (ACP has no system prompt field).
3. **Agent registry, as data** (`src/chat/agents.rs`): `{ id, name, detect: binary name, launch: command + args, kind: Native | Npx, docs_url }`. Native entries are found on the resolved PATH: Gemini CLI (`gemini --acp`), Cursor (`cursor-agent acp`), OpenCode (`opencode acp`), Goose (`goose acp`), Kimi (`kimi acp`), Devin (`devin acp`), Junie (`junie --acp=true`), Mistral Vibe (`vibe-acp`), Hermes (`hermes acp`), OpenHands (`openhands acp`), Stakpak (`stakpak acp`), VT Code (`vtcode acp`), Poolside (`pool acp`), Droid (`droid exec --output-format acp-daemon`). `npx`-launched entries (Qwen Code, Kilo, Cline, Auggie, Copilot CLI, Qoder, Codebuddy, Pi via `pi-acp`, Claude/Codex via their ACP adapters) are listed as available but off until the user enables them once, because enabling runs code fetched by `npx`. Claude Code and Codex keep their direct adapters (already proven) and are the default for them. Exact flags are verified against each installed agent's `--help` where installed; the rest keep the documented command and fail with the agent's own error.
4. **Custom agent**: name + command + args (ACP over stdio). No provider lists beyond the registry.
5. **No install instructions.** A missing agent shows "Not found" plus "Set path…" and a link to the agent's own docs (`docs_url`), nothing else.
6. **Agent picker becomes a panel**, not a dropdown: opened from the agent chip, sized like the chat column (overlay with shadow and a backdrop), sections Detected, Enable (npx), API endpoints, Custom, a search field, and per row: icon, name, status chip (Ready, Needs enable, Not found, Signed out if detectable), version, path. Clicking a row selects it for this chat.
7. **Settings → Chat** redesign with the same list (one row per agent with a details disclosure: path override, args override, enable), API endpoints as compact rows with an edit disclosure, consistent 16/24 px padding, labels aligned on one grid, the page scrolls instead of being cut off.
8. **Setup state** in the chat column: one card "Connect an agent" listing detected agents to pick from (usually already some), plus "Add an API endpoint" and "Add custom agent"; no install commands.
9. **Send bug**: reproduce, find the root cause, fix, and add a regression test; any start failure must show an error row in the thread (never a silent no-op) and keep the typed text.
10. **Multiple selections**: each `@` adds a chip (selection at that moment, labelled by layer names); chips stack, can be removed one by one; all are sent as context lines with node ids.

## Not in scope

* Installing agents.
* ACP terminal/file-system client methods beyond what PaperCraft needs (answer `fs/*` and `terminal/*` requests with "not supported" unless the agent requires them to work; most agents use their own file tools).

## Acceptance

* With the app launched from the Dock: Claude Code, Codex, Pi, OpenCode, Gemini, Goose and Cursor are detected on this machine.
* A turn with Gemini CLI, OpenCode, Goose and Cursor (each installed here) creates an artboard through PaperCraft's tools via ACP; a Pi turn works after enabling `pi-acp`.
* Stop cancels an ACP turn within 3 s.
* Picker panel, Settings → Chat and the setup state match new mock-ups at 1440p and 1280x800; nothing clipped; shadows on overlays.
* Sending a message always either starts a turn or shows an error row.
* Three `@` chips can be added and removed independently.
* Tests: PATH merge, registry detection with a fake PATH, ACP JSON-RPC handshake and update mapping against a fake agent process (recorded fixture), permission flow, cancel.
