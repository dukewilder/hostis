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

def linked(prefix="fonts/"):
    """The same faces as files beside the page, so the first paint does not wait for 400 KB of
    type inside it: the @font-face rules pointing at them, {file name: bytes}, and the file that
    holds each face's basic Latin, by (family, weight, style), for preloading. A name carries its
    face and a fingerprint of its bytes, so a cached copy is never stale."""
    import base64, hashlib
    css, files, latin = [], {}, {}
    for b in re.findall(r"@font-face\s*{.*?}", faces(), re.S):
        fam = re.search(r"font-family:\s*['\"]([^'\"]+)['\"]", b).group(1)
        w = (re.search(r"font-weight:\s*(\d+)", b) or [0, "400"])[1]
        st = (re.search(r"font-style:\s*(\w+)", b) or [0, "normal"])[1]
        data = base64.b64decode(re.search(r"base64,([A-Za-z0-9+/=]+)", b).group(1))
        name = re.sub(r"[^a-z0-9]+", "-", fam.lower()) + "-" + w + ("-italic" if st == "italic" else "") + "-" + hashlib.sha256(data).hexdigest()[:8] + ".woff2"
        files[name] = data
        if "unicode-range" not in b or "U+0000-00FF" in b: latin[(fam, w, st)] = prefix + name
        rule = re.sub(r"url\(['\"]?data:[^)]*\)(\s*format\([^)]*\))?", f"url({prefix}{name}) format('woff2')", b)
        if "font-display" not in rule: rule = rule.replace("{", "{font-display:swap;", 1)
        css.append(rule)
    return "\n".join(css), files, latin

def sheet():
    return open(f"{R}/brand/hostis.css", encoding="utf-8").read()

if __name__ == "__main__":
    print(len(faces()), "bytes of faces")
