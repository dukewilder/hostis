# Governance

Pyrate Radio has no company behind it and no owner. The code is CC0, the network is the phones and Lanterns that run it, and no server has to stay up for any of it to work. Only a few things need trust: who may merge code and cut a release, the keys that sign releases, and the shape of the network a fork shares. This is how those are held and how they pass on.

## Maintainers

A maintainer is someone the others trust to review and merge code and to cut a release. You become one the plain way: send good work, keep sending it, and review other people's. When the maintainers agree you are carrying real weight, one of them adds you to the list in this file and gives you push.

The rules are few.

- A change lands when a maintainer who is not its author has read it and the suites pass. Nobody merges their own work unreviewed.
- A maintainer earns the name by the record, not by a vote on the first day.
- No one owns a corner of the tree. Any maintainer may review any change, and disagreement is settled by the argument, not by rank.
- A maintainer who goes quiet for six months comes off the list, with no hard feelings, and may come back the way they joined.
- Anyone may fork at any time. That right is the real check on a maintainer, so it is never taken away.

## The release keys

A build is installed only if two of the three release keys have signed it (see `RELEASES.md`). The three keys are held by three different people, in three different places, offline. No one person can ship a build, and the loss of any one key ships nothing on its own.

A signer is replaced when a holder leaves, a key is lost, or a machine is thought to be compromised. Two sound keys are enough to do it, which is the point of three.

1. The holders of the two sound keys agree on the new third holder.
2. The new holder makes a key offline, on a machine that has never been online, and reads out only its public half. The private half, the 24 words, never leaves that machine or its paper.
3. A maintainer puts the new public key in place of the old one in `release_keys.txt`, where the app reads the three it trusts.
4. The next app release carries the new `release_keys.txt` and is signed, as every release is, by two good keys. As phones take that release they learn the new three.

Until a phone updates it still trusts the old three, so a lost or stolen key is a danger until the new release has gone round. Treat a suspected key as burnt from the hour it is suspected: tell the other holders at once, bring out the replacement release, and never sign with the suspect key again.

The Android signing key is a single key, not one of the three. It only lets Android accept each release as an update of the last, so it is replaced the way Android asks, by a clean reinstall, and it is kept as carefully as a release key all the same.

## Forks

Anyone may fork, and owes nothing. To stay compatible, so your fork's stations, links and listeners still meet the main network, keep the parts that two nodes have to agree on and change the rest freely.

- Keep the wire formats and the station Log as they are. These are the handshake between nodes. Change them and your fork drifts onto a network of its own.
- Keep the bootstrap doors and relays, or add your own beside them, so your nodes still find the shared network.
- Use your own release keys and your own Android signing key. Never ship under the main network's keys. Your builds are yours to vouch for, not ours.
- A HOSTIS name belongs to HOSTIS. Serve the open player under a name of your own, never a HOSTIS one, as the Dockerfile and `LANTERN.md` already ask.

If you mean to change the protocol rather than keep it, change the network's own id too, so the two networks do not try to talk and confuse each other. A clean break is friendlier than a half one.

## Maintainers now

The list lives here and changes by the rules above.

- Duke Wilder, founder. See `HANDOVER.md` for how everything here is done without the founder, which is the state this project is built to survive.
