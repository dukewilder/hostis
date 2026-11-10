# Draws the app's icons from the HOSTIS skull: the ☠ of Noto Sans Symbols 2, the glyph the
# site's own bar wears, in bone on HOSTIS black. Launcher, adaptive foreground (and its
# monochrome), and the notification icon as a vector. The plain icon is left alone.
import os
from PIL import Image, ImageDraw, ImageFont
from fontTools.ttLib import TTFont
from fontTools.pens.svgPathPen import SVGPathPen
from fontTools.pens.transformPen import TransformPen
R = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
FONT = f"{R}/app/assets/fonts/NotoSansSymbols2-Skull.ttf"
RES = f"{R}/app/android/app/src/main/res"
BONE, BLACK = (237, 228, 206, 255), (3, 3, 3, 255)
SK = "☠"

def skull(size, frac):
    """The skull, centred, its larger side frac of size, bone on clear."""
    im = Image.new("RGBA", (size * 4, size * 4), (0, 0, 0, 0))
    big = size * 4
    f = ImageFont.truetype(FONT, int(big * frac * 1.25))
    d = ImageDraw.Draw(im)
    l, t, r, b = d.textbbox((0, 0), SK, font=f)
    s = big * frac / max(r - l, b - t)
    f = ImageFont.truetype(FONT, int(big * frac * 1.25 * s))
    l, t, r, b = d.textbbox((0, 0), SK, font=f)
    d.text(((big - (r - l)) / 2 - l, (big - (b - t)) / 2 - t), SK, font=f, fill=BONE)
    return im.resize((size, size), Image.LANCZOS)

for dpi, n in {"mdpi": 48, "hdpi": 72, "xhdpi": 96, "xxhdpi": 144, "xxxhdpi": 192}.items():
    base = Image.new("RGBA", (n, n), BLACK)
    base.alpha_composite(skull(n, 0.74))
    base.convert("RGB").save(f"{RES}/mipmap-{dpi}/ic_launcher.png")
    fg = n * 9 // 4  # 108dp of a 48dp icon
    # The adaptive safe zone is the middle 66dp of 108: the skull sits well inside it.
    skull(fg, 0.5).save(f"{RES}/mipmap-{dpi}/ic_launcher_foreground.png")

# The computers: a Mac's icon is a rounded square standing in its own margin, Windows takes
# an .ico of several sizes, Linux a PNG beside its .desktop entry.
APP = f"{R}/app"
def tile(n, radius=0.225, margin=0.1):
    im = Image.new("RGBA", (n * 4, n * 4), (0, 0, 0, 0))
    m, r = int(n * 4 * margin), int(n * 4 * (1 - 2 * margin) * radius)
    ImageDraw.Draw(im).rounded_rectangle((m, m, n * 4 - m, n * 4 - m), r, fill=BLACK, outline=(237, 228, 206, 60), width=max(1, n // 64))
    im = im.resize((n, n), Image.LANCZOS)
    im.alpha_composite(skull(n, 0.56))
    return im
for n in [16, 32, 64, 128, 256, 512, 1024]:
    tile(n).save(f"{APP}/macos/Runner/Assets.xcassets/AppIcon.appiconset/app_icon_{n}.png")
sq = Image.new("RGBA", (256, 256), BLACK); sq.alpha_composite(skull(256, 0.8))
sq.save(f"{APP}/windows/runner/resources/app_icon.ico", sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])
sq.save(f"{APP}/linux/pyrate-radio.png")

# The notification icon: the same glyph as a vector, white on clear, in a 24 by 24 box.
font = TTFont(FONT)
gname = font.getBestCmap()[ord(SK)]
gs = font.getGlyphSet()
from fontTools.pens.boundsPen import BoundsPen
bp = BoundsPen(gs); gs[gname].draw(bp)
x0, y0, x1, y1 = bp.bounds
scale = 22 / max(x1 - x0, y1 - y0)
dx = (24 - (x1 - x0) * scale) / 2 - x0 * scale
dy = (24 + (y1 - y0) * scale) / 2 + y0 * scale
pen = SVGPathPen(gs, ntos=lambda v: f"{v:.2f}".rstrip("0").rstrip("."))
gs[gname].draw(TransformPen(pen, (scale, 0, 0, -scale, dx, dy)))
open(f"{RES}/drawable/ic_flag.xml", "w").write(
    '<vector xmlns:android="http://schemas.android.com/apk/res/android" android:width="24dp" android:height="24dp" android:viewportWidth="24" android:viewportHeight="24">\n'
    f'    <path android:fillColor="#FFFFFFFF" android:pathData="{pen.getCommands()}"/>\n</vector>\n')
print("icons drawn")
