# Tasks: free agents

1. [x] Free models: `is_free`, `parse_pi`/`fetch_pi`, `free_order`. Unit tests in `models.rs`.
2. [x] Rotation: `fallbacks`, `is_retryable`, `Event::ModelSwitch`, retry loop. Fake-agent fixture
   test (first model rate limited, second completes; error after a tool is not retried).
3. [x] Provider keys: `chat/keys.rs` (keyring, env). Test env building with an injected getter.
4. [x] Lean Pi: `pure_env("pi")`, `write_pi_home`, keys in env, Pi catalog fetch. Tests for env and
   generated files.
5. [x] UI models: "Free (auto)" row, "Free only" toggle, fallbacks filled, failed cache, switch line.
   UI test: clicking Free (auto) sets `FREE_AUTO`.
6. [x] Install data and runner: `chat/install.rs` methods per OS, `Host` detection. Tests per fake
   host (macOS brew, macOS no Node, Ubuntu, Arch, Windows scoop).
7. [x] Install UI card + Pi key settings UI. UI tests: Install shows command before running; failure
   keeps command and docs link.
8. [x] README/ROADMAP notes, clippy, targeted tests.
