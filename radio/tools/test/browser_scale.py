"""The open player with a thousand stations on the air: browser_scale.py <port> <out dir> [n] [width]
Serve a test build first (serve_test.sh). The page's own copy of app.js is rewritten on the way
in, so the Log it reads has n made-up stations added to the real ones; nothing is changed in the
build. Times a refresh of the list and of the dial, checks the list pages and the search finds a
station, and writes screenshots of the list, the dial and the map."""
import json, random, sys, time
from playwright.sync_api import sync_playwright
port, out = sys.argv[1], sys.argv[2]
N = int(sys.argv[3]) if len(sys.argv) > 3 else 1000
W = int(sys.argv[4]) if len(sys.argv) > 4 else 390
random.seed(7)
A = "Black Salt Night Harbour Lantern Iron Tide Copper Low Long Grey Red Quiet Free Signal Shanty North Old Broken Open".split()
B = "Radio Watch Wire Hours Coast Air Bell Deck Station Waves Mile Crew Light Tape Frequency Room Hold Cabin".split()
PORTS = ["Lisbon", "Bristol", "Austin, Texas", "Paris, France", "Lagos", "Osaka", "Buenos Aires", "Reykjavik", "Manila", "Cape Town", "Montreal", "Sydney", ""]
KINDS = ["music", "talk", "news", "readings", "sound", "jazz", "punk", "folk"]
fake = []
for i in range(N):
    fake.append({"id": "%064x" % random.getrandbits(256), "name": f"{random.choice(A)} {random.choice(B)}", "call_sign": "".join(random.choice("BDFGHKLMNPRSTVZ") + random.choice("AEIOU") for _ in range(3)) + "·%04X" % random.getrandbits(16),
                 "about": random.choice(["", "Records from the stacks, all night.", "Talk and the news from the harbour wall, read by whoever turns up.", "Field recordings.", "Punk singles from a basement, in the order they were found."]),
                 "aboard": max(0, int(400 / (i + 1) ** 0.7) + random.randint(0, 2)), "live": random.random() < 0.03, "port": random.choice(PORTS), "kinds": random.sample(KINDS, 2), "mature": random.random() < 0.1,
                 "since": 1_700_000_000_000 + i * 3_600_000, "tracks": 0 if random.random() < 0.1 else random.randint(1, 400), "listed": True})
hook = "const live = JSON.parse(radio.stations()).filter(inSet).concat(window.__fake || []);"
with sync_playwright() as p:
    b = p.chromium.launch()
    pg = b.new_page(viewport={"width": W, "height": 900 if W > 600 else 844})
    def patch(route):
        r = route.fetch(); body = r.text()
        assert "const live = JSON.parse(radio.stations()).filter(inSet);" in body
        route.fulfill(response=r, body=body.replace("const live = JSON.parse(radio.stations()).filter(inSet);", hook))
    pg.route("**/app.js*", patch)
    pg.add_init_script("window.__fake = " + json.dumps(fake) + ";")
    E = []; pg.on("pageerror", lambda e: E.append(str(e)))
    pg.goto(f"http://127.0.0.1:{port}/index.html")
    t0 = time.time()
    while pg.evaluate("document.querySelectorAll('.row:not(.skel)').length") < 60 and time.time() - t0 < 60: time.sleep(0.5)
    time.sleep(2)
    # The dial: a mark for every station, and names only around the needle, none touching another.
    marks = pg.locator("#marks i").count()
    boxes = pg.eval_on_selector_all("#names button", "bs => bs.map(b => { const r = b.getBoundingClientRect(); return [r.left, r.right] })")
    g = pg.evaluate("(() => { const r = document.getElementById('glass').getBoundingClientRect(); return [r.left, r.right] })()")
    apart = all(boxes[i][1] <= boxes[j][0] or boxes[j][1] <= boxes[i][0] for i in range(len(boxes)) for j in range(i + 1, len(boxes)))
    inside = all(g[0] <= l and r <= g[1] for l, r in boxes)
    print(f"dial: {marks} marks, {len(boxes)} names, none overlapping: {apart}, all within the glass: {inside}")
    if not (marks >= N * 0.8 and 1 <= len(boxes) <= 25 and apart and inside): print("FAIL the dial at a thousand stations")
    pg.click(".tab[data-v=list]"); time.sleep(0.5)
    rows = pg.locator(".row:not(.skel)").count()
    more = pg.text_content(".morerows .fine") or ""
    names = pg.locator("#names button").count()
    lt = pg.evaluate("() => new Promise(r => { const t0 = performance.now(); let n = 0; const o = new PerformanceObserver(l => { for (const e of l.getEntries()) n = Math.max(n, e.duration) }); o.observe({ type: 'longtask', buffered: false }); setTimeout(() => { o.disconnect(); r(n) }, 9000) })")
    print(f"rows shown {rows}, footer {more!r}, names on the dial {names}, longest task over 9 s of refreshes {lt:.0f} ms")
    pg.screenshot(path=f"{out}/scale-{W}-list.png")
    pg.click(".morerows button"); time.sleep(0.5)
    print("after Show more:", pg.locator(".row:not(.skel)").count())
    target = fake[N // 2]["call_sign"]
    pg.fill("#find", target); time.sleep(0.4)
    print("search for", target, "->", pg.locator(".row:not(.skel)").count(), "row(s)")
    pg.fill("#find", "harbour"); time.sleep(0.4)
    print("search for harbour ->", pg.text_content(".morerows .fine") or pg.locator(".row:not(.skel)").count())
    pg.fill("#find", "")
    pg.click(".tab[data-v=radio]"); time.sleep(0.5)
    pg.screenshot(path=f"{out}/scale-{W}-set.png")
    pg.click(".tab[data-v=list]"); time.sleep(0.5)
    pg.click("#vchart"); time.sleep(2)
    pg.locator("#chart").scroll_into_view_if_needed(); pg.screenshot(path=f"{out}/scale-{W}-map.png")
    empties = sum(1 for f in fake if f["tracks"] == 0)
    print("stations with nothing to play kept out:", empties, "| footer", pg.text_content(".morerows .fine"))
    print("rows marked Same name in the first page:", pg.locator(".row .tag", has_text="Same name").count(), "| kinds chips:", pg.locator("#kinds .btn").all_text_contents())
    pg.locator("#kinds .btn").nth(1).click(); time.sleep(0.4)
    print("after a kind chip:", pg.text_content(".morerows .fine") or pg.locator(".row:not(.skel)").count())
    pg.locator("#v-list").scroll_into_view_if_needed(); pg.screenshot(path=f"{out}/scale-{W}-kinds.png")
    print("errors:", E[:3])
    b.close()
