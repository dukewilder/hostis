# Hardening plan

This compiles three rounds of threat modelling for Pyrate Radio. Each attack is listed with its fix and with who does the work. Building starts when Dylan gives the word.

**Who does it**
- **C**: I build, test and push alone.
- **C+dw**: I write one message for the Claude on dukewilder and Dylan pastes it.
- **D**: only Dylan can do it.
- **R**: Dylan rules first, then I build.

**Already true (checked in code)**
- The node key renews daily.
- Dark nodes publish no address.
- Art is decoded in memory-safe Rust and never handed to the phone's own image decoder.
- The app sets FLAG_SECURE.
- Android backup is off.
- Recordings are deleted after use.
- The player has a strict CSP and sends no referrer.
- A node's clock moves only when five stations agree.
- There are no analytics anywhere.

## 1. Tying a person to a station

**1.1 Upload timing.** The line's provider sees an upload at 21:03, and the entry appears on the network at 21:03.
*Fix:* stems release after a random delay of several minutes. A release queue lets a keeper set tracks to go out at random times, so the keeper is offline when they appear. `C` W3

**1.2 Upload size.** A 3 MB burst is distinctive even when encrypted. The same applies to listening, since chunk sizes give away the station.
*Fix:* pad chunks to a few fixed sizes and trickle uploads at a steady rate. `C` W3

**1.3 Live bursts every six seconds.**
*Fix:* jitter and padding. `C` W3

**1.4 One node spreading two stations links them.**
*Fix:* give each station its own network identity, fresh every session. `C` W3

**1.5 A lasting node id follows a device from home to work.**
*Fix:* listener ids are fresh at every launch. They already renew daily. `C` W3

**1.6 Sybil and eclipse attacks.** An adversary surrounds a broadcaster with its own nodes to see the first hop.
*Fix:* peer diversity, with at most two peers from one address block or one relay. Keep long-held peers, and draw the stem guard from them. `C` W3

**1.7 Voiceprints at scale.** A state runs carry-all Lanterns, transcribes every broadcast and matches voices.
*Fix:* voice disguise (pitch and formant shift) before encoding, in the app and in the browser. `C` W6

**1.8 Background sound.** Mains hum dates and places a recording against the grid's logs (ENF). Sirens and bells place it too.
*Fix:* a hum filter, on by default for microphone and live, and a warning before going live. `C` W6

**1.9 Writing style.**
*Fix:* advice in the keeper's handbook. `C` W8

**1.10 Self-declared places.** The home port and time zone fields.
*Fix:* a warning beside each field. The time zone becomes optional and coarse. `C` W1

**1.11 Waking hours.** The times of posts reveal a time zone.
*Fix:* the release queue (1.1), and timestamps coarsened wherever the programme does not need precision. `C` W3

**1.12 Watermarks in source audio.** Promo copies and AI music generators embed marks that survive re-encoding and can name an account.
*Fix:* a warning in the studio and the handbook. No filter removes a robust mark, and saying so is the fix. `C` W8

**1.13 Encoder fingerprint.** The app and the browser encode differently, which tells which tool a keeper used.
*Fix:* identical Opus settings everywhere. `C` W1

**1.14 File names become titles.** A name such as `C:\Users\dylan\...` would go out as a title.
*Fix:* the studio shows every title before raising and flags anything that looks like a path or a username. `C` W1

**1.15 Door operators see IPs.** Whoever serves the player sees the IP of everyone who loads it: a gateway, a Lantern or dukewilder.
*Fix:* more doors (onion, IPFS copies, the app). The handbook tells broadcasters to use the app or the onion. `C` `C+dw` W5

**1.16 GitHub logs every visitor to hostis.black.**
*Fix:* the onion mirror and IPFS copies. `C+dw` W5

**1.17 Cloudflare.** If relay.hostis.black still runs through the Cloudflare Tunnel, Cloudflare sees every client.
*Fix:* check it, then serve the relay only through the forwarded port and the onion. `C+dw` W5

**1.18 number0's relays.** The default relay map includes number0's relays, so they see client IPs. Nodes also publish their relay to number0's lookup server.
*Fix:* drop number0's relays from the map. Use the crew relays plus relays that Lanterns advertise in signed notes. Look peers up in the network's own book, and carry address records in gossip rather than on number0's server. Apply all of this to the browser too. `C` W3

**1.19 Proton can be compelled.** Proton can be ordered to log one account going forward; it did so for a Proton Mail user in 2021. Forwarded port 48799 at that exit belongs to one account.
*Fix:* put dukewilder's public face on a Tor onion service, which needs no account (`C+dw` W5). Pay for the VPN with cash or Monero, or move to a provider that accepts them (`D`).

**1.20 Clipboard sync, keyboard learning and history sync.**
*Fix:* mark copies as sensitive and clear the clipboard after 60 seconds. Use incognito typing on all 29 fields (5 today). Keep the station id out of the address bar. `C` W1

**1.21 "Now playing" leaks.** The lock screen, smartwatches and car stereos show what is playing, and car head units keep logs that forensic kits read.
*Fix:* a "Hide what's playing" option that sets the media session to "Radio" alone. `C` W1, default set by R5.

**1.22 Recents thumbnails and notification history.**
*Fix:* extend FLAG_SECURE to the recents screen. Notifications carry no station names while the option above is on. `C` W1

**1.23 Desktop indexing.** Spotlight, Windows Search, thumbnail caches and recent-file lists.
*Fix:* mark the data folder not to be indexed, and never register files in recent lists. `C` W1

**1.24 Keys in swap and hibernation files.**
*Fix:* lock key memory and wipe it after use. `C` W7

**1.25 Deputy keys in clear text.** The browser keeps them in localStorage.
*Fix:* seal them under a passphrase (WebCrypto), with a "forget when closed" option. `C` W1

**1.26 WebRTC leaking local addresses.**
*Fix:* a test proving the browser build opens no WebRTC, and a test proving the player calls nothing outside the network and its gateways. `C` W1

## 2. Seized devices and coerced people

**2.1 Unlocked phone taken.**
*Fix:* a duress PIN that opens an empty app and quietly scuttles. `C` W7

**2.2 Forced to recite the words.**
*Fix:* duress words that open a decoy station. `C` W7

**2.3 Border search.**
*Fix:* travel mode. Keys are wiped and the app shows nothing; the station is restored from the words later. `C` W7

**2.4 The app's presence.** Other apps can see it installed, and its icon gives it away.
*Fix:* a choice of icon and name, plus a renamed-package build. `C` W7

**2.5 Planted evidence.** An adversary pushes an illegal station into a target's carry, then seizes the phone.
*Fix:* the hold key already turns carry into noise. Add an option for phones to carry only stations they have played. Lanterns are unchanged. `C` W7

**2.6 The list of followed stations reveals interests.**
*Fix:* it is sealed under the hold key, and duress (2.1) covers it. `C` W7

**2.7 The desktop hold key sits in a file.**
*Fix:* move it to the OS keychain. `C` W7

**2.8 Words photographed or phished.**
*Fix:* an optional passphrase on top of the words. The app says plainly that nobody will ever ask for them. `C` W1 (text), W7 (passphrase)

**2.9 Dylan coerced into signing a bad update.**
*Fix:* two of three release keys must sign, kept in three places. Every update is announced on the network seven days before apps install it, and a public log records every release. `C` W4 builds it; `D` holds the keys.

**2.10 Dylan ordered to hand over data.**
*Fix:* leave nothing to hand over. Audit that no box he runs keeps logs (kubo, relay, Lantern), and state it in the canary. `C+dw` W5

**2.11 Gag order.**
*Fix:* a warrant canary, signed monthly by hand. Apps warn when it goes stale. `C` W4 builds it; `D` signs it.

**2.12 Station or deputy key seized.**
*Fix:* deputy keys expire and can be revoked. A pre-signed successor key lets a keeper move the station. `C` W4

**2.13 Developer liability (the Tornado Cash theory).**
*Fix:* no key that can stop or steer the network, no revenue, and nothing Dylan runs that the network needs. Anyone can run every role from the app. Publish a plain account of how the network works. `C` code; R1 for the page.

## 3. Taking it down

**3.1 The repo is taken down** (copyright claim or anti-circumvention, as with youtube-dl).
*Fix:*
- a git bundle on IPFS and as a torrent at every push (`C`);
- a Radicle seed on dukewilder (`C+dw`);
- a Codeberg mirror (`D` account, optional).

**3.2 The GitHub account is suspended and takes hostis.black with it.**
*Fix:* the landing page goes on IPFS under IPNS (`C`) and on the onion (`C+dw`).

**3.3 The domain is seized** by the registry or the registrar.
*Fix:* apps never need DNS, because the chart (3.5) carries node ids, the onion and IPNS. Lock the registrar and use hardware two-factor (`D`).

**3.4 Gateways drop the player, or Safe Browsing flags it.**
*Fix:* many gateways, the onion and the app. Publish nothing that scanners flag, such as zips with installers inside. `C`

**3.5 One bootstrap.** A new install knows only the hardcoded relays.
*Fix:* **the chart**, one small document signed by the release keys. It lists:
- relays, Lanterns, the onion and IPNS;
- the current CIDs;
- the latest versions and fingerprints.

Any carrier works because the signature is checked: gossip, IPFS, DNS TXT, Nostr (posted with a throwaway key, since the content is signed), torrents and paper QR codes. Invitations also carry the sender's peers, so a friend is a door. `C` W4; it goes live with the release keys.

**3.6 Rollback.** A replayed old IPNS record points at an old player with known flaws.
*Fix:* IPNS records signed offline, and a player that checks the chart for anything newer. `C` W4; `D` signs monthly.

**3.7 Google's developer verification.** Since 30 September 2026, certified phones in Brazil, Indonesia, Singapore and Thailand refuse apps from unregistered developers unless the user goes through the "advanced flow" (a 24-hour wait and a restart) or ADB. It goes global in 2027.
*Fix:*
- an install guide for the advanced flow;
- GrapheneOS guidance (unaffected);
- our own F-Droid repo;
- reproducible builds;
- renamed-package builds.

`C`; registering is R3.

**3.8 No app store.**
*Fix:* our own F-Droid repo, on IPFS and hostis.black (`C`; `D` makes the repo key). Listings on IzzyOnDroid and F-Droid main need an account (`D`, optional).

**3.9 Protocol blocking.** Deep packet inspection, or countries that block UDP.
*Fix:* a tested fallback to TCP on 443, ordinary-looking relay names, ECH where available, and Tor (W9). `C` W3

**3.10 Internet shutdowns.**
*Fix:* signed station packs passed by SD card, USB or Bluetooth; local Wi-Fi discovery; QR invitations. `C` W9. LoRa (Meshtastic) for short notices comes later.

**3.11 Flooding the relay and pinner on dukewilder.**
*Fix:* rate limits and connection caps. `C` W3 `C+dw`

**3.12 Filling Lanterns' disks.**
*Fix:* proof of work that rises with load, per-station quotas and a disk-cap warning. `C` W2

**3.13 Crash bugs.** Malformed wire messages or oversized frames.
*Fix:* fuzz every decoder and set hard size caps. `C` W2

**3.14 Hostile audio.** A station's Opus frames reach C code (libopus) on every listener's phone, so a decoder bug becomes remote code execution.
*Fix:* check frame lengths and headers before decoding, fuzz the decode path, pin the newest libopus, and decode in an isolated process on Android. `C` W2

**3.15 Look-alike stations.** "HОSTIS" with a Cyrillic O, or text-direction tricks.
*Fix:* strip direction controls, mark mixed-script names, show a station's fingerprint words, and keep the harbour listed by id. `C` W1

**3.16 Fake apps.**
*Fix:* the signing fingerprint appears on every page, the app shows its own, and the landing page explains how to check. `C` W1; needs the Android cert from `D`.

**3.17 A gateway injects code.**
*Fix:* the player checks its own fingerprint against the chart and warns when they differ. `C` W4

**3.18 Quantum: store now, decrypt later.**
*Fix:* hybrid post-quantum key exchange (X25519 with ML-KEM) on connections (`C` W9). Hybrid signatures come later.

**3.19 Clock spoofing.** Already holds.

## 4. Supply chain

**4.1 A poisoned crate, package or npm update.**
*Fix:* cargo-deny, `--locked` builds, checksums on the vendored Opus wasm, and pinned lockfiles. `C` W4

**4.2 A hijacked Actions tag.**
*Fix:* pin every action to a commit. `C` W4

**4.3 A backdoored build.** This workspace or GitHub's runners slip a backdoor into an APK that Dylan signs without seeing.
*Fix:* reproducible builds. dukewilder rebuilds and compares hashes before Dylan signs. `C` `C+dw` W4

**4.4 A stolen GitHub token pushes a bad player.**
*Fix:* signed commits (`D` key), and the player checks the chart (3.17).

**4.5 Dylan's own machines.**
*Fix:* a hardening pass for dukewilder: SSH keys only, automatic security updates, a check that nothing listens beyond what exists, and minimal logs (`C+dw` W5). On the Windows PC: BitLocker and a separate browser profile (`D`).

**4.6 Telemetry in dependencies.**
*Fix:* an audit, plus the test in 1.26. `C` W4

## Progress

- **W1 shipped in 0.8.7** (10 November). Found already in place while building it: the plain disguise (a plain icon named Radio, and a lock screen, notifications and car display that say only "Radio") covers 1.21 and part of 2.4, so R5 is settled by it, and it now blanks the recents screen too (1.22). Updates already need two of three release keys (part of 2.9). Encoder settings already match across app and browser (1.13). Clipboard history on Windows keeps its own copy for a time; the ninety-second clear removes it from the clipboard itself.
- **W2 shipped in 0.8.8.** 3.12 and 3.13 as planned (DECISIONS XXVIII). 3.14: chunks were already split with bounds checks, every packet's length checked against one 20 ms frame before decoding, and bad chunks concealed; libopus is 1.6.1 and its decode path is in the fuzz pass. Moving decoding into an isolated Android process waits for W7, with the other Android work. Counting stays at 20 bits per listener-hour: raising it would split old apps from new ones. Counts can no longer keep a junk station in a full Log, so cheap inflation buys nothing there.
- **W3 shipped in 0.9.0** (DECISIONS XXIX). Found while building it: relay.hostis.black has no DNS record, so 1.17 (Cloudflare) is moot and every node homes on number0's relays until W5 gives dukewilder's Lantern a public relay (`PYRATE_RELAY_URL`, and the door in bootstrap.txt as `id@relay`); the mechanism to prefer it is in place. A broadcasting phone already never claims to carry its own station (1.4), so the remaining link between two stations is the guard, which sees one device sending both. Steady-rate trickling (1.2) is left out: constant-bitrate chunks remove what sizes said, and a trickle would slow every upload. Signed entry times stay exact, since the programme counts on them (1.11): the stem's wait and the sparse cards cover the timing an observer sees on the wire. Tabs keep number0's relays, because a page served over https cannot reach a plain relay; that waits for a relay with a certificate. TCP on 443 already works: relays are reached over https, and a node with UDP blocked runs through them.
- **W4 shipped in 0.9.1** (DECISIONS XXX). Waiting on Dylan's release keys before they do anything live: the bearings and the canary (the `release` tool gains `canary` and `sign-paper`). The update delay is three days, not seven, with a tap to take it sooner. Left out: the player checking its own fingerprint (3.17), because code a gateway has altered can say whatever it likes about itself; the defence is content addressing (the service-worker gateway checks every block against its CID) and the app. Moved to later waves: deputy expiry and station succession (W7, with the other key work, since both change the log format); the F-Droid repo (it needs a repo key Dylan makes); IPNS records signed offline (W5, on dukewilder); a fully reproducible APK build on dukewilder (W5).
- **W5 written, waiting on dukewilder.** `tools/dukewilder-wave5.md` is the whole wave as one message for the Claude on dukewilder: the harbour on its own Proton tunnel with a public relay that follows the forwarded port, the dark hold over Tor, the open player as an onion, logs capped at a megabyte per container, BitTorrent seeding of the app, the builds and the source, and a security check of the box that asks before touching SSH. It needs one more Proton WireGuard configuration with port forwarding. Left out of it: Radicle (the source already ships each release as a tarball, a torrent and an IPFS name, which any host can carry), a reproducible APK build on dukewilder (its toolchain would take most of the FIREBAT's disk; `tools/apk_compare.py` is ready for when another machine builds one), and signing IPNS records offline (the bearings, signed by the release keys, name every player by its content, so a seized IPNS key can only point at a player the bearings do not list). Once the report is back: the harbour's `id@relay` goes into bootstrap.txt and the bearings, and the onion onto the site and into the bearings.

- **W6 shipped in 0.9.2** (DECISIONS XXXI). The hum filter (1.8) and voice disguise (1.7) are in core's `voice.rs` and matched in the browser's encode worker; the go-live warning (1.8, 1.9, 1.12) is on both on-air screens, and names the leaks no filter removes (background sound, watermarks in source audio, writing style). The disguise is a fixed pitch-and-formant shift, off by default; a sweep of random shifts per broadcast was left out as it would make a keeper sound unlike themselves between sessions, which a listener relies on to know the station is the same one.
- **W7 shipped in 0.9.3** (DECISIONS XXXII). 2.1, 2.5, 2.7 (Windows and macOS), 2.8 and 1.24 (Linux, Android and macOS) as planned. Found already in place: the disguise of icon and name (2.4, the plain disguise), now joined by a build under another package name; the followed-stations list is sealed under the hold key (2.6), so the second PIN covers it. Settled by the passphrase rather than built apart: duress words (2.2), since the words alone open an empty station; travel mode (2.3), which is scuttle before the crossing and the words and passphrase after. Left out, with reasons: deputy expiry and station succession (2.12), since a seized deputy is already revoked from the words and both would change the log format for every node; pocket key rotation, since a new key means inviting every member again, which a keeper can already do by raising a new pocket; locking memory on Windows, which needs a system call the app does not otherwise use and protects 64 bytes; decoding in an isolated Android process (3.14), a rebuild of the playback path for a decoder already fuzzed and kept current.
- **W8 shipped** (DECISIONS XXXIII). The handbook is on the radio's own pages, with the record of how the network stands, so R1 needs only Dylan's word on whether hostis.black's front page links to it. The install guide for Google's advanced flow was already on the radio's page. The canary is shown in the app (W4); a canary page on hostis.black is R2.
- **W9 shipped in 0.9.4** (DECISIONS XXXIV). Tor by Orbot or a computer's Tor (not run against Orbot here: the container has no Tor), station packs and nearby radios (tools: `grid_test`, 8 checks), post-quantum key exchange for nodes. Left out, with reasons: Tor built into the app (Arti), which would add megabytes and a slow start for everyone to do what Orbot already does well; QR codes for invitations, since the share sheet and the poster's code already pass them; a disguised protocol (3.19), which needs pluggable transports that only Tor carries well, so Through Tor with a bridge set in Orbot is the answer; hybrid signatures, since ML-DSA would change every log and card on the network for a threat years away. Browser tabs keep X25519 alone: the wasm build has no aws-lc-rs.

## Build waves

Each wave has tests, the docs (DECISIONS, SHIPS-LOG, the brief) in the same commit, a push, a deploy and one line to Dylan.

| Wave | Contents | Size |
|---|---|---|
| W1 | Device leaks: 1.10, 1.13, 1.14, 1.20 to 1.23, 1.25, 1.26, the words text, 3.15, 3.16. App 0.8.7. | small |
| W2 | Robustness: 3.12 to 3.14, harder counting. | medium |
| W3 | Network identity and traffic shape: 1.1 to 1.6, 1.11, 1.18, 3.9, 3.11. App 0.9.0, still compatible with older Lanterns. | large |
| W4 | Releases and supply chain: the chart, the update delay, two-of-three keys, canary, rollback checks, fingerprint checker, reproducible builds, pinned Actions, cargo-deny, source bundles, F-Droid repo, APK and desktop on IPFS, deputy expiry, station succession. | large |
| W5 | dukewilder, one paste: onion for the player, Lantern and relay; Cloudflare check; log audit; Radicle seed; torrent seeder; hardening; the rebuild-and-compare job. | small for Dylan |
| W6 | Sound: hum filter, voice disguise, go-live warning. | medium |
| W7 | Seizure and coercion: duress PIN, duress words, travel mode, disguise, keychain, locked memory, passphrase, carry-only-played, pocket key rotation. | large |
| W8 | Docs: keeper's handbook, advanced-flow install guide, the network statement (R1), the canary page (R2). | small |
| W9 | Tor and shutdowns: embedded Tor (Arti) for relay connections on app and desktop, Orbot as fallback, station packs, local Wi-Fi, QR invitations, post-quantum key exchange. | large |

W4 runs on test keys until the release keys exist, then goes live.

## Only Dylan

**Tonight**
1. Make the release keys. Create three keys with `release.exe init` and keep their words in three places: the house, a second building and a deposit box or family. Run `android-init` once. Send me the public halves and android-cert.der.
2. Sign APK 0.8.6. For every later APK, compare its hash with dukewilder's rebuild first.
3. Rotate the WireGuard key, and delete the copy in `~/.claude/uploads` on dukewilder.
4. Turn on hardware two-factor for GitHub, Proton and the registrar, and lock hostis.black at the registrar.

**Soon**
5. Paste the W5 message into dukewilder's Claude.
6. Pay the VPN anonymously, or switch providers (1.19).
7. Once a month, sign the canary and the IPNS record on your PC. I will make it one command.
8. Try the Windows build.
9. Make the F-Droid repo key (one command).
10. Optional: a Codeberg mirror, an IzzyOnDroid listing, free pinning accounts. Each adds copies, and each is a company that can be asked to drop them.
11. Your own devices: BitLocker, a separate browser profile for HOSTIS work, and an always-on VPN on your phone.

## Rulings needed

- **R1** A page on hostis.black explaining how the network works, so the record exists before anyone needs it. This changes the site.
- **R2** A canary page, and an onion link on the front page.
- **R3** Register with Google's verification? My advice is no. It ties the app to your ID. Guide users through the advanced flow instead, and point them to GrapheneOS and the F-Droid repo.
- **R4** Tombstones. A keeper could sink their own station, honoured by nodes that choose to honour it. Should Lanterns honour them by default, given that they carry the whole Log?
- **R5** Should "Hide what's playing" be on or off by default?

## What no fix covers

- An adversary who watches both ends of every connection can still correlate timing. The fixes raise the cost, and Tor raises it most.
- A compromised phone or computer gives up everything on it.
- Robust watermarks in source audio.
- Anything a keeper says or sings in their own voice and words, beyond the disguise.
- Dylan in custody can be made to act. Split keys and the seven-day delay mean he cannot act quickly or alone.
