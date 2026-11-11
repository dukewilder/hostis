#!/usr/bin/env python3
# The Numbers: HOSTIS's numbers station, built as a Lantern seed folder.
#
#   the_numbers.py <voice.onnx> <out folder>
#
# Every transmission is a real message. Each group of five figures is one word: the first two
# figures name one of Bartholomew Roberts's articles as Charles Johnson printed them in 1724
# (A General History of the Pyrates), the last three the word's place in that article, counting
# words as printed and leaving out figures. roberts-articles.json beside this script holds the
# key as the script counts it. The voice is Piper's LJ Speech model, trained on a public domain
# recording. Everything else here is synthesised: the interval signal is an original phrase.
# No rights reserved.
import json, os, random, subprocess, sys, tempfile, wave
import numpy as np
from scipy.signal import butter, sosfilt

HERE = os.path.dirname(os.path.abspath(__file__))
VOICE, OUT = sys.argv[1], sys.argv[2]
SR = 22050
rng = np.random.default_rng(1724)
random.seed(1724)
os.makedirs(OUT, exist_ok=True)

ARTICLES = json.load(open(f"{HERE}/roberts-articles.json"))
ROMAN = ["I", "II", "III", "IV", "V", "VI", "VII", "VIII", "IX", "X", "XI"]

# The messages, each made only of words the articles hold.
MESSAGES = [
    "no man has title to the sea",
    "every man has a vote",
    "the company shall share the publick stock",
    "all hands on deck at night",
    "let none desert the ship",
    "every man has equal title to the sea",
    "put the lights out at eight o clock",
    "keep piece pistols and cutlash clean and fit for service",
    "the crew is to come on shore at the island",
    "no man to talk of breaking up",
    "the musicians to have rest on the sabbath day",
    "every man to his quarters",
]


def encode(sentence):
    """Five figures a word: article, then the word's place in it (from one), as printed."""
    groups = []
    for w in sentence.split():
        spots = [(a, i) for a, r in enumerate(ROMAN, 1) for i, x in enumerate(ARTICLES[r], 1) if x.lower() == w]
        if not spots:
            raise SystemExit(f"'{w}' is not in the articles")
        a, i = random.choice(spots)
        groups.append(f"{a:02d}{i:03d}")
    return groups


def decode(groups):
    return " ".join(ARTICLES[ROMAN[int(g[:2]) - 1]][int(g[2:]) - 1].lower() for g in groups)


# --- sound -----------------------------------------------------------------------------------

def say(text):
    with tempfile.NamedTemporaryFile(suffix=".wav", delete=False) as f:
        path = f.name
    subprocess.run([sys.executable, "-m", "piper", "-m", VOICE, "-f", path, "--length-scale", "1.08", "--noise-scale", "0.4", "--noise-w-scale", "0.5"],
                   input=text.encode(), check=True, capture_output=True)
    with wave.open(path) as w:
        sr = w.getframerate()
        x = np.frombuffer(w.readframes(w.getnframes()), dtype=np.int16).astype(np.float32) / 32768
    os.unlink(path)
    assert sr == SR, sr
    # Trim the silence the model leaves either side.
    loud = np.where(np.abs(x) > 0.02)[0]
    return x[max(0, loud[0] - 200): loud[-1] + 400] if len(loud) else x


_said = {}
def word(w):
    if w not in _said:
        _said[w] = say(w)
    return _said[w]


def silence(s):
    return np.zeros(int(s * SR), np.float32)


def tine(freq, dur=1.6):
    """One note of a small music box: a struck tine, slightly out of tune with itself."""
    t = np.arange(int(dur * SR)) / SR
    env = np.exp(-t * 3.2) * np.minimum(1, t / 0.003)
    x = (np.sin(2 * np.pi * freq * t) + 0.32 * np.sin(2 * np.pi * freq * 2.01 * t) * np.exp(-t * 6)
         + 0.12 * np.sin(2 * np.pi * freq * 3.03 * t) * np.exp(-t * 9) + 0.05 * np.sin(2 * np.pi * freq * 4.27 * t) * np.exp(-t * 14))
    return (x * env * 0.32).astype(np.float32)


A4 = 440.0
def note(semis):
    return A4 * 2 ** (semis / 12)

# The interval signal: an original phrase in A minor, eight notes and a rest.
PHRASE = [(0, 0.42), (3, 0.42), (7, 0.42), (5, 0.84), (3, 0.42), (2, 0.42), (-5, 0.84), (None, 0.6)]


def phrase(transpose=0, tempo=1.0):
    total = sum(d for _, d in PHRASE) / tempo
    out = np.zeros(int((total + 1.8) * SR), np.float32)
    at = 0.0
    for n, d in PHRASE:
        if n is not None:
            x = tine(note(n + transpose))
            i = int(at * SR)
            out[i:i + len(x)] += x
        at += d / tempo
    return out[:int(total * SR)]


def interval(seconds, transpose=0, tempo=1.0):
    p = phrase(transpose, tempo)
    n = int(np.ceil(seconds * SR / len(p))) + 1
    x = np.concatenate([p] * n)[:int(seconds * SR)]
    # A tape that has been played too often: a slow wobble in pitch.
    t = np.arange(len(x)) / SR
    idx = np.clip(np.arange(len(x)) + 18 * np.sin(2 * np.pi * 0.37 * t), 0, len(x) - 1)
    return np.interp(idx, np.arange(len(x)), x).astype(np.float32)


def band(x, lo, hi):
    return sosfilt(butter(4, [lo, hi], btype="band", fs=SR, output="sos"), x).astype(np.float32)


def air(x, level=0.035, whistle=True):
    """Shortwave: band-limited hiss, the slow fade of the ionosphere, a faint heterodyne."""
    n = len(x)
    t = np.arange(n) / SR
    hiss = band(rng.standard_normal(n).astype(np.float32), 250, 3600) * level
    fade = 0.72 + 0.28 * np.sin(2 * np.pi * (0.07 * t + 0.3 * np.sin(2 * np.pi * 0.011 * t)))
    y = band(x, 280, 3300) * fade + hiss * (1.25 - fade * 0.5)
    if whistle:
        f = 1180 + 40 * np.sin(2 * np.pi * 0.003 * t)
        y += 0.006 * np.sin(2 * np.pi * np.cumsum(f) / SR).astype(np.float32)
    y = np.tanh(y * 1.6) / 1.6
    return y.astype(np.float32)


def write(path, x, fade_in=1.5, fade_out=2.5):
    x = x.copy()
    a, b = int(fade_in * SR), int(fade_out * SR)
    x[:a] *= np.linspace(0, 1, a)
    x[-b:] *= np.linspace(1, 0, b)
    x = x / max(1e-6, np.max(np.abs(x))) * 0.89
    pcm = (x * 32767).astype(np.int16)
    with tempfile.NamedTemporaryFile(suffix=".wav", delete=False) as f:
        tmp = f.name
    with wave.open(tmp, "wb") as w:
        w.setnchannels(1); w.setsampwidth(2); w.setframerate(SR); w.writeframes(pcm.tobytes())
    subprocess.run(["ffmpeg", "-y", "-loglevel", "error", "-i", tmp, "-c:a", "libvorbis", "-q:a", "2", path], check=True)
    os.unlink(tmp)


DIGITS = ["zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine"]


def figures(s, gap=0.22):
    parts = []
    for ch in s:
        parts += [word(DIGITS[int(ch)]), silence(gap)]
    return np.concatenate(parts)


def transmission(n, groups):
    """Interval signal, the call, the groups twice each, the end, the signal again."""
    call = f"{n:03d}"
    voice = [silence(0.8)]
    for _ in range(2):
        voice += [word("Attention."), silence(0.9)]
    for _ in range(3):
        voice += [figures(call), silence(0.9)]
    voice += [word("Groups."), silence(0.4), figures(f"{len(groups):02d}"), silence(1.6)]
    for g in groups:
        voice += [figures(g), silence(0.9), figures(g), silence(1.8)]
    voice += [word("End of message."), silence(0.7), word("End."), silence(1.2)]
    v = np.concatenate(voice)
    head = interval(42, 0)
    tail = interval(16, 0)
    # The signal ducks under the voice rather than stopping, as a tape left running would.
    bed = interval(len(v) / SR, 0) * 0.16
    body = v * 0.95 + bed[:len(v)]
    return air(np.concatenate([head, body, tail]))


def main():
    notes = []
    for n, m in enumerate(MESSAGES, 1):
        g = encode(m)
        assert decode(g) == m, (decode(g), m)
        write(f"{OUT}/HOSTIS - Transmission {n:03d}.ogg", transmission(n, g))
        notes.append({"n": n, "groups": g, "words": m})
        print(f"transmission {n:03d}: {' '.join(g)}")
    for i, (tr, tempo, title) in enumerate([(0, 1.0, "I"), (-2, 0.92, "II"), (3, 1.06, "III")]):
        write(f"{OUT}/HOSTIS - Interval Signal {title}.ogg", air(interval(360, tr, tempo), level=0.04 + 0.006 * i))
    bell = np.concatenate([silence(0.6), word("This is The Numbers."), silence(0.5), word("Five figures, on the hour."), silence(0.8)])
    write(f"{OUT}/bell.ogg", air(bell * 0.95 + interval(len(bell) / SR + 0.01, 0)[:len(bell)] * 0.18), fade_in=0.3, fade_out=0.8)
    src = "Synthesised for HOSTIS. Voice: Piper, LJ Speech model (public domain recording). No rights reserved."
    for f in os.listdir(OUT):
        if f.endswith(".ogg") and f != "bell.ogg":
            open(f"{OUT}/{f[:-4]}.source.txt", "w").write(src + "\n")
    json.dump(notes, open(f"{HERE}/numbers-key.json", "w"), indent=1)
    slots = [{"title": "Transmission", "about": "Five figures, each group twice. Every group is a word.",
              "span": {"days": 127, "start_min": h * 60, "len_min": 5},
              "fill": {"kind": "tracks", "tracks": [f"Transmission {h % 12 + 1:03d}"], "shuffle": False}, "live": False} for h in range(24)]
    json.dump({"utc_offset_min": 0, "zone_name": "UTC", "mode": {"kind": "shuffle"}, "hours": [], "slots": slots, "ident_every_min": 15}, open(f"{OUT}/programme.json", "w"), indent=1)
    open(f"{OUT}/notices.txt", "w").write(
        "Five figures on the hour, every hour, UTC. Each group of five is one word: the first two figures name an article, "
        "the last three the word's place in it. The key is the Articles of Bartholomew Roberts, as Charles Johnson printed them in 1724, "
        "counting words as printed and leaving out figures.\n")


if __name__ == "__main__":
    main()
