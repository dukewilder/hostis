#!/usr/bin/env python3
"""Writes an unsigned draft of the bearings from what this build publishes: the first doors,
the open players, the IPNS name, and every published file by its IPFS name. The release keys
sign it with `release sign-paper`; a Lantern holding the Releases deputy then carries it.

  bearings.py <radio dir> <site's pyrateradio dir>

No rights reserved."""
import json, os, sys, time
R, site = sys.argv[1], sys.argv[2]
doors = open(f"{R}/app/assets/bootstrap.txt").read().split()
players = [l.strip() for l in open(f"{R}/app/assets/open_players.txt") if l.strip()]
ipns = open(f"{R}/browser/ipns.txt").read().strip()
files = []
p = f"{site}/ipfs/files.txt"
if os.path.exists(p):
    for l in open(p):
        if l.strip():
            cid, name = l.split(maxsplit=1)
            files.append({"name": name.strip(), "cid": cid})
doc = {"pyrate": "bearings", "at": int(time.time() * 1000), "doors": doors, "players": players, "ipns": ipns, "onion": "", "files": files}
print(json.dumps(doc, indent=1))
