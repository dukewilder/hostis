#!/usr/bin/env python3
# Brings a seed folder in line with its manifest before fetch.py runs: a recording already here
# under another name (found by its SHA-1) takes the name the manifest gives it, and every
# recording the manifest no longer names moves, with its .source.txt, into the retired folder.
# Nothing is deleted. fetch.py then fetches what is missing and rewrites every credit.
#   tidy.py <manifest.json> <seed folder> <retired folder>
# No rights reserved.
import hashlib, json, os, sys

manifest, folder, retired = sys.argv[1], sys.argv[2], sys.argv[3]
AUDIO = (".mp3", ".ogg", ".flac", ".wav", ".m4a", ".opus", ".oga")

# The names fetch.py gives, in the same order and with the same suffix for a repeat.
want, used = {}, set()
for x in json.load(open(manifest, encoding="utf-8")):
    name = f"{x['by']} - {x['title']}"
    n = 2
    while name.lower() in used:
        name = f"{x['by']} - {x['title']} ({n})"
        n += 1
    used.add(name.lower())
    want[name + ".mp3"] = x.get("sha1")

here = [f for f in os.listdir(folder) if f.lower().endswith(AUDIO) and not f.lower().startswith("bell.")]
sha = {f: hashlib.sha1(open(os.path.join(folder, f), "rb").read()).hexdigest() for f in here if f not in want}
renamed = 0
for name, h in want.items():
    if os.path.exists(os.path.join(folder, name)) or not h:
        continue
    old = next((f for f, s in sha.items() if s == h), None)
    if old:
        os.replace(os.path.join(folder, old), os.path.join(folder, name))
        src = os.path.splitext(old)[0] + ".source.txt"
        if os.path.exists(os.path.join(folder, src)):
            os.makedirs(retired, exist_ok=True)
            os.replace(os.path.join(folder, src), os.path.join(retired, src))
        del sha[old]
        renamed += 1

moved = 0
for f in sha:
    os.makedirs(retired, exist_ok=True)
    for g in (f, os.path.splitext(f)[0] + ".source.txt"):
        if os.path.exists(os.path.join(folder, g)):
            os.replace(os.path.join(folder, g), os.path.join(retired, g))
    moved += 1
print(f"{renamed} renamed, {moved} moved to {retired}, {sum(os.path.exists(os.path.join(folder, n)) for n in want)} of {len(want)} in place")
