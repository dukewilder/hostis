# hostis.black's On the Account share image, redrawn at 1200x630 from the approved 627x329 one.
# Every position below is in the old image's pixels (measured from it); the viewBox scales them.
#   python3 og_account.py [out.png]    default: /home/claude/main-wt/og-account.png
import os, sys
from art import write_fonts, HERE

BG, BONE, RED, BAR = "#0e0e0e", "#ede4ce", "#c52828", "#7d1818"

def svg():
    o = [f'<rect width="627" height="329" fill="{BG}"/>',
         '<line x1=".5" y1="0" x2=".5" y2="329" stroke="rgba(237,228,206,.11)"/>']
    grid = "rgba(237,228,206,.045)"
    for y0, y1 in ((29, 178), (214, 329)):
        o.append(f'<line x1="241.5" y1="{y0}" x2="241.5" y2="{y1}" stroke="rgba(237,228,206,.075)"/>')
        for x in (249.5, 306.5, 363.5, 478.5, 535.5, 592.5):
            o.append(f'<line x1="{x}" y1="{y0}" x2="{x}" y2="{y1}" stroke="{grid}"/>')
    for y in (65.5, 102.5, 140.5, 177.5, 251.5, 289.5, 326.5):
        o.append(f'<line x1="0" y1="{y}" x2="627" y2="{y}" stroke="rgba(237,228,206,.05)"/>')
    o.append('<line x1="421.5" y1="0" x2="421.5" y2="329" stroke="rgba(237,228,206,.12)" stroke-dasharray="2 2"/>')

    def mono(x, y, t, w, col="rgba(237,228,206,.3)", fs=6.6):
        return f'<text x="{x}" y="{y}" class="mono" font-size="{fs}" fill="{col}" textLength="{w}" lengthAdjust="spacing">{t}</text>'
    for y, num, era, sub, ew, sw in ((20.5, "I", "THE BUCCANEERS", "TORTUGA &amp; PORT ROYAL", 83, 119),
                                     (206.5, "II", "THE PIRATE ROUND", "THE RED SEA MEN", 102, 88)):
        o.append(f'<text x="11" y="{y}" class="mono" font-size="6.6" fill="{RED}">{num}</text>')
        o.append(mono(11 + (12 if num == "I" else 18), y, era, ew))
        o.append(mono(250, y, sub, sw))
    for y, name, w in ((50.5, "ROCHE BRAZILIANO", 144.5), (88.5, "FRANÇOIS L’OLONNAIS", 166.5),
                       (125.5, "SIR HENRY MORGAN", 144.5), (162.5, "BARTHOLOMEW SHARP", 167.5),
                       (236.5, "ROBERT CULLIFORD", 147.5), (274.5, "THOMAS TEW", 99.5), (311.5, "HENRY EVERY", 103.5)):
        o.append(f'<text x="11.5" y="{y}" class="sc" font-size="14.2" fill="{BONE}" textLength="{w}" lengthAdjust="spacing">{name}</text>')
    for x0, x1, y in ((253, 423, 42), (301, 400, 79), (330, 427, 117), (508, 519, 154), (558, 610, 154)):
        o.append(f'<rect x="{x0}" y="{y}" width="{x1-x0}" height="11" fill="{BAR}"/>')
    o.append('<defs><pattern id="waf" width="4.4" height="4.4" patternUnits="userSpaceOnUse" patternTransform="rotate(45)">'
             '<rect width="4.4" height="4.4" fill="#4a4339"/><rect width="2" height="4.4" fill="#ede4ce"/></pattern></defs>')
    o.append('<rect x="519" y="154" width="30" height="11" fill="url(#waf)"/>')
    for x0, x1 in ((549.5, 557.5), (610.5, 640)):
        o.append(f'<rect x="{x0}" y="154.5" width="{x1-x0}" height="10" fill="none" stroke="rgba(237,228,206,.4)" stroke-dasharray="2 1.5"/>')
    for y, xs in ((35, (357,)), (73, (370, 380)), (110, (391.5, 400.5, 420.5)), (148, (527, 541))):
        for x in xs:
            o.append(f'<path d="M{x-1.9} {y-1.2}h3.8l-1.9 2.8z" fill="#a9a397"/>')
    lab = "rgba(237,228,206,.42)"
    o.append(f'<text x="430" y="48.6" class="mono" font-size="6.4" fill="{lab}" textLength="78" lengthAdjust="spacing">? VANISHED C.1671</text>')
    o.append(f'<text x="407" y="86.6" class="skull" font-size="7" fill="{RED}">☠</text>')
    o.append(mono(418, 85.6, "TORN APART 1669", 69, lab, 6.4))
    o.append(f'<text x="434" y="124.4" class="skull" font-size="6.6" fill="{BONE}" fill-opacity=".6">⚑</text>')
    o.append(mono(444, 123.6, "DIED IN BED 1688", 76, lab, 6.4))
    return "\n".join(o)

def page():
    css = write_fonts()
    return f"""<!DOCTYPE html><html lang="en-GB"><head><meta charset="utf-8"><style>{css}
html,body{{margin:0;background:{BG}}}svg{{display:block}}
.sc{{font-family:'IM Fell DW Pica SC',Georgia,serif}}.mono{{font-family:'JetBrains Mono',monospace}}
.skull{{font-family:'Noto Sans Symbols 2',serif}}</style></head><body>
<svg xmlns="http://www.w3.org/2000/svg" width="1200" height="630" viewBox="0 0 627 329" preserveAspectRatio="none">{svg()}</svg></body></html>"""

if __name__ == "__main__":
    out = sys.argv[1] if len(sys.argv) > 1 else "/home/claude/main-wt/og-account.png"
    p = f"{HERE}/og-account.html"
    open(p, "w", encoding="utf-8").write(page())
    import render
    render.render([(p, out, 1200, 630, 1)])
    print(out)
