# Free agents: OpenCode and Pi set up for free models, with an install wizard

## Why (user request, 2026-10-29)

* The user wants to run PaperCraft chats on alternative agents (OpenCode, Pi; later Codex and
  Antigravity) without paying for a model.
* OpenCode ships free models (`opencode/*-free`, `opencode/big-pickle`) that need no key, but they
  are "free for a limited time" and change; a chat pinned to one that disappears or is rate
  limited fails.
* Pi is detected but useless in a chat: `pi-acp` accepts the MCP servers PaperCraft sends and
  never passes them to Pi, so Pi has no PaperCraft tools. Pi also loads the user's whole setup
  (14 packages, ~190 skills), like OpenCode did before lean mode.
* Pi supports OpenRouter (`:free` models), Groq, Gemini, NVIDIA NIM and Cerebras through
  `*_API_KEY` env vars; the user has no key flow for them.
* A missing agent shows "Not found" and a docs link only. The user now wants an install wizard.
  This reverses "No install instructions" in `specs/chat-agents/spec.md` for OpenCode and Pi.

## Decisions (asked 2026-10-29)

* Pi reaches PaperCraft through `pi-mcp-adapter` installed in an isolated Pi dir.
* Provider keys live in the OS secret store through the `keyring` crate, as `src/chat/endpoint.rs`
  does: Keychain on macOS, Credential Manager on Windows, Secret Service on Linux. They reach Pi
  only as env vars of its process.
* Pi providers: OpenRouter plus Groq, Gemini, NVIDIA NIM and Cerebras, each used when its key is
  set.
* The wizard installs on a click after a confirmation, and falls back to a copyable command and
  the docs link.

## Scope

1. **Free model set.** A model is free when the catalog gives it input and output cost 0
   (`opencode models --verbose`; Pi: `pi --list-models` plus OpenRouter's `:free` suffix). The
   model picker gets a "Free only" filter, on by default for OpenCode and Pi, and a "Free (auto)"
   row that means "rotate through free models".
2. **Rotation.** With "Free (auto)", a turn starts on the first free model in a ranked order
   (context size, then image input, then the last one that worked). When the turn fails with a
   rate-limit, quota, overload or unknown-model error before any tool ran, PaperCraft switches the
   session to the next free model (`session/set_config_option`, else `session/set_model`) and
   resends the prompt once per model, at most 3 models. A model that failed is skipped for 15 min.
   The thread shows one line per switch ("Switched to GLM-5.3 Flash Free: Big Pickle is rate
   limited"). Errors after a tool ran are not retried (the design may have changed).
3. **Lean Pi.** `PI_CODING_AGENT_DIR=<config_dir>/agent-home/pi/agent` with a generated
   `settings.json` (packages: `npm:pi-mcp-adapter` only, the chosen default provider and model)
   and the adapter's MCP config pointing at PaperCraft's URL with `tools=core`, MCP timeout above
   `CALL_TIMEOUT` like OpenCode's. `pi-acp` is launched with that env. Non-lean keeps the user's
   own `~/.pi`.
4. **Provider keys.** Settings › Agents › Pi lists OpenRouter, Groq, Gemini, NVIDIA NIM and
   Cerebras: a key field (masked, "Get a free key" link to the provider's key page), saved to the
   OS secret store, removed with "Forget". Keys are passed as `OPENROUTER_API_KEY` etc. and never
   written to disk or logs. The free model set for Pi is the union over providers with a key.
5. **Install wizard, per OS.** When OpenCode or Pi is not found, its row and the setup card show
   "Install". The wizard detects the OS (and on Linux the distribution, from `/etc/os-release`)
   and which package managers are on the resolved PATH, then offers the methods that apply, the
   first one preselected and the others in a menu. The methods come from each agent's install docs
   (checked 2026-10-29), as data in the agent registry:

   | Agent | macOS | Linux | Windows |
   |---|---|---|---|
   | OpenCode | `brew install anomalyco/tap/opencode`, `npm install -g opencode-ai`, `curl -fsSL https://opencode.ai/install \| bash` | Homebrew tap, `npm install -g opencode-ai`, Arch `paru -S opencode-bin`, install script | `scoop install opencode`, `choco install opencode`, `npm install -g opencode-ai` |
   | Pi | `npm install -g --ignore-scripts @earendil-works/pi-coding-agent`, `brew install pi-coding-agent`, `curl -fsSL https://pi.dev/install.sh \| sh` | npm, install script, `nix profile add github:earendil-works/pi/stable` | npm, `powershell -c "irm https://pi.dev/install.ps1 \| iex"` |

   * OpenCode's docs say plain `brew install opencode` and Arch's `pacman -S opencode` install
     OpenCode 2. PaperCraft's ACP and model code is tested on 1.x, so the wizard offers the 1.x
     packages until 2 is tested. A detected 2.x is still used, with a warning.
   * The npm methods need Node (Pi: 22.19 or newer). When `node` is missing or too old, npm
     methods are hidden and the card says so, with a link to nodejs.org. Pi's install scripts
     install Node themselves.
   * Script methods (`curl … | sh`, `irm … | iex`) run remote code. They are listed last and the
     confirmation shows the full command and URL.
   * On Windows, the commands run through PowerShell. `npx` resolves to `npx.cmd`. The agent home
     goes under `%APPDATA%` through `config_dir()`.
   * The click shows exactly what will run, and runs it only after "Install" is confirmed. Output
     streams into the card, then the agent is detected again.
   * Nothing runs elevated. When a method needs `sudo` or an admin shell, the card shows the
     command to copy instead of running it.
   * On failure, the card keeps the output, the copyable command and the docs link.
   * After install, Pi asks for at least one provider key. OpenCode needs nothing.
   * `pi-acp` and `pi-mcp-adapter` are fetched on first use.
6. **Usage and price.** Free models show "Free" in the chip; context use comes from
   `usage_update` (Pi sends it each turn) or the catalog's context size.

## Not in scope

* Codex and Antigravity (separate spec; Codex lean mode, Antigravity after checking its CLI).
* Paid-model rotation, provider sign-up flows, `pi-freerouter`.
* Installing Node or Homebrew themselves (the wizard says what is missing and links to them).

## Risks

* The free lists and their limits change without notice; rotation reads the live catalog at the
  start of each chat, never a hard-coded list.
* OpenRouter `:free` allows 50 requests a day without credits, and one design turn makes many.
  The picker says so next to OpenRouter.
* `pi-mcp-adapter` and `pi-acp` are third-party npm code fetched on first use; enabling Pi
  already requires a confirmation, and the wizard repeats it.
* The error text OpenCode and Pi send for rate limits is not documented; classification is
  tested against recorded fixtures and falls back to "not retried".

## Acceptance

* A new OpenCode chat with "Free (auto)" builds an artboard on a free model; with the first model
  forced to fail (fixture), the turn switches once and completes, and the thread shows the switch.
* A lean Pi chat with an OpenRouter key lists only free models, calls PaperCraft tools through
  `pi-mcp-adapter`, and loads none of the user's `~/.pi` packages or skills.
* No key appears in any file under the config dir, the agent home or the logs.
* With OpenCode missing from PATH (test PATH), the card offers Install. The install command is
  shown before it runs. A failure keeps the command and docs link.
* The offered methods match a fake OS, distribution and PATH: macOS with brew, macOS without
  Node, Ubuntu, Arch, and Windows with scoop.
* Keys round-trip through `keyring` on macOS here. Windows is covered by the CI run.
* Tests: free filter and ranking, error classification (recorded fixtures), rotation with a fake
  ACP agent, Pi lean env and generated settings, key env passing (fake keyring), wizard state, method selection per OS.
