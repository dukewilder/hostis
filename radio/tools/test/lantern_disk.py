#!/usr/bin/env python3
"""Walks a Lantern's data folder and fails on anything kept in the clear.

Every file must be either sealed under the hold key (named so nobody without the key can say
what it is, and noise inside) or a known public artefact (a signed release, the keeper's own
maroon list). Anything else is a finding: a key in the clear, a plain log, a plain episode, a
file this script does not know.

Usage: lantern_disk.py <data folder> [--needle <text>]...
  --needle   text that must appear in no file name and no file, such as a station's name, its
             id or a recording's title. Repeatable.
Prints one FAIL line per finding and a last PASS or FAIL line. Exits 1 on any finding.
Reads at most the first megabyte of each file, so a large hold is walked quickly.
Writes nothing and prints no file's contents.
"""
import os
import re
import sys
import zlib

HEX64 = r"[0-9a-f]{64}"
# Sealed under the hold key: (pattern, what it is).
SEALED = [
    (rf"notes/{HEX64}(\.part)?", "a note"),
    (rf"keys/{HEX64}\.k2", "a station key"),
    (rf"logs/{HEX64}\.(sl2|tmp)", "a station log"),
    (rf"hold/[0-9a-f]{{2}}/{HEX64}(\.part)?", "a chunk"),
    (rf"episodes/{HEX64}\.ep", "an episode"),
    (rf"episodes/{HEX64}\.\d+\.part", "an episode being made"),
    (r"(.*/)?\.[^/]+\.\d+\.tmp", "a secret being written"),
]
# Public, or the keeper's own input, and fine in the clear.
PUBLIC = [
    (r"updates/pyrate-[0-9]+\.(apk|part)", "a signed release"),
    (r"releases/[^/]+", "a signed release file"),
    (r"maroon\.txt", "the keeper's maroon list"),
]
SEALED_KEY = "hold.sealed"
READ = 1 << 20


def noise(b: bytes) -> bool:
    """Whether bytes look like ciphertext: no long run of text, and they do not compress."""
    if re.search(rb"[\x20-\x7e\t\r\n]{24,}", b):
        return False
    if len(b) >= 512 and len(zlib.compress(b, 6)) < 0.9 * len(b):
        return False
    return True


def main() -> int:
    args = sys.argv[1:]
    if not args or args[0] in ("-h", "--help"):
        print(__doc__)
        return 2
    root = os.path.abspath(args[0])
    needles = [args[i + 1].encode() for i, a in enumerate(args) if a == "--needle" and i + 1 < len(args)]
    if not os.path.isdir(root):
        print(f"FAIL {root} is not a folder")
        return 1
    findings, sealed, public = [], 0, 0
    for d, dirs, files in os.walk(root):
        for f in files:
            p = os.path.join(d, f)
            rel = os.path.relpath(p, root).replace(os.sep, "/")
            if os.path.islink(p):
                findings.append(f"{rel} is a link")
                continue
            for n in needles:
                if n.decode(errors="ignore").lower() in rel.lower():
                    findings.append(f"{rel} names something it must not (a needle is in its name)")
            try:
                with open(p, "rb") as fh:
                    b = fh.read(READ)
            except OSError as e:
                findings.append(f"{rel} could not be read ({e.strerror})")
                continue
            for n in needles:
                if n and n in b:
                    findings.append(f"{rel} holds a needle in the clear")
            if rel == SEALED_KEY:
                if len(b) == 88 and b[:4] == b"PYH1" and noise(b[28:]):
                    sealed += 1
                else:
                    findings.append(f"{rel} is not a sealed hold key")
                continue
            if rel == "hold.key" or f == "hold.key":
                findings.append(f"{rel} is a hold key in the clear")
                continue
            kind = next((w for pat, w in SEALED if re.fullmatch(pat, rel)), None)
            if kind:
                if len(b) == 0 or noise(b):
                    sealed += 1
                else:
                    findings.append(f"{rel} is filed as {kind} but is not sealed")
                continue
            kind = next((w for pat, w in PUBLIC if re.fullmatch(pat, rel)), None)
            if kind:
                public += 1
                continue
            findings.append(f"{rel} is not sealed and not a known public artefact")
    for x in findings:
        print(f"FAIL {x}")
    if findings:
        print(f"FAIL {len(findings)} findings in {root} ({sealed} sealed, {public} public)")
        return 1
    print(f"PASS {root}: {sealed} files sealed, {public} public, nothing else")
    return 0


if __name__ == "__main__":
    sys.exit(main())
