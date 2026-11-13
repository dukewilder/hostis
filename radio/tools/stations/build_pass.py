#!/usr/bin/env python3
# The November 2026 stations pass: writes netlabel.json, poets.json and contraband.json beside
# this script, the manifests fetch.py follows. Every item's licence is read from its archive.org
# page and checked against what HOSTIS airs: the public domain, CC0, CC BY and CC BY-SA.
# Anything else is refused and the run stops.
#   build_pass.py
# No rights reserved.
import html, json, os, re, sys, time, urllib.request

HERE = os.path.dirname(os.path.abspath(__file__))
FREE = re.compile(r"^https?://creativecommons\.org/(licenses/(by|by-sa)/[0-9.]+(/[a-z]{2})?|publicdomain/(zero|mark)/1\.0|licenses/publicdomain)/?$")
NAMES = {"by": "CC BY", "by-sa": "CC BY-SA", "zero": "CC0", "mark": "Public domain", "publicdomain": "Public domain"}


def meta(item):
    for attempt in range(4):
        try:
            with urllib.request.urlopen(f"https://archive.org/metadata/{item}", timeout=60) as r:
                return json.load(r)
        except OSError:
            if attempt == 3:
                raise
            time.sleep(5 * (attempt + 1))


def seconds(v):
    v = str(v or 0)
    if ":" in v:
        return sum(float(x) * 60 ** i for i, x in enumerate(reversed(v.split(":"))))
    return float(v)


def licence(md, item):
    lic = md.get("licenseurl", "")
    if not FREE.match(lic):
        sys.exit(f"{item}: {lic or 'no licence'} is not free to air")
    desc = re.sub(r"<[^>]+>", " ", html.unescape(md.get("description") if isinstance(md.get("description"), str) else " ".join(md.get("description") or [])))
    if re.search(r"non-?commercial|by-nc|by-nd|no ?deriv|all rights reserved", desc, re.I):
        sys.exit(f"{item}: its page says otherwise in its description")
    kind = next(k for k in ("zero", "mark", "by-sa", "by", "publicdomain") if f"/{k}/" in lic)
    ver = re.search(r"/([0-9.]+)(/[a-z]{2})?/?$", lic)
    return lic, f"{NAMES[kind]}{' ' + ver.group(1) if ver and kind in ('by', 'by-sa') else ''}"


def clean(s):
    s = re.sub(r"\s+", " ", html.unescape(str(s or ""))).strip()
    return s.replace(chr(92), "").replace("/", "-").replace(chr(0), "")[:90]


def entry(item, f, by, title, album, lic, name):
    return {"item": item, "file": f["name"], "sha1": f.get("sha1"), "size": int(f.get("size", 0)), "length": round(seconds(f.get("length"))),
            "by": by, "title": title, "album": album, "licence": f"{name}: {lic}"}


# Netlabel. Each release by its archive.org item, the files taken from it (by number, as the
# label numbered them), fixes for what its tags got wrong, and whether the programme plays it
# through the small hours (folders.py reads that list).
NETLABEL = [
    ("mt001", "02 04", {}, True), ("mt003", "01 03 04 05", {}, True), ("mt005", "01 02 03", {}, True),
    ("mt007", "01 03 04 08 11", {}, True), ("mt010", "01 02 03 05 06 08", {"Kraftim": "KraftiM"}, True),
    ("Seetyca_Suite", "01 02 03 05", {}, True),
    ("Jade_Furnace_I", "02 04 05 08 12 14 16 20", {"K.M.Krebs": "K.M. Krebs"}, True), ("Generative_I", "02 03 04 05", {}, True),
    ("alg010", "06 07 14", {"": "aNACRUSa"}, True), ("ndn025", "02 05 08", {}, True), ("bof065", "04", {}, True),
    ("USR-019", "01 02 04 05 08 09", {}, False), ("EAR014", "01 02 04 05 07 09 10", {}, False),
    ("gruen011", "01 02 03 04", {}, False), ("Monokrak214FloatingMind_Hidden_Passion", "Acid_Swimm Acidict Deconstruction", {"FloatingMind": "Floating Mind"}, False),
    ("MNF022_DCM--Elevacion", "01 02 03 04", {}, False), ("In_Vitro_-_Phantastico_Dogma_Remixes", "01 02 05 06", {}, False),
    ("onmp071", "01 02 05 08 11", {}, False), ("tng1003", "01 02 03 04", {"Zenlu": "ZenLu"}, False),
    ("gt373TheRogueSources-Honey", "1. 2. 4.", {}, False), ("gt422Naugolnik-ExoticPassion", "1. 3. 4. 9.", {"*": "Naugolnik"}, False),
]


def picked(name, keys):
    base = os.path.basename(name)
    for k in keys.split():
        if re.match(r"^\d+\.$", k):
            if base.startswith(k):
                return True
        elif k.isdigit():
            if re.search(rf"(^|[_\-]){k}[_\-.]", base):
                return True
        elif k in base:
            return True
    return False


def netlabel():
    out = []
    for item, keys, fix, is_night in NETLABEL:
        m = meta(item)
        md = m["metadata"]
        lic, name = licence(md, item)
        album = clean(md.get("title"))
        files = [f for f in m["files"] if f.get("source") == "original" and f["name"].lower().endswith(".mp3") and picked(f["name"], keys)]
        for f in sorted(files, key=lambda f: f["name"]):
            by = clean(f.get("artist") or md.get("creator") or "")
            by = fix.get("*", fix.get(by, by)) or by
            title = clean(f.get("title") or os.path.splitext(f["name"])[0])
            title = re.sub(r"^\d{1,2}\s*[.\-]\s*", "", title).strip()
            if item == "alg010" and "verdugo" in title:
                title = "el sueño del verdugo"
            out.append(entry(item, f, by, title, album, lic, name))
    return out


# Poets' Corner keeps its recordings and gets its credits right: poets by their full names,
# titles as the poets gave them, and one reading of each poem.
POET = {"G. M. Hopkins": "Gerard Manley Hopkins", "H. W. Longfellow": "Henry Wadsworth Longfellow", "W. Owen": "Wilfred Owen",
        "C. Lamb": "Charles Lamb", "E.A. Poe": "Edgar Allan Poe", "Edgar Allen Poe": "Edgar Allan Poe", "H. Doolittle": "H.D.",
        "S. Coleridge": "Samuel Taylor Coleridge", "T. Peacock": "Thomas Love Peacock", "C. Rossetti": "Christina Rossetti",
        "E. Millay": "Edna St. Vincent Millay", "Helen Selina, Lady Dufferin": "Lady Dufferin", "Walter de la Mere": "Walter de la Mare",
        "Benjanim Franklin": "Benjamin Franklin", "Thomas Wharton": "Thomas Warton", "Lady Winchilsea": "Anne Finch",
        "Michael Angelo Buonarroti": "Michelangelo", "Felicia Dorothea Hemans": "Felicia Hemans", "Richard le Gallienne": "Richard Le Gallienne",
        "Mewlana Jalaluddin Rumi": "Rumi", "Alfred, Lord Tennyson": "Alfred Tennyson", "George Gordon, Lord Byron": "Lord Byron",
        "Robert W. Service": "Robert Service"}
RETITLE = {"Kublai Khan": "Kubla Khan", "Dulce et Decorum": "Dulce et Decorum Est", "Wreck Of The Hesperus": "The Wreck of the Hesperus",
           "Inscription for the Entrance t": "Inscription for the Entrance to a Wood", "How they Brought the Good News from Ghent to Aix":
           "How They Brought the Good News from Ghent to Aix", "Thou Art not Lovelier than Lilacs No": "Thou Art Not Lovelier Than Lilacs",
           "The Happiest Day": "The Happiest Day, the Happiest Hour", "Washington’s Monument, February, 1885": "Washington's Monument, February, 1885",
           "Oh Captain! My Captain!": None, "The Tiger": None}
# Second readings, and a title cut off in the catalogue.
DROP = {("shortpoetry_002_librivox", "Annabel Lee"), ("shortpoetry_005_librivox", "The Lady of Shalott"),
        ("short_poetry_001_librivox", "Lament of the Irish Emigrant"), ("spc277_2607_librivox", "Verses Occasion’d by an &c. at the End of Mr. D’Urfy’s Name in the Title to one of hi")}


def poets():
    old = json.load(open(f"{HERE}/poets.json", encoding="utf-8"))
    for item in sorted({x["item"] for x in old}):
        licence(meta(item)["metadata"], item)
    out = []
    for x in old:
        if (x["item"], x["title"]) in DROP or RETITLE.get(x["title"], "") is None:
            continue
        y = dict(x, by=POET.get(x["by"], x["by"]), title=RETITLE.get(x["title"]) or x["title"])
        if not y["licence"].startswith("Public domain"):
            y["licence"] = f"Public domain: {x['licence']}"
        out.append(y)
    return out


# Contraband: what kings, censors and courts tried to stop, read by LibriVox's volunteers. Each
# work by its LibriVox item, the sections taken (all when empty), and the title it airs under.
CONTRABAND = [
    ("areopagitica_0806_librivox", "John Milton", "Areopagitica", None),
    ("commonsense_gg_librivox", "Thomas Paine", "Common Sense", None),
    ("candide_librivox", "Voltaire", "Candide", None),
    ("frederick-douglass_jf_librivox", "Frederick Douglass", "Narrative of the Life of Frederick Douglass", None),
    ("civil_disobedience_librivox", "Henry David Thoreau", "Civil Disobedience", None),
    ("anarchism_otheressays_0908_librivox", "Emma Goldman", "Anarchism and Other Essays", "00 01 02 03 04 05 06"),
]


def contraband():
    out = []
    for item, by, work, keys in CONTRABAND:
        m = meta(item)
        md = m["metadata"]
        lic, name = licence(md, item)
        files = sorted({f["name"]: f for f in m["files"] if f["name"].endswith("_64kb.mp3")}.values(), key=lambda f: f["name"])
        # A book read chapter by chapter is numbered by its chapters; anything else by its order.
        chapters = all(re.match(r"^chapter\s+\d+", clean(f.get("title")), re.I) for f in files)
        n = 0
        for f in files:
            sec = clean(f.get("title") or "")
            num = re.match(r"^(?:chapter\s+)?(\d+)\s*[a-z]?\b", sec, re.I)
            if keys and not (num and f"{int(num.group(1)):02d}" in keys.split()):
                continue
            n += 1
            rest = re.sub(r"^\d+[a-z]?\s*(?:-\s*)?", "", sec).strip()
            rest = re.sub(r"^Chapter 0*(\d)", r"Chapter \1", rest)
            ch = re.match(r"^chapter\s+(\d+)(?:\s*-\s*(.+))?$", sec, re.I)
            if len(files) == 1:
                title = work
            elif chapters and ch:
                title = f"{work} {int(ch.group(1)):02d}" + (f" \u00b7 {ch.group(2)}" if ch.group(2) else "")
            elif rest and not re.match(r"^(section|part)\s*\d*$", rest, re.I):
                title = f"{work} {n:02d} · {rest}"
            else:
                title = f"{work} {n:02d}"
            title = title.replace("Prisons, A Social Crime & Failure", "Prisons: A Social Crime and Failure").replace("\u00b7 Francisco Ferrer", "\u00b7 Francisco Ferrer and the Modern School")
            out.append(entry(item, f, by, clean(title)[:80], clean(md.get("title")), lic, name))
    return out


if __name__ == "__main__":
    net = netlabel()
    po = poets()
    cb = contraband()
    for name, rows in [("netlabel", net), ("poets", po), ("contraband", cb)]:
        keys = [(r["by"] + " - " + r["title"]).lower() for r in rows]
        dup = {k for k in keys if keys.count(k) > 1}
        assert not dup, (name, dup)
        json.dump(rows, open(f"{HERE}/{name}.json", "w", encoding="utf-8"), indent=1, ensure_ascii=False)
        print(f"{name}: {len(rows)} recordings, {sum(r['length'] for r in rows) / 3600:.1f} hours, {sum(r['size'] for r in rows) / 1e6:.0f} MB")
