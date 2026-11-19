# Signing a release

For the three people who hold the release keys. A release goes out only when two of you have signed it, each on your own computer. It takes about five minutes, on Windows, with your paper of 24 words beside you.

## What arrives

A zipped folder from Dylan, named like `pyrate-release-0.9.18.zip`. Inside it are `release.exe`, **Sign this release**, and the files that make up the release:

- the Android app, named like `31-0.9.18.apk`;
- `player.manifest`, the list of the browser player's files;
- `players.txt`, the list of every browser player HOSTIS has published;
- the computers' builds, ending `.zip` or `.tar.gz`;
- `bearings.json`, the list of ways into the network.

A folder may hold only some of these. Sometimes a release needs a second, smaller folder a day later, because the list of players can be made only once the first signatures are back.

## Signing it

1. Save the zip to your Desktop. Right-click it, choose **Extract All**, then press **Extract**. A folder named like `pyrate-release-0.9.18` opens.
2. Take the computer off the internet: click the network icon at the bottom right of the screen and turn Wi-Fi off, or pull out the network cable. The window in the next step says when the computer is offline.
3. In that folder, double-click **Sign this release**. A black window opens and lists the files waiting to be signed.
4. Choose your key number (1, 2 or 3, as written on your paper), type your 24 words with a space between each, and press **SIGN THE RELEASE**.
5. The window lists each file as signed. It zips the signed folder for you, beside the folder, named like `pyrate-release-0.9.18-signed-key-2.zip`, and opens it highlighted.
6. Close the window and turn the internet back on. The first holder sends that zip to the second; the second holder sends it back to Dylan. The window says which.

To sign without the window, open a command prompt in the folder (type `cmd` in its address bar and press **Enter**) and run `release.exe sign-all - <your key number> .`, then type the words.

## What you send back

The whole folder, zipped. Your signature sits in the new files ending `.sigs`, and inside `bearings.json`. Your words stay on your paper. The window reads them once and writes them nowhere.

## When something looks wrong

- If the window says those words are not your key, check the key number you chose and the words on your paper, then try again.
- If it says there is nothing here to sign, or release.exe is missing, the zip was opened without extracting it. Close it and start again at step 1.
- To see what is signed without signing anything, open the window and read the list before typing anything, or run `release.exe check .` in a command prompt.
- Sign only a folder you were expecting, sent by Dylan the usual way. If anything about it surprises you, ask him on another channel before you sign.
- Nobody from HOSTIS will ever ask for your words, Dylan included.
