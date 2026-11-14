# Releases

Releases travel as a station, so they need no server.

**Once, offline, on the keyholder's machine:**

    release init release-keys

This makes three release keys as 24 words each (`release-key-1.txt` to `-3.txt`), `release_keys.txt` (the public keys, which go into the app), and `lantern-genesis` (the Releases station with a deputy key for the Lantern). Write each word file down, keep the three in three places, and delete the files.

**Each release:**

1. Build the APK and give it a higher version code.
2. Sign with two of the three keys, each on its own:

       release sign release-key-1.txt 1 pyrate.apk 5 >  5-0.5.0.sigs
       release sign release-key-3.txt 3 pyrate.apk 5 >> 5-0.5.0.sigs

3. Put `5-0.5.0.apk`, `5-0.5.0.sigs` and, if wanted, `5-0.5.0.txt` (release notes) in the Lantern's `releases/` folder. Within a minute it is published, and phones offer it within half an hour.

A release with fewer than two good signatures is never published and never installed. Phones install only on the owner's tap, and Android asks again.

**What a Lantern carries** is its operator's choice: one station id per line in its `maroon.txt` and it will not list, play or carry that station. The file stays on the box and is published nowhere.

**The Android signing key (once, offline).** Android accepts a new version only if it carries the same signature as the one installed.

    release android-init release-keys

makes `android-key.txt` (24 words: write them down apart from the release keys, then delete the file) and `android-cert.der` (public). For each release, after building the unsigned APK:

    release sign-apk android-key.txt android-cert.der unsigned.apk "Pyrate Radio.apk"

The first signed build needs one reinstall over the test builds; every release after installs as an update.

## Reproducible builds

Every byte we ship can be rebuilt from this source and checked, so nobody has to take our word that a download holds the code in the tree. The toolchains are pinned and the build paths are remapped, so the same source gives the same bytes on any machine.

**The pins.**

- The browser player: Rust 1.97.0 (`rust-toolchain.toml`), wasm-bindgen 0.2.130, wasm-opt 132, and the `web` cargo profile in `Cargo.toml` (opt-level z, fat LTO, one codegen unit, no debug info). `browser/build.sh` remaps the source and registry paths, so the wasm carries none of the builder's own paths.
- The Android app: Flutter on Dart 3.9 (`app/pubspec.yaml`, `sdk: ^3.9.0`), Gradle 9.3.1 (`app/android/gradle/wrapper/gradle-wrapper.properties`), Kotlin 2.4.0, minSdk 28, and the NDK the Flutter release names. The Flutter SDK is not kept in this tree, so record your `flutter --version` beside the release.

**Check the player.**

    tools/repro.sh player

builds the wasm and compares its SHA-256 against the one in `tools/repro.hashes`. A match means the player you built is the player we published, to the byte. After a real release, set the published hash with `tools/repro.sh player --record`. The deploy (`tools/deploy_site.sh`) builds the player the same way, so a deployed player matches too.

**Check the APK.**

    tools/repro.sh apk <the published .apk>

builds the release APK where Flutter is set up and compares it against the published one with `tools/apk_compare.py`, which leaves the signatures aside, since two people who sign the same code each sign with their own key. Where Flutter is not set up the script prints the recipe and stops without building.
