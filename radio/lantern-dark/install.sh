#!/usr/bin/env bash
# Pyrate Radio: a dark Lantern beside Tor, in one command. No rights reserved.
#
#   curl -fsSL https://raw.githubusercontent.com/dukewilder/hostis/radio/radio/lantern-dark/install.sh -o dark.sh && bash dark.sh
#
# It asks one thing, once: the passphrase that seals the Lantern's hold key. The passphrase is
# kept in a file only root can read (hold-pass beside compose.yaml) and is never printed. To keep
# it on a removable disk instead, put it in a file there first and run with
#   PYRATE_HOLD_PASS_PATH=/media/usb/pyrate/hold-pass bash dark.sh
# Then it builds Tor and the Lantern and starts them with docker compose. No port is published.
# Run it again at any time to update: the passphrase and everything kept stay.
# PYRATE_DRY=1 shows every step and changes nothing. PYRATE_DARK_DIR names where the source goes.
set -euo pipefail

REPO=https://github.com/dukewilder/hostis
BRANCH=radio
DRY=${PYRATE_DRY:-0}

say() { printf '\n\033[1m%s\033[0m\n' "$*"; }
die() { printf '\n\033[1;31m%s\033[0m\n' "$*" >&2; exit 1; }
run() { if [ "$DRY" = 1 ]; then printf '  + %s\n' "$*" >&2; else "$@"; fi; }

if [ "$DRY" != 1 ] && [ "$(id -u)" != 0 ]; then
  if command -v sudo >/dev/null && [ -f "$0" ]; then exec sudo PYRATE_HOLD_PASS_PATH="${PYRATE_HOLD_PASS_PATH:-}" PYRATE_DARK_DIR="${PYRATE_DARK_DIR:-}" bash "$0" "$@"; fi
  die "Run this as root (log in as root, or put sudo in front)."
fi

# ---- Docker --------------------------------------------------------------------------------
if ! command -v docker >/dev/null || ! docker compose version >/dev/null 2>&1; then
  # shellcheck disable=SC1091
  . /etc/os-release 2>/dev/null || true
  case " ${ID:-} ${ID_LIKE:-} " in
    *" debian "* | *" ubuntu "*)
      say "Docker"
      run apt-get update -qq
      run env DEBIAN_FRONTEND=noninteractive apt-get install -y -qq docker.io git
      run env DEBIAN_FRONTEND=noninteractive apt-get install -y -qq docker-compose-v2 ||
        run env DEBIAN_FRONTEND=noninteractive apt-get install -y -qq docker-compose-plugin ||
        die "Docker is in, but not its compose plugin. Install it (docs.docker.com/compose/install) and run this again."
      run systemctl enable --now docker
      ;;
    *) [ "$DRY" = 1 ] || die "This needs Docker with its compose plugin. Install both and run this again." ;;
  esac
fi

# ---- The source ----------------------------------------------------------------------------
# From a checkout, it uses the checkout. Otherwise it fetches the radio branch.
HERE=$(cd "$(dirname "$0")" 2>/dev/null && pwd || true)
if [ -n "$HERE" ] && [ -f "$HERE/compose.yaml" ] && [ -f "$HERE/../Dockerfile" ]; then
  DIR=$HERE
else
  SRC=${PYRATE_DARK_DIR:-/opt/pyrate-dark/hostis}
  say "The source ($SRC)"
  if [ -d "$SRC/.git" ]; then
    run git -C "$SRC" fetch -q --depth 1 origin "$BRANCH"
    run git -C "$SRC" reset -q --hard FETCH_HEAD
  else
    run mkdir -p "$(dirname "$SRC")"
    run git clone -q --depth 1 --branch "$BRANCH" "$REPO" "$SRC"
  fi
  DIR=$SRC/radio/lantern-dark
fi

# ---- The passphrase ------------------------------------------------------------------------
# Asked once, unseen, twice to be sure, and written where only root reads it. Never printed.
PASS_PATH=${PYRATE_HOLD_PASS_PATH:-}
if [ -z "$PASS_PATH" ] && [ -f "$DIR/.env" ]; then
  PASS_PATH=$(sed -n 's/^PYRATE_HOLD_PASS_PATH=//p' "$DIR/.env")
fi
PASS_PATH=${PASS_PATH:-$DIR/hold-pass}
# read_secret PROMPT: one line, unseen, from the terminal; from standard input when there is none.
read_secret() {
  local a=""
  if [ -r /dev/tty ] && [ -w /dev/tty ] && { : </dev/tty; } 2>/dev/null; then
    printf '%s' "$1" >/dev/tty
    IFS= read -r -s a </dev/tty || true
    printf '\n' >/dev/tty
  else
    IFS= read -r a || true
  fi
  printf '%s' "$a"
}
if [ -s "$PASS_PATH" ]; then
  say "The passphrase: kept in $PASS_PATH"
else
  say "The passphrase"
  echo "Everything the Lantern keeps is sealed under a passphrase. Twelve characters or more;"
  echo "four or five random words are good. Lose it and everything the Lantern kept is lost."
  p1=$(read_secret "Passphrase: ")
  [ ${#p1} -ge 12 ] || die "That is shorter than twelve characters. Nothing was changed."
  p2=$(read_secret "Again: ")
  [ "$p1" = "$p2" ] || die "The two did not match. Nothing was changed."
  if [ "$DRY" = 1 ]; then
    printf '  + write %s (root only, never printed)\n' "$PASS_PATH" >&2
  else
    (umask 077 && mkdir -p "$(dirname "$PASS_PATH")" && printf '%s\n' "$p1" >"$PASS_PATH")
    chmod 600 "$PASS_PATH"
    chown 0:0 "$PASS_PATH" 2>/dev/null || true
  fi
  unset p1 p2
fi
if [ "$DRY" = 1 ]; then
  printf '  + write %s/.env naming the passphrase file\n' "$DIR" >&2
else
  printf 'PYRATE_HOLD_PASS_PATH=%s\n' "$PASS_PATH" >"$DIR/.env"
  chmod 600 "$DIR/.env"
fi

# ---- Tor and the Lantern -------------------------------------------------------------------
say "Tor and the Lantern (the first build takes 10 to 30 minutes and shows little while it works)"
run docker compose --project-directory "$DIR" -f "$DIR/compose.yaml" up -d --build
run docker builder prune -af >/dev/null 2>&1 || true
run docker image prune -f >/dev/null 2>&1 || true

NODE=""
if [ "$DRY" != 1 ]; then
  for _ in $(seq 1 30); do
    NODE=$(docker compose --project-directory "$DIR" -f "$DIR/compose.yaml" exec -T lantern lantern --dir /data --id 2>/dev/null | sed -n 's/^node id: \([0-9a-f]\{64\}\)$/\1/p' || true)
    [ -n "$NODE" ] && break
    sleep 4
  done
fi

say "Done."
echo "Lantern:  ${NODE:-(not started yet: docker compose -f $DIR/compose.yaml logs lantern)}"
echo
echo "It runs dark through Tor: no port open, no address published, nothing in the DHT. It holds,"
echo "backs up and passes on sealed sound for the network through relays, and carries every"
echo "station blind. A dark Lantern serves fewer listeners than an open one, since only relays"
echo "reach it; an open Lantern (LANTERN.md) serves more and is seen to."
echo "Keep a copy of the passphrase somewhere safe. Run this again to update."
