# dukewilder: Lanterns for 0.9.10, and three new HOSTIS stations

Paste this to the Claude on dukewilder. It supersedes `tools/dukewilder-0.9.8.md`: if that rebuild was never done, this one covers it.

---

Rebuild the three Pyrate Radio Lanterns (pyrate-lantern, pyrate-hold, pyrate-releases) from the radio branch of github.com/dukewilder/hostis at commit 6d30377 or later, then add three new stations to the harbour. Work through the steps in order and report at the end.

## Rules, as before

- No published ports. No router changes. Everything that reaches the internet goes through the VPN containers: the git fetch, the image build, and the downloads in step 4.
- Do not touch Jellyfin, the arr stack, Monero or the main gluetun.
- Ask before deleting anything.
- Never print a WireGuard key, a Tor `hs_ed25519_secret_key`, or any Lantern's `hold.key`, `node.key`, `seed.key` or `keys/` files. Report paths, never contents.

## 1. Fetch and build

1. Fetch the radio branch through the tunnel, the same way as the last rebuild in `~/pyrate-rebuild-report.txt`, and check out 6d30377 or later.
2. Tag the running image first: `docker tag pyrate-lantern:latest pyrate-lantern:prev-2026-11-11b`.
3. Build the new image the same way as last time, in the rust:bookworm container.

This version sends a broadcaster's station only through two other devices, and phones on 0.9.8 and later rely on that. It also refuses relay addresses that point at private networks, reads the hold key from `PYRATE_HOLD_KEY` with a one-time copy, and caps the memory a crowd of requests can take.

## 2. Move each Lantern's hold key off its data volume

Each Lantern warns at every start that its hold key lies beside the data it seals. Do this for all three, one at a time, starting with pyrate-hold:

1. Note the Lantern's station count from its last `in the Log` line and, for the harbour, its own stations from the `keeping` line.
2. Make a key folder outside every Docker volume, on a different disk if the box has one: `/srv/pyrate-keys/<lantern name>`, mode 700, owned by the user the container runs as.
3. Recreate the container with its compose entry unchanged except for two additions: mount that folder at `/keys`, and set `PYRATE_HOLD_KEY=/keys/hold.key`.
4. On first start the Lantern copies the old key into `/keys/hold.key`, checks the copy, and logs `the hold key is copied to PYRATE_HOLD_KEY; once this Lantern runs, remove the old hold.key beside the data`. Confirm that line, that the "lies beside the data" warning is gone, and that the station count matches step 1.
5. Do not remove the old `hold.key` from the data volume. Ask me first.

Also add `PYRATE_QUIET=1` to pyrate-hold, so it stops logging station and peer names. Leave the harbour's logging as it is.

## 3. The two stations the harbour already keeps get art

The repo now has a cover and a banner for every HOSTIS station in `radio/stations/<station>/station.jpg` and `banner.jpg`.

1. Find the harbour's seed folders: `docker inspect pyrate-lantern` shows its `--seed "Name|About|/folder|kind"` arguments and the mounts behind each folder.
2. Copy `radio/stations/the-public-domain/station.jpg` and `banner.jpg` into The Public Domain's seed folder, and `radio/stations/readings/station.jpg` and `banner.jpg` into Readings' seed folder. If a station.jpg or banner.jpg is already there, ask before replacing it.

The harbour raises the art within ten minutes, or at its next start.

## 4. Three new stations

The three seed folders are in `radio/stations/`. Each holds its bell, programme, notices and art; The Numbers holds its recordings too. Netlabel and Poets' Corner fetch theirs from archive.org.

1. Copy `radio/stations/the-numbers`, `radio/stations/netlabel` and `radio/stations/poets-corner` into the parent folder of the harbour's existing seed folders (the same host folder the harbour mounts them from), keeping those three folder names.
2. Fetch the recordings, through the harbour's own VPN network so the house address never reaches archive.org. Use the harbour's network mode from `docker inspect -f '{{.HostConfig.NetworkMode}}' pyrate-lantern`:

```
docker run --rm --network <harbour network mode> \
  -v <checkout>/radio/tools/stations:/t:ro -v <seed parent>/netlabel:/out \
  python:3.12-slim python /t/fetch.py /t/netlabel.json /out
docker run --rm --network <harbour network mode> \
  -v <checkout>/radio/tools/stations:/t:ro -v <seed parent>/poets-corner:/out \
  python:3.12-slim python /t/fetch.py /t/poets.json /out
```

   Netlabel is 97 recordings, about 690 MB. Poets' Corner is 180 recordings, about 175 MB. Each run ends with `N recordings in /out, 0 failed`; run it again if anything failed, since it fetches only what is missing. Every file is checked against its archive.org SHA-1, and each gets a `.source.txt` beside it with its credit and licence.
3. Make sure the harbour's user can read all three folders.
4. Recreate the harbour with its compose entry unchanged except for three more `--seed` arguments, after the existing ones, with the folder paths as the container sees them:

```
--seed "The Numbers|Five figures on the hour, every hour. Every group is a word, and the key is a book this radio reads.|<path>/the-numbers|talk"
--seed "Netlabel|Creative Commons electronic, ambient and experimental, from the netlabels that gave it away.|<path>/netlabel|music"
--seed "Poets' Corner|Short public domain poems, read aloud by LibriVox volunteers. A poem everyone should hear once, on the hour.|<path>/poets-corner|talk"
```

   Keep the existing seeds exactly as they are, in the same order.
5. Watch `docker logs -f pyrate-lantern`. Each new station logs `set <title>` for every recording, then `cover raised`, `banner raised`, `programme set` and `notice up`. Setting 290 recordings takes a while: Netlabel is the slow one. It is done when the minute lines show `in the Log` again and no more `set` lines arrive.
6. Then restart the harbour once more so it prints its stations at start. It logs one `station <name>: <id>` line for each seeded station.

## 5. The station keys, for Dylan

The harbour holds the master key of every HOSTIS station it raised, sealed in its hold. Dylan needs each station's 24 words on paper. Once step 4 is finished:

1. Stop the harbour.
2. Run the same image once with the harbour's volumes, `PYRATE_HOLD_KEY` and `/keys` mount exactly as its compose entry has them, the harbour's network mode, and one more mount: a folder in Dylan's home directory, mode 700, at `/words`. Pass only `--dir` as the harbour does and `--words /words/pyrate-station-words.txt`. It writes the words of every station it holds the master key for and stops. Its last line reads `the 24 words of N stations are in ...`.
3. Start the harbour again and check it comes back as in step 4.
4. Do not open, print, copy, upload or summarise that file, and never put it anywhere synced, shared or inside a Docker volume. Tell Dylan its full path and the simplest way for him to open it on this machine himself, then remind him to delete it once the words are on paper. Ask before deleting it yourself.

## 6. Check

- `docker ps` shows all three Lanterns up, with no published ports.
- Each Lantern's first lines after start show `LANTERN LIT`, its node id, and later an `in the Log` count matching what it had before.
- The harbour still lists its relay as `http://45.87.214.21:42779/` (or whatever `PYRATE_RELAY_URL` held before), and `relay on` appears in its log.
- No `programme.json names a recording not on the clock` lines for the three new stations after the recordings are all set. If any appear, report them.

## Report

- The new image id.
- For each Lantern: its node id and its first `in the Log` count after the rebuild, and that the hold key moved, with its new path. Confirm the warning is gone. Leave the old key in place.
- The `station <name>: <id>` lines for all five HOSTIS stations.
- The count N from step 5, and where you told Dylan the words file is. Never its contents.
- Each `fetch.py` run's last line.
- Anything that did not go as written.
