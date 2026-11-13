"""A station kept in the browser, changed from the studio and seen by a listener: browser_station.py [port]
Serve a test build first (serve_test.sh). A tab raises a station with two recordings; a second
browser, a listener, tunes to it. The keeper then changes everything the studio's Your station
panel holds: name, about, home port and tip address; kinds and mature; the cover and banner; the
schedule's mode and a daily hour kept for going live; a notice; and one recording taken out.
Each change is looked for on the listener's set and station page. Then the notice comes down and
the station is rated explicit, and those are looked for too. Edits reach listeners only after a
carrier's privacy wait (up to a minute and a half), so this takes a few minutes.
Prints PASS or FAIL for each check and exits non-zero on any failure. Screenshots of the panel
go to /tmp/station-1440.png and /tmp/station-390.png, and the listener's set to /tmp/station-heard.png."""
import sys, time, os, wave, tempfile, shutil
import numpy as np
from PIL import Image, ImageDraw
from playwright.sync_api import sync_playwright
port = sys.argv[1] if len(sys.argv) > 1 else "8099"
URL = f"http://127.0.0.1:{port}/"
ok = True
def check(c, what):
    global ok; ok &= bool(c); print(("PASS " if c else "FAIL ") + what, flush=True)
def wait(pg, js, secs):
    end = time.time() + secs
    while time.time() < end:
        if pg.evaluate(js): return True
        time.sleep(0.25)
    raise TimeoutError(js)
def within(pg, js, secs):
    try: return wait(pg, js, secs)
    except TimeoutError: return False

tmp = tempfile.mkdtemp(prefix="station-")
def tone(name, f, secs):
    path = os.path.join(tmp, name); w = wave.open(path, "wb"); w.setnchannels(1); w.setsampwidth(2); w.setframerate(48000)
    w.writeframes((0.3 * 32767 * np.sin(2 * np.pi * f * np.arange(int(48000 * secs)) / 48000)).astype("<i2").tobytes()); w.close()
    return path
KEEP, DROP = tone("Keep Me.wav", 440, 8), tone("Drop Me.wav", 550, 8)
# A picture of the wrong shape for each, so the crop is tested: a tall cover, a wide banner.
COVER, BANNER = os.path.join(tmp, "cover.png"), os.path.join(tmp, "banner.jpg")
im = Image.new("RGB", (900, 1400), (24, 24, 24)); ImageDraw.Draw(im).rectangle([150, 400, 750, 1000], fill=(190, 30, 30)); im.save(COVER)
im = Image.new("RGB", (2600, 700), (20, 60, 120)); ImageDraw.Draw(im).ellipse([1000, 100, 1600, 600], fill=(230, 220, 200)); im.save(BANNER, quality=90)
TIP = "4AdUndXHHZ6cfufTMvppY6JwXNouMBzSkbLYfpAV5Usx3skxNgYeYTRj5UzqtReoS44qo9mtmXCqY45DJ852K5Jv2684Rge"
KINDS = ["music", "jazz", "talk", "shanties", "folk"]
def view(pg, v):
    if pg.evaluate("window.pyrate.state().view") == v: return
    # The station page opens over the radio, from its Page button.
    if v == "page": pg.click(".tab[data-v=radio]"); pg.click("#pagego")
    else: pg.click(f".tab[data-v={v}]")

errs = []
try:
    with sync_playwright() as p:
        b = p.chromium.launch(args=["--autoplay-policy=no-user-gesture-required"])
        # The page runs under its own Content-Security-Policy, as served: anything it refuses is noted.
        def tab(w, h):
            x = b.new_context(viewport={"width": w, "height": h})
            x.add_init_script("window.__csp = []; document.addEventListener('securitypolicyviolation', e => window.__csp.push(e.violatedDirective + ' ' + e.blockedURI))")
            return x.new_page()
        a = tab(1440, 1000)
        a.on("pageerror", lambda e: errs.append("keeper: " + str(e))); a.on("dialog", lambda d: d.accept())
        # The radio's own notes (shown with ?debug): any stem that did not take the station's cargo.
        stems = []; a.on("console", lambda m: stems.append(m.text.split(": ", 1)[-1]) if ": stem:" in m.text else None)
        a.goto(URL + "?debug"); wait(a, "document.getElementById('signal').textContent.includes('Connected')", 90)
        a.click(".tab[data-v=studio]"); a.fill("#st-name", "Ship Shape"); a.fill("#st-about", "Before the edit")
        check(a.is_checked("#st-newoff") and a.is_disabled("#st-newoff"), "the raise form shows the station starting off the dial, fixed")
        a.click("#st-raise")
        a.wait_for_selector("#st-show:not([hidden])"); a.check("#st-written"); a.click("#st-done")
        wait(a, "!document.getElementById('st-status').textContent.includes('ready') && document.getElementById('st-status').textContent.length > 0", 120)
        sid = a.evaluate("window.pyrate.mine()[0]")
        a.set_input_files("#st-files", [KEEP, DROP])
        wait(a, "document.querySelectorAll('#st-queue .qi.done').length === 2", 90)
        check(a.is_visible("#ys") and a.text_content("#ys-lib-v") == "2 recordings", "the keeper's panel shows the station and its two recordings")
        within(a, "!document.getElementById('st-dial').hidden", 30)
        check("plays by its link" in (a.text_content("#st-dialnote") or "") and "joins" in (a.text_content("#st-dialnote") or "") and sid in (a.text_content("#st-link") or ""), "the keeper is told the station plays by its link and when it joins the dial: " + repr(a.text_content("#st-dialnote")))
        # A stranger, with no link, who meets the station only in the list once it is listed.
        d = tab(1440, 900); d.on("pageerror", lambda e: errs.append("stranger: " + str(e)))
        d.goto(URL); wait(d, "document.getElementById('signal').textContent.includes('Connected')", 90)
        # A listener, in a browser of its own, tunes to the station.
        c = tab(1440, 900)
        c.on("pageerror", lambda e: errs.append("listener: " + str(e)))
        c.goto(URL + "#" + sid)
        try: wait(c, "document.getElementById('name').textContent === 'Ship Shape'", 240)
        except TimeoutError: print("keeper: " + a.text_content("#st-status") + " / listener: " + c.text_content("#name") + "\n" + "\n".join(sorted(set(stems)))); raise
        view(c, "page")

        # Every change at once, as a keeper would make them. The tabs and the mini player stay
        # fixed on the screen, and a click scrolled under them would land on them.
        a.evaluate("document.querySelectorAll('.tabs,#mini').forEach(e => { e.style.visibility = 'hidden'; e.style.pointerEvents = 'none' })")
        info = lambda: a.evaluate(f"window.pyrate.station('{sid}')")
        a.click("#ys-colours summary"); a.fill("#ys-name", "Ship Shape Two"); a.fill("#ys-about", "After the edit"); a.fill("#ys-port", "Bristol"); a.fill("#ys-tip", TIP)
        a.click("#ys-colours [data-save]")
        # Music is one the test network's own stations give, so it is in the filters however many stations are about.
        a.click("#ys-details summary"); a.fill("#ys-kinds", "music, jazz, talk, shanties, folk, news"); a.dispatch_event("#ys-kinds", "input")
        check(a.is_visible("#ys-kindnote"), "a sixth kind is said to be left off")
        a.check("#ys-mature"); a.click("#ys-details [data-save]")
        a.click("#ys-art summary"); a.set_input_files("#ys-coverfile", COVER); wait(a, f"!!window.pyrate.station('{sid}').cover", 20)
        a.set_input_files("#ys-bannerfile", BANNER); wait(a, f"!!window.pyrate.station('{sid}').banner", 20)
        a.click("#ys-sched summary"); a.locator(".ys-modes label", has_text="Newest first").click(); a.check("#ys-live"); a.fill("#ys-liveat", "21:30")
        check("same hour each day is kept for you to go live" in a.text_content("#ys-modenote") and a.is_visible("#ys-zone"), "the schedule says the hour is kept, and that its times carry the time zone")
        a.click("#ys-sched [data-save]")
        a.click("#ys-notices summary"); a.fill("#ys-notice", "Back on Friday"); a.click("#ys-notices [data-save]")
        a.click("#ys-lib summary"); drop = [t["track"] for t in info()["library"] if t["title"] == "Drop Me"]
        a.click(f"#ys-library [data-out='{drop[0]}']")
        t_edit = time.time()
        v = info()
        check(v["name"] == "Ship Shape Two" and v["about"] == "After the edit" and v["port"] == "Bristol" and v["tip"] == TIP, "name, about, home port and tip address are written")
        check(v["kinds"] == KINDS and v["mature"], f"five kinds and mature are written ({v['kinds']})")
        check(v["cover"] and v["banner"], "a cover and a banner are written")
        pm = v["programme"] or {}
        check(pm.get("mode", {}).get("kind") == "newest_first" and any(s["live"] and s["span"]["start_min"] == 21 * 60 + 30 for s in pm.get("slots", [])) and len(pm.get("zone", [])) >= 1,
              "the schedule is written: newest first, an hour kept at 21:30, and the time zone with it")
        check([n["text"] for n in v["notices"]] == ["Back on Friday"], "the notice is written")
        check([t["title"] for t in v["library"]] == ["Keep Me"], "the recording is taken out of the library")
        check(a.text_content("#st-mname") == "Ship Shape Two" and "Ship Shape Two" in (a.text_content("#ys-colours-v") or ""), "the studio shows the new name")
        a.set_viewport_size({"width": 1440, "height": 2400}); time.sleep(0.6); a.locator("#ys").screenshot(path="/tmp/station-1440.png")
        a.set_viewport_size({"width": 390, "height": 3200}); time.sleep(0.6); a.locator("#ys").screenshot(path="/tmp/station-390.png")
        a.set_viewport_size({"width": 1440, "height": 1000})

        # The listener sees each change once a carrier shows it.
        LOOK = {
            "name on the set": "document.getElementById('name').textContent === 'Ship Shape Two'",
            "about on the page": "document.getElementById('pabout').textContent === 'After the edit'",
            "home port on the page": "document.getElementById('pmeta').textContent.includes('Bristol')",
            "the Tip button": "!document.getElementById('tip').hidden",
            "the five kinds": f"(() => {{ const k = window.pyrate.card('{sid}')?.kinds || []; return {KINDS}.every(x => k.includes(x)) && [...document.querySelectorAll('#kinds button')].some(b => b.textContent === 'music') }})()",
            "the cover on the set": "(() => { const c = document.getElementById('cover'); return !c.hidden && c.src.startsWith('blob:') && c.naturalWidth === 640 && c.naturalHeight === 640 })()",
            "the banner on the set": "document.getElementById('banner').style.backgroundImage.includes('blob:')",
            "the schedule's mode on the page": "document.getElementById('how').textContent.includes('newest recordings first')",
            "the hour kept for going live": "!!document.querySelector('#listings .lv')",
            "the notice on the page": "document.getElementById('notices').textContent.includes('Back on Friday')",
            "one recording left in the library": f"window.pyrate.card('{sid}')?.tracks === 1",
        }
        seen = {}
        while time.time() - t_edit < 300 and len(seen) < len(LOOK):
            view(c, "page")
            for k, js in LOOK.items():
                if k not in seen and c.evaluate(js): seen[k] = round(time.time() - t_edit)
            time.sleep(2)
        on = c.evaluate("window.pyrate.state().station")
        check(on == sid, "the listener's set stays on the station its link opened" + ("" if on == sid else f" (moved to {on})"))
        for k in LOOK: check(k in seen, f"the listener sees {k}" + (f" ({seen[k]} s)" if k in seen else ""))
        if "the banner on the set" in seen:
            dims = c.evaluate("new Promise(r => { const i = new Image(); i.onload = () => r([i.naturalWidth, i.naturalHeight]); i.onerror = () => r(null); i.src = document.getElementById('banner').style.backgroundImage.slice(5, -2) })")
            check(dims == [1500, 500], f"the banner is cropped to 1500 by 500 ({dims})")
        check("Drop Me" not in (c.text_content("#next") or "") and "Drop Me" not in (c.text_content("#listings") or ""), "the recording taken out is not on the listener's schedule")
        c.locator("#set").screenshot(path="/tmp/station-heard.png")
        # A kind in the filters finds the station. Kinds only one station gives show when the list is short.
        view(c, "list"); chips = c.evaluate("[...document.querySelectorAll('#kinds button')].map(b => b.textContent)")
        c.locator("#kinds button", has_text="music").first.click(); time.sleep(0.5)
        check(c.locator(f".row[data-id='{sid}']").count() == 1, f"the music filter lists the station ({sum(k in chips for k in KINDS)} of its five kinds in the filters)")
        c.locator("#kinds button", has_text="music").first.click(); view(c, "page")

        # Listed: a day after it first went on the air (a minute in a test build, PYRATE_LIST_AGE_MS)
        # and once the keeper's browser has done its work, the station is in a stranger's list.
        inlist = f"!!document.querySelector('#rows .row[data-id=\"{sid}\"]')"
        # Every new station starts off the dial; the keeper puts it on.
        check(a.is_checked("#st-linkonly"), "a new station starts off the dial")
        check(not d.evaluate(inlist), "and is in no stranger's list while it is")
        a.uncheck("#st-linkonly")
        view(d, "list"); t_l = time.time()
        check(within(d, inlist, 300), f"a stranger's list shows the station once it is listed ({round(time.time() - t_l)} s more)")
        # Kept off the dial: out of strangers' lists, and its link still plays.
        a.check("#st-linkonly"); t_o = time.time()
        check("Off the dial" in (a.text_content("#st-dialnote") or ""), "the keeper's switch keeps it off the dial")
        gone = None
        while time.time() - t_o < 300 and gone is None:
            if not d.evaluate(inlist): gone = round(time.time() - t_o)
            time.sleep(2)
        check(gone is not None, f"a station kept off the dial leaves strangers' lists ({gone} s)")
        check(c.evaluate("window.pyrate.state().station") == sid, "and its link still plays it")
        # The notice comes down, and the station calls itself explicit.
        a.click("#ys-notices summary") if not a.is_visible("#ys-noticelist") else None
        a.click("#ys-noticelist [data-down]")
        a.click("#ys-details summary") if not a.is_visible("#ys-explicit") else None
        a.check("#ys-explicit"); a.click("#ys-details [data-save]"); t_two = time.time()
        check(info()["notices"] == [] and info()["explicit"], "the notice is taken down and explicit is written")
        down = rated = None
        while time.time() - t_two < 300 and (down is None or rated is None):
            view(c, "page")
            if down is None and "Back on Friday" not in (c.text_content("#notices") or ""): down = round(time.time() - t_two)
            if rated is None and c.evaluate(f"window.pyrate.card('{sid}')?.explicit === true"): rated = round(time.time() - t_two)
            time.sleep(2)
        check(down is not None, f"the listener sees the notice gone ({down} s)")
        check(rated is not None, f"the listener's radio has the station as explicit ({rated} s)")
        # Explicit stations stay out of a listener's list unless they ask for them.
        view(c, "list"); time.sleep(4.5)
        check(c.locator(f".row[data-id='{sid}']").count() == 0, "the listener's list leaves the explicit station out")
        csp = a.evaluate("window.__csp") + c.evaluate("window.__csp")
        b.close()
    check(not csp, "nothing is refused by the page's own policy: %s" % csp[:3])
    for s in sorted(set(stems)): print("note: the keeper's " + s)
    check(not errs, "no page errors: %s" % errs[:3])
finally:
    shutil.rmtree(tmp, ignore_errors=True)
sys.exit(0 if ok else 1)
