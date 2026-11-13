# PYRATE RADIO, coming soon. The skull rises over the FM band as a sun over the sea: the dial
# is the horizon, and the skull's reflection is cut into the water in red engraved lines.
#   python3 rising.py            writes rising-<W>x<H>.html for each size
#   python3 rising.py --bone     the same with the reflection in bone instead of red
#   python3 rising.py --render [dir]   also renders each to pyrate-coming-soon-<W>x<H>.png in dir
#                                      (default /mnt/user-data/outputs)
from art import *

LINE = ("Soon on the air.", "Never off it.")

SIZES = {
    (1600, 900): dict(word_top=64, word_w=820, hz=594, S=750, dx=118, refl=150,
                      line_y=(785,), line_fs=50, url_y=840, url_fs=19),
    (1080, 1350): dict(word_top=128, word_w=880, hz=842, S=1060, dx=78, refl=196,
                       line_y=(1112, 1180), line_fs=62, url_y=1264, url_fs=21),
    (1080, 1080): dict(word_top=92, word_w=800, hz=668, S=880, dx=78, refl=150,
                       line_y=(894,), line_fs=50, url_y=972, url_fs=18),
}
CUT = .505     # where the horizon crosses the skull, as a fraction of the mark's box: under the nose
FRAME = 26     # inset of the ruled double edge

def compose(W, H, L, water=RED):
    hz, S, cx, depth = L["hz"], L["S"], W / 2, L["refl"]
    top = hz - (CUT - SKULL["top"]) * S
    x, y = cx - S * SKULL["cx"], top - S * SKULL["top"]
    sk = f'<use href="#skull" x="{x:.1f}" y="{y:.1f}" width="{S}" height="{S}" color="{{c}}"/>'
    # The water: rules that open out and thin as they go down, so the reflection breaks up.
    rules, yy = [], hz + 5.0
    while yy < hz + depth:
        t = (yy - hz) / depth
        rules.append(f'<rect x="0" y="{yy:.1f}" width="{W}" height="{max(.5, 2.8 * (1 - t) ** 1.2):.2f}" fill="#fff"/>')
        yy += 5.6 + 9 * t ** 1.6
    extra = (skull_symbol() +
        f'<clipPath id="sky"><rect x="0" y="0" width="{W}" height="{hz}"/></clipPath>'
        f'<clipPath id="sea"><rect x="0" y="{hz+3}" width="{W}" height="{depth}"/></clipPath>'
        f'<mask id="water" maskUnits="userSpaceOnUse" x="0" y="0" width="{W}" height="{H}">{"".join(rules)}</mask>'
        '<filter id="wave" x="-5%" y="0" width="110%" height="100%"><feTurbulence type="fractalNoise" baseFrequency=".006 .11" numOctaves="2" seed="4"/>'
        '<feDisplacementMap in="SourceGraphic" scale="22" xChannelSelector="R" yChannelSelector="G"/></filter>')
    o = [defs(extra), f'<rect width="{W}" height="{H}" fill="{BLACK}"/>',
         rite(FRAME, FRAME, W - 2 * FRAME, H - 2 * FRAME, gap=9, op=.26, d=11)]
    o.append(word(cx, L["word_top"], L["word_w"])[0])
    o.append(f'<g clip-path="url(#sky)">{sk.format(c=BONE)}</g>')
    o.append(f'<g clip-path="url(#sea)" mask="url(#water)" opacity="{1 if water == RED else .6}"><g filter="url(#wave)">'
             f'<g transform="translate(0 {2*hz}) scale(1 -1)">{sk.format(c=water)}</g></g></g>')
    half = (SKULL["x1"] - SKULL["x0"]) * S * .5 + 30
    o.append(f'<g filter="url(#inklite)">{dial(L["dx"], W - L["dx"], hz, tick=(7, 13, 22), label=14, lab_op=.7, stroke=1.6, skip=(cx - half, cx + half))}</g>')
    lines = LINE if len(L["line_y"]) > 1 else (" ".join(LINE),)
    for j, ln in enumerate(lines):
        o.append(f'<text x="{cx}" y="{L["line_y"][j]}" class="fell" font-style="italic" font-size="{L["line_fs"]}" fill="{BONE}" text-anchor="middle">{ln}</text>')
    uy, fs = L["url_y"], L["url_fs"]
    for s in (-1, 1):
        o.append(diamond(cx + s * 118 * fs / 15, uy - fs * .36, 7))
    o.append(f'<text x="{cx}" y="{uy}" class="mono" font-size="{fs}" fill="{BONE}" fill-opacity=".72" text-anchor="middle" letter-spacing=".32em">HOSTIS.BLACK</text>')
    o.append(grain(W, H))
    return "\n".join(o)

def build(water=RED, suffix=""):
    css = write_fonts()
    out = []
    for (W, H), L in SIZES.items():
        p = f"{HERE}/rising-{W}x{H}{suffix}.html"
        open(p, "w").write(head(css, W, H) + compose(W, H, L, water) + FOOT)
        out.append((p, W, H))
    return out

if __name__ == "__main__":
    import sys
    bone = "--bone" in sys.argv
    pages = build(BONE if bone else RED, "-bone" if bone else "")
    if "--render" in sys.argv:
        import render
        i = sys.argv.index("--render")
        d = sys.argv[i + 1] if len(sys.argv) > i + 1 and not sys.argv[i + 1].startswith("--") else "/mnt/user-data/outputs"
        os.makedirs(d, exist_ok=True)
        render.render([(p, f"{d}/pyrate-coming-soon-{W}x{H}{'-bone' if bone else ''}.png", W, H, 1) for p, W, H in pages])
    for p, W, H in pages:
        print(p)
