#!/usr/bin/env bash
# Pyrate Radio: a relay and a Lantern on a fresh Debian or Ubuntu server. No rights reserved.
#
#   curl -fsSL https://raw.githubusercontent.com/dukewilder/hostis/radio/radio/tools/vps/install.sh -o pyrate.sh && bash pyrate.sh
#
# It asks one thing, the relay's name, and prints the relay's address and the Lantern's id at
# the end. Run it again at any time: it changes only what is out of date, and that is also how
# to update. DOMAIN=relay.example.org skips the question. PYRATE_DRY=1 shows every step and
# changes nothing.
set -euo pipefail

IROH_VERSION=1.4.1 # the iroh in radio/Cargo.lock
SHA_x86_64=75254641e3fad7fcbac6dee4d485556fbe06ef708340a9d8810469270ba626b8
SHA_aarch64=3fc0c2d67c1cd66250bf82f3e2f17a985b7081a3d272186bad9d63e3f2347b38
REPO=https://github.com/dukewilder/hostis
BRANCH=radio
SRC=/opt/pyrate/hostis
CONF=/etc/pyrate/relay.toml
UNIT=/etc/systemd/system/pyrate-relay.service
QUIC_PORT=7842
DRY=${PYRATE_DRY:-0}

say() { printf '\n\033[1m%s\033[0m\n' "$*"; }
die() { printf '\n\033[1;31m%s\033[0m\n' "$*" >&2; exit 1; }
run() { if [ "$DRY" = 1 ]; then printf '  + %s\n' "$*" >&2; else "$@"; fi; }
ask() { local a=""; read -r -p "$1" a </dev/tty 2>/dev/null || true; printf '%s' "$a"; }
# write PATH MODE < content: replaces the file only when it differs. True when it changed.
write() {
  local tmp; tmp=$(mktemp)
  cat >"$tmp"
  if [ "$DRY" = 1 ]; then printf '  + write %s\n' "$1"; sed 's/^/      /' "$tmp"; rm -f "$tmp"; return 0; fi
  if [ -f "$1" ] && cmp -s "$tmp" "$1"; then rm -f "$tmp"; return 1; fi
  install -D -m "$2" "$tmp" "$1"; rm -f "$tmp"
}

# ---- Who and where -------------------------------------------------------------------------
if [ "$DRY" != 1 ] && [ "$(id -u)" != 0 ]; then
  if command -v sudo >/dev/null && [ -f "$0" ]; then exec sudo DOMAIN="${DOMAIN:-}" bash "$0" "$@"; fi
  die "Run this as root (log in as root, or put sudo in front)."
fi
# shellcheck disable=SC1091
. /etc/os-release 2>/dev/null || true
case " ${ID:-} ${ID_LIKE:-} " in *" debian "* | *" ubuntu "*) ;; *) [ "$DRY" = 1 ] || die "This needs Debian or Ubuntu." ;; esac
ARCH=$(uname -m)
case $ARCH in x86_64) SHA=$SHA_x86_64 ;; aarch64) SHA=$SHA_aarch64 ;; *) die "No relay build for $ARCH." ;; esac

public_ip() {
  local ip
  ip=$(ip -4 route get 1.1.1.1 2>/dev/null | awk '{for (i = 1; i < NF; i++) if ($i == "src") { print $(i + 1); exit }}' || true)
  case $ip in
    "" | 10.* | 127.* | 192.168.* | 172.1[6-9].* | 172.2[0-9].* | 172.3[01].* | 100.6[4-9].* | 100.[7-9][0-9].* | 100.1[01][0-9].* | 100.12[0-7].*)
      ip=$(curl -4 -fsS --max-time 10 https://ipv4.icanhazip.com 2>/dev/null | tr -d '[:space:]' || true) ;;
  esac
  printf '%s' "$ip"
}
IP=$(public_ip)
[ -n "$IP" ] || { [ "$DRY" = 1 ] && IP=203.0.113.7; } || die "Could not find this server's public address."

DOMAIN=$(printf '%s' "${DOMAIN:-}" | tr '[:upper:]' '[:lower:]' | sed -e 's#^[a-z]*://##' -e 's#/.*$##')
if [ -z "$DOMAIN" ]; then
  # A name from an earlier run is kept.
  [ -f "$CONF" ] && DOMAIN=$(sed -n 's/^hostname = "\(.*\)"$/\1/p' "$CONF")
  say "Your relay needs a name that points at this server ($IP), like relay.example.org."
  echo "No name of your own? Press Enter for a free one made from this server's address."
  [ -n "$DOMAIN" ] && echo "Press Enter to keep $DOMAIN."
  a=$(ask "Name: " | tr '[:upper:]' '[:lower:]' | sed -e 's#^[a-z]*://##' -e 's#/.*$##')
  [ -n "$a" ] && DOMAIN=$a
fi
FREE=()
if [ -z "$DOMAIN" ] || [[ $DOMAIN == *.sslip.io || $DOMAIN == *.nip.io ]]; then
  # Two free names, each its own Let's Encrypt allowance: the second if the first is spent.
  FREE=("${IP//./-}.sslip.io" "$IP.nip.io")
  [ -n "$DOMAIN" ] && [ "$DOMAIN" = "${FREE[1]}" ] && FREE=("${FREE[1]}" "${FREE[0]}")
  DOMAIN=${FREE[0]}
fi
[[ $DOMAIN =~ ^[a-z0-9]([a-z0-9-]*[a-z0-9])?(\.[a-z0-9]([a-z0-9-]*[a-z0-9])?)+$ ]] || die "\"$DOMAIN\" is not a name."

if [ ${#FREE[@]} = 0 ] && [ "$DRY" != 1 ]; then
  seen=$(getent ahostsv4 "$DOMAIN" 2>/dev/null | awk '{print $1}' | sort -u | tr '\n' ' ' || true)
  if [ -z "$seen" ]; then
    die "$DOMAIN does not point anywhere yet. Add an A record for it with $IP, wait a few minutes, and run this again."
  elif [[ " $seen" != *" $IP "* ]]; then
    say "$DOMAIN points at $seen, not at this server ($IP)."
    echo "If the name is on Cloudflare, click the orange cloud beside it so it turns grey (DNS only)."
    [ "$(ask 'Carry on anyway? [y/N] ')" = y ] || exit 1
  fi
fi
say "Relay name: $DOMAIN"

# ---- Packages, swap, firewall --------------------------------------------------------------
say "Packages"
PKGS=(ca-certificates curl git ufw tar)
command -v docker >/dev/null || PKGS+=(docker.io)
missing=()
for p in "${PKGS[@]}"; do dpkg -s "$p" >/dev/null 2>&1 || missing+=("$p"); done
if [ ${#missing[@]} -gt 0 ]; then
  run apt-get update -qq
  run env DEBIAN_FRONTEND=noninteractive apt-get install -y -qq "${missing[@]}"
fi
if ! docker buildx version >/dev/null 2>&1 && apt-cache show docker-buildx >/dev/null 2>&1; then
  run env DEBIAN_FRONTEND=noninteractive apt-get install -y -qq docker-buildx || true
fi
run systemctl enable --now docker

# Building the Lantern wants about 4 GB of memory; a small server borrows it from the disk.
mem_kb=$(awk '/^(MemTotal|SwapTotal):/ {s += $2} END {print s + 0}' /proc/meminfo)
if [ "$mem_kb" -lt 3800000 ] && [ ! -f /swapfile ]; then
  swap=4G
  [ "$(df -Pk / | awk 'NR == 2 {print $4}')" -gt 16000000 ] || swap=2G
  say "Swap ($swap, for building)"
  run fallocate -l "$swap" /swapfile
  run chmod 600 /swapfile
  run mkswap /swapfile
  run swapon /swapfile
  grep -q '^/swapfile ' /etc/fstab 2>/dev/null || run sh -c 'echo "/swapfile none swap sw 0 0" >> /etc/fstab'
fi

say "Firewall: SSH, 80 and 443, and $QUIC_PORT for QUIC. Nothing else comes in."
ssh_ports=$(sshd -T 2>/dev/null | awk '$1 == "port" {print $2}' | sort -u || true)
for p in ${ssh_ports:-22}; do run ufw allow "$p/tcp" >/dev/null; done
run ufw allow 80/tcp >/dev/null
run ufw allow 443/tcp >/dev/null
run ufw allow "$QUIC_PORT/udp" >/dev/null
run ufw default deny incoming >/dev/null
run ufw default allow outgoing >/dev/null
run ufw --force enable >/dev/null

# ---- The relay -----------------------------------------------------------------------------
say "Relay (iroh-relay $IROH_VERSION)"
if ! /usr/local/bin/iroh-relay --version 2>/dev/null | grep -qx "iroh-relay $IROH_VERSION"; then
  url="https://github.com/n0-computer/iroh/releases/download/v$IROH_VERSION/iroh-relay-v$IROH_VERSION-$ARCH-unknown-linux-musl.tar.gz"
  if [ "$DRY" = 1 ]; then
    printf '  + fetch %s, check sha256 %s, install /usr/local/bin/iroh-relay\n' "$url" "$SHA"
  else
    tmp=$(mktemp -d)
    curl -fsSL "$url" -o "$tmp/r.tgz"
    echo "$SHA  $tmp/r.tgz" | sha256sum -c --quiet || die "The relay download does not match its checksum. Nothing was installed."
    tar -xzf "$tmp/r.tgz" -C "$tmp" ./iroh-relay
    install -m 755 "$tmp/iroh-relay" /usr/local/bin/iroh-relay
    rm -rf "$tmp"
  fi
fi
ANY="[::]"
[ -f /proc/net/if_inet6 ] || ANY="0.0.0.0"

relay_config() {
  write "$CONF" 644 <<EOF
# Pyrate Radio relay. Written by tools/vps/install.sh: run it again rather than editing this.
enable_relay = true
http_bind_addr = "$ANY:80"
enable_quic_addr_discovery = true
enable_metrics = false

[tls]
cert_mode = "LetsEncrypt"
hostname = "$1"
contact = "relay@$1"
cert_dir = "/var/lib/pyrate-relay/certs"
https_bind_addr = "$ANY:443"
quic_bind_addr = "$ANY:$QUIC_PORT"

# No one client takes the relay for itself: a megabyte a second each, bursts of eight.
[limits]
accept_conn_limit = 50.0
accept_conn_burst = 200

[limits.client.rx]
bytes_per_second = 1000000
max_burst_bytes = 8000000
EOF
}

changed=0
relay_config "$DOMAIN" && changed=1
write "$UNIT" 644 <<EOF && changed=1
[Unit]
Description=Pyrate Radio relay
After=network-online.target
Wants=network-online.target

[Service]
ExecStart=/usr/local/bin/iroh-relay --config-path $CONF
DynamicUser=yes
StateDirectory=pyrate-relay
AmbientCapabilities=CAP_NET_BIND_SERVICE
CapabilityBoundingSet=CAP_NET_BIND_SERVICE
NoNewPrivileges=yes
ProtectSystem=strict
ProtectHome=yes
PrivateTmp=yes
LimitNOFILE=65536
Environment=RUST_LOG=warn
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
EOF
run systemctl daemon-reload
run systemctl enable -q pyrate-relay
if [ "$changed" = 1 ]; then run systemctl restart pyrate-relay; else run systemctl start pyrate-relay; fi

# Let's Encrypt answers within a minute or two. A free name whose allowance is spent moves to
# the other free name.
relay_up() {
  [ "$DRY" = 1 ] && return 0
  local i
  for i in $(seq 1 36); do
    curl -fsS --max-time 5 "https://$1/ping" >/dev/null 2>&1 && return 0
    [ "$i" = 1 ] && echo "Waiting for the certificate (up to three minutes)..."
    sleep 5
  done
  return 1
}
RELAY_OK=0
if relay_up "$DOMAIN"; then
  RELAY_OK=1
elif [ ${#FREE[@]} = 2 ]; then
  DOMAIN=${FREE[1]}
  say "No certificate yet. Trying the other free name: $DOMAIN"
  relay_config "$DOMAIN" || true
  run systemctl restart pyrate-relay
  relay_up "$DOMAIN" && RELAY_OK=1
fi
RELAY_URL="https://$DOMAIN/"

# ---- The Lantern ---------------------------------------------------------------------------
say "Lantern (the first build takes 10 to 30 minutes and shows nothing while it works)"
if [ -d "$SRC/.git" ]; then
  run git -C "$SRC" fetch -q --depth 1 origin "$BRANCH"
  run git -C "$SRC" reset -q --hard FETCH_HEAD
else
  run mkdir -p "$(dirname "$SRC")"
  run git clone -q --depth 1 --branch "$BRANCH" "$REPO" "$SRC"
fi
run docker build -q -t pyrate-lantern -f "$SRC/radio/Dockerfile" "$SRC/radio" >/dev/null
# The compiler stays behind in the build cache: gigabytes this server does not need.
run docker builder prune -af >/dev/null || true
run docker image prune -f >/dev/null
run docker rmi rust:bookworm >/dev/null 2>&1 || true

have=0 want_img=new have_img=none have_env="" have_net=""
if docker container inspect lantern >/dev/null 2>&1; then
  have=1
  want_img=$(docker image inspect -f '{{.Id}}' pyrate-lantern 2>/dev/null | tr -d '[:space:]' || true)
  have_img=$(docker container inspect -f '{{.Image}}' lantern | tr -d '[:space:]')
  have_env=$(docker container inspect -f '{{range .Config.Env}}{{println .}}{{end}}' lantern | grep '^PYRATE_RELAY_URL=' || true)
  have_net=$(docker container inspect -f '{{.HostConfig.NetworkMode}}' lantern | tr -d '[:space:]')
fi
if [ "$have_img" = "$want_img" ] && [ "$have_env" = "PYRATE_RELAY_URL=$RELAY_URL" ] && [ "$have_net" = host ]; then
  run docker start lantern >/dev/null
else
  # Its keys and everything it carries live in the pyrate volume, which stays.
  [ "$have" = 0 ] || run docker rm -f lantern >/dev/null
  run docker run -d --name lantern --restart unless-stopped --network host \
    -v pyrate:/data -e "PYRATE_RELAY_URL=$RELAY_URL" \
    --log-opt max-size=10m --log-opt max-file=3 pyrate-lantern >/dev/null
fi

NODE=""
if [ "$DRY" != 1 ]; then
  for _ in $(seq 1 30); do
    NODE=$(docker logs lantern 2>&1 | sed -n 's/^node id: \([0-9a-f]\{64\}\)$/\1/p' | tail -1 || true)
    [ -n "$NODE" ] && break
    sleep 2
  done
fi

# ---- Done ----------------------------------------------------------------------------------
say "Done."
echo "Relay:    $RELAY_URL"
echo "Lantern:  ${NODE:-(not started yet: see docker logs lantern)}"
[ -n "$NODE" ] && echo "Harbour:  $NODE@$RELAY_URL"
if [ "$RELAY_OK" != 1 ] && [ "$DRY" != 1 ]; then
  echo
  echo "The relay has no certificate yet. Check that $DOMAIN points at $IP, that your provider's"
  echo "own firewall lets in 80, 443 and $QUIC_PORT/udp, then run this again."
  echo "What the relay says: journalctl -u pyrate-relay -n 30"
fi
echo
echo "Both lines are public: send them to whoever runs the radio. The Lantern's keys stay"
echo "in its volume on this server and are never shown. Run this again to update."
