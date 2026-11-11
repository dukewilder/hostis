# Live from the open player: a tab raises a station and goes live from a (fake) microphone; a
# second tab tuned to it hears the station live, ten to twenty seconds behind.
import sys, time
from playwright.sync_api import sync_playwright
ok = True
def check(c, what):
    global ok; ok &= bool(c); print(("PASS " if c else "FAIL ") + what, flush=True)
with sync_playwright() as p:
    b = p.chromium.launch(args=["--autoplay-policy=no-user-gesture-required", "--use-fake-ui-for-media-stream", "--use-fake-device-for-media-stream"])
    ctx = b.new_context(bypass_csp=True, permissions=["microphone"]); a = ctx.new_page(); errs = []
    a.on("pageerror", lambda e: errs.append(str(e))); a.on("dialog", lambda d: d.accept())
    a.goto("http://127.0.0.1:8099/"); a.wait_for_function("document.getElementById('signal').textContent.includes('neighbour')", timeout=90000)
    a.fill("#st-name", "Open Mic"); a.click("#st-raise"); a.wait_for_selector("#st-show:not([hidden])")
    a.check("#st-written"); a.click("#st-done")
    a.wait_for_function("!document.getElementById('st-status').textContent.includes('proof') && document.getElementById('st-status').textContent.length > 0", timeout=120000)
    # PYRATE_DISGUISE=1 airs with the voice disguise on, through the same path.
    if __import__("os").environ.get("PYRATE_DISGUISE"): a.check("#st-disguise")
    a.click("#st-live"); time.sleep(2)
    check("ON AIR" in (a.text_content("#st-livetime") or ""), "the tab is on the air")
    sid = a.evaluate("JSON.parse(localStorage.getItem('pyrate.mine'))[0].id")
    c = ctx.new_page(); c.on("pageerror", lambda e: errs.append(str(e)))
    c.goto("http://127.0.0.1:8099/#" + sid); c.wait_for_selector("#go:not([hidden])", timeout=90000); time.sleep(2); c.click("#go")
    try:
        c.wait_for_function("document.getElementById('np').textContent.includes('Live')", timeout=150000); heard = True
    except Exception: heard = False
    check(heard, "a second tab hears it live: " + (c.text_content("#np") or ""))
    check(c.is_visible("#air.on") or "on" in (c.get_attribute("#lamp", "class") or ""), "the lamp is lit")
    a.click("#st-live"); time.sleep(1)
    check(not (a.text_content("#st-livetime") or ""), "ending the broadcast stops the microphone")
    check(not errs, "no page errors: %s" % errs[:3])
    b.close()
sys.exit(0 if ok else 1)
