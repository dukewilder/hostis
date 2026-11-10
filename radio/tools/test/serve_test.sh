#!/bin/sh
# Builds both copies of the browser set against the private test network and serves them:
#   the open player on 8099 (/tmp/dist-test), hostis.black's copy on 8097 (/tmp/dist-harbour),
# whose HOSTIS station is the test Lantern's and whose open player is 8099. Then rebuilds the
# public copies. Usage: serve_test.sh <relay lantern id> <a station id to treat as HOSTIS's>
set -e
R=$(cd "$(dirname "$0")/../.." && pwd); cd "$R"
echo "[{\"id\":\"$2\",\"name\":\"Gangway\",\"call_sign\":\"\"}]" > /tmp/harbour-test.json
PYRATE_HARBOUR="$1@http://127.0.0.1:3340" PYRATE_RELAYS=http://127.0.0.1:3340 PYRATE_HARBOUR_STATIONS=/tmp/harbour-test.json \
  PYRATE_OPEN_PLAYERS=http://127.0.0.1:8099/ python3 browser/build.py >/dev/null
mkdir -p /tmp/dist-test /tmp/dist-harbour
rm -rf /tmp/dist-test/* /tmp/dist-harbour/*
cp -r browser/www/dist-open/* /tmp/dist-test/; cp -r browser/www/dist/* /tmp/dist-harbour/
python3 browser/build.py >/dev/null
for p in 8099:/tmp/dist-test 8097:/tmp/dist-harbour; do
  port=${p%%:*}; dir=${p#*:}
  pgrep -f "http.server $port" >/dev/null || (cd "$dir" && setsid nohup python3 -m http.server "$port" --bind 127.0.0.1 >/dev/null 2>&1 &)
done
echo "open player :8099, hostis.black copy :8097"
