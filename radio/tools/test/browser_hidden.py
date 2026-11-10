"""A hidden tab keeps more than a minute queued, so a browser that wakes it once a minute
never lets it run dry; back in view it plays on. browser_hidden.py <port>"""
import sys, time
from playwright.sync_api import sync_playwright
with sync_playwright() as p:
    b = p.chromium.launch(args=["--autoplay-policy=no-user-gesture-required"]); pg = b.new_page(); errs = []
    pg.on("pageerror", lambda e: errs.append(str(e)))
    pg.goto(f"http://127.0.0.1:{sys.argv[1]}/index.html")
    for i in range(40):
        time.sleep(1)
        if "Gangway" in (pg.text_content("#name") or ""): break
    pg.click("#go")
    for i in range(60):
        time.sleep(1)
        if pg.evaluate("window.pyrate.state().scheduled") > 0: break
    time.sleep(8); seen = pg.evaluate("window.pyrate.state().ahead_ms")
    pg.evaluate("Object.defineProperty(document, 'hidden', { configurable: true, get: () => true })")
    time.sleep(40); hidden = pg.evaluate("window.pyrate.state().ahead_ms")
    ok = seen <= 19000 and hidden > 30000
    print(("PASS" if ok else "FAIL") + f" in view {seen} ms queued, hidden {hidden} ms", "| errors:", errs[:2])
    b.close(); sys.exit(0 if ok else 1)
