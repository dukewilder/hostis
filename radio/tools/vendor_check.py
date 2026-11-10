#!/usr/bin/env python3
"""Checks the third-party files the browser player ships as they are (libopus in wasm and the
Opus decoder) against the fingerprints recorded in tools/vendor.lock.json, so a file swapped in
the repo, or by a compromised build, fails the build instead of reaching listeners. After a
deliberate upgrade, run with --record. No rights reserved."""
import hashlib, json, os, sys
R = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
LOCK = os.path.join(R, "tools", "vendor.lock.json")
lock = json.load(open(LOCK))
if "--record" in sys.argv:
    lock = {f: hashlib.sha256(open(os.path.join(R, f), "rb").read()).hexdigest() for f in lock}
    json.dump(lock, open(LOCK, "w"), indent=1); open(LOCK, "a").write("\n"); print("recorded"); sys.exit(0)
bad = [f for f, h in lock.items() if hashlib.sha256(open(os.path.join(R, f), "rb").read()).hexdigest() != h]
for f in bad: print("CHANGED", f)
print("vendored files match" if not bad else "vendored files do not match their fingerprints")
sys.exit(1 if bad else 0)
