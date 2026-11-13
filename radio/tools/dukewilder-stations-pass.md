# dukewilder: the stations pass

Paste this to the Claude on dukewilder after the 0.9.16 Lantern rebuild. It changes HOSTIS's own stations on the harbour (pyrate-lantern) and adds one. The Numbers, Shanties and the other two Lanterns are left alone.

---

Work through the steps in order and report at the end.

## Rules

- Everything that reaches the internet goes through the harbour's VPN network: the git fetch and every download below. No published ports, no router changes.
- Do not touch Jellyfin, the arr stack, Monero, the main gluetun, pyrate-hold, pyrate-releases or pyrate-tunnel.
- Delete nothing. Old recordings and replaced files are moved into a `<station>-retired` folder beside the seed folder. Ask Dylan before deleting anything, those folders included.
- Never print a WireGuard key, a Tor secret, or any Lantern's `hold.key`, `node.key`, `seed.key`, `keys/` files or words file. Report paths, never contents.

## 1. Before changing anything

1. Fetch the latest commit of branch `radio` through the tunnel, the same way as the last rebuild.
2. Confirm the harbour's binary knows `--reseed`. This prints the Lantern's flags and stops without starting a node:
   ```
   docker run --rm --network none pyrate-lantern:latest --no-such-flag
   ```
   If `--reseed` is not in the list, stop: the 0.9.16 rebuild has not been done.
3. From `docker inspect pyrate-lantern`, note:
   - every `--seed "Name|About|/folder|kind"` argument, in order, and the host folder behind each `/folder`;
   - the parent host folder that holds the seed folders (call it `<seed parent>` below);
   - the network mode, from `docker inspect -f '{{.HostConfig.NetworkMode}}' pyrate-lantern`;
   - the user the container runs as.
4. Run `df -h` on the disk that holds `<seed parent>` and on the disk that holds the harbour's data folder. This pass needs about 2 GB free on the first and 1 GB on the second. If either has less, stop and report.
5. For the record, write a list of The Public Domain's and Readings' recordings to `~/pyrate-stations-list.txt`: for each of the two seed folders, every audio file's name and size, and the one line of its `.source.txt` if it has one. Report the path, the number of files in each folder and their total size. These are not secret.
6. Note the harbour's last `in the Log` line and its station count.

## 2. Fetch the new recordings, while the harbour runs

The station kits are in `<checkout>/radio/stations/` and the manifests in `<checkout>/radio/tools/stations/`.

1. Netlabel's recordings are replaced. Its old list was licensed CC BY-NC and BY-ND, which HOSTIS does not air; the new one is CC BY, CC BY-SA and CC0 only. Fetch it into a new folder beside the seed folders, so the running harbour does not see it yet:
   ```
   docker run --rm --network <harbour network mode> \
     -v <checkout>/radio/tools/stations:/t:ro -v <seed parent>/netlabel-new:/out \
     python:3.12-slim python /t/fetch.py /t/netlabel.json /out
   ```
   It is 87 recordings, about 870 MB, and ends with `87 recordings in /out, 0 failed`.
2. Contraband is a new station. Copy `<checkout>/radio/stations/contraband` into `<seed parent>`, keeping its name, and fetch its recordings into it the same way, with `/t/contraband.json` and `<seed parent>/contraband` as `/out`. It is 68 recordings, about 500 MB, and ends with `68 recordings in /out, 0 failed`. Every recording must be in the folder before the station is seeded in step 4, because it plays its books in the order they were set.

Run either again if anything failed: it fetches only what is missing. Every file is checked against its archive.org SHA-1.

## 3. Stop the harbour and rearrange the folders

Stop pyrate-lantern (`docker compose stop pyrate-lantern`, or however it was started). Then, with the harbour stopped:

1. **Netlabel.** Move every audio file and `.source.txt` in `<seed parent>/netlabel` into a new folder `<seed parent>/netlabel-retired`. This does it, and prints only counts:
   ```
   docker run --rm --network none \
     -v <checkout>/radio/tools/stations:/t:ro -v <seed parent>:/p \
     python:3.12-slim python /t/tidy.py /t/netlabel.json /p/netlabel /p/netlabel-retired
   ```
   It should end `0 renamed, 97 moved to /p/netlabel-retired, 0 of 87 in place`. Then move every file from `netlabel-new` into `netlabel`. The empty `netlabel-new` folder can stay.
2. **Poets' Corner.** Its recordings stay; their names are corrected (poets' full names, titles as written, one reading of each poem). The same tool renames each file by its checksum and moves the six it drops into `<seed parent>/poets-corner-retired`:
   ```
   docker run --rm --network none \
     -v <checkout>/radio/tools/stations:/t:ro -v <seed parent>:/p \
     python:3.12-slim python /t/tidy.py /t/poets.json /p/poets-corner /p/poets-corner-retired
   ```
   It should end `33 renamed, 6 moved to /p/poets-corner-retired, 174 of 174 in place`. Then run `fetch.py` with `/t/poets.json` and `<seed parent>/poets-corner` as `/out`, through the harbour's network as in step 2. It downloads nothing and rewrites each credit, ending `174 recordings in /out, 0 failed`.
3. **The kits.** Copy these files from `<checkout>/radio/stations/<station>/` into each station's seed folder. Where a file of the same name is already there, move the old one into that station's `-retired` folder first.

   | Station | Seed folder | Files |
   |---|---|---|
   | The Public Domain | its own, from step 1 | `bell.ogg`, `programme.json`, `notices.txt` |
   | Readings | its own, from step 1 | `station.jpg`, `banner.jpg`, `bell.ogg`, `programme.json`, `notices.txt` |
   | Netlabel | `netlabel` | `station.jpg`, `banner.jpg`, `bell.ogg`, `programme.json`, `notices.txt` |
   | Poets' Corner | `poets-corner` | `station.jpg`, `banner.jpg`, `bell.ogg`, `programme.json`, `notices.txt` |

   The kit for The Public Domain is in `radio/stations/the-public-domain` and for Readings in `radio/stations/readings`, whatever their seed folders are called here.
4. Make sure the harbour's user can read every file in all five folders.

## 4. Start the harbour with the new seeds

Edit only the harbour's compose entry, and only these arguments. Keep every other argument, environment value, mount and the order of the seeds exactly as they are. Use each seed's existing folder path, as the container sees it.

1. Four seeds get new words, and The Public Domain becomes `mono`:
   ```
   --seed "The Public Domain|Music and speech off records from 1925 and before. Each New Year's Day another year of them goes free.|<its path>|mono"
   --seed "Readings|Captain Johnson's General History of the Pyrates, The Pirates Own Book and Treasure Island, read aloud chapter by chapter.|<its path>|talk"
   --seed "Netlabel|Electronic, ambient and experimental records their makers set free under Creative Commons. Ambient alone through the small hours.|<path>/netlabel|music"
   --seed "Poets' Corner|Blake, Dickinson, Shelley, Owen and over a hundred more poets, read aloud. Nobody owns a word of it.|<path>/poets-corner|talk"
   ```
   Poets' Corner's name has an apostrophe: quote it the way the compose file already does. Keep any home port a seed already carries, as a fifth field.
2. Add one seed after the last:
   ```
   --seed "Contraband|Books that kings, censors and courts tried to stop, read aloud round the clock. Milton, Paine, Voltaire, Douglass, Thoreau, Goldman.|<path>/contraband|talk"
   ```
3. For this one start only, add:
   ```
   --reseed "The Public Domain" --reseed "Netlabel" --reseed "Poets' Corner"
   ```
   `--reseed` sets every recording in that station's folder again with the seed's kind, then withdraws the old ones, so the station stays on the air throughout. The Public Domain is re-made as one channel at 32 kbit/s: old records are mono, and this halves what each listener of it costs the Lanterns. Netlabel's old recordings and Poets' Corner's old names go off the clock the same way.
4. Bring the harbour up and follow `docker logs -f pyrate-lantern`. Setting the recordings takes a while; The Public Domain is the slow one. Expect, for each station:
   - **The Public Domain:** `reseeding The Public Domain: N recordings set again`, a `set` line for each recording, then `reseeded The Public Domain: N old recordings withdrawn, 0 kept`, then `colours set`, `the bell is set`, `programme set`, `notice up`.
   - **Netlabel:** `reseeding Netlabel: 97 ...`, 87 `set` lines, `reseeded Netlabel: 97 old recordings withdrawn, 0 kept`, `colours set`, `cover raised`, `banner raised`, `the bell is set`, `programme set`, a `notice taken down` and two `notice up`.
   - **Poets' Corner:** `reseeding Poets' Corner: 180 ...`, 174 `set` lines, `reseeded Poets' Corner: 180 old recordings withdrawn, 0 kept`, `colours set`, `cover raised`, `banner raised`, `the bell is set`, `programme set`, `notice taken down` and two `notice up`.
   - **Readings:** `colours set`, `cover raised`, `banner raised`, `the bell is set`, `programme set`, two `notice up`.
   - **Contraband:** a `set` line for each of 68 recordings, `kinds set`, `cover raised`, `banner raised`, `the bell is set`, `programme set`, three `notice up`.

   If Netlabel or Poets' Corner reports any number other than `0 kept`, stop and report the line: the recordings it kept must not stay on the air. If any `programme.json names a recording not on the clock` line appears, report it. It is done when the minute lines show `in the Log` again and no more `set` lines arrive.
5. Take the three `--reseed` arguments out and recreate the harbour once more. It prints one `station <name>: <id>` line for each seeded station. Report them all; Contraband's id is new and goes into the app.

## 5. Check

- `docker ps` shows the three Lanterns up, with no published ports.
- The harbour's station count in `in the Log` is one more than in step 1 (Contraband).
- GET https://relay.hostis.black/ping answers 200, and two minutes after the last restart the harbour's pkarr record still lists https://relay.hostis.black/.
- The memory of the harbour after ten minutes, and the size of its data folder before step 4 and now.
- No line containing `panicked` or `error` in the harbour's log since step 4.

## 6. Shanties, later

Shanties' recordings are being replaced in a separate brief; leave its seed as it is today. When that brief changes Shanties' seed, its kind should be `mono` and the first start after it should carry `--reseed "Shanties"`, for the same reason as The Public Domain.

## Report

- The `--no-such-flag` output line that lists the flags.
- The free space from step 1, the path of `~/pyrate-stations-list.txt` and its counts.
- The last line of each `fetch.py` and `tidy.py` run.
- Every `reseeding` and `reseeded` line, and any line step 4 says to report.
- All `station <name>: <id>` lines.
- The checks in step 5.
- The sizes of `netlabel-retired` and `poets-corner-retired`, and a question to Dylan: may they be deleted? Delete nothing until he says yes.
- Anything that did not go as written.
