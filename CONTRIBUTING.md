# Contributing to PaperCraft

Thanks for wanting to help. PaperCraft is an alpha, so there is plenty to do and the bar for a
first contribution is low: a typo fix, a clearer error message, a missing test or a one-line bug
fix are all welcome.

Please read [RULES.md](RULES.md) before writing code: it holds the standards a change is judged by.
By taking part you agree to the [Code of Conduct](CODE_OF_CONDUCT.md). Using an AI tool is fine;
the [AI usage policy](AI_POLICY.md) says what you stay responsible for, and coding agents should read
[AGENTS.md](AGENTS.md).

## Ways to help

* **Report a bug** or **suggest a feature** with an [issue](https://github.com/nxtime/papercraft/issues/new/choose).
  A screenshot or a short recording helps; so does the project folder (or a page of it) that
  triggers the bug.
* **Pick something from [ROADMAP.md](ROADMAP.md).** Rows marked `todo` or `partial` are open. Say
  which one you are taking in an issue first, so two people do not build the same thing.
* **Improve the docs, skills or screenshots.** [docs/](docs), [skills/](skills) and the README are
  as important as code.
* **Review a pull request.** A second pair of eyes is always useful.

Security problems do not go in public issues: see [SECURITY.md](SECURITY.md).

## Before you start

For anything bigger than a small fix, open an issue (or comment on one) describing the problem and
how you plan to solve it. Large features start as a short spec in [specs/](specs): a `spec.md`
(why, what, what is out of scope) and a `plan.md`. [specs/component-props](specs/component-props)
shows the shape. A spec agreed up front saves a rewrite later.

## Set up

You need Rust 1.95 or newer (`rust-version` in `Cargo.toml`) and the platform libraries listed in
the README's [Build from source](README.md#build-from-source).

```sh
git clone https://github.com/nxtime/papercraft
cd papercraft
cargo build --release
```

Run a test copy next to your own PaperCraft so it cannot touch your real projects or collide with
the running app's MCP port:

```sh
PAPERCRAFT_HOME=/tmp/pc PAPERCRAFT_MCP_PORT=29998 ./target/release/papercraft
```

`PAPERCRAFT_HOME` moves the project root and the config folder; `PAPERCRAFT_MCP_PORT` moves the
MCP server. Always set both when developing.

The code depends on [`craft-ui`](https://git.marcospaulo.dev.br/masi/craft-libs), which Cargo
fetches from a public git repository at the revision pinned in `Cargo.toml`.

## Make the change

1. Fork the repository and branch from `main` (`fix/short-name`, `feat/short-name`).
2. **Write the test first.** Fix a bug by adding a test that fails, then make it pass. See
   [Tests](#tests).
3. Keep the change focused. One concern per pull request; unrelated cleanups go in their own.
4. Run the checks below and fix what they report.
5. Update the docs the change touches: the README, `docs/`, the tool description for an MCP tool,
   the skills if agents should now behave differently.
6. Add a short entry to [PROGRESS.md](PROGRESS.md) for user-visible changes.

### Checks

```sh
cargo fmt --all
cargo clippy --all-targets
cargo test --no-fail-fast
cargo check --lib --target wasm32-unknown-unknown     # the web build
```

Keep the tree free of the third-party branding patterns listed in `.debrand`.

### Tests

* Unit tests sit next to the code in `#[cfg(test)]` modules.
* Scenario tests live in [tests/](tests). They drive the editor with synthetic pointer and
  keyboard events; `tests/mcp.rs` covers every MCP tool on a headless session.
* Aim for **80% coverage** on code you add or change, including the error paths.
* GPU-dependent tests skip when no adapter exists. Set `PAPERCRAFT_REQUIRE_GPU=1` to make them
  fail instead.
* `tests/update.rs` cannot run on Windows (it needs elevation); that failure is known and not
  yours.
* For UI work, run the app, look at it, and attach a screenshot. Headless rendering needs no
  display: `papercraft --screenshot ui.png --demo --size 1440x900 --scale 2`.

## Commits

[Conventional commits](https://www.conventionalcommits.org): `type(scope): summary`, with the
types `feat`, `fix`, `refactor`, `docs`, `test`, `chore`, `perf` and `ci`.

```
fix(ui): keep the review row's revert button off the hover edge
feat(mcp): add set_screens
docs(readme): add the demo video
```

Write the summary in the imperative, under about 72 characters, and use the body to say *why*.
Reference the issue (`Fixes #123`).

## Pull requests

* Fill in the template: what changed, why, how you tested it, screenshots for UI changes.
* Keep it reviewable. If it grows past a few hundred lines of non-generated code, split it.
* The branch must be up to date with `main`; rebase rather than merge.
* Expect review comments; they are about the code, not you. Push fixes as new commits and we
  squash on merge.
* A maintainer merges once a review is approved.

## Releases

Maintainers cut releases. A `v*` tag pushed to GitHub builds Windows, macOS and Linux and
publishes them (`.github/workflows/release.yml`). Contributors do not need to bump versions.

## Licensing

PaperCraft is dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE). Unless you
state otherwise, anything you submit is licensed under the same terms, with no extra conditions.
Only submit work you wrote or have the right to submit. Code ported from another project needs a
compatible license and an entry in [THIRD_PARTY.md](THIRD_PARTY.md); bundled assets go in
[ATTRIBUTION.md](ATTRIBUTION.md).
