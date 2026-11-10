#!/bin/sh
# Builds the browser radio and the landing, and deploys them to hostis.black/pyrateradio/.
# deploy_site.sh "<what landed>"   (the APK offered stays the published one named below)
set -e
R=$(cd "$(dirname "$0")/.." && pwd)
SITE=${SITE:-/home/claude/main-wt}
APK=${APK:-$SITE/pyrateradio/pyrate-radio-0.8.0.apk}
VER=${VER:-0.8.0}
sh "$R/browser/build.sh" > /tmp/browser-build.log 2>&1
python3 "$R/landing/build.py" "$APK" "$VER" | tail -1
H=$(git -C "$R" rev-parse --short HEAD)
cd "$SITE"
git fetch -q origin main
git rebase -q origin/main
cp "$R/landing/out/index.html" pyrateradio/index.html
rm -rf pyrateradio/listen
cp -r "$R/landing/out/listen" pyrateradio/listen
# The open player's CAR, kept beside every earlier one still on the site (the newest twelve),
# with the list of every open player ever published, for pinners (tools/pin_player.sh).
CID=$(python3 -c "import json;print(json.load(open('$R/browser/www/open.json'))['cid'])")
mkdir -p pyrateradio/ipfs
cp "$R/browser/www/open.car" "pyrateradio/ipfs/$CID.car"
touch pyrateradio/ipfs/players.txt
grep -qx "$CID" pyrateradio/ipfs/players.txt || echo "$CID" >> pyrateradio/ipfs/players.txt
ls -t pyrateradio/ipfs/*.car | tail -n +13 | xargs -r git rm -q --cached --ignore-unmatch
ls -t pyrateradio/ipfs/*.car | tail -n +13 | xargs -r rm -f
cp "$R/tools/pin_player.sh" pyrateradio/ipfs/pin_player.sh
git add -A pyrateradio
git commit -q -m "pyrateradio: $1 (radio $H)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01PDUodF7caB4z7mhG1vnrX5"
git push -q origin site-pyrateradio:main
git log --oneline -1
