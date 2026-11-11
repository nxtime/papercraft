#!/bin/sh
# Build the browser app into web/pkg. Needs: rustup target add wasm32-unknown-unknown,
# cargo install wasm-bindgen-cli (same version as the wasm-bindgen crate in Cargo.lock).
set -e
cd "$(dirname "$0")/.."
# Homebrew's cargo can be older than rust-version; prefer the rustup toolchain.
export PATH="$HOME/.cargo/bin:$PATH"
cargo rustc --lib --profile web --target wasm32-unknown-unknown --crate-type cdylib
TARGET_DIR="${CARGO_TARGET_DIR:-target}"
wasm-bindgen --target web --no-typescript --out-dir web/pkg "$TARGET_DIR/wasm32-unknown-unknown/web/papercraft.wasm"
if command -v wasm-opt >/dev/null 2>&1; then
    wasm-opt -Oz -o web/pkg/papercraft_bg.wasm web/pkg/papercraft_bg.wasm
fi
# The build stamp index.html loads the glue and the wasm under, so no browser mixes two builds.
if command -v sha256sum >/dev/null 2>&1; then
    sha256sum web/pkg/papercraft_bg.wasm | cut -c1-16 > web/pkg/build.txt
else
    shasum -a 256 web/pkg/papercraft_bg.wasm | cut -c1-16 > web/pkg/build.txt
fi
# The home-screen app: the manifest and its icons sit next to index.html.
mkdir -p web/icons
cp assets/app-icon/papercraft-1024.png assets/app-icon/papercraft.svg web/icons/
cp assets/app-icon/papercraft-macos-512.png web/icons/papercraft-512.png
