# Light a Lantern

A Lantern is a node that never sleeps. It carries every station it hears of, relays for phones that cannot reach each other, and keeps the network findable. Every Lantern is another full copy of every station.

**With Docker** (any Linux machine, a Raspberry Pi, a VPS):

    git clone --depth 1 --branch radio https://github.com/dukewilder/hostis
    docker build -t pyrate-lantern -f hostis/radio/Dockerfile hostis/radio
    docker run -d --name lantern --restart unless-stopped -v pyrate:/data pyrate-lantern

It opens no ports and needs none. Behind a VPN it works the same, through the relays.

**Your own stations on your own site.** Add `--http 127.0.0.1:8080` and `--seed "Name|About|/folder|music"`, then put a web server or tunnel in front of port 8080. The page plays your Lantern's stations and nothing else, and any page can embed a station with one `<audio>` tag.

**Quota.** `--quota-gb` caps what it carries. Fifty gigabytes holds about 1,700 hours of music.

**Art.** Put `station.jpg` (the cover) and `banner.jpg` in a seed folder and the Lantern raises them as the station's art within ten minutes. A `cover.jpg` beside recordings, or an album cover inside them, shows while they play. Change a picture by replacing the file.
