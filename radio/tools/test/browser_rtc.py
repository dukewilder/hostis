"""The direct link to the guard: browser_rtc.py [port] [relay url]
Serve a test build first (serve_test.sh; the open player is on 8099). Three tabs, each in a
browser of its own, all with "Carry the radio while you listen" on. Tab A plays. Tab B keeps A
as its guard from an earlier visit (as a returning listener would), so A is its first hop; tab C
keeps none. Checks that B's leg to A comes to run over a WebRTC data channel, that the bytes of
that leg over the relay fall once it does, that A takes nothing but forwards over it, and that no
other leg opens one: C opens none, and no deck or way in ever does. Prints PASS or FAIL for each
check and exits non-zero on any failure."""
import sys, time
from playwright.sync_api import sync_playwright

port = sys.argv[1] if len(sys.argv) > 1 else "8099"
relay = sys.argv[2] if len(sys.argv) > 2 else "http://127.0.0.1:3340/"
ok = True


def check(c, what):
    global ok
    ok &= bool(c)
    print(("PASS " if c else "FAIL ") + what, flush=True)


def wait(pg, js, secs):
    end = time.time() + secs
    while time.time() < end:
        try:
            if pg.evaluate(js):
                return True
        except Exception:
            pass
        time.sleep(0.25)
    return False


def tab(b, guards=None):
    ctx = b.new_context(viewport={"width": 1280, "height": 900})
    init = "try { localStorage.setItem('pyrate.crew', '1');"
    if guards:
        init += f" localStorage.setItem('pyrate.guards', JSON.stringify({guards!r}));"
    ctx.add_init_script(init + " } catch (e) {}")
    pg = ctx.new_page()
    pg.goto(f"http://127.0.0.1:{port}/index.html")
    wait(pg, "(document.querySelector('#name')?.textContent || '').includes('Gangway')", 40)
    t = time.time()
    pg.click("#go")
    heard = wait(pg, "window.pyrate.state().heard", 30)
    return pg, (time.time() - t if heard else None)


with sync_playwright() as p:
    b = p.chromium.launch(args=["--autoplay-policy=no-user-gesture-required"])
    a, ta = tab(b)
    aid = a.evaluate("window.pyrate.node()")
    print(f"  tab A {aid[:10]} listening after {ta and round(ta, 2)} s", flush=True)
    bb, tb = tab(b, [f"{aid}@{relay}"])
    print(f"  tab B listening through A after {tb and round(tb, 2)} s", flush=True)
    c, tc = tab(b)
    check(bb.evaluate("window.pyrate.crew().first_hop") == aid, "B keeps A as its guard, and A is its first hop")
    linked = wait(bb, f"window.pyrate.rtc().some(l => l.peer === '{aid}' && l.role === 'ask')", 90)
    check(linked, "B's leg to its guard runs over a WebRTC data channel")
    s0 = bb.evaluate("window.pyrate.crew()")
    time.sleep(60)
    s1 = bb.evaluate("window.pyrate.crew()")
    rtc = s1["rtc_bytes"] - s0["rtc_bytes"]
    over_relay = s1["relay_first_bytes"] - s0["relay_first_bytes"]
    before = s0["relay_first_bytes"]
    print(f"  B's leg to A: before the link {before // 1024} KB over the relay; over the next 60 s {rtc // 1024} KB over the link and {over_relay // 1024} KB over the relay", flush=True)
    check(rtc > 0 and over_relay * 4 < rtc, "with the link open, the guard's leg takes no more than a fifth over the relay")
    check(bb.evaluate("window.pyrate.state().playing && window.pyrate.state().ahead_ms > 0"), "B is still playing")
    sa = a.evaluate("window.pyrate.crew()")
    check(sa["rtc_served"] > 0 and sa["rtc_refused"] == 0, f"A took only forwards over the link ({sa['rtc_served']} forwards, {sa['rtc_refused']} refused)")
    links = {"A": a.evaluate("window.pyrate.rtc()"), "B": bb.evaluate("window.pyrate.rtc()"), "C": c.evaluate("window.pyrate.rtc()")}
    print(f"  links open: { {k: [(l['peer'][:10], l['role']) for l in v] for k, v in links.items()} }", flush=True)
    bid = bb.evaluate("window.pyrate.node()")
    only = links["C"] == [] and links["B"] == [{"peer": aid, "role": "ask"}] and links["A"] == [{"peer": bid, "role": "serve"}]
    check(only, "no other leg opened a link: none for C, which keeps no guard, and none for any deck or way in")
    b.close()
sys.exit(0 if ok else 1)
