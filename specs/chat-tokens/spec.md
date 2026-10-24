# Chat: fewer tokens, model and effort, image zoom

## Why

A Claude Code chat turn in the Interview project cost $0.56 (API-equivalent). Most of it was
a 50k-token prompt prefix written to the cache and re-read on every tool step. Measured with
`claude -p --model haiku` and the chat's own flags, before anything the user typed:

| Claude Code flags | Prompt tokens |
|---|---|
| what the chat sent | 42.7k |
| `--disable-slash-commands` (no user skills list) | 31.4k |
| `--setting-sources project,local` (no user CLAUDE.md, hooks, plugins) | 30.6k |
| both + `--tools Read,TodoWrite,ToolSearch` | 20.6k |

`--tools` without `ToolSearch` makes it worse (43.9k): the CLI then loads every PaperCraft
tool schema up front. Bash, Edit, Write and the web tools were never usable anyway: in `-p`
mode only `mcp__papercraft__*` is allowed, everything that needs permission is denied.

The API backend sends all 70 tool schemas (about 77 KB, ~19k tokens) on every request.

## What

1. **Lean Claude Code sessions** (Settings → Chat, on by default): the flags above.
   Off keeps the user's own CLAUDE.md, hooks, plugins and skills.
2. **API tool search**: on `api.anthropic.com`, the tools a design turn rarely needs are sent
   with `defer_loading` behind the BM25 tool-search tool; the core set stays loaded.
3. **Ask before building**: the chat system prompt tells the agent to ask up to three short
   questions (with proposed defaults) when a request is too broad to build well, and to
   avoid screenshots it does not need.
4. **Model and effort for Claude Code**: the agent panel's Model/Effort block, today only for
   Claude API endpoints, also shows for Claude Code. Claude Code no longer gets the default
   API endpoint's model forced on it (a non-Claude default endpoint sent its model id).
5. **Images: smaller to send, zoom for detail**: attachments are sent at most 1568 px on the
   long edge (about 1.15 MP, at most ~1.6k image tokens instead of ~4.8k). When an image was
   scaled down, the original is saved next to the sent one; the `zoom_image` tool returns a
   region of it at full resolution. It reads only files under the project's chat folder.

## Acceptance

* Claude Code args contain the lean flags when the setting is on, and none of them when off.
* API request: core tools have no `defer_loading`, the others do, and the tool-search tool is
  present; non-Claude hosts get the plain full list.
* A Claude Code chat with a picked model/effort passes `--model`/`--effort`; without a pick it
  passes neither.
* A 4000×1000 PNG is sent as 1568×392 and its original is kept; `zoom_image` on the sent path
  returns a crop of the original; a path outside the chat folder is refused.

## Follow-up: brief, design system, finish (after the binder test)

The binder page (Claude Code, lean) built straight away at 1440x1024 with 44px buttons
everywhere, no components for the 9 repeated slots, and a 350-word report. The 62.8k context
was a 20.3k prefix (lean, as measured) plus the agent's own 19.3k tokens of HTML and the tool
results; each part was written to the cache once. Lean mode also dropped the user's
`"model": "opus"` from `~/.claude/settings.json`, so the turn ran on Sonnet 5.5.

6. **`ask_user` tool + question card**: one to five questions, two to five options, palette
   options with swatches; the chat draws clickable chips, "Send answers" and "Use your
   defaults". The tool only checks the shape and tells the agent to end its turn.
7. **Chat rules**: a brief before the first design (frame size, density, palette, type, scope,
   references), variables and components first, sizes from the agreed density, a five-bullet
   report and next steps through `ask_user`.
8. **Lint**: `touch-target` asks 24px (WCAG 2.2 AA) inside screens 1024px or wider, 44px on
   narrower screens and standalone components; the autofix follows.
9. **Lean keeps the user's model**: with no pick, the chat passes the `model` from the user's
   Claude Code settings.

## Follow-up: long chats (the Jira chat at 230k)

The Jira chat reached 230k of context; every step re-read it. Claude Code's `total_cost_usd`
adds up over a resumed session, so turns showed the running total ($6.72 for a $0.47 turn).

10. **Meter**: a 1M window (current Claude models), counts as `1.6M`; amber past 150k.
11. **Per-turn cost**: `Message::session_cost` keeps the CLI's running total; a resumed turn shows
    the difference (older saved turns are derived at display time).
12. **Fresh session past 150k (now 120k)** (CLI backends): not resumed; the new session starts with a recap
    (the last 8 requests, the last report) and reads the canvas with tools.
13. **Compact now**: clicking the meter queues the same fresh start for the next message.
14. **API context editing**: `clear_tool_uses_20250919` past 60k input, keep 4, clear ≥15k, tool
    inputs too (beta `context-management-2025-06-27`).
15. **Research sub-agent**: with Web on, Claude Code gets a `researcher` agent on Haiku
    (`--agents`); it searches and fetches, the main context gets the kept paths only. A research
    turn on a fresh chat: $0.26 in all, main context 25.8k (the late in-session one cost $1.71).
    Sub-agent text stays out of the chat; its tool calls (and galleries) show.
16. **Turn budget** (Settings → Chat, default 10M tokens read): a turn past it is stopped like
    Stop, with a note.
