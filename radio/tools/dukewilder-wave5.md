Wave 5 of Pyrate Radio's hardening plan, for dukewilder. Work through it in order, and stop to ask Dylan where a step says so. The repo is github.com/dukewilder/hostis, branch `radio`; everything named `radio/...` is a path in it. This replaces `radio/tools/dukewilder-rebuild.md`, which was never fully run: steps 3 and 4 below are that rebuild, brought up to date.

What it builds: the public Lantern (the **harbour**) moves onto a VPN tunnel of its own, with a public relay that phones prefer over number0's; a dark Lantern (the **hold**) carries the whole Log over Tor; the open player gets a Tor onion address; logs shrink to almost nothing; the app, the computers' builds and the source seed over BitTorrent; and the box gets a security check.

Rules that hold throughout:
- The house IP must never be exposed. Every Pyrate container runs inside a gluetun network namespace (`network_mode: container:<gluetun>` or `service:<gluetun>`). No published ports, no port forwards on the router, no router changes.
- Do not touch the Jellyfin, arr or Monero containers, or the gluetun they use.
- Ask Dylan before deleting any file, image, volume or log.
- Never print a WireGuard private key, a Tor `hs_ed25519_secret_key`, or a Lantern's `hold.key`, `node.key`, `seed.key` or `keys/` files. Report paths, never contents.

## 1. Look before changing anything

Run and keep the output for the report:
- `docker ps -a --format '{{.Names}}\t{{.Image}}\t{{.Status}}\t{{.Ports}}'`
- `docker inspect` of every Pyrate container (the Lantern, `gluetun-ipfs`, `pyrate-kubo`, any `tor`): image, command and arguments, environment with secrets masked, volumes, restart policy, network mode, log driver.
- The compose file(s) these come from, if any, and their service names.
- Anything named `cloudflared`, or any Cloudflare Tunnel config on the host (`ls /etc/cloudflared ~/.cloudflared 2>/dev/null`, `systemctl list-units | grep -i cloudflare`). If one routes anything Pyrate (a hostname containing `relay` or `hostis`), report it and do not change it yet.
- `crontab -l` and `ls /etc/cron.d`.
- Whether the main gluetun (the arr stack's) has port forwarding on. Read only, change nothing.

## 2. Update the code and build the image

From the repository root:
- `git fetch origin radio && git checkout radio && git pull --ff-only origin radio`; `git log --oneline -1` should show `8efcf5f` or later.
- `docker build -t pyrate-lantern -f radio/Dockerfile radio`. It compiles Rust and takes a while. If it fails, stop and report the error. Do not change the Dockerfile.

## 3. The harbour, on a tunnel of its own, with a public relay

The harbour is the Lantern everyone can see. It keeps only HOSTIS's own stations, serves the web page, and from now on runs a relay that phones use in place of number0's. It moves onto its own VPN exit, so the radio shares no exit with the arr stack.

3a. **Ask Dylan for a new Proton WireGuard configuration** for this, made as he made the one for `gluetun-ipfs`: Proton account, Downloads, WireGuard, a P2P server, with **NAT-PMP (port forwarding) on**. He passes it to you privately. Without it, skip to step 4 and say so in the report; the harbour then stays where it is, gaining only the changes in 3d.

3b. Add a third gluetun, `gluetun-radio`, set up as `gluetun-ipfs` is (same image, `VPN_SERVICE_PROVIDER=protonvpn`, `VPN_TYPE=wireguard`, `VPN_PORT_FORWARDING=on`, the new key), with `VPN_PORT_FORWARDING_STATUS_FILE=/gluetun/forwarded_port` and its own small volume for `/gluetun`. No published ports. When it is up, read the forwarded port (`docker exec gluetun-radio cat /gluetun/forwarded_port`) and the exit IP (`docker exec gluetun-radio wget -qO- http://127.0.0.1:8000/v1/publicip/ip`).

3c. A small host script, `/srv/pyrate/harbour.sh`, that (re)creates the harbour so its relay listens on the forwarded port and announces it:
- reads PORT from the status file and IP from gluetun's control server, as above;
- if `http://IP:PORT` differs from what the running harbour has in `PYRATE_RELAY_URL`, recreates the harbour (same name, same data volume, same `--seed`, `--release-keys`, `--quota-gb` and restart policy as step 1 recorded) with:
  - network mode `container:gluetun-radio`;
  - arguments: what it had, plus `--carry-own`, `--http 127.0.0.1:<its http port>` and `--relay 0.0.0.0:PORT`;
  - environment: `PYRATE_RELAY_URL=http://IP:PORT/`, and `PYRATE_OPEN_PLAYER=` set to the first line of `radio/app/assets/open_players.txt`; no `PYRATE_PLAYER` (the harbour answers at a HOSTIS name and must never serve the open player);
  - log driver `local`, `max-size=1m`, `max-file=1`;
- otherwise does nothing.
Run it once, then from cron every five minutes: `*/5 * * * * sh /srv/pyrate/harbour.sh >> /srv/pyrate/harbour.log 2>&1`. Proton gives a new port when the tunnel reconnects; the script follows it, and the harbour's signed notes tell every phone the new address within minutes. Phones fall back to number0's relays for the moments between.

3d. If 3a was skipped, recreate the harbour where it is with `--carry-own`, `PYRATE_OPEN_PLAYER` as above, no `PYRATE_PLAYER`, and the `local` log driver, and leave out the relay.

3e. Check: `docker logs --tail 40 <harbour>` shows `LANTERN LIT`, `relay on http://0.0.0.0:PORT`, `neighbour up`, and only HOSTIS's own stations under `in the Log:`. From a machine outside the house (Dylan's phone off Wi-Fi will do), `http://IP:PORT/ping` answers. Report the harbour's node id (`node id:` in its log) and its relay URL.

## 4. The hold: a dark carrier over Tor

The hold carries everyone's sealed logs blind and is tied to nothing: a fresh identity, no address published, every connection through Tor.

4a. A `tor` service in `gluetun-ipfs`'s namespace (network mode `container:gluetun-ipfs`), from a maintained Tor image or Debian with the `tor` package, with a volume for `/var/lib/tor` and this torrc:
```
HTTPTunnelPort 127.0.0.1:8118
SocksPort 0
HiddenServiceDir /var/lib/tor/player
HiddenServicePort 80 127.0.0.1:8481
```
The last two lines are step 5's onion. Log driver `local`, `max-size=1m`, `max-file=1`.

4b. The hold: a new service from `pyrate-lantern`, network mode `container:gluetun-ipfs`, a fresh volume `pyrate-hold-data` at its `--dir`, arguments `--dir <that dir> --quota-gb <as large as the disk sensibly allows>` and nothing else (no `--http`, `--relay` or `--carry-own`), environment `PYRATE_DARK=1` and `PYRATE_PROXY=http://127.0.0.1:8118`, log driver `local` as above, restart `unless-stopped`.

4c. Check: `docker logs --tail 40 <hold>` shows `dark running: relays only, no address published`, then `neighbour up`, and in time far more stations in the Log than the harbour. `docker inspect <hold>` shows gluetun-ipfs's network mode and no ports.

## 5. The open player as a Tor onion

Tor from step 4a already serves kubo's gateway as an onion. Read the address: `docker exec <tor> cat /var/lib/tor/player/hostname` (the hostname only, never the key files beside it). Then, from a Tor Browser anywhere, `http://<onion>/ipns/k51qzi5uqu5dhbdmsbccqsq3q9cfzwdod64hxnmila4ark5kds3fxf7k6pc0fr/` should open the open player and come aboard. Report the onion hostname: it goes into the bearings and onto the site.

## 6. Logs that keep nothing

- Every Pyrate container (harbour, hold, tor, kubo, the gluetuns for the radio) uses the `local` log driver with `max-size=1m` and `max-file=1`, so a seized box holds minutes of logs, not months.
- kubo: set `GOLOG_LOG_LEVEL=error` in its environment, and confirm its gateway access log is off (kubo has none by default; report if any reverse proxy in front of it logs).
- Report the current size of each Pyrate container's existing logs (`docker inspect --format '{{.LogPath}}' <c>` then `du -h` of that path) and of `/srv/pyrate/pin_player.log`. Ask Dylan before clearing any of them.
- `/srv/pyrate/pin_player.log`: change its cron line to keep only the last run (`> /srv/pyrate/pin_player.log` in place of `>>`).

## 7. Seeding the app, the builds and the source by BitTorrent

Add a `transmission` service (`lscr.io/linuxserver/transmission`) in `gluetun-ipfs`'s namespace, no published ports, a volume for downloads and a watch folder, log driver `local`. A host cron line, daily, fetches the current torrents into the watch folder:
```
37 5 * * * cd /srv/pyrate/watch && for t in $(wget -qO- https://hostis.black/pyrateradio/ | grep -o 'pyrate-radio-[^"]*\.torrent' | sort -u); do wget -qN "https://hostis.black/pyrateradio/$t"; done; wget -qN "https://hostis.black/pyrateradio/source/$(wget -qO- https://hostis.black/pyrateradio/ipfs/files.txt | grep -o 'pyrate-radio-src-[^ ]*\.tar\.gz' | head -1).torrent"
```
Each torrent names hostis.black as its web seed, so transmission fetches the files itself and then seeds them. If the existing qBittorrent is already seeding the APK, leave it be: two seeders are better than one.

## 8. A security check of the box

Report, and change only what is marked safe:
- `ss -tulpn`: every listening socket and its process. Anything listening on an address other than 127.0.0.1, the LAN, or inside a container namespace is a finding.
- SSH: `sshd -T | grep -Ei 'passwordauthentication|permitrootlogin|pubkeyauthentication'`. **Ask Dylan before changing SSH.** If he agrees and a working key login exists, set `PasswordAuthentication no` and `PermitRootLogin prohibit-password`, reload sshd, and confirm a new key login works before ending the session.
- Updates: if `unattended-upgrades` is not installed, install it and enable security updates only (safe). Do not enable automatic reboots.
- Docker: report the Docker version and whether the daemon listens on TCP (it must not).
- Disk encryption: report whether the system disk is encrypted (`lsblk -o NAME,TYPE,FSTYPE,MOUNTPOINT` and `cryptsetup status` where it applies). Change nothing.

## 9. Report back

Send Dylan, for Claude:
- the commit built from, and each new or changed container's command line, environment (secrets masked) and network mode;
- the harbour's node id and relay URL, and whether `http://IP:PORT/ping` answered from outside;
- the onion hostname, and whether the open player came aboard through it in Tor Browser;
- the last twenty log lines of the harbour, the hold and tor;
- what step 1 found about Cloudflare, and every finding from step 8;
- anything you skipped, and why.
