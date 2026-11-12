"""The deployed player on the real network: Listen, switch and stop, timed, through the session
proxy. real_browser2.py <url>"""
import json, os, sys, time
from playwright.sync_api import sync_playwright
url = sys.argv[1]
proxy = os.environ.get("HTTPS_PROXY")
def run():
  with sync_playwright() as p:
    # A page served from this machine is reached directly; everything else goes through the proxy.
    b = p.chromium.launch(proxy={"server": proxy, "bypass": "127.0.0.1,localhost"}, args=["--autoplay-policy=no-user-gesture-required"])
    pg = b.new_context(ignore_https_errors=True).new_page()
    t0 = time.time(); T = lambda: time.time() - t0
    pg.goto(url, wait_until="domcontentloaded")
    S = lambda: pg.evaluate("window.pyrate ? window.pyrate.state() : {}")
    V = lambda: pg.evaluate("[document.getElementById('name')?.textContent, document.getElementById('np')?.textContent]")
    def until(cond, limit, label):
        t = time.time(); last = None
        while time.time() - t < limit:
            st, v = S(), V()
            if json.dumps(v) != last: print(f"  {T():6.1f} {label}: shows {v}"); last = json.dumps(v)
            if cond(st): return time.time() - t
            time.sleep(0.2)
        return None
    r = {}
    r['stations'] = until(lambda s: s.get('stations', 0) > 0 and not pg.evaluate("document.getElementById('go').hidden"), 90, 'open')
    # 0.9.14 and later say when the first sound is heard; older players only what is scheduled.
    sound = lambda s: s.get('heard') if 'heard' in s else s.get('scheduled', 0) > 0
    pg.click('#go'); r['listen_to_sound'] = until(sound, 90, 'listen')
    first = S().get('station')
    rows = pg.query_selector_all('#rows .row')
    other = next((x for x in rows if x.get_attribute('data-id') != first), None)
    if other:
        other.click(); time.sleep(0.3)
        r['switch_to_sound'] = until(lambda s: s.get('station') != first and sound(s) and s.get('ahead_ms',0) > 0, 90, 'switch')
    pg.click('#go'); time.sleep(1.5); st = S(); r['after_stop'] = {k: st.get(k) for k in ('playing', 'scheduled', 'ctx')}
    pg.click('#go'); r['relisten_to_sound'] = until(sound, 60, 'relisten')
    r['total'] = T()
    print(json.dumps(r)); b.close()
run()
