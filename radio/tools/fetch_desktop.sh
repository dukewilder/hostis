#!/bin/sh
# Brings the computers' builds (Windows, macOS, Linux) of one release commit from the radio
# desktop workflow into landing/desktop/, for landing/build.py to publish with checksums,
# torrents and, once two holders have signed them (release sign-all), their signatures.
#   fetch_desktop.sh <release commit> [run id]
# A run built from any other commit is refused, whoever started it.
set -e
R=$(cd "$(dirname "$0")/.." && pwd)
WANT=$(git -C "$R" rev-parse --verify "${1:?usage: fetch_desktop.sh <release commit> [run id]}^{commit}" 2>/dev/null) || { echo "$1 is not a commit here"; exit 1; }
RUN=${2:-$(gh run list --workflow radio-desktop.yml --branch radio --status success --commit "$WANT" -L 1 --json databaseId --jq '.[0].databaseId')}
[ -n "$RUN" ] || { echo "no successful desktop run for $WANT yet"; exit 1; }
HEAD=$(gh run view "$RUN" --json headSha --jq .headSha)
[ "$HEAD" = "$WANT" ] || { echo "refused: run $RUN was built from $HEAD, not the release commit $WANT"; exit 1; }
rm -rf "$R/landing/desktop"; mkdir -p "$R/landing/desktop"
gh run download "$RUN" -D "$R/landing/desktop"
find "$R/landing/desktop" -type f \( -name '*.zip' -o -name '*.tar.gz' \) -exec mv {} "$R/landing/desktop/" \;
find "$R/landing/desktop" -mindepth 1 -type d -empty -delete
echo "$HEAD" > "$R/landing/desktop/commit"
ls -la "$R/landing/desktop"
