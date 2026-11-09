# A Lantern in one command. No rights reserved.
#   docker build -t pyrate-lantern -f radio/Dockerfile radio
#   docker run -d --restart unless-stopped -v pyrate:/data pyrate-lantern
FROM rust:bookworm AS build
RUN apt-get update && apt-get install -y --no-install-recommends cmake && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY . .
RUN cargo build --release -p pyrate-node --bin lantern

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=build /src/target/release/lantern /usr/local/bin/lantern
VOLUME /data
ENTRYPOINT ["lantern", "--dir", "/data", "--quota-gb", "50", "--bootstrap", "a324c679b0dda368e23f21d6abbdaec0ef541a4ac317fa0643802b2734c9a442"]
