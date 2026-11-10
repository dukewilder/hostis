# The two copies: hostis.black's plays its HOSTIS station and hands every other link on to the
# open player, hash intact, without showing it; the open player keeps what it is given.
# Run after tools/test/serve_test.sh. Usage: browser_scope.py <HOSTIS station id>
import sys, time
from playwright.sync_api import sync_playwright
own = sys.argv[1]; other = "ab" * 32
ok = True
def check(c, what):
    global ok; ok &= bool(c); print(("PASS " if c else "FAIL ") + what)
with sync_playwright() as p:
    b = p.chromium.launch(args=["--autoplay-policy=no-user-gesture-required"])
    pg = b.new_page(); errs = []; pg.on("pageerror", lambda e: errs.append(str(e)))
    pg.goto("http://127.0.0.1:8097/#" + other); time.sleep(6)
    u = pg.evaluate("location.href"); check(u.startswith("http://127.0.0.1:8099/") and u.endswith("#" + other), "a stranger's station goes straight to the open player: " + u)
    pg.goto("http://127.0.0.1:8097/#pocket=xyz"); time.sleep(6)
    u = pg.evaluate("location.href"); check(u.startswith("http://127.0.0.1:8099/"), "an invitation goes to the open player, which takes its key out of the address bar")
    pg.goto("http://127.0.0.1:8097/"); pg.wait_for_selector("#go:not([hidden])", timeout=90000); time.sleep(4)
    names = pg.eval_on_selector_all("#names button", "bs => bs.map(b => b.dataset.id)")
    check(names and all(n == own for n in names), "hostis.black's dial holds only HOSTIS's: %d" % len(names))
    check(pg.is_hidden("#chatgo") and pg.is_visible("#chatopen") is not None, "no chatter on hostis.black")
    check("http://127.0.0.1:8099/" in (pg.get_attribute(".everyone a.lead-btn", "href") or ""), "every station leads to the open player")
    pg.goto("http://127.0.0.1:8099/#" + own); pg.wait_for_selector("#go:not([hidden])", timeout=90000); time.sleep(3)
    check(pg.url.startswith("http://127.0.0.1:8099/"), "the open player keeps what it is given")
    check(not errs, "no page errors: %s" % errs[:3])
    b.close()
sys.exit(0 if ok else 1)
