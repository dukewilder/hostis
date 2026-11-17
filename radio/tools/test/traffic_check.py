#!/usr/bin/env python3
"""Reads one packet capture of the test network as a watcher on the line would.

Usage: traffic_check.py <capture.pcap> <ndpiReader> <name>

Three checks, one PASS or FAIL line each:
  - nDPI names every flow as generic QUIC, HTTP/3, TLS, HTTP (WebSocket included, since the
    test relay speaks plain HTTP where a real one speaks TLS), the BitTorrent DHT or ICMP. A
    flow nDPI cannot name, or names as something else from a handful of bytes, passes only when
    it is QUIC that carries nothing to name it by: every UDP packet in it a QUIC packet (a
    short header, or a long one of QUIC version 1 or 2), and either two or more of them or a
    socket that began a QUIC handshake nDPI named. A TCP
    flow whose start the capture never saw (a relay connection opened before it) is counted
    apart, as nobody, nDPI included, can name it from its middle.
  - every QUIC handshake offers only h3 and h3-29, and names no server.
  - no byte of the capture says "pyrate" in any case. The bodies of plain HTTP answers are left
    out: a test mirror serves a seed list over plain HTTP, where a real mirror serves it over
    TLS, and the list itself says what it is. Their heads are read.
Notes (not failures): how many flows of each kind, and how often iroh names itself in the
clear (the test relay's handshake, in plain HTTP here and inside TLS on a real relay).
Exits 1 on any FAIL.
"""
import collections
import re
import struct
import subprocess
import sys


def packets(path):
    """(proto, src, sport, dst, dport, payload) for each TCP or UDP packet over IPv4 or IPv6."""
    with open(path, "rb") as f:
        head = f.read(24)
        magic = struct.unpack("<I", head[:4])[0]
        endian = "<" if magic in (0xA1B2C3D4, 0xA1B23C4D) else ">"
        link = struct.unpack(endian + "I", head[20:24])[0]
        while True:
            h = f.read(16)
            if len(h) < 16:
                return
            _, _, incl, _ = struct.unpack(endian + "IIII", h)
            data = f.read(incl)
            off = 14 if link == 1 else 16 if link == 113 else 0
            if link == 1:
                etype = struct.unpack(">H", data[12:14])[0]
            elif link == 113:
                etype = struct.unpack(">H", data[14:16])[0]
            else:
                etype = 0x0800 if data and data[0] >> 4 == 4 else 0x86DD
            ip = data[off:]
            if etype == 0x0800 and len(ip) >= 20:
                ihl = (ip[0] & 15) * 4
                proto, src, dst = ip[9], ".".join(map(str, ip[12:16])), ".".join(map(str, ip[16:20]))
                l4 = ip[ihl:]
            elif etype == 0x86DD and len(ip) >= 40:
                proto, src, dst = ip[6], ip[8:24].hex(), ip[24:40].hex()
                l4 = ip[40:]
            else:
                continue
            if proto == 17 and len(l4) >= 8:
                sp, dp = struct.unpack(">HH", l4[:4])
                yield ("udp", src, sp, dst, dp, l4[8:], 0)
            elif proto == 6 and len(l4) >= 20:
                sp, dp = struct.unpack(">HH", l4[:4])
                yield ("tcp", src, sp, dst, dp, l4[(l4[12] >> 4) * 4:], l4[13])


def quic_packet(x):
    """Whether bytes are a QUIC packet: a short header (fixed bit, form bit clear), or a long
    header of QUIC version 1 or 2."""
    return len(x) >= 1 and (x[0] & 0xC0 == 0x40 or (x[0] & 0xC0 == 0xC0 and len(x) >= 5 and x[1:5] in (b"\0\0\0\1", b"\x6b\x33\x43\xcf")))


def ndpi_flows(reader, path):
    out = subprocess.run([reader, "-i", path, "-v", "2", "-q"], capture_output=True, text=True).stdout
    out += subprocess.run([reader, "-i", path, "-v", "2"], capture_output=True, text=True).stdout if "[proto:" not in out else ""
    flows = {}
    for m in re.finditer(r"^\s*\d+\s+(TCP|UDP|ICMP\S*)\s+(\S+):(\d+)\s+<?->\s+(\S+):(\d+)\s+\[proto: [\d.]+/([^\]]+)\]", out, re.M):
        kind, a, ap, b, bp, name = m.groups()
        flows[(kind.lower(), a, int(ap), b, int(bp))] = name
    return flows


def main():
    path, reader, name = sys.argv[1], sys.argv[2], sys.argv[3]
    fail = False

    def say(ok, what):
        nonlocal fail
        print(("PASS " if ok else "FAIL ") + f"{name}: {what}")
        fail |= not ok

    pkts = list(packets(path))
    flows = ndpi_flows(reader, path)
    kinds = collections.Counter(flows.values())
    good = re.compile(r"^(QUIC|HTTP3|HTTP_3|TLS|HTTP|HTTP\.WebSocket|BitTorrent|ICMP|ICMPV6)$", re.I)
    quic_socks = {(a, ap) for (k, a, ap, b, bp), n in flows.items() if n.upper() in ("QUIC", "HTTP3")} | {(b, bp) for (k, a, ap, b, bp), n in flows.items() if n.upper() in ("QUIC", "HTTP3")}
    by_flow = collections.defaultdict(list)
    syn = set()
    for p in pkts:
        key = frozenset([(p[1], p[2]), (p[3], p[4])])
        by_flow[(p[0], key)].append(p[5])
        if p[0] == "tcp" and p[6] & 0x02:
            syn.add(key)
    bad, quiet, before = [], 0, 0
    for (k, a, ap, b, bp), n in flows.items():
        if good.match(n):
            continue
        key = frozenset([(a, ap), (b, bp)])
        payloads = [x for x in by_flow.get((k, key), []) if x]
        if k == "tcp" and key not in syn:
            before += 1
            continue
        shaped = k == "udp" and payloads and all(quic_packet(x) for x in payloads)
        if shaped and (len(payloads) >= 2 or (a, ap) in quic_socks or (b, bp) in quic_socks):
            quiet += 1
            continue
        bad.append(f"{k} {a}:{ap} <-> {b}:{bp} as {n}")
    say(bool(flows) and not bad, f"{len(flows)} flows, every one generic QUIC, HTTP/3, TLS, HTTP, the DHT or ICMP ({dict(kinds)}; {quiet} of them QUIC flows nDPI could not name, {before} begun before the capture){'; ' + '; '.join(bad[:6]) if bad else ''}")

    hello = subprocess.run(["tshark", "-r", path, "-Y", "tls.handshake.type==1 && quic", "-T", "fields", "-e", "tls.handshake.extensions_alpn_str", "-e", "tls.handshake.extensions_server_name"], capture_output=True, text=True).stdout
    offers = collections.Counter(l.strip() for l in hello.splitlines() if l.strip() or l)
    wrong = [o for o in offers if o.split("\t")[0] not in ("h3", "h3,h3-29") or (len(o.split("\t")) > 1 and o.split("\t")[1])]
    say(not wrong, f"{sum(offers.values())} QUIC handshakes offer only h3 and h3-29 and name no server ({dict(offers)})")

    hits, iroh, skipped = [], 0, 0
    body = set()
    for proto, src, sp, dst, dp, pl, _ in pkts:
        d = (src, sp, dst, dp)
        if proto == "tcp" and pl.startswith(b"HTTP/1."):
            head, _, rest = pl.partition(b"\r\n\r\n")
            if b" 101 " not in pl[:16]:
                body.add(d)
            else:
                body.discard(d)
            look = head
            skipped += len(rest)
        elif proto == "tcp" and d in body and not pl.startswith((b"GET ", b"POST ", b"HEAD ", b"PUT ")):
            skipped += len(pl)
            continue
        else:
            look = pl
        if re.search(rb"(?i)pyrate", look):
            hits.append(f"{proto} {src}:{sp} -> {dst}:{dp}")
        iroh += len(re.findall(rb"(?i)iroh", look))
    say(len(pkts) > 0 and not hits, f"{len(pkts)} packets, none says Pyrate ({skipped} bytes of plain HTTP answers' bodies left out){'; ' + '; '.join(hits[:4]) if hits else ''}")
    print(f"NOTE {name}: iroh named in the clear {iroh} times (the test relay's plain-HTTP handshake; inside TLS on a real relay)")
    return 1 if fail else 0


if __name__ == "__main__":
    sys.exit(main())
