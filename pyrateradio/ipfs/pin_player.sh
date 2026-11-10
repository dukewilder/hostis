#!/bin/sh
# Keeps the open player alive on IPFS. Any IPFS node can run it, hourly from cron:
#   sh pin_player.sh            (uses the ipfs on PATH; IPFS_PATH as usual)
# It reads the list of every open player HOSTIS has published, and for each one this node does
# not yet hold, fetches its CAR from hostis.black and pins it. Every node that runs it is one
# more place the player lives. A CAR is checked by IPFS itself: its blocks must hash to its name.
set -e
BASE=${BASE:-https://hostis.black/pyrateradio/ipfs}
LIST=$(curl -fsS "$BASE/players.txt")
for cid in $LIST; do
  case "$cid" in bafy*) ;; *) continue ;; esac
  if ipfs pin ls --type=recursive "$cid" >/dev/null 2>&1; then continue; fi
  tmp=$(mktemp)
  if curl -fsS -o "$tmp" "$BASE/$cid.car"; then
    ipfs dag import --pin-roots "$tmp" >/dev/null && echo "pinned $cid"
  else
    # An old CAR may have left the site; ask the network for it instead.
    ipfs pin add --progress=false "$cid" >/dev/null 2>&1 && echo "pinned $cid from the network" || echo "could not reach $cid"
  fi
  rm -f "$tmp"
done
