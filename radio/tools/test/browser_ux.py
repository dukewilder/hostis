"""The player as a listener meets it, timed and checked: browser_ux.py <port> <out dir> [width]
Serve a test build first (serve_test.sh). Times the first row, the first station on the set,
Listen to sound and a switch to sound; checks that no title shows before its sound, that the
name changes at once on a switch, that Stop stops (also pressed while still connecting), that
every section opens and Back returns, and that the mini player comes when the set scrolls away.
Writes screenshots to <out dir> and prints one line per check."""
import sys, time, json
from playwright.sync_api import sync_playwright
port, out = sys.argv[1], sys.argv[2]
W = int(sys.argv[3]) if len(sys.argv) > 3 else 390
H = 844 if W < 600 else 900
bad = []
def check(ok, what):
    print(("ok   " if ok else "FAIL ") + what)
    if not ok: bad.append(what)
with sync_playwright() as p:
    b = p.chromium.launch(args=["--autoplay-policy=no-user-gesture-required"])
    pg = b.new_page(viewport={"width": W, "height": H}, device_scale_factor=1)
    E = []
    pg.on("pageerror", lambda e: E.append(str(e)))
    pg.on("console", lambda m: m.type == "error" and E.append(m.text))
    t0 = time.time()
    pg.goto(f"http://127.0.0.1:{port}/index.html")
    st = lambda: pg.evaluate("window.pyrate.state()")
    t_name = t_row = None
    for i in range(400):
        if t_name is None and (pg.text_content("#name") or "").strip(): t_name = time.time() - t0
        if t_row is None and pg.locator(".row:not(.skel)").count(): t_row = time.time() - t0
        if t_name is not None and t_row is not None: break
        time.sleep(0.05)
    pg.screenshot(path=f"{out}/{W}-1-open.png")
    print(f"time  name on the set {t_name and round(t_name, 2)} s, first row {t_row and round(t_row, 2)} s")
    check(t_name is not None and t_name < 1.5, "a station is on the set within 1.5 s")
    # Listen, watching every tenth of a second that no title shows before its sound.
    t1 = time.time(); pg.click("#go"); early = []; t_heard = None; stages = []
    for i in range(900):
        s = st(); np = pg.text_content("#np") or ""
        if s["stage"] and (not stages or stages[-1] != s["stage"]): stages.append(s["stage"])
        if s["heard"]: t_heard = time.time() - t1; break
        title = pg.evaluate("(() => { const p = window.pyrate.plan(); return p && p.kind === 'track' ? p.title : null })()")
        if title and np == title: early.append(np)
        if i == 3: pg.screenshot(path=f"{out}/{W}-2-tuning.png")
        time.sleep(0.1)
    print(f"time  Listen to sound {t_heard and round(t_heard, 2)} s, stages {stages}")
    check(t_heard is not None, "Listen reaches sound")
    check(not early, "no title before its sound" + (f" (saw {early[0]!r})" if early else ""))
    time.sleep(1.5); pg.screenshot(path=f"{out}/{W}-3-playing.png")
    check((pg.text_content("#go .lbl") or "") == "Stop", "the button says Stop while playing")
    # Switch to another station from the list.
    rows = pg.locator(".row:not(.skel)")
    first = st()["station"]; other = None
    for i in range(rows.count()):
        if rows.nth(i).get_attribute("data-id") != first: other = rows.nth(i); break
    if other is not None:
        name = other.locator(".name").evaluate("e => e.firstChild.textContent")
        t2 = time.time(); other.click()
        time.sleep(0.12)
        check((pg.text_content("#name") or "") == name, f"the name changes at once on a switch ({name})")
        t_sw = None
        for i in range(900):
            s = st()
            if s["heard"] and s["station"] == other.get_attribute("data-id"): t_sw = time.time() - t2; break
            time.sleep(0.1)
        print(f"time  switch to sound {t_sw and round(t_sw, 2)} s")
        check(t_sw is not None, "a switch reaches sound")
    # Stop stops.
    pg.click("#go"); time.sleep(0.4); s = st()
    check(not s["playing"] and s["scheduled"] == 0, "Stop stops at once")
    time.sleep(2); s = st()
    check(not s["playing"] and s["scheduled"] == 0 and s["ctx"] == "suspended", "and stays stopped")
    # Listen and Stop straight away, while it is still tuning in.
    pg.click("#go"); time.sleep(0.25); pg.click("#go"); time.sleep(4); s = st()
    check(not s["playing"] and s["scheduled"] == 0, "Stop pressed while tuning in stays stopped")
    # A station left on the set brings its sound ahead: turned to on the dial, left a few
    # seconds, then Listen.
    first = st()["station"]
    pg.evaluate("(() => { const b = [...document.querySelectorAll('#names button')].find(b => b.dataset.id !== window.pyrate.state().station); b && b.click() })()")
    time.sleep(8); sat = st()["station"]
    t3 = time.time(); pg.click("#go"); t_sat = None
    for i in range(900):
        s = st()
        if s["heard"] and s["station"] == sat: t_sat = time.time() - t3; break
        time.sleep(0.05)
    print(f"time  Listen to sound after the station sat on the set 8 s: {t_sat and round(t_sat, 2)} s")
    check(t_sat is not None and t_sat < 1.0, "a station left on the set plays within a second of Listen")
    pg.click("#go"); time.sleep(0.5)
    # The station's actions are one toolbar on one line; More holds the sleep timer.
    tops = pg.eval_on_selector_all(".controls .buttons .btn:not([hidden])", "bs => bs.map(b => Math.round(b.getBoundingClientRect().top))")
    check(len(tops) >= 2 and len(set(tops)) == 1, f"the station's actions sit on one line ({len(tops)} of them)")
    pg.click("#more"); time.sleep(0.3)
    check(pg.is_visible("#morebox") and pg.is_visible("#sleepbtns"), "More opens with the sleep timer")
    pg.click("#sleepbtns [data-m='15']"); time.sleep(0.2)
    check((pg.text_content("#more span") or "") == "Sleep" and pg.is_visible("#sleepoff"), "a sleep timer is set and shown")
    pg.click("#sleepoff"); pg.keyboard.press("Escape"); time.sleep(0.3)
    check(not pg.is_visible("#morebox") and (pg.text_content("#more span") or "") == "More", "the timer cancels and Escape closes More")
    # The sections, and Back.
    for v, label in [("page", "Station page"), ("settings", "Settings"), ("list", "Stations")]:
        pg.click(f".tab[data-v={v}]"); time.sleep(0.5)
        check(st()["view"] == v and pg.is_visible(f"#v-{v}"), f"the {label} tab opens its section")
        pg.screenshot(path=f"{out}/{W}-4-{v}.png", full_page=False)
    pg.click(".tab[data-v=page]"); time.sleep(0.3); pg.go_back(); time.sleep(0.5)
    check(st()["view"] == "list", "Back returns to the stations")
    # The mini player, once the set scrolls away.
    pg.evaluate("window.scrollTo(0, document.body.scrollHeight)"); time.sleep(0.8)
    # On a tall screen a short page may never scroll the set away: then there is nothing to show.
    if pg.evaluate("(() => { const r = document.getElementById('set').getBoundingClientRect(); return Math.max(0, Math.min(r.bottom, innerHeight) - Math.max(r.top, 0)) / r.height > 0.1 })()"):
        pg.evaluate("document.body.style.minHeight = '4000px'; window.scrollTo(0, 3000)"); time.sleep(0.8)
    check(pg.is_visible("#mini"), "the mini player shows once the set is out of view")
    pg.screenshot(path=f"{out}/{W}-5-mini.png")
    pg.click("#mini-go"); time.sleep(0.5)
    check(st()["playing"], "the mini player's button plays")
    pg.click("#mini-go"); time.sleep(0.5)
    check(not st()["playing"], "and stops")
    pg.screenshot(path=f"{out}/{W}-6-full.png", full_page=True)
    check(not E, "no errors in the console" + (f": {E[:2]}" if E else ""))
    b.close()
print("FAILED: " + "; ".join(bad) if bad else "all passed")
sys.exit(1 if bad else 0)
