# The open player as plain files, for whatever passes it on: a Lantern's image (--dir) or the
# computers' app (--tar). No rights reserved.
#
#   python3 tools/open_player.py --dir <folder>     the player's files in a folder
#   python3 tools/open_player.py --tar <file.tar>   the same, as one tar (the app's asset)
#
# It takes browser/www/dist-open when the player has been built here. Otherwise it fetches the
# player the app names (app/assets/open_players.txt) from IPFS as a CAR and checks every block
# against its fingerprint before using a byte of it, so no gateway can slip in a file of its own.
# Without --need, a player it cannot get leaves the folder empty and the build carries on.
import hashlib, io, os, re, sys, tarfile, urllib.request

R = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
LOCAL = f"{R}/browser/www/dist-open"
GATEWAYS = ["https://trustless-gateway.link", "https://ipfs.io", "https://dweb.link"]
B32 = "abcdefghijklmnopqrstuvwxyz234567"


def varint(b, i):
    n = s = 0
    while True:
        x = b[i]; i += 1
        n |= (x & 0x7F) << s; s += 7
        if x < 0x80: return n, i


def b32decode(t):
    bits = n = 0; out = bytearray()
    for c in t:
        n = (n << 5) | B32.index(c); bits += 5
        if bits >= 8: bits -= 8; out.append((n >> bits) & 0xFF)
    return bytes(out)


def cid_at(b, i):
    """A binary CID at b[i:], as (key, codec, digest, next index). Version 0 is a bare sha2-256."""
    if b[i] == 0x12 and b[i + 1] == 0x20:
        return b[i:i + 34], 0x70, b[i + 2:i + 34], i + 34
    s = i
    _, i = varint(b, i); codec, i = varint(b, i); code, i = varint(b, i); ln, i = varint(b, i)
    if code != 0x12 or ln != 32: raise ValueError("only sha2-256 names are taken")
    return b[s:i + 32], codec, b[i:i + 32], i + 32


def proto(b):
    """Fields of a protobuf message, as (field, value) in order: varints as ints, the rest as bytes."""
    i, out = 0, []
    while i < len(b):
        k, i = varint(b, i); f, t = k >> 3, k & 7
        if t == 0: v, i = varint(b, i)
        elif t == 2: ln, i = varint(b, i); v = b[i:i + ln]; i += ln
        else: raise ValueError("unexpected field type")
        out.append((f, v))
    return out


def from_car(car, root):
    """Every file under root, as {path: bytes}, from a CAR whose blocks are each checked."""
    hl, i = varint(car, 0); i += hl
    blocks = {}
    while i < len(car):
        ln, i = varint(car, i); end = i + ln
        key, codec, digest, j = cid_at(car, i)
        data = car[j:end]
        if hashlib.sha256(data).digest() != digest: raise ValueError("a block does not match its name")
        blocks[key] = (codec, data); i = end
    files = {}

    def node(key):
        if key not in blocks: raise ValueError("a block is missing")
        return blocks[key]

    def read_file(key):
        codec, data = node(key)
        if codec == 0x55: return data
        fields = proto(data)
        links = [cid_at(dict(proto(v))[1], 0)[0] for f, v in fields if f == 2]
        unix = dict((f, v) for f, v in proto(next((v for f, v in fields if f == 1), b"")) if f in (1, 2))
        return unix.get(2, b"") + b"".join(read_file(k) for k in links)

    def walk(key, prefix):
        codec, data = node(key)
        fields = proto(data)
        unix = dict((f, v) for f, v in proto(next((v for f, v in fields if f == 1), b"")) if f == 1)
        if codec != 0x70 or unix.get(1) != 1: raise ValueError("not a plain folder")
        for f, v in fields:
            if f != 2: continue
            link = dict(proto(v))
            name = link[2].decode()
            if not name or name.startswith(".") or "/" in name or "\\" in name: raise ValueError("a bad name")
            k = cid_at(link[1], 0)[0]
            c, d = node(k)
            kind = dict((f2, v2) for f2, v2 in proto(next((v2 for f2, v2 in proto(d) if f2 == 1), b"")) if f2 == 1).get(1) if c == 0x70 else None
            if kind == 1: walk(k, prefix + name + "/")
            else: files[prefix + name] = read_file(k)

    walk(root, "")
    return files


def fetch():
    names = open(f"{R}/app/assets/open_players.txt").read()
    m = re.search(r"\b(bafy[a-z2-7]{50,})\b", names)
    if not m: raise SystemExit("open_players.txt names no player")
    cid = m.group(1)
    root = b32decode(cid[1:])
    for g in GATEWAYS:
        try:
            req = urllib.request.Request(f"{g}/ipfs/{cid}?format=car&dag-scope=all", headers={"Accept": "application/vnd.ipld.car", "User-Agent": "pyrate-open-player"})
            car = urllib.request.urlopen(req, timeout=120).read()
            files = from_car(car, root)
            if "index.html" in files:
                print(f"open player {cid}, checked, from {g}", file=sys.stderr)
                return files
        except Exception as e:
            print(f"open player: {g}: {e}", file=sys.stderr)
    return None


def local():
    if not os.path.isfile(f"{LOCAL}/index.html"): return None
    files = {}
    for d, _, fs in os.walk(LOCAL):
        for f in fs:
            p = os.path.join(d, f)
            files[os.path.relpath(p, LOCAL).replace(os.sep, "/")] = open(p, "rb").read()
    print("open player from browser/www/dist-open", file=sys.stderr)
    return files


def main():
    a = sys.argv[1:]
    if len(a) < 2 or a[0] not in ("--dir", "--tar"): raise SystemExit("open_player.py --dir <folder> | --tar <file> [--need]")
    files = local() or fetch()
    if not files:
        if "--need" in a: raise SystemExit("open player: none to be had")
        print("open player: none to be had, so none is passed on", file=sys.stderr)
        files = {}
    if a[0] == "--dir":
        os.makedirs(a[1], exist_ok=True)
        for p, data in files.items():
            out = os.path.join(a[1], *p.split("/"))
            os.makedirs(os.path.dirname(out), exist_ok=True)
            open(out, "wb").write(data)
    else:
        os.makedirs(os.path.dirname(os.path.abspath(a[1])), exist_ok=True)
        # The same files make the same tar: sorted, dated nothing, owned by nobody.
        with tarfile.open(a[1], "w", format=tarfile.USTAR_FORMAT) as t:
            for p in sorted(files):
                info = tarfile.TarInfo(p); info.size = len(files[p]); info.mode = 0o644; info.mtime = 0
                t.addfile(info, io.BytesIO(files[p]))
    print(f"{len(files)} files", file=sys.stderr)


if __name__ == "__main__":
    main()
