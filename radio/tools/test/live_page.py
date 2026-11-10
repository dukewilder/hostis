"""Loads a deployed page and checks the radio starts. live_page.py <url>"""
import sys,time
from playwright.sync_api import sync_playwright
with sync_playwright() as p:
    b=p.chromium.launch(); pg=b.new_page(); L=[]
    pg.on("pageerror", lambda e: L.append("PAGEERR "+str(e)))
    pg.goto(sys.argv[1]+"?v=%d" % time.time()); time.sleep(10)
    print("name:", pg.text_content("#name"), "| signal:", pg.text_content("#signal"))
    print(pg.evaluate("import('./pkg/pyrate_browser.js?x='+Date.now()).then(m => m.default()).then(()=>'init ok', e => 'ERR '+e)"))
    print(L[:5]); b.close()
