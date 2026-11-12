Pyrate Radio 0.9.12 is a security release. Please update the three Lanterns.

1. Tag the running image pyrate-lantern:prev-2026-11-11d first. Then rebuild the three Lanterns at commit 643d7aa or later (branch radio), the same way as last time: binary built in rust:bookworm inside the VPN namespace, then put onto the previous image. Keep every compose entry exactly as it is, pyrate-tunnel included.

   What changed for a Lantern: a stranger's stem is checked before it is relayed or held, and the stems it holds while they wait are capped at 256 and 256 MiB. Answers time out after 20 s. Station cards past fixed sizes are refused. Secret files are created private from their first byte. Nothing in the configuration changes.

2. After the restarts, check each Lantern:
   - its first "in the Log" line appears, and its station count is the same as before;
   - GET https://relay.hostis.black/ping still answers 200 (use GET, not HEAD);
   - two minutes after the harbour restarts, its pkarr record still lists https://relay.hostis.black/.

3. Then watch for ten minutes. Report the resident memory of each Lantern container at the start and end, and any line containing "panicked" or "error" in their logs.

4. Also report whether pyrate-tunnel restarted at any point since 00:10 UTC on 12 November, other than during this update.

Do not print any key, token or the words file. The words file stays where it is until Dylan says he has copied it.

Report: the new image id, each Lantern's first "in the Log" line and station count, the three checks in step 2, the memory figures, and the answer to step 4.
