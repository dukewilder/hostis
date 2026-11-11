# Light a Lantern

A Lantern is a node that never sleeps. It carries every station it hears of, relays for phones that cannot reach each other, and keeps the network findable. Every Lantern is another full copy of every station.

**With Docker** (any Linux machine, a Raspberry Pi, a VPS):

    git clone --depth 1 --branch radio https://github.com/dukewilder/hostis
    docker build -t pyrate-lantern -f hostis/radio/Dockerfile hostis/radio
    docker run -d --name lantern --restart unless-stopped -v pyrate:/data pyrate-lantern

It opens no ports and needs none. Behind a VPN it works the same, through the relays.

**Its keys.** Everything a Lantern keeps is sealed under its hold key, which by default lies in `hold.key` beside the data, so a copy of the whole folder opens. Put the key on another disk, or a USB stick, with `-e PYRATE_HOLD_KEY=/keys/hold.key -v /path/on/that/disk:/keys`, and turn on the disk's own encryption. The Lantern says so at every start until you do. Never print or share `hold.key`.

**Its log.** A Lantern logs, for you, which stations it carries and which peers it meets, and Docker keeps those lines. `-e PYRATE_QUIET=1` stops them.

**Your own stations on your own site.** Add `--http 127.0.0.1:8080` and `--seed "Name|About|/folder|music"`, then put a web server or tunnel in front of port 8080. The page plays your Lantern's stations and nothing else, and any page can embed a station with one `<audio>` tag. The page and its streams are ordinary web: whoever runs the Lantern, and any proxy or tunnel in front of it, can see a listener's address beside the station played, and the page says so. Keep no access logs if you would rather not hold that.

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

**The open player.** A Lantern at a name of its own can serve the open player, the browser radio that plays every station and raises new ones, at `/player/`. Fetch `https://hostis.black/pyrateradio/ipfs/open-player.tar.gz`, unpack it into a folder, and start the Lantern with `PYRATE_PLAYER=<that folder>`. Every Lantern that does is one more door to the whole Log that no takedown of any one site can close. A Lantern at a HOSTIS name never serves it: hostis.black carries HOSTIS's stations alone. Any IPFS node can keep the player alive too, with `pin_player.sh` from the same folder, hourly from cron.
