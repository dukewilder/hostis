# Tor for a dark Lantern, from Debian's own package. No rights reserved.
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends tor && rm -rf /var/lib/apt/lists/* \
 && mkdir -p /var/lib/tor && chown debian-tor:debian-tor /var/lib/tor && chmod 700 /var/lib/tor
COPY torrc /etc/tor/torrc
USER debian-tor
ENTRYPOINT ["tor", "-f", "/etc/tor/torrc"]
