#!/bin/sh
# Builds the browser radio into browser/www/dist.
set -e
cd "$(dirname "$0")/.."
# The build paths are remapped so the wasm is the same bytes wherever it is built, from the same
# source and the pinned toolchains (rust-toolchain.toml, RELEASES.md, tools/repro.sh).
export RUSTFLAGS="${RUSTFLAGS:-} --remap-path-prefix=$(pwd)=/pyrate --remap-path-prefix=${CARGO_HOME:-$HOME/.cargo}=/cargo"
CC_wasm32_unknown_unknown=clang cargo build -p pyrate-browser --target wasm32-unknown-unknown --profile web
wasm-bindgen --target web --out-dir browser/www/pkg --no-typescript "${CARGO_TARGET_DIR:-target}/wasm32-unknown-unknown/web/pyrate_browser.wasm"
if command -v wasm-opt >/dev/null; then wasm-opt -Oz --enable-bulk-memory --enable-nontrapping-float-to-int --enable-sign-ext --enable-mutable-globals -o browser/www/pkg/pyrate_browser_bg.wasm browser/www/pkg/pyrate_browser_bg.wasm; fi
python3 browser/build.py
