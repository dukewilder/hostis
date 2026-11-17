#!/bin/sh
# What a watcher on the line sees of the test network (0.9.20, phase C). Captures the loopback
# while four things happen, one capture each, and reads each as a watcher would
# (traffic_check.py): nDPI must name every flow generic QUIC, HTTP/3, TLS, HTTP, the DHT or
# ICMP; every QUIC handshake must offer h3 alone or h3 then h3-29 and name no server; and no
# byte may say Pyrate.
#   proof    a keeper's phone broadcasting through a listener's phone to a Lantern carrying
#            blind, and a listener playing through both (proof_test, whose own checks run too)
#   tab      a browser tab playing through the test relay (browser_play.py on the player at 8099)
#   find     a phone finding the network through a DHT lookup, and another through a seed list
#            fetched from a mirror (find_test)
# Usage: traffic_test.sh <relay id> <Gangway id> [<wav>]   (run under the test network's lock,
# with the network up and serve_test.sh's player on 8099, as all.sh has them)
# Needs tcpdump and tshark (apt) and ndpiReader: NDPI_READER, or one built here once from
# github.com/ntop/nDPI into ~/.cache/pyrate-ndpi.
R=$(cd "$(dirname "$0")/../.." && pwd)
RELAY=$1; GANG=$2; WAV=${3:-/tmp/tone.wav}
OUT=${PYRATE_TRAFFIC_OUT:-/tmp/traffic-test}
NET="PYRATE_RELAYS=http://127.0.0.1:3340 PYRATE_LAN_ANY=1 PYRATE_NULL_AUDIO=1 PYRATE_QUARTER_MS=20000"
rm -rf "$OUT"; mkdir -p "$OUT"

for t in tcpdump tshark; do
  command -v $t > /dev/null || DEBIAN_FRONTEND=noninteractive apt-get install -y -q tcpdump tshark > "$OUT/apt.log" 2>&1
  command -v $t > /dev/null || { echo "FAIL $t is missing and could not be installed"; exit 1; }
done
N=${NDPI_READER:-$HOME/.cache/pyrate-ndpi/nDPI/example/ndpiReader}
if [ ! -x "$N" ]; then
  echo "building ndpiReader from github.com/ntop/nDPI (once)"
  mkdir -p "$HOME/.cache/pyrate-ndpi" && cd "$HOME/.cache/pyrate-ndpi" || exit 1
  [ -d nDPI ] || git clone -q --depth 1 https://github.com/ntop/nDPI.git nDPI
  (cd nDPI && ./autogen.sh && ./configure && make -j4) > "$OUT/ndpi-build.log" 2>&1
  cd "$R" || exit 1
  [ -x "$N" ] || { echo "FAIL ndpiReader could not be built (see $OUT/ndpi-build.log)"; exit 1; }
fi

# The machine's own services (anything listening before the test, but the test relay and the
# player) are left out of every capture: they are this machine's, not the radio's.
SKIP=$(python3 -c "
import glob
s = set()
for f in ('/proc/net/tcp', '/proc/net/tcp6'):
    try:
        for l in open(f).readlines()[1:]:
            c = l.split()
            if c[3] == '0A':
                s.add(int(c[1].split(':')[1], 16))
    except OSError:
        pass
print(' '.join(str(p) for p in sorted(s - {3340, 8099, 8097})))
")
FILTER="not port 8099 and not port 8097"
for p in $SKIP; do FILTER="$FILTER and not port $p"; done

# cap <name> <command>: the loopback captured while the command runs.
cap() {
  name=$1; shift
  tcpdump -i lo -U -s 0 -w "$OUT/$name.pcap" "$FILTER" > "$OUT/$name.tcpdump" 2>&1 &
  T=$!
  sleep 2
  sh -c "$*" > "$OUT/$name.log" 2>&1
  r=$?
  sleep 2
  kill $T; wait $T 2>/dev/null
  grep -E "^(PASS|FAIL)" "$OUT/$name.log" | sed "s/^/  $name: /"
  [ $r -eq 0 ] && ! grep -q "^FAIL" "$OUT/$name.log"; ok=$?
  if [ $ok = 0 ]; then echo "PASS $name ran"; else echo "FAIL $name ran (see $OUT/$name.log)"; fi
}

cap proof "$NET $R/target/release/proof_test $OUT/proof-work '$WAV'"
cap tab "python3 $R/tools/test/browser_play.py 8099"
cap find "$NET $R/target/release/find_test $RELAY $GANG"

for name in proof tab find; do
  python3 "$R/tools/test/traffic_check.py" "$OUT/$name.pcap" "$N" "$name"
done
rm -rf "$OUT/proof-work"
# The captures stay for reading in Wireshark; they are a few tens of megabytes.
