#!/bin/sh
# The computer build of the app on the private test network, under a virtual screen, with a
# screenshot of each tab and of tuning in. Usage: app_shots.sh <relay id> <out dir> [w] [h]
# Needs tools/test/net_up.sh first, `flutter build linux --release`, Xvfb and xdotool. The app
# starts with a fresh home each time, so what it shows is a first open.
set -e
R=$(cd "$(dirname "$0")/../.." && pwd)
OUT=$2; W=${3:-412}; H=${4:-900}
mkdir -p "$OUT"
B="$R/app/build/linux/x64/release/bundle"
pkill -f "Xvfb :77" 2>/dev/null || true
Xvfb :77 -screen 0 ${W}x${H}x24 >/dev/null 2>&1 &
sleep 1
export DISPLAY=:77 HOME=$(mktemp -d) PYRATE_RELAYS=http://127.0.0.1:3340 PYRATE_BOOTSTRAP=$1 PYRATE_NULL_AUDIO=1
"$B/pyrate" > "$OUT/app.log" 2>&1 &
APP=$!
sleep 4
WIN=$(xdotool search --pid $APP | tail -1)
xdotool windowsize "$WIN" $W $H windowmove "$WIN" 0 0
shot() { sleep "$1"; import -window root "$OUT/$2.png"; }
tab() { xdotool mousemove $(( W * (2 * $1 + 1) / 8 )) $(( H - 30 )) click 1; }
shot 1 app-1-open
shot 5 app-2-dial
# Listen, then watch tuning in.
xdotool mousemove $(( W / 2 )) $(( H - 150 )) click 1
shot 0.4 app-3-tuning
shot 4 app-4-playing
tab 1; shot 1.5 app-5-stations
tab 2; shot 1.5 app-6-broadcast
tab 3; shot 1.5 app-7-settings
tab 0; shot 1 app-8-back
kill $APP 2>/dev/null || true
pkill -f "Xvfb :77" 2>/dev/null || true
echo "shots in $OUT"
