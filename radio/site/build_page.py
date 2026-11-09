# Builds radio/index.html: the download page, in the site's type and ink, fonts embedded.
import re, base64, sys
root = sys.argv[1]
site = open(f"{root}/index.html", encoding="utf-8").read()
faces = []
for b in re.findall(r"@font-face\s*{.*?}", site, re.S):
    fam = re.search(r"font-family:\s*['\"]([^'\"]+)", b).group(1)
    if fam in ("Cinzel", "IM Fell English", "JetBrains Mono") and "base64," in b:
        faces.append(b)
sha = open("/tmp/apk.sha").read().strip()
tpl = open(f"{root}/radio/site/page.html", encoding="utf-8").read()
out = tpl.replace("/*FONTS*/", "\n".join(faces)).replace("{{SHA}}", sha)
open(f"{root}/radio/index.html", "w", encoding="utf-8").write(out)
print(len(out), "bytes,", len(faces), "faces")
