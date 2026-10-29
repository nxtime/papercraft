# Plan: free agents

## Facts checked (2026-10-29)

* `pi-acp` 0.0.34 runs `pi --mode rpc` (or `PI_ACP_PI_COMMAND`). It honours `PI_CODING_AGENT_DIR`
  and offers models through ACP `configOptions` / `availableModels` with `setSessionConfigOption`
  and `setSessionModel`, so the existing ACP model code works for Pi unchanged.
* `pi-mcp-adapter` 2.11 reads `$PI_CODING_AGENT_DIR/mcp.json` (`mcpServers`, `settings`).
  `settings.requestTimeoutMs` sets the call timeout. A server with `"url"` uses streamable HTTP.
  `"directTools": true` registers its tools one by one instead of behind the `mcp` proxy tool.
  It also reads `~/.config/mcp/mcp.json` and the project's `.mcp.json` / `.pi/mcp.json`.
* `pi --list-models` prints `provider model context max-out thinking images` for providers that
  have a key. It has no price column.
* OpenCode's free models are `opencode/*-free` and `opencode/big-pickle`, and cost 0 in
  `opencode models --verbose`.

## Design

* **Free models** (`src/chat/models.rs`):
  * `AgentModel::is_free()`: cost 0, or an id ending in `:free` (OpenRouter).
  * Pi's free-tier providers (Groq, Cerebras, NVIDIA NIM, Gemini) count as free when their key is
    set. `ponytail:` a provider-level guess, because Pi lists no prices.
  * `parse_pi` / `fetch_pi` read `pi --list-models`.
  * `free_order(catalog, skip)` ranks by context size, then image input.
* **Rotation** (`src/chat/acp.rs`):
  * `TurnConfig.fallbacks: Vec<String>` holds the next models to try, filled by the UI when the
    chat's model is `FREE_AUTO`.
  * A `session/prompt` error that `is_retryable` before any tool call switches the model
    (`pick_model`) and resends, at most `MAX_SWITCHES`.
  * `Event::ModelSwitch { from, to, reason }` becomes one line in the thread, and the UI marks
    `from` as failed for 15 min.
* **Lean Pi** (`src/chat/agents.rs`):
  * `pure_env("pi", home, ..)` sets `PI_CODING_AGENT_DIR=<home>/agent`.
  * `write_pi_home(home, mcp_url, model)` writes `settings.json` (`packages:
    ["npm:pi-mcp-adapter"]`, default provider/model) and `mcp.json` atomically.
  * Keys are added to the env from the keyring.
  * The UI calls it right before starting the turn, because the URL carries the chat id.
* **Keys** (`src/chat/keys.rs`):
  * `PROVIDERS` lists, per provider: id, name, env var and key page URL.
  * `get` / `set` / `forget` go through `keyring` (service "PaperCraft", user `pi:<id>`).
  * `env()` returns the env pairs for the providers that have a key.
* **Install** (`src/chat/install.rs`):
  * `Method { os, label, program, args, needs, kind: Package | Script, elevated }` holds the
    methods per agent, as data.
  * `Host { os, distro, has(program), node_major }` is detected once.
  * `methods(agent, host)` returns the methods that apply, in preference order.
  * `run(method) -> Receiver<Line>` streams output and runs nothing elevated.
* **UI**:
  * `chat_models.rs`: a "Free (auto)" row and a "Free only" toggle, on for opencode and pi.
  * `chat_agents.rs`: Install on missing OpenCode/Pi rows. The card holds the method menu, the
    command preview, Confirm, streamed output, the copy command and the docs link.
  * `settings.rs`: a Pi provider key list.
  * `chat.rs`: filling in the fallbacks, tracking failed models, and the switch line.

## Risks

* Retryable error text is a guess. `is_retryable` matches `429`, `rate limit`, `quota`,
  `overloaded`, `unavailable`, `not found`, `insufficient`, and is tested on fixtures. An error
  it doesn't recognise ends the turn, as today.
* `pi --list-models` output format may change. The parser skips lines it can't read.
* `chat.rs` and `chat_agents.rs` are already over 800 lines. New UI goes in new files
  (`chat_install.rs`, `chat_keys.rs`).
