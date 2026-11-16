# Introduced doors and duty by need (0.9.19)

Owner's ruling (15 November 2026): listeners should carry the network, as far as possible by themselves; Lanterns, servers and dedicated phones are extra help. Privacy is strict: no listener, operator or broadcaster may become a standing target. Read DECISIONS LI, LII and LIII and audit-notes/2026-11-15/ first.

## 1. Introduced doors

Today a door gossips a signed note carrying its public address, and tabs dial it straight from the note (WebRTC-direct). Replace that for listeners' doors:

- An app door gossips a presence note with no address: its node id, the time, its load (tabs served now and its limit) and a flag for whether it can map a port. Signed, short-lived (15 minutes), never published by Lanterns, never baked into builds.
- A tab already on the network asks for a way in with a new sealed request carried through its private route (guard, then a middle). The middle, never the tab, draws a door at random from the presence notes it holds, weighted towards light load, and forwards the tab's WebRTC offer (ufrag, pwd, fingerprint, candidates) to it sealed. The door answers the same way with its own (host and server-reflexive candidates, its fingerprint), runs full ICE from its side so it can punch through NAT with no router mapping, and then serves the tab exactly as a door serves today (guard and relay for its sealed hops, cards and gossip, nothing else).
- A door takes at most a set number of introductions an hour (start at 12) and refuses the rest; a middle forwards at most a few introductions a minute per previous hop. Enumerating doors then costs many slow, rate-limited requests, each through a fresh route.
- A tab uses an introduced door as its guard only under the existing rules (rule A, seasoning); otherwise as a relay for its hops.
- Doors with a listable address keep publishing as now: doors with `app: false` (Lanterns, servers) and app doors whose mapped address passed the VPN rule and differs from any home address the node can see (the VPN case). Link doors stay as now (owner's switch).
- WebRTC-direct from a note stays for listed doors only.

## 2. Duty by need

Each listener's node decides, every few minutes, which jobs to take within its limits (computer, or phone on Wi-Fi while charging; not on Tor; not "keep only what I play"; owner's door switch not off):

- Door duty: open when the known doors' average load is above half, or fewer than a target number of doors are known for the tabs the network counts; close when the average load stays under a tenth for 30 minutes and enough other doors are known. A spell of door duty lasts at most 6 hours, then the node rests for at least as long and its door identity (certificate, port) is renewed before the next spell. Jitter every threshold so nodes do not open and close together.
- Hop duty (middle and way in) follows the same pattern from the load of known middles.
- Ballast already follows the fleet size; leave it.
- The app shows the jobs this device is doing now in one plain line, and the existing one-time notice still appears the first time a door opens.

## 3. Tests

- Unit: presence notes carry no address and are refused when they do; a middle never forwards an introduction to a door the tab named; rate limits hold; duty opens under load and closes when idle, with jitter.
- Network: a test with two app nodes (one door-capable, no listed doors at all) and a headless tab reaching the network through the test relay: the tab is introduced, opens a direct WebRTC link to the app door and plays; the relay can then be stopped and the tab keeps playing through the door. Use tools/test/browser_rtc.py as the model.
- privacy_test: no gossip message ever carries an app door's address when the VPN rule fails.
