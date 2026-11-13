# Render an HTML design to an exact-size image with Playwright's Chromium.
# python3 render.py page.html out.png W H [scale]
import sys
from playwright.sync_api import sync_playwright
def render(jobs):
    with sync_playwright() as p:
        b = p.chromium.launch()
        for src, out, w, h, scale in jobs:
            pg = b.new_page(viewport={"width": w, "height": h}, device_scale_factor=scale)
            pg.goto("file://" + src)
            pg.evaluate("document.fonts.ready")
            pg.wait_for_timeout(400)
            kw = {"type": "jpeg", "quality": 72} if out.endswith(".jpg") else {"type": "png"}
            pg.screenshot(path=out, clip={"x": 0, "y": 0, "width": w, "height": h}, **kw)
            pg.close()
        b.close()
if __name__ == "__main__":
    a = sys.argv[1:]
    render([(__import__("os").path.abspath(a[0]), a[1], int(a[2]), int(a[3]), float(a[4]) if len(a) > 4 else 1)])
