# Pyrate Radio · Ship's log

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
