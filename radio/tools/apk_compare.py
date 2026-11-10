#!/usr/bin/env python3
"""Compares two APKs by what they hold, leaving out their signatures, so an APK built here can
be checked against one built from the same source on another machine before it is signed.

  apk_compare.py <a.apk> <b.apk>

Prints every entry that differs and exits 1 if any does. No rights reserved."""
import sys, zipfile, hashlib
SIG = ("META-INF/MANIFEST.MF",)
def entries(p):
    with zipfile.ZipFile(p) as z:
        return {i.filename: hashlib.sha256(z.read(i)).hexdigest() for i in z.infolist()
                if not (i.filename.startswith("META-INF/") and i.filename.rsplit(".", 1)[-1] in ("SF", "RSA", "EC", "DSA") or i.filename in SIG)}
a, b = entries(sys.argv[1]), entries(sys.argv[2])
diff = sorted(n for n in set(a) | set(b) if a.get(n) != b.get(n))
for n in diff:
    print(("only in first " if n not in b else "only in second " if n not in a else "differs ") + n)
print(f"{len(a)} entries compared; " + ("the same" if not diff else f"{len(diff)} differ"))
sys.exit(1 if diff else 0)
