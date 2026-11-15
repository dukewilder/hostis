#!/bin/sh
# Keeps the open player alive on IPFS. Any IPFS node can run it, hourly from cron:
#   sh pin_player.sh            (uses the ipfs on PATH; IPFS_PATH as usual)
# It reads the list of every open player HOSTIS has published, and for each one this node does
# not yet hold, fetches its CAR from hostis.black and pins it. Every node that runs it is one
# more place the player lives. A CAR is checked by IPFS itself: its blocks must hash to its name.
set -e
BASE=${BASE:-https://hostis.black/pyrateradio/ipfs}
# curl if there is one, wget if not (the kubo image has only wget).
get() { if command -v curl >/dev/null 2>&1; then curl -fsS -o "$2" "$1"; else wget -q -O "$2" "$1"; fi; }
HERE=$(cd "$(dirname "$0")" && pwd)
RAW=$(mktemp)
get "$BASE/players.txt" "$RAW"
LIST=$(cat "$RAW")
for cid in $LIST; do
  case "$cid" in bafy*) ;; *) continue ;; esac
  if ipfs pin ls --type=recursive "$cid" >/dev/null 2>&1; then continue; fi
  tmp=$(mktemp)
  if get "$BASE/$cid.car" "$tmp"; then
    ipfs dag import --pin-roots "$tmp" >/dev/null && echo "pinned $cid"
  else
    # An old CAR may have left the site; ask the network for it instead.
    ipfs pin add --progress=false "$cid" >/dev/null 2>&1 && echo "pinned $cid from the network" || echo "could not reach $cid"
  fi
  rm -f "$tmp"
done
# The app, the computers' builds and the source, by the IPFS names files.txt gives them: each
# is fetched from the site once, added, and kept only if it hashes to the name it was listed by.
FILES=$(get "$BASE/files.txt" - 2>/dev/null || true)
echo "$FILES" | while read -r cid name; do
  case "$cid" in bafy*) ;; *) continue ;; esac
  if ipfs pin ls --type=recursive "$cid" >/dev/null 2>&1; then continue; fi
  tmp=$(mktemp)
  if get "${BASE%/ipfs}/$name" "$tmp" && [ "$(ipfs add --cid-version=1 -Q "$tmp")" = "$cid" ]; then
    echo "pinned $name"
  else
    ipfs pin rm "$(ipfs add --only-hash --cid-version=1 -Q "$tmp" 2>/dev/null)" >/dev/null 2>&1 || true
    echo "could not pin $name"
  fi
  rm -f "$tmp"
done
# With IPNS_KEY set (the name of a key in this node, made once with `ipfs key gen pyrate-player`),
# the newest player is also published under that key's fixed name, so its address, and with it
# everything a browser keeps for the radio, stays the same from one release to the next. Only a
# list two of the three release keys signed (players.sigs, `release sign-all`) moves the name:
# verify_players.py checks it here when this machine has Python, or the host checks it first and
# passes what it found as SIGNED_CID (tools/dukewilder-rebuild.md). A list that fails, or one
# shorter than the last one named, leaves the name on the last signed player.
if [ -n "$IPNS_KEY" ]; then
  STATE=${STATE:-${IPFS_PATH:-$HOME/.ipfs}/pyrate-named}
  last=$(cat "$STATE" 2>/dev/null || true)
  signed=${SIGNED_CID:-}
  if [ -z "$signed" ] && command -v python3 >/dev/null 2>&1 && [ -f "$HERE/verify_players.py" ]; then
    sigs=$(mktemp)
    get "$BASE/players.sigs" "$sigs" 2>/dev/null && signed=$(python3 "$HERE/verify_players.py" "$RAW" "$sigs" || true)
    rm -f "$sigs"
  fi
  name=""
  if [ -n "$signed" ]; then
    n=${signed%% *}; cid=${signed##* }
    if ! echo "$LIST" | grep -qx "$cid"; then
      echo "refused: $cid is not in players.txt"
    elif [ -n "$last" ] && [ "$n" -lt "${last%% *}" ]; then
      echo "refused: the signed list holds fewer players than the one last named"
    else
      name=$cid; echo "$n $cid" > "$STATE"
    fi
  else
    echo "players.txt is not signed by two release keys"
  fi
  if [ -z "$name" ] && [ -n "$last" ]; then name=${last##* }; echo "the name stays on $name"; fi
  [ -n "$name" ] && ipfs name publish --key="$IPNS_KEY" --lifetime=72h "/ipfs/$name" >/dev/null && echo "named $name"
fi
rm -f "$RAW"
