"""Plays a Lantern stream at 127.0.0.1:8383, breaks it, and checks the page asks again. lantern_stream.py"""
import time
from playwright.sync_api import sync_playwright
with sync_playwright() as p:
    b=p.chromium.launch(args=["--autoplay-policy=no-user-gesture-required"]); pg=b.new_page(); L=[]
    pg.on("pageerror", lambda e: L.append(str(e)[:200]))
    pg.goto("http://127.0.0.1:8383/"); time.sleep(6)
    pg.click("#go"); time.sleep(12)
    t1 = pg.evaluate("document.getElementById('a').currentTime")
    # Break the stream as a dropped network would: the page should ask again by itself.
    pg.evaluate("document.getElementById('a').dispatchEvent(new Event('ended'))"); time.sleep(6)
    t2 = pg.evaluate("[document.getElementById('a').currentTime, document.getElementById('a').paused, document.getElementById('np').textContent]")
    print("played:", round(t1,1), "| after a break:", t2, "| errors:", L[:3]); b.close()
