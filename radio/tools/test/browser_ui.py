"""Checks the set's controls on a test page: quiet console, chatter join and leave, a passed-on
link that never names the address it was served from. browser_ui.py <port>"""
import sys, time
from playwright.sync_api import sync_playwright
ok = True
def check(name, cond):
    global ok
    ok &= bool(cond); print(("PASS " if cond else "FAIL ") + name)
with sync_playwright() as p:
    b = p.chromium.launch(); pg = b.new_page(); logs = []; errs = []
    pg.on("console", lambda m: logs.append(m.text)); pg.on("pageerror", lambda e: errs.append(str(e)))
    pg.goto(f"http://127.0.0.1:{sys.argv[1]}/index.html")
    # The first open lands on the most listened station on the list: on the test network that is
    # Gangway or a test station, so any station the list holds will do.
    on = lambda: pg.evaluate("(() => { const s = window.pyrate?.state?.(); return !!(s && s.station && window.pyrate.card(s.station) && (document.getElementById('name').textContent || '').trim()) })()")
    for i in range(40):
        time.sleep(1)
        if on(): break
    check("a station shows", on())
    check("the console names no node or station", not any(l.startswith("pyrate:") for l in logs))
    pg.click(".tab[data-v=radio]"); pg.click("#pagego"); pg.wait_for_selector("#chatgo", state="visible", timeout=60000)
    pg.click("#chatgo")
    for i in range(20):
        time.sleep(1)
        if pg.is_visible("#chatroom"): break
    check("the chatter joins", pg.is_visible("#chatroom"))
    pg.click("#chatleave"); time.sleep(3)
    check("the chatter can be left", pg.is_visible("#chatjoin") and not pg.is_visible("#chatroom"))
    pg.click("#pass"); time.sleep(1)
    link = pg.text_content("#plink") or ""
    check("a link served from a bare address points at hostis.black", link.startswith("https://hostis.black/pyrateradio/listen/#"))
    pg.evaluate("localStorage.setItem('pyrate.name', 'Test')")
    pg.on("dialog", lambda d: d.accept())
    pg.click(".tab[data-v=settings]"); pg.click("#forget"); time.sleep(4)
    left = pg.evaluate("Object.keys(localStorage).filter(k => k.startsWith('pyrate.') && k !== 'pyrate.token' && k !== 'pyrate.harbours')")
    check("forgetting clears what the browser keeps" + (f" ({left})" if left else ""), not left)
    check("the page's policy refuses nothing it needs", not any("Content Security Policy" in l for l in logs))
    check("no page errors", not errs)
    if errs: print(errs[:3])
    b.close()
sys.exit(0 if ok else 1)
