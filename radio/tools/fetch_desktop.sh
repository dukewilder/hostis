#!/bin/sh
# Brings the computers' builds (Windows, macOS, Linux) from the radio desktop workflow's latest
# successful run into landing/desktop/, for landing/build.py to publish with checksums and
# torrents. fetch_desktop.sh [run id]
set -e
R=$(cd "$(dirname "$0")/.." && pwd)
RUN=${1:-$(gh run list --workflow radio-desktop.yml --branch radio --status success -L 1 --json databaseId --jq '.[0].databaseId')}
[ -n "$RUN" ] || { echo "no successful desktop run yet"; exit 1; }
rm -rf "$R/landing/desktop"; mkdir -p "$R/landing/desktop"
gh run download "$RUN" -D "$R/landing/desktop"
find "$R/landing/desktop" -type f \( -name '*.zip' -o -name '*.tar.gz' \) -exec mv {} "$R/landing/desktop/" \;
find "$R/landing/desktop" -mindepth 1 -type d -empty -delete
gh run view "$RUN" --json headSha --jq .headSha > "$R/landing/desktop/commit"
ls -la "$R/landing/desktop"
