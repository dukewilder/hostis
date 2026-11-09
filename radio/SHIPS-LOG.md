# Pyrate Radio · Ship's log

## Night of 8 to 9 November 2026

**Built and tested**

| Piece | Where | State |
| --- | --- | --- |
| Transport | `spike/` | iroh confirmed on a phone on T-Mobile: direct in 5 s, relay where blocked, no gaps |
| Core | `core/` | Keys and 24-word export, call signs, signed hash-linked log, Opus chunks, file ingest. 16 tests |
| The clock | `core/src/clock.rs` | Same answer on every node to the millisecond. Weekly rounds, blocks, ident on the bell |
| Node | `node/` | Store with quota, wire protocol, the Log over gossip, proof of work, aboard sketches, playout, ON AIR |
| Lantern | `node/src/bin/lantern.rs` | Running on Dylan's PC. Carries every station and raises HOSTIS's own from folders, rescanning every ten minutes |
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

**The PC Lantern**

- Folder: `Documents\Pyrate Radio` on Dylan's PC. Data in `lantern-data`, audio in `stations\public-domain` and `stations\readings`.
- Node id `72b966c645197137b553995ff690349153c4219cc08278ebdde15dcf7bd29d72`, the app's first peer (`app/assets/bootstrap.txt`).
- Relight: `powershell -ExecutionPolicy Bypass -File light.ps1`. New files dropped in a station folder go on its clock within ten minutes.
- The PC is set never to sleep on mains power. If it does, the HOSTIS stations stay up only on phones that carry them.
