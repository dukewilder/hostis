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
# The computers' builds, if the landing carried them, beside the Android app; older ones go.
if ls "$R"/landing/out/pyrate-radio-*-windows.zip >/dev/null 2>&1; then
  git rm -q --ignore-unmatch pyrateradio/pyrate-radio-*-windows.zip* pyrateradio/pyrate-radio-*-macos.zip* pyrateradio/pyrate-radio-*-linux-x64.tar.gz* >/dev/null
fi
for f in "$R"/landing/out/pyrate-radio-*-windows.zip* "$R"/landing/out/pyrate-radio-*-macos.zip* "$R"/landing/out/pyrate-radio-*-linux-x64.tar.gz*; do
  [ -e "$f" ] && cp "$f" pyrateradio/
done
rm -rf pyrateradio/listen
cp -r "$R/landing/out/listen" pyrateradio/listen
# The open player's CAR, kept beside every earlier one still on the site (the newest twelve),
# with the list of every open player ever published, for pinners (tools/pin_player.sh).
CID=$(python3 -c "import json;print(json.load(open('$R/browser/www/open.json'))['cid'])")
mkdir -p pyrateradio/ipfs
touch pyrateradio/ipfs/players.txt
# The player this build names, and the one the app's own links name, if it differs.
APPCID=$(grep -o 'bafy[a-z2-7]*' "$R/app/assets/open_players.txt" | head -1)
for c in $APPCID $CID; do
  [ -f "$R/browser/www/cars/$c.car" ] || continue
  cp "$R/browser/www/cars/$c.car" "pyrateradio/ipfs/$c.car"
  grep -qx "$c" pyrateradio/ipfs/players.txt || echo "$c" >> pyrateradio/ipfs/players.txt
done
ls -t pyrateradio/ipfs/*.car | tail -n +25 | xargs -r git rm -q --cached --ignore-unmatch
ls -t pyrateradio/ipfs/*.car | tail -n +25 | xargs -r rm -f
cp "$R/tools/pin_player.sh" pyrateradio/ipfs/pin_player.sh
# The source at this commit, as a tarball and a torrent, so the code outlives any one host.
mkdir -p pyrateradio/source
git rm -q --ignore-unmatch "pyrateradio/source/pyrate-radio-src-*" >/dev/null 2>&1 || true
rm -f pyrateradio/source/pyrate-radio-src-*
SRC=pyrate-radio-src-$H.tar.gz
git -C "$R" archive --format=tar.gz --prefix=pyrate-radio-src/ HEAD . > "pyrateradio/source/$SRC"
python3 "$R/tools/torrent.py" "pyrateradio/source/$SRC" "pyrateradio/source/$SRC.torrent" "https://hostis.black/pyrateradio/source/$SRC" > pyrateradio/source/magnet.txt
# Every file here by its IPFS name too (computed, not stored: any node that pins it from this
# site gets the same name), for pin_player.sh and the bearings.
: > pyrateradio/ipfs/files.txt
for f in pyrateradio/*.apk pyrateradio/pyrate-radio-*.zip pyrateradio/pyrate-radio-*.tar.gz "pyrateradio/source/$SRC"; do
  [ -e "$f" ] || continue
  echo "$(IPFS_PATH="$R/browser/www/.ipfs" ipfs add --only-hash --cid-version=1 -Q "$f") ${f#pyrateradio/}" >> pyrateradio/ipfs/files.txt
done
# An unsigned draft of the bearings, for the release keys to sign (release sign-paper).
python3 "$R/tools/bearings.py" "$R" pyrateradio > pyrateradio/ipfs/bearings-draft.json
# The newest open player as plain files, for any Lantern to serve at /player/ (LANTERN.md).
tar -C "$R/browser/www/dist-open" -czf pyrateradio/ipfs/open-player.tar.gz .
git add -A pyrateradio
git commit -q -m "pyrateradio: $1 (radio $H)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01PDUodF7caB4z7mhG1vnrX5"
git push -q origin site-pyrateradio:main
git log --oneline -1
