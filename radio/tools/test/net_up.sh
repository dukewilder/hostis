#!/bin/sh
# The private test network in one go: a relay Lantern with a station of its own (Gangway), and a
# carry-all Lantern with a second (Second Watch), each with art and two recordings. Prints the
# relay's id and Gangway's id, ready for serve_test.sh. Usage: net_up.sh [<music dir>]
# The music dir needs gang/ and watch/ folders of recordings (and station.jpg if you like).
set -e
R=$(cd "$(dirname "$0")/../.." && pwd)
M=${1:-/tmp/net}
L="$R/target/release/lantern"
export PYRATE_RELAYS=http://127.0.0.1:3340
pkill -f "$L" 2>/dev/null || true
sleep 1
rm -rf /tmp/net-relay /tmp/net-carry
setsid nohup "$L" --dir /tmp/net-relay --relay 127.0.0.1:3340 --carry-own --seed "Gangway|Shanties and sea songs for the test network|$M/gang|music|Bristol" > /tmp/net-relay.log 2>&1 &
for i in $(seq 1 40); do grep -q "Gangway" /tmp/net-relay.log 2>/dev/null && grep -qi "id" /tmp/net-relay.log && break; sleep 1; done
RELAY=$(grep -o "[0-9a-f]\{64\}" /tmp/net-relay.log | head -1)
setsid nohup "$L" --dir /tmp/net-carry --bootstrap "$RELAY" --seed "Second Watch|Night music for the late watch|$M/watch|music|Lisbon" > /tmp/net-carry.log 2>&1 &
sleep 8
GANG=$(grep "Gangway" /tmp/net-relay.log | grep -o "[0-9a-f]\{64\}" | head -1)
echo "relay $RELAY"
echo "gangway $GANG"
