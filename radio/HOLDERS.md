# Signing a release

For the three people who hold the release keys. A release goes out only when two of you have signed it, each on your own computer. It takes about five minutes, on Windows, with your paper of 24 words beside you.

## What arrives

A zipped folder from Dylan, named like `pyrate-release-0.9.18.zip`. Inside it are `release.exe` and the files that make up the release:

- the Android app, named like `31-0.9.18.apk`;
- `player.manifest`, the list of the browser player's files;
- `players.txt`, the list of every browser player HOSTIS has published;
- the computers' builds, ending `.zip` or `.tar.gz`;
- `bearings.json`, the list of ways into the network.

A folder may hold only some of these. Sometimes a release needs a second, smaller folder a day later, because the list of players can be made only once the first signatures are back.

## Signing it

1. Save the zip to your Desktop. Right-click it, choose **Extract All**, then press **Extract**. A folder named like `pyrate-release-0.9.18` opens.
2. Take the computer off the internet. Click the network icon at the bottom right of the screen and turn Wi-Fi off, or pull out the network cable.
3. In the open folder, click once in the address bar at the top, where the folder's name is shown. Type `cmd` and press **Enter**. A black window opens.
4. Copy this line, right-click inside the black window to paste it, change the `2` to your own key number (1, 2 or 3, as written on your paper), and press **Enter**:

       release.exe sign-all - 2 .

5. Type your 24 words from the paper, with a space between each, and press **Enter**.
6. The window lists each file as signed, then says how many keys have signed each one. The first holder sees "One more holder needs to sign it" against each file. The second holder sees "Everything in this folder is signed. Send it back."
7. Close the black window. Turn the internet back on.
8. Right-click the folder, choose **Send to**, then **Compressed (zipped) folder**. Send the new zip back to Dylan the way the first one came.

## What you send back

The whole folder, zipped. Your signature sits in the new files ending `.sigs`, and inside `bearings.json`. Your words stay on your paper. The window reads them once and writes them nowhere.

## When something looks wrong

- If the window says those words are not your key, check the key number in the line you pasted and the words on your paper, then paste the line again.
- If it says there is nothing here to sign, the black window opened somewhere else. Close it and start again at step 3, inside the folder itself.
- To see what is signed without signing anything, paste `release.exe check .` in place of the line in step 4.
- Sign only a folder you were expecting, sent by Dylan the usual way. If anything about it surprises you, ask him on another channel before you sign.
- Nobody from HOSTIS will ever ask for your words, Dylan included.
