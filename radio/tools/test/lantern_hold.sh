#!/bin/sh
# A Lantern's hold key off the disk (0.9.20, phase A). Starts real Lanterns from
# target/release/lantern in a scratch folder and checks:
#   - with no passphrase it opens nothing, starts nothing and makes no file;
#   - a passphrase from a file, a file descriptor or a command opens the same hold, and a wrong
#     one opens nothing and changes nothing;
#   - an older Lantern's hold.key is sealed under the passphrase and the plain file goes;
#   - --scuttle destroys the sealed key;
#   - --open takes ids, a file of ids or all, and refuses anything else;
#   - lantern_disk.py finds nothing in the clear in the folder, episodes included.
# One PASS or FAIL line a check. Usage: lantern_hold.sh [<wav>]   (default /tmp/tone.wav)
# Run under the test network's lock. It joins no network: its relay is a closed port.
R=$(cd "$(dirname "$0")/../.." && pwd)
L="$R/target/release/lantern"
WAV=${1:-/tmp/tone.wav}
W=$(mktemp -d /tmp/lantern-hold.XXXXXX)
PORT=${PYRATE_HOLD_TEST_PORT:-8377}
unset PYRATE_HOLD_KEY PYRATE_HOLD_PASS_FILE PYRATE_HOLD_PASS_CMD PYRATE_LAN_ANY
# Kept off the test network, so its station never reaches the dial the other suites see.
export PYRATE_RELAYS=http://127.0.0.1:9
mkdir -p "$W/usb" "$W/music"
cp "$WAV" "$W/music/Tone - Hold Tone.wav"
printf 'four random words for the hold\n' > "$W/usb/pass"
printf 'not the passphrase at all\n' > "$W/usb/wrong"
SEED="Hold Test|A station for the hold test|$W/music|talk"

ok() { if [ "$1" = 0 ]; then echo "PASS $2"; else echo "FAIL $2"; fi; }
# lit <log>: waits for a Lantern to light and set its recording, at most 90 seconds.
lit() { for _ in $(seq 1 90); do grep -q "station Hold Test:" "$1" 2>/dev/null && grep -q "set Hold Tone\|LANTERN LIT" "$1" && break; sleep 1; done; }
sid() { grep "station Hold Test:" "$1" | grep -o "[0-9a-f]\{64\}" | head -1; }
stop() { kill "$1" 2>/dev/null; wait "$1" 2>/dev/null; }
# A refusal comes quickly, with a non-zero exit, and leaves the folder as it was.
refuses() { timeout 60 "$L" "$@" > "$W/refusal.log" 2>&1; c=$?; [ $c -ne 0 ] && [ $c -ne 124 ]; }
listing() { (cd "$1" && find . -type f | sort | xargs -r sha256sum); }

# 1. No passphrase: nothing opened, nothing made.
refuses --dir "$W/a"; r=$?
grep -q "opens nothing" "$W/refusal.log" && [ -z "$(ls -A "$W/a" 2>/dev/null)" ]; ok $((r + $?)) "with no passphrase a new Lantern opens nothing and makes no file"

# 2. A passphrase from a file on another disk: a station, a feed and an episode.
PYRATE_HOLD_PASS_FILE="$W/usb/pass" "$L" --dir "$W/a" --show-ids --seed "$SEED" --http "127.0.0.1:$PORT" > "$W/a1.log" 2>&1 &
P=$!
lit "$W/a1.log"
for _ in $(seq 1 60); do grep -q "set Hold Tone" "$W/a1.log" && break; sleep 1; done
S1=$(sid "$W/a1.log")
[ -n "$S1" ] && [ -f "$W/a/hold.sealed" ] && [ ! -e "$W/a/hold.key" ]; ok $? "a passphrase from a file starts the Lantern with its key sealed and none in the clear"
EP=$(curl -s "http://127.0.0.1:$PORT/feed/$S1" | grep -o "http://[^\"]*/episode/[0-9a-f]*/[0-9a-f]*\.mp3" | head -1)
curl -s -r 0-99 -o "$W/ep-head" "$EP" ; curl -s -o "$W/ep-all" "$EP"
H=$(head -c 3 "$W/ep-head" | od -An -tx1 | tr -d ' \n')
{ [ "$H" = "494433" ] || [ "$(echo "$H" | cut -c1-2)" = "ff" ]; } && [ "$(stat -c %s "$W/ep-head")" = 100 ] && [ "$(stat -c %s "$W/ep-all")" -gt 100000 ] && cmp -n 100 "$W/ep-head" "$W/ep-all"; ok $? "an episode is made, kept sealed, and served whole and by range ($H)"
ls "$W/a/episodes/"*.ep > /dev/null 2>&1 && ! ls "$W/a/episodes/"*.mp3 > /dev/null 2>&1; ok $? "the episode lies on disk sealed, under a name that says nothing"
grep -q "^node id" "$W/a1.log"; ok $? "with --show-ids the log names the node"
stop $P
python3 "$R/tools/test/lantern_disk.py" "$W/a" --needle "Hold Test" --needle "Hold Tone" --needle "$S1" > "$W/disk-a.txt"; r=$?
cat "$W/disk-a.txt" | sed 's/^/  /'
ok $r "the data folder holds nothing in the clear but known public artefacts"

# The walk itself finds what lies in the clear: a key, a plain note, an episode by its name.
mkdir -p "$W/bad/notes" "$W/bad/episodes"
head -c 32 /dev/urandom > "$W/bad/hold.key"
printf '{"kept":["Hold Test"]}' > "$W/bad/notes/$(printf %064d 0)"
cp "$W/ep-all" "$W/bad/episodes/$S1-00.mp3"
python3 "$R/tools/test/lantern_disk.py" "$W/bad" --needle "Hold Test" > "$W/disk-bad.txt"; r=$?
[ $r -ne 0 ] && [ "$(grep -c '^FAIL' "$W/disk-bad.txt")" -ge 4 ]; ok $? "the walk fails on a key, a note and an episode left in the clear"

# 3. Without the passphrase, or with a wrong one: nothing opened, nothing changed.
BEFORE=$(listing "$W/a")
refuses --dir "$W/a" --hold-key-on-disk; r=$?
grep -q "sealed under a passphrase and none was given" "$W/refusal.log"; ok $((r + $?)) "with its hold sealed and no passphrase, it opens nothing, even with --hold-key-on-disk"
PYRATE_HOLD_PASS_FILE="$W/usb/wrong" refuses --dir "$W/a"; r=$?
grep -q "does not open" "$W/refusal.log"; ok $((r + $?)) "a wrong passphrase opens nothing"
PYRATE_HOLD_PASS_FILE="$W/nowhere/pass" refuses --dir "$W/a"; r=$?
grep -q "Is its disk mounted" "$W/refusal.log"; ok $((r + $?)) "a passphrase file whose disk is not there opens nothing and says so"
[ "$(listing "$W/a")" = "$BEFORE" ]; ok $? "and none of those refusals changed a file"

# 4. The same hold through a file descriptor, and through a command.
"$L" --dir "$W/a" --show-ids --seed "$SEED" --hold-pass-fd 3 3< "$W/usb/pass" > "$W/a2.log" 2>&1 &
P=$!; lit "$W/a2.log"; stop $P
[ "$(sid "$W/a2.log")" = "$S1" ]; ok $? "a passphrase through a file descriptor opens the same hold"
PYRATE_HOLD_PASS_CMD="cat '$W/usb/pass'" "$L" --dir "$W/a" --show-ids --seed "$SEED" > "$W/a3.log" 2>&1 &
P=$!; lit "$W/a3.log"; stop $P
[ "$(sid "$W/a3.log")" = "$S1" ]; ok $? "a passphrase from a command, as a keyring's tool gives it, opens the same hold"

# 5. An older Lantern's hold.key: sealed under the passphrase, and the plain file goes.
"$L" --dir "$W/b" --show-ids --hold-key-on-disk --seed "$SEED" > "$W/b1.log" 2>&1 &
P=$!; lit "$W/b1.log"; stop $P
S2=$(sid "$W/b1.log")
[ -f "$W/b/hold.key" ] && grep -q "WARNING the hold key lies in the clear" "$W/b1.log"; ok $? "--hold-key-on-disk keeps the key in the clear and warns at the start"
PYRATE_HOLD_PASS_FILE="$W/usb/pass" "$L" --dir "$W/b" --show-ids --seed "$SEED" > "$W/b2.log" 2>&1 &
P=$!; lit "$W/b2.log"; stop $P
[ -n "$S2" ] && [ "$(sid "$W/b2.log")" = "$S2" ] && [ ! -e "$W/b/hold.key" ] && [ -f "$W/b/hold.sealed" ] && grep -q "copy in the clear at .* is gone" "$W/b2.log"; ok $? "an old hold.key is sealed under the passphrase, the plain file goes, and the same hold opens"
python3 "$R/tools/test/lantern_disk.py" "$W/b" --needle "Hold Test" --needle "$S2" > "$W/disk-b.txt"; ok $? "and after the move the folder holds nothing in the clear"

# 6. --open: ids, a file of ids, all; anything else stops the Lantern before it starts.
refuses --dir "$W/c" --hold-key-on-disk --open nonsense; r=$?
grep -q "neither a station id nor a file" "$W/refusal.log"; ok $((r + $?)) "--open refuses what is neither an id nor a file of ids"
"$L" --dir "$W/c" --hold-key-on-disk --open "$R/browser/harbour.json" --open "$S1" > "$W/c1.log" 2>&1 &
P=$!; for _ in $(seq 1 30); do grep -q "kept open" "$W/c1.log" && break; sleep 1; done; stop $P
grep -q "^11 stations named open" "$W/c1.log" && ! grep -q "^node id" "$W/c1.log"; ok $? "--open takes HOSTIS's harbour list and an id, and the log names neither nor the node"

# 7. Scuttle: the sealed key goes, and the hold opens no more.
"$L" --dir "$W/b" --scuttle > /dev/null 2>&1
[ ! -e "$W/b/hold.sealed" ] && [ ! -e "$W/b/hold.key" ]; ok $? "--scuttle destroys the sealed hold key"
PYRATE_HOLD_PASS_FILE="$W/usb/pass" refuses --dir "$W/b"; r=$?
grep -q "Nothing was started" "$W/refusal.log"; ok $((r + $?)) "and the scuttled hold never opens again, nor is a new key made over it"

rm -rf "$W"
