# Page management and destructive-action safety

Fix page deletion during UI iteration. Page deletion requires confirmation using stable page IDs; cancelling or dismissing makes no changes, and the last page remains protected. Page menus have consistent width and separate destructive commands.

Add single-level named page groups/folders: create, rename, collapse/expand, move page into/out of group, remove group while preserving its pages. Persist group membership/order through project save/reopen; old projects load without groups. Added/duplicated pages inherit source group.

Replace missing Unicode remove glyphs for modes, variables and tokens with existing painted Trash icon. Confirm removal of pages, groups, modes, variables, tokens, projects and permanent trash operations. Ordinary undoable canvas edits keep current workflow.

Restore four Catppuccin modes in existing NotesCraft design project from original token sources, preserving existing token names, unrelated tokens and canvas content. Back up affected project before applying; use app tools and save.

Acceptance: UI regression reproduces deletion crash before fix and passes after; cancellation keeps data; stale dialogs cannot delete another page/project; group roundtrip and legacy loading pass; vector icons render; screenshot inspection covers page menu, groups and confirmation. Build, focused tests, formatting and Clippy complete. No commits.

Verification found color-variable transactions could reflow existing frames. Variable-only edits now retain undo/history without structural layout fixup. NotesCraft geometry was restored from backup and live mode switching verified unchanged nodes.
