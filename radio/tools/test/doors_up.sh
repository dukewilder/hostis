#!/bin/sh
# The test network with no relay at all: listeners' doors are the only way in for browsers.
#   A   a harbour Lantern with Gangway, no relay
#   B   a Lantern with Second Watch, found through A on the local network
#   D1  a listener's app as a door at 127.0.0.1:47101, D2 another at 127.0.0.1:47102,
#       both letting links name them
# Every node is told of a relay at 127.0.0.1:3340 that is not running, so nothing can use one.
# Writes /tmp/doors-ids: A's id, Gangway's id, Second Watch's id, D1's note, D2's note.
# Usage: doors_up.sh [<music dir>]   (the music dir as for net_up.sh)
set -e
R=$(cd "$(dirname "$0")/../.." && pwd)
M=${1:-/tmp/net}
L="$R/target/release/lantern"
export PYRATE_RELAYS=http://127.0.0.1:3340 PYRATE_LAN_ANY=1 PYRATE_NULL_AUDIO=1
pkill -f "$L" 2>/dev/null || true
sleep 1
if (echo > /dev/udp/127.0.0.1/3340) 2>/dev/null && ss -lnt 2>/dev/null | grep -q ":3340 "; then echo "a relay is still listening on 3340"; exit 1; fi
rm -rf /tmp/doors-a /tmp/doors-b /tmp/doors-d1 /tmp/doors-d2
setsid nohup "$L" --dir /tmp/doors-a --carry-own --lan --seed "Gangway|Shanties and sea songs for the test network|$M/gang|music|Bristol" > /tmp/doors-a.log 2>&1 &
for i in $(seq 1 40); do grep -q "node id" /tmp/doors-a.log 2>/dev/null && break; sleep 1; done
A=$(grep "node id" /tmp/doors-a.log | grep -o "[0-9a-f]\{64\}" | head -1)
setsid nohup "$L" --dir /tmp/doors-b --carry-own --lan --bootstrap "$A" --seed "Second Watch|Night music for the late watch|$M/watch|music|Lisbon" > /tmp/doors-b.log 2>&1 &
PYRATE_DOOR_PORT=47101 setsid nohup "$L" --dir /tmp/doors-d1 --listener --lan --bootstrap "$A" --door 127.0.0.1:47101 --door-links --http 127.0.0.1:8091 > /tmp/doors-d1.log 2>&1 &
PYRATE_DOOR_PORT=47102 setsid nohup "$L" --dir /tmp/doors-d2 --listener --lan --bootstrap "$A" --door 127.0.0.1:47102 --door-links --http 127.0.0.1:8092 > /tmp/doors-d2.log 2>&1 &
for i in $(seq 1 60); do grep -q '"open":true' /tmp/doors-d1.log 2>/dev/null && grep -q '"open":true' /tmp/doors-d2.log 2>/dev/null && break; sleep 1; done
sleep 4
GANG=$(grep "Gangway" /tmp/doors-a.log | grep -o "[0-9a-f]\{64\}" | head -1)
WATCH=$(grep "Second Watch" /tmp/doors-b.log | grep -o "[0-9a-f]\{64\}" | head -1)
D1=$(curl -s http://127.0.0.1:8091/doors.txt | head -1)
D2=$(curl -s http://127.0.0.1:8092/doors.txt | head -1)
echo "$A $GANG $WATCH $D1 $D2" > /tmp/doors-ids
echo "harbour $A"
echo "gangway $GANG"
echo "watch $WATCH"
grep -h '"open":true' /tmp/doors-d1.log /tmp/doors-d2.log | cut -c1-160
