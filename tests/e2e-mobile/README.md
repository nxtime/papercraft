# Mobile end-to-end test

Drives the browser build at phone size (390x844, touch, 3x) with real touch events through
Playwright 1.64.0 (chromium build 1248, already cached; no browser download). The UI is an egui
canvas, so assertions use screenshots, pixel probes and the document state read over the relay's
MCP link (`/s/{token}/mcp`, tab token pre-seeded in localStorage). Screenshots go to
`D:\Projects\papercraft-wt\mobile-shots\e2e\` (`PAPERCRAFT_E2E_SHOTS` overrides).

## Prepare

```sh
# wasm-bindgen 0.2.129 lives in D:\Projects\papercraft-wt\tools\bin; put it on PATH, then build web + relay
# (through the gate): web/build.sh, then `cargo build --release --bin papercraft-relay`.
cd tests/e2e-mobile && npm install
```

`RELAY_BIN` (default `D:/Projects/papercraft-wt/target/release/papercraft-relay.exe`) and `WEB_DIR`
(default `<repo>/web`, needs `pkg/`) point at other builds. The relay runs on port 29982
(`PAPERCRAFT_E2E_PORT`), with a temp data dir; only its own PID is stopped.

## Browser

The web build is WebGPU-only. Playwright's default headless shell has no adapter ("No available
adapters"), so `playwright.config.mjs` uses new headless chromium (`channel: 'chromium'`) with
`--enable-unsafe-webgpu --enable-unsafe-swiftshader` (software Vulkan). Do not add
`--use-angle=swiftshader`: it breaks adapter discovery. The fixture also creates and opens a
project over MCP, because a fresh tab shows the home screen and `get_basic_info` fails there.

## Run

```sh
npm test                 # base: harness tests pass (1a, link, 4), phone-UI tests must fail (test.fail)
npm run test:phone       # after mobile-ui is merged: PAPERCRAFT_E2E_PHONE=1, everything must pass
```

Scenario 9 (Chat with a fake agent) is `test.skip` with a TODO until server-agent's chat frames
and test stand-in exist. Not part of CI.
