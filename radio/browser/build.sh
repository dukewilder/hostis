#!/bin/sh
# Builds the browser radio into browser/www/dist.
set -e
cd "$(dirname "$0")/.."
CC_wasm32_unknown_unknown=clang cargo build -p pyrate-browser --target wasm32-unknown-unknown --profile web
wasm-bindgen --target web --out-dir browser/www/pkg --no-typescript target/wasm32-unknown-unknown/web/pyrate_browser.wasm
if command -v wasm-opt >/dev/null; then wasm-opt -Oz --enable-bulk-memory --enable-nontrapping-float-to-int --enable-sign-ext --enable-mutable-globals -o browser/www/pkg/pyrate_browser_bg.wasm browser/www/pkg/pyrate_browser_bg.wasm; fi
python3 browser/build.py
