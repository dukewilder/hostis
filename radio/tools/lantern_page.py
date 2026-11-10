# Builds the Lantern's own page (node/src/web/listen.html, built into the binary) from
# listen.src.html with the HOSTIS faces and sheet from brand/. Run after either changes.
import os, sys
R = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, f"{R}/brand")
import fonts
src = open(f"{R}/node/src/web/listen.src.html", encoding="utf-8").read()
assert src.count("/*FONTS*/") == 1 and src.count("/*HOSTIS*/") == 1
out = src.replace("/*FONTS*/", fonts.faces()).replace("/*HOSTIS*/", fonts.sheet())
open(f"{R}/node/src/web/listen.html", "w", encoding="utf-8").write(out)
print("listen.html", len(out), "bytes")
