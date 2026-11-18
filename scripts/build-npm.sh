#!/bin/sh
# Builds the npm package in npm/: the WebAssembly build and the docs beside it.
# Needs `rustup target add wasm32-unknown-unknown`.
set -eu
cd "$(dirname "$0")/.."
# Panic messages embed source paths; keep the build machine's out of the shipped file.
RUSTFLAGS="-C target-feature=+simd128 --remap-path-prefix=$HOME=~ --remap-path-prefix=$PWD=." cargo build -q --release --lib --target wasm32-unknown-unknown
cp target/wasm32-unknown-unknown/release/ooxml_validator.wasm npm/
cp README.md LICENSE NOTICE npm/
ls -l npm/ooxml_validator.wasm
