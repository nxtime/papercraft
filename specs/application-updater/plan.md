# Plan

Implement native updater module and regression tests independently with OpenCode. Integrate updater window/menu and worker messages in UI/native shell. Add local install script through separate OpenCode task. Verify current feed, fixtures for newer releases and unsafe URLs, native/wasm compile boundaries, and local install. Compare alternatives: self_update (Gitea and bundle replacement), release-hub (signed manifest/bundle deployment), existing reqwest + native installers (chosen to match published DMG/MSI artifacts without extra service).
