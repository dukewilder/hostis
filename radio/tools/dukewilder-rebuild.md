Rebuild the Pyrate Radio Lantern on dukewilder from the latest `radio` branch of github.com/dukewilder/hostis, keeping its data, its flags and its network exactly as they are.

Rules that hold throughout:
- The house IP must never be exposed. The Lantern stays inside gluetun's network namespace. No port forwards, no router changes, no new published ports.
- Ask Dylan before deleting any file, image or volume. Old images can stay.
- Do not touch the Jellyfin, arr or Monero containers.

1. Find the running Lantern and record how it runs:
   `docker ps --format '{{.Names}}\t{{.Image}}\t{{.Status}}'`
   then `docker inspect <lantern container>` and note its image, its full command and entrypoint arguments, its environment, its mounts (the data volume), its restart policy, and its network mode (it should be `container:gluetun` or `service:gluetun` in compose). If it runs from a compose file, note the file and the service name.

2. Update the code in the hostis checkout the image was built from:
   `git fetch origin radio && git checkout radio && git pull --ff-only origin radio`
   `git log --oneline -1` should show `0a0232d` or later.

3. Build the new image from the repository root:
   `docker build -t pyrate-lantern -f radio/Dockerfile radio`
   It compiles Rust inside the build stage and takes a while on the FIREBAT. If it fails, stop and report the error; do not change the Dockerfile.

4. Recreate the Lantern with exactly what step 1 recorded: same name, same arguments, same environment, same data volume, same network mode, same restart policy. With compose: `docker compose up -d --no-deps --force-recreate <service>`. Keep any `--seed`, `--http`, `--relay`, `--release-keys` and `--quota-gb` arguments it had.

5. Check it:
   - `docker logs --tail 60 <lantern>` shows `LANTERN LIT`, `neighbour up`, and the HOSTIS stations under `in the Log:`. No lines starting `chatter:` with errors.
   - If it runs the web listener (`--http`), from inside the namespace: `docker exec gluetun wget -qO- http://127.0.0.1:<port>/log.json | head -c 300` returns JSON, and `/feed/<station id>.xml` returns a podcast feed for each of this Lantern's own stations.

6. Report back: the commit it built from, the container's command line as it now runs, and the last twenty log lines.

What the new version brings, so you know what to look for:
- Chatter rooms. The Lantern sits in the rooms of its own stations and the busiest in the Log, so listeners who join find each other and the last lines. Nothing is written to disk.
- Seed folders may hold a `programme.json` (the station's schedule) and a `notices.txt` (one notice per paragraph). See `radio/LANTERN.md`. Leave them out for now unless Dylan asks.
- The web listener serves each own station as a podcast feed at `/feed/<station id>.xml`.
- The bell no longer overwrites a programme.
