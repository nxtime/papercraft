# Application updater and local installation

Use existing Gitea server: GET https://git.marcospaulo.dev.br/api/v1/repos/masi/papercraft/releases/latest. Show native in-app Check for updates from app menu and home/editor UI. Check in background without freezing GUI; report current/latest version, release notes, up-to-date, network failure, missing installer, and retry. Compare semantic versions; ignore drafts/prereleases. No startup network request unless user checks.

Select only exact desktop platform/architecture installer names: macOS .dmg (native arch then universal), Windows .msi (x64/arm64/x86 mapping). Linux/WSL missing assets get explicit release-page/manual-install path. Never choose CLI/relay/web archives. Validate HTTPS same-server repository download URLs; refuse redirects to other hosts, cap metadata/download bytes, enforce timeouts. Download only after explicit click, show progress/status, then open installer using OS handler. Do not replace running binary silently or auto-install releases during development.

Provide tools/install-local.sh: one command builds current checkout and installs/launches macOS app. Use existing plist/icon/entitlements, sign full bundle, preserve prior app as recoverable backup, verify before swap, gracefully save/quit existing app, support CARGO_TARGET_DIR and --no-launch. Local command must not require release publication.

Acceptance: parser/version/asset selection/host rejection tests; worker remains off GUI thread; offline error/retry supported; real Gitea feed checked read-only; UI smoke and local install pass. No release publishing or commits.
