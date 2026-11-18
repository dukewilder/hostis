# B. Browser player and web pages: privacy, security and UX

Commit 327d6c5, read only. Live pages checked 15 November 2026 with curl and headless Chrome at 390 px (this sandbox passes HTTPS and WebSockets, not UDP).

## Privacy and security

### B-1 [Critical] One unsigned list re-points the open player's fixed address
Who: listener, broadcaster
Where: tools/pin_player.sh:11, 43-44; tools/deploy_site.sh:35-41; browser/www/app.js:96-133
What happens: Anyone who can push to hostis.black's Pages repo (a stolen token, a seized account or domain) appends a CID and its CAR to /pyrateradio/ipfs/players.txt. Within the hour HOSTIS's pin_player.sh publishes it under the fixed IPNS name, the first address in every share link, the landing, the app and the iPhone steps. Every returning open-player visitor then runs that page at the origin that holds their sealed deputy keys, pocket invitations, kept list and counting key, and page code opens all of them through the non-extractable key (dopen). The release keys never enter this path, and inbrowser.link's gateway script, served unchecked from Cloudflare Pages, runs at that origin too.
Evidence: Verified. pin_player.sh publishes `tail -1` of a list fetched over HTTPS with no signature; the live list ends with the landing's CID and no signature file exists (players.sig 404); the gateway's error page names a single provider for the name (<address withheld until checked>, HOSTIS's node).
Fix: Have two of the three release keys sign players.txt (the release message with the CID in place of the SHA) and make pin_player.sh verify before `ipfs name publish`.

### B-2 [High] HOSTIS's own machines fill both hops and the way in on a fresh visit
Who: listener
Where: browser/src/lib.rs:2717-2731 (choose_intro) and route drawing; app/assets/bootstrap.txt
What happens: A new tab knows only the two bootstrap harbours, both HOSTIS Lanterns homed on relay.hostis.black. On hostis.black the first log fetches went "via a324c679b0 to f6d5c5276a": guard and exit both HOSTIS, with the tab's address on HOSTIS's relay. The open player boots from the same harbours. A playing tab's deck picks a harbour as its way in when no other crew radio is known; the way in learns which node serves which station and HOSTIS's relay maps the node to an address. HOSTIS, or whoever seizes HOSTIS's node, sees who listens to what.
Evidence: Verified live with ?debug ("route log from 0 via a324c679b0 to f6d5c5276a (2 known)"; both ids are in bootstrap.txt and both "reached through https://relay.hostis.black/"). The way-in fallback is verified by reading.
Fix: Count the harbours as one operator: never draw two roles on one route from bootstrap.txt, and wait for another radio as XXXVIII already does.

### B-3 [High] Any peer can make a tab reveal its real address over WebRTC
Who: listener
Where: browser/src/lib.rs:2402-2411; browser/www/app.js:301-311, 359-377, 414-437
What happens: With Carry the radio on (the default), on_hop answers a Hop::Rtc offer from any sender. The tab builds a peer connection and runs ICE checks toward candidates the sender chose, so the sender receives UDP straight from the tab's real address. A tab's id lasts one visit, so it is never a kept guard and only an attacker sends this. Doors leak the same way: a link's `~door` hint is dialled before the door's note is checked, so a crafted share link logs the address of every opener whose relays have been quiet for 24 s, and a censor who blocks the relays makes that every opener. UDP skips browser proxies, extension VPNs and split tunnels, and nothing in the player turns WebRTC off.
Evidence: Verified by reading (nothing checks that `from` keeps this tab as guard; door_up checks the note only after ICE to the link's address). Not run live.
Fix: Tabs refuse Hop::Rtc. Dial a link's door only when a second source names it. Add a direct-links switch under Advanced and say on how-it-works that direct links bypass a browser's proxy.

### B-4 [High] number0, Cloudflare and GitHub see every browser listener
Who: listener, HOSTIS
Where: browser/src/lib.rs:45, 61, 3268-3276; app/assets/bootstrap.txt; relay.hostis.black
What happens: At every start a tab asks number0's dns.iroh.link for both harbour ids (in the URL path, so number0 learns the address runs Pyrate Radio), pings number0's four relays and may home on one. relay.hostis.black sits behind Cloudflare's proxy, so Cloudflare terminates TLS on the relay WebSocket and sees each tab's address and the node ids its frames go to. inbrowser.link is Cloudflare too, GitHub and Fastly serve hostis.black, and loading the open player connects each visitor to HOSTIS's node, its only IPFS provider. The pages name none of them; HARDENING-PLAN 1.17 was closed as moot when the relay had no DNS record.
Evidence: Verified live: the trace shows dns.iroh.link/pkarr/<id> twice, pings to four relay.n0.iroh.link hosts and wss to relay.hostis.black; curl shows `server: cloudflare` and `cf-ray` in front of iroh-relay's /ping and /relay.
Fix: Grey-cloud relay.hostis.black, as LANTERN.md already advises. Write bootstrap.txt entries as `id@https://relay.hostis.black` so tabs skip the lookup. Give number0's relays the fifteen-second standby XXIX gives the app. Name these parties on how-it-works.

### B-5 [High] The installed player does not enforce two of three keys
Who: listener
Where: browser/www/sw.js:7, 25-42, 78; browser/www/app.js:2524
What happens: The rule covers the files sw.js caches and not sw.js itself. Browsers refetch sw.js from the origin on navigation, at most a day apart, and the worker calls skipWaiting, so a seized hostis.black or hostile mirror ships its own sw.js and takes returning visitors at once. Live it protects nothing: hostis.black has no player.sigs, so the worker never keeps a copy and every load comes from the network; on inbrowser.link registration fails because the gateway answers sw.js with text/html. On ipfs.io the SHARED guard skips the worker: Cache Storage is per origin, so any other page on ipfs.io can write `pyrate-meta` and a cache that the player's worker then serves at the genuine CID path.
Evidence: Verified in code, by curl (player.sigs 404; inbrowser.link sw.js 200 text/html) and headless (worker registered, only `pyrate-meta` cached). ipfs.io poisoning by reading; ipfs.io sent this sandbox a Cloudflare challenge.
Fix: Say in LII and on how-it-works that whoever serves an origin can replace its worker. Register only on content-addressed origins (CID subdomains) and never when SHARED. Ship player.sigs with each deploy or drop the promise.

### B-6 [Medium] The counting key is a 30-day listening history and a copied profile opens it
Who: listener
Where: browser/src/lib.rs:594, 1628-1631; browser/www/app.js:90-133, 2506-2510
What happens: Each hour's token is BLAKE3(token key, station, hour), gossiped raw. Whoever logs gossip and later gets the key recomputes and matches every station and hour of the last 30 days; the same key derives the listener's chat voice in every room. The phone steps its key forward daily, while the tab keeps one for 30 days. The key is sealed under a non-extractable AES key, but browsers store CryptoKey bytes in the profile's IndexedDB files, so a copied laptop profile or backup opens the token, kept list, invitations and deputy keys. app.js:91 says a copy of the profile's files finds only sealed text.
Evidence: Derivation verified in code. Key bytes in IndexedDB is documented Chromium and Firefox behaviour, not reproduced here.
Fix: Step the key forward each day as the phone does and keep only today's. Derive the chat voice from a separate secret. Correct the comment and the page.

### B-7 [Medium] Station names reach the OS and browser history even for invite-only stations
Who: listener
Where: browser/www/app.js:997-1001, 1154
What happens: The tab title becomes "▶︎ <station> · Pyrate Radio" and Media Session sends title, artist, album and cover to the lock screen, Bluetooth, cars, the Windows flyout and tab-reading extensions. Browsers keep titles in history and sync them, undoing app.js:1098's care with the URL. Neither path skips pockets, and the browser lacks the app's plain disguise.
Evidence: Title verified live ("▶︎ Netlabel · Pyrate Radio"). Pocket handling and Media Session verified by reading. History storage of titles varies by browser (Suspected).
Fix: Keep the title "Pyrate Radio". Send pockets to Media Session as "Radio" with no art. Offer the app's plain disguise as a switch.

### B-8 [Medium] hostis.black's copy carries strangers' stations including explicit ones
Who: listener, HOSTIS
Where: browser/src/lib.rs:2875-2903, 3809-3818; browser/www/app.js:168, 186-188
What happens: On a computer with Carry on, pick_ballast draws two of the least-carried stations on the whole Log each hour, ignoring the page's scope and the explicit switch, keeps up to 500 MB of them in hostis.black's IndexedDB and serves them on. A visitor to HOSTIS's own page stores and passes on a stranger's explicit or infringing station, against XXV and XIX. The store is keyed by seal::name(chunk), which anyone with the public log can compute, so the disk does show which stations were carried, contrary to app.js:186.
Evidence: Verified by reading; pick_ballast has no scope or explicit filter.
Fix: In the harbour and embed scopes draw ballast from HARBOUR_STATIONS only. Skip explicit stations unless the listener shows them. Key the store by HMAC under the device key.

### B-9 [Medium] Copy promises more than the browser keeps
Who: listener, HOSTIS
Where: hostis.black/pyrateradio/ (The Promises), /how-it-works/, /handbook/; browser/body.html:141, 277
What happens: "Not even HOSTIS ... can see who listens" and "No single radio on the way learns both who you are and what you hear" fail on first visits (B-2). "A browser tab ... reaches the network through a relay" leaves out doors and guard links (B-3). "Copies of the open player that the visitor's own browser checks" leaves out that the checker comes unverified from inbrowser.link (B-1). "An update needs two of three keys" holds for the app alone (B-5). "No one can tell who hears what" (crew note) and "Nobody Can Switch It Off" are absolutes while the fixed address rests on one provider. The handbook and body.html:141 send listeners to Tor Browser, which disables service workers, so the checked open player and every invitation cannot open there.
Evidence: Verified against live page text. The Tor Browser part is Suspected (not run there).
Fix: Scope each line ("while the network is young, HOSTIS's first doors may carry both hops"; "copies a gateway checks"; "the app's updates"), add direct links to the browser paragraph, and give Tor users the onion address and the Standard level.

### B-10 [Low] Some of what the browser keeps is unsealed and Forget leaves the worker
Who: listener
Where: browser/www/app.js:438, 448-449, 2534, 1642-1649
What happens: Hidden stations, chat name and mutes, door notes (operators' home addresses), guards and peers sit in plain localStorage; XXXVIII says the browser seals what it keeps. Forget this browser leaves the service worker and Cache Storage with `pyrate-meta` and its timestamp, then reloads and writes a fresh token and harbour list at once.
Evidence: Verified by reading and live (localStorage keys and `pyrate-meta` after one visit).
Fix: Seal maroon, name, mutes and doors with keepSealed. Make Forget unregister the worker, delete the caches and stay stopped.

## UX and product

### B-UX-1 [High] "Every station" leads to one fragile address and its fallback never fires
Where: landing/index.html:197, 325; browser/www/app.js:32-36; browser/mirrors.txt
What happens: Every way to the full player goes to the IPNS name on inbrowser.link, and so do the iPhone steps. The availability ping fetches favicon.png, and inbrowser.link answers any path with 200 and its own HTML, so the fallback to other copies never runs. The name had one provider, and here the visitor got "SERVICE WORKER GATEWAY · 502 Bad Gateway ... providers ... failed to send the data", a stranger's error page in place of the radio. mirrors.json is empty.
Evidence: Verified by curl (favicon.png and an invented path both 200 text/html) and headless Chrome (the 502 page); without UDP here, real browsers may fare better.
Fix: Pin the player on a second provider and list a Lantern /listen/ mirror. Make the ping read a known file from a copy that serves raw bytes. Send the iPhone steps to hostis.black/pyrateradio/listen/ when the open player does not answer.

### B-UX-2 [High] The first seconds announce four different songs
Where: hostis.black/pyrateradio/listen/ (Netlabel)
What happens: In two runs the set showed Acidict (1:52 of 8:49), The Gosper Gun, Riveal (2:30 of 3:26) and riflessione (3:49 of 4:34) within 13 s of Listen, and a fifth title, 21-01-01, before it. Sound came in about 2 s, but the title will not hold.
Evidence: Title changes verified live, twice. That the sound jumps with them is Suspected.
Fix: Hold the first plan until the station's rota and library pages are aboard, or let the first track play out before the new plan takes over.

### B-UX-3 [Medium] The status and error line fails contrast
Where: browser/player.css:18 (.signal); .slot, .time and .field>span use --faint too
What happens: rgba(237,228,206,.34) on #030303 is 2.5:1 for 11 px capitals, and every error and tuning stage is written there.
Evidence: Verified by computing the ratio.
Fix: Use --soft (8:1) for .signal and errors and at least --dim for times and labels.

### B-UX-4 [Medium] Tor Browser listeners get a generic failure
Where: browser/www/app.js:2501
What happens: At the Safer level Tor Browser turns off WebAssembly, and the page says only "This browser cannot run the radio. Try another browser, or the app." The open player cannot load there at all.
Evidence: Suspected; not run in Tor Browser.
Fix: Detect Tor Browser, say to choose Standard for this site and link the onion copy.

### B-UX-5 [Low] "0 listening" on the first screen
Where: the set's ident line at 390 px
What happens: HOSTIS's flagship opens on "0 listening", which reads as an empty room.
Evidence: Verified live (screenshot).
Fix: Hide counts below one or show "Carried by N radios".

### B-UX-6 [Low] Small targets and motion
Where: dial names and the sleep buttons (26 px tall); smooth scrollIntoView calls; the VU needle
What happens: Targets under 44 px on phones; smooth scrolling and the meter ignore prefers-reduced-motion.
Evidence: Verified (measured; reduced-motion rules cover only needle, progress bar and one spinner).
Fix: 44 px minimum; instant scrolling and a still meter under reduced motion.

## Checked and sound
- The five pages load no third-party fonts, scripts, analytics or images; fonts are self-hosted or inlined (live).
- The player's CSP and no-referrer meta are live; outbound links carry noreferrer.
- Station ids, invitations and flocks stay in the fragment and leave the address bar before boot; foreign links leave hostis.black unread.
- hostis.black's copy lists and plays only HOSTIS's stations; no third-party art.
- Text sinks are escaped and tip links keep to payment schemes.
- Keys, invitations, kept list, token and ballast list are AES-GCM sealed, falling back to sessionStorage only.
- Path gateways keep nothing in localStorage or IndexedDB (SHARED), apart from the worker in B-5.
- build.py refuses public STUN servers, stun.txt is empty and there is no TURN.
- A link's door cannot become a guard on its own word, and a door's note must match the dialled fingerprint.
- open_player.py checks every CAR block against its CID; pin_player.sh checks files.txt hashes.
- Sound about 2 s after Listen on hostis.black; plain no-network and no-WebAssembly messages.
- Rows, dial and buttons are keyboard-reachable with visible focus, and the 390 px layout fits with no sideways scroll.
