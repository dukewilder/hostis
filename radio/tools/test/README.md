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
- `net_music.py [<music dir>]`: the recordings net_up.sh, doors_up.sh and the sweep expect (Gangway's Harbour Lights and Low Tide in gang/, Second Watch's Lamplight and Second Watch in watch/, four-note cycles, and a station.jpg each), made only where a folder has none. all.sh runs it first.
- `serve_test.sh <relay id> <station id>`: builds both copies against the test network: the open player on 8099, hostis.black's copy on 8097 (with the given station standing in for HOSTIS's), then rebuilds the public copies.
- `lantern_player.sh [port]`: after `serve_test.sh`, a Lantern with `--http` and `--player /tmp/dist-test` serves the open player at /listen/; every file comes back byte for byte with its type, nothing outside the folder can be had, /player/ moves to /listen/, and a tab plays from the Lantern's copy (loopback and this machine's own address, with no secure context) as from 8099.
- `browser_scope.py <station id>`: hostis.black's copy plays only HOSTIS's stations, hands a stranger's station, an invitation or a flock straight to the open player, and shows no chatter.
- `browser_raise.py <wav>`: a tab raises a station in the open player, puts the recording on it and sends it along the stem; a fresh tab finds and plays it after the first has closed. Needs a Lantern that carries every station (no `--carry-own`) on the test network.
- `browser_live.py`: a tab goes live from a fake microphone; a second tab hears the station live.
- `players_sigs.py <release binary>`: throwaway release keys sign `players.txt` with `release sign-all`; `verify_players.py` and `pin_player.sh` (with a stand-in ipfs) move the player's fixed name only for a list two keys signed, keep it on the last signed player otherwise, and refuse a shorter list.
- `hum_comb.js` (node): the encoder's hum comb takes every harmonic of 50 and 60 Hz up to 1 kHz down by more than 40 dB and lets the tones between them through.
- `rustc --edition 2021 --test browser/src/pure.rs -o /tmp/pure && /tmp/pure`: the tab's rules with no network in them (quarter-hour stamps, the held track's chunk).
- `browser_held.py [port]`: with no network, a station raised in the studio holds its entries in the tab until their quarter-hour stamp, keeps them sealed, takes them back after a reopening (inside the wait or after it) and lets them go when the stamp comes. The clock is moved through Date.now.
- `browser_fifth.py [port]`: with no network, a test copy names no station in its title, keeps hidden stations, the chat name and its keys sealed, cleans studio titles by the app's rules, never dials a door a link alone names, and lets go of its worker and caches on Forget and stays stopped.
- `intro_up.sh [<music dir>]` then `browser_intro.py`: introduced doors. A relay that can be stopped on its own, a harbour, a Lantern that introduces, a listener's app that is a door (no listed door anywhere, no port mapped) and one that is not. A tab joins through the relay, a middle introduces it to the app door, it links to it directly over WebRTC with full ICE, the door relays its hops (not its guard: one middle drew it), and with the relay stopped the tab keeps playing on fresh sound through the door. The station's recording must be longer than the test (seven minutes, say).
