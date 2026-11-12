# Pyrate Radio: handoff and audit brief

For a fresh session picking this up cold. Read this whole file first, then `DECISIONS.md` (the Ship's Articles) and `SHIPS-LOG.md`. The mandate in section 7 is a full audit of every feature and every privacy and decentralisation property. Section 10 is how the work is done: every change is committed, pushed and taken live as soon as it is built and tested. Section 11 sets up a fresh container.

---

## 1. What this is

Pyrate Radio is the app arm of HOSTIS (hostis.black). It is a takedown-resistant, peer-to-peer radio network: anyone raises a station from their phone, anyone listens in the app or any browser, and the stations stay on the air with no server, carried between the listeners themselves. Think the Tor, Signal, Freenet or BitTorrent of radio, and the aim is to be better than any of them at being a genuinely good, free radio app as well as a hardened one.

The product comes first. It has to be a radio app someone would choose for the experience, not only for the politics. Privacy, anonymity and decentralisation are the ground it stands on, not the pitch. The look follows hostis.black's design system exactly (bone on near-black, Cinzel and IM Fell English and JetBrains Mono, the blood diamond, engraved and sober). No amber, no glow, no SaaS-card kit.

## 2. The mythos and philosophy (HOSTIS)

Read the site before auditing for fit: the full HOSTIS site source is in this repo at `/home/claude/hostis/index.html` (WebFetch of hostis.black is not allowlisted here, so read the local file). HOSTIS is an anti-enclosure project. Its name is from `hostis humani generis`, the enemy of all mankind, the old law's words for the pirate. The frame: enclosure of the commons, of speech, of culture and of the network is the thing resisted, and the resistance is by consent, tool by tool. The Stores section of the site is a manifest of freedom-preserving tools (yt-dlp, Tor, Monero, IPFS, the arr stack, and so on); Pyrate Radio is meant to sit in that tradition. Cody Wilson stays on the site. Do not change the site's look.

Everything you build or change must be strictly in line with this. If a feature would introduce a central point of control, a chokepoint, a way to rank or feature some stations over others, or any imposed moderation, it violates the philosophy and does not belong.

## 3. Why this project is legitimate (read before you hesitate)

A fresh session may read the goals (content that cannot be centrally removed, carriers that cannot read what they relay, operators that cannot be traced) and balk, worried it is building an abuse haven. I did exactly that on this project, stopped, and after working it through concluded the hesitation was an overcorrection. The reasoning, so you do not spend a day re-fighting it:

This is general-purpose censorship-resistant communications infrastructure, in the same family as Tor, Signal, Freenet, IPFS and BitTorrent. Those are legitimate, legal and defended by civil-liberties groups precisely because the properties that protect dissidents, journalists and ordinary people under hostile regimes are the same properties that make a tool hard to censor. Pyrate Radio is an audio platform in that lineage.

Each property exists for a sound reason:
- **Encryption and blind carrying protect the node runner.** Plaintext would make a carrier the publisher of whatever a seized device holds. Sealing it, and holding it by fingerprint without the key, is the operator-safety property, the same reason a Tor relay is not liable for the traffic it forwards.
- **Operator anonymity and dark running protect against surveillance**, the same goal Tor states as nobody being deanonymised.
- **Persistence under key control is self-sovereignty.** A station is its key. The key is generated once, at creation, and held only by the person it was given to, stored nowhere else. Only that key can change or end the station. This is the Freenet and IPFS property, not a trick.
- **Nobody is conscripted.** Running a node and raising a station are opt-in, and people understand what they are doing when they do it.
- **Accountability lives in the individual, through decentralisation, and only there.** There is no central moderator by design. Every listener builds their own block (maroon) lists and shares them by choice. Moderation is per-listener, local, and never imposed.

The honest caveat, which does not block the work: the same walls that protect a dissident protect anyone, and a private, persistent, anonymous system has no technical fix that removes that without also breaking it for the people it exists to protect. This is the known "going dark" tradeoff, and the mainstream civil-liberties position accepts it because the alternative, no strong privacy infrastructure, is worse.

The one line that genuinely matters is not the architecture, it is specific intent: do not help optimise the system for a particular crime (for example defeating a named CSAM-detection system, or a feature whose only purpose is to distribute illegal material). Nothing in this project asks for that. It is anti-enclosure infrastructure for speech and culture. Build it well.

## 4. Where everything lives

- **Repo:** github.com/dukewilder/hostis. The whole hostis.black site, with the radio under `radio/`.
- **Remotes:** `origin` only. Branches on origin: `main` (what GitHub Pages serves) and `radio` (the code).
- **Worktrees in this environment** (a fresh container has neither; section 11 makes them):
  - `/home/claude/hostis` on branch `radio` (the code; `radio/` is the subproject root at `/home/claude/hostis/radio`).
  - `/home/claude/main-wt` on local branch `site-pyrateradio`, which is pushed to `origin main` to deploy. Its `pyrateradio/` folder is the live subsite.
- **Computers' builds:** CI builds them on every push to radio (radio-desktop.yml). For a new version, wait for that run to pass, then `sh tools/fetch_desktop.sh` before `tools/deploy_site.sh`, or the site publishes the last builds fetched under the new version number.
- **Deploy topology:** build the landing into `radio/landing/out`, copy it into `main-wt/pyrateradio/`, commit on `site-pyrateradio`, then `git push origin site-pyrateradio:main`. Push code with `git push origin radio` from `/home/claude/hostis`. `local` branch `main` is stale and diverged; ignore it, `origin/main` is the deploy source.
- **Attribution on commits:** follow the session's own reminder for the Co-Authored-By / Claude-Session lines.

### Source map (about 14k lines of Rust and Dart, plus Kotlin, JS, Python)

Rust workspace (`radio/Cargo.toml`, members core, node, ffi, spike, browser; edition 2024; `web` profile for wasm):

- `core/src/` shared logic, no I/O:
  - `keys.rs` station identity (ed25519), the 24-word recovery, deputies.
  - `seal.rs` blind-carry sealing (Freenet CHK style): name and key derived from the content hash, ChaCha20-Poly1305.
  - `chat.rs` per-station gossip: Say/Cheer/Request/Hail, proof-of-work over a digest, voices from a node secret.
  - `pocket.rs` pocket stations: topic, tag and sealing from a station key; the invitation format.
  - `programme.rs`, `clock.rs` the schedule engine (shuffle, in order, newest, daily, weekly, on-air hours, shows, premieres, notices) in the keeper's own time zone as a table of offsets.
  - `page.rs` the station page and programme JSON, shared by phones and browsers.
  - `audio.rs`, `archive.rs`, `tape.rs`, `art.rs`, `callsign.rs`, `ingest.rs`, `lib.rs`.
- `node/src/` the running node (phone, Lantern or browser share most of this):
  - `lib.rs` the heart: start-up, relays, discovery, dark running, daily id rotation, playout, serve, gossip, pockets, flock, commands.
  - `net.rs` the wire protocol (Request/Response, the Gossip enum, the listener-count sketch).
  - `store.rs` on-disk store, including sealed logs at rest.
  - `chat.rs`, `flock.rs`, `pocket.rs` node sides of those features.
  - `hop.rs` relayed fetch. `trust.rs` Lantern duties. `page.rs` keeper commands. `player.rs` playback. `web.rs` + `web/` the Lantern's web listener, podcast feeds, episode cache, and the `/player/` mirror. `art.rs`, `clockfix.rs`.
  - `bin/lantern.rs` the Lantern daemon. `bin/voyage_test.rs` (22-check end-to-end), `flock_test.rs`, `pocket_test.rs`, `onair_test.rs`, `art_test.rs`, `probe.rs`. `bin/release/` signing helpers.
- `ffi/src/lib.rs` the C FFI the app calls.
- `browser/src/lib.rs` the wasm node. Built by `browser/build.sh` into `browser/www/pkg`, pages assembled by `browser/build.py` into `browser/www/dist`.

App (Flutter, `app/`): `lib/main.dart` (shell, tuner, drawer Lantern), `lib/station.dart` (station page, programme and show editors, creation, posters, chart, chatter, flock, invite sheet, voice kit), `lib/more.dart` (the Log, tips), `lib/art.dart`, `lib/native.dart` (FFI bridge). Android Kotlin in `app/android/.../`: `MainActivity.kt`, `Native.kt`, `Ship.kt`, `PlaybackService.kt`, `LanternService.kt` (drawer Lantern), `Bells.kt`, `Voice.kt` (TTS idents). Version in `app/pubspec.yaml` only (the app reads its build code from the installed package), now 0.8.6+19.

Browser player JS/CSS: `browser/www/app.js` (the radio set, station page, chatter, hail, tips, flock, pockets, harbour memory), `browser/body.html`, `browser/player.css`, `browser/harbour.json` (HOSTIS's own stations for the landing set).

Docs: `DECISIONS.md` (the Ship's Articles, sections I to XXII), `SHIPS-LOG.md`, `LANTERN.md`, `RELEASES.md`. Tools: `tools/dukewilder-rebuild.md` (the two-identity server rebuild), `tools/torrent.py`, `tools/bell.py`, `tools/feed2folder.py`.

## 5. Everything built so far

- **Core radio.** Stations as keys, signed hash-linked logs, Opus in six-second chunks fetched by fingerprint from whoever carries them, a dial frequency drawn from the station id, listener counts from anonymous hourly tokens with proof-of-work and a HyperLogLog sketch nobody takes on trust.
- **Stations.** Raise a station in the app, programme it (shuffle, in order, newest, daily or weekly tape, on-air hours, regular shows, premieres, notices) in your own time zone, spoken idents via TTS, a numbers-station kit, posters with the frequency and a scan code, a podcast feed per station and an episode cache, a video clip for socials, a chart of stations by home port.
- **Social.** Per-station chatter (talk, cheers, requests the keeper can play next), hail (a voice note sealed to the keeper and deputies), tips with a note. All signed, checked by everyone, held in memory; Lanterns keep rooms and a backlog.
- **Pocket stations.** Private, invitation-only, never on the Log or dial. Key is the invitation; own sealed gossip topic; log requested by a tag, never by id; raised in the app, joined in app or browser.
- **Flock.** Several phones in one room play as one off a shared clock, within about a millisecond, with a trim for a slow speaker.
- **Hardening.** Blind carrying (sealed chunks and logs served by fingerprint, plaintext only for your own stations), sealed logs at rest, a fresh node id every 24h for phones with a steady seed behind it, dark running (`PYRATE_DARK` no direct paths and no address published, `PYRATE_PROXY` over Tor), the two-identity harbour (public face plus dark hold), and no single door (nodes remember peers, browser tabs remember every harbour).
- **Privacy fails closed.** Logs and chunks come through two hops on phones and in the browser, and nothing goes direct unless the owner turns on "ask carriers directly"; stems start two hops out; carrier notes are signed; secrets are sealed in the hold. AUDIT-2026-11.md lists the November audit and every fix; `privacy_test` checks the fail-closed path.
- **The second audit (0.9.10).** AUDIT-2026-11b.md: the broadcaster's stem never goes plain unless the owner allows direct connections; browser secrets fail closed; invitations go only to checked open-player copies; relay addresses must be public; Lantern feeds take `PYRATE_PUBLIC_URL`; a byte budget for requests; the clock eases. Pockets hide from outsiders, not from their own members, and the app says so.
- **HOSTIS stations as seed folders.** `stations/<slug>/` holds each station's art, bell, programme and notices; The Numbers holds its recordings, built by `tools/stations/the_numbers.py`. Netlabel and Poets' Corner are fetched on the harbour by `tools/stations/fetch.py` from manifests that `tools/stations/build_manifests.py` writes. `tools/stations/art.py` draws every cover and banner. `tools/dukewilder-0.9.10.md` is the harbour's brief. Once dukewilder reports the three new station ids, add them to `browser/harbour.json`, `app/assets/harbour.txt` and the landing's station list.
- **Plain words (0.9.9).** Four tabs: the Dial, the Log, My Station, Settings. Everything a listener or keeper touches is in plain words, with technical switches folded under Advanced or More; only the how-it-works page goes deep (DECISIONS XXXIX). "Ask carriers directly" is now "Connect directly when needed", in Settings, Advanced, Connections.
- **Ballast by shares.** Every node holds a share of every chunk in the Log, chosen by a public hash of its id and the chunk, aiming at eight copies across the fleet and shrinking as it grows; the budget follows the free space (phones up to 4 GB on Wi-Fi while charging, computers up to 20 GB). `share_test` checks it.
- **Lantern.** Headless node carrying the whole Log, web listener (`--http`), relay (`--relay`), the `/player/` browser mirror, `--carry-own` to carry only its own stations. The drawer-phone Lantern is the Android foreground-service version with a ballast size from 1 to 256 GB.
- **Browser.** A tab is a full node in wasm from the same core and wire protocol: listens, chatters, hails, joins flocks and pockets, remembers harbours, fresh key each visit.
- **Hardening plan.** `HARDENING-PLAN.md` holds every attack found in three rounds of threat modelling, its fix, who does it, and the nine build waves. Wave 1 (device leaks) shipped in 0.8.7; work goes wave by wave from there, and the plan is marked as each wave ships.
- **Distribution.** APK over BitTorrent with a web seed and magnet, phone-to-phone pass-on, in-app updates as a station verified by 2-of-3 HOSTIS release keys.

## 6. How to build and test

- **Rust checks and tests:** `cargo check`, `cargo test` from `radio/`. End-to-end harnesses: `cargo run -p pyrate-node --bin voyage_test` (22 checks), and `flock_test`, `pocket_test`, `onair_test`, `art_test`.
- **A private test network:** set `PYRATE_RELAYS` to a local relay and run several lanterns/nodes; browser tabs tested two at a time against a local build. Do not point the public build at test relays.
- **Lantern:** `cargo run -p pyrate-node --bin lantern -- --seed <folder> --http --relay [--carry-own] --quota-gb <N>`. Env knobs: `PYRATE_DARK`, `PYRATE_PROXY`, `PYRATE_RELAYS`, `PYRATE_PLAYER`, `PYRATE_HAILS` (keep hails sealed for the keeper; `--open-hails <folder>` opens them).
- **Browser (public):** `bash browser/build.sh` with no `PYRATE_HARBOUR`/`PYRATE_RELAYS` set (harbour then comes from `app/assets/bootstrap.txt`).
- **App:** `flutter` is at `/opt/flutter/bin/flutter`, not on PATH. `app/build_native.sh` builds the Android FFI and copies the `.so`, then calls bare `flutter` and fails; after it runs, do `cd app && /opt/flutter/bin/flutter build apk --release --target-platform android-arm64`. Analyze with `/opt/flutter/bin/flutter analyze`. APK is debug-signed in this workspace (consistent with the deployed ones); the real Android release key stays on Dylan's PC.
- **Landing:** `python3 landing/build.py <apk path> <version>` into `landing/out`, then deploy as in section 4.

**Tested so far:** sealed carrying and playback, sealed logs surviving a restart, chatter with backlog, a hail reaching a Lantern, a dark Lantern carrying and mirroring the player, pocket seen by a member and invisible to an outsider, a flock within about a millisecond, the voyage_test 22 checks, and the pocket test with a browser tab as a member. **Needs real devices:** a flock across several phones in a room, the drawer Lantern over days, iPhone lock-screen playback, and the two-identity dukewilder rebuild.

**Environment notes:** disk is tight, `target/debug` is regenerable and safe to clear, only delete your own build outputs and ask before anything else. Outbound HTTPS is allowlisted (npm, pip, github fine; arbitrary sites refused; do not probe the proxy). `gh` auth fails but `git push` works.
## 7. The audit mandate

Do a full, line-by-line audit of the whole project, in two tracks, and keep the product-first principle throughout: this must end up a genuinely excellent, feature-rich, unique radio app that also happens to be the most hardened one that has existed.

**Track A, the product.** Read every feature's code and confirm it works and is as good as it can be. Correctness (the clock and programme maths, the log linking and verification, chunk fetch and playout seams, the FFI boundary, the wasm paths). Completeness and polish (is the experience seamless, is anything half-wired, do the editors and sheets all function, is the design true to hostis.black). Feature-richness (what would make it a better radio app, in line with the philosophy). Propose and, where you can, build improvements.

**Track B, privacy, anonymity and decentralisation.** Audit every surface for leaks and holes that could compromise a listener, a node runner or a broadcaster from any angle. Go through: the network layer (address publishing, relay metadata, direct paths, DNS, the proxy path, dark mode), identity (daily rotation, the seed, token unlinkability, the chat voice), the sealed-at-rest store and the blind-carry paths (can a node ever be made to reveal or serve plaintext it should not), pocket stations (can an outsider ever detect or reach one), hail and flock, the browser (fresh key, no storage leaks, the harbour memory), and distribution. For each, state the threat, whether it holds, and the fix. Harden and make it more resilient wherever you can, strictly within the HOSTIS philosophy: no central control, no chokepoint, no ranking, no imposed moderation, accountability only in the individual and the decentralisation. If there is a specific thing you cannot do because it hits a boundary, that is one thing, but do not brush off an entire scope of work to avoid it. That is unacceptable. Build each thing individually until you hit the actual specific wall and then you can tell me. There should be no issues here. Mapping a boundary is not a way to get around it. It is so I can know the boundary and avoid it. Do everything that doesn't cross the boundary and when you get to the boundary you don't do it. 

Also weigh new features or improvements that would strengthen either track while staying in line with HOSTIS, and read the site first to be sure of that line.

**Do everything you can yourself first.** Build, test, verify, fix. Only what you genuinely cannot do in this environment goes on the hand-back list below.

## 8. What only Dylan can do (keep this list, add to it)

Known now:
- Sign each new APK with `Sign Pyrate Radio.bat` in the pyrate-keys folder on his PC (0.9.10 is signed and published). Paste `tools/dukewilder-0.9.10.md` into the Claude on dukewilder and send back its report: the five HOSTIS station ids go into `browser/harbour.json`, `app/assets/harbour.txt` and the landing.
- Generate the signing keys on his PC with `pyrate-keys.exe` and send `release_keys.txt` and the Android cert; the release and Android signing keys never enter this workspace.
- Test on real devices: a flock of several phones, the drawer Lantern over days (and after a reboot), iPhone and iPad lock-screen playback through a phone call, the playback service from the lock screen and the car (its threading is new in 0.8.4 and has only been built, never run), a bell, and SCUTTLE on a phone you can spare.
- Try the computers' builds on hostis.black/pyrateradio: the Windows zip on his PC (run pyrate.exe), and the Mac zip if a Mac is to hand (right-click, Open). They were built in CI and have only been run on Linux here.
- Make one more Proton WireGuard configuration (P2P server, NAT-PMP on) for the harbour's own tunnel, then paste `tools/dukewilder-wave5.md` into the Claude on dukewilder and send back its report. It supersedes `tools/dukewilder-rebuild.md`.
- Send DNS screenshots, and `station.jpg` / `banner.jpg` if still wanted.
- Approve any change to the hostis.black front page, including a link to the radio.
- Approved by Dylan on 10 November: one file at the root of hostis.black, `/.well-known/assetlinks.json`, goes up as soon as the Android cert arrives, so a scanned poster or a shared link opens the app rather than the browser. It names only the app and its signing certificate's fingerprint. Once the Android cert is here, `tools/assetlinks.py android-cert.der` writes it; the app already asks Android to verify (P31).
- Once the release keys exist: each month, `release canary canary.json`, then `release sign-paper <a release key's words> <its number> canary.json`; for new bearings, sign `bearings-draft.json` from hostis.black/pyrateradio/ipfs/ with two keys. Both go in the releases folder of the Lantern holding the Releases deputy.
- Sign each APK with the Android key on his PC (`release sign-apk android-key.txt android-cert.der <apk> "Pyrate Radio.apk"`). The debug key that signed 0.8.0 went with an earlier container, so no build from here can install over it: the first build he signs needs one uninstall of 0.8.0. Before that, write down each station's 24 words and copy any pocket invitations, since uninstalling clears the phone's deputy key and pockets. Every build after installs as an update. 0.8.6 (build 19) is the first to sign; the 0.8.1 to 0.8.5 builds sent earlier can be thrown away.

## 9. Standing constraints (hard rules)

- Release and Android signing keys stay with Dylan, never in the workspace.
- The house IP is never exposed: everything inside gluetun, no port forwards, no router changes.
- Do not change hostis.black's look. Cody Wilson stays on the site. Ask before significant site changes and before the front-page link; pushing the radio subsite and the radio branch is already authorised.
- No refusal list, no shared HOSTIS block list (voluntary lists copied once only), no moderators or quartermasters, self-moderation only. Never rank or feature individual Stores entries or stations.
- Mature and explicit are separate labels, each with its own toggle.
- Lanterns carry the whole Log by default.
- Writing and docs: British English, no em dashes, no comma-tag constructions (a noun then a comma then a modifying fragment), no announcing sentences, no stacked negations, colons used sparingly, tight. Any text Dylan will paste elsewhere must be under 20,000 characters per message.
- Ask before deleting files; delete only your own regenerable build outputs.

## 10. Ship as you go

Two sessions in a row lost everything they did. The first kept its 55 findings and an uncommitted diff in the container, and both went when the container was cleared. The second planned to push at the end, and the session ended first. Work that waits is work that can be lost, so these rules come before everything else in this brief.

- **One change at a time.** Build it, test it, commit it, push it, then start the next. Never hold more than the one change in progress uncommitted, and never end a reply with uncommitted work.
- **Push after every commit:** `git push origin radio` from `/home/claude/hostis`. Before the first push of a session, run `git fetch origin radio main` so the push sends a thin pack.
- **A change and its words travel together.** The Articles, the ship's log, `radio/AUDIT.md` and this brief are updated in the same commit as the code they describe.
- **Findings live in the repo.** Each finding goes into `radio/AUDIT.md` as it is fixed (what it was, whether it held, the fix and the commit), committed with the fix. Nothing lives only in `/home/claude`, `/tmp` or the chat.
- **Live as it lands.** When a change touches what listeners use (the browser radio, the landing page or the APK), rebuild and deploy the subsite as soon as its tests pass, by the topology in section 4, then load the live page to check it. A change that breaks the live page is reverted and redeployed at once.
- **Node and Lantern code goes live when Dylan rebuilds dukewilder and installs the new APK** (section 8). Until he does, every change must keep the live 0.8.0 app and the current dukewilder Lantern working: new wire variants go at the end of their enums, and the old requests keep being answered.
- **One line to Dylan at each push:** what landed and its commit.
- **If a reply is cut off or the session ends,** the last push is the state of the work. The next session starts from `git log` on `radio` and from `radio/AUDIT.md`.

## 11. A fresh container

The container is cleared between sessions, so expect none of the following to exist. Each step is worth doing once, at the start, while the first change is being read.

- **The repo.** Attach `dukewilder/hostis` with push access, then `git clone --depth 1 --no-single-branch https://github.com/dukewilder/hostis /home/claude/hostis` (allow ten minutes), `git checkout -b radio origin/radio`, and `git worktree add /home/claude/main-wt -B site-pyrateradio origin/main` for deploys. Base it on `origin/main`: the `site-pyrateradio` branch on origin is stale.
- **System packages:** `apt-get install -y libasound2-dev pkg-config cmake clang` (the workspace build needs ALSA headers; the wasm build needs clang).
- **Rust targets and tools:** `rustup target add wasm32-unknown-unknown aarch64-linux-android` and `cargo install wasm-bindgen-cli --version 0.2.130 --locked` (the version must match `Cargo.lock`). For `wasm-opt`, `npm i -g binaryen`: Ubuntu's binaryen is too old, and its output breaks the externref table so the tab says it cannot run the radio.
- **Flutter:** the current stable tarball from `storage.googleapis.com/flutter_infra_release/releases/` (its path is in `releases_linux.json`), unpacked to `/opt/flutter`. Run `git config --global --add safe.directory /opt/flutter` and `flutter config --no-analytics`.
- **Android:** NDK r28c (`dl.google.com/android/repository/android-ndk-r28c-linux.zip`) unpacked to `/opt/android-ndk-r28c`, the command-line tools to `/opt/android-sdk/cmdline-tools/latest`, then `sdkmanager "platform-tools" "platforms;android-36" "build-tools;36.0.0"` and `flutter config --android-sdk /opt/android-sdk`. Set `ANDROID_HOME`, `ANDROID_SDK_ROOT` and `ANDROID_NDK_HOME` before `app/build_native.sh`.
- **Disk.** With all of the above and one debug build there is about 11 GB left. `target/debug` is the first thing to clear.
- **The private test network.** Start a relay first: `PYRATE_RELAYS=http://127.0.0.1:3340 lantern --dir /tmp/relaynode --relay 127.0.0.1:3340 --carry-own`, and set the same `PYRATE_RELAYS` for every harness. Nodes in this container cannot reach number0's address lookup, so a node can dial only the peers whose relay it already knows. A harness that cannot find its peers is a fault in the test network, not in the code. Give the relay a station of its own (`--seed "Gangway|Test|/folder|music"`) and start a carry-all Lantern with `--bootstrap <relay id>`: on a network with no stations no node knows another, so a stem has nowhere to go and a browser's station waits for ever.
- **The browser radio** can be tested headless: Playwright's Chromium is in `/opt/pw-browsers`, and Python's `playwright` package is installed. Serve `browser/www/dist` on localhost and drive the page.

The audit restarted from the code on 10 November 2026. Its findings, open and fixed, are in `radio/AUDIT.md`; start from there and from `git log` on `radio`.

- 11 November 2026: signed 0.9.11 (cert af4c08f6…) live at hostis.black/pyrateradio with the computers' builds from run 43087563507. https relay chosen: Cloudflare Tunnel run inside gluetun, brief in tools/dukewilder-relay-tunnel.md. Waiting on Dylan moving hostis.black DNS to Cloudflare.

- 11 November 2026: https://relay.hostis.black live through Cloudflare Tunnel (pyrate-tunnel in gluetun-radio, edge from the Proton exit). Harbour publishes it to pkarr and its relay is fixed at 42779. Browser player tested from hostis.black/pyrateradio/listen: The Numbers played with 13.7 s buffered, five stations found.

- **Attack harness (0.9.13).** Build with `cargo build --release -p pyrate-node --features playback --bin pentest`, then `PYRATE_NULL_AUDIO=1 PYRATE_LOG_MB=64 PYRATE_LAN_ANY=1 target/release/pentest <empty dir> <wav> <attacks>`. The target runs in a process of its own (`pentest --serve`), so memory and CPU figures are its alone. Attacks: `stems slow flood cards deputy fuzz netfuzz loop logflood cardwire logreply now routes nest web eclipse schedule`; run `schedule` last, as it kills a target that stops answering. `PYRATE_LOG_MB` sets a Lantern's budget for carried logs; `PYRATE_NULL_AUDIO=1` plays into nothing for machines with no speaker (also for `privacy_test`). Run the harness after any change to networking, stems, cards, gossip, logs, the clock or the web listener. For a baseline, build the commit before a fix in a worktree with its own `CARGO_TARGET_DIR`: a shared target directory left stale core artefacts. AUDIT-2026-11c.md and AUDIT-2026-11d.md have what each attack does. Nearby radios (LAN) need 0.9.12 or later on both ends.
- **Clock tests.** `cargo test --release -p pyrate-core --test clock_fuzz` (about four minutes) checks the costliest schedules a station can sign and that the quick clock agrees with one that walks every minute; run it after any change to `clock.rs` or `programme.rs`.
