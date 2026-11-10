#!/usr/bin/env python3
"""Makes a .torrent and a magnet link for a file, with the given web seeds, so the file
travels by BitTorrent and its DHT whether or not any website stays up. Prints the magnet.

  torrent.py <file> <out.torrent> [web seed url]...

No rights reserved.
"""
import hashlib, os, sys, urllib.parse

def bencode(x):
    if isinstance(x, int): return b"i%de" % x
    if isinstance(x, bytes): return b"%d:%s" % (len(x), x)
    if isinstance(x, str): return bencode(x.encode())
    if isinstance(x, list): return b"l" + b"".join(bencode(i) for i in x) + b"e"
    if isinstance(x, dict): return b"d" + b"".join(bencode(k) + bencode(x[k]) for k in sorted(x, key=lambda k: k.encode() if isinstance(k, str) else k)) + b"e"
    raise TypeError(type(x))

def main():
    path, out, seeds = sys.argv[1], sys.argv[2], sys.argv[3:]
    piece = 256 * 1024
    data = open(path, "rb").read()
    pieces = b"".join(hashlib.sha1(data[i:i + piece]).digest() for i in range(0, len(data), piece))
    name = os.path.basename(path)
    info = {"name": name, "length": len(data), "piece length": piece, "pieces": pieces}
    trackers = ["udp://tracker.opentrackr.org:1337/announce", "udp://open.demonii.com:1337/announce", "udp://tracker.torrent.eu.org:451/announce"]
    t = {"info": info, "announce": trackers[0], "announce-list": [[u] for u in trackers], "comment": "Pyrate Radio. No rights reserved.", "created by": "Pyrate Radio"}
    if seeds:
        t["url-list"] = seeds
    open(out, "wb").write(bencode(t))
    ih = hashlib.sha1(bencode(info)).hexdigest()
    q = [("xt", "urn:btih:" + ih), ("dn", name)] + [("tr", u) for u in trackers] + [("ws", u) for u in seeds]
    print("magnet:?" + "&".join(k + "=" + urllib.parse.quote(v, safe=":") for k, v in q))

main()
