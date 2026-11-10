"""Plays the first station the tab finds, as a listener would. browser_play.py <port>
Serve a test build of browser/www/dist (PYRATE_HARBOUR and PYRATE_RELAYS set) on that port first."""
import sys
import time
from playwright.sync_api import sync_playwright
with sync_playwright() as p:
    b=p.chromium.launch(args=["--autoplay-policy=no-user-gesture-required"]); pg=b.new_page(); E=[]
    pg.on("pageerror", lambda e: E.append(str(e)))
    pg.goto(f"http://127.0.0.1:{sys.argv[1]}/index.html"); 
    for i in range(40):
        time.sleep(1)
        if "Gangway" in (pg.text_content("#name") or ""): break
    print("station:", pg.text_content("#name"))
    pg.click("#go"); time.sleep(25)
    print("np:", pg.text_content("#np"), "| state:", pg.evaluate("window.pyrate.state()"))
    print("errors:", E[:3]); b.close()
