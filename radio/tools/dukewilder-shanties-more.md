Shanties gets 59 more recordings, newer ones among them, and moves to the `mono` kind. Use a Lantern build that knows `mono` and `--reseed` (commit c524544 or later).

1. Pull `radio` as usual. Find the harbour's seed parent with `docker inspect pyrate-lantern`, as in `radio/tools/dukewilder-0.9.10.md`, and its network mode with `docker inspect -f '{{.HostConfig.NetworkMode}}' pyrate-lantern`.

2. Copy `radio/stations/shanties/notices.txt` over the one in the Shanties seed folder. Then fetch the new recordings through the harbour's VPN network, as before:

```
docker run --rm --network <harbour network mode> \
  -v <checkout>/radio/tools/stations:/t:ro -v <seed parent>/shanties:/out \
  python:3.12-slim python /t/fetch.py /t/shanties.json /out
```

   The 64 recordings already there are left alone. It adds about 330 MB and ends with `123 recordings in /out, 0 failed`; run it again if anything failed. Some of the new files are `.ogg`, and some come from the Library of Congress and Wikimedia Commons as well as archive.org. Make sure the harbour's user can read them.

3. In the harbour's compose entry, change the Shanties seed to `--seed "Shanties|Sea shanties, sea songs and pirate songs: records from 1925 and before, field recordings the U.S. Government made, the Navy and Marine bands, and newer recordings their makers freed under Creative Commons.|<path>/shanties|mono"`. Keep everything else exactly as it is, and recreate the harbour. Within ten minutes it logs a `set` line for each new recording and `notice up`.

4. Optional, and only if Dylan agrees: put the 64 older recordings on `mono` too, which halves what they cost to stream. Add `--reseed "Shanties"` for one start and recreate the harbour. Every recording is set again, and once they are on the clock the old copies are withdrawn; expect a `set` line for each of the 123 and a line `reseeded Shanties: N old recordings withdrawn`. If any other station says reseeded, stop and report it. Then take `--reseed` out and recreate the harbour once more.

Report: the fetch's last line, the number of `set` lines for Shanties, and, if step 4 ran, its `withdrew` lines.
