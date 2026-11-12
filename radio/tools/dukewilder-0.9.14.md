Pyrate Radio 0.9.14 makes the radio start fast. Please update the three Lanterns. It replaces 0.9.13, so if 0.9.13 is not on them yet, go straight to 0.9.14.

1. Before changing anything, run docker stats once for the three Lantern containers and note each one's NET I/O sent figure, wait ten minutes, and note it again.

2. Tag the running image pyrate-lantern:prev-2026-11-13. Then rebuild the three Lanterns at the 0.9.14 commit on branch radio (the release commit is in SHIPS-LOG.md, or use the latest), the same way as last time: binary built in rust:bookworm inside the VPN namespace, then put onto the previous image. Keep every compose entry exactly as it is, pyrate-tunnel included.

   What changed for a Lantern: a newcomer can ask it for the stations it knows, and it answers with their signed cards and the carriers' own signed notes (which stations each carries, and its relay), so the newcomer can reach them through two hops at once. The answer is cached for five seconds and counted against the same memory budget as log pages, and the Lantern keeps up to a few megabytes of those notes. It now says each station it carries every four minutes instead of every ten seconds (and at once when a log grows), so its outgoing gossip should drop sharply. A reader that stops reading is let go after five seconds. The 0.9.13 log budget is in this release too. Nothing in the configuration has to change.

3. After the restarts, check each Lantern:
   - its first "in the Log" line appears, and its station count is the same as before;
   - within a minute of starting, a line of the form "N cards and M carrier notes from <id>" appears in the log of each Lantern that bootstraps from another;
   - GET https://relay.hostis.black/ping still answers 200 (use GET, not HEAD);
   - two minutes after the harbour restarts, its pkarr record still lists https://relay.hostis.black/.

4. Then watch for thirty minutes. Report the resident memory of each Lantern container at the start, after ten minutes and at the end; the size of each Lantern's data folder before and after; the NET I/O sent figure at the ten-minute mark and again ten minutes later, as in step 1; and any line containing "panicked" or "error" in their logs.

5. Then two of HOSTIS's stations change: The Numbers gets new recordings and loses its description, and Shanties is a new station. Find the harbour's seed folders and their parent with `docker inspect pyrate-lantern`, as in the 0.9.10 brief.
   - Fetch the `numbers-audio` branch the same way as `radio`. It holds only The Numbers' recordings, `001.mp3` to `058.mp3`, about 165 MB.
   - In the parent folder, make a folder `the-numbers-retired` and move into it every file in The Numbers' seed folder except `station.jpg` and `banner.jpg`: the twelve transmissions, the three interval signals, their `.source.txt` files, `bell.ogg`, `programme.json` and `notices.txt`. Dylan wants them moved, not deleted.
   - Copy into The Numbers' seed folder every `.mp3` from `numbers-audio`, and `programme.json`, `notices.txt` (it is empty), `station.jpg` and `banner.jpg` from `radio/stations/the-numbers`, replacing the two pictures.
   - Copy `radio/stations/shanties` into the parent folder, keeping its name, and fetch its recordings through the harbour's VPN network so the house address never reaches archive.org:

```
docker run --rm --network <harbour network mode> \
  -v <checkout>/radio/tools/stations:/t:ro -v <seed parent>/shanties:/out \
  python:3.12-slim python /t/fetch.py /t/shanties.json /out
```

     It is 64 recordings, about 310 MB, and ends with `64 recordings in /out, 0 failed`. Run it again if anything failed. Make sure the harbour's user can read both folders.
   - In the harbour's compose entry, give The Numbers an empty About, `--seed "The Numbers||<path>/the-numbers|talk"`, and add `--seed "Shanties|Sea shanties, sea songs and pirate songs on records from 1925 and before, all in the public domain.|<path>/shanties|music"` after the last seed. For this one start, add `--withdraw "hostis transmission" --withdraw "hostis interval signal"` as well, which takes the old recordings off The Numbers. Keep everything else exactly as it is, and recreate the harbour.
   - Its log should show a `withdrew` line for each of the fifteen old recordings and no other; if any other title is withdrawn, stop and report it. The Numbers then logs `set 001` to `set 058`, `colours set`, `banner raised`, `programme set` and `notice taken down`. Shanties logs a `set` line for each recording, then `cover raised`, `banner raised`, `the bell is set`, `programme set` and `notice up`. It is done when the minute lines show `in the Log` again and no more `set` lines arrive.
   - Take the two `--withdraw` arguments out and recreate the harbour once more. It prints one `station <name>: <id>` line for each seeded station.

Do not print any key, token or the words file.

Report: the new image id, each Lantern's first "in the Log" line and station count, the checks in step 3, the memory, disk and network figures from steps 1 and 4, the `withdrew` lines, and the `station Shanties: <id>` line.
