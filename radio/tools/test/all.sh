#!/bin/sh
# Every suite in one go, before a launch: a fresh test network, the test wasm, then each suite in
# turn, the quick first and the slow last. One PASS or FAIL line a suite, with its time, and a
# total; full logs in /tmp/all-tests/. Exits non-zero if any suite fails.
# Usage: tools/test/all.sh [<music dir>]   (default /tmp/net, with gang/ and watch/ as for net_up.sh)
# Release builds only. Stops before building if the disk has under 800 MB free.
R=$(cd "$(dirname "$0")/../.." && pwd); cd "$R" || exit 2
M=${1:-/tmp/net}
OUT=/tmp/all-tests
WAV=${PYRATE_TEST_WAV:-"$M/gang/$(ls "$M/gang" | grep -i '\.wav$' | head -1)"}
rm -rf "$OUT"; mkdir -p "$OUT/privacy" "$OUT/desk" "$OUT/pentest" "$OUT/ux" "$OUT/scale" "$OUT/sweep"
export PLAYWRIGHT_BROWSERS_PATH=${PLAYWRIGHT_BROWSERS_PATH:-/opt/pw-browsers}
NET="PYRATE_RELAYS=http://127.0.0.1:3340 PYRATE_LAN_ANY=1 PYRATE_NULL_AUDIO=1"
PASSED=0; FAILED=0; START=$(date +%s)
SUMMARY="$OUT/summary.txt"; : > "$SUMMARY"

free_mb() { df -Pm / | awk 'NR==2 {print $4}'; }
# Every build of the player keeps its CAR in browser/www/cars; the test builds made during this
# run are let go after each suite, all but the newest, so the disk holds.
touch "$OUT/.start"
tidy() { ls -t browser/www/cars/*.car 2>/dev/null | tail -n +2 | while read -r f; do [ "$f" -nt "$OUT/.start" ] && rm -f "$f"; done; }
line() { echo "$1" | tee -a "$SUMMARY"; }
# run <name> <timeout s> <command...>: one suite, its log in $OUT/<name>.log.
run() {
  name=$1; secs=$2; shift 2
  t0=$(date +%s)
  if timeout "$secs" sh -c "$*" > "$OUT/$name.log" 2>&1 && ! grep -q "^FAIL" "$OUT/$name.log"; then
    r=PASS; PASSED=$((PASSED + 1))
  else
    r=FAIL; FAILED=$((FAILED + 1))
  fi
  line "$r $name ($(( $(date +%s) - t0 )) s)"
  tidy
  if [ "$(free_mb)" -lt 800 ]; then line "FAIL the disk fell under 800 MB after $name: stopping"; exit 2; fi
}

if [ "$(free_mb)" -lt 800 ]; then echo "under 800 MB free on /: not building"; exit 2; fi
line "== builds (release; the test wasm with PYRATE_LIST_AGE_MS=60000 PYRATE_DOOR_AGE_MS=30000)"
# The listening suites play (to a null sink with PYRATE_NULL_AUDIO), so they are built with playback.
run build-native 2400 "cargo build --release -p pyrate-node --bin lantern --bin pentest && cargo build --release -p pyrate-node --features playback --bin desk_test --bin privacy_test --bin onair_test --bin raise_test --bin crew_deck_test --bin pocket_test"
if [ "$(free_mb)" -lt 800 ]; then line "FAIL disk under 800 MB after the native build"; exit 2; fi
run build-wasm 2400 "PYRATE_LIST_AGE_MS=60000 PYRATE_DOOR_AGE_MS=30000 sh browser/build.sh"

line "== unit"
run unit-node 1200 "cargo test --release -p pyrate-node --lib"
run unit-core 1200 "cargo test --release -p pyrate-core"

line "== the test network (relay on 3340)"
pkill -f "http.server 80" 2>/dev/null
# The Lanterns print what they serve every five seconds, for the crowd test.
PYRATE_STATS_S=5 sh tools/test/net_up.sh "$M" > "$OUT/net_up.log" 2>&1
RELAY=$(awk '/^relay/ {print $2}' "$OUT/net_up.log"); GANG=$(awk '/^gangway/ {print $2}' "$OUT/net_up.log")
echo "$RELAY $GANG" > /tmp/net-ids
sh tools/test/serve_test.sh "$RELAY" "$GANG" > "$OUT/serve_test.log" 2>&1
sleep 5

line "== native suites"
run privacy_test 600 "$NET target/release/privacy_test /tmp/all-tests/privacy '$WAV'"
run onair_test 600 "$NET target/release/onair_test $RELAY '$WAV'"
run desk_test 900 "$NET target/release/desk_test $RELAY /tmp/all-tests/desk"
run crew_deck_test 600 "$NET target/release/crew_deck_test $RELAY $GANG"

line "== browser suites (open player 8099, hostis.black's copy 8097)"
B="python3 tools/test"
run browser_play 300 "$B/browser_play.py 8099"
run browser_ui 300 "$B/browser_ui.py 8099"
run browser_hidden 300 "$B/browser_hidden.py 8099"
run browser_trap 300 "$B/browser_trap.py"
run browser_scope 300 "$B/browser_scope.py $GANG"
run browser_return 400 "$B/browser_return.py 8097"
run browser_pass 300 "$B/browser_pass.py"
run browser_live 400 "$B/browser_live.py"
run browser_station 900 "$B/browser_station.py 8099"
run browser_desk 400 "$B/browser_desk.py 8099"
run browser_rtc 400 "$B/browser_rtc.py 8099 http://127.0.0.1:3340/"
run browser_attack 400 "$B/browser_attack.py 8099 http://127.0.0.1:3340/ target/release/pentest forwards"
run browser_raise 600 "$B/browser_raise.py '$WAV'"
run browser_w1 600 "$B/browser_w1.py $GANG '$WAV'"
run browser_pocket 400 "rm -f $OUT/invite; (PYRATE_BROWSER_INVITE=$OUT/invite PYRATE_BROWSER_WAIT=150 $NET target/release/pocket_test $RELAY '$WAV' > $OUT/pocket_test.log 2>&1 &); for i in \$(seq 1 120); do [ -s $OUT/invite ] && break; sleep 1; done; $B/browser_pocket.py $OUT/invite 8099"
run browser_ux 900 "$B/browser_ux.py 8099 $OUT/ux"
run browser_scale 900 "$B/browser_scale.py 8099 $OUT/scale 300"
run browser_crew 900 "$B/browser_crew.py 8099"

line "== doors, with no relay at all (the relay stopped)"
pkill -f "http.server 80" 2>/dev/null
sh tools/test/doors_up.sh "$M" > "$OUT/doors_up.log" 2>&1
sleep 32
run browser_doors 900 "$B/browser_doors.py"
run browser_offline 600 "$B/browser_offline.py"

line "== the slowest, last"
# A station raised by a phone waits for its work and a day before the dial lists it; the test
# network shortens the day to a minute, as the test wasm does.
pkill -f "$R/target/release/lantern" 2>/dev/null; pkill -f "http.server 80" 2>/dev/null
PYRATE_STATS_S=5 sh tools/test/net_up.sh "$M" > "$OUT/net_up2.log" 2>&1
RELAY=$(awk '/^relay/ {print $2}' "$OUT/net_up2.log"); GANG=$(awk '/^gangway/ {print $2}' "$OUT/net_up2.log")
echo "$RELAY $GANG" > /tmp/net-ids
sh tools/test/serve_test.sh "$RELAY" "$GANG" > "$OUT/serve_test2.log" 2>&1
sleep 5
run raise_test 1300 "PYRATE_LIST_AGE_MS=60000 $NET target/release/raise_test $RELAY '$WAV'"
# The landing page's part of the sweep builds the landing round the released app.
[ -e /tmp/pyrate-radio-0.9.14.apk ] || ln -s /mnt/user-data/outputs/pyrate-radio-0.9.14-to-sign.apk /tmp/pyrate-radio-0.9.14.apk
run browser_sweep 2400 "$B/browser_sweep.py --out $OUT/sweep"
run pentest 2400 "$NET target/release/pentest /tmp/all-tests/pentest '$WAV' all"

pkill -f "$R/target/release/lantern" 2>/dev/null
python3 browser/build.py > /dev/null 2>&1
line "== $PASSED passed, $FAILED failed, $(( ($(date +%s) - START) / 60 )) min in all; logs in $OUT"
[ "$FAILED" -eq 0 ]
