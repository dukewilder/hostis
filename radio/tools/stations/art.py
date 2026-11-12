#!/usr/bin/env python3
# Cover and banner for every HOSTIS station, drawn in the site's own faces and colours and
# rendered by a headless Chrome: station.jpg (1280 square) and banner.jpg (1500 by 500) in
# each station's folder, where a Lantern seed takes them.
#   art.py <stations folder> [station ...]      (every station, or only those named)
# No rights reserved.
import os, random, sys
from playwright.sync_api import sync_playwright

R = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
sys.path.insert(0, f"{R}/brand")
import fonts

ROOT = sys.argv[1]
FACES = fonts.faces()
random.seed(1724)

BASE = """
:root{--black:#030303;--ink:#ede4ce;--dim:rgba(237,228,206,.5);--faint:rgba(237,228,206,.3);--ghost:rgba(237,228,206,.13);--red:#c52828}
*{margin:0;padding:0;box-sizing:border-box}
html,body{width:%(w)dpx;height:%(h)dpx;background:var(--black);overflow:hidden}
body{position:relative;color:var(--ink);font-family:'IM Fell English',serif}
.grain{position:absolute;inset:0;opacity:.18;mix-blend-mode:screen}
.frame{position:absolute;inset:%(m)dpx;border:1px solid rgba(237,228,206,.22)}
.frame::after{content:'';position:absolute;inset:10px;border:1px solid rgba(237,228,206,.09)}
.dia{position:absolute;width:12px;height:12px;background:var(--red);transform:rotate(45deg)}
.skull{font-family:'Noto Sans Symbols 2';color:var(--ink);line-height:1}
.title{font-family:'Cinzel',serif;font-weight:600;letter-spacing:.16em;text-transform:uppercase;line-height:1.05}
.cap{font-family:'JetBrains Mono',monospace;letter-spacing:.32em;text-transform:uppercase;color:var(--dim)}
"""

GRAIN = """<svg class="grain" width="100%" height="100%"><filter id="n"><feTurbulence type="fractalNoise" baseFrequency=".9" numOctaves="2" stitchTiles="stitch"/>
<feColorMatrix values="0 0 0 0 .93 0 0 0 0 .89 0 0 0 0 .81 0 0 0 .55 0"/></filter><rect width="100%" height="100%" filter="url(#n)"/></svg>"""


def corners(m):
    o = m - 6
    return "".join(f'<div class="dia" style="{y}:{o}px;{x}:{o}px"></div>' for y in ("top", "bottom") for x in ("left", "right"))


def numbers_motif(size, nrows=7, ncols=4):
    rows = []
    for r in range(nrows):
        groups = []
        for c in range(ncols):
            g = "".join(random.choice("0123456789") for _ in range(5))
            red = (r, c) == (nrows // 2, ncols // 2)
            groups.append(f'<span style="color:{"var(--red)" if red else "inherit"}">{g}</span>')
        op = 0.9 - abs(r - nrows // 2) * 0.22
        rows.append(f'<div style="opacity:{max(op, .08):.2f}">{"&ensp;".join(groups)}</div>')
    return f'<div style="font-family:JetBrains Mono;font-size:{size}px;letter-spacing:.12em;line-height:1.55;text-align:center;color:var(--ink)">{"".join(rows)}</div>'


def dial(width):
    ticks = "".join(f'<line x1="{x}" y1="{18 if i % 5 else 6}" x2="{x}" y2="30" stroke="rgba(237,228,206,{.6 if i % 5 == 0 else .25})" stroke-width="1"/>'
                    for i, x in enumerate(range(0, width + 1, width // 50)))
    return f'<svg width="{width}" height="40" style="display:block;margin:0 auto">{ticks}<line x1="{int(width * .62)}" y1="0" x2="{int(width * .62)}" y2="40" stroke="#c52828" stroke-width="3"/></svg>'


def rings(size):
    c = size / 2
    out = "".join(f'<circle cx="{c}" cy="{c}" r="{r}" fill="none" stroke="rgba(237,228,206,{0.14 + 0.6 * (1 - r / c) ** 1.2:.3f})" stroke-width="{1.2 if i % 4 else 2.2}"/>'
                  for i, r in enumerate(range(int(c * 0.12), int(c * 0.98), int(c / 26))))
    arcs = "".join(f'<path d="M {c} {c - r} A {r} {r} 0 0 1 {c + r * 0.87} {c - r * 0.5}" fill="none" stroke="#c52828" stroke-opacity="{0.95 - k * 0.2:.2f}" stroke-width="4"/>' for k, r in enumerate([c * 0.45, c * 0.62, c * 0.79]))
    return f'<svg width="{size}" height="{size}" viewBox="0 0 {size} {size}">{out}{arcs}<circle cx="{c}" cy="{c}" r="{c * 0.05}" fill="#c52828"/></svg>'


def anchor(size, rope=True):
    """A fouled anchor, the sailors' own mark: the cable round the shank in red."""
    r = ('<path d="M100,24 C150,44 58,74 118,96 S66,140 104,160 S136,186 120,204" fill="none" stroke="#c52828" stroke-width="5" stroke-linecap="round"/>' if rope else "")
    return (f'<svg width="{size}" height="{int(size * 1.07)}" viewBox="0 0 200 214" style="display:block;margin:0 auto">'
            '<g fill="none" stroke="#ede4ce" stroke-linecap="round" stroke-linejoin="round">'
            '<circle cx="100" cy="20" r="13" stroke-width="7"/><line x1="100" y1="34" x2="100" y2="200" stroke-width="11"/>'
            '<line x1="54" y1="58" x2="146" y2="58" stroke-width="9"/><path d="M30,140 Q38,206 100,206 Q162,206 170,140" stroke-width="10"/></g>'
            '<g fill="#ede4ce"><circle cx="50" cy="58" r="7"/><circle cx="150" cy="58" r="7"/><polygon points="16,152 30,116 47,148"/><polygon points="184,152 170,116 153,148"/></g>'
            f'{r}</svg>')


STATIONS = {
    "the-numbers": {
        "title": "The Numbers", "cap": "",
        "cover": lambda: f'<div style="position:absolute;top:150px;left:0;right:0;text-align:center"><div class="skull" style="font-size:64px">☠︎</div></div>'
                         f'<div style="position:absolute;top:270px;left:0;right:0">{dial(860)}</div>'
                         f'<div style="position:absolute;top:350px;left:0;right:0">{numbers_motif(44)}</div>'
                         f'<div style="position:absolute;bottom:170px;left:0;right:0;text-align:center"><div class="title" style="font-size:112px">The Numbers</div>'
                         f'<div class="cap" style="font-size:22px;margin-top:26px">Pyrate Radio &middot; a HOSTIS station</div></div>',
        "banner": lambda: f'<div style="position:absolute;left:96px;top:118px;width:520px">{numbers_motif(30, 6, 3)}</div>'
                          f'<div style="position:absolute;right:110px;top:160px;text-align:right"><div class="title" style="font-size:84px">The Numbers</div>'
                          f'<div class="cap" style="font-size:18px;margin-top:22px">Pyrate Radio</div></div>',
    },
    "shanties": {
        "title": "Shanties", "cap": "sea songs and pirate songs",
        "cover": lambda: f'<div style="position:absolute;top:110px;left:0;right:0">{anchor(560)}</div>'
                         f'<div style="position:absolute;bottom:150px;left:0;right:0;text-align:center"><div class="title" style="font-size:132px">Shanties</div>'
                         f'<div class="cap" style="font-size:22px;margin-top:26px">Pyrate Radio &middot; a HOSTIS station</div></div>',
        "banner": lambda: f'<div style="position:absolute;left:150px;top:60px">{anchor(330)}</div>'
                          f'<div style="position:absolute;right:110px;top:150px;text-align:right"><div class="title" style="font-size:104px">Shanties</div>'
                          f'<div class="cap" style="font-size:18px;margin-top:22px">Sea songs and pirate songs</div></div>',
    },
    "netlabel": {
        "title": "Netlabel", "cap": "free music, freely carried",
        "cover": lambda: f'<div style="position:absolute;top:110px;left:50%;transform:translateX(-50%)">{rings(760)}</div>'
                         f'<div style="position:absolute;bottom:150px;left:0;right:0;text-align:center"><div class="title" style="font-size:132px">Netlabel</div>'
                         f'<div class="cap" style="font-size:22px;margin-top:26px">Creative Commons &middot; electronic &middot; ambient</div></div>',
        "banner": lambda: f'<div style="position:absolute;left:70px;top:-130px">{rings(760)}</div>'
                          f'<div style="position:absolute;right:110px;top:150px;text-align:right"><div class="title" style="font-size:104px">Netlabel</div>'
                          f'<div class="cap" style="font-size:18px;margin-top:22px">Free music, freely carried</div></div>',
    },
    "poets-corner": {
        "title": "Poets' Corner", "cap": "poems read aloud",
        "cover": lambda: f'<div style="position:absolute;top:70px;left:0;right:0;text-align:center"><div class="skull" style="font-size:420px;color:var(--red);line-height:.8">❧</div></div>'
                         f'<div style="position:absolute;top:560px;left:150px;right:150px;text-align:center;font-style:italic;font-size:54px;line-height:1.35;color:rgba(237,228,206,.86)">'
                         f'&ldquo;Look on my Works, ye Mighty, and despair!&rdquo;</div>'
                         f'<div style="position:absolute;top:770px;left:0;right:0;text-align:center" class="cap"><span style="font-size:20px">Shelley &middot; 1818</span></div>'
                         f'<div style="position:absolute;bottom:150px;left:0;right:0;text-align:center"><div class="title" style="font-size:102px">Poets&rsquo; Corner</div>'
                         f'<div class="cap" style="font-size:22px;margin-top:26px">Public domain poems, read aloud</div></div>',
        "banner": lambda: f'<div style="position:absolute;left:110px;top:56px" class="skull"><span style="font-size:340px;color:var(--red);line-height:1">❧</span></div>'
                          f'<div style="position:absolute;right:110px;top:120px;text-align:right"><div class="title" style="font-size:84px">Poets&rsquo; Corner</div>'
                          f'<div style="font-style:italic;font-size:34px;margin-top:22px;color:rgba(237,228,206,.8)">Look on my Works, ye Mighty, and despair!</div></div>',
    },
    "the-public-domain": {
        "title": "The Public Domain", "cap": "everything nobody owns",
        "cover": lambda: f'<div style="position:absolute;top:120px;left:0;right:0;text-align:center"><div style="position:relative;display:inline-block;font-size:560px;line-height:1;color:var(--ink)">&copy;'
                         f'<div style="position:absolute;left:-30px;right:-30px;top:52%;height:22px;background:var(--red);transform:rotate(-38deg)"></div></div></div>'
                         f'<div style="position:absolute;bottom:150px;left:0;right:0;text-align:center"><div class="title" style="font-size:100px">The Public<br>Domain</div>'
                         f'<div class="cap" style="font-size:22px;margin-top:26px">Pyrate Radio &middot; a HOSTIS station</div></div>',
        "banner": lambda: f'<div style="position:absolute;left:96px;top:80px;font-size:280px;line-height:1">&copy;<div style="position:absolute;left:-20px;right:-20px;top:52%;height:18px;background:var(--red);transform:rotate(-38deg)"></div></div>'
                          f'<div style="position:absolute;right:110px;top:150px;text-align:right"><div class="title" style="font-size:74px">The Public Domain</div>'
                          f'<div class="cap" style="font-size:18px;margin-top:22px">Everything nobody owns</div></div>',
    },
    "readings": {
        "title": "Readings", "cap": "the pyrates, read aloud",
        "cover": lambda: f'<div style="position:absolute;top:190px;left:150px;right:150px;text-align:center;font-family:IM Fell DW Pica SC;color:rgba(237,228,206,.82)">'
                         f'<div style="font-size:38px;letter-spacing:.08em">A</div><div style="font-size:84px;letter-spacing:.05em;line-height:1.1">General History</div>'
                         f'<div style="font-size:30px;margin:10px 0">of the</div><div style="font-size:128px;line-height:1;color:var(--red)">Pyrates</div>'
                         f'<div style="height:1px;background:rgba(237,228,206,.3);margin:34px 120px"></div>'
                         f'<div style="font-family:IM Fell English;font-style:italic;font-size:34px">By Captain Charles Johnson</div>'
                         f'<div style="font-family:JetBrains Mono;font-size:20px;letter-spacing:.3em;margin-top:18px;color:var(--dim)">LONDON &middot; MDCCXXIV</div></div>'
                         f'<div style="position:absolute;bottom:150px;left:0;right:0;text-align:center"><div class="title" style="font-size:124px">Readings</div>'
                         f'<div class="cap" style="font-size:22px;margin-top:26px">Read aloud, round the clock</div></div>',
        "banner": lambda: f'<div style="position:absolute;left:120px;top:110px;font-family:IM Fell DW Pica SC;color:rgba(237,228,206,.82)"><div style="font-size:54px">A General History</div>'
                          f'<div style="font-size:30px;margin:6px 0">of the</div><div style="font-size:96px;line-height:1;color:var(--red)">Pyrates</div></div>'
                          f'<div style="position:absolute;right:110px;top:150px;text-align:right"><div class="title" style="font-size:104px">Readings</div>'
                          f'<div class="cap" style="font-size:18px;margin-top:22px">The pyrates, read aloud</div></div>',
    },
}


def page(w, h, m, body):
    return f"<!DOCTYPE html><html><head><meta charset='utf-8'><style>{FACES}{BASE % {'w': w, 'h': h, 'm': m}}</style></head><body>{GRAIN}<div class='frame'></div>{corners(m)}{body}</body></html>"


def main():
    with sync_playwright() as p:
        b = p.chromium.launch(executable_path="/usr/bin/google-chrome", args=["--no-sandbox"])
        only = sys.argv[2:]
        for slug, s in STATIONS.items():
            if only and slug not in only:
                continue
            out = f"{ROOT}/{slug}"
            os.makedirs(out, exist_ok=True)
            for kind, (w, h, m) in {"cover": (1280, 1280, 56), "banner": (1500, 500, 34)}.items():
                pg = b.new_page(viewport={"width": w, "height": h})
                pg.set_content(page(w, h, m, s[kind]()))
                pg.wait_for_timeout(400)
                pg.screenshot(path=f"{out}/{'station' if kind == 'cover' else 'banner'}.jpg", type="jpeg", quality=88)
                pg.close()
            print(slug)
        b.close()


if __name__ == "__main__":
    main()
