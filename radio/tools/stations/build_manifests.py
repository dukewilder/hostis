#!/usr/bin/env python3
# Writes the manifests that fetch.py follows for HOSTIS's archive-built stations: every
# recording by its archive.org item, file, checksum, licence and credit, chosen once here so
# that the Lantern that keeps the station fetches exactly these and nothing else.
#   build_manifests.py        (writes netlabel.json and poets.json beside this script)
# No rights reserved.
import html, json, os, re, urllib.request

HERE = os.path.dirname(os.path.abspath(__file__))

# Netlabel: Creative Commons electronic, ambient and experimental, from long-running netlabels
# whose releases archive.org keeps with their licences.
NETLABEL = ["mtcomp001", "mtcomp002", "mtcomp003", "obcomp001", "NS050", "DWK031", "DWK315",
            "pcr089EmilDavydov-Sketches", "LBN003_-_Elisa_Luu_-_The_time_of_waiting", "afm004_allmyfaults_neonoir",
            "afm020_noctiflora_0407", "csr020", "Vkrsnl037CandlegravityAMomentForMyself"]
# Poets' Corner: LibriVox's short poetry collections, read by volunteers into the public domain.
POETS = ["short_poetry_001_librivox", "shortpoetry_002_librivox", "shortpoetry_005_librivox", "shortpoetry_043_librivox",
         "short_poetry_collection_082_librivox", "spc276_2606_librivox", "spc277_2607_librivox"]


def meta(item):
    with urllib.request.urlopen(f"https://archive.org/metadata/{item}", timeout=60) as r:
        return json.load(r)


def seconds(v):
    if not v:
        return 0
    if ":" in str(v):
        p = [float(x) for x in str(v).split(":")]
        return sum(x * 60 ** i for i, x in enumerate(reversed(p)))
    return float(v)


def clean(s):
    s = re.sub(r"\s+", " ", html.unescape(str(s or ""))).strip()
    # A Lantern's seed folder is on Linux: only a slash cannot stand in a file name.
    return s.replace(chr(92), "").replace("/", "-").replace(chr(0), "")[:90]


def take(items, kind, max_tracks, min_s, max_s):
    out = []
    for item in items:
        m = meta(item)
        md = m["metadata"]
        lic = md.get("licenseurl", "")
        if kind == "music" and "creativecommons.org" not in lic:
            print("skip, no CC licence:", item)
            continue
        if kind == "poems" and "publicdomain" not in lic:
            print("skip, not public domain:", item)
            continue
        album = clean(md.get("title"))
        album_by = clean(md.get("creator") if isinstance(md.get("creator"), str) else (md.get("creator") or [""])[0])
        files = [f for f in m["files"] if f.get("source") == "original" and f["name"].lower().endswith(".mp3")]
        if kind == "poems":
            files = [f for f in m["files"] if f["name"].endswith("_64kb.mp3")]
        n = 0
        for f in sorted(files, key=lambda f: f["name"]):
            length = seconds(f.get("length"))
            if not (min_s <= length <= max_s):
                continue
            title = clean(f.get("title")) or clean(os.path.splitext(f["name"])[0])
            by = clean(f.get("artist") or f.get("creator") or album_by)
            by = re.sub(r"-monotonik$", "", by)
            # Titles as files were named: underscores, track numbers and the artist's own name.
            title = re.sub(r"^\d{1,3}\.\s*", "", title.replace("_", " ")).strip()
            title = re.sub(r"^\d{1,3}\s+-\s+", "", title).strip()
            title = re.sub(r"\s{2,}", " ", title)
            for lead in (by, by.replace("-", " ")):
                if title.lower().startswith(lead.lower() + " - "):
                    title = title[len(lead) + 3:]
            title = re.sub(r"^\d{1,3}\s+-\s+", "", title).strip()
            if not title or "\u00c3" in title or "\u00c3" in by:
                continue
            if not by or by.lower() in ("various", "various artists", "none"):
                continue
            out.append({"item": item, "file": f["name"], "sha1": f.get("sha1"), "size": int(f.get("size", 0)), "length": round(length),
                        "by": by, "title": title, "album": album, "licence": lic})
            n += 1
            if n >= max_tracks:
                break
        print(f"{item}: {n} recordings")
    return out


if __name__ == "__main__":
    net = take(NETLABEL, "music", 9, 120, 900)
    json.dump(net, open(f"{HERE}/netlabel.json", "w"), indent=1, ensure_ascii=False)
    poems = take(POETS, "poems", 40, 20, 600)
    json.dump(poems, open(f"{HERE}/poets.json", "w"), indent=1, ensure_ascii=False)
    for name, l in [("netlabel", net), ("poets", poems)]:
        print(f"{name}: {len(l)} recordings, {sum(x['length'] for x in l) / 3600:.1f} hours, {sum(x['size'] for x in l) / 1e6:.0f} MB")
