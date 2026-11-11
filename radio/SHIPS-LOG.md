# Pyrate Radio · Ship's log

## Evening of 9 November 2026: 0.7.0 and 0.7.1

**Built and tested**

| Piece | Where | State |
| --- | --- | --- |
| The programme | `core/src/programme.rs`, `core/src/clock.rs` | Local time with daylight saving in the log; shuffled, in order, newest first, daily and weekly tapes; on-air hours; regular shows (recordings, newest, a hand's watch, live); premieres; notices. 11 tests |
| The station page | `core/src/page.rs`, app, browser | Notices, on now, up next, the week. Same code in the phone and the tab. Bells in the app, calendar files in the browser |
| Starting a station | app | Round the clock, in order, daily or weekly loop, a podcast feed, live every day, chosen when the colours go up |
| Feeds | app, `node/src/web.rs` | Podcast feeds come aboard on the phone; a Lantern serves each own station as a podcast feed with MP3 episodes. Checked: valid RSS, 128 kbit/s stereo, ranges |
| Lantern seed files | `node/src/bin/lantern.rs` | `programme.json`, `notices.txt`, a home port in the seed. Checked: set once, idempotent on restart, notices come down |
| Chatter | `core/src/chat.rs`, `node/src/chat.rs`, browser | Rooms on gossip, voices per station, work against floods, nothing on disk. Lanterns keep rooms and hand newcomers the last lines. Checked between two tabs and two Lanterns |
| The browser set | `browser/` | A dial with every station at its frequency, needle, lamp, meter, in hostis.black's own design. Spin the dial, the chart of the seas, Pass it on with a QR code, Clip it (20 s vertical video with sound) |
| Posters | app | Name, frequency and a code to scan, for paper or screens |
| Spoken idents, numbers kit | app | The phone's own voice; interval signal and five-figure groups |

**Not yet**

- Pocket stations (private, invitation only). Hail (a voice note to the keeper). Flock (phones in one room playing as one). A drawer phone kept running as a Lantern by a service of its own.
- The browser on Safari and with the screen locked on an iPhone, and on the public network from this workshop.

## Night of 8 to 9 November 2026

**Built and tested**

| Piece | Where | State |
| --- | --- | --- |
| Transport | `spike/` | iroh confirmed on a phone on T-Mobile: direct in 5 s, relay where blocked, no gaps |
| Core | `core/` | Keys and 24-word export, call signs, signed hash-linked log, Opus chunks, file ingest. 16 tests |
| The clock | `core/src/clock.rs` | Same answer on every node to the millisecond. Weekly rounds, blocks, ident on the bell |
| Node | `node/` | Store with quota, wire protocol, the Log over gossip, proof of work, aboard sketches, playout, ON AIR |
| Lantern | `node/src/bin/lantern.rs` | Running on Dylan's PC. Carries its own stations (every station with `--carry-all`) and raises HOSTIS's own from folders, rescanning every ten minutes |
| App | `app/` | Flutter, Android first, in the site's type and ink. The dial, the Log, colours, ON AIR, the library, the key, strike |
| Page | `index.html` (built by `site/build_page.py`) | `hostis.black/radio/`, on this branch only |

End to end, tested from a cloud machine through the PC Lantern: the Log arrives in 6 to 12 seconds, a station's log syncs, the clock picks the track, the right chunk comes from a carrier and decodes. A live broadcast from one node was followed chunk by chunk by another.

On the phone the node starts, joins the Log and finds the PC Lantern. Confirmed by Dylan on the morning of 9 November: it plays, and keeps playing with the screen locked.

**On the air**

- The Public Domain (`GAHAGU·3695`): 1925-and-earlier recordings from Wikimedia Commons, public-domain only, source line on every track.
- Readings (`DEVAKA·F42C`): LibriVox. Johnson's General History of the Pyrates (two readings), The Pirates Own Book, Treasure Island. A children's book the search pulled in was withdrawn with `lantern --withdraw`.

**Open**

1. Until a HOSTIS Lantern runs on a public server, the network leans on number0's relays and DNS for finding peers. A small VPS running a Lantern with its own relay ends that, and it is also what the web listener and iPhones need.
2. On Android the node asks 1.1.1.1 and 9.9.9.9 for DNS, because the system resolver needs a Java context the node does not yet pass to iroh's resolver.
3. Release signing: the key stays with Dylan. Builds are signed with the debug key until then.
4. Not yet built: web listener, Hail, Tape it, Local filter, Pass it on, F-Droid repository, reproducible builds, export notice, security review.

**9 November: the radio plays in a browser**

- `browser/` builds the listener to WebAssembly: 3 MB, 1.3 MB over the wire. `browser/build.sh` writes the static site to `browser/www/dist`.
- Tested in Chrome against a private network on the cloud machine (its own relay and a Lantern with a test station): the log arrived in 0.4 s and the card by gossip in 9 s. Twenty-two seconds of recorded output had the right tones in every second and no gaps on either decoder. The tab counted itself aboard within 5 s.
- Not yet heard against the public network from here, because this machine's proxy will not carry the relay's WebSocket. The first real test is opening the page on a phone or PC.
- Not yet tested: a live broadcast in the tab, Safari, and the locked-screen trick on an iPhone.

**9 November: the Lantern moves to dukewilder**

- The HOSTIS Lantern now runs on dukewilder, inside the gluetun tunnel with the arr stack, with no ports open. Node id `a324c679b0dda368e23f21d6abbdaec0ef541a4ac317fa0643802b2734c9a442`, now the app's first peer.
- The station keys, logs and recordings were copied over the LAN and adopted. A fresh node that knew only dukewilder found both stations in 12 seconds and played from them.
- Public relays throttle bulk transfer (12 MB in 15 minutes) but carry a stream at radio speed. A HOSTIS relay on a public server lifts the throttle for listeners behind VPNs.

**9 November: the gangway**

- The Lantern serves a web listener (`--http`): the Log as JSON, each station as an endless 128 kbit/s MP3 stream that any browser and any iPhone plays, and a page in the site's type and ink. One encoder per station, however many tabs listen. Browser listeners count toward aboard.
- The Lantern runs an iroh relay (`--relay`). Every node now knows `relay.hostis.black` alongside number0's relays and falls back to number0 while it is dark.
- Both bind to loopback inside the tunnel. A Cloudflare tunnel, itself running inside the Proton tunnel, puts them at `radio.hostis.black` and `relay.hostis.black`. The house address appears nowhere.
- Next: a second relay route through Proton port forwarding, its moving address published in the signed Log, so the relay outlives Cloudflare.

**The PC Lantern (stopped)**

- Folder: `Documents\Pyrate Radio` on Dylan's PC. Data in `lantern-data`, audio in `stations\public-domain` and `stations\readings`.
- Node id `72b966c645197137b553995ff690349153c4219cc08278ebdde15dcf7bd29d72`, the app's first peer (`app/assets/bootstrap.txt`).
- Relight: `powershell -ExecutionPolicy Bypass -File light.ps1`. New files dropped in a station folder go on its clock within ten minutes.
- The PC is set never to sleep on mains power. If it does, the HOSTIS stations stay up only on phones that carry them.

**9 November: the second voyage**

- Built and tested end to end on one machine (`voyage_test`, 16 checks): cards carrying kinds, mature, port and tips; a listener working out the watch bill; a co-host appointed with the 24 words and writing as a deputy; a Lantern carrying the whole Log, and its maroon list taking a station off it, art included; maroon lists shared and taken aboard; an old-format log carried into the new format.
- The Lantern's hourly bell set from a seed folder. Google's apksigner verifies the offline signing.
- Every decision and what was set down is in the Ship's Articles, sections XVI and XVII.


**9 November: hardening, and the private stations**

- Carriers now hold sealed. A carried station's sound and log sit on a node by fingerprint, under a key it does not have. A node opens plaintext only for stations whose keys it holds, and sealed logs read as noise on disk until the right id arrives. A seized node answers for what its owner kept, not for what it carried.
- Phones draw a fresh network id every day, a steady seed behind it keeping the count, the chat voice and the ballast seat. Dark running drops direct paths, publishes no address and speaks through Tor. A harbour splits into a public face that keeps only its own stations and a dark hold that carries the whole Log over Tor on a separate id. The dukewilder rebuild in `tools/dukewilder-rebuild.md` sets both up inside gluetun.
- Pocket stations: private, off the Log, found only by the key that is their invitation, in the app and the browser. Flock: phones in a room on one clock. Hail: a sealed voice note to the keeper. The drawer phone: an old handset left as a Lantern. Tips carry a note.
- Built and checked on the test network: sealed carrying and playback, sealed logs surviving a restart, chatter with a backlog, a hail reaching a keeper's Lantern, a dark Lantern carrying and mirroring the player, pocket stations seen by a member and invisible to an outsider, a flock within about a millisecond.
- App at 0.8.0. The browser player rides on any Lantern at `/player/`, and the APK seeds over BitTorrent with a web seed.

## 10 November 2026: the audit

A full read of every line, findings kept in `AUDIT.md` as they are found and fixed. Each fix is its own commit.

- The stem no longer names its broadcaster: every node sends the hop count as zero, and a stem that comes round again spreads.
- Pocket stations stay off the public Log: their tokens count among members on their own topic, they never take the public stem, and the Lantern's gangway refuses them.
- A pocket's chatter is sealed on a topic drawn from its key, in the app and the browser, and a tab never puts a pocket's id or invitation in the address bar.
- A phone carries for the fleet only on Wi-Fi while charging, and never a station its owner marooned or keeps hidden.
- One strict hex parser everywhere, the app's door catches panics, and the Android context is set once a process: a malformed link or a second start no longer kills the app.
- A plain chunk from before sealing can no longer be fetched in the clear; struck stations leave no pictures; chatter memory stays bounded.
- The Lantern's player mirror serves its own folder and nothing else: a doubled slash could have reached any file on the machine.
- The network clock takes one vote a station and needs five to agree, so no single station can set every listener's clock wrong.
- The clock cannot be stopped, hung or overflowed by anything a station signs: daylight saving east and west, nonsense minutes, twenty-millisecond tracks and shows at the end of time all have tests.
- The Lantern's gangway holds under load: bounded listeners, towers, guest counting and episode making, ranges served from disk, and feeds that no Host header or stray character can break.
- A false carrier claim jumps no queue and a flood of them grows nothing; peers that fail to answer are asked last.
- Nodes find each other inside the network: carriers sign a note of their relay into the gossip, no node id is looked up in plain DNS, and tabs publish nothing about themselves.
- Private fetches keep a long-lived first hop, as Tor's guards do, and a broadcaster's phone never asks carriers for its own station from its own address.
- Strike colours works from the app: it takes the 24 words, as the screen always said.
- The app keeps its secrets on the phone: no backup or transfer, a media browser only for the system and the car, a quiet log, and the 24 words guarded from loss, screenshots and keyboard learning.
- App 0.8.1 (build 14) built with everything above. It waits on Dylan's Android key: the debug key that signed 0.8.0 is gone, so this build cannot install over it unsigned.
- Everything a node keeps on disk is sealed under a key of the device's own, wrapped by the phone's secure hardware: a seized phone is noise, even to someone holding the whole Log.
- A browser tab is counted only while it plays, through two hops as a phone is, and answers nobody about what it holds.
- The Colours PIN is kept under a slow, memory-hard hash.
- The playback service leaves the foreground when a broadcast ends, keeps one claim on the speaker, and in the plain disguise names nothing on the lock screen, the bells or the Lantern.
- A link from outside waits on the dial for a yes before anything plays or joins. Pocket stations offer no public link or poster, chatter and pockets can be left, muted voices heard again, and only the very first open plays on its own.
- Podcast feeds bring aboard their newest few and then only new episodes, read themselves every few hours on Wi-Fi, and every fetch from the open web goes out under a plain name with timeouts and a size cap and leaves nothing behind.
- Closing the segment recorder stops the microphone, takes and copies leave the phone once set, recordings are set one at a time, and the programme editor will not save over a programme it failed to read.
- The dial keeps its station when the Log reorders, and turning it from a link no longer tunes in before the owner says yes.
- A hosted flock keeps its code when the sheet closes, and clearing the hold or taking a station back answers on the spot.
- Asking for the microphone waits for the answer, and a phone that will not ask again offers its settings instead of the same dead message.
- A bell is the same bell on every screen and in every build, and one set while notifications are off says so at once.
- A drawer Lantern starts its own node when Android restarts it, keeps its own charger-and-Wi-Fi gate, comes back after a reboot, and a radio that cannot start says so.
- The phone's own notes outside the node (its bells, its wake-up station, the last station tuned) are sealed under its hardware key like everything else it keeps.
- The app keeps to the site's sheet: no amber anywhere, one red kept for the open air, square hairline dialogs and sheets in Fell, and a black window even on a phone set to light.
- Passing the app on by QR serves it under a secret path on one address for fifteen minutes, and selected text is never offered to other apps.
- The screen and the playback service never wait on the node: the dial polls in the background and the service has a thread of its own for every call.
- The numbers kit builds its messages in the background.
- App 0.8.2 (build 15) built with the whole app audit, every phone finding closed. It waits on Dylan's Android key, as 0.8.1 did; the 0.8.1 build is superseded.
- The browser radio recovers from a stumble, says when a station has struck or no carrier answers, keeps trying a relay, fetches each chunk from two carriers at once, and trims what a long-open tab gathers.
- In the browser: flocks and chatter can be left, shared chat cannot impersonate a keeper or run text backwards, the console stays quiet, a hail cannot go astray, covers load as they come into view, links outlive hostis.black without naming anyone's address, and a deploy can never mix old and new code.
- The landing's Android fallback works, its build will not ship placeholders, its copy says what the embedded set holds, and the browser set keeps bright red for the open air with no text under the site's smallest size.
- A keeper's Lantern keeps hails sealed under its own key rather than writing them out, and opens them only when the keeper asks.
- The Lantern's page and the browser set both run under a strict content policy with no referrer, and a Lantern's /player/ copy links home properly.
- A Lantern's web listener reconnects by itself when its stream breaks.
- A browser tab whose node faults says so and comes back aboard by itself, once.
- The browser set says what it keeps and can forget all of it in one press.
- Browser playback holds up on iPads and through interruptions, a hidden tab keeps a minute and more queued, and the chart, harbour memory and debug hook are tidied.
- The clock crosses a week of short recordings in a few steps, so a station of jingles plays to the end of Sunday and every listener's clock does less work.
- The hour the clocks repeat in autumn neither rewinds a daily or weekly tape nor cuts a slot short, and the page lists slots as they air.
- The watch bill and the page's next shows list what really airs, cut where a slot, a show or the end of the hours takes over, so a bell rings when its show begins.
- A premiere of a recording added moments before airs, and a show with nothing to play is not promised on the page.
- Clock and sound hardening: a station's page shows its key beside its call sign, every chunk plays exactly its length, tapes never come out unplayable, a span must name a day, and a library in order is heard in order across its set hours.
- Pictures come out upright with a stable accent, films keep their dialogue when folded to stereo and silent files are refused, the device's own sample format is used, the page is built in one pass, the keeper hears when a time-zone table runs out, and the clean turn of the week is ruled.
- SCUTTLE: one press makes everything the radio keeps on a phone unreadable at once and closes the app; a Lantern has --scuttle. App 0.8.3 (build 16) carries it with everything since 0.8.2, waiting on Dylan's Android key.
- Distribution: an update is checked against its signed hash every time before it is offered, and a release needs two different keys however the key list is written.
- App 0.8.4 (build 17): everything above, the update check included; it supersedes 0.8.3 and waits on Dylan's Android key.
- One more HOSTIS page: the landing, the browser set and a Lantern's page now wear the HOSTIS bar, masthead, divider, section heads, grain and footer, in the site's own faces and sheet. The set is redrawn as a receiver, the radio's own wordmark retires, and the landing's set no longer leaves a gap under it on a phone.
- App 0.8.5 (build 18): the app wears the same HOSTIS bar on every tab, Pica titles and station names, the site's red for Listen and for the station in hand, mono captions, the divider, and the HOSTIS skull as its icon and its notification mark. It supersedes 0.8.4 and waits on Dylan's Android key.
- hostis.black now plays HOSTIS's stations alone. Every other station, invitation and flock goes straight to the open player: the same set on IPFS, owned by nobody, pinned by anyone who runs pin_player.sh. The app, invitations and flock links name it too.
- Any browser can now raise a station and put it on the air, iPhones included: the open player makes the key, shows the 24 words once, levels and encodes recordings with libopus in the tab and sends them along the stem. The app runs on Windows, macOS and Linux, built from the same code in CI. The open player installs to a home screen.
- The open player is live on IPFS under one fixed name, served by dukewilder's own node through a separate VPN tunnel, and opens and comes aboard in an ordinary browser. Every link to another station now uses that name.
- App 0.8.7 (build 20) and the open player: wave 1 of HARDENING-PLAN.md. Stations stay out of browser history, a tab can seal its broadcasting key under a passphrase, recorder dates never become titles, look-alike names are marked, the keyboard and clipboard keep nothing, the time zone goes out only with clock times, and computers keep recordings out of the shared temporary folder.
- App 0.8.8 (build 21) and Lanterns: wave 2 of HARDENING-PLAN.md. A full Log admits a new station only for more proof of work than its weakest, own stations work harder when the Log is crowded, a station flooding a disk is trimmed first, and every decoder a stranger can reach is fuzzed with each build.
- App 0.9.0 (build 22), Lanterns and the open player: wave 3 of HARDENING-PLAN.md. Phones and tabs no longer tell number0 which peers they want, the network's own address book survives a restart, a node prefers its own relays to number0's once it has any, a pure listener is new at every start, stems hold cargo a while before showing it, keepers' cards no longer mark the second their device wakes, guards are veterans, every chunk is the same size, and a Lantern's relay shares itself fairly.
- App 0.9.1 (build 23), Lanterns and the site: wave 4 of HARDENING-PLAN.md. Releases wait three days from first sight (or a tap), signed bearings and a monthly canary ride the Releases station, every published file has an IPFS name that pin_player.sh pins, the source ships as a tarball and torrent each release, CI actions are pinned to commits with locked dependencies and an advisory audit, the vendored Opus files are fingerprinted, and two APKs can be compared without their signatures.
- App 0.9.2 (build 24), core and the open player: wave 6 of HARDENING-PLAN.md. A voice loses its mains hum before encoding (live, talk recordings and talk files, in app, computers and browser), a keeper can shift their voice lower to foil a voiceprint, and the on-air screens say plainly what the filter cannot take out.
