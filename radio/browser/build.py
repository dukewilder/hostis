# Builds the radio as static pages any host can serve, on the HOSTIS sheet, in two copies:
#   www/dist       hostis.black's own: HOSTIS's stations only; every other link goes to an open player
#   www/dist-open  the open player: the whole Log, served from IPFS and Lanterns, owned by nobody
# The open copy is packed as a CAR (www/open.car) under its IPFS name, and that name is what
# hostis.black's copy, the landing and the app send every other station to.
import re, os, shutil, subprocess, sys, hashlib
R = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
W = f"{R}/browser/www"
sys.path.insert(0, f"{R}/brand")
import fonts
# The page is one more HOSTIS page: the site's faces and sheet (brand/), then the set's own.
head = """<!DOCTYPE html>
<html lang="en-GB" class="scope-{{SCOPE}}">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover">
<title>Pyrate Radio · Listen · HOSTIS</title>
<meta name="description" content="Takedown-resistant radio under HOSTIS colours. Plays in the browser, peer to peer.">
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
<style>
""" + fonts.faces() + "\n" + fonts.sheet()
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
for k in ["'{{HARBOUR}}'", "'{{RELAYS}}'", "'{{HARBOUR_STATIONS}}'", "'{{SCOPE}}'", "'{{OPEN_PLAYERS}}'"]:
    assert js.count(k) == 1, k
base_js = js.replace("'{{HARBOUR}}'", json.dumps(harbour)).replace("'{{RELAYS}}'", json.dumps(os.environ.get("PYRATE_RELAYS", ""))).replace("'{{HARBOUR_STATIONS}}'", harbour_stations)

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
    assert head.count('<meta charset="utf-8">\n') == 1
    h = head.replace('<meta charset="utf-8">\n', '<meta charset="utf-8">\n' + meta, 1).replace("{{SCOPE}}", scope)
    if scope == "open":
        h = h.replace("<title>Pyrate Radio · Listen · HOSTIS</title>", "<title>Pyrate Radio · The Open Player · HOSTIS</title>")
    open(f"{dist}/index.html", "w", encoding="utf-8").write(h + extra + "</style>\n" + embed + "</head>\n<body>\n" + body.replace("{{V}}", v) + "</body>\n</html>\n")
    open(f"{dist}/app.js", "w", encoding="utf-8").write(out_js.replace("./pkg/pyrate_browser.js", f"./pkg/pyrate_browser.js?v={v}"))
    # The wasm takes the build's version in its name and the glue is pointed at it, so a cached
    # old wasm can never meet new glue: every deploy is a new file.
    glue = open(f"{W}/pkg/pyrate_browser.js", encoding="utf-8").read()
    assert glue.count("new URL('pyrate_browser_bg.wasm', import.meta.url)") == 1
    open(f"{dist}/pkg/pyrate_browser.js", "w", encoding="utf-8").write(glue.replace("new URL('pyrate_browser_bg.wasm', import.meta.url)", f"new URL('pyrate_browser_bg.{v}.wasm', import.meta.url)"))
    shutil.copy(f"{W}/pkg/pyrate_browser_bg.wasm", f"{dist}/pkg/pyrate_browser_bg.{v}.wasm")
    shutil.copy(f"{W}/opus-decoder.min.js", dist)
    shutil.copy(f"{W}/chart.json", dist)
    for f in ["favicon.png", "apple-touch-icon.png"]:
        shutil.copy(f"{R}/../{f}", dist)
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
# Gateways that serve each IPFS name at an address of its own, then one that serves by path,
# then any public Lanterns listed by hand in open_players.json. The first that answers wins.
extra_players = json.load(open(f"{R}/browser/open_players.json", encoding="utf-8")) if os.path.exists(f"{R}/browser/open_players.json") else []
# A test build can send them to its own: PYRATE_OPEN_PLAYERS="<url>,<url>".
players = os.environ["PYRATE_OPEN_PLAYERS"].split(",") if os.environ.get("PYRATE_OPEN_PLAYERS") else [f"https://{cid}.ipfs.dweb.link/", f"https://{cid}.ipfs.w3s.link/", f"https://ipfs.io/ipfs/{cid}/"] + extra_players
json.dump({"cid": cid, "players": players}, open(f"{W}/open.json", "w"), indent=1)
# The app names the same open players in its links; a test build leaves the app's list alone.
if not os.environ.get("PYRATE_OPEN_PLAYERS"):
    open(f"{R}/app/assets/open_players.txt", "w").write("\n".join(players) + "\n")
print("open player", cid)
build("harbour", f"{W}/dist", players)
