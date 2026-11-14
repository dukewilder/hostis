"""Doors with no relay: browser_doors.py
Bring up the network with tools/test/doors_up.sh first (no relay runs at all), and build the test
wasm (PYRATE_LIST_AGE_MS=60000 PYRATE_DOOR_AGE_MS=30000). This builds two copies of the player:
one carrying door D1 (port 8093), one carrying none (8094). Then:
  1. four tabs on the first copy join through a door from its built-in list, play and carry;
  2. a tab on the bare copy, opened from a link whose fragment carries D2 and D1, joins through
     those doors, and draws its guard only from a door two sources name;
  3. a shared link carries at most two doors, none of them a carrier of its station.
Prints PASS or FAIL for each check, first Listen for every tab, and exits non-zero on failure."""
import base64, json, os, shutil, socket, subprocess, sys, time
from playwright.sync_api import sync_playwright

R = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
A, GANG, WATCH, D1, D2 = open("/tmp/doors-ids").read().split()
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
        time.sleep(0.1)
    return None


def note_door(h):
    # Gossip::Door, postcard: variant, node[32], addr (varint len + bytes), fp[32], ...
    b = bytes.fromhex(h)
    i = 1
    node = b[i:i + 32]; i += 32
    n = b[i]; i += 1
    addr = b[i:i + n].decode(); i += n
    fp = b[i:i + 32]
    return node.hex(), addr, fp.hex()


def code(addr, fp):
    ip, port = addr.rsplit(":", 1)
    raw = socket.inet_aton(ip) + int(port).to_bytes(2, "big") + bytes.fromhex(fp)
    return base64.urlsafe_b64encode(raw).decode().rstrip("=")


def build(doors, out, port):
    env = dict(os.environ, PYRATE_HARBOUR=f"{A}@http://127.0.0.1:3340", PYRATE_RELAYS="http://127.0.0.1:3340",
               PYRATE_HARBOUR_STATIONS="/tmp/doors-harbour.json", PYRATE_OPEN_PLAYERS=f"http://127.0.0.1:{port}/", PYRATE_DOORS=doors)
    subprocess.run([sys.executable, f"{R}/browser/build.py"], env=env, check=True, stdout=subprocess.DEVNULL)
    shutil.rmtree(out, ignore_errors=True)
    shutil.copytree(f"{R}/browser/www/dist-open", out)
    if subprocess.run(["pgrep", "-f", f"http.server {port}"], stdout=subprocess.DEVNULL).returncode != 0:
        subprocess.Popen([sys.executable, "-m", "http.server", str(port), "--bind", "127.0.0.1"], cwd=out, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True)


json.dump([{"id": GANG, "name": "Gangway", "call_sign": ""}], open("/tmp/doors-harbour.json", "w"))
build(D1, "/tmp/dist-doors", 8093)
build("", "/tmp/dist-doors-bare", 8094)
subprocess.run([sys.executable, f"{R}/browser/build.py"], check=True, stdout=subprocess.DEVNULL)
d1_id, d1_addr, d1_fp = note_door(D1)
d2_id, d2_addr, d2_fp = note_door(D2)
time.sleep(1)


def tab(b, url, errors):
    ctx = b.new_context(viewport={"width": 1280, "height": 900})
    ctx.add_init_script("try { localStorage.setItem('pyrate.crew', '1') } catch (e) {}")
    ctx.on("weberror", lambda e: errors.append(str(e.error)))
    pg = ctx.new_page()
    pg.goto(url)
    return ctx, pg


def listen(pg, label):
    wait(pg, "(document.querySelector('#name')?.textContent || '').includes('Gangway')", 40)
    wait(pg, "window.pyrate.doors()?.through", 20)
    t = time.time()
    pg.click("#go")
    heard = wait(pg, "window.pyrate.state().heard", 30)
    f = time.time() - t if heard else None
    print(f"  {label}: first sound {f and round(f, 2)} s, door {pg.evaluate('window.pyrate.doors()')}", flush=True)
    return f


with sync_playwright() as p:
    b = p.chromium.launch(args=["--autoplay-policy=no-user-gesture-required"])
    errors = []
    # 1. Four tabs through the built-in door.
    tabs, firsts = [], []
    for i in range(4):
        ctx, pg = tab(b, f"http://127.0.0.1:8093/index.html", errors)
        firsts.append(listen(pg, f"tab {i + 1}"))
        tabs.append((ctx, pg))
        time.sleep(2)
    print("  the four are in; carrying for 90 s", flush=True)
    time.sleep(90)
    stats = [pg.evaluate("window.pyrate.doors()") or {} for _, pg in tabs]
    crew = [pg.evaluate("window.pyrate.crew()") or {} for _, pg in tabs]
    playing = sum(1 for _, pg in tabs if pg.evaluate("window.pyrate.state().playing && window.pyrate.state().ahead_ms > 0"))
    mid = sorted(f for f in firsts if f is not None)
    check(all(f is not None for f in firsts), f"every tab heard its first sound with no relay: first Listen {[f and round(f, 2) for f in firsts]} (median {mid and round(mid[len(mid) // 2], 2)} s)")
    check(all(s.get("through") == d1_addr for s in stats), f"every tab came in through the built-in door {d1_addr}")
    check(all(s.get("first_hop") and s.get("door_only") for s in stats), "every tab's guard is a door, and no relay ever answered")
    check(all(s.get("relay_ok_ago", 0) < 0 for s in stats), "not one answer came over a relay")
    check(playing == 4, f"all four still playing after 90 s ({playing})")
    served = sum(s.get("served", 0) for s in stats)
    passed = sum(c.get("passed_today", 0) for c in crew)
    decks = sum(len(c.get("decks", [])) for c in crew)
    check(all(s.get("behind", 0) >= 1 for s in stats), f"the tabs know each other as crew behind the door ({[s.get('behind') for s in stats]})")
    # Decks and ways in stay relay-only: a tab reached only through a door opens no deck, so with
    # no relay at all the tabs carry nothing (the forced trade-off, reported, not worked around).
    check(decks == 0, f"no tab opened a deck through a door ({decks}); carried with no relay: {served} forwards, {passed // 1024} KB")
    shared = [s.get("routes_shared", 0) for s in stats]
    routes = [s.get("routes", 0) for s in stats]
    print(f"  routes drawn {routes}; with no exit apart from the guard {shared}; capped {[s.get('capped', 0) for s in stats]}", flush=True)
    # 3. A shared link's doors.
    url = tabs[0][1].evaluate("window.pyrate.shareUrl()") or ""
    frag = url.split("~", 1)[1] if "~" in url else ""
    codes = [c for c in frag.split(".") if c]
    check(1 <= len(codes) <= 2 and len(url) < 260, f"a shared link carries {len(codes)} door(s) and stays short ({len(url)} characters): {url}")
    for c, _ in tabs:
        c.close()
    # 2. A tab from a link alone: a copy with no doors built in, every site down but the doors.
    link = f"http://127.0.0.1:8094/index.html#{GANG}~{code(d2_addr, d2_fp)}.{code(d1_addr, d1_fp)}"
    ctx, pg = tab(b, link, errors)
    f = listen(pg, "link tab")
    s = pg.evaluate("window.pyrate.doors()") or {}
    check(f is not None, f"a tab opened from a link's doors alone plays (first sound {f and round(f, 2)} s)")
    check("~" not in pg.evaluate("location.hash"), "the link's doors left the address bar")
    check(s.get("first_hop") and s.get("first_hop") == d1_id and s.get("through") == d1_addr,
          f"its guard is D1, which the link and D2's cards both name, not D2, which only the link named (through {s.get('through')}, {s.get('trusted')} trusted of {s.get('known')})")
    ctx.close()
    b.close()

check(not errors, f"no page errors {errors[:2]}")
sys.exit(0 if ok else 1)
