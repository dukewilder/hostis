#!/usr/bin/env python3
# Fetches the recordings a manifest names from archive.org into a Lantern seed folder, checks
# each against its SHA-1, and writes its credit and licence beside it (Name.source.txt), as the
# Lantern reads them. A file already here and whole is left alone, so a second run only fills
# the gaps.
#   fetch.py <manifest.json> <seed folder>
# No rights reserved.
import hashlib, json, os, sys, time, urllib.parse, urllib.request

manifest, out = sys.argv[1], sys.argv[2]
os.makedirs(out, exist_ok=True)
items = json.load(open(manifest, encoding="utf-8"))
used, ok, bad = set(), 0, 0
for x in items:
    name = f"{x['by']} - {x['title']}"
    n = 2
    while name.lower() in used:
        name = f"{x['by']} - {x['title']} ({n})"
        n += 1
    used.add(name.lower())
    path = os.path.join(out, name + ".mp3")
    def whole():
        return os.path.exists(path) and (not x.get("sha1") or hashlib.sha1(open(path, "rb").read()).hexdigest() == x["sha1"])
    if not whole():
        url = f"https://archive.org/download/{x['item']}/{urllib.parse.quote(x['file'])}"
        for attempt in range(3):
            try:
                urllib.request.urlretrieve(url, path + ".part")
                os.replace(path + ".part", path)
                break
            except Exception as e:
                print(f"  {name}: {e}; again shortly")
                time.sleep(5 * (attempt + 1))
        if not whole():
            print(f"FAILED {name}")
            bad += 1
            if os.path.exists(path):
                os.remove(path)
            continue
    lic = x["licence"]
    open(os.path.join(out, name + ".source.txt"), "w", encoding="utf-8").write(
        f"{x['title']} by {x['by']}, from {x['album']}. {lic} https://archive.org/details/{x['item']}\n")
    ok += 1
print(f"{ok} recordings in {out}, {bad} failed")
