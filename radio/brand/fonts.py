# The HOSTIS faces, taken from the site's own sheet (account.html), so the radio's pages
# set type exactly as the sibling pages do. Nothing is fetched: every face is embedded.
import os, re
R = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
WANT = {"Cinzel", "IM Fell English", "IM Fell DW Pica SC", "JetBrains Mono"}

def faces():
    site = open(f"{R}/../account.html", encoding="utf-8").read()
    out = []
    for b in re.findall(r"@font-face\s*{.*?}", site, re.S):
        fam = re.search(r"font-family:\s*['\"]([^'\"]+)['\"]", b).group(1)
        if "base64," not in b: continue
        # Of the symbol face, only the block that holds the skull and the arrows (U+25A0-27BF).
        if fam == "Noto Sans Symbols 2" and "U+25A0-27BF" in b: out.append(b)
        elif fam in WANT: out.append(b)
    assert len(out) >= 10, len(out)
    return "\n".join(out)

def sheet():
    return open(f"{R}/brand/hostis.css", encoding="utf-8").read()

if __name__ == "__main__":
    print(len(faces()), "bytes of faces")
