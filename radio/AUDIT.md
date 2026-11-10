# Pyrate Radio · The audit

Started 10 November 2026 from the code on `radio` at `736a0d1`. Two tracks: the product (does every feature work, and work well) and the walls (what a listener, a node runner or a broadcaster could be made to give away). Every finding lands here when it is found, and moves to Fixed in the same commit as its fix.

Each entry gives the threat, whether the wall held, and the fix.

## Open

### Track B: privacy, anonymity and decentralisation

**B5. Private fetches have no guard.** Each private fetch draws a fresh first hop, so over a day of listening a hostile pair of nodes (first hop and exit) will eventually both be drawn and can tie a listener to a station. Holds only in part. Fix: a long-lived first hop for fetches, as the stem already has, and as Tor's entry guards do.

**B8. The hold opens for anyone with the public Log.** Chunks and logs are sealed under keys drawn from public facts (a station's id, a chunk's hash), and filed under names drawn from the same. Someone who seizes a disk and also runs a node can test it for every station on the Log, and a deleted file on flash may still be read. Holds against a casual look only. Fix: a hold key of the device's own, under which every file is sealed again and named, so the disk is noise without it and Clear the hold destroys the key.

**B9. The PIN falls to a fast hash.** The Colours PIN is stored as a salted BLAKE3 hash, so a seized disk gives up a four-digit PIN at once, and with it any lock-screen PIN that matches. Holds only against a casual look. Fix: a memory-hard hash (Argon2id).

**B13. One company's DNS sits under the network.** Nodes publish their id and relay to number0's pkarr server and look carriers up there, and every lookup also goes out as a plain DNS query that the line's provider can read. If that server goes, nodes can find each other only through gossip neighbours; while it stands, it and the line see who dials whom. Holds only in part. Fix: no plain DNS lookups of node ids, and each node's relay address carried inside the network, signed by the node, so nodes find each other without number0.

**B14. A broadcaster asks carriers for its own station directly.** When the private route fails, a phone syncing its own station asks carriers for the log by id from its own address, which tells them who keeps it. Does not hold. Fix: a keeper's phone never asks directly for its own station unless that station is already on the air directly from the phone.

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

- **C3** (high) count_guest spawns one OS thread per request, with no cap. `node/src/web.rs:325-328`
- **C4** (high) Episode generation holds the whole recording in memory. `node/src/web.rs:205-232`
- **C5** (medium) Concurrent first requests for one episode race on the same temp file. `node/src/web.rs:198-247`
- **C6** (medium) Cached episodes ignore withdrawal and ignore which station owns the track. `node/src/web.rs:198-203`
- **C7** (medium) Streaming tower busy-loops on bad entries. `node/src/web.rs:428`
- **C8** (medium) No connection, rate or time limits on the HTTP listener; `--web-all` lets anyone spawn encoders. `node/src/web.rs:52-70`
- **C9** (medium) Feed XML: unescaped Host-derived attributes and control characters. `node/src/web.rs:124-134`
- **C10** (low) Episode Range handling reads the whole file per request and mis-answers unsatisfiable ranges. `node/src/web.rs:248-265`
- **C11** (medium) Per-request cost amplification on unauthenticated routes. `node/src/web.rs:72-78`
- **C12** (low) Lantern page and tower have no recovery when the stream ends. `node/src/web.rs:442-449`
- **C13** (low) /player/ mirror: broken home links, no redirect, no hardening headers. `node/src/web.rs:62-63`
- **C15** (high) Unsigned Carrier gossip: fake carriers jump the queue, grow memory and eclipse peers. `browser/src/lib.rs:328-345`
- **C16** (medium) Chunk fetch is serial, has no failure memory, and retries dead peers every time. `browser/src/lib.rs:296-322`
- **C17** (medium) Aboard token broadcasts what the tab is tuned to, whether or not it plays. `browser/src/lib.rs:519-543`
- **C19** (medium) Peers can probe a tab for what it has just played. `browser/src/lib.rs:199-201`
- **C20** (medium) Unbounded growth: cards, listings, archives, rooms, connections, seen set. `browser/src/lib.rs:472-502`
- **C21** (medium) plan() re-hashes the whole log on every call. `browser/src/lib.rs:556-563`
- **C22** (low) Struck station leaves the tab stuck "Searching the Log" and hammering carriers. `browser/src/lib.rs:378-381`
- **C23** (low) Flock: no way to leave, relay URL taken from the link without checks. `browser/src/lib.rs:944-982`
- **C24** (low) Chat: the keeper label can be imitated, and bidi controls are not stripped. `browser/www/app.js:453; core/src/chat.rs:166-168`
- **C25** (low) Wasm traps are unrecoverable. `Cargo.toml `[profile.web] panic = "abort"`; browser/www/app.js:608-611`
- **C26** (low) Console logs identify the fresh node and the station being synced. `browser/src/lib.rs:49-51`
- **C27** (medium) presets::N0 may contact number0's discovery services and default relays. `browser/src/lib.rs:694-714`
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

- **P2** (high) STRIKE COLOURS can never complete: the node drops the 24 words and signs with the deputy key. `main.dart:1079-1101; node/src/lib.rs:1957-1960`
- **P3** (high) Auto Backup is left on: deputy keys, pocket secrets and prefs go to the user's Google account. `AndroidManifest.xml:16-19`
- **P4** (high) The 24 words can be lost three ways. `main.dart:646-679`
- **P5** (high) Exported MediaBrowser service hands the station list and control to any app, private pocket stations included. `AndroidManifest.xml:81-88; PlaybackService.kt:272-288`
- **P7** (medium) Podcast feeds and "cover from a link" call the network straight from the phone, outside the node. `station.dart:929-976`
- **P8** (medium) Feed "keep the newest N" downloads the whole back catalogue, and the feed list can be overwritten. `station.dart:889-926`
- **P9** (medium) Podcast feeds are never refreshed on their own. `station.dart:804`
- **P10** (medium) Drawer Lantern: the service restarts with no node, and the charging and Wi-Fi gate lives only in the Dart UI. `LanternService.kt:27-52`
- **P11** (medium) The "plain" disguise leaks the station name and the app name. `PlaybackService.kt:239-244`
- **P12** (medium) Station ids, bells and the alarm station sit in plaintext SharedPreferences and prefs.json. `PlaybackService.kt:90 and MainActivity.kt:164`
- **P13** (medium) Listening history is written to logcat. `main.dart:59`
- **P14** (medium) Temporary audio, pictures and APK copies are left on disk. `Ship.kt:22-42`
- **P15** (medium) After END BROADCAST the "ON AIR" notification, foreground state and both locks stay. `PlaybackService.kt:239-245`
- **P16** (medium) Every tune stacks a new audio-focus request. `PlaybackService.kt:102-107 and 123-140`
- **P17** (medium) Blocking native calls on the UI thread, in Dart and in the service. `PlaybackService.kt:53-58`
- **P18** (medium) The dial's PageView follows list position, not the station. `main.dart:203-214`
- **P19** (medium) Links join pockets, flocks and stations without asking. `main.dart:178-197; MainActivity.kt:28-39`
- **P20** (medium) Microphone permission dead-ends and the messages mislead. `MainActivity.kt:69-75`
- **P21** (medium) RECORD A SEGMENT leaves the microphone open if the panel is closed. `more.dart:700-703`
- **P22** (medium) The programme editor treats a failed load as loaded and can overwrite the real programme. `station.dart:536-547`
- **P23** (medium) A hosted flock loses its code when the sheet is closed. `station.dart:1688`
- **P24** (medium) Joined chatter and joined pockets cannot be left; muted voices cannot be unmuted. `station.dart:1212`
- **P25** (medium) Pocket stations are offered a public share link, poster and QR. `station.dart:199-201`
- **P26** (low) Bell ids differ between screens, depend on `String.hashCode`, and notifications are not checked. `station.dart:290 vs more.dart:373`
- **P27** (medium) The numbers kit decodes and rewrites WAV files sample by sample on the UI isolate. `station.dart:1562-1596`
- **P28** (low) Direct fixes to misplaced results: CLEAR THE HOLD, strike and restore all write the same `error` field. `main.dart:903-906`
- **P29** (medium) The 24 words and PINs are typed and shown with the keyboard's learning and no screen protection. `main.dart:825`
- **P30** (low) Opening the app always autoplays. `main.dart:70`
- **P31** (low) Android App Links are not verified, so scanned posters open the browser. `AndroidManifest.xml:43-54`
- **P32** (low) `pyrate_start` is not safe against two starters, and a failed start is invisible. `ffi/src/lib.rs:38-68; main.dart:58-60; Native.kt:21-31`
- **P33** (low) Feed and URL fetch code writes unsolicited temp files and the downloads can be stranded. `station.dart:952-975`
- **P34** (medium) Brass (amber) is used throughout. `main.dart:25`
- **P35** (low) Red is used beyond ON AIR, and there are two reds; M3 defaults bring rounded pickers and Roboto. `main.dart:23 vs 24`
- **P36** (low) The window behind Flutter is white in light mode. `app/android/app/src/main/res/values/styles.xml`
- **P37** (low) `kBuildCode` duplicates the pubspec build number. `main.dart:37`
- **P38** (low) Manifest hygiene. `AndroidManifest.xml:4`
- **P39** (low) `setState` after await without `mounted`, and controllers that are never disposed. `more.dart:394-406`
- **P40** (low) Passing the app on: a LAN HTTP server with no timeouts, binding every interface. `Ship.kt:90-125`

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

**A2. `t - cursor` underflow in `walk` when a run's computed start is after `t` (panic in debug, garbage in release).** (high) Fixed: every walk now refuses a start after the moment asked about, a tape whose start the clocks have only just reached begins a day or a week earlier, and a programme read from a log is made sane (offsets clamped to fourteen hours and sorted, minutes taken within the day, lengths within the week, days within seven). Regression tests: Berlin's spring change and a weekly tape at minute 65,535. (pending)

**A22. Spring-forward gap and `utc_ms` are inconsistent between east and west zones.** (low) Fixed: a local minute that was skipped begins when the clocks changed, and one that came twice begins the first time, by the same rule east and west of UTC. Tested at Berlin's spring change. (pending)

**A3. Rota walk has no step cap: a track of a few milliseconds hangs the lookup.** (high) Fixed: a recording under a second is not played (an Article ruling), the rota's walk has the same step cap as the programme's, and the watch bill has a budget of steps across all its lookups. A regression test signs a twenty-millisecond track and finds the clock quick and silent. (pending)

**A7. `at_ms + show_len` overflows for a signed Show with a huge `at_ms`.** (medium) Fixed: show ends are added with saturation. Tested with a show dated at u64::MAX. (pending)

**A16. Spans and tables read from the log are not validated, so `now` and `listings` can disagree and costs are unbounded.** (low) Fixed: the clock builds one index of tracks by hash and normalises the programme once, in Clock::new (see A2). (pending)
