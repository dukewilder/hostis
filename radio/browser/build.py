# Builds browser/www: the radio as a static page any host can serve. Fonts and look come
# from the Lantern's web listener, so the two stay one design.
import re, os, shutil, subprocess, sys, hashlib
R = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
W = f"{R}/browser/www"
src = open(f"{R}/node/src/web/listen.html", encoding="utf-8").read()
head = src[:src.index("</style>")]
head = head.replace('href="/brand/mark.png"', 'href="mark.png"').replace("<title>Pyrate Radio</title>", "<title>Pyrate Radio</title>\n<meta name=\"description\" content=\"Takedown-resistant radio under HOSTIS colours. Plays in the browser, peer to peer.\">")
extra = "\n" + open(f"{R}/browser/player.css", encoding="utf-8").read()
body = open(f"{R}/browser/body.html", encoding="utf-8").read()
js = open(f"{W}/app.js", encoding="utf-8").read()
# A test build names its own harbour and relays: PYRATE_HARBOUR="<id>@<relay>", PYRATE_RELAYS.
harbour = os.environ.get("PYRATE_HARBOUR") or ",".join(open(f"{R}/app/assets/bootstrap.txt").read().split())
import json
# HOSTIS's own stations: the only ones the landing page's set offers.
harbour_stations = json.dumps(json.load(open(f"{R}/browser/harbour.json", encoding="utf-8")), ensure_ascii=False)
out_js = js.replace("{{HARBOUR}}", harbour).replace("{{RELAYS}}", os.environ.get("PYRATE_RELAYS", "")).replace("'{{HARBOUR_STATIONS}}'", harbour_stations)
v = hashlib.sha256(out_js.encode() + open(f"{W}/pkg/pyrate_browser_bg.wasm", "rb").read()).hexdigest()[:10]
dist = f"{W}/dist"
if os.path.exists(dist): shutil.rmtree(dist)
os.makedirs(f"{dist}/pkg")
# ?embed shows the set alone, for the landing page's hero; decided before first paint.
embed = "<script>if(/[?&]embed/.test(location.search))document.documentElement.classList.add('embed')</script>\n"
open(f"{dist}/index.html", "w", encoding="utf-8").write(head + extra + "</style>\n" + embed + "</head>\n<body>\n" + body.replace("{{V}}", v) + "</body>\n</html>\n")
open(f"{dist}/app.js", "w", encoding="utf-8").write(out_js.replace("./pkg/pyrate_browser.js", f"./pkg/pyrate_browser.js?v={v}"))
for f in ["pyrate_browser.js", "pyrate_browser_bg.wasm"]:
    shutil.copy(f"{W}/pkg/{f}", f"{dist}/pkg/{f}")
shutil.copy(f"{W}/opus-decoder.min.js", dist)
shutil.copy(f"{W}/chart.json", dist)
shutil.copy(f"{R}/node/src/web/wordmark.png", dist)
shutil.copy(f"{R}/node/src/web/mark.png", dist)
print("built", v)
