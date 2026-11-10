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
# Each value goes in as a JSON string, so nothing in it can close the quote it sits in.
for k in ["'{{HARBOUR}}'", "'{{RELAYS}}'", "'{{HARBOUR_STATIONS}}'"]:
    assert js.count(k) == 1, k
out_js = js.replace("'{{HARBOUR}}'", json.dumps(harbour)).replace("'{{RELAYS}}'", json.dumps(os.environ.get("PYRATE_RELAYS", ""))).replace("'{{HARBOUR_STATIONS}}'", harbour_stations)
v = hashlib.sha256(out_js.encode() + open(f"{W}/pkg/pyrate_browser_bg.wasm", "rb").read()).hexdigest()[:10]
dist = f"{W}/dist"
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
head = head.replace('<meta charset="utf-8">\n', '<meta charset="utf-8">\n' + meta, 1)
open(f"{dist}/index.html", "w", encoding="utf-8").write(head + extra + "</style>\n" + embed + "</head>\n<body>\n" + body.replace("{{V}}", v) + "</body>\n</html>\n")
open(f"{dist}/app.js", "w", encoding="utf-8").write(out_js.replace("./pkg/pyrate_browser.js", f"./pkg/pyrate_browser.js?v={v}"))
# The wasm takes the build's version in its name and the glue is pointed at it, so a cached
# old wasm can never meet new glue: every deploy is a new file.
glue = open(f"{W}/pkg/pyrate_browser.js", encoding="utf-8").read()
assert glue.count("new URL('pyrate_browser_bg.wasm', import.meta.url)") == 1
open(f"{dist}/pkg/pyrate_browser.js", "w", encoding="utf-8").write(glue.replace("new URL('pyrate_browser_bg.wasm', import.meta.url)", f"new URL('pyrate_browser_bg.{v}.wasm', import.meta.url)"))
shutil.copy(f"{W}/pkg/pyrate_browser_bg.wasm", f"{dist}/pkg/pyrate_browser_bg.{v}.wasm")
shutil.copy(f"{W}/opus-decoder.min.js", dist)
shutil.copy(f"{W}/chart.json", dist)
shutil.copy(f"{R}/node/src/web/wordmark.png", dist)
shutil.copy(f"{R}/node/src/web/mark.png", dist)
print("built", v)
