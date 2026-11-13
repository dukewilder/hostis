# Split the potrace mark into its skull and its microphones, by the bounding box of each path.
import re
def bbox(d):
    toks = re.findall(r"[MmCcLlZz]|-?\d*\.?\d+", d)
    x = y = sx = sy = 0; cmd = None; xs = []; ys = []; i = 0
    while i < len(toks):
        t = toks[i]
        if t.isalpha():
            cmd = t; i += 1
            if cmd in "zZ": x, y = sx, sy
            continue
        if cmd == "M": x, y = float(toks[i]), float(toks[i+1]); sx, sy = x, y; i += 2; cmd = "l"
        elif cmd == "m": x += float(toks[i]); y += float(toks[i+1]); sx, sy = x, y; i += 2; cmd = "l"
        elif cmd == "c": x += float(toks[i+4]); y += float(toks[i+5]); i += 6
        elif cmd == "l": x += float(toks[i]); y += float(toks[i+1]); i += 2
        else: raise ValueError(cmd)
        xs.append(x); ys.append(y)
    # potrace space: X = x/10, Y = 1144 - y/10
    return min(xs)/10, max(xs)/10, 1144 - max(ys)/10, 1144 - min(ys)/10
def parts(svg):
    return [(d, bbox(d)) for d in re.findall(r'<path d="([^"]+)"', svg)]
if __name__ == "__main__":
    s = open("../pyrate-mark.svg").read()
    for d, b in parts(s): print([round(v) for v in b])
