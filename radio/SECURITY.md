# Security policy for Pyrate Radio

## Reporting a problem

Report security problems privately, through GitHub's private vulnerability reporting on this repository: the **Security** tab, then **Report a vulnerability**. Please do not open a public issue for anything that could hurt listeners, broadcasters or nodes before it is fixed.

Say which version you tested (the app's version line, or the commit), what an attacker needs, what happens, and how to repeat it. A test that shows it is the most useful thing you can send. The attack harness in `node/src/bin/pentest.rs` is a good place to start one.

Reports are read within a week. You will be told what was found, what will change, and when. Credit is given in the release notes unless you ask for none.

## Supported versions

Only the latest release is supported. Security fixes go into a new release, published at hostis.black/pyrateradio and carried to phones over the network, signed by two of three release keys. Nodes and Lanterns on older versions should be updated; older versions are not patched.

## What is in scope

Everything in this directory: the Rust core, node, Lantern and browser player, the Android and desktop apps, the web player, and the release and signing tools. Out of scope: hostis.black itself (report to its maintainer), third-party relays and gateways, and attacks that need the victim's unlocked device.

## How the project tests itself

- `node/src/bin/pentest.rs` turns malicious peers on a real node in a separate process and prints PASS or FAIL with what it measured. Run `PYRATE_LOG_MB=64 PYRATE_LAN_ANY=1 target/release/pentest <empty dir> <any wav> all`.
- `node/src/bin/privacy_test.rs` checks what each node asks for, and of whom, when a phone listens or broadcasts.
- `core/tests/` holds the limit, nesting and random-log tests, and unit tests run with `cargo test`.
- The audit records are AUDIT-2026-11b.md, AUDIT-2026-11c.md and AUDIT-2026-11d.md.

## What the design does not promise

The guarantees are mode by mode, and the how-it-works page sets them out. In short:

- **Two hops.** Two hops keep any one node from learning both who asks and what for. Someone who runs both hops of a route can match the two ends.
- **Station identities.** Many station keys are not proof of many people.
- **Unlocked devices.** A device that is unlocked and carries malware can be read whatever the app does.
- **Invite-only stations.** Members of an invite-only station can see each other's devices.
- **Chat.** Joining a station's chat shows the others there that your device listens to it.
- **Direct and MP3 listening.** The web player's relay and a Lantern's direct MP3 stream see a listener's internet address.

## When something goes wrong

- **A flaw in a release.** Fix it, raise the version code, sign the build with two of the three release keys and put it in the Lantern's `releases/` folder (RELEASES.md). Phones offer it within half an hour and take it three days after first hearing of it, or at once if the owner taps. For anything urgent, say so on hostis.black and ask people to tap. Phones never take a lower version code, and Android refuses a downgrade under the same signature.
- **One release key lost or exposed.** One key cannot publish anything. Keep signing with the other two and plan new keys for a later build.
- **Two release keys exposed.** Network updates can no longer be trusted. Publish a build with new release keys on the site for installing by hand, say why on the site, and let the canary lapse if anyone has ordered silence.
- **The Android signing key exposed.** A copy of the app signed with it would install over a real one by hand. It still cannot travel as an update without two release keys. Warn on the site, and only ever install from hostis.black, IPFS or the torrent with the checksums beside them.
- **A Lantern taken or broken into.** Take it off the network. Its keys seal only its own disk and name only itself. The Releases station's deputy key on it cannot publish a release without two release keys; revoke that deputy with the Releases station's words and appoint a new one on the replacement.
- **A station's words or deputy taken.** Only the words can revoke a deputy or strike the station. Type the words into another device, revoke the lost deputy and appoint a new one.
