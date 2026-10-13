#!/bin/sh
# Build the browser app into web/pkg. Needs: rustup target add wasm32-unknown-unknown,
# cargo install wasm-bindgen-cli (same version as the wasm-bindgen crate in Cargo.lock).
set -e
cd "$(dirname "$0")/.."
# Homebrew's cargo can be older than rust-version; prefer the rustup toolchain.
export PATH="$HOME/.cargo/bin:$PATH"
cargo rustc --lib --release --target wasm32-unknown-unknown --crate-type cdylib
wasm-bindgen --target web --no-typescript --out-dir web/pkg target/wasm32-unknown-unknown/release/papercraft.wasm
