"""Simulates a wasm trap on the test page (port 8099): the set recovers once, then asks for a reload. browser_trap.py"""
import time
from playwright.sync_api import sync_playwright
with sync_playwright() as p:
    b=p.chromium.launch(); pg=b.new_page()
    pg.goto("http://127.0.0.1:8099/index.html")
    for i in range(40):
        time.sleep(1)
        if "Gangway" in (pg.text_content("#name") or ""): break
    pg.evaluate("window.dispatchEvent(new ErrorEvent('error', {error: new WebAssembly.RuntimeError('unreachable')}))")
    time.sleep(1); print("after the trap:", pg.text_content("#np"))
    time.sleep(12); print("after coming back:", pg.text_content("#name"), "|", pg.evaluate("location.hash.length"))
    pg.evaluate("window.dispatchEvent(new ErrorEvent('error', {error: new WebAssembly.RuntimeError('unreachable')}))")
    time.sleep(4); print("a second trap soon after:", pg.text_content("#np"))
    b.close()
