#!/bin/sh
# Reproducible builds. Build the browser player, and where the toolchain is present the Android
# APK, then check the result against a published hash, so anyone can prove the bytes we ship come
# from this source and nothing added. The toolchains are pinned; the recipe is in RELEASES.md.
#
#   tools/repro.sh player            build the player, compare its wasm to tools/repro.hashes
#   tools/repro.sh player --record   build the player, write its wasm hash to tools/repro.hashes
#   tools/repro.sh apk <published>   build the APK and compare it, signatures left aside
#
# No rights reserved.
set -e
R=$(cd "$(dirname "$0")/.." && pwd); cd "$R"
HASHES="$R/tools/repro.hashes"

# The pins. A build with other versions may still match, but this is what the published hash was
# made with; a mismatch here is the first thing to check if the wasm hash does not match.
PIN_RUSTC=1.97.0
PIN_BINDGEN=0.2.130
PIN_WASMOPT=132

want() { grep -E "^$1 " "$HASHES" 2>/dev/null | awk '{print $2}'; }
warn_pin() { [ "$2" = "$3" ] || echo "  ! $1 is $2, the pin is $3 (RELEASES.md): the hash may differ"; }

player() {
  echo "== browser player =="
  rv=$(rustc --version 2>/dev/null | awk '{print $2}')
  bg=$(wasm-bindgen --version 2>/dev/null | awk '{print $2}')
  wo=$(wasm-opt --version 2>/dev/null | awk '{print $3}')
  echo "  rustc $rv, wasm-bindgen $bg, wasm-opt $wo"
  warn_pin rustc "$rv" "$PIN_RUSTC"
  warn_pin wasm-bindgen "$bg" "$PIN_BINDGEN"
  warn_pin wasm-opt "$wo" "$PIN_WASMOPT"
  echo "  building (browser/build.sh) ..."
  sh "$R/browser/build.sh" > /tmp/repro-player.log 2>&1
  h=$(sha256sum browser/www/pkg/pyrate_browser_bg.wasm | awk '{print $1}')
  echo "  pyrate_browser_bg.wasm  $h"
  if [ "$1" = "--record" ]; then
    tmp=$(mktemp)
    grep -v '^player-wasm ' "$HASHES" 2>/dev/null > "$tmp" || true
    echo "player-wasm $h" >> "$tmp"
    sort "$tmp" -o "$HASHES"; rm -f "$tmp"
    echo "  recorded in tools/repro.hashes"
    return 0
  fi
  w=$(want player-wasm)
  if [ -z "$w" ]; then echo "  no published hash yet: run tools/repro.sh player --record"; return 1; fi
  if [ "$h" = "$w" ]; then echo "  MATCH the published hash"; else echo "  MISMATCH: published $w"; return 1; fi
}

apk() {
  echo "== android apk =="
  pub=$1
  if ! command -v flutter >/dev/null 2>&1; then
    echo "  no flutter toolchain here. The recipe and the pinned versions are in RELEASES.md."
    echo "  Build it where flutter is set up, then compare with tools/apk_compare.py."
    return 0
  fi
  [ -n "$pub" ] || { echo "  usage: tools/repro.sh apk <published.apk>"; return 2; }
  echo "  building (app/build_native.sh) ..."
  bash "$R/app/build_native.sh"
  built="$R/app/build/app/outputs/flutter-apk/app-release.apk"
  echo "  comparing $built against $pub, signatures aside"
  python3 "$R/tools/apk_compare.py" "$built" "$pub"
}

cmd=${1:-player}
case "$cmd" in
  player) player "$2" ;;
  apk) apk "$2" ;;
  all) player "" && apk "$2" ;;
  *) echo "usage: tools/repro.sh [player [--record] | apk <published.apk>]"; exit 2 ;;
esac
