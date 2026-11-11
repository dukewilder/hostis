# dukewilder: rebuild the Lanterns for 0.9.8

Paste this to the Claude on dukewilder.

---

Rebuild the three Pyrate Radio Lanterns (pyrate-lantern, pyrate-hold, pyrate-releases) from the radio branch of github.com/dukewilder/hostis at commit 4fa68b2 or later, the same way as the last rebuild in ~/pyrate-rebuild-report.txt: fetch through the tunnel, build the image in the rust:bookworm container, tag the old image pyrate-lantern:prev-2026-11-11 first, then recreate the three containers with their existing compose entries unchanged. This version signs its carrier notes and starts stems two hops out; phones on 0.9.8 rely on both.

Rules, as before: no published ports; do not touch Jellyfin, the arr stack, Monero or the main gluetun; ask before deleting anything; never print a WireGuard key, a Tor hs_ed25519_secret_key, or any Lantern's hold.key, node.key, seed.key or keys/ files.

New in this version, and worth doing while you are there:
1. Each Lantern now warns at start that its hold key lies beside its data. For pyrate-hold, put the hold key on a different disk: copy /srv/pyrate/hold/hold.key (or wherever its data volume keeps it) to a directory outside that volume, mount that directory into the container, set PYRATE_HOLD_KEY to the key's path inside the container, start it, confirm the warning is gone and the Lantern still opens its hold (its station count matches before and after), and only then ask me before removing the old copy.
2. Add PYRATE_QUIET=1 to pyrate-hold if you want it to stop logging station and peer names. Leave the harbour as it is.

Report: the new image id, each container's first lines after start (with the node id and "in the Log"), that the warning is gone for the hold, and that the harbour still lists its relay as http://45.87.214.21:42779/.
