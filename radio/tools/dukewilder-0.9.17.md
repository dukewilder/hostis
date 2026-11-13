Pyrate Radio 0.9.17: thank you for the 0.9.16 report. This follow-up:
- lets the three Lanterns reach each other directly;
- rebuilds them so the hold stops drawing routes it can't answer;
- fetches the 24 blocked shanties;
- deletes the retired folders, which Dylan approved.

Rules as before: print no keys, tokens, words or `keys/` contents; don't touch Jellyfin, the arr stack, Monero or the main gluetun; publish no ports. If anything fails, stop and report.

Part 1. Direct paths between the Lanterns (Dylan approved)
1. Report the Docker networks that gluetun-radio and gluetun-ipfs are attached to, with each network's subnet: `docker inspect -f '{{json .NetworkSettings.Networks}}' <container>`. Subnets are private addresses, not secret.
2. In the compose entries for gluetun-radio and gluetun-ipfs only, set FIREWALL_OUTBOUND_SUBNETS to the subnets of those Docker networks, comma-separated. If either already had a value, keep it and add to it. Change nothing else in either entry. This only allows the VPN containers to talk to each other inside dukewilder. Nothing is published, and nothing leaves except through the VPN.
3. Recreate the two gluetun containers, then the three Lanterns that use them, with `/srv/pyrate/MAINTENANCE` set while you work so the watchdog stays quiet. Remove it after.
4. Check that both VPNs are connected again (gluetun's log says the VPN is up and reports a public IP; don't report that IP). Check that the relay ping (https://relay.hostis.black/ping) answers 200.

Part 2. Rebuild
Tag the running image `pyrate-lantern:prev-2026-11-13c`. Rebuild the three Lanterns at the latest commit on branch `radio`, the same way as before. What changed: a Lantern running dark (the hold) no longer announces the stations it carries, because nobody can open a route to it. It still keeps and syncs everything. Recreate all three Lanterns. Check the "in the Log" lines, the station counts (7 harbour, 7 or more hold, 6 releases) and the relay ping. Report the new image id.

Part 3. The 24 blocked shanties
The fetch script now identifies itself to the Library of Congress and Wikimedia, and slows down for them. Run the Shanties fetch again exactly as in `radio/tools/dukewilder-shanties-more.md` step 2, from the new checkout. Expect it to end with `123 recordings in /out, 0 failed`. It may take several minutes, since it pauses between those hosts. If some still fail, report their names and the error, and carry on. Then wait up to 15 minutes for the harbour to set the new files on its own (its seed loop picks up new files in the folder; no `--reseed`). Report the number of `set` lines for Shanties.

Part 4. Retired folders (Dylan approved)
Delete these folders and nothing else: netlabel-retired, poets-corner-retired, the-public-domain-retired, readings-retired, the-numbers-retired. Before deleting, list each folder's full path and size; after, confirm that each path is gone.

Part 5. Watch
Thirty minutes after the last Lantern restart, report:
- every `paths:` line from the harbour and from pyrate-releases. I expect `direct` between those two now. The hold prints none, since it runs dark;
- any "a hop to", "panicked" or "error" lines (ignore poem titles);
- memory of each Lantern at start and end.

Report, in order:
1. The networks and subnets, and the before and after FIREWALL_OUTBOUND_SUBNETS values.
2. The checks from Parts 1 and 2, and the image id.
3. The Shanties fetch's last line and set count.
4. The deleted paths.
5. The Part 5 lines.
