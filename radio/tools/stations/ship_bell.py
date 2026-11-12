#!/usr/bin/env python3
# A ship's bell for Shanties: eight bells, struck in pairs as a watch ends. Synthesised here,
# a struck bronze bell's partials each fading at its own rate. No rights reserved.
#   ship_bell.py <out.ogg>
import subprocess, sys
import numpy as np

SR = 44100
PARTIALS = [(0.5, 0.35, 1.2), (1.0, 1.0, 1.6), (1.19, 0.55, 2.2), (1.51, 0.4, 2.8), (2.0, 0.45, 3.4), (2.74, 0.22, 4.6), (3.0, 0.18, 5.2), (4.07, 0.08, 7.0)]


def strike(f0=830.0, dur=3.2, rng=np.random.default_rng(8)):
    t = np.arange(int(dur * SR)) / SR
    x = sum(a * np.sin(2 * np.pi * f0 * r * (1 + rng.normal(0, 0.0008)) * t + rng.random() * 6.28) * np.exp(-t * d) for r, a, d in PARTIALS)
    click = rng.standard_normal(len(t)) * np.exp(-t * 180) * 0.25
    return ((x + click) * np.minimum(1, t / 0.002)).astype(np.float32)


def main(out):
    hits, at = [], 0.0
    for pair in range(4):
        hits += [at, at + 0.38]
        at += 1.45
    x = np.zeros(int((at + 3.4) * SR), np.float32)
    for h in hits:
        s = strike()
        i = int(h * SR)
        x[i:i + len(s)] += s[:len(x) - i]
    x = x / np.abs(x).max() * 0.6
    subprocess.run(["ffmpeg", "-loglevel", "error", "-y", "-f", "f32le", "-ar", str(SR), "-ac", "1", "-i", "-", "-c:a", "libvorbis", "-q:a", "4", out],
                   input=x.tobytes(), check=True)


if __name__ == "__main__":
    main(sys.argv[1])
