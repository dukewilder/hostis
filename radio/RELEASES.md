# Releases and the refusal list

Both travel as stations, so neither needs a server.

**Once, offline, on the keyholder's machine:**

    release init release-keys

This makes three release keys as 24 words each (`release-key-1.txt` to `-3.txt`), `release_keys.txt` (the public keys, which go into the app), and `lantern-genesis` (the Releases station and the HOSTIS refusal list, each with a deputy key for the Lantern). Write each word file down, keep the three in three places, and delete the files.

**Each release:**

1. Build the APK and give it a higher version code.
2. Sign with two of the three keys, each on its own:

       release sign release-key-1.txt 1 pyrate.apk 5 >  5-0.5.0.sigs
       release sign release-key-3.txt 3 pyrate.apk 5 >> 5-0.5.0.sigs

3. Put `5-0.5.0.apk`, `5-0.5.0.sigs` and, if wanted, `5-0.5.0.txt` (release notes) in the Lantern's `releases/` folder. Within a minute it is published, and phones offer it within half an hour.

A release with fewer than two good signatures is never published and never installed. Phones install only on the owner's tap, and Android asks again.

**The refusal list:** one station id per line in the Lantern's `refusals.txt`, a space, then the reason. Removing a line lifts the refusal. Following the list is on by default and every listener can turn it off.
