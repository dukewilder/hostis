# HOSTIS relay over https (Cloudflare Tunnel)

Dylan will give you a Cloudflare tunnel token. Treat it like a key: never print it, never commit it, keep it in a file with mode 600 under /srv/pyrate-keys/.

Goal: https://relay.hostis.black reaches the harbour Lantern's relay, so browser listeners can connect. No published ports, no router changes, and the house IP never touches Cloudflare.

1. Run `cloudflare/cloudflared:latest` as a new container named `pyrate-tunnel`, with `network_mode: "container:<the gluetun the Lanterns use>"` so its outbound connection leaves through Proton. Command: `tunnel --no-autoupdate run`, with the token passed as the environment variable `TUNNEL_TOKEN` read from the key file. Restart policy `unless-stopped`. Do not touch Jellyfin, the arr stack, Monero or the main gluetun.
2. The tunnel's public hostname (Dylan sets it in the Cloudflare dashboard) points at `http://localhost:42779`. If the harbour's relay listens on a different inside port, tell Dylan the right one so he can change it.
3. Set `PYRATE_RELAY_URL=https://relay.hostis.black/` on the harbour and restart it.
4. Check, all through the VPN namespace:
   - `curl -sI https://relay.hostis.black/ping` answers 200.
   - Two minutes after the restart, the harbour's pkarr record at https://dns.iroh.link/pkarr/<z-base-32 id> contains `https://relay.hostis.black/`.
   - From the host itself (outside the VPN) `curl https://relay.hostis.black/ping` also answers, and the tunnel log shows its edge connections came from the Proton exit, not the house.
5. Report the three checks, the container name and the inside port. Do not print the token.
