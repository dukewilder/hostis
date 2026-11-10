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
