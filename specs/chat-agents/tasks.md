# Tasks

Stop on failure. UI tasks go to Sonnet (or Codex Sol), never Haiku. Each agent cleans its temp homes, logs and drivers when done; one shared `target/`.

## Wave 1 (parallel)

- [x] 1. **Mock-ups** on page `Chat agents` (agent panel, setup, Settings / Chat, multiple selections). Sonnet.
- [x] 2. **PATH resolution, no install hints, send bug, multiple `@` chips** (spec items 1, 5, 9, 10). Sonnet.

## Wave 2 (after wave 1)

- [x] 3. **ACP backend** `src/chat/acp.rs`: JSON-RPC client, session/new with PaperCraft's MCP server (HTTP or the `papercraft-mcp` stdio bridge), prompt, update mapping, permission flow, cancel, load. Fake-agent fixture tests; real turns with Gemini CLI, OpenCode, Goose and Cursor (installed here). Sonnet.
- [x] 4. **Agent registry and custom agents** `src/chat/agents.rs`: data table, detection on the resolved PATH, npx enable state, custom agent entries in ChatPrefs, BackendKind/Backend for ACP agents. Codex Sol (Sonnet if unavailable).

## Wave 3 (after wave 2 and the mock-ups)

- [x] 5. **Agent panel and setup state** in the chat column (spec items 6, 8). Sonnet.
- [x] 6. **Settings → Chat redesign** (spec item 7). Sonnet.

## Wave 4

- [x] 7. **Review** (Codex Sol, read-only), fixes (Sonnet).
- [x] 8. **Docs** `docs/chat.md` (agents, ACP, custom), README. Haiku.
