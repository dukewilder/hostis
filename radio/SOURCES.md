# Where the source lives

The code is the network's only master. No server has to stay up for Pyrate Radio to run, and no single host has to stay up for the source to be found. This file is the roster of every place the whole source tree lives, so the code outlives any one of them. Keep it true as hosts come and go.

Today the source is on GitHub. The rest of the list is set down now as places to fill, so that moving house later is filling a line rather than inventing a plan. A second git forge guards against one account being shut. Radicle puts the repo on a peer network with no host at all. IPFS gives every file a name that is its own content, so a copy anywhere is the copy. A torrent of the source tarball spreads the whole tree to anyone who seeds it.

Each deploy already publishes, under `hostis.black/pyrateradio/`, a source tarball and its torrent at `source/`, every published file by its IPFS name at `ipfs/files.txt`, and an unsigned draft of the bearings for the release keys to sign. This file is the human roster above those; the record below is the part a release key signs.

## The record

A release key signs the block below, byte for byte, so a reader can trust the roster the way they trust a release. Keep the block in this exact shape: one place per line, the kind first, then its address, then a branch or name where one is needed. Fill a placeholder only when the place is real and reachable; leave it as written until then.

```
pyrate-sources v1
git       https://github.com/dukewilder/hostis            radio
mirror    <a second git forge, to be filled>              radio
radicle   <rad: URN, to be filled>
ipfs      <IPNS name or source-tree CID, to be filled>
torrent   <magnet of a released source tarball, to be filled>
```

Notes on the lines, for whoever fills them.

- **git** is the home today. The `radio` branch holds the radio under its `radio/` folder. Clone it as in `LANTERN.md`.
- **mirror** is a second git host, on a different account and company from the first, pushed the same commit. One shutdown should never take the source with it.
- **radicle** is the repo's Radicle URN, once it is seeded, so the source has a home with no host to shut.
- **ipfs** is a name under which the whole source tree is pinned. It is not the open player's IPNS in `browser/ipns.txt`, which names the player, not the source.
- **torrent** is the magnet of a source tarball cut at a known release. Each deploy also writes a per-commit magnet at `pyrateradio/source/magnet.txt`; this line names one that was released, so the signature points at a fixed tree.

When a line changes, change it here, raise the signature again, and the roster is sound once more.
