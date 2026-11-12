#!/usr/bin/env python3
# Checks every registry crate in a Cargo.lock against OSV. Usage: python3 -I tools/audit/osv.py Cargo.lock
import json, re, sys, urllib.request
lock = open(sys.argv[1]).read()
pkgs = []
for blk in lock.split('[[package]]')[1:]:
    n = re.search(r'name = "([^"]+)"', blk); v = re.search(r'version = "([^"]+)"', blk)
    src = re.search(r'source = "([^"]+)"', blk)
    if n and v and src and 'registry' in src.group(1):
        pkgs.append((n.group(1), v.group(1)))
print(len(pkgs), "registry packages", file=sys.stderr)
found = {}
for i in range(0, len(pkgs), 500):
    chunk = pkgs[i:i+500]
    q = {"queries": [{"package": {"name": n, "ecosystem": "crates.io"}, "version": v} for n, v in chunk]}
    req = urllib.request.Request("https://api.osv.dev/v1/querybatch", data=json.dumps(q).encode(), headers={"Content-Type": "application/json"})
    res = json.load(urllib.request.urlopen(req, timeout=60))
    for (n, v), r in zip(chunk, res["results"]):
        for vuln in r.get("vulns", []):
            found.setdefault(vuln["id"], []).append(f"{n} {v}")
for vid, who in sorted(found.items()):
    try:
        d = json.load(urllib.request.urlopen(f"https://api.osv.dev/v1/vulns/{vid}", timeout=30))
        summ = d.get("summary") or d.get("details", "")[:120]
        aliases = ",".join(d.get("aliases", []))
        sev = d.get("database_specific", {}).get("informational") or ""
    except Exception as e:
        summ, aliases, sev = f"(detail fetch failed: {e})", "", ""
    print(f"{vid} [{sev}] {aliases} :: {', '.join(who)} :: {summ.strip()[:160]}")
