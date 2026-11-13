"""The open player from a Lantern's /listen/. lantern_player.py <port> <player folder>
Checks every file comes back as it is on disk with its proper type, that nothing outside the
folder can be had, that the old /player/ address moves on to /listen/, and that a tab plays a
station from the Lantern's copy as it does from the plain copy on 8099. Run by lantern_player.sh."""
import os, re, socket, sys, time, urllib.error, urllib.request
from playwright.sync_api import sync_playwright

port, folder = sys.argv[1], sys.argv[2]
base = f"http://127.0.0.1:{port}"
fails = []


def check(ok, what):
    print(("ok   " if ok else "FAIL ") + what)
    if not ok: fails.append(what)


class NoFollow(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, *a): return None


get = urllib.request.build_opener(NoFollow).open


def fetch(path):
    try:
        r = get(base + path, timeout=10); return r.status, r.headers, r.read()
    except urllib.error.HTTPError as e:
        return e.code, e.headers, b""


TYPES = {"html": "text/html", "js": "text/javascript", "wasm": "application/wasm", "json": "application/json",
         "webmanifest": "application/manifest+json", "png": "image/png", "jpg": "image/jpeg", "woff2": "font/woff2"}
n = 0
for d, _, fs in os.walk(folder):
    for f in fs:
        rel = os.path.relpath(os.path.join(d, f), folder).replace(os.sep, "/")
        st, h, body = fetch("/listen/" + rel)
        want = TYPES.get(rel.rsplit(".", 1)[-1], "")
        if st != 200 or body != open(os.path.join(d, f), "rb").read() or not h.get("Content-Type", "").startswith(want):
            check(False, f"/listen/{rel}: {st} {h.get('Content-Type')}")
        n += 1
check(not fails, f"all {n} files of the player, byte for byte, each with its type")
st, h, body = fetch("/listen/")
check(st == 200 and b"Pyrate Radio" in body and h.get("Cache-Control") == "no-cache", "/listen/ is the player's page, looked at afresh each visit")
st, h, _ = fetch("/listen")
check(st in (301, 308) and h.get("Location") == "/listen/", "/listen moves to /listen/")
st, h, _ = fetch("/player/")
check(st in (301, 308) and h.get("Location") == "/listen/", "the old /player/ moves to /listen/")
st, h, _ = fetch("/player/app.js")
check(st in (301, 308) and h.get("Location") == "/listen/app.js", "the old /player/app.js moves to /listen/app.js")
for bad in ["/listen/../Cargo.toml", "/listen/%2e%2e/%2e%2e/etc/passwd", "/listen/.hidden", "/listen//etc/passwd", "/player/..%2fx", "/listen/" + "ab" * 32]:
    st, _, _ = fetch(bad)
    check(st == 404, f"{bad} is not found ({st})")

lan = socket.gethostbyname(socket.gethostname())
if lan.startswith("127."):
    s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM); s.connect(("192.0.2.1", 9)); lan = s.getsockname()[0]; s.close()


def plays(url, flags=()):
    with sync_playwright() as p:
        b = p.chromium.launch(args=["--autoplay-policy=no-user-gesture-required", *flags])
        pg = b.new_page(); errors = []
        pg.on("pageerror", lambda e: errors.append(str(e)))
        pg.goto(url)
        for _ in range(40):
            time.sleep(1)
            if "Gangway" in (pg.text_content("#name") or ""): break
        name = pg.text_content("#name") or ""
        secure = pg.evaluate("isSecureContext")
        pg.click("#go")
        # A fresh tab may wait on a carrier's harbour note before the first sound: up to two minutes.
        heard, took = False, 0
        for took in range(120):
            time.sleep(1)
            heard = pg.evaluate("window.pyrate ? window.pyrate.state().heard : false")
            if heard: break
        b.close()
        check("Gangway" in name and heard and not errors, f"{url} plays Gangway (secure context {secure}; heard after {took + 1} s; errors {errors[:2]})")


plays(f"{base}/listen/")
# As a phone on the Wi-Fi sees it: plain HTTP at a local address, so no secure context. A real
# Wi-Fi's tabs reach public relays; the test relay sits on loopback, which Chrome keeps from
# pages at any other address, so that one check is off here and nothing else.
plays(f"http://{lan}:{port}/listen/", ["--disable-features=LocalNetworkAccessChecks,BlockInsecurePrivateNetworkRequests,PrivateNetworkAccessSendPreflights"])
plays("http://127.0.0.1:8099/")
print("\n" + ("all good" if not fails else f"{len(fails)} failed"))
sys.exit(1 if fails else 0)
