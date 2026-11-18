# A. Network node: privacy and security review

Commit 327d6c5 (branch radio). Scope: node/src/*.rs, node/src/bin/lantern.rs, core/src/route.rs. Read only. Dependency sources were checked from crates.io: iroh 1.4.1, iroh-relay 1.4.1, iroh-gossip 0.102.0, str0m 0.25.0, igd-next 0.17.1. Nothing was built or run. Line numbers are at 327d6c5; the working tree now carries an uncommitted dark-door change that moves door.rs lines after 234 down by six.

## Who learns what

| Party | What it can learn |
|---|---|
| Gossip neighbour | The origin of every gossip message, since iroh-gossip sends a publisher's own messages at round 0: the station each crew listener plays (A-2). It can also dial and probe the node (A-1, A-3). |
| Guard | Device IP and node id, and with A-1 and A-3 which station the device keeps or plays. Guards churn (A-7) and see restarts (A-8). |
| Middle, exit | Middle: guard and next radio. Exit: station and guard. Sound. |
| Carrier | The exit's asks only, apart from the A-1 and A-3 probes any peer can make. |
| Relay operator (relay.hostis.black and the Cloudflare Tunnel in front of it, number0 on first run or fallback, any Lantern relay) | Client IP, node id, and a deck id from the same IP whose public note names the station (A-2). Restarts (A-8). |
| Door | Tab IP and tab node id, by design. |
| Lantern keeper | Web listeners (documented). It serves listeners' door IPs to anyone (A-4). |
| Passive ISP | Radio use (relay DNS and SNI, QUIC ALPN), burst timing, and "Pyrate Radio door" on an ISP-managed router (A-6). |
| HOSTIS | Everything a relay operator learns (A-2). A deploy bakes listener IPs into every player build (A-4). |

## Findings

### A-1 [Critical] A keeper's device hands its own station's sound to anyone who asks
Who: broadcaster
Where: node/src/lib.rs:610-613; node/src/store.rs:487-498; accept path lib.rs:498-575
What happens: A peer that knows a phone's node id (a gossip neighbour, its guard, anyone holding its crew, harbour or door note, anyone on its LAN) sends `Request::Chunk { hash }` naming one track chunk from station S's public log. The node returns it only if S is one of its own stations (`is_own_named`). One request per card across the Log names the keeper of every station whose device the prober can reach. The stem guard, which sees the device send sealed cargo, confirms the station with one request, which undoes XXXVIII's "the guard sees the phone and never the station". `Request::Log` gives the same answer during the stem's 20 to 90 s wait, when only the keeper holds the newest entries.
Evidence: Verified by reading. serve() answers clear Chunk requests for own chunks on every node, and cfg.private is never consulted on the accept path.
Fix: On cfg.private nodes, answer `Request::Chunk` from art_mem only, and refuse `Request::Log` for own stations unless the station is in `direct`. Add a privacy_test that probes a keeper phone for its own chunk.

### A-2 [High] Deck notes tie each crew listener to the station it plays
Who: listener
Where: node/src/deck.rs:121-130, 167; node/src/lib.rs:1141-1145; iroh-gossip src/proto/plumtree.rs:225-229, 627-644
What happens: Crew is on by default (lib.rs:2582, app/lib/main.dart:261). On Wi-Fi, or on any computer, the node opens a deck for the station it plays and broadcasts `Gossip::Deck { deck, station, .. }` from its own gossip endpoint. iroh-gossip sends a publisher's messages at `Round(0)`, and receivers get `scope.is_direct()`. Every gossip neighbour therefore knows the note naming the station came from that node, its id and, on a direct path, its IP. Separately, the deck endpoint homes on the node's own relay from the same IP, and the note names the deck's id publicly. The relay operator (HOSTIS and Cloudflare's tunnel, number0 when its relays are home, any Lantern relay) maps IP to station for every crew listener. A fresh deck endpoint per tune also gives the relay each tune-in time.
Evidence: Verified by reading the node and the iroh-gossip source (`Round(0)` at broadcast; `is_direct` documented as "directly received from its publisher").
Fix: Send the deck note through a private route to an exit that broadcasts it, as tokens go. Have app decks use the `LEAD_PULL` path the door tabs already use, so a deck needs no endpoint of its own on any relay.

### A-3 [High] Phones and computers answer probes about what they played
Who: listener
Where: node/src/lib.rs:603-606, 612, 614-616, 620; node/src/chat.rs:270-279; playback keeps chunks lib.rs:1063-1064, 3922, 3966; provisions lib.rs:1718-1762
What happens: A peer dials a listener's node and asks `SealedChunk { name }` for chunks of station S that fall outside the node's ballast share, which anyone can compute from the node id. On Wi-Fi or a computer, a hit means the node played or warmed S when that chunk aired. Asking for chunks scheduled in the next six hours finds the provisioned kept stations, which XII says never leave the phone. `Request::Log` answers for every log held, which is decisive on "Keep only what I play" phones and phones never on a charger. `Request::Chatter` returns lines for every room the node sits in, and `Request::Chunk` returns covers from art_mem, the stations browsed lately. C19 closed all of this for tabs; the node never got the same rule.
Evidence: Verified by reading. serve() has no cfg.private branch for Log, SealedChunk or Chatter, and played chunks are fetched with keep set.
Fix: On cfg.private nodes, answer SealedChunk only for names in `store.ballast`, Log only for pinned stations, Chatter only to that room's gossip neighbours, and clear Chunk not at all. What a listener played is then served through its deck, three hops away.

### A-4 [High] Lanterns and every player build publish listeners' home IPs
Who: listener, operator, HOSTIS
Where: node/src/web.rs:204-214, 233, 567-571; node/src/net.rs:690-694, 726-732, 795; node/src/door.rs:303-306; browser/build.py:95-104
What happens: /doors.txt, unauthenticated, returns the 16 freshest door notes the Lantern has heard, and /listen/ injects 8 into a meta tag. Nothing filters `app: true`, so listeners' app doors (home IP, port and persistent fingerprint) go to any web visitor. A deploy runs `curl <lantern>/doors.txt > browser/doors.txt`, and build.py bakes up to 16 into every build, which is pinned to IPFS and seeded over BitTorrent. Listener IPs become permanent, mirrored records published by HOSTIS. The Cards answer from doors and nodes hands out the same 16.
Evidence: Verified by reading. doors_hex and Notes::doors take every checked door note, and build.py checks only that each line is hex.
Fix: Serve and bake only notes with `app == false` in doors_hex, /doors.txt, the Cards answer and build.py (decode and assert there). App doors stay in gossip, where they expire in 15 minutes.

### A-5 [High] One browser can exhaust a door's memory
Who: operator
Where: node/src/door.rs:303-306, 417-421, 552-565; node/src/lib.rs:628-655
What happens: A tab needs no hello to send requests. It sends thousands of 6-byte `Request::Cards` frames. Each is answered in its own task from the cached card list (up to 1 MiB plus door notes), the per-tab ask count drops as soon as the answer is built, and `Cmd::Send` appends the frames to `s.out` with no bound. A home uplink drains a few MB a second, so a 60 KB upload queues gigabytes and kills the listener's app. Door addresses are public (A-4), so one script can take down every door.
Evidence: Verified by reading; not run.
Fix: Cap `s.out` per session (4 MiB, then disconnect), free an ask's slot only when its answer has drained, require the hello before any request, and allow one Cards answer per session every ten seconds.

### A-6 [Medium] The router mapping outlives the door and follows the device
Who: operator
Where: node/src/portmap.rs:25-36, 95, 99; node/src/door.rs:289-294; node/src/lib.rs:3541-3550
What happens: map_udp spawns a renewal loop with no exit. door_close never stops it, and nothing in node/src deletes a mapping. After the owner unplugs, turns carrying off or quits, the port stays forwarded to a closed socket (so a probe now sees the home online) for the life of the process, and up to an hour after a quit or crash. Each reopen adds another loop. On a phone that leaves home, every loop asks each new network's router every 30 minutes for a mapping described "Pyrate Radio door", which any host on that LAN can list and which ISP-managed routers report upstream. A door switched off while its mapping is still being made opens and announces anyway.
Evidence: Verified by reading. The loop at portmap.rs:29 has no stop, and no delete call exists.
Fix: Return a handle from map_udp. door_close and shutdown stop renewal and delete the mapping (IGD DeletePortMapping, NAT-PMP and PCP with lifetime 0). Renew only while the gateway is the one mapped, use an empty description, and check the switch again once the mapping returns.

### A-7 [Medium] Guards change whenever the guard's own id rotates
Who: listener, broadcaster
Where: node/src/hop.rs:266-293 (fetch guard), 1029-1050 (stem guard); node/src/lib.rs:2323-2331
What happens: A held guard is kept only while `known.contains(&g)`. App nodes take a fresh id at every start, or daily if they keep a station, so an app guard is gone within a day and a new one is drawn; only Lantern guards last. A few always-on, fixed-id nodes that carry everything win a share of these redraws, and each redraw is another chance to hold a listener's guard while also holding its exit, which is what B5 was meant to stop. The stem guard has no newcomer rule and falls back to `known.first()`, which may be a well-known Lantern, against its own comment.
Evidence: Verified by reading.
Fix: Draw both guards only from radios seen under the same id for at least a week (keep first_seen in the sealed peers note), apply `seasoned` to the stem guard, and drop the `known.first()` fallback.

### A-8 [Medium] Keeping the node id across restarts marks keepers and pocket members
Who: broadcaster, listener
Where: node/src/lib.rs:2323-2331
What happens: A node with no station and no pocket gets a fresh id at every start. One holding a station key or a pocket keeps its id for 24 hours. The guard, saved peers, gossip neighbours and the relay all see a device reconnect after a restart, and the same id coming back means it keeps a station or sits in a pocket. That narrows A-1's search to a few devices, and passive parties see it without sending a probe.
Evidence: Verified by reading.
Fix: One rule for every non-Lantern node: 24 hours from creation, restarts included.

### A-9 [Low] A chatter voice stays the same for a station across days
Who: listener
Where: node/src/lib.rs:2293-2305; core/src/chat.rs:130-135; node/src/chat.rs:191-210
What happens: The voice secret is made once and never rotated, and each room's voice comes from it and the station. Lines leave the speaker's own endpoint at round 0. Anyone in or lurking on a public room (its topic comes from the station id) ties today's node id and IP to yesterday's under the same voice. That undoes the daily id rotation for anyone who chats, and a hail signed by the same voice links to that speaker's lines.
Evidence: Verified by reading.
Fix: Derive the voice per station per week by default, offer "keep my voice here" as a choice, and say so at the join notice.

### A-10 [Low] Door slots and STUN answer anyone
Who: operator
Where: node/src/door.rs:153-198, 371-380, 552-566
What happens: A door takes 64 sessions with no per-address cap. One host opening 8 a second fills them and holds them while its channels live, shutting real tabs out. Sessions with no hello can still use the door as a first hop. The port answers plain STUN from anyone without limit (a 1.6x reflector), and a `pyrate+door+v1/` binding request confirms the radio at a known IP and port after its note has expired.
Evidence: Verified by reading.
Fix: Four sessions per source IP, a hello before any request, and a per-source rate on STUN answers.

### A-11 [Low] number0 is contacted by default on first run and by silent fallback
Who: listener, operator
Where: node/src/lib.rs:1253-1258, 2404-2425, 2448, 2489-2505
What happens: Until a first door's harbour note fills the "relays" note, the relay map is number0's map plus relay.hostis.black. iroh's net report then probes number0's relays from the device's IP and may home there, taking the A-2 deck with it. Afterwards, number0's relays are inserted without a word whenever ours fail to answer within 15 seconds. Apps not running dark also resolve their first doors through number0's pkarr server over HTTPS.
Evidence: The relay map is Verified by reading. Whether the pkarr lookup fires in practice is Suspected, since iroh decides when to consult lookups.
Fix: Start with CREW_RELAYS alone, put number0 behind a switch, and tell the owner when it stands in.

## Checked and sound

- Through Tor: relay dials use HTTP CONNECT by hostname, and HTTPS and captive-portal probes go through the proxy (iroh-relay client/tls.rs:163-175; iroh net_report/reportgen.rs:575-610). Dark nodes skip pkarr, LAN beacons, decks and carrier notes. The door's portmap while dark is the known finding, being fixed in the working tree.
- Strict mode: logs, chunks, own-station sync, tokens and stems never go direct or one hop without direct-ok (lib.rs:905-907, 1052-1058, 1075-1083; hop.rs:418-421, 1110-1157).
- The node contacts no NTP, GitHub, IPFS gateway or public STUN server. The clock comes from station cards. With --public-dns, which the Android app always sets, names are looked up at Cloudflare (1.1.1.1, 1.0.0.1) and Quad9 (9.9.9.9, 149.112.112.112) over DNS over TLS on port 853, with no plain-UDP fallback.
- Lantern pkarr records carry relay addresses only (AddrFilter), never IPs.
- Counting tokens are keyed by a daily-stepped key, station and hour, and travel two hops; pocket tokens stay on the pocket topic. Nothing links them across hours.
- Apps announce only hash-chosen ballast, and only on Wi-Fi while charging; dark nodes announce nothing.
- Ballast log sync asks for every carried station alike and skips the device's own.
- Dial-neighbour warming goes by private routes, skips pockets, and runs at most every ten minutes per station.
- Three-hop routes: the middle excludes both guards and the way in, every layer has a fresh ephemeral X25519 key (route.rs:30-39), a deck answers only its way in with the switch on (route.rs:118-120), and a door tab's deck pulls by cookie.
- Pockets have no deck and no public stem; log and chatter are asked by tag; card and tokens stay on the pocket topic. A stranger can derive neither topic nor tag.
- LAN beacons are signed, dated, local-only, off by default and never sent dark.
- Flock answers only the code holder.
- Portmap refuses private and CGNAT outside addresses before anything is announced.
- Door STUN and ICE parsing is bounds-checked, and frame reassembly is capped at 4 MiB and 16 open messages.
- Hails are sealed to station keys, and sealed again under the hold key when kept.
- Lanterns show their IP to peers by design, LANTERN.md says so, and the Lantern relay has per-client rate limits.
