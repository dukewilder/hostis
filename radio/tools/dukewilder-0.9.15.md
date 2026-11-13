Pyrate Radio 0.9.15 makes the Lanterns answer listeners quickly. Please update the three Lanterns and run a few checks. Nothing in the configuration has to change.

1. Before changing anything:
   - For each of the three Lantern containers, report the "node id:" line from the start of its log (`docker logs <container> 2>&1 | grep -m1 "node id"`) and the container's name. Ids are not secret.
   - For the harbour (pyrate-lantern), report only the values of PYRATE_RELAY_URL and PYRATE_RELAY_LOCAL from `docker inspect`, if they are set. Print nothing else from any container's environment.
   - Run `docker stats --no-stream` once, and note CPU, memory and NET I/O for the three Lanterns, the harbour's gluetun and the hold's network container.

2. Time the relays from inside each Lantern's network. For the harbour and for the hold, run this five times each, with `--network` set to that Lantern's network mode (`docker inspect -f '{{.HostConfig.NetworkMode}}' <container>`):
   ```
   docker run --rm --network <network mode> curlimages/curl -s -o /dev/null -w "%{time_connect} %{time_starttransfer} %{time_total}\n" https://relay.hostis.black/ping
   docker run --rm --network <network mode> curlimages/curl -s -o /dev/null -w "%{time_connect} %{time_starttransfer} %{time_total}\n" https://use1-1.relay.n0.iroh.link/ping
   ```
   Report all twenty lines. They show whether the harbour's own path out is slow; at the moment it answers listeners five to twenty times slower than the hold.

3. Tag the running image `pyrate-lantern:prev-2026-11-13`. Then rebuild the three Lanterns at the latest commit on branch `radio`, the same way as last time: binary built in rust:bookworm inside the VPN namespace, then put onto the previous image. Keep every compose entry, harbour.sh and pyrate-tunnel exactly as they are.

   What changed for a Lantern:
   - It no longer asks every other carrier before it answers a listener's request for a log through two hops. It answers at once from what it holds, a quarter of a megabyte at a time, and catches up in the background. Before, those answers ran out the listener's eight seconds.
   - A chunk it lacks is fetched from three carriers at once.
   - It writes "a hop to <id> failed: <reason>" to its log, at most once a minute, when it cannot reach the next radio in a route.
   - A station seeded with no kinds takes music or talk from its seed, so it shows in the player's filters. Each seeded station logs "kinds set" once.
   - It understands the new end-of-broadcast entry.

4. After the restarts, check each Lantern:
   - its first "in the Log" line appears and its station count is the same as before;
   - GET https://relay.hostis.black/ping answers 200;
   - two minutes after the harbour restarts, its pkarr record still lists https://relay.hostis.black/.

5. Watch for twenty minutes. Report:
   - every line containing "a hop to" (they name radio ids only);
   - any line containing "panicked" or "error";
   - the memory of each Lantern at the start and at the end.

Do not print any key, token or the words file.

Report: the node ids and names from step 1, the relay environment values, the docker stats lines, the twenty timing lines, the new image id, the checks in step 4 and everything from step 5.
