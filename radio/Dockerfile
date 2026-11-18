# A Lantern in one command. No rights reserved.
#   docker build -t pyrate-lantern -f radio/Dockerfile radio
#   docker run -d --restart unless-stopped -v pyrate:/data \
#     -v /media/usb/pyrate:/pass:ro -e PYRATE_HOLD_PASS_FILE=/pass/hold-pass pyrate-lantern
# The passphrase seals the hold key; without it the Lantern opens nothing (LANTERN.md). Add
# --hold-key-on-disk after the image name to keep the key on the volume in the clear instead.
# To run it dark through Tor, with nothing published, see lantern-dark/ (one command).
# To serve the open player at /listen/ on a name of your own (never a HOSTIS name), add
#   -p 8080:8080 ... pyrate-lantern --http 0.0.0.0:8080 --player /usr/share/pyrate/player
FROM rust:bookworm AS build
RUN apt-get update && apt-get install -y --no-install-recommends cmake && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY . .
RUN cargo build --release -p pyrate-node --bin lantern

# The open player, for --player: the one built here if there is one, otherwise the one the app
# names, fetched from IPFS and checked block by block. None to be had leaves the folder empty.
FROM python:3.12-slim-bookworm AS player
WORKDIR /src
COPY tools/open_player.py tools/
COPY app/assets/open_players.txt app/assets/
COPY browser/www/ browser/www/
RUN python3 tools/open_player.py --dir /player

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=build /src/target/release/lantern /usr/local/bin/lantern
COPY --from=player /player /usr/share/pyrate/player
# The release keys, so the Lantern takes seed lists and releases two of them signed.
COPY app/assets/release_keys.txt /usr/share/pyrate/release_keys.txt
VOLUME /data
# Its first door is named with its relay, so no lookup is needed to reach it (a Lantern running
# dark makes none).
ENTRYPOINT ["lantern", "--dir", "/data", "--quota-gb", "50", "--bootstrap", "a6e12327929c36614b4cd24483103a0dc125fdd0be97fd27b85bdb61b8c397c3@https://relay2.hostis.black,a324c679b0dda368e23f21d6abbdaec0ef541a4ac317fa0643802b2734c9a442@https://relay.hostis.black", "--release-keys", "/usr/share/pyrate/release_keys.txt"]
