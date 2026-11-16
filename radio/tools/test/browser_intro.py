"""Introduced doors: browser_intro.py
Bring up the network with tools/test/intro_up.sh first, its gang/ recording longer than the test
(seven minutes, say, so the tab needs fresh sound after the relay stops), and build the test wasm
(PYRATE_LIST_AGE_MS=60000 PYRATE_DOOR_AGE_MS=30000). This builds a copy of the player with no doors
built in (port 8095), so no door is listed anywhere. Then:
  1. a tab joins through the relay and plays;
  2. it asks a middle for a way in, through its own guard, and the middle introduces it to D, the
     listener's app that is a door: no note names D's address, and no port is mapped on either
     side; the tab opens a direct WebRTC link to D, which relays its hops (D is not its guard:
     one middle drew it, and rule A wants two);
  3. the relay is stopped, and the tab keeps playing through D.
Prints PASS or FAIL for each check and exits non-zero on any failure."""
import json, os, shutil, subprocess, sys, time
from playwright.sync_api import sync_playwright

R = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
RID, A, GANG, D = open("/tmp/intro-ids").read().split()
PORT = 8095
ok = True


def check(c, what):
    global ok
    ok &= bool(c)
    print(("PASS " if c else "FAIL ") + what, flush=True)


def wait(pg, js, secs):
    end = time.time() + secs
    while time.time() < end:
        try:
            v = pg.evaluate(js)
            if v:
                return v
        except Exception:
            pass
        time.sleep(0.25)
    return None


def build(out, port):
    json.dump([{"id": GANG, "name": "Gangway", "call_sign": ""}], open("/tmp/intro-harbour.json", "w"))
    env = dict(os.environ, PYRATE_HARBOUR=f"{A}@http://127.0.0.1:3340", PYRATE_RELAYS="http://127.0.0.1:3340",
               PYRATE_HARBOUR_STATIONS="/tmp/intro-harbour.json", PYRATE_OPEN_PLAYERS=f"http://127.0.0.1:{port}/", PYRATE_DOORS="", PYRATE_STUN="")
    subprocess.run([sys.executable, f"{R}/browser/build.py"], env=env, check=True, stdout=subprocess.DEVNULL)
    shutil.rmtree(out, ignore_errors=True)
    shutil.copytree(f"{R}/browser/www/dist-open", out)
    if subprocess.run(["pgrep", "-f", f"http.server {port}"], stdout=subprocess.DEVNULL).returncode != 0:
        subprocess.Popen([sys.executable, "-m", "http.server", str(port), "--bind", "127.0.0.1"], cwd=out, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True)


build("/tmp/dist-intro", PORT)
subprocess.run([sys.executable, f"{R}/browser/build.py"], check=True, stdout=subprocess.DEVNULL)
time.sleep(1)
log_d = lambda: open("/tmp/intro-d.log", errors="replace").read()

with sync_playwright() as p:
    b = p.chromium.launch(args=["--autoplay-policy=no-user-gesture-required"])
    ctx = b.new_context(viewport={"width": 1280, "height": 900})
    ctx.add_init_script("try { localStorage.setItem('pyrate.crew', '1') } catch (e) {}")
    errors = []
    ctx.on("weberror", lambda e: errors.append(str(e.error)))
    pg = ctx.new_page()
    pg.on("console", lambda m: print("  console:", m.text, flush=True) if ("introduc" in m.text or "door" in m.text) else None)
    pg.goto(f"http://127.0.0.1:{PORT}/index.html")
    wait(pg, "(document.querySelector('#name')?.textContent || '').includes('Gangway')", 60)
    t = time.time()
    pg.click("#go")
    heard = wait(pg, "window.pyrate.state().heard", 40)
    check(heard, f"the tab plays through the relay (first sound after {heard and round(time.time() - t, 1)} s)")
    # 1. No door is listed anywhere: the tab knows none from a note, and D says it is listed nowhere.
    check("door open for introduced tabs only" in log_d(), "the app door opened for introduced tabs only, its address in no note")
    check(not any(w in log_d() for w in ["UPnP-IGD mapped", "router's port", "NAT-PMP", "PCP mapped"]), "no router was asked for a port on the door's side")
    # 2. Introduced, and through D.
    t = time.time()
    through = wait(pg, "window.pyrate.doors()?.through_intro", 150)
    s = pg.evaluate("window.pyrate.doors()") or {}
    print(f"  after {round(time.time() - t, 1)} s: {s}", flush=True)
    check(through == D, f"a middle introduced the tab to the app door D ({through and through[:10]}), after {s.get('intro_tries')} tries")
    check(s.get("known", 1) == 0, "the tab never learnt a door from a note")
    check("a tab introduced by" in log_d(), "the door took the introduction from a middle")
    check(s.get("relays") and not s.get("door_guard"), "the door relays the tab's hops and is not its guard (one middle drew it)")
    check(s.get("first_hop") and s.get("first_hop") != D, f"the tab's guard is another radio ({(s.get('first_hop') or '')[:10]})")
    time.sleep(20)
    s0 = pg.evaluate("window.pyrate.doors()") or {}
    asks0, bytes0 = s0.get("asks", 0), s0.get("bytes", 0)
    check(asks0 > 0, f"the tab's hops go through the door ({asks0} requests, {bytes0 // 1024} KB over its channel)")
    # 3. The relay stops; the tab plays on through D.
    subprocess.run(["pkill", "-f", "dir /tmp/intro-r"], check=False)
    print("  the relay is stopped", flush=True)
    playing, aheads = 0, []
    for i in range(6):
        time.sleep(15)
        st = pg.evaluate("window.pyrate.state()")
        aheads.append(st.get("ahead_ms", 0))
        if st.get("playing") and st.get("ahead_ms", 0) > 0:
            playing += 1
    s = pg.evaluate("window.pyrate.doors()") or {}
    print(f"  ahead over 90 s with no relay {aheads}; door {s}", flush=True)
    check(s.get("relay_ok_ago", 0) > 20000 and s.get("door_only"), "no relay has answered since it stopped")
    more = s.get("bytes", 0) - bytes0
    check(s.get("asks", 0) > asks0 + 3 and more > 200_000, f"fresh sound came through the door with the relay gone ({s.get('asks', 0) - asks0} requests, {more // 1024} KB)")
    check(playing == 6, f"the tab kept playing through the door with no relay ({playing} of 6 looks)")
    check(s.get("through_intro") == D, "still through the same door")
    b.close()

check(not errors, f"no page errors {errors[:2]}")
sys.exit(0 if ok else 1)
