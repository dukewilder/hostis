# D. What a keeper gives away

Read-only review of `radio` at 327d6c5; earlier audits' fixes are not repeated. Throwaway checks in /tmp/drev: the title functions on recorder file names, and a Python port of `voice.rs`. The working tree held someone else's uncommitted edits (lib.rs from line 3538, net.rs, door.rs, web.rs, privacy_test.rs); no cited line is in them.

Counts: Critical 1, High 2, Medium 8, Low 4.

### D-1 [Critical] A keeper's node confirms its own station to anyone who asks
Who: broadcaster
Where: node/src/lib.rs:608-613 (`Request::Chunk`), lib.rs:603-606 (`Request::Log`), lib.rs:502-519 (any peer is served)
What happens: `serve` hands out a chunk in the clear only when `is_own_named` says it is one of this node's own stations, and answers `Log` for any log it holds, its own included, with entries still waiting out the stem. Nothing checks who asks or whether the node is private. The guard (which sees the device send sealed cargo), a gossip neighbour, or anyone reading the Carried, Harbour and Crew notes a keeper phone signs under its day-long id while carrying ballast, sends `Chunk{h}` for a hash from station X's public log. An answer proves the node keeps X, and the same connection gives its relay and, on a direct path, its address. "The guard sees the phone and never the station" (XXXVIII) fails. The same `Log` path tells a prober what a listener phone plays.
Evidence: Verified by reading. The code says it ("In the clear, a node hands out only its own stations' sound", lib.rs:608) and the tab relies on it (browser/src/lib.rs:1340), while the tab itself refuses both requests (browser/src/lib.rs:555-560). No test sends `Request::Chunk` to a keeper.
Fix: on a private node, serve `Chunk` only for stations in `direct` or pockets, and `Log` for an own non-pocket station only up to the head carriers have landed. Add a privacy_test case asking a strict keeper for both.

### D-2 [High] Every signed entry carries the keeper's millisecond
Who: broadcaster
Where: node/src/store.rs:460-467; node/src/lib.rs:1848, 1860, 2827-2840, 1453; browser/src/lib.rs:3957
What happens: Track, TrackArt, Art, Colours, Programme, Deputy and Listing entries are stamped `now_ms()` at signing, and the cargo leaves on the next 10-second tick. The stem's 20 to 90 s wait hides when cargo appears; the stamp inside says when it was sent. The guard or the keeper's ISP sees a sealed upload of size S at T, then the public log shows an entry dated just before T whose constant-bitrate chunks add up to S. Two or three uploads single out the device. With no vantage, months of stamps (and the card's exact `since`) give waking hours, and gaps between batch-uploaded tracks equal encode time, which tells the device class.
Evidence: Verified by reading. XXIX (HARDENING-PLAN.md:240) kept stamps exact "since the programme counts on them", but the clock reads them only as "from when" (core/src/clock.rs:277-305), which a later time does not break. The release queue planned in 1.1 was never built.
Fix: round non-live stamps up to the next quarter hour (past the previous entry) and release the cargo then, not on the next tick; check that a live show started inside that window sorts after the pending stamp. Size still matches (1.2 was left out), so the handbook should say a guard or ISP can match uploads and live shows, and Through Tor answers it.

### D-3 [High] A co-host's name "for your own record" is signed into the public log
Who: broadcaster
Where: app/lib/more.dart:1190; node/src/lib.rs:2856-2866; card proof lib.rs:1450-1452
What happens: Adding a co-host asks for "Their name, for your own record". `appoint` writes it as the label of the master-signed `Body::Deputy`, which sits in the public, hash-linked log for good and rides as `proof` on every card the co-host signs. A real name typed there is published beside the station and cannot be taken back. The raise labels also differ ("phone" in the app, "deputy" in the tab, browser/src/lib.rs:3501), telling which tool raised it.
Evidence: Verified by reading.
Fix: keep the label in the hold (a note keyed by the deputy's key) and sign an empty one; use one raise label everywhere.

### D-4 [Medium] Local recording times reach public titles
Who: broadcaster
Where: app/lib/more.dart:1028; core/src/chat.rs:176-212; node/src/lib.rs:3500; app/lib/main.dart:1612-1625; browser/www/app.js:1979-2042
What happens: An untitled take recorded in the app goes out as "Segment, 21:03" in local time; beside the entry's UTC stamp that gives the keeper's UTC offset to the minute, a path the programme's zone note does not cover. File names become titles with no review in the app (`add`) or the studio (`airFile` signs as soon as encoding ends), `plain_title` cuts only runs of twelve or more digits, and `by` is never cleaned. Premieres set on the local hour hint at the zone too.
Evidence: Verified with test programs (the Rust function compiled alone, the JS one in node): "Audio Recording 2026-11-14 at 21.03.55", "Nov 14, 2026 9.03 PM", "11-14-26 21-03", "AUD-20261114-WA0003" and "jane-mbp take 2" pass unchanged; only "Recording_20261114_210355" is cut.
Fix: default the take to "Segment" and a counter; cut any token holding a clock time (h:mm, h.mm.ss, AM/PM) or an 8-digit date; clean `by` alike; show titles for editing before signing, as 1.14 planned.

### D-5 [Medium] Uploads skip the hum filter and the disguise unless "Spoken word" is on
Who: broadcaster
Where: core/src/ingest.rs:226; browser/www/encode-worker.js:60; browser/www/app.js:2010, 2025; browser/body.html:203; app/lib/main.dart:1284, 1775
What happens: Both filters run only for the Talk profile, and "Spoken word" starts off in app and studio. In the studio "Disguise my voice" sits beside it in the upload options; a keeper who ticks only the disguise uploads their own voice unshifted, hum and all, for good. The app's Spoken word note speaks only of file size.
Evidence: Verified by reading.
Fix: with Disguise on, treat an upload as talk or refuse it until Spoken word is ticked, and say on each switch what it does to the voice.

### D-6 [Medium] The disguise is one fixed shift that anyone can undo
Who: broadcaster
Where: core/src/voice.rs:69-109; -4 set in app/lib/more.dart:1734, app/lib/desk.dart:201, browser/www/app.js:2025, 2088
What happens: Pitch and formants move together by exactly four semitones on every device, so whoever knows the software shifts back four and has the keeper's spectral envelope for a voiceprint match. The app says "Lower and harder to recognise", the handbook "not quite yours", XXXI that it "is not the voice a voiceprint would be drawn from".
Evidence: Verified by simulation on a synthetic vowel (120 Hz, three formants): disguised f0 95.2 Hz, envelope correlation with the original 0.947; shifted back, f0 120.0 Hz and correlation 0.997.
Fix: say on both switches and in the handbook that it stops a casual ear, not a matcher; draw pitch and formant ratios separately per station from a secret seed so undoing it takes a search; drop the voiceprint claim from XXXI.

### D-7 [Medium] The hum comb stops at 360 Hz
Who: broadcaster
Where: core/src/voice.rs:51-52, 70
What happens: Notches reach the 7th harmonic of 50 Hz and the 6th of 60 Hz. Harmonics above (400, 420, 450, 480 Hz) pass untouched and carry the same grid trace, and the notches leave a residue off their centre.
Evidence: Verified before encoding by simulation (180 s of 60 Hz hum wandering 0.04 Hz, ten harmonics, noise, through the ported comb): grid-frequency correlation at 420 and 480 Hz stays 0.998; at the notched 60 and 360 Hz it is still 0.47 and 0.56. Suspected after Opus at 24 kbit/s, not run.
Fix: extend the comb to about 1 kHz for both grids, cascade two notches per harmonic, and add a voice.rs test measuring the trace after an Opus round trip.

### D-8 [Medium] Keeper fetches bypass Through Tor, and a podcast host can tie the keeper to the station
Who: broadcaster
Where: app/lib/more.dart:635-672, 1806; app/lib/station.dart:925, 994-1030; app/lib/main.dart (`linkArt`)
What happens: Through Tor routes the node only. Feeds, their episodes and "cover from a link" leave from the device's own address through a plain `HttpClient`. An episode is fetched, then signed seconds later as a Track whose `source` is the feed's URL, every few hours, so the host's log holds the keeper's address downloading exactly what appears on station X. The copy says "every connection goes through Tor" and the host sees the address "as with any podcast app".
Evidence: Verified by reading; no `findProxy` or `HttpOverrides` exists in app/lib.
Fix: with Tor set, give `fetchOut` a `findProxy` for the Tor HTTP tunnel or refuse the fetch; say on the feed note that the host can match the download to the station.

### D-9 [Medium] "Connect directly when needed" also sends the keeper's own station from the device, until restart
Who: broadcaster
Where: node/src/lib.rs:1491-1512, 1540-1569; app/lib/more.dart:1817-1820; app/lib/main.dart:1763
What happens: With strict off, three stems without a taker put the station in `direct`; the device then gossips its own card and signs a Carried note naming itself, the first observable source. The switch's text speaks only of listening. Nothing removes a station from `direct`, so this lasts until restart, though the page says "until more radios join".
Evidence: Verified by reading and grep (no remove, retain or clear on `direct`).
Fix: leave own stations out of this fallback unless the switch says so; keep trying the stem while `direct` and leave it once one is taken.

### D-10 [Medium] Keeper chatter comes straight from the keeper's device
Who: broadcaster
Where: node/src/chat.rs:150-155, 191-207; app/lib/station.dart:1447
What happens: In its own room the keeper speaks with the station or deputy key, shown as "(the station)", on a gossip topic whose members connect directly. A neighbour gets station-signed lines first from the keeper's node and sees its address. The keeper gets only the listener's note.
Evidence: Verified by reading.
Fix: give the keeper its own note before joining; better, send the keeper's lines through two hops to a node in the room.

### D-11 [Medium] The card's cadence shows when the keeper's device is on
Who: broadcaster
Where: node/src/lib.rs:1466-1483, 295-297
What happens: Each own card is signed afresh every 8 to 12 minutes while the node runs, with an exact `at_ms`, and stays fresh for 15. Anyone keeping the station can log the series; its gaps are the hours the device was off.
Evidence: Verified by reading.
Fix: re-sign an unchanged card at most daily at a random time with `at_ms` rounded to the hour, and let freshness come from carrier notes, which `fresh` already accepts; the 30-day listing rule (L) needs no more.

### D-12 [Low] PIN files outlive a wipe and name the emergency PIN
Who: broadcaster
Where: node/src/lib.rs:3304-3352; app/lib/shell.dart:266-276; app/android/app/src/main/kotlin/black/hostis/pyrate/Hold.kt:79-96
What happens: `pin` and `pin-duress` sit in the node folder under plain names, outside the hold, as Argon2id (32 MiB, t=3) of PINs as short as four digits: a forensic copy shows an emergency PIN exists, and 10,000 offline guesses recover each. On computers a wipe deletes only the key, so both files stay and the next start asks for the PIN again, unlike "opens next time as if newly installed". A matching emergency PIN returns before the second hash, so "both PINs are always worked out" is false (moot, as the app then closes).
Evidence: Verified by reading.
Fix: keep one fixed-size PIN record as a hold note (a dummy when no emergency PIN is set), hash both before answering, and on computers clear the node folder after a wipe, as Android's `clearIfScuttled` does.

### D-13 [Low] Tips and hails say more than their copy
Who: broadcaster / listener
Where: app/lib/more.dart:760-768, 1103-1104; app/android/app/src/main/kotlin/black/hostis/pyrate/Ship.kt:25-28; node/src/chat.rs:349-354
What happens: A Lightning tip address (name@domain) names the keeper's account with a provider that usually knows them, and the note covers only Bitcoin and Monero. Tippers are not told a Bitcoin tip is public. The "note for the keeper" travels as BIP21 `message` or Monero `tx_description`, which never leave the tipper's wallet. A hail is the phone's raw MPEG-4 file, sealed as is: the keeper gets the sender's unfiltered voice and whatever the recorder writes in the file.
Evidence: Verified by reading for tips and the raw hail; Suspected for the hail's file metadata (creation time, Android version), not inspected on a device.
Fix: one line under the tip field for Lightning and one on the tipper's sheet for Bitcoin; drop the note box for Bitcoin and Monero; send hails through the Opus talk path, hum comb included, before sealing.

### D-14 [Low] The words alone open no decoy
Who: broadcaster
Where: core/src/keys.rs:92-99; app/lib/main.dart:1432
What happens: XXXII and the `from_words` doc say words given up under force "open that other, empty one instead". Nothing is raised under the words alone, so restoring with them fails with "Those words did not open a station. Check each word and the passphrase, if you set one", which tells a coercer a passphrase exists. The app's own copy makes no such claim.
Evidence: Verified by reading.
Fix: drop the claim from XXXII and the doc, or raise a small decoy station under the words alone at raise time.

### D-15 [Low] Track art goes out without being shown
Who: broadcaster
Where: node/src/lib.rs:1857-1862; node/src/art.rs:46-56
What happens: An upload signs the file's embedded picture, else any cover, folder, front or album image beside it, as the track's art unasked. It is redrawn (no EXIF), but a folder.jpg that is a personal photo is published for good.
Evidence: Verified by reading.
Fix: show the picture and ask before signing it, or take art only from inside the file.

## Checked and sound

- Art is decoded, turned upright, cropped to a fixed 640x640 or 1500x500 and re-encoded as plain JPEG; EXIF, GPS, serials and original size do not survive, and the studio goes through the same `prepare` (core/src/art.rs:49-69).
- ID3, Vorbis and MP4 tags never reach an entry; only the picture is read (core/src/ingest.rs:192-202). File mtimes and `File.lastModified` are read nowhere.
- Opus settings match across app, desk and studio (VOIP for talk, AUDIO for music, constant bitrate).
- Every non-live stem cargo, cards included, waits 20 to 90 s at the stem's end, live under 1.5 s (node/src/hop.rs:936); strict nodes never fall back to a plain stem.
- The zone goes out only when the programme keeps clock times, and the editor says so (app/lib/station.dart:609-625).
- The tab answers no one's Log, Chunk or SealedChunk request (browser/src/lib.rs:555-560).
- The live desk runs the hum comb and disguise on the microphone in every show (node/src/desk.rs:1306-1341).
- Pocket keys are random 32 bytes kept in the hold, with distinct derive_key contexts and random 96-bit nonces (core/src/pocket.rs).
- seal.rs: each key comes from one chunk's hash and seals only that chunk, so the fixed nonce never repeats under a key.
- Routes and hails: a fresh X25519 key per request or hail, distinct ask and answer nonces, wrap keys bound to their recipient (core/src/route.rs, core/src/chat.rs:63-98).
- The hold: name and seal keys derived apart, random nonces with the file's name as associated data, mlock and zeroize (node/src/hold.rs).
- keys.rs: getrandom seeds, Argon2id at 64 MiB, t=3, p=1, salt drawn from the seed, `verify_strict` signatures; a co-host's code carries only its public key (node/src/lib.rs:3179-3183).
