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

Do not print any key, token or the words file.

Report: the new image id, each Lantern's first "in the Log" line and station count, the checks in step 3, and the memory, disk and network figures from steps 1 and 4.
