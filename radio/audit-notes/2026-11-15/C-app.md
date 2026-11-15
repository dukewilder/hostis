# C-app: Pyrate Radio app, privacy, forensics and UX review

Repo /home/claude/hostis/radio, branch radio, commit 327d6c5. Read-only; nothing edited. Prior audits and HARDENING-PLAN skimmed; fixed items not repeated. "Verified" means confirmed by reading the cited code path end to end; nothing was run on a device.

## Privacy and security

### C-1 [High] A keeper's private voice message is decrypted to a plain file that desktop never deletes
Who: broadcaster (keeper), and the listener who sent the hail
Where: node/src/chat.rs:341 (`chat_hail_open`), app/lib/station.dart:1517-1519, app/lib/shell.dart:150 (`play_file`)
What happens: A listener sends a hail "sealed to the station" (the app says "Only the station can read it"). The keeper taps PLAY IT. The node writes the opened audio to `<node dir>/hail.m4a`, outside the sealed hold. On a computer `play_file` just calls `xdg-open`/`open`/`FileProtocolHandler`; nothing ever removes the file, and scuttle only destroys the hold key, so the plaintext voice survives a wipe and a disk image. The default media player may also add it to a recent-files list. On Android `Ship.playOnce` deletes on completion, but a crash or app kill mid-play leaves `files/node/hail.*` until the next scuttle-start.
Evidence: Verified by reading (write at chat.rs:341, no delete in the desktop branch of shell.dart).
Fix: On desktop write the hail into `scratchDir()/share`, and delete it when the keeper closes the sheet; on both platforms add `hail.*` to `sweepTemp` and to the scuttle clean-up (C-3). Better: play it in the node from memory and never write it.

### C-2 [High] The PIN hashes and setting flags sit in the clear in the node folder, so the duress PIN is detectable and crackable
Who: operator, broadcaster
Where: node/src/lib.rs:3304-3310 (`pin`, `pin-duress`), 3290 (`direct-ok`), 3298 (`provisions-off`), 3389 (`art-kept-only`)
What happens: A border officer or lab images the phone or computer. The files `pin` and `pin-duress` are written with `std::fs::write` in the node folder, not through `hold.write_note`. The image therefore shows that a duress PIN exists (by name), and both Argon2id hashes (32 MiB, t=3) fall to an offline run over the 10,000 four-digit PINs in minutes. The examiner then knows which PIN to demand and which to avoid, which defeats XXXII's "the time taken does not say which one matched". `direct-ok` also states that strict anonymity was switched off. On Android none of this is removed at scuttle; it waits for the next start (C-3).
Evidence: Verified by reading.
Fix: Store the PIN records and the flag files as hold notes (`write_note`/`read_note`), so they are noise once the key goes. Cheap extra: require 6 digits for the duress PIN, or bind the PIN check to a hold-derived secret so a hash alone cannot be tested offline.

### C-3 [High] Scuttle and the duress PIN leave unsealed temp files until the next launch (Android) or for ever (desktop)
Who: broadcaster, listener
Where: app/android/.../Hold.kt:79-95; app/lib/shell.dart:266-276; app/lib/more.dart:731-745
What happens: A keeper hits the duress PIN at a checkpoint. `Hold.scuttle` destroys the Keystore key and clears prefs, but only `clearIfScuttled` (run at the next launch) deletes `cacheDir`. Until then the cache still holds `share/<Station> - <Title>.ogg` (a tape of what the owner listened to, named by station), `share/<Station> poster.png`, `rec-*.m4a` voice takes, `voice-*.wav`, `file_picker/*` copies of audio and images, `hail.*`, and `files/node/updates/*.apk`. On desktop `_scuttle` deletes only `hold.key` and exits: the `scratch` folder (WAV recordings, tapes, shares, picker copies) and `hail.*` stay in the clear, and are emptied only by the next start's `deskTidy`. Nothing in the 6-hour `sweepTemp` helps a seizure.
Evidence: Verified by reading.
Fix: In `Hold.scuttle` call `ctx.cacheDir.deleteRecursively()` and delete `files/node/hail.*` before the key; in `Desktop._scuttle` run the `deskTidy` loop and delete `hail.*` first. Same two lines for the Wipe button, which uses the same path.

### C-4 [High] Podcast feeds, linked covers and banners bypass Through Tor and the node's routes
Who: broadcaster (keeper); HOSTIS (promise in XXXIV)
Where: app/lib/more.dart:635 (`fetchOut`, plain `dart:io` HttpClient, no proxy, no Tor check); callers station.dart:1017, 1044; main.dart:1566
What happens: A keeper turns on Go through Tor, then adds a podcast feed or "cover from a link". The phone or computer connects straight to the feed host and each enclosure host with its real address, now and every three hours in the background (`_checkFeeds`, Wi-Fi only), and plain `http` is accepted. The host learns that this address follows this feed and, via the keeper's other signals, which station it feeds. DECISIONS XXXIV says Through Tor "sends every connection by way of Tor's HTTP tunnel". The cover dialog warns the site "sees this device's address" but does not change under Tor, and the feeds screen says nothing.
Evidence: Verified by reading: no `findProxy`, no `tor` lookup anywhere in the Dart tree (`grep -in "findProxy\|'tor'"` finds only the settings toggle).
Fix: In `fetchOut` read `Native.call({'cmd':'tor'})['url']` and set `client.findProxy = (_) => 'PROXY 127.0.0.1:8118'`; refuse with a plain message if Tor is on and the tunnel does not answer. Refuse `http://` outright. Skip the background feed run when Tor is on and the tunnel is down.

### C-5 [Medium] Nothing stops another app starting or steering the exported playback service
Who: listener, broadcaster
Where: AndroidManifest.xml (PlaybackService `exported="true"`, no permission); PlaybackService.kt:273-299
What happens: `onGetRoot` guards the media browser, but `onStartCommand` does not check the caller. Any installed app can `startForegroundService` it with `action=wake, station=<any string>`: the node tunes that station (a planted station enters the played set and the carry-only-played rule of HARDENING 2.5), volume ramps up, and after 45 s the phone's alarm tone rings. With `live=true` it posts an "ON AIR" notification and a microphone-type foreground service. The exported `MediaButtonReceiver` also lets any app send Play, which resumes the last station.
Evidence: Verified by reading that no check exists. Suspected for reach: Android 12+ background-start limits may restrict this to a foreground attacker app; not tested.
Fix: Have `AlarmReceiver`, `MainActivity` and the car path add the existing `Bells.bellKey()` as an extra, and make `onStartCommand` ignore any `wake`, `live`, `stop` or title intent without it. Validate the station id with the same 64-hex test used in `onPlayFromMediaId`.

### C-6 [Medium] The recents thumbnail, lock-screen notification and "ON AIR" are public by default
Who: listener, broadcaster
Where: MainActivity.kt:77-83 (`FLAG_SECURE` only when `plain_lock`, or while words or a PIN are shown); PlaybackService.kt:240-260; Bells.kt:160-170
What happens: On a fresh install a phone left on a table shows the station name and track on the lock screen, the app switcher shows the dial with the tuned station, and while broadcasting the lock screen says ON AIR. The protection is an off-by-default switch named "Hide from the lock screen" in the middle of Settings, a separate switch from "Plain icon". HARDENING R5 was marked "settled by the plain disguise", which only settles it for people who find the switch.
Evidence: Verified by reading (default `false`).
Fix: Set `FLAG_SECURE` and private notification visibility on whenever a broadcast is live, regardless of the switch; offer "Hide what is playing" as a single question on first Raise colours; consider default-on for notification text only (title "Radio", no station) with the thumbnail left to the switch.

### C-7 [Medium] Desktop turns on autostart and tray residency by default after the first listen, with a three-second notice
Who: listener on a shared or seized computer
Where: app/lib/main.dart:551-560; shell.dart:56-85
What happens: The first Listen on a computer writes a Run key, LaunchAgent or `~/.config/autostart/pyrate-radio.desktop` named Pyrate Radio, and keeps a tray node running when the window closes. The only notice is a 3-second snackbar of two sentences (`say` duration 3 s). A forensic reader of the registry or autostart folder sees the app, and a flatmate sees an icon after the window is closed. The off-switches are under Keeping stations on the air.
Evidence: Verified by reading. (DECISIONS records "said once", not consent.)
Fix: Ask once with a dialog (Start with this computer: Yes / Not now), default Not now for the tray and autostart; keep the carrying benefit as a plain sentence.

### C-8 [Medium] Desktop builds are unsigned and sit outside the release keys; the download hashes come from the same server
Who: listener (desktop), HOSTIS
Where: .github/workflows/radio-desktop.yml (no Authenticode, ad-hoc macOS `codesign --sign -`, no detached signature on Linux); radio/tools/fetch_desktop.sh; landing/build.py:44-52; shell.dart:`build_code` returns 0
What happens: Android has two-of-three signed releases, a three-day wait and Android's own signature check. Desktop has none: `gh run download` fetches the latest successful run of any commit on `radio` (including `workflow_dispatch`), `build.py` hashes the files locally and publishes the hashes beside the files, and desktop apps never check for updates. Whoever can push to `radio` or take the site can ship a trojaned zip with a matching checksum, and users on old versions never learn of fixes while autostart keeps them running.
Evidence: Verified by reading.
Fix: Sign each desktop zip's SHA-256 with two release keys into the bearings (the `papers` mechanism already exists) and have `fetch_desktop.sh` refuse a run whose `headSha` is not the signed release commit; add `actions/attest-build-provenance`; let the desktop app show "newer version signed" from the same Releases log without installing.

### C-9 [Medium] The 24 words can be selected and copied to the unmanaged clipboard
Who: broadcaster
Where: app/lib/main.dart:1471 (`SelectionArea` around the words)
What happens: On the "Write these down" dialog long-press selection offers Copy. The words then sit on the system clipboard with no sensitive flag and no 90-second clear (those exist only in `copy_secret`), visible to keyboards, clipboard managers and Windows clipboard history. FLAG_SECURE is on for this dialog, but the clipboard is not covered.
Evidence: Verified by reading.
Fix: Remove the `SelectionArea` (plain `Text` per word).

### C-10 [Medium] Desktop invitation links go to the clipboard with no expiry
Who: operator, pocket members
Where: app/lib/shell.dart:152 (`share_text` calls `_copy(secret: false)`); station.dart:2143, 2072
What happens: SEND THE INVITATION for a private station copies a link carrying the pocket key (`pocket=`). The shell comment says an invitation's key is a secret, but the call is not marked secret, so the key stays on the clipboard and in clipboard history indefinitely. Link-only station links are rightly left.
Evidence: Verified by reading.
Fix: Route links containing `pocket=` or `flock=` through `copy_secret`.

### C-11 [Low] Android settings flags in the clear reveal habits
Who: operator, listener
Where: Bells.kt:93 (`wake_on`, `wake_h`, `wake_m`), MainActivity.kt:237 and LanternService.kt:141 (`plain_lock`, `lantern_on`, `lantern_gb`), Bells.kt:`bell_key`
What happens: A device image shows the wake time, that a Lantern runs, and that the disguise is on, which is itself the thing the disguise hides. `Hold.putS` seals only station names.
Evidence: Verified by reading.
Fix: Seal these through `putS`/`getS` (`bell_key` too).

### C-12 [Low] An unconditional `eprintln` names a station by call sign
Who: broadcaster, operator
Where: node/src/store.rs:264; also door.rs, player.rs:102,240 and ffi/src/lib.rs:108
What happens: These bypass `say()` and the QUIET flag, so a desktop launched from a terminal, a systemd user unit or the autostart entry writes "carried <call sign> into the new log format" and audio device errors to stderr or the journal.
Evidence: Verified by reading. Android stderr normally goes nowhere; not tested.
Fix: Route them through `say()`.

### C-13 [Low] A fixed, unauthenticated LAN port identifies a computer running the radio
Who: listener
Where: app/lib/mirror.dart:12, 38 (port 47824 on `anyIPv4`); main.dart:273 (restored on every start)
What happens: With Share the player on, anyone scanning the Wi-Fi finds port 47824 serving the player and knows the machine runs Pyrate Radio. It is off by default and the setting says addresses are visible, but it is re-enabled on each start and survives Through Tor.
Evidence: Verified by reading.
Fix: Random port per session, shown in the link and QR; keep it off while Through Tor is on.

### C-14 [Low] macOS hold key passes through a command line
Who: listener (macOS)
Where: app/lib/shell.dart:`security add-generic-password ... -w k`
What happens: For a moment the 64-hex key is in the argument list, visible to other processes of the same user (`ps`). Not a remote issue.
Evidence: Verified by reading.
Fix: Use `security -i` with the command on stdin, or the Security framework through FFI.

### C-15 [Low] The renamed-package build still says Pyrate Radio
Who: operator under a blocking regime
Where: AndroidManifest.xml (`android:label="Pyrate Radio"` on the Skull alias, `pyrate://` and hostis.black intent filters, `black.hostis.pyrate` classes)
What happens: `-PPYRATE_APP_ID` changes only the package; the label, scheme and class names stay.
Evidence: Verified by reading the manifest.
Fix: Template the label, scheme and filters from the same property in the disguise build.

## UX and product

### C-UX-1 [High] The Tor switch is buried and its promise is partly untrue
Who: everyone who needs Tor most
Where: more.dart:1798 (Settings, Advanced, Connections, folded), main.dart (feeds, C-4)
What happens: The one control that changes the threat model is the third item in a fold at the foot of Settings, off by default, taking effect "from the next time the radio starts" with no restart button. People at risk will not find it, and when they do, feeds and links still leak (C-4).
Evidence: Verified by reading.
Fix: Move it into Privacy and safety as "Hide this device's address (needs Orbot)" with a Restart now button; say what it does not cover until C-4 is fixed.

### C-UX-2 [High] Accessibility is thin
Who: listeners using TalkBack or large text
Where: lib-wide; `Semantics` appears 1 time in glass.dart, 8 in main.dart, 11 in desk.dart, 0 in more.dart and station.dart; about 114 text styles at 9.5 to 11 px in widely spaced capitals (`caps`, `carved`); disabled and "ghost" labels at 22 percent alpha (contrast about 1.7:1, e.g. LeadButton off state, main.dart:125)
What happens: Custom `InkWell`/`GestureDetector` controls (19 in the tree) are not announced as buttons when they hold text, icon-only controls have no label, and eleven-pixel letterspaced Cinzel is hard to read at normal scale and overflows at 200 percent (fixed `SizedBox` widths such as station.dart:326, a fixed 118-high readout at main.dart:1261). The app follows the system font scale, which is right, but nothing was built for it.
Evidence: Suspected for overflow and announcements (not run with TalkBack); Verified for the counts and sizes.
Fix: Wrap each custom tap target in `Semantics(button: true, label: ...)` or use `TextButton`; floor body text at 14 px and caps labels at 12; test with fontScale 2.0; keep 48 dp tap targets (only three places set them).

### C-UX-3 [Medium] "You can change all of this later" is false for the passphrase and invite-only
Who: new keeper
Where: main.dart (Start a station, note under the Raise colours button); station.dart:857 ("all of it can change later")
What happens: A passphrase fixes which station the 24 words open, and invite-only changes the station's kind. A keeper who skips the passphrase to "add it later" cannot, and one who sets it and forgets it loses the station. The passphrase sits in a fold titled More options, so most keepers will not see the decision.
Evidence: Suspected (core behaviour inferred from DECISIONS XXXII and the raise code; not run).
Fix: Change the line to "Everything but the passphrase and invite-only can change later", and lift the passphrase out of the fold with one line on who it protects against.

### C-UX-4 [Medium] A cold notification permission prompt arrives before the first sound
Who: new listener
Where: MainActivity.kt:90-95
What happens: On Android 13+ the system dialog appears on the first frame with no explanation, over the first-open playback the app is proud of (XXXVI). Refusal also silently removes lock-screen controls.
Evidence: Verified by reading.
Fix: Ask when the first station starts, after a one-line reason on the mini player, or skip the ask and rely on the foreground service notice.

### C-UX-5 [Medium] Vocabulary a listener must learn before they can act
Who: new listener, new keeper
Where: main.dart:1664 ("Raise colours"), the "Colours" page title, "Broadcast" tab, "Lantern", "Keep it off the dial", "door", "carry"
What happens: The brand voice is strong but the Raise button, the word colours and the invitation to "carry" ask the user to decode pirate metaphors at the moment of commitment; the tab called Broadcast opens a page called Colours. A mainstream radio app shows "Start a station" on the tab it is named for.
Evidence: Verified by reading.
Fix: Keep the voice in headings; make the button "Start the station" and the page title match its tab.

### C-UX-6 [Medium] Settings are long and mix three kinds of thing
Who: everyone
Where: more.dart:1590-1840: about 35 controls in five sections
What happens: Listening, hiding, Lantern, wake alarm, Tor and diagnostics sit in one scroll. Privacy controls that a worried user wants first (PIN, Emergency PIN, Hide from the lock screen, Plain icon, Wipe) sit mid-page; the Wake alarm and Hidden stations sit above them.
Evidence: Verified by counting.
Fix: Put "Privacy and safety" first and fold the rest; add a one-screen "Keep me safe" preset (PIN, hide lock screen, plain icon, Tor) that sets the four together.

### C-UX-7 [Low] A recording fails silently on desktop mic and keeps raw takes
Who: keeper
Where: shell.dart `mic` returns `'ok'` without asking the OS; record_start returns null on failure
What happens: The permission step is skipped on computers, so a blocked microphone shows only the generic "The microphone would not start. Another app may have it."
Evidence: Verified by reading.
Fix: Say what failed ("No microphone found" or "Microphone blocked in system settings") from the node's error.

### C-UX-8 [Low] House-rule wording slips
Who: listener, keeper
Where: station.dart:1230 ("Packing the station: its log, its sound and its art."), station.dart:1232 ("Not packed: ..."), more.dart:1602 ("Not saved: that folder cannot be written to."), more.dart:1794 ("Not set: ..."), more.dart:1792 ("That file would not open: ..."), more.dart:1627 ("Not cleared: ...")
What happens: Colon hinges break the no-colon-hinge rule. No em dashes found in the app's strings; the only en dash is the time range at station.dart:326, which is fine. British spelling holds in every user string checked.
Evidence: Verified by grep.
Fix: "Packing the station with its log, sound and art", "Not packed. {reason}", and so on, one sentence each.

### C-UX-9 [Low] Desktop users get no update signal
Who: desktop listener
Where: shell.dart (`build_code` returns 0)
What happens: The phone shows "A new version is ready"; the computer never does, though it stays running via autostart.
Evidence: Verified by reading.
Fix: Show the signed Releases entry as "A newer version exists, take it from hostis.black" without downloading.

## Checked and sound

- allowBackup false, `dataExtractionRules` exclude every domain; no `android:debuggable`; no cleartext flag needed.
- Prefs store: `prefs.json` is a sealed hold note (node/src/lib.rs:1777, 3368); resume, feeds, kept, hidden, Tor URL and chat name are inside it, not in SharedPreferences.
- Android notes (bells, wake station, last station) sealed under the Keystore key; Hold.kt key wrap and scuttle ordering correct.
- Release builds write nothing to logcat (`say()` is a no-op on release Android); Dart has no `print` or `debugPrint`.
- Every text field has incognito typing; PIN, passphrase and words fields use obscure or visible-password with suggestions off.
- `copy_secret` marks IS_SENSITIVE and clears after 90 s only if unchanged; tip addresses use it.
- FLAG_SECURE covers the words, co-host and PIN screens on every device.
- Deep links: the `pyrate://` and https filters set only a pending id; the app asks before tuning or joining; the bell tap needs the app's secret key.
- `MediaBrowserService.onGetRoot` admits only this app, the system, Android Auto and Bluetooth; pocket stations are omitted from the car list.
- FileProvider exports only `node/updates/` and `cache/share/`, `exported=false`, per-grant.
- Update path: two-of-three release keys over code and SHA-256, three-day wait, hash re-checked before offering, assembled part file renamed only when whole, and Android's own signer check on install.
- Pass-on server: private address only, 72-bit path token, 2 KB request line, four connections, closed after 15 minutes.
- Desktop hold key goes to DPAPI, macOS keychain or Secret Service with a read-back check before the file drops it; scratch folder is app-private, emptied at start, kept out of search indexes.
- Tip links always carry a payment scheme; no shell is used for `open_uri` (argument list).
- CI: every action pinned by commit SHA, `permissions: contents read`, `cargo build --locked`, cargo-audit, vendored Opus fingerprints checked, open player fetched by content and verified, no secrets in the workflow.
- Release APK is built with the debug key and then re-signed offline, never on a build server (RELEASES.md).
