#!/bin/sh
# What the radio costs a computer left on: the Linux app under Xvfb, playing nothing, first with
# its window open and then started in the tray with no window (PYRATE_TRAY=1, as "Keep the radio
# running when the window is closed" leaves it). Each settles for a minute and is then measured
# for a minute: CPU as a share of one core, and memory (RSS, the app with its node).
# Joins the private test network if it is up (net_up.sh), so nothing leaves this machine.
# Usage: tools/test/desk_idle.sh [<bundle dir>]   (default app/build/linux/x64/release/bundle)
R=$(cd "$(dirname "$0")/../.." && pwd)
B=${1:-$R/app/build/linux/x64/release/bundle}
SETTLE=${SETTLE:-60}; SPAN=${SPAN:-60}
[ -x "$B/pyrate" ] || { echo "no app at $B"; exit 2; }
HZ=$(getconf CLK_TCK)
RELAY=$(cut -d' ' -f1 /tmp/net-ids 2>/dev/null)

measure() {
  label=$1; shift
  H=$(mktemp -d /tmp/desk-idle.XXXXXX)
  env HOME="$H" XDG_DATA_HOME="$H/data" XDG_CONFIG_HOME="$H/config" XDG_CACHE_HOME="$H/cache" \
    PYRATE_RELAYS=http://127.0.0.1:3340 PYRATE_LAN_ANY=1 PYRATE_NULL_AUDIO=1 ${RELAY:+PYRATE_BOOTSTRAP=$RELAY} "$@" \
    xvfb-run -a -s "-screen 0 1280x800x24" "$B/pyrate" > "$H/app.log" 2>&1 &
  sleep 5
  P=$(pgrep -n -f "$B/pyrate")
  [ -n "$P" ] || { echo "$label: the app did not start"; cat "$H/app.log" | tail -5; rm -rf "$H"; return 1; }
  sleep "$SETTLE"
  t0=$(awk '{print $14 + $15}' /proc/$P/stat); sleep "$SPAN"; t1=$(awk '{print $14 + $15}' /proc/$P/stat)
  rss=$(awk '/^VmRSS/ {print int($2 / 1024)}' /proc/$P/status); anon=$(awk '/^RssAnon/ {print int($2 / 1024)}' /proc/$P/status)
  thr=$(ls /proc/$P/task | wc -l)
  echo "$label: CPU $(awk -v a="$t0" -v b="$t1" -v hz="$HZ" -v s="$SPAN" 'BEGIN {printf "%.2f", (b - a) * 100 / hz / s}')% of one core over ${SPAN} s, memory ${rss} MB (${anon} MB its own, the rest shared libraries), ${thr} threads"
  kill "$P" 2>/dev/null; sleep 2; pkill -f "Xvfb :" 2>/dev/null
  rm -rf "$H"
}

measure "window open, idle"
measure "in the tray, no window" PYRATE_TRAY=1
