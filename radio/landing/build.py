# Builds hostis.black/pyrateradio/ into landing/out: the landing page, the
# browser radio at /listen/, and the app. Fonts come from the HOSTIS site, so one ship
# keeps one type.
import re, os, shutil, sys, hashlib, subprocess
R = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
apk = sys.argv[1] if len(sys.argv) > 1 else None
version = sys.argv[2] if len(sys.argv) > 2 else "0.6.2"
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
    
else:
    sha, size = "(build pending)", "?"
page = page.replace("{{VERSION}}", version).replace("{{SHA}}", sha).replace("{{SIZE}}", size)
open(f"{out}/index.html", "w", encoding="utf-8").write(page)
shutil.copytree(f"{R}/browser/www/dist", f"{out}/listen")
for f in ["wordmark.png", "mark.png"]:
    shutil.copy(f"{R}/node/src/web/{f}", out)
print("site at", out, "·", len(faces), "faces ·", sha[:16])
