Pyrate Radio 0.9.13 is a security release. Please update the three Lanterns.

1. Tag the running image pyrate-lantern:prev-2026-11-12 first. Then rebuild the three Lanterns at the 0.9.13 commit on branch radio (the release commit is in SHIPS-LOG.md, or use the latest), the same way as last time: binary built in rust:bookworm inside the VPN namespace, then put onto the previous image. Keep every compose entry exactly as it is, pyrate-tunnel included.

   What changed for a Lantern: logs it carries for other stations now share a budget of 1 GB, no station more than 256 MB, and a station nobody listens to through it goes first when it is full. Its own stations are never counted. Log replies go out in pages of 2 MiB. Its web page builds each station's view once per change. Once a minute it hands spare memory back to the system. Nothing in the configuration has to change. If a Lantern's disk is small, `-e PYRATE_LOG_MB=<megabytes>` sets the budget; leave it out otherwise.

2. After the restarts, check each Lantern:
   - its first "in the Log" line appears, and its station count is the same as before;
   - GET https://relay.hostis.black/ping still answers 200 (use GET, not HEAD);
   - two minutes after the harbour restarts, its pkarr record still lists https://relay.hostis.black/.

3. Then watch for thirty minutes. Report the resident memory of each Lantern container at the start, after ten minutes and at the end, the size of each Lantern's data folder before and after, and any line containing "panicked" or "error" in their logs.

4. Also report whether pyrate-tunnel restarted at any point since the 0.9.12 update, other than during this update.

Do not print any key, token or the words file.

Report: the new image id, each Lantern's first "in the Log" line and station count, the three checks in step 2, the memory and disk figures, and the answer to step 4.
