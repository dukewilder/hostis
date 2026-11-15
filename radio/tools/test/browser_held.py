#!/usr/bin/env python3
"""A keeper's entries wait in the tab for their stamp: browser_held.py [port]
With no network. Builds a test copy of the open player, serves it on the port (default 8094)
and checks, in Chromium, that a station raised in the studio:
  1. holds its entries (they are stamped at the next quarter hour and have not been let go);
  2. keeps them sealed in storage, naming nothing of the station;
  3. gets them back after the tab is closed and opened again inside the wait, still held;
  4. lets them go once the stamp has come, and the stored copy goes with them;
  5. when the tab is closed in the wait and opened again after the stamp, takes them back and lets
     them go at once, for the stem to carry.
The clock is moved with Date.now; nothing else is faked. Rebuilds the public copy at the end."""
import os, shutil, subprocess, sys, threading, time, functools, http.server
from playwright.sync_api import sync_playwright

R = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
PORT = int(sys.argv[1]) if len(sys.argv) > 1 else 8094
OUT = "/tmp/dist-held"
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
http.server.SimpleHTTPRequestHandler.log_message = lambda *a: None
srv = http.server.ThreadingHTTPServer(("127.0.0.1", PORT), functools.partial(http.server.SimpleHTTPRequestHandler, directory=OUT))
threading.Thread(target=srv.serve_forever, daemon=True).start()
# The clock's offset is read once when a page loads (from sessionStorage, which survives a reload),
# and can be moved in a running page through window.__skew.
SKEW = "const D = Date.now.bind(Date); window.__skew = +sessionStorage.getItem('__skew') || 0; Date.now = () => D() + window.__skew;"
URL = f"http://127.0.0.1:{PORT}/"


def boot(ctx):
    pg = ctx.new_page()
    pg.add_init_script(SKEW)
    pg.goto(URL)
    wait_for(pg, "window.pyrate && window.pyrate.node && window.pyrate.node()", 60)
    return pg


def mine(pg):
    return pg.evaluate("window.pyrate.keeper()")


def stored(pg):
    return pg.evaluate("localStorage.getItem('pyrate.held')")


def wait_for(pg, js, secs):
    # Polled with evaluate: the page's policy forbids wait_for_function's string eval after a reload.
    end = time.time() + secs
    while time.time() < end:
        try:
            if pg.evaluate(js):
                return True
        except Exception:
            pass
        time.sleep(0.4)
    return False


def raise_station(pg, name):
    pg.evaluate(f"document.getElementById('st-name').value = '{name}'; document.getElementById('st-raise').click()")
    return wait_for(pg, "window.pyrate.keeper().length > 0", 20)


try:
    with sync_playwright() as p:
        b = p.chromium.launch()
        for round_, skew_on_return in ((1, 0), (2, 16 * 60_000)):
            ctx = b.new_context()
            pg = boot(ctx)
            check(raise_station(pg, "Held Test"), f"round {round_}: a station is raised in the studio")
            k = mine(pg)[0]
            check(k["entries"] >= 3 and k["held"] == k["entries"] and k["let_go"] == 0,
                  f"round {round_}: every entry is held in the tab ({k['held']} of {k['entries']}, {k['let_go']} let go)")
            time.sleep(3)
            s = stored(pg)
            check(s and s.startswith('{"d":1') and "Held" not in s, f"round {round_}: the held entries are stored sealed ({(s or '')[:12]})")
            # The tab closes; the same browser opens it again, at the clock set for this round.
            pg.evaluate(f"sessionStorage.setItem('__skew', '{skew_on_return}')")
            pg.reload()
            wait_for(pg, "window.pyrate && window.pyrate.node && window.pyrate.node()", 60)
            back = wait_for(pg, "window.pyrate.keeper().length > 0", 100)
            check(back, f"round {round_}: the station comes back from the held entries with no network")
            if not back:
                break
            k = mine(pg)[0]
            if skew_on_return == 0:
                check(k["held"] == k["entries"] and k["let_go"] == 0, f"round 1: still held after the reopening ({k['held']} of {k['entries']})")
                time.sleep(3)
                check(stored(pg) is not None, "round 1: still stored")
                pg.evaluate("window.__skew = 960000")
                time.sleep(4)
                k = mine(pg)[0]
                check(k["held"] == 0, f"round 1: the stamp comes and nothing is held ({k['held']})")
                check(wait_for(pg, "localStorage.getItem('pyrate.held') === null", 8), "round 1: the stored copy goes when the entries do")
            else:
                time.sleep(3)
                check(k["held"] == 0 and k["entries"] >= 3, f"round 2: opened after the stamp, nothing is held ({k['held']} of {k['entries']})")
                check(wait_for(pg, "localStorage.getItem('pyrate.held') === null", 8), "round 2: the stored copy goes when the entries do")
            ctx.close()
        b.close()
finally:
    srv.shutdown()
    subprocess.run([sys.executable, f"{R}/browser/build.py"], check=True, stdout=subprocess.DEVNULL)
sys.exit(0 if ok else 1)
