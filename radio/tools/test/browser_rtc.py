"""A tab never answers an offer of a direct link: browser_rtc.py [port] [relay url]
Serve a test build first (serve_test.sh; the open player is on 8099). Three tabs, each in a
browser of its own, all with "Carry the radio while you listen" on. Tab A plays. Tab B keeps A
as its guard from an earlier visit (as a returning listener would), so A is its first hop, and
offers it a direct link; tab C keeps none. A tab's id lasts one visit, so no honest requester
keeps one as a guard, and answering would send UDP from A's own address to wherever the offer
said. Checks that A answers no offer, that B goes on playing over the relay, and that no tab
holds a link. Prints PASS or FAIL for each check and exits non-zero on any failure."""
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
    time.sleep(40)
    links = {"A": a.evaluate("window.pyrate.rtc()"), "B": bb.evaluate("window.pyrate.rtc()"), "C": c.evaluate("window.pyrate.rtc()")}
    print(f"  links open: { {k: [(l['peer'][:10], l['role']) for l in v] for k, v in links.items()} }", flush=True)
    check(links == {"A": [], "B": [], "C": []}, "A answered no offer of a direct link, and no tab holds one")
    check(bb.evaluate("window.pyrate.state().playing && window.pyrate.state().ahead_ms > 0"), "B plays on over the relay")
    b.close()
sys.exit(0 if ok else 1)
