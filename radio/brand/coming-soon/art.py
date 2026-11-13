# Shared pieces for the Pyrate Radio "coming soon" graphics: the brand symbols, the faces,
# the print filters and the FM dial. Every design in this folder is drawn from these.
import os, re, sys
HERE = os.path.dirname(os.path.abspath(__file__))
BRAND = os.path.dirname(HERE)
sys.path.insert(0, BRAND)
import fonts as _fonts

BONE, BLACK, RED, BLOOD = "#ede4ce", "#030303", "#c52828", "#7d1818"
LO, HI = 87.5, 108.1

def write_fonts():
    """The HOSTIS faces as files in fonts/, and the @font-face rules that point at them."""
    css, files, _ = _fonts.linked("fonts/")
    os.makedirs(f"{HERE}/fonts", exist_ok=True)
    for name, data in files.items():
        p = f"{HERE}/fonts/{name}"
        if not os.path.exists(p):
            open(p, "wb").write(data)
    return css

def symbol(fname, sid):
    s = open(f"{BRAND}/{fname}", encoding="utf-8").read()
    vb = re.search(r'viewBox="([^"]+)"', s).group(1)
    g = s[s.index("<g"):s.rindex("</g>") + 4].replace('fill="#ede4ce"', 'fill="currentColor"')
    return f'<symbol id="{sid}" viewBox="{vb}">{g}</symbol>'

MARK_VB, WORD_VB = 1144.0, (6900.0, 2100.0)
# Where the ink sits inside each symbol, as fractions of its box (measured from the PNGs).
MARK_INK = dict(x0=40/1024, x1=990/1024, y0=113/1024, y1=916/1024, skull_cx=548/1024,
                skull_y0=113/1024, skull_y1=668/1024, eyes=440/1024, nose=492/1024)
WORD_INK = dict(x0=59/2000, x1=1950/2000, y0=82/609, y1=517/609)

def defs(extra=""):
    return f"""<defs>
{symbol('pyrate-mark.svg', 'mark')}
{symbol('pyrate-wordmark.svg', 'word')}
<filter id="ink" filterUnits="userSpaceOnUse" x="0" y="0" width="4000" height="4000">
  <feTurbulence type="fractalNoise" baseFrequency=".75" numOctaves="2" seed="7" result="n"/>
  <feDisplacementMap in="SourceGraphic" in2="n" scale="2.4" xChannelSelector="R" yChannelSelector="G" result="d"/>
  <feTurbulence type="fractalNoise" baseFrequency="1.6" numOctaves="1" seed="11" result="w"/>
  <feColorMatrix in="w" type="matrix" values="0 0 0 0 0  0 0 0 0 0  0 0 0 0 0  0 0 0 -9 5.6" result="wear"/>
  <feComposite in="d" in2="wear" operator="in"/>
</filter>
<filter id="inklite" filterUnits="userSpaceOnUse" x="0" y="0" width="4000" height="4000">
  <feTurbulence type="fractalNoise" baseFrequency=".6" numOctaves="2" seed="3" result="n"/>
  <feDisplacementMap in="SourceGraphic" in2="n" scale="1.6" xChannelSelector="R" yChannelSelector="G" result="d"/>
  <feTurbulence type="fractalNoise" baseFrequency="1.4" numOctaves="1" seed="5" result="w"/>
  <feColorMatrix in="w" type="matrix" values="0 0 0 0 0  0 0 0 0 0  0 0 0 0 0  0 0 0 -9 7.4" result="wear"/>
  <feComposite in="d" in2="wear" operator="in"/>
</filter>
<filter id="grain" x="0" y="0" width="100%" height="100%">
  <feTurbulence type="fractalNoise" baseFrequency=".85" numOctaves="2" stitchTiles="stitch"/>
  <feColorMatrix type="saturate" values="0"/>
</filter>
{extra}
</defs>"""

def _head(css, w, h, bg=BLACK):
    return f"""<!DOCTYPE html><html lang="en-GB"><head><meta charset="utf-8">
<style>{css}
html,body{{margin:0;background:{bg}}}
svg{{display:block}}
.sc{{font-family:'IM Fell DW Pica SC',Georgia,serif}}
.fell{{font-family:'IM Fell English',Georgia,serif}}
.mono{{font-family:'JetBrains Mono',monospace}}
.carved{{font-family:'Cinzel',Georgia,serif}}
</style></head><body>
<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="{w}" height="{h}" viewBox="0 0 {w} {h}">"""

FOOT = "</svg>" + "{CENTRE}" + "</body></html>"

def grain(w, h, op=.07):
    return f'<rect width="{w}" height="{h}" filter="url(#grain)" opacity="{op}" style="mix-blend-mode:screen"/>'

def fx(f, x0, x1):
    return x0 + (f - LO) / (HI - LO) * (x1 - x0)

def dial(x0, x1, y, col=BONE, down=True, tick=(10, 18, 30), label=15, every=2, op=1.0, stroke=1.4, numy=None, lab_op=.62, skip=None):
    """A straight FM scale: a rule, a tick at every 0.1 MHz, longer at the half and whole,
    the even whole numbers set in mono. Ticks hang below the rule when down is True."""
    s = 1 if down else -1
    out = [f'<g opacity="{op}"><line x1="{x0}" y1="{y}" x2="{x1}" y2="{y}" stroke="{col}" stroke-width="{stroke*1.6}"/>']
    for i in range(round((HI - LO) * 10) + 1):
        f = round(LO + i / 10, 1)
        x = fx(f, x0, x1)
        if abs(f - round(f)) < 1e-6:
            L, sw = tick[2], stroke * 1.4
        elif abs(f * 2 - round(f * 2)) < 1e-6:
            L, sw = tick[1], stroke
        else:
            L, sw = tick[0], stroke * .7
        out.append(f'<line x1="{x:.2f}" y1="{y}" x2="{x:.2f}" y2="{y + s*L}" stroke="{col}" stroke-width="{sw}"/>')
        if label and abs(f - round(f)) < 1e-6 and int(round(f)) % every == 0 and not (skip and skip[0] < x < skip[1]):
            ny = numy if numy is not None else y + s * (tick[2] + label * 1.25) + (label * .35 if down else 0)
            out.append(f'<text x="{x:.2f}" y="{ny}" class="mono" font-size="{label}" fill="{col}" fill-opacity="{lab_op}" text-anchor="middle" letter-spacing=".04em">{int(round(f))}</text>')
    out.append("</g>")
    return "\n".join(out)

def rite(x, y, w, h, gap=10, col=BONE, op=.32, d=11, red=RED):
    """The box hostis.black signs its mark in: a ruled double edge and a red point at each corner."""
    o = [f'<rect x="{x}" y="{y}" width="{w}" height="{h}" fill="none" stroke="{col}" stroke-opacity="{op}" stroke-width="1.5"/>',
         f'<rect x="{x+gap}" y="{y+gap}" width="{w-2*gap}" height="{h-2*gap}" fill="none" stroke="{col}" stroke-opacity="{op}" stroke-width="1.5"/>']
    for cx, cy in ((x, y), (x + w, y), (x, y + h), (x + w, y + h)):
        o.append(f'<rect x="{cx-d/2}" y="{cy-d/2}" width="{d}" height="{d}" fill="{red}" transform="rotate(45 {cx} {cy})"/>')
    return "\n".join(o)

def diamond(cx, cy, d=9, col=RED):
    return f'<rect x="{cx-d/2}" y="{cy-d/2}" width="{d}" height="{d}" fill="{col}" transform="rotate(45 {cx} {cy})"/>'

def word(cx, top, width, col=BONE, extra=""):
    """The wordmark, placed by the width of its ink and the top of its ink."""
    ink_w = WORD_INK["x1"] - WORD_INK["x0"]
    W = width / ink_w
    H = W * WORD_VB[1] / WORD_VB[0]
    x = cx - W * (WORD_INK["x0"] + ink_w / 2)
    y = top - H * WORD_INK["y0"]
    return f'<use href="#word" x="{x:.1f}" y="{y:.1f}" width="{W:.1f}" height="{H:.1f}" color="{col}" {extra}/>', y + H * WORD_INK["y1"]

def mark_at(skull_cx, skull_top, size, col=BONE, extra=""):
    """The mark at size (its full box), placed by the skull's centre line and the top of the skull."""
    x = skull_cx - size * MARK_INK["skull_cx"]
    y = skull_top - size * MARK_INK["skull_y0"]
    return f'<use href="#mark" x="{x:.1f}" y="{y:.1f}" width="{size:.1f}" height="{size:.1f}" color="{col}" {extra}/>'

def skull_symbol(sid="skull"):
    """The skull of the mark alone, without its microphones (paths sorted by their bounds)."""
    from paths import parts
    s = open(f"{BRAND}/pyrate-mark.svg", encoding="utf-8").read()
    vb = re.search(r'viewBox="([^"]+)"', s).group(1)
    g = re.search(r"<g [^>]*>", s).group(0).replace('fill="#ede4ce"', 'fill="currentColor"')
    keep = []
    for d, (x0, x1, y0, y1) in parts(s):
        mic = y0 >= 760 or x1 <= 400 or (x0 >= 745 and y0 > 500)
        if not mic:
            keep.append(f'<path d="{d}"/>')
    return f'<symbol id="{sid}" viewBox="{vb}">{g}{"".join(keep)}</g></symbol>'

SKULL = dict(cx=613.5/1144, top=126/1144, bottom=743/1144, x0=372/1144, x1=855/1144)

# The HOSTIS skull: the Noto Sans Symbols 2 glyph, as hostis.black and the radio draw it (font units,
# y up, baseline at 0). Bounds of the ink: x 51 to 868, y -61 to 736; the nose ends at y 236.
HSKULL_D = open(f"{HERE}/hostis-skull.d").read().strip()
HSK = dict(x0=51, x1=868, cx=459.5, top=736, bottom=-61, nose=228)

def hskull(cx, base_y, s, col=BONE, extra=""):
    """The HOSTIS skull with its glyph baseline at base_y, centred on cx, s pixels per font unit."""
    return (f'<g transform="translate({cx - HSK["cx"] * s:.2f} {base_y:.2f}) scale({s:.5f} {-s:.5f})" fill="{col}" {extra}>'
            f'<path d="{HSKULL_D}"/></g>')

def lockup(cx, base, fs, title_base, title_fs, col=BONE):
    """The radio's lockup: the HOSTIS bar (skull and HOSTIS, as the flagbar sets them) and
    "Pyrate Radio" set below it in the masthead's face. Centred on cx at render time."""
    sk = fs * 30 / 14 / 1000            # the flagbar: a 30px skull beside 14px type, 12px apart
    g = (f'<g class="centre" data-cx="{cx}">{hskull(919 * sk / 2, base, sk, col)}'
         f'<text x="{919 * sk + fs * 12 / 14:.1f}" y="{base}" class="sc" font-size="{fs}" fill="{col}" letter-spacing=".3em">HOSTIS</text></g>')
    t = (f'<text x="{cx}" y="{title_base}" class="sc" font-size="{title_fs}" fill="{col}" text-anchor="middle" '
         f'letter-spacing=".09em" dx="{title_fs * .045:.1f}">Pyrate Radio</text>')
    return g + t

CENTRE_JS = """<script>for(const g of document.querySelectorAll('.centre')){const b=g.getBBox(),
ls=parseFloat(getComputedStyle(g.querySelector('text')).letterSpacing)||0;
g.setAttribute('transform','translate('+(+g.dataset.cx-(b.x+(b.width-ls)/2))+' 0)')}</script>"""

FOOT = "</svg>" + CENTRE_JS + "</body></html>"
head = _head
