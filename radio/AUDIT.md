# Pyrate Radio · The audit

Started 10 November 2026 from the code on `radio` at `736a0d1`. Two tracks: the product (does every feature work, and work well) and the walls (what a listener, a node runner or a broadcaster could be made to give away). Every finding lands here when it is found, and moves to Fixed in the same commit as its fix.

Each entry gives the threat, whether the wall held, and the fix.

## Open

### Track B: privacy, anonymity and decentralisation

### Track A: the clock, the programme, sound and pictures

Reported by a reader of the code; each is checked against the code before it is fixed or set down.

- **A4** (medium) `walk` gives up after 400,000 steps, so short tracks mean dead air late in the week. `core/src/clock.rs:22`
- **A5** (medium) Fall-back hour: Daily and Weekly tapes rewind, slots drop out. `core/src/clock.rs:440-451`
- **A6** (medium) `upcoming()` (watch bill and the page's "next") ignores every cut. `core/src/clock.rs:275-291`
- **A8** (medium) A show premiering before its track was added never airs, and the listing still promises it. `core/src/clock.rs:313-315`
- **A9** (medium) Call signs carry ~35 bits, so a lookalike can be ground out for a target. `core/src/callsign.rs:9-23`
- **A11** (low) `ChunkDecoder::decode` trusts the chunk's packet count and sizes. `core/src/audio.rs:166-174; archive.rs `Chunk::check``
- **A12** (low) Ogg writer can overflow the lacing byte and drops bad chunks silently. `core/src/tape.rs:26-45`
- **A14** (low) `span_from` truncates `days` and accepts a span that never runs. `core/src/page.rs:56`
- **A15** (low) Hours plus a continuous mode silently skips what plays off the air. `core/src/clock.rs:422-453`
- **A17** (low) `art::accent` depends on HashMap iteration order for ties. `core/src/art.rs:81`
- **A18** (low) `art::prepare` ignores EXIF orientation. `core/src/art.rs:49-54`
- **A19** (low) Ingest folds multichannel files badly. `core/src/ingest.rs:33-43`
- **A20** (low) Hard seams at the week boundary (and at every anchor). `core/src/clock.rs:425-453`
- **A21** (low) Offset table ends: time zone freezes after the last entry. `core/src/programme.rs:32-42; page.rs:88-95`
- **A23** (low) Output and input streams ignore the device's sample format. `node/src/player.rs:41-43`
- **A24** (low) `page_of` days overflow and repeated full-log hashing. `core/src/page.rs:39-47`

### Track A and B: the browser, the Lantern's web listener and the landing

Reported by a reader of the code; each is checked against the code before it is fixed or set down.

- **C12** (low) Lantern page and tower have no recovery when the stream ends. `node/src/web.rs:442-449`
- **C13** (low) /player/ mirror: broken home links, no redirect, no hardening headers. `node/src/web.rs:62-63`
- **C16** (medium) Chunk fetch is serial, has no failure memory, and retries dead peers every time. `browser/src/lib.rs:296-322`
- **C20** (medium) Unbounded growth: cards, listings, archives, rooms, connections, seen set. `browser/src/lib.rs:472-502`
- **C21** (medium) plan() re-hashes the whole log on every call. `browser/src/lib.rs:556-563`
- **C22** (low) Struck station leaves the tab stuck "Searching the Log" and hammering carriers. `browser/src/lib.rs:378-381`
- **C23** (low) Flock: no way to leave, relay URL taken from the link without checks. `browser/src/lib.rs:944-982`
- **C24** (low) Chat: the keeper label can be imitated, and bidi controls are not stripped. `browser/www/app.js:453; core/src/chat.rs:166-168`
- **C25** (low) Wasm traps are unrecoverable. `Cargo.toml `[profile.web] panic = "abort"`; browser/www/app.js:608-611`
- **C26** (low) Console logs identify the fresh node and the station being synced. `browser/src/lib.rs:49-51`
- **C28** (medium) No CSP, no Referrer-Policy, inline script in head. `node/src/web/listen.html:1-9`
- **C29** (medium) Storage and URL inventory (what ties a visit to the next, and what stays in history). `browser/www/app.js:22-35`
- **C31** (medium) Playback loop can die silently (exception, empty decode, missing decoder script). `browser/www/app.js:205-256`
- **C32** (medium) Going live after a track leaves silence and extra latency. `browser/www/app.js:206-207`
- **C33** (low) The "No carrier answered yet" message is overwritten within a second. `browser/www/app.js:232`
- **C34** (medium) Background and Safari playback: iPad detection, interruptions, meter, hidden tab buffer. `browser/www/app.js:100-111`
- **C35** (medium) Hail: wrong target after switching station, stale stop timer, mic left on. `browser/www/app.js:472-486`
- **C36** (medium) Chatter and rows are re-rendered wholesale on a timer. `browser/www/app.js:369-377`
- **C37** (medium) Cover art is fetched for every listed station, retried every 4 s, with no size cap. `browser/www/app.js:43-49`
- **C38** (low) Object URLs and animation work never released. `browser/www/app.js:46`
- **C39** (low) Second tap on Listen while booting starts a second node (embed). `browser/www/app.js:407-412`
- **C40** (medium) No relay or no stations is a dead end with no message. `browser/www/app.js:597-607`
- **C41** (low) ICS export can be injected with a bare CR in a title or about text. `browser/www/app.js:285-291`
- **C42** (low) Pass-it-on, clip QR and ICS links are hard-wired to hostis.black. `browser/www/app.js:494`
- **C43** (low) Station chart, harbour merge and debug hook small defects. `browser/www/app.js:26-31`
- **C44** (medium) Build: wasm is not cache-busted, so a deploy can mix new glue with old wasm. `browser/build.py:18`
- **C45** (low) Build embeds env/config strings unescaped. `browser/build.py:13-17`
- **C46** (medium) Android "open in the app" fallback goes to a 404. `landing/index.html:84`
- **C47** (low) Landing build can publish placeholder text. `landing/build.py:21-22`
- **C48** (low) Embedded set cannot record a hail and the copy there overpromises. `landing/index.html:79; browser/www/app.js:473-475`
- **C49** (low) Red used as a general accent, pill buttons, and emoji-capable glyphs. `browser/player.css:4`
- **C50** (low) Informational text set in the ghost tone and at sub-10px sizes. `browser/player.css:30`

### Track A and B: the phone app, its Android shell and the FFI

Reported by a reader of the code; each is checked against the code before it is fixed or set down.

- **P31** (low) Android App Links are not verified, so scanned posters open the browser. `AndroidManifest.xml:43-54`
- **P39** (low) `setState` after await without `mounted`, and controllers that are never disposed. `more.dart:394-406`
## Fixed

**B1. The stem names its origin.** `hop.rs` starts every stem at `hops: 0` and each relay adds one, so the guard that receives `hops: 0` knows its peer is the broadcaster. Does not hold. Fixed: every node, broadcaster and relay alike, now sends the count as zero and nothing reads it. A stem ends by the fluff draw alone (four hops on average), and a node that sees the same cargo come round again spreads it. Older nodes still count up from zero, which tells a new node only that its sender was a relay. Checked by the voyage test (18 of 18). (ae5aa7b)

**B2. Pocket stations surface on the public Log.** Three ways: a listener's hourly token for a pocket station is gossiped on the public Log by whoever counts it (phones through their exit, tabs directly), naming the station; a pocket station that goes ON AIR sends its live entries down the public stem, where a stranger takes them aboard, announces itself as a carrier and serves the log by id; and a keeper's phone syncing its own pocket asks an exit for it by id. Does not hold. Fixed: a pocket's hourly tokens now travel sealed on its own topic and are counted among its members; a pocket station never goes down the public stem; a keeper's phone asks its members for its own pocket by tag; and a member fetches a pocket's sound from the circle rather than through an exit, which would have learnt the id. The browser's half is C18. The pocket test now runs phones as they ship and checks that an outsider never hears of the station while a member listens and the keeper goes live; on the old code that check fails. (b85e7bd)

**C2. The web listener publishes pocket (private) stations.** (high) Fixed: the gangway refuses pocket stations in every route and leaves them out of log.json. (b85e7bd)

**B3. Pocket chatter is open.** A pocket's room sits on a topic drawn from the station's id and its lines travel in the clear, so anyone who learns the id can read the room. Does not hold. Fixed: a pocket's room now sits on a topic drawn from its key, met through its hosts and members, and every line (hails included) is sealed with the key in the app, on a Lantern and in the browser. The last lines go to members who ask by the pocket's tag, sealed, through a new request at the end of the enum; a request by the station's id gets nothing. Older apps in a pocket keep their own room until they update. Checked by the pocket test: the keeper hears a member and a browser tab, and an outsider joining by the id hears nothing. (ce61d80)

**C18. Pocket station id leaks to the public network.** (high) Fixed: a tab's hourly token for a pocket goes sealed on the pocket's own topic and never on the public Log; the pocket's room is sealed as in B3; and Clip it stays hidden for a pocket. Checked with the browser in the pocket test: an outsider never hears of the station while the tab listens and talks. (ce61d80)

**C30. Invitation key sits in the address bar for the whole join and forever on failure.** (medium) Fixed: the page reads an invitation and clears the address bar before anything else runs, joins from memory, and never writes a pocket's id into the address bar or the history. A flock link leaves the address bar as soon as it is read. (ce61d80)

**B4. Ballast is announced and served away from the charger.** A phone keeps announcing its ballast stations and serving sealed chunks to strangers on mobile data, which spends the owner's data, and its ballast set (fixed by its steady seed) ties each day's fresh id to the last and each network it uses to the next. Holds only in part. Fixed: a phone announces its ballast only while it is on Wi-Fi and charging, and on mobile data it serves strangers no sealed chunks at all. What remains: a phone that charges on Wi-Fi in more than one place can still be tied across those places by its ballast set, and a keeper who cares can set the ballast aside. (a5f12b1)

**B6. Stems and ballast ignore the listener's own switches.** A stem can end on a phone for a station its owner marooned, and ballast and stems take aboard explicit stations on a phone whose owner keeps them hidden. Does not hold for the owner's choice. Fixed: a node declines the stem and the ballast for any station its owner marooned, and a phone declines explicit stations while its owner hides them, and mature ones while they hide those. The station's word is read from its card, its log or the stem's own cargo. A declined stem is handed on, as a Lantern that carries its own already does. Checked by a new voyage test step (20 of 20). (a5f12b1)

**B10. A malformed id can stop a node.** `StationId::from_hex`, `unhex` and the browser's `unhex32` slice strings by byte, so 64 bytes holding a multi-byte character panic. The FFI and the browser are reached by pasted links. Fixed: every hex parser (ids, keys, hashes, flock codes, release signatures) now goes through one strict parser in the core that reads bytes, takes only hex digits and cannot panic. A test feeds it 64 bytes holding a two-byte character and a leading plus. (7bd378e)

**P6. One non-ASCII "hex" string aborts the app: no catch_unwind and byte-slicing parsers.** (medium) Fixed: the same strict parser closes the panics, and every door of the FFI (the C calls and the JNI calls) now catches a panic and answers with an error instead of aborting the app. (7bd378e)

**P1. Second `initNative` call aborts the process.** (high) Fixed: the Android context is set once a process on the Rust side, whoever asks first, so the playback service starting after the app (or before it) no longer trips ndk-context's assertion. Confirmed in ndk-context 0.1.1, which asserts on a second call; checked by building the FFI for Android. (7bd378e)

**A13. `unhex` accepts a leading `+`.** (low) Fixed: the strict parser takes no sign. (7bd378e)

**B7. Old plain chunks answer to a sealed request.** A chunk kept from before sealing lies on disk under its plain hash, and a `SealedChunk` request naming that hash is served the plain bytes. Does not hold for those files. Fixed: a sealed request that finds a plain file under its name seals it in place, removes the plain copy and answers nothing. A unit test plants an old plain chunk and asks for it by its hash. (0b3d89c)

**B11. A struck station's art stays on disk.** Strike drops a station's recordings but not its pictures. Fixed: a strike drops the station's pictures with its recordings. (0b3d89c)

**B12. A Lantern's chatter memory never empties.** Each room keeps every line id it has seen for the life of the process. Fixed: each room, on a node and in a tab, forgets a line's id once a line that old would be refused anyway (twenty minutes), so memory holds only the last few minutes. Unit tested. (0b3d89c)

**B15. Small faults.** The flock's round trip can underflow if the network clock steps between pings. Fixed: the round trip is taken with a saturating subtraction. (0b3d89c)

**C1. /player/ serves any file on disk via an absolute path.** (high) Fixed: the mirror builds the file's path from plain segments only (no empty, dotted, backslashed or colon segments) and serves it only if, links followed, it still lies inside the player folder. It also sends nosniff, no-referrer and frame-ancestors 'self', and knows the svg, ico, woff2 and txt types. A unit test tries absolute, doubled-slash, dotted and linked paths, and a live Lantern answers 404 to /player//etc/passwd. (ac1a5e3)

**A1. One hostile station can move every listener's clock by up to five minutes.** (high) Fixed: the network clock now lives in the core (drift.rs), shared by phones, Lanterns and tabs: each station gets one vote (its latest gap), nothing moves until five stations agree, the median is applied only past 2.5 seconds and eases off below one, and anything past five minutes is not trusted. Unit tests show a hundred cards from one station moving nothing. (417040b)

**C14. One station can set every tab's clock wrong.** (high) Fixed: the tab uses the same core drift as the app (see A1). (417040b)

**A10. Clock correction jumps from 0 to 2 s instead of easing in.** (low) Fixed: the correction turns on past 2.5 seconds and off below one (see A1). (417040b)

**A2. `t - cursor` underflow in `walk` when a run's computed start is after `t` (panic in debug, garbage in release).** (high) Fixed: every walk now refuses a start after the moment asked about, a tape whose start the clocks have only just reached begins a day or a week earlier, and a programme read from a log is made sane (offsets clamped to fourteen hours and sorted, minutes taken within the day, lengths within the week, days within seven). Regression tests: Berlin's spring change and a weekly tape at minute 65,535. (bbfe09d)

**A22. Spring-forward gap and `utc_ms` are inconsistent between east and west zones.** (low) Fixed: a local minute that was skipped begins when the clocks changed, and one that came twice begins the first time, by the same rule east and west of UTC. Tested at Berlin's spring change. (bbfe09d)

**A3. Rota walk has no step cap: a track of a few milliseconds hangs the lookup.** (high) Fixed: a recording under a second is not played (an Article ruling), the rota's walk has the same step cap as the programme's, and the watch bill has a budget of steps across all its lookups. A regression test signs a twenty-millisecond track and finds the clock quick and silent. (bbfe09d)

**A7. `at_ms + show_len` overflows for a signed Show with a huge `at_ms`.** (medium) Fixed: show ends are added with saturation. Tested with a show dated at u64::MAX. (bbfe09d)

**A16. Spans and tables read from the log are not validated, so `now` and `listings` can disagree and costs are unbounded.** (low) Fixed: the clock builds one index of tracks by hash and normalises the programme once, in Clock::new (see A2). (bbfe09d)

**C3. count_guest spawns one OS thread per request, with no cap.** (high) Fixed: one worker counts every web guest from a queue of 64, and a guest past a full queue goes uncounted; a tab is counted once it has listened thirty seconds, then hourly, so reconnects are not a crowd. (261c896)

**C4. Episode generation holds the whole recording in memory.** (high) Fixed: an episode is made a chunk at a time straight to disk, so memory stays flat however long the show, and recordings over six hours are refused. On the way a fault surfaced in the LAME wrapper: it writes into a vector's spare room and gives LAME no size when there is none, so the worst case is now reserved before every call. (261c896)

**C5. Concurrent first requests for one episode race on the same temp file.** (medium) Fixed: two requests for one episode wait on one maker, and each maker writes its own temporary file; at most two episodes are made at once. (261c896)

**C6. Cached episodes ignore withdrawal and ignore which station owns the track.** (medium) Fixed: an episode is served only if the recording is in the named station's library now, and the cache is named by station and recording. (261c896)

**C7. Streaming tower busy-loops on bad entries.** (medium) Fixed: a tower sleeps after any step that sends nothing and comes down after a minute of them; an off-air wait does not count. (261c896)

**C8. No connection, rate or time limits on the HTTP listener; `--web-all` lets anyone spawn encoders.** (medium) Fixed: the gangway streams to at most 512 listeners and 32 stations at once, and refuses a station it does not hold before starting a tower. What remains: no header-read timeout of its own; the tunnel in front of it is the first line for slow clients. (261c896)

**C9. Feed XML: unescaped Host-derived attributes and control characters.** (medium) Fixed: feed text drops the control characters XML forbids, and the base address takes only a plain host name and http or https, escaped. (261c896)

**C10. Episode Range handling reads the whole file per request and mis-answers unsatisfiable ranges.** (low) Fixed: episodes are read from disk as they go out, with suffix ranges and 416 for a range past the end. (261c896)

**C11. Per-request cost amplification on unauthenticated routes.** (medium) Fixed: a picture is looked for only among the stations the page may show. What remains: /now and /log.json still work out their answers on every request, cheap for a Lantern's own few stations. (261c896)

**C15. Unsigned Carrier gossip: fake carriers jump the queue, grow memory and eclipse peers.** (high) Fixed: a carrier's claimed head never counts past the station's own card, a listing keeps the 64 freshest carriers, a carrier or token for a station with no card here is dropped (pockets excepted), and a node that failed to answer in the last ten minutes is asked last. The same in the app, on Lanterns and in tabs; cards now go out before carrier claims so nothing honest is dropped. Unit tested; voyage (20 of 20) and pocket (8 of 8) pass. (51ff651)

**B13. One company's DNS sits under the network.** Nodes publish their id and relay to number0's pkarr server and look carriers up there, and every lookup also goes out as a plain DNS query that the line's provider can read. If that server goes, nodes can find each other only through gossip neighbours; while it stands, it and the line see who dials whom. Holds only in part. Fixed: node ids are no longer looked up in plain DNS at all (the endpoint is built from the minimal preset, the network's own book and number0's pkarr resolver over HTTPS). Every carrier now gossips a harbour note: its relay, signed with its own node key, which other nodes and tabs put in their address book for carriers and harbours they know, so nodes find each other inside the network without number0. Dark nodes send none. Tabs publish their address nowhere (C27). What remains: non-dark phones and Lanterns still publish their relay to number0's pkarr server, so that 0.8.0 nodes can find them; a later release can stop once the fleet carries notes. Unit tested; voyage (20 of 20) and pocket (8 of 8) pass. (ee94550)

**C27. presets::N0 may contact number0's discovery services and default relays.** (medium) Fixed: a tab publishes its address nowhere and looks peers up in its own book first (see B13). (ee94550)

**B5. Private fetches have no guard.** Each private fetch draws a fresh first hop, so over a day of listening a hostile pair of nodes (first hop and exit) will eventually both be drawn and can tie a listener to a station. Holds only in part. Fixed: a private fetch now goes through a guard kept as long as it answers (a file beside the stem's guard), passing over the well-known Lanterns, with a fresh exit each time; a guard that fails is dropped and another drawn. Voyage test 20 of 20. This container's test network has a single Lantern to route through, so every private fetch here takes one hop and the guard is not exercised; it needs a network of several carriers. (5a20daf)

**B14. A broadcaster asks carriers for its own station directly.** When the private route fails, a phone syncing its own station asks carriers for the log by id from its own address, which tells them who keeps it. Does not hold. Fixed: a phone asks carriers for its own station directly only once that station is already on the air directly from it. (5a20daf)

**P2. STRIKE COLOURS can never complete: the node drops the 24 words and signs with the deputy key.** (high) Fixed: strike now takes the master from the 24 words through the same path as restore, and still strikes with the key itself on a Lantern that keeps it. The app shows the result beside the act, and the words field learns nothing (no suggestions, autocorrect or keyboard learning) and is cleared after. Two new voyage steps: wrong words strike nothing, the right ones end the station (22 of 22). (ff7121e)

**P3. Auto Backup is left on: deputy keys, pocket secrets and prefs go to the user's Google account.** (high) Fixed: the app turns off backup and device transfer entirely (allowBackup false, no full-backup content, and extraction rules excluding every domain), so the deputy key, pocket invitations and the owner's chart never leave the phone that way. (bddbcf2)

**P4. The 24 words can be lost three ways.** (high) Fixed: the words are shown before anything that can fail, in a dialog that neither the back gesture nor a stray tap can close, and that closes only once the keeper ticks that all 24 are on paper; a later error is shown after. The node returns the words even if a pocket's topic fails to open (the station stays private, as it is marked so before the topic opens). (bddbcf2)

**P5. Exported MediaBrowser service hands the station list and control to any app, private pocket stations included.** (high) Fixed: the media browser answers only this app, the system, Bluetooth and the car, lists no pocket stations, and tunes only to a well-formed id. (bddbcf2)

**P13. Listening history is written to logcat.** (medium) Fixed: the released app writes nothing to the phone's log: the Dart prints are gone and the node's log line is empty on Android outside debug builds. (bddbcf2)

**P29. The 24 words and PINs are typed and shown with the keyboard's learning and no screen protection.** (medium) Fixed: the 24 words and PIN fields learn nothing (no suggestions, autocorrect or keyboard learning), are cleared once read, and the screen is marked secure (no screenshot, recents thumbnail or recording) while the words, strike and appoint dialogs are up. (bddbcf2)

**P37. `kBuildCode` duplicates the pubspec build number.** (low) Fixed: the app reads its version code from the installed package through the shell, so it can never disagree with the build; the constant is gone. (d6e1097)

**B8. The hold opens for anyone with the public Log.** Chunks and logs are sealed under keys drawn from public facts (a station's id, a chunk's hash), and filed under names drawn from the same. Someone who seizes a disk and also runs a node can test it for every station on the Log, and a deleted file on flash may still be read. Holds against a casual look only. Fixed: every file the node keeps is now sealed again and named under a hold key of the device's own (node/src/hold.rs): chunks (inside, still sealed under their own addresses, so a carrier still cannot read what it carries), logs, station keys, and the small files that name stations (the chart of kept and marooned, the settings, pocket invitations, proof-of-work and aboard counts). On a phone the hold key is wrapped by the Android Keystore, so a copy of storage taken off the chip is noise; a Lantern keeps it in hold.key, or wherever PYRATE_HOLD_KEY points. Files from before are carried over the first time each is read. Unit tests: someone holding a station's id, log and chunk address, and the old naming, finds nothing on the disk to test or open, another device's key opens nothing, and an old hold comes across and leaves no old file behind. Voyage (22 of 22), pocket (8 of 8) and art pass; the APK builds. What remains: destroying the hold key as a one-tap scuttle is not built yet. (359c9ae)

**P12. Station ids, bells and the alarm station sit in plaintext SharedPreferences and prefs.json.** (medium) Fixed: the node's chart, settings, pocket invitations and counts are sealed under the hold key (see B8). What remains: the Android side's own small store (the last station, bells and the wake station in SharedPreferences) is still in the clear. (359c9ae)

**C17. Aboard token broadcasts what the tab is tuned to, whether or not it plays.** (medium) Fixed: a tab is counted aboard only while it plays (the page says when listening starts and stops), and its hourly token goes to an exit through a first hop drawn once a visit, as a phone's does: the first hop sees the tab and not the station, the exit the station and not the tab. With a single node to hand it falls back to the harbour, which would hear it anyway. The seal for private requests now lives in the core, shared by phones and tabs. Checked in Chromium against two Lanterns: nothing is counted while browsing, and the token goes through two hops once listening; voyage 22 of 22. (99cbb2f)

**C19. Peers can probe a tab for what it has just played.** (medium) Fixed: a tab now answers nobody about what it holds: requests for logs, sealed chunks and plain chunks get nothing. It takes from the network and gives back its count and its chatter. (99cbb2f)

**B9. The PIN falls to a fast hash.** The Colours PIN is stored as a salted BLAKE3 hash, so a seized disk gives up a four-digit PIN at once, and with it any lock-screen PIN that matches. Holds only against a casual look. Fixed: the PIN is kept under Argon2id (32 MiB, three passes), so each guess against a copy of the file costs real time, and a PIN from before is moved to it the first time it is entered right. The app checks and sets it off the screen's thread, and asking only whether one is set costs nothing. Unit tested. (1245195)

**P15. After END BROADCAST the "ON AIR" notification, foreground state and both locks stay.** (medium) Fixed: ending a broadcast now leaves the foreground (and its locks) when nothing is tuned, or puts the station back on the notification when something is; ON AIR shows only while live. (650c233)

**P16. Every tune stacks a new audio-focus request.** (medium) Fixed: the service keeps one request for the speaker for its whole life and asks with it again, so no stale listener answers later focus changes. (650c233)

**P11. The "plain" disguise leaks the station name and the app name.** (medium) Fixed: in the plain disguise the playback notification is private with a public face that says only Radio, the app's station title no longer overrides it, ON AIR is not shown, and bells and the Lantern notification say only Radio. What remains: the system still names the app in the notification header, which no app can change. (650c233)

**P19. Links join pockets, flocks and stations without asking.** (medium) Fixed: A link from outside waits on the dial for a yes: LISTEN or JOIN, or NO. Nothing is played or joined before it. A bell's own tap carries a private key held in the app's preferences and still tunes at once; a forged tap from another app asks like any link. (e9bbfaf)

**P24. Joined chatter and joined pockets cannot be left; muted voices cannot be unmuted.** (medium) Fixed: The chatter has LEAVE THE CHATTER and a MUTED list where each voice can be heard again. A joined pocket has LEAVE THIS POCKET in the Log, which drops its key, log and sound from the phone. (e9bbfaf)

**P25. Pocket stations are offered a public share link, poster and QR.** (medium) Fixed: Pocket stations offer no PASS IT ON, no poster and no public link anywhere in the app; shareStation refuses a pocket as a backstop. (e9bbfaf)

**P30. Opening the app always autoplays.** (low) Fixed: The first open plays the busiest earned station once in the phone's life, kept in the sealed prefs; later opens stay silent until the owner tunes. (e9bbfaf)

**P7. Podcast feeds and "cover from a link" call the network straight from the phone, outside the node.** (medium) Fixed: The fetches stay on the phone, since the node would leave from the same address, but all of them now go through one guarded helper: a plain client name in place of Dart's own, timeouts on connecting and on every gap, a size cap, a client that is always closed. The feed screens say plainly that the feed's host sees this phone's address as it would any podcast app's. (0702d98)

**P8. Feed "keep the newest N" downloads the whole back catalogue, and the feed list can be overwritten.** (medium) Fixed: A feed's first reading takes its newest few and passes over the back catalogue for good; after that only episodes newer than the last one taken come aboard. Each feed's memory is written back alone, so a feed added or removed during a reading stays that way, and the memory is trimmed to what the feed still lists. (0702d98)

**P9. Podcast feeds are never refreshed on their own.** (medium) Fixed: The app reads its feeds every three hours, on Wi-Fi only, while it is open. (0702d98)

**P33. Feed and URL fetch code writes unsolicited temp files and the downloads can be stranded.** (low) Fixed: A fetch writes a .part file that takes its name only when whole and is deleted on any failure; the art and the episodes are deleted once set; and each start sweeps the cache of anything older than six hours that an earlier run left behind. (0702d98)

**P14. Temporary audio, pictures and APK copies are left on disk.** (medium) Fixed: Takes, picked files, pictures and fetches are dropped from the cache once set or discarded, through one helper that only ever deletes inside the app's own cache; each start sweeps anything an earlier run left there for more than six hours, the shared tapes and the APK copy included. Recordings are set one after another rather than all at once. (dd9d820)

**P21. RECORD A SEGMENT leaves the microphone open if the panel is closed.** (medium) Fixed: Closing the panel stops the microphone at once and drops the take; DISCARD deletes it; the hail recorder does the same when its sheet closes. (dd9d820)

**P22. The programme editor treats a failed load as loaded and can overwrite the real programme.** (medium) Fixed: Only a whole answer counts as read. A failed read keeps SET THE PROGRAMME shut, says what went wrong and offers TRY AGAIN, so a blank editor can never be saved over the programme on the air. (dd9d820)

**P18. The dial's PageView follows list position, not the station.** (medium) Fixed: The dial follows the station rather than its place in the list: when the Log reorders or shrinks, the face turns back to the tuned or shown station. Any turn the app makes itself goes through one path that tunes nothing, which also closes a hole in P19, where turning the dial to an invited station tuned it through the page listener before the owner said yes. (dea6713)

**P23. A hosted flock loses its code when the sheet is closed.** (medium) Fixed: The node's flock status carries the link for as long as the flock is hosted, and the sheet reads it back, so the code and its QR are there whenever the sheet is opened again. (36d90c2)

**P28. Direct fixes to misplaced results: CLEAR THE HOLD, strike and restore all write the same `error` field.** (low) Fixed: CLEAR THE HOLD and TAKE IT BACK each answer with their own word on the spot, success or failure, as the strike already does; neither writes the shared error line any more, and the hold is cleared off the UI thread. (36d90c2)

**P20. Microphone permission dead-ends and the messages mislead.** (medium) Fixed: Every microphone ask goes through one path that waits for the owner's answer rather than replying before the dialog shows. A refusal says to press again; a phone that will no longer ask says so and offers OPEN SETTINGS, straight to the app's own page; a microphone that would not start says another app may have it. (ca5bf40)

**P26. Bell ids differ between screens, depend on `String.hashCode`, and notifications are not checked.** (low) Fixed: A bell's id now comes from the station and the minute the show starts, by FNV-1a rather than the toolkit's hash, so the page and the watch bill agree and every build agrees. Setting or silencing a bell also clears any older bell for the same show, whatever id it was given, and a bell set while notifications are off for the app or its bells says so at once and offers OPEN SETTINGS. (a0d9777)

**P10. Drawer Lantern: the service restarts with no node, and the charging and Wi-Fi gate lives only in the Dart UI.** (medium) Fixed: The Lantern service starts the node itself when Android restarts it with no screen and reads its size back, and it keeps its own charger-and-Wi-Fi gate: power and network changes and a minute's check set it, and switching the Lantern off shuts it. A phone left as a Lantern comes back on after a reboot. (17e2416)

**P32. `pyrate_start` is not safe against two starters, and a failed start is invisible.** (low) Fixed: Starting is held under one lock, so the screen and a service starting at once make one node; the Kotlin side counts an already-running node as started; and a failed start is kept and shown on the dial in place of an endless search. (17e2416)

**B16. The Android side keeps listening notes in the clear.** The bells (each show's title, station and name), the wake-up station and the last station tuned sit in plain shared preferences beside the sealed hold, so a copy of the phone's storage says what its owner listens to and when. Does not hold. Fixed: the bells, the wake-up station and its name, and the last station tuned are sealed under the phone's hardware key, the same that wraps the hold key; a note written in the clear by an older build is read once and sealed when next written. Only a random bell key and on/off switches stay plain. (ab09d3d)

**P34. Brass (amber) is used throughout.** (medium) Fixed: Amber is gone: the update banner, SCANNING, the sleep timer, KEPT, the watch bill's days, ringing bells and the station-art note are all bone in its three weights. (bb90709)

**P35. Red is used beyond ON AIR, and there are two reds; M3 defaults bring rounded pickers and Roboto.** (low) Fixed: One red, #ed5555, kept to ON AIR, LIVE, the live dot and a microphone while it records (ruled in the Articles). Errors, STRIKE, MAROON, section heads, invitations and buttons are bone. The theme now sets Fell as the default face and squares dialogs, sheets, menus, toasts, the time picker, checkboxes and text buttons with hairline borders, with no tint. (bb90709)

**P36. The window behind Flutter is white in light mode.** (low) Fixed: The window behind the app, the splash and the system bars are the sheet's black whatever the phone's light or dark setting. (bb90709)

**P38. Manifest hygiene.** (low) Fixed: The unused Wi-Fi state permission is gone, and so is the toolkit's query for apps that process text, so selected words or a PIN are never offered to a translator or any other app; only text-to-speech engines are looked for. (f9984ff)

**P40. Passing the app on: a LAN HTTP server with no timeouts, binding every interface.** (low) Fixed: The server listens on the phone's own local address only, answers only under a random path carried in the QR (anything else gets an empty 404, so a scan of the room's Wi-Fi finds nothing naming the app), serves four at a time, drops a request not made within fifteen seconds, and shuts itself after fifteen minutes. (f9984ff)

**P17. Blocking native calls on the UI thread, in Dart and in the service.** (medium) Fixed: The dial's poll and every slow command (go on air, end, strike, tape, maroon, keep, flock join, clearing art, importing a maroon list) run off the screen's thread, the poll one at a time. In the playback service every call into the node, its start included, runs on one thread of its own; the lock screen, the speaker and the notification are touched only on the main thread, the service goes to the foreground before the node is asked anything, and the car's list of stations is built in the background. Built, not yet run on a phone: this build is in the APK for Dylan to sign and try. (79ce351)

**P27. The numbers kit decodes and rewrites WAV files sample by sample on the UI isolate.** (medium) Fixed: The message and its interval signal are read, joined and written in a background isolate from typed buffers in two writes, not sample by sample on the screen's thread. A round trip against Python's wave module (a stereo file in, its first channel out after the signal) matches sample for sample. (pending)
