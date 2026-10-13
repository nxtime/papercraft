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
