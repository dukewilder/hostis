#!/usr/bin/env bash
# Pyrate Radio: a relay and a Lantern on a fresh Debian or Ubuntu server. No rights reserved.
#
#   curl -fsSL https://raw.githubusercontent.com/dukewilder/hostis/radio/radio/tools/vps/install.sh -o pyrate.sh && bash pyrate.sh
#
# It asks one thing, the relay's name, and prints the relay's address, the Lantern's id and the
# open player's address at the end. Run it again at any time: it changes only what is out of
# date, and that is also how to update. DOMAIN=relay.example.org skips the question.
# PLAYER_DOMAIN=listen.example.org serves the open player at a second name (none: not at all).
# PYRATE_DRY=1 shows every step and changes nothing.
#
# Caddy holds 80 and 443 and the certificates: the relay behind it at the relay's name, and the
# Lantern's copy of the open player at /listen/. The relay's QUIC port uses Caddy's certificate.
set -euo pipefail

IROH_VERSION=1.4.1 # the iroh in radio/Cargo.lock
SHA_x86_64=75254641e3fad7fcbac6dee4d485556fbe06ef708340a9d8810469270ba626b8
SHA_aarch64=3fc0c2d67c1cd66250bf82f3e2f17a985b7081a3d272186bad9d63e3f2347b38
CADDY_VERSION=2.11.4
CADDY_SHA_x86_64=8220d1f013b6f27510247b2360c9e0ca9f018feebd82515f07635318b34ff9777ccc8fd0b6e6f2486ce3a33fe389fbb7db12d05baa474f4587509fb4f5ebf1c9
CADDY_SHA_aarch64=d5a7c423853c24a799765e0e8210d5c7c22a8f56ed37a3cae2fb9f58be138853c02b4efd6b59d576e6d8c7c0d30b9c1592deeaa6a536ff69bcca23b8c1ea709c
REPO=https://github.com/dukewilder/hostis
BRANCH=radio
SRC=/opt/pyrate/hostis
CONF=/etc/pyrate/relay.toml
UNIT=/etc/systemd/system/pyrate-relay.service
QUIC_PORT=7842
STUN_PORT=3478 # STUN only, for a tab's direct link to its guard (browser/stun.txt)
STUN_CONF=/etc/pyrate/stun.conf
STUN_UNIT=/etc/systemd/system/pyrate-stun.service
RELAY_HTTP=127.0.0.1:3340   # the relay, behind Caddy
LANTERN_HTTP=127.0.0.1:8089 # the Lantern's gangway, behind Caddy
CADDYFILE=/etc/pyrate/Caddyfile
CADDY_UNIT=/etc/systemd/system/pyrate-caddy.service
CADDY_DATA=/var/lib/pyrate-caddy
TLS=/etc/pyrate/tls # Caddy's certificate, copied for the relay's QUIC port
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
case $ARCH in
  x86_64) SHA=$SHA_x86_64 CADDY_SHA=$CADDY_SHA_x86_64 CADDY_ARCH=amd64 ;;
  aarch64) SHA=$SHA_aarch64 CADDY_SHA=$CADDY_SHA_aarch64 CADDY_ARCH=arm64 ;;
  *) die "No relay build for $ARCH." ;;
esac

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

# The open player goes at the relay's name, or at a name of its own. A HOSTIS name never serves
# it (hostis.black carries HOSTIS's stations alone), so there it takes a free name instead.
hostis() { [[ $1 == hostis.black || $1 == *.hostis.black ]]; }
PLAYER=$(printf '%s' "${PLAYER_DOMAIN:-}" | tr '[:upper:]' '[:lower:]' | sed -e 's#^[a-z]*://##' -e 's#/.*$##')
if [ -z "$PLAYER" ]; then
  PLAYER=$DOMAIN
  if hostis "$PLAYER"; then PLAYER="${IP//./-}.sslip.io"; fi
fi
if [ "$PLAYER" != none ]; then
  if hostis "$PLAYER"; then die "The open player is never served at a HOSTIS name. Set PLAYER_DOMAIN to another, or to none."; fi
  [[ $PLAYER =~ ^[a-z0-9]([a-z0-9-]*[a-z0-9])?(\.[a-z0-9]([a-z0-9-]*[a-z0-9])?)+$ ]] || die "\"$PLAYER\" is not a name."
  say "Open player: https://$PLAYER/listen/"
fi

# ---- Packages, swap, firewall --------------------------------------------------------------
say "Packages"
PKGS=(ca-certificates curl git ufw tar coturn)
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

say "Firewall: SSH, 80 and 443, $QUIC_PORT for QUIC and $STUN_PORT for STUN. Nothing else comes in."
ssh_ports=$(sshd -T 2>/dev/null | awk '$1 == "port" {print $2}' | sort -u || true)
for p in ${ssh_ports:-22}; do run ufw allow "$p/tcp" >/dev/null; done
run ufw allow 80/tcp >/dev/null
run ufw allow 443/tcp >/dev/null
run ufw allow "$QUIC_PORT/udp" >/dev/null
run ufw allow "$STUN_PORT/udp" >/dev/null
run ufw default deny incoming >/dev/null
run ufw default allow outgoing >/dev/null
run ufw --force enable >/dev/null

# ---- STUN ----------------------------------------------------------------------------------
# A tab asks this what its own address looks like from outside, so that it and its guard can
# link directly. STUN only: no TURN, no relaying, no accounts, and no log of who asked.
say "STUN (port $STUN_PORT, STUN only)"
write "$STUN_CONF" 644 <<EOF || true
listening-port=$STUN_PORT
stun-only
no-tls
no-dtls
no-tcp
no-cli
no-software-attribute
fingerprint
no-stdout-log
log-file=/dev/null
EOF
write "$STUN_UNIT" 644 <<EOF || true
[Unit]
Description=Pyrate Radio STUN (STUN only)
After=network-online.target
Wants=network-online.target

[Service]
ExecStart=/usr/bin/turnserver -c $STUN_CONF
DynamicUser=yes
NoNewPrivileges=yes
ProtectSystem=strict
ProtectHome=yes
PrivateTmp=yes
Restart=always

[Install]
WantedBy=multi-user.target
EOF
# The package's own service (TURN, with its own config) stays off.
run systemctl disable --now coturn >/dev/null 2>&1 || true
run systemctl daemon-reload
run systemctl enable -q pyrate-stun
run systemctl restart pyrate-stun

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

# ---- Caddy ---------------------------------------------------------------------------------
say "Caddy $CADDY_VERSION (the certificates, and the front door on 80 and 443)"
if ! /usr/local/bin/caddy version 2>/dev/null | grep -q "^v$CADDY_VERSION "; then
  url="https://github.com/caddyserver/caddy/releases/download/v$CADDY_VERSION/caddy_${CADDY_VERSION}_linux_$CADDY_ARCH.tar.gz"
  if [ "$DRY" = 1 ]; then
    printf '  + fetch %s, check sha512 %s, install /usr/local/bin/caddy\n' "$url" "$CADDY_SHA"
  else
    tmp=$(mktemp -d)
    curl -fsSL "$url" -o "$tmp/c.tgz"
    echo "$CADDY_SHA  $tmp/c.tgz" | sha512sum -c --quiet || die "The Caddy download does not match its checksum. Nothing was installed."
    tar -xzf "$tmp/c.tgz" -C "$tmp" caddy
    install -m 755 "$tmp/caddy" /usr/local/bin/caddy
    rm -rf "$tmp"
  fi
fi
getent group pyrate-tls >/dev/null || run groupadd --system pyrate-tls

# caddy_config RELAY_NAME: the relay at its name, and the open player at /listen/ there or at a
# name of its own. Port 80 answers the relay's captive-portal check and sends the rest to 443.
caddy_config() {
  local relay=$1 front="" own=""
  if [ "$PLAYER" = "$relay" ]; then
    front="
	redir / /listen/ 302
	handle /listen* {
		reverse_proxy $LANTERN_HTTP
	}"
  elif [ "$PLAYER" != none ]; then
    own="
$PLAYER {
	handle /listen* {
		reverse_proxy $LANTERN_HTTP
	}
	handle {
		redir * /listen/ 302
	}
}"
  fi
  write "$CADDYFILE" 644 <<EOF
# Pyrate Radio's front door. Written by tools/vps/install.sh: run it again rather than editing this.
{
	admin off
	email relay@$relay
	acme_ca https://acme-v02.api.letsencrypt.org/directory
	storage file_system $CADDY_DATA
	log {
		level WARN
	}
}

$relay {$front
	handle {
		reverse_proxy $RELAY_HTTP
	}
}

http://$relay {
	handle /generate_204 {
		reverse_proxy $RELAY_HTTP
	}
	handle {
		redir https://{host}{uri} 308
	}
}
$own
EOF
}

relay_config() {
  write "$CONF" 644 <<EOF
# Pyrate Radio relay. Written by tools/vps/install.sh: run it again rather than editing this.
# Caddy in front holds 80 and 443 and the certificate. The relay speaks plain HTTP to Caddy
# alone, and QUIC on $QUIC_PORT with Caddy's certificate, read again each day.
enable_relay = true
http_bind_addr = "$RELAY_HTTP"
enable_quic_addr_discovery = true
enable_metrics = false

[tls]
cert_mode = "Reloading"
hostname = "$1"
manual_cert_path = "$TLS/relay.crt"
manual_key_path = "$TLS/relay.key"
quic_bind_addr = "$ANY:$QUIC_PORT"
dangerous_http_only = true

# No one client takes the relay for itself: a megabyte a second each, bursts of eight.
[limits]
accept_conn_limit = 50.0
accept_conn_burst = 200

[limits.client.rx]
bytes_per_second = 1000000
max_burst_bytes = 8000000
EOF
}

# Where Caddy keeps a name's certificate. A copy goes where the relay can read it, each time
# Caddy renews it. True when either unit changed.
issued() { printf '%s/certificates/acme-v02.api.letsencrypt.org-directory/%s/%s' "$CADDY_DATA" "$1" "$1"; }
tls_units() {
  local c n=1
  c=$(issued "$1")
  write /etc/systemd/system/pyrate-tls.service 644 <<EOF && n=0
[Unit]
Description=Pyrate Radio: Caddy's certificate for the relay's QUIC port

[Service]
Type=oneshot
ExecStart=/usr/bin/install -m 640 -g pyrate-tls -D $c.crt $TLS/relay.crt
ExecStart=/usr/bin/install -m 640 -g pyrate-tls -D $c.key $TLS/relay.key
EOF
  write /etc/systemd/system/pyrate-tls.path 644 <<EOF && n=0
[Unit]
Description=Pyrate Radio: copy Caddy's certificate when it is renewed

[Path]
PathChanged=$c.crt

[Install]
WantedBy=multi-user.target
EOF
  return $n
}

# ---- The relay's service --------------------------------------------------------------------

changed=0
relay_config "$DOMAIN" && changed=1
write "$UNIT" 644 <<EOF && changed=1
[Unit]
Description=Pyrate Radio relay
After=network-online.target pyrate-caddy.service
Wants=network-online.target

[Service]
ExecStart=/usr/local/bin/iroh-relay --config-path $CONF
DynamicUser=yes
SupplementaryGroups=pyrate-tls
StateDirectory=pyrate-relay
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
caddy_changed=0
caddy_config "$DOMAIN" && caddy_changed=1
write "$CADDY_UNIT" 644 <<EOF && caddy_changed=1
[Unit]
Description=Pyrate Radio front door (Caddy)
After=network-online.target
Wants=network-online.target

[Service]
ExecStart=/usr/local/bin/caddy run --config $CADDYFILE --adapter caddyfile
DynamicUser=yes
StateDirectory=pyrate-caddy
Environment=HOME=$CADDY_DATA XDG_DATA_HOME=$CADDY_DATA XDG_CONFIG_HOME=$CADDY_DATA
AmbientCapabilities=CAP_NET_BIND_SERVICE
CapabilityBoundingSet=CAP_NET_BIND_SERVICE
NoNewPrivileges=yes
ProtectSystem=strict
ProtectHome=yes
PrivateTmp=yes
LimitNOFILE=65536
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
EOF
tls_units "$DOMAIN" && caddy_changed=1
if [ "$DRY" != 1 ] && ! /usr/local/bin/caddy validate --config "$CADDYFILE" --adapter caddyfile >/dev/null 2>&1; then
  die "Caddy will not take $CADDYFILE. What it says: caddy validate --config $CADDYFILE --adapter caddyfile"
fi
run systemctl daemon-reload
run systemctl enable -q pyrate-relay pyrate-caddy pyrate-tls.path
# An earlier run's relay held 80 and 443 itself: it lets go before Caddy takes them.
if [ "$changed" = 1 ]; then run systemctl stop pyrate-relay; fi
if [ "$caddy_changed" = 1 ]; then run systemctl restart pyrate-caddy; else run systemctl start pyrate-caddy; fi
run systemctl restart pyrate-tls.path

# Let's Encrypt answers Caddy within a minute or two; then the relay takes the certificate and
# starts. A free name whose allowance is spent moves to the other free name.
relay_up() {
  [ "$DRY" = 1 ] && return 0
  local i c
  c=$(issued "$1")
  for i in $(seq 1 36); do
    if [ -s "$c.crt" ]; then
      systemctl start pyrate-tls.service
      if [ "$changed" = 1 ]; then systemctl restart pyrate-relay; else systemctl start pyrate-relay; fi
      changed=0
      break
    fi
    [ "$i" = 1 ] && echo "Waiting for the certificate (up to three minutes)..."
    sleep 5
  done
  for i in $(seq 1 12); do
    curl -fsS --max-time 5 "https://$1/ping" >/dev/null 2>&1 && return 0
    sleep 5
  done
  return 1
}
RELAY_OK=0
if relay_up "$DOMAIN"; then
  RELAY_OK=1
elif [ ${#FREE[@]} = 2 ]; then
  [ "$PLAYER" = "$DOMAIN" ] && PLAYER=${FREE[1]}
  DOMAIN=${FREE[1]}
  say "No certificate yet. Trying the other free name: $DOMAIN"
  relay_config "$DOMAIN" || true
  caddy_config "$DOMAIN" || true
  tls_units "$DOMAIN" || true
  run systemctl daemon-reload
  run systemctl restart pyrate-caddy pyrate-tls.path
  changed=1
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
run docker rmi rust:bookworm python:3.12-slim-bookworm >/dev/null 2>&1 || true

# The gangway on loopback, for Caddy alone, with the image's copy of the open player.
LANTERN_ARGS=()
[ "$PLAYER" = none ] || LANTERN_ARGS=(--http "$LANTERN_HTTP" --player /usr/share/pyrate/player)
have=0 want_img=new have_img=none have_env="" have_net="" have_args=""
if docker container inspect lantern >/dev/null 2>&1; then
  have=1
  want_img=$(docker image inspect -f '{{.Id}}' pyrate-lantern 2>/dev/null | tr -d '[:space:]' || true)
  have_img=$(docker container inspect -f '{{.Image}}' lantern | tr -d '[:space:]')
  have_env=$(docker container inspect -f '{{range .Config.Env}}{{println .}}{{end}}' lantern | grep '^PYRATE_RELAY_URL=' || true)
  have_net=$(docker container inspect -f '{{.HostConfig.NetworkMode}}' lantern | tr -d '[:space:]')
  have_args=$(docker container inspect -f '{{join .Args " "}}' lantern)
fi
# The container keeps its flags after the image's own: they must be the ones wanted now.
args_ok=0
if [ ${#LANTERN_ARGS[@]} = 0 ]; then
  [[ $have_args == *--player* ]] || args_ok=1
else
  [[ $have_args == *" ${LANTERN_ARGS[*]}" ]] && args_ok=1
fi
if [ "$have_img" = "$want_img" ] && [ "$have_env" = "PYRATE_RELAY_URL=$RELAY_URL" ] && [ "$have_net" = host ] && [ "$args_ok" = 1 ]; then
  run docker start lantern >/dev/null
else
  # Its keys and everything it carries live in the pyrate volume, which stays.
  [ "$have" = 0 ] || run docker rm -f lantern >/dev/null
  run docker run -d --name lantern --restart unless-stopped --network host \
    -v pyrate:/data -e "PYRATE_RELAY_URL=$RELAY_URL" \
    --log-opt max-size=10m --log-opt max-file=3 pyrate-lantern "${LANTERN_ARGS[@]}" >/dev/null
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
echo "STUN:     stun:$DOMAIN:$STUN_PORT   (a line for browser/stun.txt)"
PLAYER_OK=0
if [ "$PLAYER" != none ]; then
  if [ "$DRY" = 1 ]; then PLAYER_OK=1; else
    for _ in $(seq 1 24); do
      if curl -fsS --max-time 5 "https://$PLAYER/listen/" 2>/dev/null | grep -q "Pyrate Radio"; then PLAYER_OK=1; break; fi
      sleep 5
    done
  fi
  echo "Player:   https://$PLAYER/listen/"
fi
if [ "$RELAY_OK" != 1 ] && [ "$DRY" != 1 ]; then
  echo
  echo "The relay has no certificate yet. Check that $DOMAIN points at $IP, that your provider's"
  echo "own firewall lets in 80, 443, $QUIC_PORT/udp and $STUN_PORT/udp, then run this again."
  echo "What they say: journalctl -u pyrate-caddy -u pyrate-relay -n 30"
fi
if [ "$PLAYER" != none ] && [ "$PLAYER_OK" != 1 ] && [ "$DRY" != 1 ]; then
  echo
  echo "The player does not answer yet. Check that $PLAYER points at $IP, then run this again."
  echo "What the Lantern says: docker logs --tail 30 lantern"
fi
echo
echo "Every line is public: send them to whoever runs the radio, and the player's address"
echo "to anyone. The Lantern's keys stay in its volume on this server and are never shown."
echo "Run this again to update."
