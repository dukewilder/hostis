# Pyrate Radio · The Ship's Articles

Every decision the build rests on, set down before the first line of code. Each one was weighed against the site: the Articles, the Practice, the Broadside, the sign rite. Where a choice and the colours pulled apart, the colours won, and the cost is written beside it.

A station that plays all day and all night, that anyone can raise in three taps, that every listener carries, and that answers to no server, no store and no account.

---

## I. Name, place and licence

**Name.** Pyrate Radio. The golden-age spelling, as the Account prints it.

**Home.** The existing HOSTIS repository, in `radio/`, served at `hostis.black/radio/`. One ship, one flag. The site already says the site will be taken down. Pyrate Radio is built so that when it is, nothing on the air goes quiet.

**Licence.** CC0, as the rest of the repository already is. No rights reserved, all copies welcome. The Articles say a copy takes nothing from the maker, and a GPL would fence the code the Articles say cannot be fenced. Dependencies keep their own licences. Each is listed in `radio/THIRD_PARTY.md`.

**Branch.** All work lands on `radio` first. Nothing reaches `main`, and so nothing reaches the live site, until Dylan says so.

## II. The words

The app speaks the site's tongue. Every term below is used in the interface, the code and the docs, and no synonym is ever used beside it.

| Term | Meaning |
| --- | --- |
| Station | A broadcaster. One keypair. |
| Colours | A station's identity: name, call sign, flag. |
| Raise colours | Create a station. |
| Strike colours | End a station. |
| Call sign | Short name derived from the key, e.g. `NAAMEH·52E1`, for saying aloud. Nobody chooses it, but it is not proof: a forger can grind out a lookalike. The key is the station, and the station's page shows it. |
| The Log | The shared directory of every station on the air. |
| Aboard | Listeners. "41 aboard." |
| Carry | Hold and relay a station's audio for others. Every listener carries. |
| ON AIR | A live broadcast, which takes the clock over until it ends. |
| The clock | The station's standing schedule, which plays when nobody is live. |
| SCAN | Step through stations, three seconds each, until you stop. |
| Hail | A sealed voice message from a listener to a station. |
| Tape it | Keep the last two minutes of what you are hearing. |
| Lantern | A headless node that runs all day on a spare machine. |
| Pass it on | Hand the app itself to someone nearby. |

The site's light imagery (the second candle lit from the first, the cathedral in the dark that keeps the candles lit) is why the always-on node is a Lantern.

## III. What it is to use

**First open plays sound.** No sign-up, no tour, no permission screens before audio. The app opens on the station with the most aboard and it is playing within three seconds. Everything else is one swipe away. A radio is something you turn on.

**Three screens.**
1. *The dial.* What is playing, who is aboard, the station's flag, ON AIR in red when live. Swipe sideways to change station. Hold to SCAN.
2. *The Log.* Every station, sorted by aboard, filterable by kind (talk, music, readings, news, local) and by port.
3. *Your colours.* Raise a station, or run the one you have.

**Raising colours takes three taps.** Name it. Drop audio in, or press the red button to go live. Done. The clock fills itself from what you dropped in, in order, looping, until you change it. Rules for the clock (this hour talk, that hour music, a station ident on the bell) are there for those who want them and invisible to those who do not.

**The look is the site.** IM Fell English for the colours, JetBrains Mono for the instruments, bone on black, red only for ON AIR and the live dot. The red is `#ed5555`, the accessible one the audit settled on. The app is the site's sister sheet, the way On the Account and the Reprints are. No new look is invented.

**A station's flag is drawn from its key.** Every station gets a black flag generated from its public key, in the manner of the site's black spot and Mirror. Two stations can never share a flag and nobody can forge one. Stations may also raise art (section XV); the flag stands wherever they have none.

## IV. The machinery

**Core.** Rust. One library, `pyrate-core`, used by every build.

**Interface.** Flutter, for Android, Linux, Windows and macOS from one codebase.

**Transport.** iroh: QUIC connections, hole punching, `iroh-gossip` for the Log and live audio, `iroh-blobs` for content-addressed archive chunks. Veilid was weighed and set down. Its private routing is the stronger privacy story, but a radio that stutters loses its listener in ten seconds, and the colours say the product has to be better than what people already use before privacy counts for anything. The spike ran on 8 November 2026 on a phone on T-Mobile and a PC on home broadband: sound in 0.72 seconds, a direct hole-punched path inside five seconds, and no gaps across relay and direct runs alike. iroh is confirmed. The results are in `spike/RESULTS.md`.

**Relays.** iroh's relays forward traffic when two peers cannot reach each other directly. Pyrate Radio never depends on anyone else's relays. Every Lantern can run as a relay, the app ships with a list of HOSTIS-run and crew-run relays, and the list is itself gossiped and grows.

**Audio.** Opus. 64 kbit/s for music, 24 kbit/s for talk, chosen by the station. Six-second segments, each hashed and signed.

**A station is a keypair.** Ed25519. The key is made on the phone and never leaves it unless the owner exports it as a word phrase. No account exists anywhere. Hold your own keys.

**The archive.** Every segment a station has broadcast sits in a signed, append-only, hash-linked log. Anyone carrying a station can prove every segment came from that key and in that order.

**The clock is computed, never served.** A station publishes a signed rota: its rules and its library. Every node works out what is playing now from the rota, the current UTC time and the day's seed. A thousand phones agree on the same second of the same track with no server telling them. Tune in and you join mid-song, as everyone else aboard is hearing it.

**ON AIR.** A live broadcast travels over gossip with five to fifteen seconds of lag. When it ends, the station hands back to the clock, and the clock resumes where the hour has reached.

**Carrying.** Every install carries the stations its owner listens to, up to a storage quota. Default 2 GB on phones, 20 GB on Lanterns, changeable either way. Carrying stops on mobile data unless the owner turns it on. A station with one listener survives its broadcaster's phone going dark. A station with a thousand cannot be found in any one place to be shut.

**The Log.** A distributed directory, gossiped between every node. Each entry is signed by the station and carries a small proof of work, so raising ten thousand junk stations costs ten thousand times the effort of raising one.

**Aboard counts.** Each listener emits a token that rotates hourly and is unlinkable to the last. Nodes merge the tokens into HyperLogLog sketches and gossip the sketches. The figure shown is rounded ("about 40") and nobody, HOSTIS included, can see who the forty are. A station cannot inflate its count without paying proof of work per fake listener, and counts from listeners who never fetch audio are discarded.

**Local.** A station may name a port: a city. Listeners never send their location. "Local" means stations that declared your chosen port, nothing more.

**Updates.** A signed release manifest is gossiped over the network itself. The app checks the signature against the key printed on the site and offers the update. No call home.

## V. Distribution

**No app stores.** The store is the fence. The 2025 TikTok blackout showed that two companies can take an app off 170 million phones in a day, and the Practice already says it: refuse what the click was meant to extract.

**Where builds come from.**
- `hostis.black/radio/`, with checksums and signatures.
- Git mirrors on GitHub and Codeberg, with releases attached.
- A torrent of every release, seeded by Lanterns.
- IPFS.
- A self-hosted F-Droid repository, so de-Googled phones get updates the way they already expect.
- Pass it on: the app serves its own installer over local Wi-Fi with a QR code, so one phone can hand it to the next with no internet at all.

**Builds are reproducible.** Anyone can rebuild from the source and get the same bytes. The release signing key stays with Dylan, offline, and never enters a build server or this workspace. Verify what you install.

**Android developer verification. No papers.** Google's rule starts on 30 September 2026 in Brazil, Indonesia, Singapore and Thailand and goes global in 2027. It requires a developer to hand Google an identity before an app installs on a certified phone. Pyrate Radio does not register. Show no papers. The cost is real and is stated plainly on the download page:
- On stock Android in those countries, listeners turn on Google's "advanced flow" once: Developer options, allow apps from unverified developers, then a one-time 24-hour wait. After that it installs like anything else.
- ADB installs are unaffected.
- GrapheneOS, LineageOS and other de-Googled systems are unaffected.
- Anyone who will not wait listens in the browser (below) the same day.

Ask no man's leave for what is already yours. A radio that needs Google's leave to be installed is a radio Google can switch off.

**iPhone.** No App Store build. iPhones listen through the web listener.

**The web listener.** A static page at `hostis.black/radio/listen/`. It reaches the network through any Lantern's relay, plays any station and shows the Log. It cannot carry or raise colours, because a browser tab cannot keep a station alive.

**Lantern.** A single binary for Linux, Raspberry Pi and any VPS. Carries, relays and serves web listeners. One command to start. The Practice says add to the crew: run a node, however small.

## VI. What HOSTIS puts on the air

The network launches with stations already playing, so the first listener never finds an empty sea. HOSTIS raises its own colours and carries only what is already free or is its own:

- **HOSTIS.** The Articles, the Broadside, the Practice and the Annals, read aloud, on a clock that runs all day. The Broadside asks to be read aloud where it will be heard. This is where.
- **The Public Domain.** Recordings published in 1925 or earlier, free in the United States since 1 January 2026, with 1926 joining on 1 January 2027. Jazz, blues, opera, country, tango, speeches. A century of sound, and every year the fence moves back another year and the station gets longer.
- **Readings.** LibriVox audiobooks, every one of them public domain, as a round-the-clock reading station.
- **The Hourglass.** A news station read from the daily broadside scout.

The Stores rule holds on the radio. No crew station is ever ranked, featured or recommended by HOSTIS. The Log sorts by aboard, and the listeners decide.

## VII. The line held

The colours make the argument. The radio is a press, and a press carries whatever its crew sets in type. The project builds the press and keeps the following lines, because a wrong step here hands the owners the ship:

- **No transmitter.** Nothing leaves the phone as a radio wave. Pyrate Radio is radio in spirit and the internet in fact, so no unlicensed broadcasting arises.
- **No locks broken.** The app contains nothing that strips DRM or defeats access controls.
- **No inducement.** HOSTIS's own stations carry public domain and HOSTIS's own words. The project does not advertise the app as a way to get anyone's catalogue. The case law on file sharing turned on exactly that.
- **Pictures never ride as ballast.** Stations carry art (section XV), but no phone holds a stranger's picture on its disk for the fleet, and browsing keeps pictures in memory only.
- **Strike colours is honoured.** A station that strikes its colours signs a strike notice, and every honest node drops its archive. The sign rite says it: the mark stays as long as the hand that made it chooses to honour it.
- **The crew chooses what it carries.** Every node can maroon any station, which hides it, stops it being carried and drops what was held. The choice stays on that device and is told to no one. There is no shared list and no one to report to: HOSTIS answers for HOSTIS's stations and its own Lantern, and every other hand answers for its own.
- **The truth about addresses.** Peers can see each other's IP address, as on BitTorrent. The app says so on the first Raise colours screen, in one sentence, and points to the Practice for those whose threat model needs a VPN or Tor. A route through Tor (Arti) is the first privacy feature after launch.
- **United States export.** The app uses standard published encryption. Before the first public release, the notice to the Bureau of Industry and Security and the NSA for publicly available encryption source code is sent by email. It is one email.

## VIII. Money

- **No ads. No token. No coin of our own.** The app moves no money.
- **Tips go straight to the station.** A station card may carry a Monero or Bitcoin address. The listener's own wallet pays it. Pay in coin.
- **No grants.** NLnet is funded by the European Commission. The Open Technology Fund is funded by the United States Congress. No flag of nation stands above the colours. The build costs what the crew puts in.
- **No analytics. No crash reporting.** A crash shows a report the user may copy and send by hand. A thing that watches its bearer is not his.

## IX. Order of work

Each step is done when it works on a real phone.

1. Transport spike: iroh stream across two phones on mobile data, through a relay and direct.
2. `pyrate-core`: keys, call signs, the signed segment log, Opus encode and decode.
3. The clock: rota format, now-playing computed identically on three machines.
4. Carrying: blobs, quota, mobile-data rules.
5. The Log: gossip, proof of work, aboard sketches.
6. ON AIR: live over gossip, hand-back to the clock.
7. Flutter app, Android first: the dial, the Log, Your colours, in the site's type and ink.
8. Desktop builds.
9. Lantern.
10. Web listener.
11. HOSTIS stations: record the readings, assemble the public domain library with sources for every recording's date.
12. Hail, Tape it, SCAN, Local, Pass it on.
13. Reproducible build pipeline. Dylan signs.
14. `hostis.black/radio/` download page, F-Droid repository, torrent, IPFS, mirrors.
15. Export notice sent.
16. Audit: security review of the core by someone who did not write it.
17. One line on the HOSTIS homepage pointing to the radio. Dylan approves the wording and the place before it goes on `main`.

## X. What Dylan does

Only what cannot be done from here:

- Holds the release signing key and signs each release.
- Pushes the `radio` branch from his machine, and later merges to `main` when he says.
- Reads the HOSTIS stations' scripts aloud, or chooses the voice that does.
- Sends the export email from the HOSTIS address.
- Approves any change to the homepage.

---

*Set down for the crew, in this present year, and for every year after.*

## XI. Ballast, the stem and the clock's foreknowledge (9 November 2026)

**Strike stays immediate.** A delayed strike was weighed and refused. Ending a station is the key holder's act, and protecting the key is the holder's charge.

**Hold and ballast.** Every node keeps two stores. The hold is what its owner listens to, kept warm and evicted when full. The ballast is a fixed donation (1 GB on phones, filled only on Wi-Fi while charging; as much as a Lantern can spare). Each node ranks every station in the Log by the hash of its own id and the station's, and carries the nearest stations whole, up to its donation and 300 MB a station. No coordination is needed, and a station with no listeners is still carried across the fleet. Erasure coding was weighed and deferred: across phones that are online a few hours a night, coded shards are often unreachable, and whole-station copies on the nodes that drew them serve better until the fleet is large and steady.

**Deputy keys.** Raising colours makes a master key and a deputy. The deputy lives on the phone and can broadcast. Only the master can strike, appoint or revoke, and the master is shown once as 24 words and kept nowhere. A seized phone is shut out the moment the words appoint a new deputy anywhere.

**The stem.** New entries and chunks leave the broadcaster along a stem of relaying nodes before they spread, after Dandelion++ in Bitcoin. Every node relays stems, so the first hop cannot tell an origin from a relay. Broadcasters keep long-lived guard nodes for the first hop, as Tor does. The cost is well under a second on a six-second chunk.

**The clock's foreknowledge.** The schedule is computed, so every node knows which chunks it will need hours before it needs them. Everything predictable is fetched ahead through anonymous routes, where latency costs nothing. Every fetch takes two hops, the first seeing who asks and the second what is asked. Tune-in costs a few hundred milliseconds more. Listener tokens for the aboard count travel the same way, so neighbours never see what a node hears. A broadcaster's node never announces itself as a carrier of its own station. Dark running (relay-only, over Tor through Orbot's HTTP tunnel port) is the next switch, for those who face an adversary on their own line.

## XII. The listener's protection, and their own chart (9 November 2026)

**A phone never announces what it listened to.** Phones announce themselves as carriers only of their ballast, which the hash chose for them. What they heard is served by Lanterns and ballast holders.

**The hold is sealed.** Every chunk on disk is named by a hash of its address and encrypted with a key drawn from the address, after Freenet's content-hash keys. A seized device holds unlabelled noise; only someone who already holds a station's log can test for that station. Older unsealed chunks are sealed the first time they are read.

**Clear the hold.** One button empties everything carried except the owner's station and the ballast.

**Keep and maroon.** Each listener keeps a chart on their own device: kept stations at the top of the Log and the dial, marooned stations gone from both. Nothing about the chart leaves the phone.

## XIII. Stress test (9 November 2026)

- **Counts are checked token by token.** A gossiped sketch could be forged to show millions aboard. Listeners now send single tokens, each bound to the station and the hour and carrying a proof of work (a second or two on a phone, once an hour), and every node builds its own count from tokens it has checked. Counts can still be bought, at a price in work.
- **The gangway carries only its Lantern's own stations.** A public page that streams any station makes its keeper the distributor of whatever is on air. Any Lantern can do what HOSTIS does: `--http` serves its own stations, with an embed line for any site.
- **A stranger's first minute must be earned.** First open lands on the busiest station with a week on the air and three independent carriers, or on HOSTIS's own stations until one exists.
- **Mobile data carries nobody else's traffic.** Phones relay hops and stems only on unmetered networks.
- **No single door.** Every node remembers the peers it has met as starting points, and a Lantern is one Docker command (`LANTERN.md`).
- **Raising a station costs 24 bits of work**, several seconds on a phone, and the Log keeps at most 5,000 cards, the weakest giving way.
- **Time is the network's.** Each node corrects its clock by the median of the station cards it hears.
- **Carriers take aboard only chunks the station's signed log names**, so nobody can fill a disk with junk that merely matches its own hash.
- **Revoked deputies cannot sign cards** that any node holding the log will accept.
- **Guards are spread.** A broadcaster's first hop avoids the well-known Lanterns, so no one machine holds every broadcaster's address.

## XIV. Releases (9 November 2026)

- **Releases are a station.** Release key one is the Releases station; its log carries versions of the app as chunks, each trusted only when two of the three release keys have signed it. Every node pins it, so an update spreads phone to phone with nothing to block. Phones offer a release and install it only on the owner's tap.
- **Dylan holds all three release keys for now**, as word sheets kept in three places. A second and third holder can take one each later without changing a line of code.
- **The Lantern publishes from a folder**: `releases/` for signed builds. The keys that sign never sit on it.
- **No refusal list.** One was built and taken out. A shared list is a lever, and whoever holds a lever gets leaned on. Each node maroons for itself, privately; a Lantern operator adds ids to `maroon.txt` on his own box.

## XV. Art (9 November 2026)

- **Stations carry pictures.** A cover for the Log and the dial, a banner across the station's page, and the album cover inside each recording while it plays. The cover's strongest colour themes the station's page; a grey cover gets bone.
- **Art is signed into the log, never linked.** A link would hand every browsing listener's address to whoever runs the image host, could be swapped silently, and goes dead. A picture named by its fingerprint can do none of that.
- **Every picture is redrawn before it sails**: decoded, resized, and written fresh as a plain JPEG of a fixed size. Location data and anything hidden in the file are gone.
- **Listeners never hand a stranger's file to the phone's decoder.** Pyrate decodes art in memory-safe code and gives the screen raw pixels. Anything but a plain JPEG of station size is refused.
- **Art is never ballast, and browsing never writes it down.** Pictures seen in the Log live in memory and are gone when the app closes. Art lands on a phone's disk, sealed, only for stations the owner keeps, listens to or broadcasts. A phone at the end of a stem holds a broadcaster's art in memory until the Lanterns take it aboard; Lanterns carry art for the stations they carry. Exits pass chunks through without keeping them.
- **A Lantern's seed folder takes `station.jpg` and `banner.jpg`** as the station's art; `cover.jpg` beside recordings is their track art.
- **The mark is the skull; the wordmark is PYRATE ☠ RADIO**, traced from Dylan's choice, in HOSTIS bone. Files in `brand/`.

## XVI. What a Lantern carries (9 November 2026)

- **A Lantern carries the whole Log by default**, Dylan's ruling: the network has to be substantial, and a station nobody carries is a station nobody hears. A keeper takes any station off his own Lantern with `maroon.txt`, and `--carry-own` limits a Lantern to its own stations.
- **Crew lists, never a HOSTIS list.** Anyone can share the stations they have marooned as a plain list; anyone can take a list aboard. It is a copy, made once: nobody can change a list after someone else takes it, so no list is a lever.
- **A station nobody can hear is worse than one heard from a known address.** If three stems in a row find no taker, the broadcaster's phone goes on the air directly and its Colours page says so in one line. As the fleet grows, stems land and the cover returns. Strict mode turns this off.

## XVII. The second voyage (9 November 2026)

Each item was weighed before building. What was built:

- **A format that can grow.** Log entries carry their bodies in a length prefix, and a body this version does not know is kept and passed on exactly as it came. A variant, once released, never changes; new information goes in a new variant. Station cards are signed over their raw bytes and only ever grow at the end, so an old app reads what it knows. Without this, every change after launch would split the network and could strand phones from their updates. Old logs are carried over once: a station whose master key is present starts a fresh log with its colours and art, and its seed folder refills it.
- **One loudness.** Every recording is measured and brought to -16 LUFS as it is added, with dead air trimmed from the head and tail and no peak above -1 dBFS. 1925 shellac and a modern podcast play at the same level.
- **The watch bill.** The clock is worked out on the phone, so the app shows what plays today and this week and rings a bell when a show starts, with no server and no connection needed.
- **Radio basics.** Lock-screen and headphone controls, Bluetooth and the car (Android Auto, for drivers who allow apps from outside the Play Store), pausing for calls and ducking for voice prompts, stopping when headphones come out.
- **Wake to the radio**, rising from quiet. If no sound comes within 45 seconds, the phone's own alarm rings, so nobody oversleeps on a dead network.
- **Lights out**, heard tonight (memory only), search, sort (most aboard, longest on air, newest, most carried), kinds, Local by port, and what is on now beside each station.
- **Provisions.** On Wi-Fi, the next six hours of up to three kept stations come aboard ahead of time, sealed.
- **For broadcasters:** record a segment in the app; the station's papers (kinds, mature, port, tips); the clock's blocks and bell; hands (co-hosts) appointed with the 24 words; an hourly chart of the aboard count. Taking a station back now fetches its log first and then shuts the old phone out, as the screen always said. Two phones writing one station at once no longer fork it: the network's copy stands and the later phone's entries are written again after it.
- **Tape it.** The last two minutes as a plain Ogg Opus file, shared from the phone.
- **Pass it on**, by QR over local Wi-Fi or a hotspot, or by Quick Share, with no internet.
- **Station links** (`hostis.black/radio/#<id>`, `pyrate://station/<id>`) open the app.
- **False colours.** HOSTIS's harbour stations carry the skull seal, which no other key can show, and a station flying a harbour station's name is marked.
- **Discretion.** A plain icon named Radio, a plain lock screen, a PIN on Colours, and strict mode for those who would rather stutter than ever ask a carrier directly.
- **The bell.** A seed folder's `bell.wav` plays on the hour. `tools/bell.py` writes one a day from the Annals, in order, and the day's broadside when given one, spoken by a voice running on the Lantern's own machine.
- **`tools/feed2folder.py`** brings a broadcaster's own podcast feed into a seed folder.
- **Signing on Dylan's machine.** `release android-init` makes the Android key as 24 words; `release sign-apk` signs a build offline. Verified with Google's own apksigner.
- **Holes found and closed on the way:** the app never read its harbour list or its release keys; a station's own phone never synced its own log; time arithmetic could overflow when a carrier's clock ran ahead; marooning left a station's art and log on disk; a Lantern's maroon list did not stop it serving the station's log.

What was weighed and set down, for now:

- **Memory-hard tokens for the aboard count.** Every node checks every token, so a heavier puzzle would cost every phone battery to slow one spammer. Sorting, maroon lists and rounded counts answer it better.
- **Hail.** Sealed voice notes would sit on Lanterns that cannot see inside them. Not while Lanterns carry everything by default.
- **The BitTorrent DHT door.** This version of iroh has no DHT discovery; crew Lanterns in the bootstrap list do the same work for now.
- **The browser node.** The whole network in a web page, so iPhones hear every station. A project of its own, and the hard part (playing on a locked iPhone) needs a real iPhone to prove.
- **Tape it as a video clip** for X and TikTok, and a podcast feed of HOSTIS's stations. Both wait until the core is on the air.
- **The deputy key in the phone's secure hardware.** The PIN covers a casual look; the hardware vault is next.

## XVIII. The browser (9 November 2026)

A tab is a node. `browser/` builds the listener to WebAssembly from the same core and the same wire protocol as the app (`node/src/net.rs`, shared as it stands), so a tab joins the Log by gossip, checks every card and entry against the station's key, follows the station's clock and fetches chunks by fingerprint from whoever carries them. Nothing runs on a server: the page is static files any host or mirror can serve, and taking one copy down takes nothing off the air.

- A browser opens no ports, so a tab reaches everyone through relays. The stations it hears from never learn its address; the relay sees only sealed traffic.
- A fresh node key every visit, so one visit cannot be tied to the next. The hourly listener token comes from a separate key the tab keeps, so a reload is not counted twice.
- Chunks and logs live in memory and go when the tab closes. While open, the tab answers others for what it holds.
- Opus decodes with the browser's own WebCodecs, kept warm across a track so the six-second seams never show. Browsers without it get a portable decoder (opus-decoder, MIT, 88 KB), loaded only when needed.
- On iPhones the sound goes out through an audio element, which keeps playing with the screen locked.
- Maroon lists work in the tab, in the same format as the app's and a Lantern's maroon.txt.
- Listening only, for now. Raising and broadcasting stay in the app.
- `PYRATE_RELAYS` (node) and `PYRATE_HARBOUR` / `PYRATE_RELAYS` (browser build) name a private network's own relays and peers.

## XIX. Self-moderation (9 November 2026)

Dylan's ruling: moderation belongs to each listener. No votes, no reputation scores, nobody elected to judge for others; anyone given power over what others see gets leaned on, or leans.

- **The station's own word.** A station says whether it is mature (strong language or themes) and whether it is explicit (sexual or graphic material). Two separate labels, because they are different things.
- **The listener's own switches.** Explicit stations stay out of the Log and off the dial until the listener turns them on. Mature stations show unless the listener hides them. Art from stations the listener has never kept can be hidden. All of it in the app and in the browser, and none of it leaves the device.
- **Maroon and pass the list.** A station that lies about itself is the listener's to maroon. Lists of marooned stations pass hand to hand as copies, taken aboard by choice.
- The explicit label rides at the end of the station card, so older apps read the card as before and an older card reads as not explicit.

## XX. Chatter, the programme and the page (9 November 2026)

**Chatter.** Every station has a room: a gossip topic of its own where listeners talk, cheer what is on and ask for a recording from the library. A listener speaks with a voice drawn from their own secret and the station, the same each visit and tied to nothing under any other station. Each line carries about 65,000 hashes of work, so a flood costs the flooder time. Lines are signed, checked by everyone, held in memory and written nowhere. A Lantern sits in the rooms of its own stations and the busiest in the Log, so a newcomer finds someone there and the last lines to read. The keeper speaks with the station's key or a deputy's and shows as the station. There is no moderator: each listener mutes whoever they choose. Joining is the listener's choice, because it tells the room this node is there, which listening never does. Hail, a voice note sealed to the keeper, waits for the room to prove itself.

**Requests.** A keeper sees requests in the room and can play one next: the app writes a one-off show of that recording at the end of what is playing.

**The programme.** A station keeps a programme in its keeper's own time, daylight saving written into the log as a table of offsets: shuffled, in order, newest first, a daily tape or a weekly one; on-air hours; regular shows filled with chosen recordings, the newest, a hand's watch or a live hour; premieres; notices. Whichever of rota and programme is later rules, so old stations are untouched.

**The page and the dial.** Every station has a page worked out from its own log by the same code on phones and in browsers. Every station has a frequency drawn from its key, the same on every dial. Posters carry it with a code to scan.


## XXI. Hardening (9 November 2026)

Every node already carried the whole Log. This voyage asks what a carrier can be made to answer for, and sets it to nothing it did not choose.

- **Blind carrying.** A carrier holds and serves a station's sound and log sealed, by fingerprint, without the key to open them. It is a name and a box of bytes, the way a Freenet node holds what it cannot read. A node serves plaintext only for the stations whose keys it holds. So a seized phone shows what its owner chose to keep and nothing of what it carried for others.
- **Sealed logs at rest.** Each carried log is written to disk sealed under a key drawn from the station's id, one record at a time. It opens only when the id arrives through the owner's own keys, a station card or a request. On disk it reads as noise.
- **A new address every day.** A phone's network identity is drawn fresh every 24 hours, so no one follows one phone from day to day by its id. A steady seed behind it keeps the listener count, the chat voice and the ballast seat unbroken across the change. Lanterns keep their id, so a newcomer has a fixed door.
- **Dark running.** `PYRATE_DARK=1` drops every direct path and stops the node publishing any address, so nothing ties its id to a relay or an IP. `PYRATE_PROXY` sends its relay connections through Tor. A dark node is reached through relays and through addresses shared inside a station's gossip, and a listener notices none of it.
- **Two identities.** A harbour is split in two: a public face that keeps only its own stations and stands as a bootstrap door, and a dark hold on a separate id that carries the whole Log over Tor and is advertised nowhere. The known node is a station keeper; the node that carries everything is tied to neither the harbour nor the house.
- **No single door.** Every node remembers the peers it has met and starts from them next time. A browser tab keeps every harbour it has ever been handed. Several harbours and the crew relays stand together, so there is no one door to close.

## XXII. Private and together (9 November 2026)

- **Pocket stations.** A station that never shows on the Log. Its key is the invitation: whoever holds it finds the station on its own sealed topic, hears it, carries it and passes it on, and no one else can tell it exists. Its log is asked for by a tag drawn from the key, never by its id, and is never served to a stranger. Raised in the app with a switch, shared by a link or a scanned code, joined in the app or any browser.
- **Flock.** Several phones in one room play as one. One phone hosts, the others sample its clock over the local relay, keep the closest reading and fall in with it, a hand's trim either way for the slow speaker in the corner. It cannot be faked without real phones in the room.
- **Hail.** A listener sends the keeper a short voice note, sealed to the station's key and its deputies, that only the keeper can open. A keeper's own Lantern may keep hails for a keeper who is away (`PYRATE_HAILS`), each still sealed to the station and sealed again under the Lantern's hold key, so its disk shows neither a voice nor a sender; `lantern --open-hails <folder>` opens them for the keeper and forgets them.
- **The drawer phone.** An old phone kept as a Lantern by itself: a foreground service with wake and Wi-Fi locks, a ballast size from one to 256 GB, carrying the Log in the back of a drawer.
- **Tips with a note.** A listener leaves the keeper a tip with a line of thanks.

## XXIII. The audit (10 November 2026)

Rulings made while reading every line again. The findings themselves are in `AUDIT.md`.

- **The stem carries no count.** Every node sends the hop count as zero, so the first hop cannot tell a broadcaster from a relay. A stem ends by the fluff draw, and cargo that comes round again spreads.
- **A pocket is sealed end to end.** Its card, carriers, tokens, log and chatter travel only on topics drawn from its key, sealed with it. It never takes the public stem, never goes through a stranger's exit, and never stands on a Lantern's gangway.
- **A phone carries for the fleet only on its charger.** Ballast is announced and served only on Wi-Fi while charging, and on mobile data a phone serves strangers nothing.
- **Carrying is the owner's choice all the way down.** No node takes a stem or ballast for a station its owner marooned, and a phone takes no explicit station while its owner hides them. Lanterns still carry the whole Log unless their keeper maroons.
- **One vote a station for the network's clock**, and five must agree before anything moves.
- **A recording shorter than a second is not played.** It is not a recording worth airing, and it would let a station make every listener's clock walk a week in twenty-millisecond steps.
- **The network keeps its own address book.** Carriers sign where they can be reached into the gossip, so no directory outside the network is needed to find a carrier, and nothing is looked up in plain DNS. Publishing to number0's directory continues only until the fleet carries the notes.
- **A disk is noise without its own key.** Everything a node keeps is sealed and named under a hold key of the device's own, kept in the phone's secure hardware. A carrier still cannot read what it carries: the old seal, under each chunk's own address, stays inside.
- **Red means the air is open.** ON AIR, LIVE, the live dot, and a microphone while it records; nothing else in the app is red, and there is one red, `#ed5555`. Warnings, strikes and maroons are bone like everything else.
- **Each week starts clean.** The library's week begins at Monday 00:00 (local under a programme, UTC under the old rota), and a slot or a daily tape begins on its minute, so any node can work out any moment from that week alone, with no history to agree on first. The price is that what is playing at the turn of the week, or when a slot begins, is cut there. Letting it finish would make each week start where the last one happened to end, and so depend on every week before it.

## XXIV. One more HOSTIS page (10 November 2026)

- **The radio wears the HOSTIS bar, not a mark of its own.** The landing, the browser set, a Lantern's page and the app open with the flagship bar (the ☠ and HOSTIS in Pica capitals, linking home), a Pica masthead with an italic line under it, the site's divider, carved section heads with mono captions, its grain and vignette, and its footer. Faces and sheet come from the site itself (`brand/fonts.py`, `brand/hostis.css`), so the radio cannot drift from its siblings. The PYRATE ☠ RADIO wordmark and the crossed-microphone skull retire; the icon is the HOSTIS skull. This replaces the wordmark ruling in XV.
- **Two reds, as on the site.** The site's red, `#c52828`, is the accent it already is on every sibling page: selection, focus, the divider's diamond, the + of a fold, a hover, the one control that matters most on a screen (Listen, Take the set), and the mark beside whatever is tuned. The bright red, `#ed5555`, stays for the open air alone: the lamp, ON AIR, LIVE, the live dot and the meter's hot end. Warnings, strikes and maroons are still bone. This refines "Red means the air is open" in XXIII.
- **The set is drawn as a receiver.** A hairline faceplate with chart corners, the station in Pica capitals, every reading in mono, the dial behind glass with the needle in blood. Square hairline controls everywhere, as the sibling pages draw them; nothing rounded.

## XXV. Nothing on hostis.black belongs to anyone else (10 November 2026)

- **hostis.black plays HOSTIS's stations alone.** Its copy of the browser set, the landing's set and every link on the domain carry only the stations HOSTIS raises. A takedown demand aimed at the site finds nothing there that is not HOSTIS's own, and the chatter, being other people's words, is not shown there either.
- **Every other station lives in the open player**, the same set served where nobody owns it: from IPFS, under a name that is the fingerprint of its own bytes, and from any Lantern that serves `/player/` at a name of its own. HOSTIS publishes each open player as a CAR beside a list of every one it has published, and any IPFS node that runs `pin_player.sh` keeps them all alive. No Lantern at a HOSTIS name serves it. Its links use one fixed IPNS name, published from dukewilder's own IPFS node through a VPN tunnel of its own with a forwarded port, so browsers can fetch the player straight from it (WebTransport, WebRTC, secure WebSockets) and a browser keeps its settings and stations from one release to the next.
- **A link for anything else goes straight on, unread.** A stranger's station, an invitation or a flock arriving at hostis.black is handed to the first open player that answers, with the part after the # intact. That part never reaches a server, and the page never names, shows or plays what it holds.
- **A stranger's station never wears hostis.black.** The app, the open player, invitations and flock links name the open player for every station but HOSTIS's own, and its posters say only to scan the code or search the call sign.

## XXVI. Every platform can listen and broadcast (10 November 2026)

- **The open player raises stations.** Any browser, an iPhone's included, can raise a station, put recordings on its clock and send them out, with nothing installed. The station is a key made in the tab. Its 24 words are shown once and kept nowhere; the browser keeps only a deputy's seed, which the words can always replace. Recordings are decoded by the browser, brought to the house loudness by the app's own rule, encoded with libopus off the page's thread, signed into the log, and sent along the stem as a phone sends them: the tab never shows the network its own station first, and it serves no one. Once a carrier has them, they play whether or not the tab is open. A tab goes live from its microphone the same way, a six-second chunk at a time, each signed and sent along the stem as it is made.
- **A tab broadcasts with less cover than a phone, and says so.** The relay a tab speaks through sees its address. The page offers Tor Browser for the most cover, and the app for more still.
- **The app runs on computers.** Windows, macOS and Linux run the same app from the same code, with the node in Rust playing, going live and recording through the computer's own sound. A computer has no secure chip the app can reach, so its hold key lives in a file only its user can read. Builds come from the public source on GitHub's machines and are published on the radio's page with checksums and torrents; nothing secret goes into them, and anyone can build the same.
- **The open player installs to a home screen** as an app of its own, on HOSTIS black.

## XXVII. A device keeps less of what it hears and airs (10 November 2026)

- **The tuned station stays out of the address bar.** The open player tunes a passed link, then takes the id out, so it lands in no history and no synced browser. The tab alone remembers it in session storage for a reload, and forgets it when it closes. On a shared gateway nothing is kept at all.
- **A tab's deputy can be sealed or forgotten.** A keeper can seal the deputy seed under a passphrase (PBKDF2 with 600,000 rounds, AES-GCM), so a copied browser profile gives nothing away, or keep it in the tab alone. The 24 words bring a station back either way, so neither choice can lose one.
- **A recorder's date and time never become a title.** Phones name recordings by the moment they were made, which gives away a keeper's day and zone. The stamp and the device's words around it ("VID", "Recording") come out, in the app, on computers and in the browser alike. A bare date stays, since a keeper may mean it.
- **Strangers' names cannot pass for others.** Direction marks and hidden characters are stripped from every name, title and line a node shows. A name mixing Latin with Greek or Cyrillic letters wears "False colours?", as a name copying a HOSTIS station already did.
- **The keyboard learns nothing.** Every text field in the app asks for incognito typing. Copies the app makes are marked sensitive, so keyboards and clipboard previews keep them out of their history and cloud, and they leave the clipboard after ninety seconds if nothing has replaced them. The same holds on computers.
- **The time zone travels only with clock times.** A programme that only shuffles goes out with no zone at all. The editor says that clock times carry the zone where anyone can read it, and the home port and tip fields say what each one shows.
- **The plain disguise also blanks the recents screen.** With the lock screen saying only "Radio", the app's window shows nothing in recents or screenshots.
- **The app shows the fingerprint of the key that signed it**, so a copy signed by anyone else can be told apart.
- **On computers, the radio's files stay its own.** Recordings and shares go to a scratch folder inside the app's own folder, emptied at each start, never the shared temporary folder another account can read. That folder is kept out of Spotlight, Windows Search and Tracker.

## XXVIII. Floods cost more than they buy (10 November 2026)

- **A full Log takes a newcomer only for more work.** Past 5,000 cards, a new station comes in only by showing more proof of work than the weakest card a stranger holds there. Each further junk station costs twice the last. A node's own stations, the ones it keeps and pockets are never at stake. Counts aboard only break ties, since they are cheap to inflate and work is not.
- **Real stations climb.** When its Log is nine-tenths full, a node keeps working on its own stations' proof, a little each tick and off the async threads, and flies the better nonce. When there is room it spends nothing.
- **A flood shrinks first.** Over quota, a node drops chunks no station names, then the oldest chunk of whichever station holds the most on its disk. One station filling a disk loses chunks before any other loses one.
- **Every decoder a stranger reaches is fuzzed with every build.** Random bytes and mutated real messages go into the gossip, card, request, response, hop, chatter, entry, chunk, Opus and art decoders, 6,000 rounds with the ordinary tests and 120,000 in a deep pass, with no panic. libopus is 1.6.1.

## XXIX. The network keeps its own address book, and its own time (10 November 2026)

- **Phones and tabs ask no one where peers are.** A phone publishes nothing to number0's address server, and asks it only about its first doors and peers met before; a tab asks only about a harbour named without its relay. Every other peer is found in the network's own book, the signed notes carriers send every two minutes. A phone that asked number0 about every carrier would have told it, from its own address, which stations it was after. Lanterns, being public doors, still publish and look anyone up, so older apps find them.
- **The book outlives a restart.** Peers met before are kept sealed, each with the relay its note named, so the next start reaches them without a lookup. A door handed over as `id@relay` is reached the same way.
- **Our own relays first, number0's on standby.** A relay named in a first door's own note, or a Lantern's own (`PYRATE_RELAY_URL`), becomes a home relay ahead of number0's. When there is one to try, number0's relays come in only if none of ours answers within fifteen seconds. relay.hostis.black has no address yet, so today every node still homes on number0's; wave 5 gives dukewilder's Lantern a public relay.
- **A pure listener is new at every start.** A phone that keeps no station and no pocket draws a fresh network id each time the app starts, not only each day. One that keeps a station or a pocket keeps its id for the day, so invitations and appointments still reach it.
- **The moment cargo appears says nothing of when it was sent.** The end of a stem waits twenty seconds to a minute and a half before it shows a recording, and under a second and a half for the live air. A keeper's card is signed afresh only when something on it changes, or every eight to twelve minutes, so the network no longer learns to the second when a keeper's device is awake. What was sent along the stem is not sent again for four minutes, which spares a phone's data, and a resend goes round the guard.
- **Guards are veterans.** A broadcaster's guard and a listener's first hop are drawn from peers met in an earlier run before newcomers, so a crowd of nodes that appeared today cannot make one of its own a guard. The well-known first doors are still passed over.
- **Every chunk of a profile is the same size.** Recordings and the live air are encoded at a constant bitrate, so a chunk's size on the wire says nothing of what plays, and a listener's traffic cannot be matched to a recording by its shape.
- **A relay serves everyone a share.** A Lantern's relay gives each client a megabyte a second with bursts of eight, and takes new connections at fifty a second, so no one client can take it for itself.

## XXX. Releases are seen before they land, and the papers travel by any road (10 November 2026)

- **A release waits three days.** An app takes a new version only three days after it first hears of it, counted on the device rather than from the time the release claims. A version the keys' holders were forced to sign is in front of everyone before it is on anyone's phone. The app says when the version arrives, and anyone can tap to take it at once.
- **Bearings and a canary.** Two signed documents ride the Releases station's log beside the releases, and travel by any other road too, because they are checked wherever they land. The bearings name the first doors, the open players, the IPNS name and every published file by its IPFS name, and take two of the three release keys, since they steer where new installs go first. A node keeps the newest it has checked and starts from its doors next time. The canary is one line signed once a month by any release key. The app shows it under "The canary", and when it is more than 45 days old it says what that may mean. Until the first canary is signed, the app shows nothing.
- **Every published file has an IPFS name.** The app, the computers' builds and the source at each release (a tarball with a torrent) are listed by their IPFS names. Any node running `pin_player.sh` fetches each once from the site and keeps it only if it hashes to the name it was listed by.
- **The build checks what it is built from.** Every action the computers' builds use is pinned to a commit, not a tag a third party can move. Cargo builds take only the locked dependency versions. Every build checks those against the RustSec advisories first. Its first run found a timing flaw in the elliptic-curve code the release tool signs APKs with, and the tool moved to the fixed version. The Opus files the browser player ships must match their recorded fingerprints, or the build stops.
- **Two builds can be compared.** `tools/apk_compare.py` compares two APKs entry by entry, leaving out the signatures, so a build made elsewhere from the same source can be checked against the one Dylan is asked to sign.


## XXXI. A voice carries less of where and who (10 November 2026)

- **Mains hum is filtered from every voice.** A microphone near the electrical grid picks up a faint hum whose small drift utility operators record over time, and a recording's hum can be matched against those records to estimate when it was made and on which grid. Live air, talk recordings and talk files have it filtered before they are encoded, in the app, on computers and in the browser alike: narrow notches at 50 and 60 Hz and their harmonics, which a voice is far too broad to be dimmed by. Music is left untouched.
- **A keeper can disguise their voice.** A switch in Discretion, and one in the browser studio, shifts the voice a few steps lower in pitch and timbre together, so what goes out is not the voice a voiceprint would be drawn from. It is off by default. The browser carries the same filter and shift as the app, checked to match it.
- **The app says what it cannot hide.** The on-air screen and the browser studio now say plainly that the hum is removed but that sirens, bells, trains and other voices nearby are not, and that these can place a broadcast. Honesty about the limit is the point: a keeper who thinks the room is scrubbed is in more danger than one who knows it is not.
