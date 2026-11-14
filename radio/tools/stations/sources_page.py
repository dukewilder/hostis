#!/usr/bin/env python3
# Writes landing/sources.body.html from the manifests beside this script: every release, book
# and recording on HOSTIS's own stations, with its licence and a link to where it came from.
# Run after any manifest changes; landing/build.py takes the page from there.
#   sources_page.py
# No rights reserved.
import html, json, os, re

HERE = os.path.dirname(os.path.abspath(__file__))
R = os.path.dirname(os.path.dirname(HERE))
# In dial order, each with the manifests that fill it and a word on what is not in them.
STATIONS = [
    ("Netlabel", ["netlabel"], ""),
    ("Drift", ["drift"], ""),
    ("Concert Hall", ["concert-hall"], ""),
    ("The Public Domain", ["public-domain"], "Every other record here was published in 1925 or earlier, and names its own source in its credit."),
    ("Readings", ["readings"], "Three LibriVox readings come before these. They are Captain Charles Johnson's General History of the Pyrates, The Pirates Own Book and Treasure Island, and all three are in the public domain."),
    ("The Numbers", [], "HOSTIS's own recordings, given to the public domain."),
    ("Poets' Corner", ["poets"], ""),
    ("Shanties", ["shanties"], ""),
    ("Contraband", ["contraband"], ""),
    ("Brass", ["brass"], ""),
]
e = lambda s: html.escape(str(s), quote=True)


def source(x):
    return x.get("source") or f"https://archive.org/details/{x['item']}"


def licence(text):
    """The licence's name, linked where the manifest gives its address."""
    m = re.match(r"^(.*?):\s*(https?://\S+)\s*$", text)
    if m:
        return f'<a href="{e(m.group(2))}" rel="noopener noreferrer">{e(m.group(1))}</a>'
    return e(text)


def rows(names):
    groups = {}
    for n in names:
        for x in json.load(open(f"{HERE}/{n}.json", encoding="utf-8")):
            g = groups.setdefault(source(x), {"album": x["album"], "by": [], "lic": x["licence"], "n": 0, "s": 0, "title": x["title"]})
            if x["by"] not in g["by"]:
                g["by"].append(x["by"])
            g["n"] += 1
            g["s"] += x["length"]
    out = []
    for url, g in groups.items():
        what = g["title"] if g["n"] == 1 and g["album"].startswith("a ") else g["album"]
        by = g["by"][0] if len(g["by"]) == 1 else (", ".join(g["by"]) if len(g["by"]) <= 3 else "various")
        n = f'{g["n"]} recordings' if g["n"] > 1 else "1 recording"
        h = f'{g["s"] / 3600:.1f} h' if g["s"] >= 3600 else f'{round(g["s"] / 60)} min'
        out.append(f'<tr><td><a href="{e(url)}" rel="noopener noreferrer">{e(what)}</a></td><td>{e(by)}</td><td>{licence(g["lic"])}</td><td>{n} &#183; {h}</td></tr>')
    return out


parts = ['<header class="masthead">\n<h1>Sources</h1>\n<p class="mh-sub">Pyrate Radio &#183; what HOSTIS airs</p>\n'
         '<p class="mh-desc">Every recording on HOSTIS\'s own stations is in the public domain or free for anyone to broadcast under its licence. Here is each one, with its licence and where it came from.</p>\n'
         '</header>\n<main id="main" class="long">\n']
for i, (name, files, note) in enumerate(STATIONS):
    slug = re.sub(r"[^a-z]+", "-", name.lower()).strip("-")
    if i:
        parts.append('<div class="divider"><span class="divider-glyph"></span></div>\n')
    r = rows(files)
    total = sum(json.load(open(f"{HERE}/{n}.json", encoding="utf-8")).__len__() for n in files)
    cap = f"{total} recordings" if total else "HOSTIS's own"
    parts.append(f'<section id="{slug}">\n<div class="sechead"><h2>{e(name)}</h2><span class="cap">{cap}</span></div>\n')
    if note:
        parts.append(f'<div class="prose"><p>{e(note)}</p></div>\n')
    if r:
        parts.append('<div class="compare-wrap wide"><table class="compare">\n<thead><tr><th scope="col">Release or work</th><th scope="col">By</th><th scope="col">Licence</th><th scope="col">Length</th></tr></thead>\n<tbody>\n'
                     + "\n".join(r) + '\n</tbody></table></div>\n')
    parts.append('</section>\n')
parts.append('<div class="divider"><span class="divider-glyph"></span></div>\n'
             '<div class="prose"><p>Something here that should not be? Write to <a href="mailto:hostisblack@pm.me">hostisblack@pm.me</a> and say which recording and which station.</p></div>\n'
             '<p class="coda"><a href="./">Back to Pyrate Radio</a></p>\n</main>\n')
open(f"{R}/landing/sources.body.html", "w", encoding="utf-8").write("".join(parts))
print("sources.body.html", sum(len(rows(f)) for _, f, _ in STATIONS), "rows")
