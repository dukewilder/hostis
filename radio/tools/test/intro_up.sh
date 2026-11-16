#!/bin/sh
# The test network for introduced doors: no listed door anywhere, and a relay that can be stopped
# on its own.
#   R   the relay on 127.0.0.1:3340, a Lantern that carries nothing
#   A   a harbour Lantern with Gangway, homed on R, found by the others on the local network too
#   B   a Lantern that carries all it hears: a middle that introduces tabs to doors
#   D   a listener's app that may be a door (--door auto: a test network asks no router, so no
#       port is mapped), opened by duty because no door is known
#   E   a listener's app that is no door
# Writes /tmp/intro-ids: R's id, A's id, Gangway's id, D's id.
# Usage: intro_up.sh [<music dir>]   (the music dir as for net_up.sh)
set -e
R=$(cd "$(dirname "$0")/../.." && pwd)
M=${1:-/tmp/net}
L="$R/target/release/lantern"
export PYRATE_RELAYS=http://127.0.0.1:3340 PYRATE_LAN_ANY=1 PYRATE_NULL_AUDIO=1 PYRATE_DUTY_MS=${PYRATE_DUTY_MS:-20000}
pkill -f "$L" 2>/dev/null || true
sleep 1
rm -rf /tmp/intro-r /tmp/intro-a /tmp/intro-b /tmp/intro-d /tmp/intro-e
id_of() { for i in $(seq 1 40); do grep -q "node id" "$1" 2>/dev/null && break; sleep 1; done; grep "node id" "$1" | grep -o "[0-9a-f]\{64\}" | head -1; }
setsid nohup "$L" --dir /tmp/intro-r --show-ids --hold-key-on-disk --relay 127.0.0.1:3340 --carry-own --lan > /tmp/intro-r.log 2>&1 &
RID=$(id_of /tmp/intro-r.log)
setsid nohup "$L" --dir /tmp/intro-a --show-ids --hold-key-on-disk --carry-own --lan --bootstrap "$RID" --seed "Gangway|Shanties and sea songs for the test network|$M/gang|music|Bristol" > /tmp/intro-a.log 2>&1 &
A=$(id_of /tmp/intro-a.log)
setsid nohup "$L" --dir /tmp/intro-b --show-ids --hold-key-on-disk --open all --lan --bootstrap "$A" > /tmp/intro-b.log 2>&1 &
PYRATE_DOOR_DEBUG=1 setsid nohup "$L" --dir /tmp/intro-d --show-ids --hold-key-on-disk --listener --lan --bootstrap "$A" --door auto > /tmp/intro-d.log 2>&1 &
setsid nohup "$L" --dir /tmp/intro-e --show-ids --hold-key-on-disk --listener --lan --bootstrap "$A" > /tmp/intro-e.log 2>&1 &
D=$(id_of /tmp/intro-d.log)
for i in $(seq 1 60); do grep -q "door open for introduced tabs only" /tmp/intro-d.log 2>/dev/null && break; sleep 1; done
for i in $(seq 1 40); do grep -q "Gangway" /tmp/intro-a.log 2>/dev/null && break; sleep 1; done
GANG=$(grep "Gangway" /tmp/intro-a.log | grep -o "[0-9a-f]\{64\}" | head -1)
echo "$RID $A $GANG $D" > /tmp/intro-ids
echo "relay $RID"
echo "harbour $A"
echo "gangway $GANG"
echo "door $D"
grep -h "door open" /tmp/intro-d.log | head -2
