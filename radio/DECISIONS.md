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
| Call sign | Short name derived from the key, e.g. `NAAMEH·52E1`. Cannot be forged or squatted. |
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
