# Builds hostis.black/pyrateradio/ into landing/out: the landing page, the
# browser radio at /listen/, and the app. Fonts and the sheet are the HOSTIS site's own
# (brand/), so the radio reads as one more HOSTIS page.
import re, os, shutil, sys, hashlib, subprocess
R = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
# The page names the app it offers, so it is built only with one: build.py <apk> <version>.
# A draft for looking at, with placeholders in place of the app, takes --draft instead.
if len(sys.argv) < 3 and "--draft" not in sys.argv:
    sys.exit("build.py <apk> <version>, or --draft for a local look with no app")
apk = sys.argv[1] if len(sys.argv) > 2 else None
version = sys.argv[2] if len(sys.argv) > 2 else "draft"
out = f"{R}/landing/out"
sys.path.insert(0, f"{R}/brand")
import fonts
if os.path.exists(out): shutil.rmtree(out)
os.makedirs(out)
page = open(f"{R}/landing/index.html", encoding="utf-8").read().replace("/*FONTS*/", fonts.faces()).replace("/*HOSTIS*/", fonts.sheet())
if apk:
    data = open(apk, "rb").read()
    sha, size = hashlib.sha256(data).hexdigest(), f"{len(data)/1e6:.0f}"
    shutil.copy(apk, f"{out}/pyrate-radio-{version}.apk")
    # The app by BitTorrent too: a torrent with this site as a web seed, and its magnet link.
    magnet = subprocess.run([sys.executable, f"{R}/tools/torrent.py", f"{out}/pyrate-radio-{version}.apk", f"{out}/pyrate-radio-{version}.apk.torrent",
                             f"https://hostis.black/pyrateradio/pyrate-radio-{version}.apk"], capture_output=True, text=True, check=True).stdout.strip()
    
else:
    sha, size, magnet = "(build pending)", "?", "#"
import json
op = json.load(open(f"{R}/browser/www/open.json"))
page = page.replace("{{OPEN_ORIGIN}}", op["players"][0].rstrip("/")).replace("{{OPEN_PLAYERS}}", json.dumps(op["players"])).replace("{{HARBOUR_IDS}}", json.dumps([h["id"] for h in json.load(open(f"{R}/browser/harbour.json", encoding="utf-8"))]))
# Every other copy of the open player: its IPFS names and the Lanterns in browser/mirrors.txt.
page = page.replace("{{MIRRORS}}", "<br>".join(f'<a href="{u}" target="_blank" rel="noopener noreferrer">{u.split("://", 1)[1].rstrip("/")}</a>' for u in op["players"]))
page = page.replace("{{VERSION}}", version).replace("{{SHA}}", sha).replace("{{SIZE}}", size).replace("{{MAGNET}}", magnet.replace("&", "&amp;"))
# The computers' builds, when tools/fetch_desktop.sh has brought them: each with its checksum
# and a torrent that seeds from this site.
desk, desk_sums = "", ""
D = f"{R}/landing/desktop"
appver = re.search(r"^version:\s*([0-9.]+)", open(f"{R}/app/pubspec.yaml").read(), re.M).group(1)
kinds = [("windows", "pyrate-radio-windows.zip", "Windows", "10 and 11 · zip"), ("macos", "pyrate-radio-macos.zip", "Mac", "Apple and Intel · zip"), ("linux", "pyrate-radio-linux-x64.tar.gz", "Linux", "x86-64 · tar.gz")]
if os.path.isdir(D) and all(os.path.exists(f"{D}/{k[1]}") for k in kinds):
    links, sums = [], []
    for key, f, label, how in kinds:
        name = f.replace("pyrate-radio-", f"pyrate-radio-{appver}-")
        shutil.copy(f"{D}/{f}", f"{out}/{name}")
        sha_d = hashlib.sha256(open(f"{out}/{name}", "rb").read()).hexdigest()
        subprocess.run([sys.executable, f"{R}/tools/torrent.py", f"{out}/{name}", f"{out}/{name}.torrent", f"https://hostis.black/pyrateradio/{name}"], capture_output=True, text=True, check=True)
        links.append(f'<a href="{name}"><b>{label}</b><small>{how}</small></a>')
        sums.append(f"{label.lower()} {sha_d} · <a href=\"{name}.torrent\">torrent</a><br>")
    desk = ('<h3 class="small">On a computer</h3>\n<div class="desk">' + "".join(links) + '</div>\n'
            '<p class="note">Version ' + appver + '. On a Mac, open it the first time with a right-click and Open. Computers take new versions from this page.</p>')
    desk_sums = "".join(sums)
page = page.replace("{{DESKTOP}}", desk).replace("{{DESKTOP_SUMS}}", desk_sums)
# Structured data for search engines: the app on the landing (free, no ratings claimed), and
# where each page sits under HOSTIS. Kept out of `head` below, so the pages beside it carry
# their own trail and not the app.
def ld(*objs):
    return "".join('<script type="application/ld+json">' + json.dumps(o, ensure_ascii=False).replace("</", "<\\/") + "</script>\n" for o in objs)
def crumbs(*trail):
    return {"@context": "https://schema.org", "@type": "BreadcrumbList", "itemListElement": [
        {"@type": "ListItem", "position": i + 1, "name": n, "item": u} for i, (n, u) in enumerate(trail)]}
HOME, PR = ("HOSTIS", "https://hostis.black/"), ("Pyrate Radio", "https://hostis.black/pyrateradio/")
app_ld = {"@context": "https://schema.org", "@type": "SoftwareApplication", "name": "Pyrate Radio", "url": PR[1],
          "description": __import__("html").unescape(re.search(r'<meta name="description" content="([^"]*)">', page).group(1)),
          "applicationCategory": "MultimediaApplication",
          "operatingSystem": ", ".join(["Android"] + (["Windows", "macOS", "Linux"] if desk else []) + ["Web"]),
          "inLanguage": "en-GB", "isAccessibleForFree": True,
          "offers": {"@type": "Offer", "price": "0", "priceCurrency": "USD"},
          "publisher": {"@type": "Organization", "name": "HOSTIS", "url": HOME[1]}}
if apk:
    app_ld.update(softwareVersion=version, fileSize=f"{size} MB", downloadUrl=f"https://hostis.black/pyrateradio/pyrate-radio-{version}.apk")
open(f"{out}/index.html", "w", encoding="utf-8").write(page.replace("</head>", ld(app_ld, crumbs(HOME, PR)) + "</head>", 1))
# The pages beside it: the same head and footer, a body of their own.
head = page[:page.index("<body>")]
flag = page[page.index('<nav class="flagbar"'):page.index("</nav>", page.index('<nav class="flagbar"')) + 6]
foot = page[page.index("<footer>"):page.index("</footer>") + 9]
desc = re.search(r'<meta name="description" content="([^"]*)">', head).group(1)
for slug, body, title, about in [
    ("handbook", "handbook.body.html", "The Keeper's Handbook", "What Pyrate Radio does to keep a station keeper unknown, what only the keeper can do, and how the network stands."),
    ("how-it-works", "how.body.html", "Nobody Can Switch It Off · How Pyrate Radio Works", "No server and nobody in charge. How Pyrate Radio carries every station, keeps listeners and keepers unseen, and what it would take to stop it."),
]:
    h = head.replace('"https://hostis.black/pyrateradio/"', f'"https://hostis.black/pyrateradio/{slug}/"')
    h = re.sub(r"<title>.*?</title>", f"<title>{title} · Pyrate Radio · HOSTIS</title>", h)
    h = h.replace(f'content="{desc}"', f'content="{about}"').replace('content="Pyrate Radio"', f'content="{title}"')
    # The site's own skull at the root: deploy_site.sh ships no copy beside these pages.
    h = h.replace('href="favicon.png"', 'href="/favicon.png"').replace('href="apple-touch-icon.png"', 'href="/apple-touch-icon.png"')
    h = h.replace("</head>", ld(crumbs(HOME, PR, (title.split(" · ")[-1], f"https://hostis.black/pyrateradio/{slug}/"))) + "</head>", 1)
    text = open(f"{R}/landing/{body}", encoding="utf-8").read().replace('href="./', 'href="../').replace('href="../"', 'href="../"').replace("{{OPEN_PLAYER}}", op["players"][0])
    os.makedirs(f"{out}/{slug}", exist_ok=True)
    open(f"{out}/{slug}/index.html", "w", encoding="utf-8").write(h + '<body>\n<a class="skip-link" href="#main">Skip to content</a>\n' + flag + "\n" + text + foot + "\n</body>\n</html>\n")
shutil.copytree(f"{R}/browser/www/dist", f"{out}/listen")
# The HOSTIS skull, the same icon as every sibling page.
for f in ["favicon.png", "apple-touch-icon.png"]:
    shutil.copy(f"{R}/../{f}", out)
print("site at", out, "·", sha[:16])
