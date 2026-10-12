# Attribution

Bundled assets and where they come from. Ported code and dependency notices are in
[THIRD_PARTY.md](THIRD_PARTY.md).

| Path | Title | Author | Source | License |
|---|---|---|---|---|
| `assets/fonts/Inter-*.ttf`, `assets/fonts/OFL.txt` | Inter 4.1 (Regular, Medium, SemiBold, Bold, Italic, Bold Italic) | The Inter Project Authors | https://rsms.me/inter/ | SIL Open Font License 1.1 |
| `assets/app-icon/*` | PaperCraft app icon | PaperCraft contributors (original artwork; colors from the Catppuccin palette) | this repository | MIT OR Apache-2.0 |

## Palettes

The Catppuccin Latte, Frappé, Macchiato and Mocha themes use the colors of the
[Catppuccin](https://github.com/catppuccin/catppuccin) palette (MIT, Copyright (c) 2021 Catppuccin),
taken from the `craft-ui` crate of [craft-libs](https://github.com/storytold/craft-libs).

## Icon sets (not bundled)

`papercraft-cli icons add <set>` downloads open icon sets from the npm registry into the user's data
folder, each with its own license file: Lucide (ISC), Tabler (MIT), Phosphor
(MIT, https://github.com/phosphor-icons/core), Heroicons (MIT) and Material Symbols (Apache-2.0).
They are not part of this repository or of the release builds.
