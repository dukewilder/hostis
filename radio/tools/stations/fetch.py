#!/usr/bin/env python3
# Fetches the recordings a manifest names into a Lantern seed folder, checks each against its
# SHA-1 (or, where its host gives none, its size), and writes its credit and licence beside it
# (Name.source.txt), as the Lantern reads them. A recording is named by its archive.org item and
# file, or by a "url" on another host; such a file keeps its own extension (.ogg, .mp3). A file
# already here and whole is left alone, so a second run only fills the gaps.
#   fetch.py <manifest.json> <seed folder>
# No rights reserved.
import hashlib, json, os, sys, time, urllib.parse, urllib.request

# Wikimedia and the Library of Congress refuse Python's default agent (HTTP 403). Say who asks.
_o = urllib.request.build_opener()
_o.addheaders = [("User-Agent", "PyrateRadio-fetch/1.0 (https://hostis.black/pyrateradio/; free-to-air recordings for a HOSTIS station)")]
urllib.request.install_opener(_o)

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
    if "url" in x:
        url = x["url"]
        ext = os.path.splitext(urllib.parse.urlparse(url).path)[1].lower() or ".mp3"
    else:
        url = f"https://archive.org/download/{x['item']}/{urllib.parse.quote(x['file'])}"
        ext = ".mp3"
    path = os.path.join(out, name + ext)
    def whole():
        if not os.path.exists(path):
            return False
        if x.get("sha1"):
            return hashlib.sha1(open(path, "rb").read()).hexdigest() == x["sha1"]
        return not x.get("size") or os.path.getsize(path) == x["size"]
    if not whole():
        for attempt in range(4):
            try:
                # Wikimedia and the Library of Congress limit how fast one asker may take files.
                if "archive.org" not in url:
                    time.sleep(4)
                urllib.request.urlretrieve(url, path + ".part")
                os.replace(path + ".part", path)
                break
            except Exception as e:
                print(f"  {name}: {e}; again shortly")
                time.sleep(30 * (attempt + 1) if "429" in str(e) else 5 * (attempt + 1))
        if not whole():
            print(f"FAILED {name}")
            bad += 1
            for p in (path, path + ".part"):
                if os.path.exists(p):
                    os.remove(p)
            continue
    lic = x["licence"]
    src = x.get("source") or f"https://archive.org/details/{x['item']}"
    if src not in lic:
        lic = f"{lic} {src}"
    open(os.path.join(out, name + ".source.txt"), "w", encoding="utf-8").write(
        f"{x['title']} by {x['by']}, from {x['album']}. {lic}\n")
    ok += 1
print(f"{ok} recordings in {out}, {bad} failed")
