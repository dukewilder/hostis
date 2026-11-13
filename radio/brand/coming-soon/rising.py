# PYRATE RADIO, coming soon. The skull rises over the FM band as a sun over the sea: the dial
# is the horizon, and the skull's reflection is cut into the water in red engraved lines.
#   python3 rising.py            writes rising-<W>x<H>.html for each size
#   python3 rising.py --bone     the same with the reflection in bone instead of red
#   python3 rising.py --render [dir]   also renders each to pyrate-coming-soon-<W>x<H>.png in dir
#                                      (default /mnt/user-data/outputs)
#   python3 rising.py --card [--bone]  the 1200x630 share card for the radio's pages, with the launch
#                                      line: red to landing/og-radio.png, bone to the outputs folder
from art import *

LINE = ("Soon on the air.", "Never off it.")

SIZES = {
    (1600, 900): dict(lock=(90, 28, 198, 100), hz=594, dx=118, refl=150,
                      line_y=(785,), line_fs=50, url_y=840, url_fs=19),
    (1080, 1350): dict(lock=(134, 32, 260, 108), hz=842, dx=78, refl=196,
                       line_y=(1112, 1180), line_fs=62, url_y=1264, url_fs=21),
    (1080, 1080): dict(lock=(98, 28, 204, 100), hz=668, dx=78, refl=150,
                       line_y=(894,), line_fs=50, url_y=972, url_fs=18),
}
# The share card (og:image) once the radio is on the air. HOSTIS.BLACK is set into the frame's
# bottom edge, and everything is sized to read at 400 px wide, as a link preview shows it.
CARD = dict(lock=(62, 22, 150, 76), hz=422, dx=70, refl=100, frame=18, gap=8, num=16,
            line=("Nobody can switch it off.",), line_y=(574,), line_fs=48, url_fs=22, url_frame=True, grain=.035)
GAP = 58       # between the lockup's title and the tips of the rising skull's bones
FRAME = 26     # inset of the ruled double edge

def compose(W, H, L, water=RED):
    hz, cx, depth = L["hz"], W / 2, L["refl"]
    hb, hfs, tb, tfs = L["lock"]
    # The HOSTIS skull, risen to just under its nose: the horizon crosses the glyph at y = nose.
    s = (hz - tb - GAP) / (HSK["top"] - HSK["nose"])
    base = hz + HSK["nose"] * s
    sk = hskull(cx, base, s, "{c}")
    # The water: rules that open out and thin as they go down, so the reflection breaks up.
    rules, yy = [], hz + 5.0
    while yy < hz + depth:
        t = (yy - hz) / depth
        rules.append(f'<rect x="0" y="{yy:.1f}" width="{W}" height="{max(.5, 2.8 * (1 - t) ** 1.2):.2f}" fill="#fff"/>')
        yy += 5.6 + 9 * t ** 1.6
    extra = (
        f'<clipPath id="sky"><rect x="0" y="0" width="{W}" height="{hz}"/></clipPath>'
        f'<clipPath id="sea"><rect x="0" y="{hz+3}" width="{W}" height="{depth}"/></clipPath>'
        f'<mask id="water" maskUnits="userSpaceOnUse" x="0" y="0" width="{W}" height="{H}">{"".join(rules)}</mask>'
        '<filter id="wave" x="-5%" y="0" width="110%" height="100%"><feTurbulence type="fractalNoise" baseFrequency=".006 .11" numOctaves="2" seed="4"/>'
        '<feDisplacementMap in="SourceGraphic" scale="22" xChannelSelector="R" yChannelSelector="G"/></filter>')
    fr, gap = L.get("frame", FRAME), L.get("gap", 9)
    o = [defs(extra), f'<rect width="{W}" height="{H}" fill="{BLACK}"/>',
         rite(fr, fr, W - 2 * fr, H - 2 * fr, gap=gap, op=.26, d=11)]
    o.append(lockup(cx, hb, hfs, tb, tfs))
    o.append(f'<g clip-path="url(#sky)">{sk.format(c=BONE)}</g>')
    o.append(f'<g clip-path="url(#sea)" mask="url(#water)" opacity="{1 if water == RED else .72}"><g filter="url(#wave)">'
             f'<g transform="translate(0 {2*hz}) scale(1 -1)">{sk.format(c=water)}</g></g></g>')
    half = 222 * s + 26   # the cranium, which is what the water shows at the numerals
    o.append(f'<g filter="url(#inklite)">{dial(L["dx"], W - L["dx"], hz, tick=(7, 13, 22), label=L.get("num", 14), lab_op=.7, stroke=1.6, skip=(cx - half, cx + half))}</g>')
    line = L.get("line", LINE)
    lines = line if len(L["line_y"]) > 1 else (" ".join(line),)
    for j, ln in enumerate(lines):
        o.append(f'<text x="{cx}" y="{L["line_y"][j]}" class="fell" font-style="italic" font-size="{L["line_fs"]}" fill="{BONE}" text-anchor="middle">{ln}</text>')
    fs = L["url_fs"]
    if L.get("url_frame"):
        # set into the bottom edge of the frame: the rules stop either side of it
        yc = H - fr - gap / 2
        uy, tw = yc + fs * .36, 10.72 * fs
        o.append(f'<rect x="{cx - tw/2 - 40}" y="{yc - gap}" width="{tw + 80}" height="{2 * gap}" fill="{BLACK}"/>')
        dx = tw / 2 + 22
    else:
        uy, dx = L["url_y"], 118 * fs / 15
    for k in (-1, 1):
        o.append(diamond(cx + k * dx, uy - fs * .36, 7))
    o.append(f'<text x="{cx + .16 * fs:.1f}" y="{uy:.1f}" class="mono" font-size="{fs}" fill="{BONE}" fill-opacity=".72" text-anchor="middle" letter-spacing=".32em">HOSTIS.BLACK</text>')
    o.append(grain(W, H, L.get("grain", .07)))
    return "\n".join(o)

def build(water=RED, suffix=""):
    css = write_fonts()
    out = []
    for (W, H), L in SIZES.items():
        p = f"{HERE}/rising-{W}x{H}{suffix}.html"
        open(p, "w").write(head(css, W, H) + compose(W, H, L, water) + FOOT)
        out.append((p, W, H))
    return out

def card(water=RED, suffix=""):
    p = f"{HERE}/card-1200x630{suffix}.html"
    open(p, "w").write(head(write_fonts(), 1200, 630) + compose(1200, 630, CARD, water) + FOOT)
    return p

def shrink(src, dst, colours=72):
    """An og:image has to stay small: the render cut to a palette, which this two-ink art survives."""
    from PIL import Image
    im = Image.open(src).convert("RGB").quantize(colours, method=Image.Quantize.MEDIANCUT, dither=Image.Dither.NONE)
    # A palette this short can average the few red pixels into a muddier red; put HOSTIS's red back.
    pal, red = im.getpalette()[:colours * 3], tuple(int(RED[i:i+2], 16) for i in (1, 3, 5))
    for i in range(0, len(pal), 3):
        r, g, b = pal[i:i+3]
        if r > 90 and r > 2.2 * g:
            k = min(1, r / red[0])
            pal[i:i+3] = list(red) if k > .6 else pal[i:i+3]
    im.putpalette(pal)
    im.save(dst, optimize=True)

if __name__ == "__main__":
    import sys
    bone = "--bone" in sys.argv
    if "--card" in sys.argv:
        import render, tempfile
        p = card(BONE if bone else RED, "-bone" if bone else "")
        dst = "/mnt/user-data/outputs/og-radio-bone.png" if bone else f"{os.path.dirname(BRAND)}/landing/og-radio.png"
        tmp = tempfile.mktemp(suffix=".png")
        render.render([(p, tmp, 1200, 630, 1)])
        shrink(tmp, dst); os.remove(tmp)
        print(dst, os.path.getsize(dst)); sys.exit()
    pages = build(BONE if bone else RED, "-bone" if bone else "")
    if "--render" in sys.argv:
        import render
        i = sys.argv.index("--render")
        d = sys.argv[i + 1] if len(sys.argv) > i + 1 and not sys.argv[i + 1].startswith("--") else "/mnt/user-data/outputs"
        os.makedirs(d, exist_ok=True)
        render.render([(p, f"{d}/pyrate-coming-soon-{W}x{H}{'-bone' if bone else ''}.png", W, H, 1) for p, W, H in pages])
    for p, W, H in pages:
        print(p)
