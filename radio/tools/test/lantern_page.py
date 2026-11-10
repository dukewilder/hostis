"""Checks a Lantern page on 127.0.0.1:8383 runs its script under its own policy. lantern_page.py"""
import time
from playwright.sync_api import sync_playwright
with sync_playwright() as p:
    b=p.chromium.launch(); pg=b.new_page(); L=[]
    pg.on("console", lambda m: L.append(m.text[:200])); pg.on("pageerror", lambda e: L.append("ERR "+str(e)[:200]))
    pg.goto("http://127.0.0.1:8383/"); time.sleep(20)
    print("script ran:", pg.evaluate("typeof tune"), "| console:", L[:4]); b.close()
