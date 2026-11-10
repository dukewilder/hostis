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
    for i in range(40):
        time.sleep(1)
        if "Gangway" in (pg.text_content("#name") or ""): break
    check("a station shows", "Gangway" in (pg.text_content("#name") or ""))
    check("the console names no node or station", not any(l.startswith("pyrate:") for l in logs))
    pg.wait_for_selector("#chatgo", state="visible", timeout=60000)
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
    check("no page errors", not errs)
    if errs: print(errs[:3])
    b.close()
sys.exit(0 if ok else 1)
