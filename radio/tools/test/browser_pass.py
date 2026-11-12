# A passphrase on top of the words: a tab raises a station with one, a fresh tab takes it back
# from the words and the passphrase, and the words alone name a different station.
# Run after tools/test/serve_test.sh, with a Lantern that carries every station.
import sys, time, json
from playwright.sync_api import sync_playwright
ok = True
def check(c, what):
    global ok; ok &= bool(c); print(("PASS " if c else "FAIL ") + what, flush=True)
with sync_playwright() as p:
    b = p.chromium.launch(); errs = []
    a = b.new_context(bypass_csp=True).new_page(); a.on("pageerror", lambda e: errs.append(str(e)))
    a.goto("http://127.0.0.1:8099/"); a.wait_for_function("window.pyrate && document.getElementById('signal').textContent.includes('Connected')", timeout=90000)
    a.click(".tab[data-v=studio]"); a.fill("#st-name", "Behind Words"); a.click("text=More options"); a.fill("#st-ppass", "lanterns at low tide")
    a.click("#st-raise"); a.wait_for_selector("#st-show:not([hidden])", timeout=60000)
    words = " ".join(a.eval_on_selector_all("#st-wordlist li", "l => l.map(x => x.textContent)"))
    check(a.is_visible("#st-passnote"), "the words come with a reminder of the passphrase")
    check(a.input_value("#st-ppass") == "", "the passphrase leaves the page")
    a.check("#st-written"); a.click("#st-done")
    sid = a.evaluate("window.pyrate.mine()[0]")
    a.wait_for_function("document.getElementById('st-status').textContent.startsWith('On the')", timeout=240000)
    c = b.new_context(bypass_csp=True).new_page(); c.on("pageerror", lambda e: errs.append(str(e)))
    c.goto("http://127.0.0.1:8099/"); c.wait_for_function("window.pyrate && document.getElementById('signal').textContent.includes('Connected')", timeout=90000)
    c.click(".tab[data-v=studio]"); c.click("text=Bring back a station with its 24 words"); c.fill("#st-words", words); c.fill("#st-rpass", "lanterns at low tide"); c.click("#st-restore")
    c.wait_for_function("window.pyrate.mine().length > 0 || document.getElementById('st-note').textContent.length > 40", timeout=120000)
    got = c.evaluate("window.pyrate.mine()")
    check(sid in got, "the words and the passphrase take the station back: %s" % (c.text_content("#st-note") or "taken"))
    d = b.new_context(bypass_csp=True).new_page()
    d.goto("http://127.0.0.1:8099/"); d.wait_for_function("window.pyrate && document.getElementById('signal').textContent.includes('Connected')", timeout=90000)
    other = d.evaluate("(w) => window.pyrate && 1", words)
    d.click(".tab[data-v=studio]"); d.click("text=Bring back a station with its 24 words"); d.fill("#st-words", words); d.click("#st-restore")
    d.wait_for_function("document.getElementById('st-note').textContent.length > 0 && !document.getElementById('st-note').textContent.startsWith('Looking')", timeout=120000)
    held = d.evaluate("window.pyrate.mine()")
    check(sid not in held, "the words alone do not open it: " + (d.text_content("#st-note") or ""))
    check(not errs, "no page errors: %s" % errs[:3])
    b.close()
sys.exit(0 if ok else 1)
