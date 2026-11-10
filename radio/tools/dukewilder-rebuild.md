Superseded by `dukewilder-wave5.md`, which brings this rebuild up to date and adds the relay, the onion, the log limits, seeding and the security check. Kept for the record.

Rebuild the Pyrate Radio Lantern on dukewilder from the latest `radio` branch of github.com/dukewilder/hostis. This version splits the one Lantern into two identities: a public **harbour** that keeps only its own stations, and a dark **hold** that carries the whole Log over Tor and is tied to nothing. The point is that the node everyone can see is just a station keeper, and the node that carries everything cannot be traced to the harbour, to the VPN, or to the house.

Rules that hold throughout:
- The house IP must never be exposed. Every container stays inside gluetun's network namespace. No port forwards, no router changes, no new published ports.
- Ask Dylan before deleting any file, image or volume. Old images can stay.
- Do not touch the Jellyfin, arr or Monero containers.

1. Find the running Lantern and record how it runs:
   `docker ps --format '{{.Names}}\t{{.Image}}\t{{.Status}}'`
   then `docker inspect <lantern container>` and note its image, its full command and arguments, its environment, its data volume, its restart policy, and its network mode (it should read `container:gluetun` or `service:gluetun`). If it runs from a compose file, note the file and the service name. This running Lantern becomes the harbour in step 4.

2. Update the code the image was built from:
   `git fetch origin radio && git checkout radio && git pull --ff-only origin radio`
   `git log --oneline -1` should show `4f4efd7` or later.

3. Build the new image from the repository root:
   `docker build -t pyrate-lantern -f radio/Dockerfile radio`
   It compiles Rust inside the build stage and takes a while on the FIREBAT. If it fails, stop and report the error. Do not change the Dockerfile.

4. The harbour (the public face). Recreate the existing Lantern with everything step 1 recorded: same name, same data volume, same `--seed`, `--release-keys` and `--quota-gb`, same network mode and restart policy. Change two things and add one:
   - add `--carry-own`, so it serves only its own stations and sheds the third-party logs it was holding. Those move to the hold in step 5. Expect `dropped N held stations` in its first log lines. This is meant to happen.
   - keep `--http` and `--relay`. The harbour is a bootstrap door and a web mirror, so it must stay reachable and must not run dark.
   - do not give it `PYRATE_PLAYER`. The harbour answers at a HOSTIS name, and the player that plays every station must never be served from one. If an earlier run set `PYRATE_PLAYER`, remove it.
   - set `PYRATE_OPEN_PLAYER=` to the first line of `radio/app/assets/open_players.txt`, so its invitations and links name the open player on IPFS.
   With compose: `docker compose up -d --no-deps --force-recreate <harbour service>`.

5. The hold (the dark carrier). Add a second Lantern service, advertised nowhere, that carries the whole Log over Tor:
   - a fresh data volume, for example `pyrate-hold-data`, mounted at its `--dir`. A fresh dir means a fresh node id, so the hold shares no identity with the harbour.
   - arguments: `--quota-gb <as large as the disk allows>` and nothing else. No `--http`, no `--relay`, no `--carry-own`. It carries everything and exposes nothing.
   - environment: `PYRATE_DARK=1` and `PYRATE_PROXY=http://127.0.0.1:8118`.
   - network mode `container:gluetun` (or `service:gluetun`), the same as the harbour.
   `PYRATE_DARK=1` drops the hold's direct paths and stops it publishing any address record, so nothing ties its node id to a relay or an IP. `PYRATE_PROXY` sends every relay connection through the Tor container in step 5a. The relay sees a Tor exit. The VPN sees only Tor. The house is never in the picture.

   5a. A Tor container for the hold. Add a small `tor` service in gluetun's namespace (network mode `container:gluetun`) configured with `HTTPTunnelPort 8118`. Nothing it does leaves gluetun, so Tor itself reaches the internet through the VPN. If you would rather not run Tor, drop `PYRATE_PROXY` and the hold still runs dark over the VPN alone. That is weaker: the relay then sees the VPN exit instead of a Tor exit. Tor is the recommended path for the first node.

6. Mirrors, so no single door sinks the network:
   - the open player on IPFS. This is the copy of the radio that plays every station, and it lives where nobody owns it. Add a `kubo` service (image `ipfs/kubo:latest`) in gluetun's namespace (network mode `container:gluetun`), with a data volume `pyrate-ipfs` at `/data/ipfs` and no published ports. It reaches the IPFS network through the VPN. If gluetun has a forwarded port spare, set kubo's swarm to listen on it (`ipfs config --json Addresses.Swarm '["/ip4/0.0.0.0/tcp/<port>","/ip4/0.0.0.0/udp/<port>/quic-v1"]'`); otherwise leave the defaults, and kubo reaches out through relays.
     Then pin the player and keep it pinned: copy `radio/tools/pin_player.sh` into the container and run `docker exec <kubo> sh /pin_player.sh` once; it prints `pinned bafy…`. Add a host cron line to run it hourly, so every new player HOSTIS publishes is pinned within the hour: `17 * * * * docker exec <kubo> sh /pin_player.sh`. The script fetches each player's CAR from hostis.black and IPFS checks every block against the name.
     Give the player one fixed address too: `docker exec <kubo> ipfs key gen pyrate-player` once, and make the cron line `17 * * * * docker exec -e IPNS_KEY=pyrate-player <kubo> sh /pin_player.sh`. Report the name the key prints (it begins `k51`): the radio's links will use it, so a browser keeps its settings and its stations across every release.
     Check it from outside: `https://<cid>.ipfs.dweb.link/` (the cid is in `radio/app/assets/open_players.txt`) should show the open player within a few minutes of pinning.
   - the APK over BitTorrent: add the `.torrent` built by `radio/tools/torrent.py` to the existing qBittorrent, pointing it at the APK file, and let it seed. The magnet and web seed are already on the landing page.
   - optional, an IPFS pin of the APK too: `docker exec <kubo> ipfs add -Q --cid-version=1 <apk>` and report the cid.

7. Check it:
   - harbour: `docker logs --tail 60 <harbour>` shows `LANTERN LIT`, `neighbour up`, `dropped … held stations`, and only its own stations under `in the Log:`. From inside the namespace, `docker exec gluetun wget -qO- http://127.0.0.1:<port>/log.json | head -c 300` returns JSON, `/player/` is not found, and `/feed/<station id>.xml` returns a feed for each of its own stations.
   - hold: `docker logs --tail 60 <hold>` shows `dark running: relays only, no address published`, `neighbour up`, and the full Log under `in the Log:`. It should list far more stations than the harbour.
   - confirm the hold has no published port of its own: `docker inspect <hold>` shows its network mode is gluetun's and its `Ports` are empty.

8. Report back: the commit both built from, each container's command line and environment as it now runs (redact nothing, this is yours), and the last twenty log lines of each.

What the two identities buy you:
- The harbour is a known station keeper and a bootstrap door. Anyone can see it. It serves only the stations whose keys you hold, so there is nothing on it you did not choose to put there.
- The hold carries everyone's sealed logs blind, the way a Tor relay carries traffic it cannot read. It is advertised nowhere, it publishes no address, and it speaks only through Tor. It is not linkable to the harbour, to the VPN exit, or to the house.
- Losing either one does not sink the network. Other harbours and the crew relays remain, and every node remembers the peers it has met, so there is never a single door to close.
