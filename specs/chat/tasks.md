# Tasks

Stop on failure. Tasks in one wave touch different files and run in parallel; a wave starts when the previous one passes `cargo test --lib`. A read-only review after waves 2 and 4; real findings are fixed before moving on.

## Wave 0: design (parallel with wave 1)

- [x] 0. **Mock-ups** in the `PaperCraft` design file, new page `Chat`: `Chat / Empty`, `Chat / Streaming` (text + tool cards + thinking row), `Chat / Run finished` (run card, canvas overlay), `Chat / Agent picker` (menu open, CLIs found/missing, API), `Chat / Setup` (no CLI, no key), `Settings / Chat`, at 2560x1440 and a 1280x800 variant of `Chat / Streaming`. The user's app had another project open, so this ran through the 0.5.0 offline CLI on the `PaperCraft` project (each call refuses if the app has it open); never `open_file` in the user's app.

## Wave 1: model and storage (parallel)

- [x] 1. **Chat model and storage** (new `src/chat/mod.rs`, `src/chat/store.rs`): types from the spec, `chats/` load/save/list/delete, legacy project without `chats/`. Tests: round trip, listing order, delete.
- [x] 2. **Event stream and adapters' parsing** (new `src/chat/events.rs`, `src/chat/claude_code.rs`, `src/chat/codex.rs`, `src/chat/anthropic.rs` parsing half): the internal `Event` enum and pure parsers from recorded fixtures (`tests/fixtures/chat/`). Record real fixtures by running the CLIs once on a scratch project with a throwaway prompt (no user data); for the API, build SSE fixtures from the documented event shapes. Tests per backend.

## Wave 2: backends (after wave 1; 3, 4, 5 parallel)

- [x] 3. **Claude Code process** (`src/chat/claude_code.rs`): detect, spawn with MCP config and allowed tools, stream stdin/stdout on a worker thread, resume, stop. Integration test gated on `claude` being on PATH (skipped otherwise).
- [x] 4. **Codex process** (`src/chat/codex.rs`): same for `codex exec --json`. Gated test.
- [x] 5. **Anthropic API loop** (`src/chat/anthropic.rs`, `keyring` dependency): key storage, request building (tools from `tool_list()`, caching, effort, thinking display, fallbacks), SSE streaming, tool loop through a channel to the UI thread (`mcp::exec::call`), usage and cost. Test with a fake HTTP server.

## Wave 3: UI (after wave 2 and the mock-ups; 6 then 7 and 8 parallel)

- [x] 6. **Chat layout and mode switch** (`src/ui/chat.rs`, `src/ui/mod.rs`): Design | Chat toggle, ⌘J, column + canvas layout, resizable column, remembered per project.
- [x] 7. **Messages and composer** (`src/ui/chat.rs`): markdown text, thinking row, tool cards, run card (reuse run review actions), errors with Retry, composer with attachments, Stop, empty state suggestions, agent picker, chat list. Canvas follows tool targets.
- [x] 8. **Settings → Chat** (`src/ui/settings.rs`): CLI detection and paths, API key (Keychain), default backend/model/effort.

## Wave 4: finish

- [x] 9. **End-to-end check** with each available backend on a scratch copy of a project (own app instance on its own port and `PAPERCRAFT_HOME`), screenshots against the mock-ups, key-leak grep.
- [x] 10. **Docs and release notes**: `docs/chat.md`, README section and screenshot, `docs/agents.md`, ROADMAP row 44 done, PROGRESS entry; full test suite, wasm and Windows checks.

## Model per task

Claude Sonnet: 0, 2, 3, 5, 6, 7, 9. Claude Haiku: 1, 8, 10. Codex `gpt-6.1-sol`: 4 and the reviews (Claude Sonnet if Codex is unavailable).
