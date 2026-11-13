Pyrate Radio 0.9.16: rebuild the three Lanterns, measure how they reach each other and how much they can send, then apply two station updates from the repo. Work through the parts in order and report at the end.

Rules for the whole job:
- Never print a WireGuard key, a Tor secret, a Cloudflare token, or any Lantern's `hold.key`, `node.key`, `seed.key`, `keys/` files or words file. Report paths, never contents.
- Don't touch Jellyfin, the arr stack, Monero or the main gluetun. Publish no ports.
- Delete nothing. Where the steps say to retire files, move them, as written.
- If anything fails or looks wrong, stop and report rather than improvising.

Part 1. Before changing anything
1. For each of the three Lantern containers, report its name and the "node id:" line from the start of its log (`docker logs <container> 2>&1 | grep -m1 "node id"`). Ids are not secret.
2. Run `docker stats --no-stream` once, and note CPU, memory and NET I/O for the three Lanterns, gluetun-radio and gluetun-ipfs.
3. Run `df -h` on the disk that holds the Lanterns' data and seed folders. Report free space.

Part 2. How much each Lantern network can send
For the harbour's network and for the hold's network (`docker inspect -f '{{.HostConfig.NetworkMode}}' <container>`), run each of these three times:
```
docker run --rm --network <network mode> --entrypoint sh curlimages/curl -c 'head -c 20000000 /dev/urandom | curl -s -o /dev/null -w "up %{speed_upload}\n" -X POST --data-binary @- https://speed.cloudflare.com/__up'
docker run --rm --network <network mode> curlimages/curl -s -o /dev/null -w "down %{speed_download}\n" "https://speed.cloudflare.com/__down?bytes=20000000"
```
Report every line, and convert each to megabits per second (bytes per second × 8 ÷ 1,000,000). These go through the VPN only. Do not run them on the host's own network.

Part 3. Rebuild the Lanterns
Tag the running image `pyrate-lantern:prev-2026-11-13b`. Rebuild the three Lanterns at the latest commit on branch `radio`, the same way as last time: build the binary in rust:bookworm inside the VPN namespace, then put it onto the previous image. Keep every compose entry, harbour.sh and pyrate-tunnel exactly as they are.

What changed for a Lantern:
- New stations wait a day before strangers see them on the dial, and a keeper may keep a station off the dial. Lanterns carry and pass on the new entry for this.
- A seed may be of kind `mono` (old records recorded in one channel: half the bytes, same sound).
- `--reseed "Station name"`, for one start, sets every recording in that station's folder again. Once the new ones are playing, it withdraws the old ones, and also any whose file has left the folder.
- Every ten minutes, a Lantern logs a line starting `paths:` that shows how it reaches each radio it talks to: direct or through a relay, and the round trip.

After the restarts, check each Lantern:
- its first "in the Log" line appears;
- its station count matches Part 1;
- GET https://relay.hostis.black/ping answers 200.

Report the new image id.

Part 4. Paths between the Lanterns
Fifteen minutes after the restarts, report every line starting `paths:` from each of the three Lanterns. They name radio ids only. I want to see whether the harbour reaches the hold and the releases Lantern directly or through a relay, and how long a round trip takes. If all three show only relay paths between each other, also report, for each of gluetun-radio and gluetun-ipfs, the value of FIREWALL_OUTBOUND_SUBNETS from `docker inspect` (just that one variable, nothing else). Change nothing.

Part 5. The stations pass
Follow `radio/tools/dukewilder-stations-pass.md` from the same checkout, start to finish. It:
- replaces Netlabel's recordings with freely licensed ones;
- corrects Poets' Corner's credits;
- gives The Public Domain and Readings new kits;
- moves The Public Domain to mono;
- adds a new station, Contraband.
Retired files go into `-retired` folders. Delete nothing.

Part 6. More shanties
Then follow `radio/tools/dukewilder-shanties-more.md`, including its step 4: Dylan agrees to moving Shanties to mono with `--reseed "Shanties"`. Take `--reseed` out again afterwards, as that file says.

Part 7. Watch
Watch all three Lanterns for twenty minutes after the last restart. Report:
- any line containing "a hop to", "panicked" or "error";
- the memory of each Lantern at the start and at the end.

Report, in this order:
1. The ids, docker stats and free space from Part 1.
2. The twelve speed lines and their Mbps from Part 2.
3. The new image id and the checks from Part 3.
4. The `paths:` lines from Part 4.
5. Everything each station file asks you to report, including every `station <name>: <id>` line. Contraband's id is new.
6. Everything from Part 7.
7. The sizes of the `-retired` folders. Ask Dylan whether they may be deleted, and delete nothing until he says yes.
