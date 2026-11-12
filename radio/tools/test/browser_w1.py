# What a tab leaves behind: the tuned station stays out of the address bar and history (the
# tab alone remembers it, for a reload); a raised station's deputy can be sealed under a
# passphrase or kept in the tab alone; a recorder's date never becomes a title; and the page
# opens no WebRTC and asks nothing of any host but the test network's.
# Run after tools/test/serve_test.sh. Usage: browser_w1.py <station id> <wav file>
import sys, time, shutil, os
from urllib.parse import urlparse
from playwright.sync_api import sync_playwright
st, wav = sys.argv[1], sys.argv[2]; ok = True
def check(c, what):
    global ok; ok &= bool(c); print(("PASS " if c else "FAIL ") + what, flush=True)
stamp = "/tmp/w1/Recording 2026-11-10 21-03-12.wav"; os.makedirs("/tmp/w1", exist_ok=True); shutil.copy(wav, stamp)
hosts = set()
with sync_playwright() as p:
    b = p.chromium.launch(args=["--autoplay-policy=no-user-gesture-required"])
    ctx = b.new_context(bypass_csp=True); errs = []
    ctx.add_init_script("window.__rtc = 0; for (const k of ['RTCPeerConnection', 'webkitRTCPeerConnection']) if (window[k]) { const R = window[k]; window[k] = function (...a) { window.__rtc++; return new R(...a) } }")
    ctx.on("request", lambda r: hosts.add(urlparse(r.url).hostname))
    a = ctx.new_page(); a.on("pageerror", lambda e: errs.append(str(e))); a.on("dialog", lambda d: d.accept())
    a.goto("http://127.0.0.1:8099/#" + st); a.wait_for_selector("#go:not([hidden])", timeout=90000); time.sleep(4)
    check(a.evaluate("location.hash") == "", "the station leaves the address bar once tuned: " + a.evaluate("location.href"))
    check(a.evaluate("sessionStorage.getItem('pyrate.cur')") == st, "the tab alone remembers it")
    a.reload(); a.wait_for_selector("#go:not([hidden])", timeout=90000)
    try: a.wait_for_function("document.getElementById('name').textContent.includes('Gangway')", timeout=90000)
    except Exception: pass
    check("Gangway" in (a.text_content("#name") or ""), "a reload comes back to it: " + (a.text_content("#name") or ""))
    a.wait_for_function("window.pyrate && document.getElementById('signal').textContent.includes('Connected')", timeout=90000)
    a.click(".tab[data-v=studio]"); a.fill("#st-name", "Sealside"); a.click("#st-raise"); a.wait_for_selector("#st-show:not([hidden])")
    a.check("#st-written"); a.click("#st-done")
    a.wait_for_function("(localStorage.getItem('pyrate.mine') || '').length > 0", timeout=10000)
    before = a.evaluate("localStorage.getItem('pyrate.mine')")
    check('"deputy"' not in before, "the stored key is sealed under the browser's own key from the start")
    a.wait_for_function("document.getElementById('st-status').textContent.startsWith('On the')", timeout=240000)
    a.click("#st-safer summary"); a.fill("#st-newpass", "lights on the harbour wall"); a.click("#st-lockit")
    a.wait_for_function("document.getElementById('st-locknote').textContent.startsWith('Locked')", timeout=30000)
    s = a.evaluate("localStorage.getItem('pyrate.mine')")
    check(s and s != before and '"deputy"' not in s, "locked: the stored key changes and stays ciphertext")
    a.reload(); a.wait_for_selector("#st-lock:not([hidden])", timeout=90000)
    check(a.is_hidden("#st-new"), "a sealed browser asks before it broadcasts, and hides the raise form")
    a.fill("#st-pass", "wrong words"); a.click("#st-open")
    a.wait_for_function("document.getElementById('st-locknote2').textContent.includes('not the passphrase')", timeout=30000); check(True, "a wrong passphrase opens nothing")
    a.fill("#st-pass", "lights on the harbour wall"); a.click("#st-open")
    a.wait_for_selector("#st-mine:not([hidden])", timeout=90000); check(a.is_hidden("#st-lock"), "the right one opens the station")
    a.click("#st-safer summary"); a.check("#st-tabonly")
    check(a.evaluate("localStorage.getItem('pyrate.mine')") is None and len(a.evaluate("sessionStorage.getItem('pyrate.mine')") or "") > 0, "tab-only: the key leaves the browser's storage and lives in the tab")
    a.set_input_files("#st-files", stamp); time.sleep(1)
    check(a.text_content(".qi .qt") == "Untitled", "a recorder's date never becomes a title: " + (a.text_content(".qi .qt") or ""))
    time.sleep(5)
    check(a.evaluate("window.__rtc") == 0, "no WebRTC")
    # The test harbour names its relay, so no one's lookup service is asked anything.
    # blob: and data: addresses (pictures made in the tab) have no host and go nowhere.
    check({h for h in hosts if h} <= {"127.0.0.1"}, "no host but the test network's: %s" % sorted(h for h in hosts if h))
    check(not errs, "no page errors: %s" % errs[:3])
    b.close()
sys.exit(0 if ok else 1)
