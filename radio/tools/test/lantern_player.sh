#!/bin/sh
# The open player from a Lantern's gangway at /listen/, on the private test network: a Lantern
# started with --http and --player serves the test build of the open player, and a tab plays a
# station from it, from loopback and from this machine's own address (no secure context, as on
# a Wi-Fi), beside the plain copy serve_test.sh puts on 8099.
# Usage: lantern_player.sh [port]   after net_up.sh and serve_test.sh <relay id> <gangway id>.
set -e
R=$(cd "$(dirname "$0")/../.." && pwd)
L="$R/target/release/lantern"
PORT=${1:-8391}
[ -f /tmp/dist-test/index.html ] || { echo "No test build in /tmp/dist-test: run net_up.sh, then serve_test.sh."; exit 1; }
RELAY=$(grep -o "[0-9a-f]\{64\}" /tmp/net-relay.log | head -1)
[ -n "$RELAY" ] || { echo "No test network: run net_up.sh first."; exit 1; }
pkill -f -- "--dir /tmp/net-player " 2>/dev/null || true
sleep 1
rm -rf /tmp/net-player
PYRATE_RELAYS=http://127.0.0.1:3340 setsid nohup "$L" --dir /tmp/net-player --bootstrap "$RELAY" --carry-own \
  --http "0.0.0.0:$PORT" --player /tmp/dist-test > /tmp/net-player.log 2>&1 &
for _ in $(seq 1 30); do grep -q "open player on" /tmp/net-player.log 2>/dev/null && break; sleep 1; done
grep "open player on" /tmp/net-player.log || { tail -20 /tmp/net-player.log; exit 1; }
PLAYWRIGHT_BROWSERS_PATH=${PLAYWRIGHT_BROWSERS_PATH:-/opt/pw-browsers} python3 "$R/tools/test/lantern_player.py" "$PORT" /tmp/dist-test
pkill -f -- "--dir /tmp/net-player " 2>/dev/null || true
