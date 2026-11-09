# Transport spike · results

64 kbit/s stream, one frame every 125 ms, iroh 1.4.1.

| Leg | Path | Sound in | Frames | Gaps | Worst jitter | Verdict |
| --- | --- | --- | --- | --- | --- | --- |
| One machine | direct | 1.2 s | 162 of 160 | 0 | 1 ms | HOLDS |
| Cloud station to a PC in Austin, 90 s | relay (use1, n0) | 0.57 s | 722 of 720 | 0 | 248 ms | HOLDS |
| PC on home broadband to phone on mobile data | direct, hole-punched | | | | | to run |

The cloud leg is relay-only by construction: the cloud machine has no UDP out. The phone leg is the one that tests hole punching through a carrier's NAT.
