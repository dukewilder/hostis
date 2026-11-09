# Transport spike · results

64 kbit/s stream, one frame every 125 ms, iroh 1.4.1.

| Leg | Path | Sound in | Frames | Gaps | Worst jitter | Verdict |
| --- | --- | --- | --- | --- | --- | --- |
| One machine | direct | 1.2 s | 162 of 160 | 0 | 1 ms | HOLDS |
| Cloud station to a PC in Austin, 90 s | relay (use1, n0) | 0.57 s | 722 of 720 | 0 | 248 ms | HOLDS |
| PC on home broadband behind ProtonVPN to a phone on T-Mobile, 120 s, two runs | relay (use1, n0) | 0.75 s | 962 of 960 | 0 | 363 ms | HOLDS |
| The same pair with the VPN paused | direct, hole-punched | | | | | to run |

The phone ran on mobile data alone. T-Mobile gives it IPv6 and reaches IPv4 only through a carrier translator. The PC has no IPv6 and sends everything out through a ProtonVPN exit. Two translators facing each other leave no hole to punch, so the relay carried both runs, and carried them clean. A crew that follows the Practice runs VPNs, so relays carry real weight in this design. Every Lantern runs one.

A bare binary on Android has no JVM, so the system DNS settings cannot be read. The spike asks 1.1.1.1 and 8.8.8.8 there. The app hands iroh the Android context and reads the system settings.

The cloud leg is relay-only by construction: the cloud machine has no UDP out. The phone leg is the one that tests hole punching through a carrier's NAT.
