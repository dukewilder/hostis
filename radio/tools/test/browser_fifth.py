#!/usr/bin/env python3
"""The fifth audit's browser fixes, without a network: browser_fifth.py [port]
Builds a test copy of the open player (relays on this machine, none running), serves it on the
port (default 8093) and checks, in Chromium:
  1. the tab's title names no station, and Keep names plain makes it "Radio";
  2. Direct links starts on, number0's relays and Keep names plain start off;
  3. the studio's titles are cleaned by the app's rules (plain_title and plain_by in the wasm);
  4. the hidden stations, the chat name and the counting key are kept sealed;
  5. a test copy on this machine keeps its worker, and Forget lets it go with every cache,
     leaves nothing of the radio in storage and stays stopped;
  6. a link's door is never dialled on the link's word alone.
Rebuilds the public copy at the end. Prints PASS or FAIL lines."""
import os, shutil, subprocess, sys, threading, time, functools, http.server
from playwright.sync_api import sync_playwright

R = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
PORT = int(sys.argv[1]) if len(sys.argv) > 1 else 8093
OUT = "/tmp/dist-fifth"
FAKE = "a324c679b0dda368e23f21d6abbdaec0ef541a4ac317fa0643802b2734c9a442"
ok = True


def check(c, what):
    global ok
    ok &= bool(c)
    print(("PASS " if c else "FAIL ") + what, flush=True)


env = dict(os.environ, PYRATE_HARBOUR=f"{FAKE}@http://127.0.0.1:3349", PYRATE_RELAYS="http://127.0.0.1:3349", PYRATE_OPEN_PLAYERS=f"http://127.0.0.1:{PORT}/")
subprocess.run([sys.executable, f"{R}/browser/build.py"], env=env, check=True, stdout=subprocess.DEVNULL)
shutil.rmtree(OUT, ignore_errors=True)
shutil.copytree(f"{R}/browser/www/dist-open", OUT)
H = functools.partial(http.server.SimpleHTTPRequestHandler, directory=OUT)
http.server.SimpleHTTPRequestHandler.log_message = lambda *a: None
srv = http.server.ThreadingHTTPServer(("127.0.0.1", PORT), H)
threading.Thread(target=srv.serve_forever, daemon=True).start()
door = "~AAAAAAAA"  # a link's door hint: an address of the link's choosing

try:
    with sync_playwright() as p:
        b = p.chromium.launch()
        ctx = b.new_context()
        pg = ctx.new_page()
        pg.on("dialog", lambda d: d.accept())
        rtc = []
        pg.add_init_script("const P = window.RTCPeerConnection; window.__rtc = 0; window.RTCPeerConnection = function (...a) { window.__rtc++; return new P(...a) }; window.RTCPeerConnection.prototype = P.prototype;")
        # A link that names a door at 127.0.0.2:9 with a made-up fingerprint.
        import base64
        raw = bytes([127, 0, 0, 2, 0, 9]) + bytes(32)
        hint = base64.urlsafe_b64encode(raw).decode().rstrip("=")
        pg.goto(f"http://127.0.0.1:{PORT}/#{FAKE}~{hint}")
        pg.wait_for_function("window.pyrate && window.pyrate.node && window.pyrate.node()", timeout=60000)
        time.sleep(1)
        check("Pyrate Radio" in pg.title() and "Radio" == pg.title().replace("▶︎ ", "").replace("Pyrate ", ""), f"the title names no station ({pg.title()!r})")
        check(pg.evaluate("document.getElementById('sw-directlinks').checked") and not pg.evaluate("document.getElementById('sw-n0').checked") and not pg.evaluate("document.getElementById('sw-plain').checked"),
              "Direct links starts on, number0's relays and plain names off")
        pg.evaluate("const e = document.getElementById('sw-plain'); e.checked = true; e.dispatchEvent(new Event('change'))")
        time.sleep(0.3)
        check(pg.title().replace("▶︎ ", "") == "Radio", f"Keep names plain makes the title Radio ({pg.title()!r})")
        cleaned = pg.evaluate("""async () => {
          const src = await (await fetch('app.js')).text(), v = src.match(/pkg\\/pyrate_browser\\.js\\?v=[0-9a-f]+/)[0];
          const m = await import('./' + v);
          return [JSON.parse(m.plain_title('Audio Recording 2026-11-14 at 21.03.55')).text, JSON.parse(m.plain_title('AUD-20261114-WA0003')).text, m.plain_by('jane-mbp'), JSON.parse(m.plain_title('Harbour Song')).text];
        }""")
        check(all(not any(c.isdigit() for c in t) for t in cleaned[:2]) and "mbp" not in cleaned[2] and cleaned[3] == "Harbour Song", f"titles cleaned by the app's rules {cleaned}")
        # Sealed: a hidden station, the chat name and the counting key.
        pg.evaluate(f"document.getElementById('myname').value = 'Anne'; document.getElementById('myname').dispatchEvent(new Event('change'))")
        pg.evaluate(f"document.getElementById('mtext').value = '{'b' * 64}'; document.getElementById('mtake').click()")
        time.sleep(1)
        kept = pg.evaluate("({ name: localStorage.getItem('pyrate.name'), maroon: localStorage.getItem('pyrate.maroon'), token: localStorage.getItem('pyrate.token'), chat: localStorage.getItem('pyrate.chatkey') })")
        check(all((kept[k] or "").startswith('{"d":1') for k in kept) and "Anne" not in kept["name"] and "bbbb" not in kept["maroon"], "hidden stations, chat name, counting and chat keys are sealed")
        # The link's door is never dialled on its word: give the dialler time past its 4 s wait.
        time.sleep(30)
        check(pg.evaluate("window.__rtc") == 0, f"a link's door alone is never dialled ({pg.evaluate('window.__rtc')} links made)")
        regs = pg.evaluate("navigator.serviceWorker.getRegistrations().then(r => r.length)")
        check(regs == 1, f"a test copy on this machine keeps its worker ({regs})")
        pg.click("#forget") if pg.is_visible("#forget") else pg.evaluate("document.getElementById('forget').click()")
        time.sleep(3)
        left = pg.evaluate("""async () => ({ ls: Object.keys(localStorage).filter(k => k.startsWith('pyrate.')), regs: (await navigator.serviceWorker.getRegistrations()).length,
          caches: (await caches.keys()).filter(k => k.startsWith('pyrate')), url: location.href, forgot: document.getElementById('forget').disabled })""")
        time.sleep(6)
        later = pg.evaluate("Object.keys(localStorage).filter(k => k.startsWith('pyrate.'))")
        check(not left["ls"] and not later and left["regs"] == 0 and not left["caches"] and left["forgot"], f"Forget lets the worker and caches go and stays stopped {left} {later}")
        b.close()
finally:
    srv.shutdown()
    subprocess.run([sys.executable, f"{R}/browser/build.py"], check=True, stdout=subprocess.DEVNULL)
sys.exit(0 if ok else 1)
