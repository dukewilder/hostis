# Light a Lantern

A Lantern is a node that never sleeps. It carries every station it hears of, relays for phones that cannot reach each other, and keeps the network findable. Every Lantern is another full copy of every station.

**With Docker** (any Linux machine, a Raspberry Pi, a VPS):

    git clone --depth 1 --branch radio https://github.com/dukewilder/hostis
    docker build -t pyrate-lantern -f hostis/radio/Dockerfile hostis/radio
    docker run -d --name lantern --restart unless-stopped -v pyrate:/data pyrate-lantern

It opens no ports and needs none. Behind a VPN it works the same, through the relays.

**A relay beside it, on a server.** A browser cannot open ports, so every browser listener reaches the network through a relay, and each new relay is one more door. On a fresh Debian or Ubuntu server, as root:

    curl -fsSL https://raw.githubusercontent.com/dukewilder/hostis/radio/radio/tools/vps/install.sh -o pyrate.sh && bash pyrate.sh

It asks for the relay's name (an A record pointing at the server, grey cloud if it is on Cloudflare), or makes a free one from the server's address. It installs iroh-relay at the iroh version the radio is built with, checked against its checksum, with its own Let's Encrypt certificate; runs the Lantern above with `PYRATE_RELAY_URL` set to the relay and restarts both on boot; and lets in only SSH, 80, 443 and 7842/udp. It prints the relay's address and the Lantern's id. Run it again to update.

**How a new relay is found.** A Lantern homes on the relay it reaches fastest, and on the same server that is its own. Every couple of minutes it signs a harbour note naming that relay, and publishes the same to number0's lookup. Phones, computers and browser tabs that know the Lantern as a carrier take the note into their address book and reach it through that relay. Make the Lantern a first door (`id@https://relay/` in the app's harbour list) and the app takes its relay as one of its own for next time.

**Its keys.** Everything a Lantern keeps is sealed under its hold key, which by default lies in `hold.key` beside the data, so a copy of the whole folder opens. Put the key on another disk, or a USB stick, with `-e PYRATE_HOLD_KEY=/keys/hold.key -v /path/on/that/disk:/keys`, and turn on the disk's own encryption. The first time it starts that way, the Lantern copies the old key there and checks the copy; remove the old `hold.key` yourself once it runs. The Lantern says so at every start until you do. Never print or share `hold.key`.

**What it carries for others.** Logs carried for other stations share 1 GB, no station more than 256 MB; a station that outgrows its share is taken no further for a week, and when the whole is full the newest station nobody has listened to through this Lantern in a day goes first. Your own stations are never counted. Set the total with `-e PYRATE_LOG_MB=<megabytes>` (16 or more); one station may then take half of it. Once a minute the Lantern hands memory it has finished with back to the system.

**Its log.** A Lantern logs, for you, which stations it carries and which peers it meets, and Docker keeps those lines. `-e PYRATE_QUIET=1` stops them.

**Your own stations on your own site.** Add `--http 127.0.0.1:8080` and `--seed "Name|About|/folder|music"` (or `talk`, or `mono` for old records and anything else recorded in one channel: half the bytes of stereo; to move a seeded station to another kind, start once with `--reseed "Name"`: every recording in the folder is set again, and once they are on the clock the old ones are withdrawn, along with any whose file has left the folder), then put a web server or tunnel in front of port 8080. The page plays your Lantern's stations and nothing else, and any page can embed a station with one `<audio>` tag. The page and its streams are ordinary web: whoever runs the Lantern, and any proxy or tunnel in front of it, can see a listener's address beside the station played, and the page says so. Keep no access logs if you would rather not hold that. Podcast feeds name the address the page was reached at: set `-e PYRATE_PUBLIC_URL=https://your.address` to fix it, or `-e PYRATE_TRUST_PROXY=1` if a proxy in front sets `X-Forwarded-Host`. Without either, the Lantern trusts only the request's own Host.

**Quota.** `--quota-gb` caps what it carries. Fifty gigabytes holds about 1,700 hours of music.

**Art.** Put `station.jpg` (the cover) and `banner.jpg` in a seed folder and the Lantern raises them as the station's art within ten minutes. A `cover.jpg` beside recordings, or an album cover inside them, shows while they play. Change a picture by replacing the file.

**What a Lantern carries.** Every station in the Log, by default. Put a station's id on a line of `maroon.txt` in the data folder (a `#` and a note after it if you like) and this Lantern drops it, art and log included, within a minute. Lists other people share are in the same form. Start with `--carry-own` to carry only your own stations.

**The bell.** A `bell.wav` (or .mp3, .ogg) in a seed folder plays on the hour. Replace the file and the new one takes over; the old one is withdrawn. `tools/bell.py` writes a daily one from the Annals; run it from cron each morning.

**Podcasts.** `tools/feed2folder.py <feed url> <seed folder>` brings your own show's episodes into a seed folder, and the Lantern puts them on the clock. Going the other way, `/feed/<station id>.xml` on the web listener is the station's own podcast feed, newest recording first, so any podcast app can follow it. Episodes are made into MP3s the first time they are asked for.

**The programme.** A `programme.json` in a seed folder is the station's programme, in the same form the app writes, and the Lantern sets it whenever the file changes. Times are the folder's own: give `"utc_offset_min"` (-360 for Chicago in winter). Name recordings by title, by "Artist - Title", or by file name.

    {
      "utc_offset_min": -360,
      "mode": { "kind": "daily", "start_min": 0 },
      "hours": [ { "days": 127, "start_min": 1080, "len_min": 360 } ],
      "slots": [
        { "title": "The Evening Watch", "about": "Low to high as the light goes.",
          "span": { "days": 31, "start_min": 1140, "len_min": 30 },
          "fill": { "kind": "tracks", "tracks": ["Low Tone", "Tester - Mid Tone"], "shuffle": false } },
        { "title": "Night Call", "span": { "days": 127, "start_min": 1290, "len_min": 60 },
          "fill": { "kind": "library" }, "live": true }
      ]
    }

Modes are `shuffle`, `in_order`, `newest_first`, `daily` and `weekly` (with `"day"`, 0 for Monday). Days are a sum: Monday 1, Tuesday 2, Wednesday 4, Thursday 8, Friday 16, Saturday 32, Sunday 64; 127 is every day. Minutes count from midnight, so 1080 is six in the evening. A slot's fill is `library`, `tracks`, `newest` (with `"count"`) or `hand` (with a hand's `"key"`). Leave out `hours` for round the clock. The bell keeps ringing unless the file names an `"ident"`.

**Notices.** A `notices.txt` in a seed folder holds the station's notices, one paragraph each. A new paragraph goes up on the station's page; delete it and the notice comes down. Notices posted from a phone are left alone.

**The open player.** A Lantern at a name of its own can serve the open player, the browser radio that plays every station and raises new ones, at `/listen/`. The image carries a copy (built here, or the one the app names, fetched from IPFS and checked block by block): run it with `-p 8080:8080` and `--http 0.0.0.0:8080 --player /usr/share/pyrate/player` after the image name. Outside Docker, `python3 tools/open_player.py --dir <folder>` makes the folder and `--player <folder>` serves it. The old `/player/` address sends visitors on to `/listen/`. Every Lantern that serves it is one more copy no takedown of any one site can close; the rented-server install (`tools/vps/install.sh`) serves it over HTTPS and prints its address. A Lantern at a HOSTIS name never serves it: hostis.black carries HOSTIS's stations alone. Any IPFS node can keep the player alive too, with `pin_player.sh`, hourly from cron.
