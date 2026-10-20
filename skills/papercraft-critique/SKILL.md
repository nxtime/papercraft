---
name: papercraft-critique
description: Pre-emit critique gate for work built in PaperCraft. Five panels (designer, critic, brand, accessibility, copy) score a screen 0-10 against PaperCraft's own objective evidence (lint_design, verify_code, get_screenshot, get_design_spec, get_variables), the weighted composite must reach 8.0, and a failed gate ships the best round instead of the last one. Use when an agent is about to hand over a PaperCraft screen and wants a tool-backed review instead of a self-praise.
---

# papercraft-critique

A gate, not a compliment. Before a screen leaves the app it goes through five panels. Each panel is a rubric pass over the same artifact, and each panel may only cite what a PaperCraft tool actually returned. The panels produce five scores, the scores produce one composite, and the composite decides whether the work ships. Mechanics live in the `papercraft` skill, taste in `papercraft-design`; this skill is the gate.

Reference: [panels.md](references/panels.md) (what each panel inspects, which tool backs it, its 0–10 bands).

## 1. When the gate runs

Run it when the resolver (section 2) says on, or when the request is a review of finished work:

* a whole screen is about to be handed over: landing page, dashboard, sign-in, settings, an onboarding flow, a whiteboard plan;
* a redesign of an existing screen, after the edits and a clean `lint_design`;
* the user asked "does this look right?", "review this", "critique it", "is it ready to ship?".

Do not run it for: a single component or icon, an exploratory draft before a direction is chosen (the `papercraft-design` section 0 step), or a one-line copy tweak. The gate costs a lint and a screenshot per round; spend it on work that is finished.

If the gate is on for a run, it must produce a status. Never skip it silently: report `Degraded` with the reason.

## 2. Enable resolver

Four levels, highest priority first. Take the first level that gives an explicit answer; "unset" falls through to the next one. A higher level saying *off* is final — a lower level cannot turn the gate back on.

| Order | Level | PaperCraft equivalent | How to read it |
|---|---|---|---|
| 1 | Per-task policy | the instruction the agent was given for this run | "run the critique gate before you finish", "review this screen" |
| 2 | Per-project setting | a toggle stored in the project's own metadata | **not implemented today** — PaperCraft has no project-level flag. Treat this level as unset and fall through to the environment flag or the per-task instruction |
| 3 | Environment flag | `PAPERCRAFT_CRITIQUE_ENABLED` | `1` / `true` / `always` = on, `0` / `false` / `never` = off, unset = fall through |
| 4 | Rollout phase | the release phase of the skill itself | off by default: the gate ships disabled until a run or a project turns it on |

So today the gate turns on in one of two ways: the user asks for it in the run's instructions, or `PAPERCRAFT_CRITIQUE_ENABLED=1` is in the environment. A future project setting — stored with the project, not with the user — would sit between those two and win over the environment, so a project could opt in permanently without an env var.

## 3. Order of operations

Objective checks first, subjective panels second. A panel never scores above evidence that contradicts it.

One round is: **evidence → repair → judge → score**.

1. **Snapshot.** Note the `pageId` and the artboard `nodeId` under review and keep them for the whole gate. A page switch between rounds invalidates the evidence.
2. **Evidence.** `lint_design` (`nodeId`, `pageId`, no `fix` yet) for the raw findings — the payload counts `errors`, `warnings` and `info`, groups them in `byRule`, and lists each issue with its `rule`, `severity`, `node`, `message` and `fix`; `get_screenshot` of the same node (`scale: 2` to read small text); `get_variables` for the token set and the active mode; `get_design_spec` (`nodeId`, `target`) when the design is headed for code. If there is code to check, `verify_code` (`nodeId`, `target`, `code`).
3. **Repair the hard evidence.** Fix every `lint_design` error and warning and re-run the lint until it is clean; fix every `verify_code` violation until `passed` is true. A panel cannot be scored on a design that still carries a lint error.
4. **Judge the five panels** on the evidence from steps 2–3 plus the screenshot. Every finding cites the tool result or the screenshot region behind it.
5. **Score.** Five scores 0–10, then the composite (section 5). Composite ≥ 8.0 → status `Shipped`, stop, `finish_working_on_nodes`.
6. **Otherwise fix and go again.** Take the findings in composite-impact order, fix them, run round N+1. Maximum 3 rounds.
7. **Ship the best round** if round 3 is still below the threshold (section 6).

The panels are rubric passes by the agent that holds the tools: one agent, five passes, five different rubrics. A runtime that can spawn subagents may run the panels in parallel, but the evidence vetoes, the composite and the round counter belong to the coordinator.

## 4. Hard evidence vetoes a panel

`lint_design` and `verify_code` are not suggestions. Each finding belongs to one panel, and that panel cannot score above the band its evidence forces.

| Evidence | Rule | Owner panel | Cap |
|---|---|---|---|
| `lint_design` error | `contrast` | Accessibility (the Critic reads it too) | 3.9 |
| `lint_design` warning | `text-size` (under 12px) | Accessibility | 6.9 |
| `lint_design` warning | `spacing-grid` (off the 4px grid) | Designer | 6.9 |
| `lint_design` warning | `touch-target` (under 44px) | Accessibility | 6.9 |
| `lint_design` warning | `layer-name` (default names) | Designer | 6.9 |
| `lint_design` warning | `hard-coded-color` | Brand | 6.9 |
| `lint_design` info | `type-scale` | Designer | none — must be listed |
| `verify_code` error | `missing-text` | Copy | 3.9 |
| `verify_code` error | `hardcoded-color` | Brand | 3.9 |
| `verify_code` error | `forbidden-absolute` | Designer | 3.9 |
| `verify_code` error | `missing-layout` | Designer | 3.9 |
| `verify_code` warning | `missing-component` | Brand | 6.9 |
| `verify_code` warning | `spacing-drift` | Designer | 6.9 |

An error caps the owning panel at 3.9 (Blocker band), a warning at 6.9 (Weak band), an `info` item caps nothing but must appear in the panel's notes. `hard-coded-color` arrives in both flavours: a warning when the color equals a variable that exists, an info when it is a literal used several times — the warning caps Brand, the info does not. The cap lifts as soon as the finding is gone; the next round re-scores the panel freely.

A finding is evidence for its owner and a hint for the others: `contrast` also informs the Critic, `spacing-grid` also informs the Brand (was it the token or the layout?), but only the owner is capped.

## 5. The panels and the composite

| Panel | Judges | Weight |
|---|---|---|
| Designer | layout, composition, hierarchy, spacing rhythm, type scale in use | **0 in v1** — it still scores and its findings still drive the next round |
| Critic | does it meet the brief; contrast, weight, readability | 0.4 |
| Brand | token compliance, on-brand color, voice | 0.2 |
| Accessibility | WCAG, focus rings, semantic structure, alt text | 0.2 |
| Copy | voice, tone, terseness, error-message quality | 0.2 |

```
composite = critic x 0.4 + brand x 0.2 + accessibility x 0.2 + copy x 0.2
```

The weights sum to 1.0, so the composite is also 0–10. The Designer carries weight 0 in v1 on purpose: it is the panel most likely to argue with itself, so its score is reported and its findings are acted on, but it does not move the composite.

Each panel scores 0–10 in half-point steps. The composite is the weighted sum, rounded to two decimals.

| Composite | Verdict |
|---|---|
| ≥ 8.0 | Ship |
| < 8.0 | Not shippable as is: fix and re-run, or ship the best round after round 3 |

The band legend is the same for every panel:

| Band | Range | Means |
|---|---|---|
| Blocker | 0.0–2.9 | Broken: the panel's own evidence is violated. Cannot ship |
| Poor | 3.0–4.9 | Works against the brief or the brand; ships visible damage |
| Weak | 5.0–6.9 | Passable but noticeably off; expect a second round |
| Acceptable | 7.0–7.9 | No blocker, but a reviewer would ask for one change |
| Good | 8.0–9.4 | Ships as is |
| Excellent | 9.5–10.0 | Reference quality for this project |

What each panel inspects, which tool backs it and where its scores land: [panels.md](references/panels.md).

## 6. Rounds and the fallback

* At most **3 rounds** per gate. The counter is per gate, not per panel.
* A round ends when the composite is computed.
* Rounds 2 and 3 exist to fix what the panels found. Fix in composite-impact order: the panel with the largest `weight x (10 - score)` first.
* **`fallbackPolicy: ship_best`.** If the third round is still below 8.0, the status is `Below threshold` and what ships is the best round, not the last one. Best = highest composite; a tie goes to the earlier round (fewer changes). A later round edited the canvas, so put the canvas back into the best round's state before handing over — undo back to it, or re-apply that round's fix list in order.
* Ship the best round's artifact and report the failing score. `Below threshold` is a status, not a licence to hide the number.

## 7. Status vocabulary

Every gate ends with exactly one status.

| Status | When | What ships |
|---|---|---|
| `Shipped` | composite ≥ 8.0 | the reviewed artifact, unchanged |
| `Below threshold` | still < 8.0 after round 3 | the best-scoring round's artifact (`ship_best`), with the score reported |
| `Timed out` | the gate ran out of its budget (rounds or minutes) before a verdict | the best round so far, with the timeout stated |
| `Interrupted` | the user or the session stopped the gate mid-run | the last complete round's artifact |
| `Degraded` | a panel could not be judged properly (section 8) | the artifact, but the verdict is provisional and the reason is named |

`Degraded` outranks `Shipped`: a degraded panel is capped at 6.9, so a degraded run reports `Degraded`, not `Shipped`, even when the composite clears 8.0.

## 8. Degraded reasons

A panel that cannot be judged on real evidence is not scored as if it were fine. It is capped at 6.9, the composite is computed with the cap, and the run reports `Degraded` with the reason and the panel it hit.

| Reason | Cause | Handling |
|---|---|---|
| `malformed_block` | a tool returned something the gate could not parse (lint payload in an unexpected shape, a truncated screenshot answer) | retry the tool once; if it still fails, judge the panel on the evidence you do have and name the missing part |
| `oversize_block` | the evidence was too big to read whole (a screenshot scaled down past legibility, a tree deeper than the context, a lint payload whose `truncated` flag is set) | narrow the scope: screenshot or lint a subtree, split the screen into regions and judge region by region |
| `adapter_unsupported` | the requested target or format is not supported here (a `verify_code` target the project does not build) | drop that check; the panel falls back to the screenshot plus the design rules |
| `protocol_version_mismatch` | the MCP server exposes a different tool set than the gate expects (a tool renamed, an argument changed) | stop using the broken tool, finish what you can, report `Degraded`; do not guess the schema |
| `missing_artifact` | the thing under review is gone (artboard deleted, page switched, stale node id) | re-snapshot with `get_basic_info` and restart the gate; if the artifact is truly gone, report `Degraded` |

Degradation is a report, not an excuse: name the reason, the panel and the evidence you lost.

## 9. What the gate reports

End the gate with the status, the numbers and the findings, in that order:

```
Critique gate - Sign in (page p1, artboard 3fa9c1:7)
Round 2 of 3 - composite 8.44 / threshold 8.00 - status Shipped
Designer 7.5 (weight 0) | Critic 8.5 | Brand 8.5 | Accessibility 8.5 | Copy 8.2
Composite = 8.5x0.4 + 8.5x0.2 + 8.5x0.2 + 8.2x0.2 = 8.44
```

Then, per finding: the panel, what was seen, the evidence behind it (tool and finding, or the region of the screenshot), the fix, and whether it was fixed in this round or is still open.

Machine-readable form, when the caller wants JSON:

```json
{
  "status": "Shipped",
  "threshold": 8.0,
  "composite": 8.44,
  "round": 2,
  "rounds": [{ "round": 1, "composite": 7.35 }, { "round": 2, "composite": 8.44 }],
  "scores": { "designer": 7.5, "critic": 8.5, "brand": 8.5, "accessibility": 8.5, "copy": 8.2 },
  "weights": { "designer": 0.0, "critic": 0.4, "brand": 0.2, "accessibility": 0.2, "copy": 0.2 },
  "degraded": [],
  "findings": [{ "panel": "critic", "evidence": "get_screenshot: headline at 16px over a photo", "severity": "warning", "state": "fixed" }]
}
```

## 10. Attribution

The five-panel critique contract — the roles, the weights, the composite, the 8.0 threshold, the three rounds and the `ship_best` fallback — is inspired by OpenDesign's Critique Theater (https://github.com/nexu-io/open-design, Apache-2.0). The contract is re-implemented here in PaperCraft's own words and against PaperCraft's own tools; no text was copied.
