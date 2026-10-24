# Gaps found while driving PaperCraft on the Interview project (2026-10-21..23)

Observed by the orchestrating agent and its subagents through MCP / papercraft-cli on the user's Mac.

## MCP / agent side
1. `get_screenshot` never draws annotations, so no agent can verify note placement; every check needed a macOS screen capture of the live window.
2. No tool to switch the page the user is viewing or to set the viewport (zoom / pan / fit a rect); `select_nodes {zoom:true}` only works on the active page and caps zoom at 1.0.
3. Agents can't see what the user sees (viewport rect, zoom, selection on screen) except `get_selection`.
4. `~/.local/bin/papercraft-cli` was stale (schema/list missing new tools) while `call` reached the app — CLI and app versions drift with no warning.
5. Applying a code change to the running app needs quit + reinstall + reopen (no dev reload / no "restart and reopen project" path).
6. Long batched edits (thousands of bindings, renames) run as many separate calls; no progress shown to the user in chat mode for CLI-driven agents beyond the "Working…" pill.
7. Cleanup agents made destructive mistakes (deleted button copies, lost icons on LG buttons) with no per-agent review/diff of what changed; undo is global, not per agent run.
8. Tidy/placement was blind to zoom-dependent card size (fixed), to screens inside sections (fixed), and lines detoured inside screens (fixed).
9. `get_tree_summary` depth semantics surprised scripts (rows under "Screens" needed depth 3).

## App / 0.6.2 follow-ups already noted
- Run card stale after undo; lingering "Working" pill; Stop reported as error.
- No-GPU panic should be a readable error; Wayland clipboard without DISPLAY.
- Email truncation (no text-overflow ellipsis).
- 13.7 s open once after install (first run, not reproduced).
10. `find_nodes` text search returned nothing for strings that exist on the page (Candidate redesign agent, 2026-10-23) — reproduce and fix.
11. `get_tree_summary` truncates long text to ~40 chars with no option for full text, so copy review across many screens is impossible without per-node reads.
12. `annotate {tidy:true}` with no notes returns `[]`, which agents read as failure (fix in A1: `{tidied: n}`).
13. When the app stops mid-run the CLI silently falls back to offline writes; several agents then write the same project files concurrently with no lock (lost-update risk). Also a CLI call during app start-up reports "app not running" and goes offline.
