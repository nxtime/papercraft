# Plan

1. Reproduce page-menu crash through egui input. Defer page deletion to confirmation shown after sidebar rows. Reuse egui Modal and existing theme controls.
2. Add optional manifest page_groups metadata and Editor group operations; Session loads/saves metadata. Implement independently with OpenCode.
3. Add page group headings and menu controls, consistent menu sizing; reset stale rename/drag state after deletion.
4. Centralize editor destructive confirmation UI; integrate mode/variable/token remove controls and home trash operations.
5. Inspect NotesCraft token source and live project; back up project and restore missing Catppuccin modes with app tools.
6. Run focused tests, inspect rendered UI, rebuild/reinstall for review.

Risks: page indices become stale during mutations, project switches while modal open, metadata migration compatibility, platform font glyph support, local storage capacity. Use stable IDs, additive defaulted metadata, painted icons, existing internal-disk Cargo target.
