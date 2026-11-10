"""Drives the browser radio into a pocket station from an invitation, and speaks in its room.
browser_pocket.py <invite file> <port>"""
import sys, time, os
from playwright.sync_api import sync_playwright
inv_path, port = sys.argv[1], sys.argv[2]
for _ in range(120):
    if os.path.exists(inv_path): break
    time.sleep(1)
link = open(inv_path).read().strip()
frag = link[link.index('#'):]
with sync_playwright() as p:
    b = p.chromium.launch(args=["--autoplay-policy=no-user-gesture-required"])
    pg = b.new_page()
    logs = []
    pg.on("console", lambda m: logs.append(m.text))
    pg.goto(f"http://127.0.0.1:{port}/index.html{frag}")
    time.sleep(1.5)
    h = pg.evaluate("location.hash")
    print(("PASS" if "pocket=" not in h else "FAIL") + f" the invitation leaves the address bar at once ({h[:20]!r})")
    ok = False
    for _ in range(60):
        time.sleep(1)
        if "Back Room" in (pg.text_content("#name") or ""): ok = True; break
    print(("PASS" if ok else "FAIL") + f" the tab opens the pocket station ({pg.text_content('#name')!r})")
    h = pg.evaluate("location.href")
    print(("PASS" if "#" not in h else "FAIL") + f" the pocket's id stays out of the address bar ({h!r})")
    print(("PASS" if pg.is_hidden("#pass") else "FAIL") + " no pass-it-on for a pocket")
    pg.click("#chatgo"); time.sleep(6)
    pg.fill("#sayt", "Ahoy from the browser"); pg.click("#say button"); time.sleep(4)
    print("lines:", (pg.text_content("#lines") or "")[:200])
    print("console tail:", [l for l in logs if "error" in l.lower()][:5])
    b.close()
