# Builds the radio as static pages any host can serve, on the HOSTIS sheet, in two copies:
#   www/dist       hostis.black's own: HOSTIS's stations only; every other link goes to an open player
#   www/dist-open  the open player: the whole Log, served from IPFS and Lanterns, owned by nobody
# The open copy is packed as a CAR (www/open.car) under its IPFS name, and that name is what
# hostis.black's copy, the landing and the app send every other station to.
import re, os, shutil, subprocess, sys, hashlib, time
R = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
W = f"{R}/browser/www"
# The vendored Opus files must match their recorded fingerprints before anything ships.
subprocess.run([sys.executable, f"{R}/tools/vendor_check.py"], check=True, stdout=subprocess.DEVNULL)
sys.path.insert(0, f"{R}/brand")
import fonts
# The page is one more HOSTIS page: the site's faces and sheet (brand/), then the set's own.
head = """<!DOCTYPE html>
<html lang="en-GB" class="scope-{{SCOPE}}">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover">
<title>Pyrate Radio · Listen · HOSTIS</title>
<meta name="description" content="Radio that anyone can raise, that every listener carries, and that nobody can switch off. Plays in the browser.">
<meta name="theme-color" content="#030303">
<meta name="color-scheme" content="dark">
<meta property="og:title" content="Pyrate Radio · Listen">
<meta property="og:description" content="Every station on the Log, played in the browser from whoever carries it.">
<meta property="og:site_name" content="HOSTIS">
<meta property="og:image" content="https://hostis.black/og-image.png">
<meta name="twitter:card" content="summary_large_image">
<meta name="twitter:site" content="@hostis_black">
<link rel="icon" type="image/png" sizes="48x48" href="favicon.png">
<link rel="apple-touch-icon" sizes="180x180" href="apple-touch-icon.png">
<link rel="manifest" href="manifest.webmanifest">
<meta name="apple-mobile-web-app-capable" content="yes">
<meta name="mobile-web-app-capable" content="yes">
<meta name="apple-mobile-web-app-title" content="Pyrate Radio">
<meta name="apple-mobile-web-app-status-bar-style" content="black">
<style>
"""
# The faces are files beside the page, not 400 KB inside it, so the page paints before they land
# (font-display: swap). The two the first screen is set in come first: the station's name and
# the masthead in Pica, and every reading on the set in the mono.
face_css, face_files, face_latin = fonts.linked()
first_faces = [face_latin[("IM Fell DW Pica SC", "400", "normal")], face_latin[("JetBrains Mono", "400", "normal")]]
head += face_css + "\n" + fonts.sheet()
extra = "\n" + open(f"{R}/browser/player.css", encoding="utf-8").read()
body = open(f"{R}/browser/body.html", encoding="utf-8").read()
js = open(f"{W}/app.js", encoding="utf-8").read()
# A test build names its own harbour and relays: PYRATE_HARBOUR="<id>@<relay>", PYRATE_RELAYS.
harbour = os.environ.get("PYRATE_HARBOUR") or ",".join(open(f"{R}/app/assets/bootstrap.txt").read().split())
import json
# HOSTIS's own stations: the only ones the landing page's set offers.
# A test build can name its own: PYRATE_HARBOUR_STATIONS=<a json file like harbour.json>.
harbour_stations = json.dumps(json.load(open(os.environ.get("PYRATE_HARBOUR_STATIONS") or f"{R}/browser/harbour.json", encoding="utf-8")), ensure_ascii=False)
# Each value goes in as a JSON string, so nothing in it can close the quote it sits in.
for k in ["'{{HARBOUR}}'", "'{{RELAYS}}'", "'{{HARBOUR_STATIONS}}'", "'{{SCOPE}}'", "'{{OPEN_PLAYERS}}'", "'{{STUN}}'", "'{{DOORS}}'"]:
    assert js.count(k) == 1, k
# Lanterns that serve the open player at /listen/ on names of their own (tools/vps/install.sh
# prints each one's address), one per line in browser/mirrors.txt. Every copy names them all,
# so the player can still be had from any one of them when the rest are gone.
def mirror_list():
    path, out = f"{R}/browser/mirrors.txt", []
    for line in (open(path, encoding="utf-8") if os.path.exists(path) else []):
        u = line.split("#", 1)[0].strip()
        if not u: continue
        u = u if u.endswith("/") else u + "/"
        assert re.fullmatch(r"https://[a-z0-9.-]+(:[0-9]+)?/[A-Za-z0-9._~/-]*", u), f"mirrors.txt: not a plain https address: {u}"
        assert not re.match(r"https://([a-z0-9.-]*\.)?hostis\.black[:/]", u), f"mirrors.txt: a HOSTIS name never serves the open player: {u}"
        if u not in out: out.append(u)
    return out
MIRRORS = mirror_list()
# The STUN servers a tab may ask its own address of, for a direct link to its guard: HOSTIS's
# own, one per line in browser/stun.txt (tools/vps/install.sh prints each), and nobody else's.
# STUN only, never TURN: when no direct link can be made the radio stays on its relay. A test
# build names its own with PYRATE_STUN (comma separated), or none, which leaves only the
# addresses the machine itself knows.
def stun_list():
    raw = os.environ.get("PYRATE_STUN")
    if raw is not None:
        lines = raw.split(",")
    else:
        path = f"{R}/browser/stun.txt"
        lines = open(path, encoding="utf-8").read().splitlines() if os.path.exists(path) else []
    out = []
    for line in lines:
        u = line.split("#", 1)[0].strip()
        if not u: continue
        assert re.fullmatch(r"stuns?:[a-z0-9.-]+(:[0-9]+)?", u), f"stun.txt: not a plain stun: address: {u}"
        assert not re.search(r"(google|l\.google|cloudflare|twilio|stunprotocol|mozilla|ekiga|voip|xten)", u), f"stun.txt: only HOSTIS's own STUN servers: {u}"
        if u not in out: out.append(u)
    return out
STUN = stun_list()
# The doors every copy carries: signed notes (hex), one a line in browser/doors.txt, as a deploy
# takes them from a harbour Lantern (curl <lantern>/doors.txt > browser/doors.txt). A test build
# names its own with PYRATE_DOORS (comma separated). Each is checked again in the tab.
def door_list():
    raw = os.environ.get("PYRATE_DOORS")
    lines = raw.split(",") if raw is not None else (open(f"{R}/browser/doors.txt").read().split() if os.path.exists(f"{R}/browser/doors.txt") else [])
    out = [l.strip().lower() for l in lines if l.split("#", 1)[0].strip()]
    for l in out:
        assert re.fullmatch(r"[0-9a-f]{80,2048}", l), f"doors: not a signed note in hex: {l[:40]}"
    return out[:16]
DOORS = door_list()
mirrors_html = "".join(f'<li><a href="{u}" target="_blank" rel="noopener noreferrer">{u[8:].rstrip("/")}</a></li>' for u in MIRRORS)
base_js = js.replace("'{{HARBOUR}}'", json.dumps(harbour)).replace("'{{RELAYS}}'", json.dumps(os.environ.get("PYRATE_RELAYS", ""))).replace("'{{HARBOUR_STATIONS}}'", harbour_stations).replace("'{{STUN}}'", json.dumps(STUN)).replace("'{{DOORS}}'", json.dumps(DOORS))

def build(scope, dist, open_players):
    out_js = base_js.replace("'{{SCOPE}}'", json.dumps(scope)).replace("'{{OPEN_PLAYERS}}'", json.dumps(open_players))
    v = hashlib.sha256(out_js.encode() + open(f"{W}/pkg/pyrate_browser_bg.wasm", "rb").read()).hexdigest()[:10]
    if os.path.exists(dist): shutil.rmtree(dist)
    os.makedirs(f"{dist}/pkg")
    # ?embed shows the set alone, for the landing page's hero; decided before first paint.
    embed = "<script>if(/[?&]embed/.test(location.search))document.documentElement.classList.add('embed')</script>\n"
    # The page's policy, in the page since GitHub Pages sends no headers of its own: scripts from
    # here and the one line above by its hash, wasm compiled, sound and pictures from here or
    # made in the tab, and connections out to relays and carriers anywhere over TLS. Nothing
    # inline beyond that, no plugins, no forms sent anywhere, and no referrer on the way out.
    import base64
    # A test build on the private network talks to a relay on this machine in the clear.
    loopback = " http://127.0.0.1:* ws://127.0.0.1:* http://localhost:* ws://localhost:*" if re.search(r"127\.0\.0\.1|localhost", os.environ.get("PYRATE_RELAYS", "") + harbour) else ""
    inline = embed[len("<script>"):-len("</script>\n")]
    csp = ("default-src 'self'; script-src 'self' 'wasm-unsafe-eval' 'sha256-" + base64.b64encode(hashlib.sha256(inline.encode()).digest()).decode() + "'; "
           "style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; media-src 'self' blob:; connect-src 'self' https: wss:" + loopback + "; "
           "font-src 'self' data:; worker-src 'self' blob:; object-src 'none'; base-uri 'none'; form-action 'none'")
    meta = f'<meta http-equiv="Content-Security-Policy" content="{csp}">\n<meta name="referrer" content="no-referrer">\n'
    # The radio itself (the module, its glue and the wasm, a megabyte and a half) starts coming
    # with the page, not after the script at the end of it asks.
    meta += f'<link rel="modulepreload" href="app.js?v={v}">\n<link rel="modulepreload" href="pkg/pyrate_browser.js?v={v}">\n<link rel="preload" href="pkg/pyrate_browser_bg.{v}.wasm" as="fetch" type="application/wasm" crossorigin>\n'
    meta += "".join(f'<link rel="preload" href="{f}" as="font" type="font/woff2" crossorigin>\n' for f in first_faces)
    assert head.count('<meta charset="utf-8">\n') == 1
    h = head.replace('<meta charset="utf-8">\n', '<meta charset="utf-8">\n' + meta, 1).replace("{{SCOPE}}", scope)
    if scope == "open":
        h = h.replace("<title>Pyrate Radio · Listen · HOSTIS</title>", "<title>Pyrate Radio · The Open Player · HOSTIS</title>")
    else:
        # hostis.black's copy: its address, its card, and where it sits under HOSTIS. The open
        # player claims no hostis.black address as its own, so its bytes (and its IPFS name) stay.
        listen = "https://hostis.black/pyrateradio/listen/"
        trail = json.dumps({"@context": "https://schema.org", "@type": "BreadcrumbList", "itemListElement": [
            {"@type": "ListItem", "position": i + 1, "name": n, "item": u} for i, (n, u) in
            enumerate([("HOSTIS", "https://hostis.black/"), ("Pyrate Radio", "https://hostis.black/pyrateradio/"), ("Listen", listen)])]})
        for old, new in [
            ('<meta property="og:description" content="Every station on the Log, played in the browser from whoever carries it.">\n',
             '<meta property="og:description" content="The HOSTIS stations, played in the browser from whoever carries them.">\n'),
            ('<meta property="og:image" content="https://hostis.black/og-image.png">\n',
             '<meta property="og:image" content="https://hostis.black/og-image.png">\n<meta property="og:image:width" content="1200">\n<meta property="og:image:height" content="630">\n'
             '<meta property="og:image:alt" content="The HOSTIS skull and crossbones over columns of ones and zeros">\n'),
            ('<meta name="color-scheme" content="dark">\n',
             '<meta name="color-scheme" content="dark">\n<meta name="robots" content="index, follow, max-image-preview:large">\n'
             f'<link rel="canonical" href="{listen}">\n<meta property="og:type" content="website">\n<meta property="og:url" content="{listen}">\n'
             f'<meta property="og:locale" content="en_GB">\n<script type="application/ld+json">{trail}</script>\n')]:
            assert h.count(old) == 1, old
            h = h.replace(old, new)
    open(f"{dist}/index.html", "w", encoding="utf-8").write(h + extra + "</style>\n" + embed + "</head>\n<body>\n" + body.replace("{{V}}", v).replace("{{MIRRORS}}", mirrors_html) + "</body>\n</html>\n")
    open(f"{dist}/app.js", "w", encoding="utf-8").write(out_js.replace("./pkg/pyrate_browser.js", f"./pkg/pyrate_browser.js?v={v}"))
    # The wasm takes the build's version in its name and the glue is pointed at it, so a cached
    # old wasm can never meet new glue: every deploy is a new file.
    glue = open(f"{W}/pkg/pyrate_browser.js", encoding="utf-8").read()
    assert glue.count("new URL('pyrate_browser_bg.wasm', import.meta.url)") == 1
    open(f"{dist}/pkg/pyrate_browser.js", "w", encoding="utf-8").write(glue.replace("new URL('pyrate_browser_bg.wasm', import.meta.url)", f"new URL('pyrate_browser_bg.{v}.wasm', import.meta.url)"))
    shutil.copy(f"{W}/pkg/pyrate_browser_bg.wasm", f"{dist}/pkg/pyrate_browser_bg.{v}.wasm")
    shutil.copy(f"{W}/opus-decoder.min.js", dist)
    os.makedirs(f"{dist}/fonts")
    for name, data in face_files.items():
        open(f"{dist}/fonts/{name}", "wb").write(data)
    if scope == "open":
        # Raising colours from the tab: libopus in wasm, run off the page's thread.
        shutil.copy(f"{W}/encode-worker.js", dist)
        # The live desk's mixing and limiting, run on the audio thread.
        shutil.copy(f"{W}/desk-worklet.js", dist)
        shutil.copytree(f"{W}/opus", f"{dist}/opus")
    shutil.copy(f"{W}/chart.json", dist)
    json.dump(MIRRORS, open(f"{dist}/mirrors.json", "w"), indent=1)
    for f in ["favicon.png", "apple-touch-icon.png"]:
        shutil.copy(f"{R}/../{f}", dist)
    # Added to a home screen it opens as an app of its own, on HOSTIS black.
    for f in ["icon-192.png", "icon-512.png"]:
        shutil.copy(f"{W}/{f}", dist)
    # HOSTIS's stations' own pictures, small, so the set and the list show them before the network answers.
    os.makedirs(f"{dist}/art", exist_ok=True)
    for h in json.loads(harbour_stations):
        slug = h.get("art", "")[4:-4]
        if not slug or not os.path.exists(f"{R}/stations/{slug}/station.jpg"): continue
        from PIL import Image
        Image.open(f"{R}/stations/{slug}/station.jpg").convert("RGB").resize((512, 512), Image.LANCZOS).save(f"{dist}/art/{slug}.jpg", quality=84, optimize=True, progressive=True)
        Image.open(f"{R}/stations/{slug}/banner.jpg").convert("RGB").resize((1200, 400), Image.LANCZOS).save(f"{dist}/art/{slug}-banner.jpg", quality=80, optimize=True, progressive=True)
    json.dump({"name": "Pyrate Radio", "short_name": "Pyrate", "description": "Radio that every listener carries.",
               "start_url": "./", "scope": "./", "display": "standalone", "background_color": "#030303", "theme_color": "#030303",
               "icons": [{"src": "icon-192.png", "sizes": "192x192", "type": "image/png", "purpose": "any maskable"},
                         {"src": "icon-512.png", "sizes": "512x512", "type": "image/png", "purpose": "any maskable"}]},
              open(f"{dist}/manifest.webmanifest", "w"), indent=1)
    # The installed player's keeper (www/sw.js), with the release keys it checks new copies by,
    # and the list of every file in this copy with its SHA-256 under the copy's code: what two of
    # the three keyholders sign (`release sign <words> <n> player.manifest <code>`, the lines going
    # into player.sigs beside it). An unsigned copy still plays; it is never installed over one.
    keys = [k for k in (os.environ.get("PYRATE_RELEASE_KEYS", "").split(",") if os.environ.get("PYRATE_RELEASE_KEYS") else open(f"{R}/app/assets/release_keys.txt").read().split()) if k]
    assert len(keys) == 3 and all(re.fullmatch(r"[0-9a-f]{64}", k) for k in keys), "three release keys"
    sw = open(f"{W}/sw.js", encoding="utf-8").read()
    assert sw.count("'{{RELEASE_KEYS}}'") == 1
    open(f"{dist}/sw.js", "w", encoding="utf-8").write(sw.replace("'{{RELEASE_KEYS}}'", json.dumps(keys)))
    code = int(os.environ.get("PYRATE_PLAYER_CODE") or time.time() // 60)
    lines = []
    for root, _, files in os.walk(dist):
        for f in files:
            path = os.path.relpath(os.path.join(root, f), dist).replace(os.sep, "/")
            if path in ("player.manifest", "player.sigs"): continue
            lines.append(hashlib.sha256(open(os.path.join(root, f), "rb").read()).hexdigest() + "  " + path)
    open(f"{dist}/player.manifest", "w", encoding="utf-8").write(f"pyrate-player/v1 code {code}\n" + "\n".join(sorted(lines, key=lambda l: l[66:])) + "\n")
    print("built", scope, v)
    return v

# The open player first, packed for IPFS: its name is a fingerprint of every byte in it.
build("open", f"{W}/dist-open", ["https://hostis.black/pyrateradio/listen/"])
IPFS = shutil.which("ipfs") or os.path.expanduser("~/bin/ipfs")
repo = f"{W}/.ipfs"
env = dict(os.environ, IPFS_PATH=repo)
if not os.path.exists(f"{repo}/config"):
    subprocess.run([IPFS, "init", "--profile=test", "-e"], env=env, check=True, capture_output=True)
cid = subprocess.run([IPFS, "add", "-r", "-Q", "--cid-version=1", "--offline", f"{W}/dist-open"], env=env, check=True, capture_output=True, text=True).stdout.strip()
with open(f"{W}/open.car", "wb") as f:
    subprocess.run([IPFS, "dag", "export", "--offline", cid], env=env, check=True, stdout=f)
subprocess.run([IPFS, "repo", "gc", "--offline"], env=env, capture_output=True)
# Every player built is kept, so one an app was built to name can always be published.
os.makedirs(f"{W}/cars", exist_ok=True)
shutil.copy(f"{W}/open.car", f"{W}/cars/{cid}.car")
# Gateways that serve each IPFS name at an address of its own (inbrowser.link fetches and checks
# the files in the visitor's own browser), then one that serves by path,
# then any public Lanterns listed by hand in open_players.json and mirrors.txt. The first that answers wins.
extra_players = json.load(open(f"{R}/browser/open_players.json", encoding="utf-8")) if os.path.exists(f"{R}/browser/open_players.json") else []
extra_players += [m for m in MIRRORS if m not in extra_players]
# The player's fixed name (IPNS, published by dukewilder's pin_player.sh) comes first: one
# address for every release, so a browser keeps what it keeps from one to the next.
ipns = open(f"{R}/browser/ipns.txt").read().strip() if os.path.exists(f"{R}/browser/ipns.txt") else ""
# A test build can send them to its own: PYRATE_OPEN_PLAYERS="<url>,<url>".
players = os.environ["PYRATE_OPEN_PLAYERS"].split(",") if os.environ.get("PYRATE_OPEN_PLAYERS") else ([f"https://{ipns}.ipns.inbrowser.link/"] if ipns else []) + [f"https://{cid}.ipfs.inbrowser.link/", f"https://{cid}.ipfs.dweb.link/", f"https://ipfs.io/ipfs/{cid}/"] + extra_players
json.dump({"cid": cid, "players": players}, open(f"{W}/open.json", "w"), indent=1)
# The app's own list (app/assets/open_players.txt) is set when an app is built: tools/app_players.py.
print("open player", cid)
build("harbour", f"{W}/dist", players)
