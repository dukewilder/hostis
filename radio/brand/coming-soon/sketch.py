# Three quick concepts at 1600x900, to choose between. Run: python3 sketch.py
import math
from art import *

LINE = "Soon on the air. Never off it."

def rising(W=1600, H=900):
    hz = 620
    wm, wb = word(W/2, 58, 740)
    S = 790
    top = hz - (MARK_INK["nose"] - MARK_INK["skull_y0"]) * S
    clip = f'<clipPath id="sky"><rect x="0" y="0" width="{W}" height="{hz}"/></clipPath><clipPath id="sea"><rect x="0" y="{hz+2}" width="{W}" height="{H}"/></clipPath>'
    stripes = "".join(f'<rect x="0" y="{hz+4+i*7 + i*i*.06:.1f}" width="{W}" height="{max(.6, 3.6 - i*.12):.2f}" fill="#fff"/>' for i in range(34))
    mask = f'<mask id="ripple" maskUnits="userSpaceOnUse" x="0" y="0" width="{W}" height="{H}">{stripes}</mask>'
    wave = '<filter id="wave"><feTurbulence type="fractalNoise" baseFrequency=".004 .09" numOctaves="2" seed="4"/><feDisplacementMap in="SourceGraphic" scale="26" xChannelSelector="R" yChannelSelector="G"/></filter>'
    sk = mark_at(W/2, top, S)
    refl = f'<g clip-path="url(#sea)" mask="url(#ripple)" opacity=".55"><g filter="url(#wave)"><g transform="translate(0 {2*hz}) scale(1 -1)">{sk}</g></g></g>'
    x0, x1 = 110, W - 110
    f = LO + (W/2 - x0) / (x1 - x0) * (HI - LO)
    body = [defs(clip + mask + wave), f'<rect width="{W}" height="{H}" fill="{BLACK}"/>',
            wm, f'<g clip-path="url(#sky)">{sk}</g>', refl,
            f'<rect x="0" y="{hz+40}" width="{W}" height="{H}" fill="{BLACK}" opacity="0"/>',
            dial(x0, x1, hz, tick=(8, 14, 24), label=14, op=.9),
            f'<line x1="{W/2}" y1="{hz-1}" x2="{W/2}" y2="{hz+34}" stroke="{RED}" stroke-width="3"/>', diamond(W/2, hz, 12),
            f'<text x="{W/2}" y="792" class="fell" font-style="italic" font-size="46" fill="{BONE}" text-anchor="middle">{LINE}</text>',
            f'<text x="{W/2}" y="852" class="mono" font-size="16" fill="{BONE}" fill-opacity=".55" text-anchor="middle" letter-spacing=".3em">HOSTIS.BLACK</text>',
            grain(W, H)]
    return "\n".join(body)

def seal(W=1600, H=900):
    cx, cy, R = 520, H/2, 360
    a0, a1 = -150, 150   # degrees from straight up, clockwise
    def pt(f, r):
        a = math.radians(a0 + (f - LO) / (HI - LO) * (a1 - a0))
        return cx + r * math.sin(a), cy - r * math.cos(a)
    o = [f'<circle cx="{cx}" cy="{cy}" r="{R}" fill="none" stroke="{BONE}" stroke-width="3"/>',
         f'<circle cx="{cx}" cy="{cy}" r="{R+12}" fill="none" stroke="{BONE}" stroke-width="1.2" stroke-opacity=".6"/>',
         f'<circle cx="{cx}" cy="{cy}" r="{R-78}" fill="none" stroke="{BONE}" stroke-width="1.2" stroke-opacity=".6"/>']
    for i in range(round((HI - LO) * 10) + 1):
        f = round(LO + i / 10, 1)
        whole = abs(f - round(f)) < 1e-6; half = abs(f*2 - round(f*2)) < 1e-6
        L = 30 if whole else 18 if half else 10
        (xa, ya), (xb, yb) = pt(f, R), pt(f, R - L)
        o.append(f'<line x1="{xa:.1f}" y1="{ya:.1f}" x2="{xb:.1f}" y2="{yb:.1f}" stroke="{BONE}" stroke-width="{2.2 if whole else 1.4 if half else 1}"/>')
        if whole and int(round(f)) % 2 == 0:
            xt, yt = pt(f, R - 52)
            o.append(f'<text x="{xt:.1f}" y="{yt+6:.1f}" class="mono" font-size="17" fill="{BONE}" fill-opacity=".7" text-anchor="middle">{int(round(f))}</text>')
    (xa, ya), (xb, yb) = pt(101.3, R + 30), pt(101.3, R - 34)
    o.append(f'<line x1="{xa:.1f}" y1="{ya:.1f}" x2="{xb:.1f}" y2="{yb:.1f}" stroke="{RED}" stroke-width="4"/>')
    o.append(mark_at(cx, cy - 220, 470))
    wm, wb = word(1170, 300, 640)
    body = [defs(), f'<rect width="{W}" height="{H}" fill="{BLACK}"/>', "\n".join(o), wm,
            f'<text x="1170" y="{wb+90}" class="fell" font-style="italic" font-size="40" fill="{BONE}" text-anchor="middle">{LINE}</text>',
            f'<text x="1170" y="{wb+150}" class="mono" font-size="15" fill="{BONE}" fill-opacity=".55" text-anchor="middle" letter-spacing=".3em">HOSTIS.BLACK</text>',
            grain(W, H)]
    return "\n".join(body)

def broadside(W=1600, H=900):
    INK = "#0d0b08"
    o = [defs(), f'<rect width="{W}" height="{H}" fill="{BONE}"/>',
         rite(36, 36, W - 72, H - 72, gap=10, col=INK, op=.85, d=14)]
    o.append(f'<text x="{W/2}" y="118" class="carved" font-weight="600" font-size="20" fill="{RED}" text-anchor="middle" letter-spacing=".42em">BY THE ARTICLES OF HOSTIS</text>')
    wm, wb = word(W/2, 150, 1060, col=INK)
    o.append(wm)
    o.append(dial(200, W - 200, wb + 70, col=INK, tick=(9, 16, 26), label=14, op=.95, lab_op=.8))
    x = fx(101.3, 200, W - 200)
    o.append(f'<line x1="{x}" y1="{wb+40}" x2="{x}" y2="{wb+100}" stroke="{RED}" stroke-width="4"/>')
    o.append(f'<text x="{W/2}" y="{wb+210}" class="fell" font-style="italic" font-size="54" fill="{INK}" text-anchor="middle">{LINE}</text>')
    o.append(f'<text x="{W/2}" y="{H-80}" class="mono" font-size="16" fill="{INK}" fill-opacity=".7" text-anchor="middle" letter-spacing=".3em">HOSTIS.BLACK</text>')
    o.append(f'<rect width="{W}" height="{H}" filter="url(#grain)" opacity=".10" style="mix-blend-mode:multiply"/>')
    return "\n".join(o)

if __name__ == "__main__":
    css = write_fonts()
    for name, fn, bg in (("a-rising", rising, BLACK), ("b-seal", seal, BLACK), ("c-broadside", broadside, BONE)):
        open(f"{HERE}/sketch-{name}.html", "w").write(head(css, 1600, 900, bg) + fn() + FOOT)
        print(name)
