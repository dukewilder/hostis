"""The crew's crowd test: browser_crew.py [port] [tabs] [settle s] [window s]
Bring the test network up with the Lanterns' stats on (PYRATE_STATS_S=5 sh tools/test/net_up.sh)
and serve it (serve_test.sh); the open player is on 8099. Runs twice: eight tabs on one station
with "Carry the radio while you listen" on, then eight with it off. Tabs join a few seconds apart,
each in a browser of its own. After the crowd settles, it measures over a window where the
chunks the tabs play come from (another listener's deck, or a Lantern), and the bytes the
Lanterns serve as exits. A fifth argument, on or off, runs that half alone and only reports. Prints PASS or FAIL for each acceptance check and exits non-zero on any
failure:
- with the switch on, at least half of the chunks the tabs play come from tabs;
- the Lanterns' bytes served per listener fall by at least half against the switch off;
- the first Listen sounds within 2 s on the test network (the middle tab's time, both runs)."""
import re, sys, time
from playwright.sync_api import sync_playwright

port = sys.argv[1] if len(sys.argv) > 1 else "8099"
TABS = int(sys.argv[2]) if len(sys.argv) > 2 else 8
SETTLE = int(sys.argv[3]) if len(sys.argv) > 3 else 60
WINDOW = int(sys.argv[4]) if len(sys.argv) > 4 else 90
ONLY = sys.argv[5] if len(sys.argv) > 5 else ""
LOGS = ["/tmp/net-relay.log", "/tmp/net-carry.log"]
ok = True


def check(c, what):
    global ok
    ok &= bool(c)
    print(("PASS " if c else "FAIL ") + what, flush=True)


def served():
    """Bytes the Lanterns have served as exits or straight to a peer, from their stats lines."""
    total = 0
    for f in LOGS:
        last = 0
        try:
            for line in open(f, errors="replace"):
                m = re.match(r"stats \d+ served (\d+) relayed (\d+)", line)
                if m:
                    last = int(m.group(1))
        except OSError:
            pass
        total += last
    return total


def wait(pg, js, secs):
    end = time.time() + secs
    while time.time() < end:
        try:
            if pg.evaluate(js):
                return True
        except Exception:
            pass
        time.sleep(0.1)
    return False


def run(b, crew_on):
    label = "on" if crew_on else "off"
    tabs, ctxs, firsts, errors = [], [], [], []
    for i in range(TABS):
        ctx = b.new_context(viewport={"width": 1280, "height": 900})
        ctx.add_init_script(f"try {{ localStorage.setItem('pyrate.crew', '{'1' if crew_on else '0'}') }} catch (e) {{}}")
        ctx.on("weberror", lambda e: errors.append(str(e.error)))
        pg = ctx.new_page()
        pg.goto(f"http://127.0.0.1:{port}/index.html")
        wait(pg, "(document.querySelector('#name')?.textContent || '').includes('Gangway')", 40)
        t = time.time()
        pg.click("#go")
        heard = wait(pg, "window.pyrate.state().heard", 30)
        firsts.append(time.time() - t if heard else None)
        print(f"  [{label}] tab {i + 1} listening, first sound after {firsts[-1] and round(firsts[-1], 2)} s", flush=True)
        tabs.append(pg)
        ctxs.append(ctx)
        time.sleep(3)
    print(f"  [{label}] the crowd is in; settling {SETTLE} s", flush=True)
    time.sleep(SETTLE)
    s0 = served()
    c0 = [pg.evaluate("window.pyrate.crew()") or {} for pg in tabs]
    time.sleep(WINDOW)
    s1 = served()
    c1 = [pg.evaluate("window.pyrate.crew()") or {} for pg in tabs]
    playing = sum(1 for pg in tabs if pg.evaluate("window.pyrate.state().playing && window.pyrate.state().ahead_ms > 0"))
    d = lambda k: sum(b.get(k, 0) - a.get(k, 0) for a, b in zip(c0, c1))
    from_tabs, from_lanterns = d("from_decks"), d("from_others")
    share = from_tabs / max(1, from_tabs + from_lanterns)
    per = (s1 - s0) / TABS
    passed = sum(c.get("passed_today", 0) for c in c1)
    decks = sum(len(c.get("decks", [])) for c in c1)
    mid = sorted(f for f in firsts if f is not None)
    first = mid[len(mid) // 2] if len(mid) == len(firsts) else None
    print(f"  [{label}] over {WINDOW} s: chunks from tabs {from_tabs}, from Lanterns {from_lanterns} ({share:.0%} from tabs); "
          f"Lanterns served {(s1 - s0) // 1024} KB, {per / 1024:.0f} KB per listener; tabs passed on {passed // 1024} KB in all; "
          f"{decks} decks open; {playing} of {TABS} playing; first Listen {[f and round(f, 2) for f in firsts]}", flush=True)
    if crew_on:
        tries = [sum(b.get("tries", [0] * 6)[i] - a.get("tries", [0] * 6)[i] for a, b in zip(c0, c1)) for i in range(6)]
        print(f"  [{label}] deck asks since each tab opened: {[sum(c.get('tries', [0] * 6)[i] for c in c1) for i in range(6)]}", flush=True)
        print(f"  [{label}] decks each tab knows of its station: {[c.get('known_decks', 0) for c in c1]}", flush=True)
        print(f"  [{label}] deck asks: no deck {tries[0]}, its own {tries[1]}, no route {tries[2]}, no answer {tries[3]}, without it {tries[4]}, answered {tries[5]}", flush=True)
    for ctx in ctxs:
        ctx.close()
    return {"share": share, "per": per, "first": first, "playing": playing, "errors": errors, "chunks": from_tabs + from_lanterns}


with sync_playwright() as p:
    b = p.chromium.launch(args=["--autoplay-policy=no-user-gesture-required"])
    on = run(b, True) if ONLY != "off" else None
    if ONLY:
        b.close()
        sys.exit(0)
    time.sleep(10)
    off = run(b, False)
    b.close()

check(on["chunks"] > 0 and on["share"] >= 0.5, f"switch on: at least half the chunks played came from tabs ({on['share']:.0%})")
check(off["share"] == 0, f"switch off: no chunk came from a tab ({off['share']:.0%})")
check(off["per"] > 0 and on["per"] <= off["per"] / 2, f"the Lanterns' bytes served per listener fell by at least half ({on['per'] / 1024:.0f} KB on against {off['per'] / 1024:.0f} KB off)")
check(on["first"] is not None and on["first"] < 2.0, f"switch on: first Listen under 2 s (middle tab {on['first'] and round(on['first'], 2)} s)")
check(off["first"] is not None and off["first"] < 2.0, f"switch off: first Listen under 2 s (middle tab {off['first'] and round(off['first'], 2)} s)")
check(on["playing"] == TABS and off["playing"] == TABS, f"every tab still playing at the end ({on['playing']} on, {off['playing']} off)")
check(not on["errors"] and not off["errors"], f"no page errors {(on['errors'] + off['errors'])[:2]}")
sys.exit(0 if ok else 1)
