"""The installed player, offline and signed: browser_offline.py
Needs the no-relay network (tools/test/doors_up.sh) and the test wasm. Builds a copy of the open
player under three test release keys, signs it with two, and serves it on 8090. Then checks that
  1. the service worker keeps the whole copy (wasm included) once two signatures check out;
  2. with the website gone, the page opens from the kept copy and plays through its doors;
  3. a newer copy signed by one key only is refused, and the old copy keeps playing;
  4. a newer copy signed by two keys is taken.
Prints PASS or FAIL for each check and exits non-zero on any failure."""
import hashlib, json, os, shutil, struct, subprocess, sys, time
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
from cryptography.hazmat.primitives import serialization
from playwright.sync_api import sync_playwright

R = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
A, GANG, WATCH, D1, D2 = open("/tmp/doors-ids").read().split()
OUT, PORT = "/tmp/dist-sw", 8090
ok = True


def check(c, what):
    global ok
    ok &= bool(c)
    print(("PASS " if c else "FAIL ") + what, flush=True)


def wait(pg, js, secs):
    end = time.time() + secs
    while time.time() < end:
        try:
            v = pg.evaluate(js)
            if v:
                return v
        except Exception:
            pass
        time.sleep(0.2)
    return None


keys = [Ed25519PrivateKey.generate() for _ in range(3)]
pubs = [k.public_key().public_bytes(serialization.Encoding.Raw, serialization.PublicFormat.Raw).hex() for k in keys]


def sign(code, signers):
    raw = open(f"{OUT}/player.manifest", "rb").read()
    msg = b"pyrate/release/v1\0" + struct.pack("<I", code) + hashlib.sha256(raw).digest()
    open(f"{OUT}/player.sigs", "w").write("".join(f"{n} {keys[n].sign(msg).hex()}\n" for n in signers))


def build(code, signers, tweak=False):
    env = dict(os.environ, PYRATE_HARBOUR=f"{A}@http://127.0.0.1:3340", PYRATE_RELAYS="http://127.0.0.1:3340", PYRATE_HARBOUR_STATIONS="/tmp/doors-harbour.json",
               PYRATE_OPEN_PLAYERS=f"http://127.0.0.1:{PORT}/", PYRATE_DOORS=D1, PYRATE_RELEASE_KEYS=",".join(pubs), PYRATE_PLAYER_CODE=str(code))
    subprocess.run([sys.executable, f"{R}/browser/build.py"], env=env, check=True, stdout=subprocess.DEVNULL)
    shutil.rmtree(OUT, ignore_errors=True)
    shutil.copytree(f"{R}/browser/www/dist-open", OUT)
    if tweak:
        # A different copy: its page says so, and its list is made again to match.
        open(f"{OUT}/app.js", "a").write(f"\nwindow.pyrateCopy = {code};\n")
        lines = open(f"{OUT}/player.manifest").read().splitlines()
        lines = [lines[0]] + [(hashlib.sha256(open(f"{OUT}/app.js", "rb").read()).hexdigest() + "  app.js") if l.endswith("  app.js") else l for l in lines[1:]]
        open(f"{OUT}/player.manifest", "w").write("\n".join(lines) + "\n")
    sign(code, signers)


server = None


def serve(on):
    global server
    if server:
        server.terminate(); server.wait(); server = None
    if on:
        server = subprocess.Popen([sys.executable, "-m", "http.server", str(PORT), "--bind", "127.0.0.1"], cwd=OUT, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        time.sleep(0.8)


json.dump([{"id": GANG, "name": "Gangway", "call_sign": ""}], open("/tmp/doors-harbour.json", "w"))
subprocess.run(["pkill", "-f", f"http.server {PORT}"])
build(100, [0, 2])
serve(True)
try:
    with sync_playwright() as p:
        b = p.chromium.launch(args=["--autoplay-policy=no-user-gesture-required"])
        ctx = b.new_context(viewport={"width": 1280, "height": 900})
        ctx.add_init_script("try { localStorage.setItem('pyrate.crew', '1') } catch (e) {}")
        pg = ctx.new_page()
        pg.goto(f"http://127.0.0.1:{PORT}/index.html")
        kept = wait(pg, "window.pyrate.copy()?.code", 40)
        check(kept == 100, f"the service worker keeps the copy signed by two of three keys (code {kept})")
        n = wait(pg, "caches.open(window.pyrate.copy().cache).then(c => c.keys()).then(k => k.length)", 10)
        wasm = wait(pg, "caches.open(window.pyrate.copy().cache).then(c => c.keys()).then(k => k.some(r => r.url.endsWith('.wasm')))", 5)
        files = len(open(f"{OUT}/player.manifest").read().splitlines()) - 1
        check(n == files and wasm, f"every file of the copy is kept, the wasm too ({n} of {files})")
        time.sleep(6)
        # 2. The website goes.
        serve(False)
        t = time.time()
        pg.goto(f"http://127.0.0.1:{PORT}/index.html#{GANG}")
        named = wait(pg, "(document.querySelector('#name')?.textContent || '').includes('Gangway')", 40)
        wait(pg, "window.pyrate.doors()?.through", 20)
        pg.click("#go")
        heard = wait(pg, "window.pyrate.state().heard", 30)
        check(named and heard, f"with no website, the kept copy opens and plays through its door in {round(time.time() - t, 1)} s")
        # 3. One signature is not enough.
        build(101, [1], tweak=True)
        serve(True)
        pg.evaluate("navigator.serviceWorker.controller?.postMessage('check')")
        time.sleep(6)
        pg.reload()
        wait(pg, "window.pyrate", 20)
        pg.evaluate("navigator.serviceWorker.controller?.postMessage('check')")
        time.sleep(4)
        code = pg.evaluate("caches.open('pyrate-meta').then(c => c.match('current')).then(r => r.json()).then(j => j.code)")
        check(code == 100 and pg.evaluate("window.pyrateCopy === undefined"), f"a newer copy signed by one key is refused: still code {code}, its code never ran")
        # 4. Two signatures are.
        build(102, [0, 1], tweak=True)
        serve(True)
        pg.evaluate("navigator.serviceWorker.controller?.postMessage('check')")
        time.sleep(8)
        code = pg.evaluate("caches.open('pyrate-meta').then(c => c.match('current')).then(r => r.json()).then(j => j.code)")
        serve(False)
        pg.reload()
        took = wait(pg, "window.pyrateCopy", 20)
        check(code == 102 and took == 102, f"a newer copy signed by two keys is taken, and served offline (code {code})")
        b.close()
finally:
    serve(False)
    subprocess.run([sys.executable, f"{R}/browser/build.py"], check=True, stdout=subprocess.DEVNULL)
sys.exit(0 if ok else 1)
