# A Lantern in one command. No rights reserved.
#   docker build -t pyrate-lantern -f radio/Dockerfile radio
#   docker run -d --restart unless-stopped -v pyrate:/data pyrate-lantern
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
VOLUME /data
ENTRYPOINT ["lantern", "--dir", "/data", "--quota-gb", "50", "--bootstrap", "a324c679b0dda368e23f21d6abbdaec0ef541a4ac317fa0643802b2734c9a442"]
