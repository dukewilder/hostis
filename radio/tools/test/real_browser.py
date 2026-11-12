"""The deployed player on the real network, timed from open to stations to sound, through the
session proxy. real_browser.py <url> [seconds]"""
import json, os, sys, time
from playwright.sync_api import sync_playwright
url = sys.argv[1]; secs = int(sys.argv[2]) if len(sys.argv) > 2 else 120
proxy = os.environ.get("HTTPS_PROXY") or os.environ.get("https_proxy")
with sync_playwright() as p:
    b = p.chromium.launch(proxy={"server": proxy, "bypass": "127.0.0.1,localhost"} if proxy else None, args=["--autoplay-policy=no-user-gesture-required"])
    ctx = b.new_context(ignore_https_errors=True)
    pg = ctx.new_page(); logs = []
    pg.on("console", lambda m: logs.append(f"{time.time()-t0:6.1f} console {m.type}: {m.text[:160]}"))
    pg.on("pageerror", lambda e: logs.append(f"{time.time()-t0:6.1f} pageerror {e}"))
    t0 = time.time()
    pg.goto(url, wait_until="domcontentloaded")
    print(f"{time.time()-t0:6.1f} page loaded")
    last = None; clicked = None; sounded = None
    while time.time() - t0 < secs:
        try:
            st = pg.evaluate("window.pyrate ? window.pyrate.state() : null")
            seen = pg.evaluate("[document.getElementById('name')?.textContent, document.getElementById('np')?.textContent, document.getElementById('signal')?.textContent, document.getElementById('go')?.hidden, document.getElementById('go')?.textContent]")
        except Exception as e:
            st, seen = None, str(e)
        line = json.dumps([st and {k: st[k] for k in ('playing','scheduled','ahead_ms','neighbours','stations')}, seen])
        if line != last:
            print(f"{time.time()-t0:6.1f} {line}"); last = line
        if clicked is None and st and st.get("stations", 0) > 0 and not seen[3]:
            pg.click("#go"); clicked = time.time(); print(f"{clicked-t0:6.1f} CLICKED LISTEN")
        if clicked and sounded is None and st and st.get("scheduled", 0) > 0:
            sounded = time.time(); print(f"{sounded-t0:6.1f} FIRST SOUND SCHEDULED ({sounded-clicked:.1f} s after Listen)")
            break
        time.sleep(0.25)
    for l in logs[-25:]: print(l)
    b.close()
