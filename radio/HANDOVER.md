# Handover

This is the runbook for keeping Pyrate Radio going without the founder. Nothing here needs a person who was there at the start. It needs the source, the pinned toolchains, the release key words from the people who hold them, and the Android key. Work through it in order. `RELEASES.md` has the exact commands; `LANTERN.md` has the running; `GOVERNANCE.md` has who may do this and how the keys pass on.

## What you need

- A clone of this repo on the `radio` branch.
- The toolchains, pinned in `RELEASES.md`: Rust 1.97.0 for the player, and Flutter on Dart 3.9 with the Android SDK for the app.
- At least two of the three release key holders, each at their own offline machine. No one person should hold two.
- The Android signing key (`android-key.txt`) and its certificate (`android-cert.der`). Without the same Android key, phones cannot install the new build as an update of the old.

## 1. Build

- The browser player and the open player it carries: `sh browser/build.sh`.
- The Android app: raise the version in `app/pubspec.yaml` (the number after the plus is the version code, and it must go up every release), then `bash app/build_native.sh` (it builds the node for Android with the build paths remapped, then the app; see `RELEASES.md`). The APK lands in `app/build/app/outputs/flutter-apk/`.

## 2. Check the build

Run `tools/repro.sh player`. A match against `tools/repro.hashes` means the player is the code in the tree and nothing else. For the APK, where a second machine can build it, compare the two with `tools/apk_compare.py`, which leaves the signatures aside. Build from the pinned toolchains or the bytes will differ.

## 3. Sign

Two signatures, two different purposes. Do them offline.

- The Android signature, so a phone installs the build as an update of the last. On the keyholder's machine: `release sign-apk android-key.txt android-cert.der <the built apk> "Pyrate Radio.apk"`.
- The release signatures, so the app's own updater offers the build at all. Two of the three release keys each sign the version code, each on its own machine, into one `.sigs` file beside the APK. See `RELEASES.md` for the exact lines. A build with fewer than two good signatures is never published and never installed.

Never type the 24 words of any key into a machine that has been online. Never print or commit a key, a word list or a seed.

## 4. Release

A release travels as a station, so it needs no server. Put the signed `<n>-<version>.apk`, its `<n>-<version>.sigs` and, if you like, a `<n>-<version>.txt` of notes into the `releases/` folder of a Lantern that holds the Releases deputy key (made by `release init`, see `RELEASES.md`). Within a minute the Lantern publishes it; phones offer it to their owners within half an hour, and install only on a tap.

## 5. Put the downloads on the site (optional)

The website is a convenience, not the network. To refresh `hostis.black/pyrateradio/` with the new player and the new app:

    APK=<site>/pyrateradio/pyrate-radio-<version>.apk VER=<version> tools/deploy_site.sh "what landed"

It builds the player and the landing, copies in the APK you name, writes the source tarball and its torrent, and pushes the site. It does not build the APK; it offers the one you point it at.

## 6. Light a Lantern


A network of one station is still a network, but it carries best when Lanterns are up. Follow `LANTERN.md`: a Lantern in one Docker command, and a relay beside it on a server so browsers can reach the network. Seed your own stations from folders of audio with `--seed`, and mark one explicit or mature with a rating after its kind, as in `Name|About|/folder|talk/explicit`.

## The keys, in one place

- Three release keys, 24 words each, held by three people in three places, offline. Two sign a release. Replacing a holder is in `GOVERNANCE.md`.
- One Android signing key, kept as carefully. Lose it and phones must reinstall from scratch to take the next update.
- The `lantern-genesis` Releases station and its deputy, made once by `release init`. The deputy publishes releases; it cannot forge one, because installing still needs two of the three.
- Each Lantern's own `hold.key` seals what that Lantern keeps. It is not a release key and is never shared.

If a key is lost or a machine is suspected, treat the key as burnt from that hour and follow the replacement in `GOVERNANCE.md`.
