# Raising colours from the open player: a tab raises a station, writes down nothing (the test
# reads the words from the page), puts a recording on it and sends it along the stem; a carry-all
# Lantern takes it, and a second, fresh tab finds the station on the Log and plays it after the
# first tab has closed. Needs tools/test/serve_test.sh and a Lantern that carries every station.
# Usage: browser_raise.py <wav file>
import sys, time, json
from playwright.sync_api import sync_playwright
wav = sys.argv[1]; ok = True
def check(c, what):
    global ok; ok &= bool(c); print(("PASS " if c else "FAIL ") + what, flush=True)
with sync_playwright() as p:
    b = p.chromium.launch(args=["--autoplay-policy=no-user-gesture-required"])
    ctx = b.new_context(bypass_csp=True); a = ctx.new_page(); errs = []; a.on("pageerror", lambda e: errs.append(str(e)))
    a.on("dialog", lambda d: d.accept())
    a.goto("http://127.0.0.1:8099/"); a.wait_for_function("window.pyrate && document.getElementById('signal').textContent.includes('neighbour')", timeout=90000)
    a.fill("#st-name", "Tabside"); a.fill("#st-about", "Raised in a browser tab"); a.fill("#st-kinds", "test, tones")
    a.click("#st-raise"); a.wait_for_selector("#st-show:not([hidden])")
    words = a.eval_on_selector_all("#st-wordlist li", "l => l.map(x => x.textContent)")
    check(len(words) == 24, "twenty-four words shown once: %d" % len(words))
    check(a.is_disabled("#st-done"), "the station waits until the words are written down")
    a.check("#st-written"); a.click("#st-done")
    check(a.eval_on_selector_all("#st-wordlist li", "l => l.length") == 0, "the words leave the page")
    kept = a.evaluate("localStorage.getItem('pyrate.mine')")
    check(kept and all(w not in kept for w in words[:6]), "the browser keeps the deputy, never the words")
    a.wait_for_function("(localStorage.getItem('pyrate.mine') || '').includes('\"d\":1')", timeout=10000)
    stored = a.evaluate("localStorage.getItem('pyrate.mine')")
    check('"deputy"' not in stored and '"d":1' in stored, "and keeps it sealed, never in the clear")
    a.wait_for_function("document.getElementById('st-status').textContent.length > 0 && !document.getElementById('st-status').textContent.includes('proof')", timeout=120000)
    check(True, "proof of work done: " + a.text_content("#st-status"))
    a.set_input_files("#st-files", wav)
    a.wait_for_selector(".qi.done, .qi.bad", timeout=120000)
    check(a.is_visible(".qi.done"), "the recording is on the clock: " + a.text_content(".qi .qs"))
    a.wait_for_function("document.getElementById('st-status').textContent.startsWith('On the air')", timeout=180000)
    check(True, "it sailed: " + a.text_content("#st-status"))
    mine = json.loads(a.evaluate("pyrate && JSON.stringify(window.pyrate) && 0") or "0") if False else None
    sid = a.evaluate("window.pyrate.mine()[0]")
    a.close()
    c = ctx.new_page(); c.on("pageerror", lambda e: errs.append(str(e)))
    c.goto("http://127.0.0.1:8099/#" + sid)
    c.wait_for_selector("#go:not([hidden])", timeout=90000); time.sleep(3)
    c.click("#go")
    try:
        c.wait_for_function("document.getElementById('np').textContent.includes('Harbour Lights')", timeout=120000); heard = True
    except Exception: heard = False
    check(heard, "a fresh tab finds it and plays it after the first has closed: " + (c.text_content("#name") or "") + " / " + (c.text_content("#np") or ""))
    check(not errs, "no page errors: %s" % errs[:3])
    b.close()
sys.exit(0 if ok else 1)
