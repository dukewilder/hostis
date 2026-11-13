"""A returning visit to hostis.black's copy: browser_return.py [port]
Serve a test build first (serve_test.sh; hostis.black's copy is on 8097). A first, cold visit
listens and leaves once the page has kept HOSTIS's logs. Returning visits, in the same browser,
take them back before they ask the network. The times to sound are printed; on the test
network a log is a few entries, so the saving shows on the real one, where logs run to thousands. Also checks
that the open player keeps no logs at all, and that Forget clears them.
Prints PASS or FAIL for each check and exits non-zero on any failure."""
import sys, time
from playwright.sync_api import sync_playwright
port = sys.argv[1] if len(sys.argv) > 1 else "8097"
ok = True
def check(c, what):
    global ok; ok &= bool(c); print(("PASS " if c else "FAIL ") + what, flush=True)
def wait(pg, js, secs):
    end = time.time() + secs
    while time.time() < end:
        if pg.evaluate(js): return True
        time.sleep(0.2)
    return False
# Listen is pressed the moment the page opens, so the time includes everything the radio must
# fetch first: on a cold visit the whole log, on a returning one only what is newer.
def visit(ctx):
    pg = ctx.new_page(); t = time.time(); pg.goto(f"http://127.0.0.1:{port}/"); pg.click("#go")
    heard = wait(pg, "window.pyrate.state().heard", 90)
    took = time.time() - t
    pg.click("#go"); time.sleep(0.5)
    return pg, (took if heard else None)

errs = []
with sync_playwright() as p:
    b = p.chromium.launch(args=["--autoplay-policy=no-user-gesture-required"])
    ctx = b.new_context(viewport={"width": 390, "height": 844})
    ctx.on("weberror", lambda e: errs.append(str(e.error)))
    # Three cold visits, each in a browser of its own, for the middle time; then this one's.
    colds = []
    for i in range(2):
        x = b.new_context(viewport={"width": 390, "height": 844}); colds.append(visit(x)[1]); x.close()
    a, t = visit(ctx); colds.append(t)
    check(a.evaluate("window.pyrate.logs().taken") == 0, "a first visit has no kept logs to take")
    cold = sorted(colds)[1] if None not in colds else None
    print(f"time  cold visits, open to sound {[c and round(c, 2) for c in colds]} s", flush=True)
    check(wait(a, "window.pyrate.logs().kept", 120), "the page keeps HOSTIS's logs a little after they arrive")
    a.close()
    warms = []
    for i in range(3):
        c, t = visit(ctx); warms.append(t)
        taken = c.evaluate("window.pyrate.logs().taken")
        if i < 2: c.close()
    check(taken > 0, f"a returning visit takes the kept logs back before it asks the network ({taken} entries)")
    warm = sorted(warms)[1] if None not in warms else None
    print(f"time  returning visits, open to sound {[w and round(w, 2) for w in warms]} s", flush=True)
    # A test station's log is a handful of entries, so here the two differ by less than the
    # network's own noise: the times are shown, and only a returning visit that takes much longer fails.
    check(warm is not None and cold is not None and warm <= cold + 1.5, f"a returning visit is no slower than a cold one (middle {warm and round(warm, 2)} s against {cold and round(cold, 2)} s)")
    # Forget clears them.
    c.on("dialog", lambda d: d.accept())
    c.click(".tab[data-v=settings]"); c.click("#forget"); time.sleep(4)
    n = c.evaluate("new Promise(r => { const q = indexedDB.open('pyrate-logs'); q.onsuccess = () => { const db = q.result; if (!db.objectStoreNames.contains('logs')) return r(0); const t = db.transaction('logs').objectStore('logs').count(); t.onsuccess = () => r(t.result) }; q.onerror = () => r(-1) })")
    check(n == 0, f"Forget this browser clears them ({n} left)")
    ctx.close()
    # The open player keeps none: there, which logs are kept would be a listening history.
    o = b.new_context(viewport={"width": 390, "height": 844}).new_page()
    o.goto("http://127.0.0.1:8099/"); o.click("#go"); wait(o, "window.pyrate.state().heard", 90); time.sleep(35)
    dbs = o.evaluate("indexedDB.databases ? indexedDB.databases().then(l => l.map(d => d.name)) : []")
    check("pyrate-logs" not in dbs and o.evaluate("window.pyrate.logs().kept") is False, f"the open player keeps no logs ({dbs})")
    b.close()
check(not errs, "no page errors: %s" % errs[:3])
sys.exit(0 if ok else 1)
