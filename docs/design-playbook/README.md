# design-playbook

A design reference built for AI agents and humans: what makes an interface good,
accessible and not generic, with every rule traceable to a source, plus the skills
to design, implement and audit UI with it. Use it as the reference for building and
improving a UI library, on the web (HTML, CSS, React) or native (Rust, egui).

## What is in it

| Path | Contents |
|---|---|
| `principles/` | 10 documents, about 150 rules with stable ids: interaction intent (INT), perception (PSY), visual (VIS), motion (MOT), accessibility (A11Y), feedback and forms (FBK), design systems and tokens (SYS), anti-patterns (BAD), CSS craft (CSS), component patterns (PAT) |
| `platforms/` | How the rules map to web and to native egui |
| `checklists/` | Design review, WCAG 2.2 AA audit, pre-ship gate |
| `templates/` | Screen brief, component spec, DTCG-shaped token files (light, dark, base) with enforced contrast pairs |
| `sources/` | Source registry with trust levels, per-video notes, a reading list of blogs/magazines/design systems with read status, a list of unverified claims, and a research protocol |
| `skills/` | `playbook-design`, `playbook-implement`, `playbook-audit` |
| `cmd/playbook`, `internal/` | The `playbook` CLI (Go, standard library only): rule search, WCAG contrast and token-pair checks, skill installer. The corpus is embedded in the binary |
| `llms.txt` | Index for agents that fetch over HTTP |

## Using it

```bash
go build -o ~/.local/bin/playbook ./cmd/playbook   # one static binary, corpus embedded

playbook search "confirm vs undo delete"   # ranked sections
playbook rule A11Y-09                      # one rule in full (a prefix like MOT works)
playbook list                              # every rule id
playbook doc platforms/native-egui.md      # print a whole document (no PATH lists them)
playbook contrast '#767676' '#ffffff'      # 4.54:1, exit 1 below 4.5
playbook tokens                            # check the shipped light and dark token files
playbook tokens my.tokens.json             # check your own contrastPairs
playbook skills install                    # copy the skills into ~/.claude and ~/.agents
go test ./...                              # keep it honest
```

No dependencies beyond the Go toolchain to build. The binary reads its embedded
copy of the rules; set `PLAYBOOK_DIR` to a checkout to read live files instead
(useful while editing rules).

## How agents should reach it

Ordered by reliability. Details in `AGENTS.md`.

1. **Skills** (primary). `playbook-design`, `playbook-implement`, `playbook-audit`
   trigger on UI work and tell the agent to look rules up here. Procedural,
   exact thresholds, no retrieval noise.
2. **The `playbook` CLI**. Any agent with a shell can search the corpus by
   keyword or rule id with no index to maintain and no checkout to locate.
3. **`llms.txt` over HTTP** from the raw file URL of wherever this repo is hosted,
   for agents without filesystem access.
4. **RAG pointer only**, if you run a RAG. A short note tells agents this repo
   exists and when to use it. Do not index the rules themselves: they are a
   versioned reference with exact numbers, and retrieval chunks lose context.
5. **Live research** when the playbook has no answer: follow the protocol in
   `sources/SOURCES.md`, then write the finding back as a rule.

## Rule conventions

- Rule heading: `### A11Y-10 Title`. Ids never change meaning; deprecated rules
  stay with a note. Cross-references are tested (`playbook_test.go`).
- A rule cites sources (`S..` standards and articles, `V..` videos). A rule with no
  citation is general knowledge (`GK`): a default to test, not a requirement.
  See `sources/UNVERIFIED.md` for claims that could not be confirmed.
- Videos are summarized in our own words; transcripts are not stored.
- No emoji anywhere; icons are SVG with `currentColor`.

## Status and honest limits

Built 2026-10-21 from 7 video transcripts (3 read in full, 4 partially) and about
35 primary or expert sources (including 8 Josh Comeau articles and 3 Inclusive Components case studies), fetched through a summarizing tool. Not yet read:
Apple HIG, Material 3, GOV.UK, the other 8 Inclusive Components studies, three downloaded
transcripts, and three videos with no retrievable captions (`sources/SOURCES.md`).
The egui guidance is partly unverified against real egui code. Next step: audit
your own library with `playbook-audit`.

## Contributing a rule

1. Fetch a primary source. 2. Add the rule to the right `principles/` file with
the next free id. 3. Add the source row. 4. If numeric and testable, put it in
tokens or a script. 5. `go test ./...`.

## License

MIT, see `LICENSE`. Third-party sources are summarized and linked, not copied; their content stays under their own terms.
