#!/usr/bin/env python3
"""Checks that two of the three release keys signed the list of open players, before anything
names the newest one. Python's standard library only, so it runs on any machine with Python 3.

  verify_players.py <players.txt> <players.sigs>
  verify_players.py --fetch https://hostis.black/pyrateradio/ipfs

Prints "<how many players> <newest player>" and exits 0 when the list is signed; prints why
not on stderr and exits 1 otherwise. The keys below are the app's (app/assets/release_keys.txt);
--keys <file> stands in other keys for a trial. The keyholders sign the list with
`release sign-all`. No rights reserved."""
import hashlib, struct, sys, urllib.request

KEYS = [
    "c33f56e169e507c2a4303a5f75a79340a058e310ac852013e583f5e657d8752f",
    "4ba81545a8e79b9856278b57b3ab1cc097ffa169b81cf56615dffee9ec3ce4df",
    "2cd5c5a95880cf7f2a7d4b5990f620e3ea1bc6e63ff3d60e542827416f5a862d",
]

# Ed25519 verification after RFC 8032, section 5.1.7.
P = 2**255 - 19
L = 2**252 + 27742317777372353535851937790883648493
D = -121665 * pow(121666, P - 2, P) % P
SQRT_M1 = pow(2, (P - 1) // 4, P)


def _x(y, sign):
    xx = (y * y - 1) * pow(D * y * y + 1, P - 2, P) % P
    x = pow(xx, (P + 3) // 8, P)
    if (x * x - xx) % P:
        x = x * SQRT_M1 % P
    if (x * x - xx) % P or (x == 0 and sign):
        return None
    return P - x if x & 1 != sign else x


def _add(a, b):
    A = (a[1] - a[0]) * (b[1] - b[0]) % P
    B = (a[1] + a[0]) * (b[1] + b[0]) % P
    C = 2 * a[3] * b[3] * D % P
    E = 2 * a[2] * b[2] % P
    e, f, g, h = B - A, E - C, E + C, B + A
    return (e * f % P, g * h % P, f * g % P, e * h % P)


def _mul(s, pt):
    q = (0, 1, 1, 0)
    while s:
        if s & 1:
            q = _add(q, pt)
        pt = _add(pt, pt)
        s >>= 1
    return q


def _point(b):
    y = int.from_bytes(b, "little")
    sign, y = y >> 255, y & ((1 << 255) - 1)
    if y >= P:
        return None
    x = _x(y, sign)
    return None if x is None else (x, y, 1, x * y % P)


_BY = 4 * pow(5, P - 2, P) % P
BASE = (_x(_BY, 0), _BY, 1, _x(_BY, 0) * _BY % P)


def verify(pub, msg, sig):
    if len(pub) != 32 or len(sig) != 64:
        return False
    a, r = _point(pub), _point(sig[:32])
    s = int.from_bytes(sig[32:], "little")
    if a is None or r is None or s >= L:
        return False
    k = int.from_bytes(hashlib.sha512(sig[:32] + pub + msg).digest(), "little") % L
    lhs, rhs = _mul(s, BASE), _add(r, _mul(k, a))
    return (lhs[0] * rhs[2] - rhs[0] * lhs[2]) % P == 0 and (lhs[1] * rhs[2] - rhs[1] * lhs[2]) % P == 0


def players(raw):
    return [c for c in raw.decode("utf-8", "replace").split() if c.startswith("bafy")]


def message(raw):
    """What each key signs (release sign-all): pyrate/players/v1, the count, the hash."""
    return b"pyrate/players/v1\0" + struct.pack("<I", len(players(raw))) + hashlib.sha256(raw).digest()


def signers(raw, sigs, keys):
    msg, good = message(raw), set()
    for line in sigs.decode("utf-8", "replace").splitlines():
        parts = line.split()
        if len(parts) != 2 or not parts[0].isdigit() or int(parts[0]) >= len(keys):
            continue
        try:
            if verify(bytes.fromhex(keys[int(parts[0])]), msg, bytes.fromhex(parts[1])):
                good.add(int(parts[0]))
        except ValueError:
            continue
    return len(good)


def main(argv):
    keys = KEYS
    if "--keys" in argv:
        i = argv.index("--keys")
        keys = open(argv[i + 1]).read().split()
        argv = argv[:i] + argv[i + 2:]
    if argv[:1] == ["--fetch"]:
        base = argv[1].rstrip("/")
        def get(name):
            with urllib.request.urlopen(f"{base}/{name}", timeout=60) as r:
                return r.read()
        try:
            raw = get("players.txt")
            sigs = get("players.sigs")
        except Exception as e:
            print(f"could not fetch the list or its signatures: {e}", file=sys.stderr)
            return 1
    else:
        raw, sigs = open(argv[0], "rb").read(), open(argv[1], "rb").read()
    n = signers(raw, sigs, keys)
    if n < 2 or not players(raw):
        print(f"players.txt is signed by {n} release key{'s' if n != 1 else ''}; it needs two", file=sys.stderr)
        return 1
    print(len(players(raw)), players(raw)[-1])
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
