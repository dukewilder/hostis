# Builds hostis.black/pyrateradio/ into landing/out: the landing page, the
# browser radio at /listen/, and the app. Fonts come from the HOSTIS site, so one ship
# keeps one type.
import re, os, shutil, sys, hashlib, subprocess
R = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
# The page names the app it offers, so it is built only with one: build.py <apk> <version>.
# A draft for looking at, with placeholders in place of the app, takes --draft instead.
if len(sys.argv) < 3 and "--draft" not in sys.argv:
    sys.exit("build.py <apk> <version>, or --draft for a local look with no app")
apk = sys.argv[1] if len(sys.argv) > 2 else None
version = sys.argv[2] if len(sys.argv) > 2 else "draft"
out = f"{R}/landing/out"
site = open(f"{R}/../index.html", encoding="utf-8").read()
faces = [b for b in re.findall(r"@font-face\s*{.*?}", site, re.S)
         if re.search(r"font-family:\s*['\"](Cinzel|IM Fell English|JetBrains Mono)['\"]", b) and "base64," in b]
if os.path.exists(out): shutil.rmtree(out)
os.makedirs(out)
page = open(f"{R}/landing/index.html", encoding="utf-8").read().replace("/*FONTS*/", "\n".join(faces))
if apk:
    data = open(apk, "rb").read()
    sha, size = hashlib.sha256(data).hexdigest(), f"{len(data)/1e6:.0f}"
    shutil.copy(apk, f"{out}/pyrate-radio-{version}.apk")
    # The app by BitTorrent too: a torrent with this site as a web seed, and its magnet link.
    magnet = subprocess.run([sys.executable, f"{R}/tools/torrent.py", f"{out}/pyrate-radio-{version}.apk", f"{out}/pyrate-radio-{version}.apk.torrent",
                             f"https://hostis.black/pyrateradio/pyrate-radio-{version}.apk"], capture_output=True, text=True, check=True).stdout.strip()
    
else:
    sha, size, magnet = "(build pending)", "?", "#"
page = page.replace("{{VERSION}}", version).replace("{{SHA}}", sha).replace("{{SIZE}}", size).replace("{{MAGNET}}", magnet.replace("&", "&amp;"))
open(f"{out}/index.html", "w", encoding="utf-8").write(page)
shutil.copytree(f"{R}/browser/www/dist", f"{out}/listen")
for f in ["wordmark.png", "mark.png"]:
    shutil.copy(f"{R}/node/src/web/{f}", out)
print("site at", out, "·", len(faces), "faces ·", sha[:16])
