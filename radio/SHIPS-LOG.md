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

On the phone the node starts, joins the Log and finds the PC Lantern. Sound out of the phone's speaker is the one thing not yet heard: the phone was locked all night and Android freezes a locked app.

**On the air**

- The Public Domain (`GAHAGU·3695`): 1925-and-earlier recordings from Wikimedia Commons, public-domain only, source line on every track.
- Readings (`DEVAKA·F42C`): LibriVox. Johnson's General History of the Pyrates, Treasure Island, and one children's book the search pulled in.

**Open**

1. Until a HOSTIS Lantern runs on a public server, the network leans on number0's relays and DNS for finding peers. A small VPS running a Lantern with its own relay ends that, and it is also what the web listener and iPhones need.
2. On Android the node asks 1.1.1.1 and 9.9.9.9 for DNS, because the system resolver needs a Java context the node does not yet pass to iroh's resolver.
3. Release signing: the key stays with Dylan. Builds are signed with the debug key until then.
4. Not yet built: web listener, Hail, Tape it, Local filter, Pass it on, F-Droid repository, reproducible builds, export notice, security review.
