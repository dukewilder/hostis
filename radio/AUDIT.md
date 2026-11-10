# Pyrate Radio · The audit

Started 10 November 2026 from the code on `radio` at `736a0d1`. Two tracks: the product (does every feature work, and work well) and the walls (what a listener, a node runner or a broadcaster could be made to give away). Every finding lands here when it is found, and moves to Fixed in the same commit as its fix.

Each entry gives the threat, whether the wall held, and the fix.

## Open

### Track B: privacy, anonymity and decentralisation

**B1. The stem names its origin.** `hop.rs` starts every stem at `hops: 0` and each relay adds one, so the guard that receives `hops: 0` knows its peer is the broadcaster. Does not hold. Fix: the count carries nothing (every node sends 0), and loops end by the fluff draw and a memory of cargo already relayed.

**B2. Pocket stations surface on the public Log.** Three ways: a listener's hourly token for a pocket station is gossiped on the public Log by whoever counts it (phones through their exit, tabs directly), naming the station; a pocket station that goes ON AIR sends its live entries down the public stem, where a stranger takes them aboard, announces itself as a carrier and serves the log by id; and a keeper's phone syncing its own pocket asks an exit for it by id. Does not hold. Fix: pocket tokens travel sealed on the pocket's own topic; pocket stations never use the stem; a keeper syncs a pocket by its tag.

**B3. Pocket chatter is open.** A pocket's room sits on a topic drawn from the station's id and its lines travel in the clear, so anyone who learns the id can read the room. Does not hold. Fix: a pocket's room uses a topic drawn from its key and every line is sealed with the key.

**B4. Ballast is announced and served away from the charger.** A phone keeps announcing its ballast stations and serving sealed chunks to strangers on mobile data, which spends the owner's data, and its ballast set (fixed by its steady seed) ties each day's fresh id to the last and each network it uses to the next. Holds only in part. Fix: ballast is announced and served only while the phone is on Wi-Fi and charging; on mobile data a phone serves strangers nothing.

**B5. Private fetches have no guard.** Each private fetch draws a fresh first hop, so over a day of listening a hostile pair of nodes (first hop and exit) will eventually both be drawn and can tie a listener to a station. Holds only in part. Fix: a long-lived first hop for fetches, as the stem already has, and as Tor's entry guards do.

**B6. Stems and ballast ignore the listener's own switches.** A stem can end on a phone for a station its owner marooned, and ballast and stems take aboard explicit stations on a phone whose owner keeps them hidden. Does not hold for the owner's choice. Fix: a phone declines stems and ballast for stations it maroons, and for explicit stations while it hides them. Lanterns still carry the whole Log by default.

**B7. Old plain chunks answer to a sealed request.** A chunk kept from before sealing lies on disk under its plain hash, and a `SealedChunk` request naming that hash is served the plain bytes. Does not hold for those files. Fix: a sealed request that finds a plain file seals it in place and answers nothing.

**B8. The hold opens for anyone with the public Log.** Chunks and logs are sealed under keys drawn from public facts (a station's id, a chunk's hash), and filed under names drawn from the same. Someone who seizes a disk and also runs a node can test it for every station on the Log, and a deleted file on flash may still be read. Holds against a casual look only. Fix: a hold key of the device's own, under which every file is sealed again and named, so the disk is noise without it and Clear the hold destroys the key.

**B9. The PIN falls to a fast hash.** The Colours PIN is stored as a salted BLAKE3 hash, so a seized disk gives up a four-digit PIN at once, and with it any lock-screen PIN that matches. Holds only against a casual look. Fix: a memory-hard hash (Argon2id).

**B10. A malformed id can stop a node.** `StationId::from_hex`, `unhex` and the browser's `unhex32` slice strings by byte, so 64 bytes holding a multi-byte character panic. The FFI and the browser are reached by pasted links. Fix: parse by bytes, never by string slices.

**B11. A struck station's art stays on disk.** Strike drops a station's recordings but not its pictures. Fix: drop the art with them.

**B12. A Lantern's chatter memory never empties.** Each room keeps every line id it has seen for the life of the process. Fix: forget ids older than the window in which a line is accepted at all.

**B13. One company's DNS sits under the network.** Nodes publish their id and relay to number0's pkarr server and look carriers up there, and every lookup also goes out as a plain DNS query that the line's provider can read. If that server goes, nodes can find each other only through gossip neighbours; while it stands, it and the line see who dials whom. Holds only in part. Fix: no plain DNS lookups of node ids, and each node's relay address carried inside the network, signed by the node, so nodes find each other without number0.

**B14. A broadcaster asks carriers for its own station directly.** When the private route fails, a phone syncing its own station asks carriers for the log by id from its own address, which tells them who keeps it. Does not hold. Fix: a keeper's phone never asks directly for its own station unless that station is already on the air directly from the phone.

**B15. Small faults.** The flock's round trip can underflow if the network clock steps between pings.
