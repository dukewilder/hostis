"""F4: a browser tab under attack keeps playing. browser_attack.py <port> <relay url> <pentest> <attack> [secs]
Serve a test build of browser/www/dist (PYRATE_HARBOUR and PYRATE_RELAYS set) on that port first,
with a relay and a Lantern carrying a station. The tab tunes to the first station it finds; then
`pentest --hit` attacks the tab through the relay while this watches the tab play. Prints PASS or FAIL."""
import json, subprocess, sys, time
from playwright.sync_api import sync_playwright

port, relay, pentest, attack = sys.argv[1:5]
secs = int(sys.argv[5]) if len(sys.argv) > 5 else 60

def chrome_mb():
    out = subprocess.run(["ps", "-C", "chrome", "-o", "rss="], capture_output=True, text=True).stdout.split()
    return sum(int(x) for x in out) // 1024

with sync_playwright() as p:
    b = p.chromium.launch(executable_path="/usr/bin/google-chrome", args=["--autoplay-policy=no-user-gesture-required"])
    pg = b.new_page(); errors = []
    pg.on("pageerror", lambda e: errors.append(str(e)))
    pg.goto(f"http://127.0.0.1:{port}/index.html")
    for _ in range(60):
        time.sleep(1)
        if pg.evaluate("window.pyrate && window.pyrate.state().stations") > 0: break
    pg.click("#go")
    for i in range(90):
        time.sleep(1)
        st = pg.evaluate("window.pyrate.state()")
        if st["scheduled"] > 0 and st["ahead_ms"] > 0: break
    st = pg.evaluate("window.pyrate.state()")
    node = pg.evaluate("window.pyrate.node()")
    print("tab", node, "plays", st["station"], "ahead", st["ahead_ms"], "ms")
    if not st["playing"] or not st["station"]:
        print("FAIL the tab never started playing"); sys.exit(1)
    before = chrome_mb()
    proc = subprocess.Popen([pentest, "--hit", node, relay, st["station"], attack, str(secs)], stdout=subprocess.PIPE, text=True)
    stalls, peak, lows = 0, before, []
    while proc.poll() is None:
        time.sleep(1)
        s = pg.evaluate("window.pyrate.state()")
        peak = max(peak, chrome_mb())
        lows.append(s["ahead_ms"])
        if not s["playing"] or s["ahead_ms"] <= 0: stalls += 1
    hit = json.loads((proc.stdout.read().strip().splitlines() or ["{}"])[-1])
    time.sleep(10)
    after = pg.evaluate("window.pyrate.state()")
    ok_play = stalls == 0 and after["playing"] and after["ahead_ms"] > 0
    ok_mem = peak - before < 300
    ok_ans = hit.get("probe_misses", 1) == 0
    print(("PASS" if ok_play else "FAIL"), f"{attack}: the tab kept playing ({stalls} seconds without sound ahead; least ahead {min(lows) if lows else 0} ms)")
    print(("PASS" if ok_mem else "FAIL"), f"{attack}: browser memory {before} -> peak {peak} MB")
    print(("PASS" if ok_ans else "FAIL"), f"{attack}: the tab answered a listener throughout (worst {hit.get('probe_worst_ms')} ms, {hit.get('probe_misses')} missed; {hit.get('streams_opened')} attack streams opened, {hit.get('streams_cut')} cut)")
    print(("PASS" if not errors else "FAIL"), f"{attack}: no page errors {errors[:2]}")
    b.close()
