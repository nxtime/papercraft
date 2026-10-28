# AI usage policy

PaperCraft is built to be driven by AI agents, and many of its contributors use them too. AI tools
are welcome here. What we ask is that a person stays accountable for everything submitted.

## For contributors

* **You own what you submit.** Read every line of your pull request before asking for review. If
  you cannot explain what a change does and how it fits the rest of the system without asking an
  AI, do not submit it yet.
* **Disclose meaningful AI help.** Say which tool you used and roughly how much it wrote in the
  pull request description (for example "drafted with Claude Code, reviewed and tested by me").
  Autocomplete and spell-checking do not need a note.
* **Write issues yourself.** An agent can help you research, but the report should be in your own
  words, trimmed to what matters, with reproduction steps you ran. Long generated text that nobody
  checked puts the burden on maintainers.
* **Think before you generate.** Agents jump to code and make their own job easy: trivial tests,
  wide or optional types, swallowed errors, redundant comments, copied local patterns that do not
  fit. Push back on all of it. [RULES.md](RULES.md) lists the standards to hold the output to.
* **Verify.** Run the checks in [CONTRIBUTING.md](CONTRIBUTING.md), use the app, and attach
  screenshots for UI changes. "The agent said it works" is not a test result.
* **Answer automated review.** If a bot reviews your pull request, fix its findings or reply with
  a one-line reason a finding does not apply.
* **No bulk or drive-by pull requests.** Many small generated changes across the tree, or changes
  to something nobody asked for, will be closed. Open an issue first for anything beyond a small
  fix.

Pull requests that clearly skip these steps may be closed without a detailed review. Repeated
low-effort submissions can lead to a block, under the [Code of Conduct](CODE_OF_CONDUCT.md).

## For coding agents

Read [AGENTS.md](AGENTS.md). It holds the commands, conventions and guardrails an agent working in
this repository should follow.

## Why

Every issue and pull request is read by a person. Review time is the scarce resource, and a change
nobody understood costs more than it saves. AI that is reviewed, tested and understood by the
person submitting it is a good way to build PaperCraft; AI used to skip that work is not.
