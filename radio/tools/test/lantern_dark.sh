#!/bin/sh
# A dark Lantern beside Tor (0.9.20, phase B). This machine runs no Docker daemon and no Tor, so
# the compose file is checked by `docker compose config`, the installer runs dry with a piped
# passphrase, and the Lantern side runs from target/release/lantern with PYRATE_DARK and a
# stand-in for Tor's HTTP tunnel (stand_in_tor.py). Checks:
#   - the compose file publishes no port, keeps the Lantern on a network with no way out but
#     Tor, caps every log at a megabyte and hands the passphrase in as a secret file;
#   - the installer asks for the passphrase once, never prints it, and keeps it root only;
#   - a dark Lantern reaches the test network through the tunnel alone (its relay, and the
#     relay's own checks), joins it, sends nothing
#     to the DHT, and leaves nothing in the clear in its folder.
# One PASS or FAIL line a check. Usage: lantern_dark.sh <relay Lantern id>   (net_up.sh's relay)
# Run on the test network under its lock (flock -o), with PYRATE_RELAYS set.
R=$(cd "$(dirname "$0")/../.." && pwd)
L="$R/target/release/lantern"
RELAY_ID=$1
RELAY_URL=${PYRATE_RELAYS:-http://127.0.0.1:3340}
W=$(mktemp -d /tmp/lantern-dark.XXXXXX)
PROXY_PORT=${PYRATE_DARK_TEST_PORT:-8378}
TRAP_PORT=$((PROXY_PORT + 1))
ok() { if [ "$1" = 0 ]; then echo "PASS $2"; else echo "FAIL $2"; fi; }

# 1. The compose file, as Docker reads it.
C=$(docker compose -f "$R/lantern-dark/compose.yaml" config 2>"$W/compose.err"); r=$?
ok $r "docker compose reads the dark Lantern's compose file"
printf '%s\n' "$C" > "$W/compose.txt"
python3 - "$W/compose.txt" <<'PY'; ok $? "no port published, the Lantern inside with Tor alone, logs capped, the passphrase a secret"
import sys, re
t = open(sys.argv[1]).read()
bad = []
if re.search(r"^\s+(ports|published):", t, re.M): bad.append("a port is published")
lan = t.split("\n  lantern:")[1].split("\n  tor:")[0]
if "outside" in lan or "network_mode" in lan: bad.append("the Lantern has a way out")
if "internal: true" not in t: bad.append("the inside network is not internal")
if t.count("max-size: 1m") < 2: bad.append("a log is not capped")
if "PYRATE_DARK: \"1\"" not in lan or "PYRATE_PROXY: http://tor:8118" not in lan: bad.append("the Lantern is not dark through Tor")
if "/run/secrets/hold-pass" not in lan or "hold-key-on-disk" in t or "show-ids" in t: bad.append("the hold key is not from the passphrase")
for b in bad: print(b)
sys.exit(1 if bad else 0)
PY

# 2. The installer, dry, with the passphrase piped twice: written root only, never printed.
cp -r "$R/lantern-dark" "$W/ld"; mkdir -p "$W/radio"; mv "$W/ld" "$W/radio/lantern-dark"; cp "$R/Dockerfile" "$W/radio/"
PASS="a dark lantern test passphrase"
printf '%s\n%s\n' "$PASS" "$PASS" | PYRATE_HOLD_PASS_PATH="$W/hold-pass" PYRATE_DRY=1 setsid bash "$W/radio/lantern-dark/install.sh" > "$W/install.log" 2>&1; r=$?
! grep -q "$PASS" "$W/install.log"; ok $((r + $?)) "the installer runs and never prints the passphrase"
grep -q "write $W/hold-pass" "$W/install.log" && grep -q "docker compose" "$W/install.log"; ok $? "the installer writes the passphrase file and starts compose"
# Its real write, as root would make it: the same function, outside the dry run.
sed -n '/^read_secret()/,/^}/p' "$R/lantern-dark/install.sh" > "$W/fn.sh"
printf '. %s/fn.sh\nP=$(read_secret x)\n(umask 077 && printf "%%s\\n" "$P" > %s/hp)\n' "$W" "$W" > "$W/w.sh"
printf '%s\n' "$PASS" | setsid bash "$W/w.sh" > /dev/null 2>&1
[ "$(stat -c %a "$W/hp" 2>/dev/null)" = 600 ] && [ "$(cat "$W/hp" 2>/dev/null)" = "$PASS" ]; r=$?
[ $r = 0 ] || echo "  mode $(stat -c %a "$W/hp" 2>&1), $(wc -c < "$W/hp" 2>&1) bytes"
ok $r "the passphrase file is readable by its owner alone"
printf 'short\nshort\n' | PYRATE_DRY=1 PYRATE_HOLD_PASS_PATH="$W/none" setsid bash "$W/radio/lantern-dark/install.sh" > "$W/short.log" 2>&1; r=$?
[ $r -ne 0 ] && grep -q "twelve" "$W/short.log"; ok $? "a passphrase under twelve characters is refused"

# 3. The Lantern side: dark, through the stand-in tunnel, with a trap where the DHT would be.
: > "$W/targets"
python3 "$R/tools/test/stand_in_tor.py" "$PROXY_PORT" "$W/targets" & PROXY=$!
python3 -c "
import socket,sys
s=socket.socket(socket.AF_INET,socket.SOCK_DGRAM); s.bind(('127.0.0.1',$TRAP_PORT))
open('$W/trap','w').close()
while True:
    d,a=s.recvfrom(4096); open('$W/trap','a').write('packet\n')
" & TRAP=$!
sleep 1
printf 'the dark lantern test passphrase\n' > "$W/pass"
PYRATE_DARK=1 PYRATE_PROXY="http://127.0.0.1:$PROXY_PORT" PYRATE_HOLD_PASS_FILE="$W/pass" \
  PYRATE_DHT_BOOTSTRAP="127.0.0.1:$TRAP_PORT" PYRATE_DHT_EVERY_S=5 PYRATE_FIND_WAIT_S=3 \
  setsid "$L" --dir "$W/data" --show-ids --bootstrap "$RELAY_ID@$RELAY_URL" > "$W/lantern.log" 2>&1 &
LP=$!
for _ in $(seq 1 90); do grep -q "neighbour up" "$W/lantern.log" && break; sleep 1; done
grep -q "neighbour up" "$W/lantern.log"; ok $? "a dark Lantern joins the test network"
HOSTPORT=${RELAY_URL#http://}; HOSTPORT=${HOSTPORT%/}
grep -q "^CONNECT $HOSTPORT" "$W/targets"; ok $? "it reaches its relay through the tunnel"
sleep 20
# Its relay through CONNECT, and the relay's own checks (/ping, /generate_204) by plain HTTP to
# the same host, since the test relay speaks HTTP: nothing else.
HOST=${HOSTPORT%:*}
! grep -v "^CONNECT $HOSTPORT\$\|^GET http://$HOST[:/]" "$W/targets" | sed 's/^/  asked: /' | grep .; ok $? "it asks the tunnel for nothing but its relay"
[ ! -s "$W/trap" ]; ok $? "it sends nothing to the DHT"
grep -q "dark running" "$W/lantern.log" && ! grep -q "left a note in the DHT" "$W/lantern.log"; ok $? "it says it runs dark and leaves no note anywhere"
kill $LP 2>/dev/null; wait $LP 2>/dev/null
python3 "$R/tools/test/lantern_disk.py" "$W/data" > "$W/disk.log" 2>&1; ok $? "nothing in its folder lies in the clear"

kill $PROXY $TRAP 2>/dev/null
rm -rf "$W"
