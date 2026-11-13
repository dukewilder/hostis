# Test harnesses outside cargo

The private test network, as in `AUDIT-BRIEF.md` section 11: a relay Lantern first, then
`PYRATE_RELAYS=http://127.0.0.1:3340` for everything.

- `cargo run -p pyrate-node --bin voyage_test <dir> <wav> <jpg>`: 22 checks end to end.
- `cargo run -p pyrate-node --bin pocket_test <lantern id> <wav>`: a pocket station, a member
  and an outsider. With `PYRATE_BROWSER_INVITE=<file>` it writes the invitation there and waits
  `PYRATE_BROWSER_WAIT` seconds for a browser to join and speak.
- `browser_pocket.py <invite file> <port>`: drives the browser radio into that pocket.
- `browser_play.py <port>`: plays the first station a tab finds.

For the browser scripts, build a test page with `PYRATE_HARBOUR="<lantern id>@http://127.0.0.1:3340"
PYRATE_RELAYS=http://127.0.0.1:3340 python3 browser/build.py`, copy `browser/www/dist` aside and
serve it (`python3 -m http.server <port>`), then rebuild the public page with plain
`python3 browser/build.py`. Playwright's Chromium: `PLAYWRIGHT_BROWSERS_PATH=/opt/pw-browsers`.
- `live_page.py <url>`: loads the deployed page and checks the wasm starts, after every deploy.
- `browser_ui.py <port>`: quiet console, chatter join and leave, passed-on links that never name the serving address.
- `lantern_page.py`: a Lantern started with `--http 127.0.0.1:8383` serves a page whose script runs under its policy.
- `lantern_stream.py`: plays a station from that Lantern page, breaks the stream, and checks the page reconnects.
- `browser_trap.py`: a simulated wasm trap; the set comes back aboard once on the same station, then asks for a reload.
- `browser_hidden.py <port>`: a hidden tab keeps more than a minute queued.
- `serve_test.sh <relay id> <station id>`: builds both copies against the test network: the open player on 8099, hostis.black's copy on 8097 (with the given station standing in for HOSTIS's), then rebuilds the public copies.
- `lantern_player.sh [port]`: after `serve_test.sh`, a Lantern with `--http` and `--player /tmp/dist-test` serves the open player at /listen/; every file comes back byte for byte with its type, nothing outside the folder can be had, /player/ moves to /listen/, and a tab plays from the Lantern's copy (loopback and this machine's own address, with no secure context) as from 8099.
- `browser_scope.py <station id>`: hostis.black's copy plays only HOSTIS's stations, hands a stranger's station, an invitation or a flock straight to the open player, and shows no chatter.
- `browser_raise.py <wav>`: a tab raises a station in the open player, puts the recording on it and sends it along the stem; a fresh tab finds and plays it after the first has closed. Needs a Lantern that carries every station (no `--carry-own`) on the test network.
- `browser_live.py`: a tab goes live from a fake microphone; a second tab hears the station live.
