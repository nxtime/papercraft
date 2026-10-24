# Tasks

Spec: `spec.md`. Stop on failure. One shared `target/`; never install or quit the user's PaperCraft (design agents are using it); test builds use their own `PAPERCRAFT_HOME` / `PAPERCRAFT_MCP_PORT` and are killed by PID. Each agent runs `cargo test --release` and `cargo clippy --release --all-targets` clean before reporting, and deletes its temp homes.

## Wave A (parallel, disjoint files)

- [x] A1. **MCP for agents**: P0.2 viewport (`get_basic_info`/`get_selection` viewport, `set_viewport`), P0.1 `get_screenshot {annotations}`, P0.6 CLI version check + live `list`/`schema`, `annotate {tidy}` returns `{tidied: n}` when no notes are given, `get_tree_summary` depth doc (P2 docs). Files: `src/mcp/exec.rs`, `src/mcp/tools.rs`, `src/io/png.rs`, `src/bin/*cli*`, docs. Tests.
- [x] A2. **Design-system context**: P0.4 digest (new module, cached on ChatState, cap const, system prompt block, CLI/ACP "[Design system changed]" prefix, Settings toggle, context meter) + `not-an-instance` lint rule in `src/mcp/lint.rs`. Tests.
- [x] A3. **Runs per client**: P0.3 (`src/runs.rs`, `src/mcp/server.rs` client name, agent view/run card labels, per-run revert guard, stale card after undo, Working pill clears, Stop not an error). Tests.

## Wave B (after A)

- [x] B1. **Actions menu**: P0.5 both lanes + toast, prompts table in `chat_msgs.rs`.
- [x] B2. **Pointing**: P1.1 Pick/area chips with crops, ⌥-click, P1.4 ⌘K Ask popover.
- [x] B3. **Presence**: P1.2 agent cursor + live outlines, P1.5 live counts in pill, P1.6 notes to prompt.

## Wave C

- [ ] C1. Review (read-only) against spec, fixes (not done yet); docs (done) (`docs/chat.md`, `docs/agents.md`, README tool count); install once design agents are done; live check.
