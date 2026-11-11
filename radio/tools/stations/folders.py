#!/usr/bin/env python3
# The seed folders of HOSTIS's archive-built stations, less their recordings (fetch.py brings
# those): the bell, the programme and the notices.
#   folders.py <voice.onnx> <stations folder>      (writes <stations>/poets-corner, netlabel)
# The bells are spoken by Piper's LJ Speech model (a public domain recording) over a struck chord.
# No rights reserved.
import json, os, subprocess, sys, tempfile, wave
import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
VOICE, ROOT = sys.argv[1], sys.argv[2]
SR = 22050


def say(text):
    with tempfile.NamedTemporaryFile(suffix=".wav", delete=False) as f:
        path = f.name
    subprocess.run([sys.executable, "-m", "piper", "-m", VOICE, "-f", path, "--length-scale", "1.05", "--noise-scale", "0.4"], input=text.encode(), check=True, capture_output=True)
    with wave.open(path) as w:
        x = np.frombuffer(w.readframes(w.getnframes()), dtype=np.int16).astype(np.float32) / 32768
    os.unlink(path)
    loud = np.where(np.abs(x) > 0.02)[0]
    return x[max(0, loud[0] - 200): loud[-1] + 400]


def chord(freqs, dur=4.0):
    t = np.arange(int(dur * SR)) / SR
    x = np.zeros_like(t)
    for i, f in enumerate(freqs):
        d = t - i * 0.07
        on = d >= 0
        x[on] += (np.sin(2 * np.pi * f * d[on]) + 0.25 * np.sin(2 * np.pi * 2.01 * f * d[on]) * np.exp(-d[on] * 5)) * np.exp(-d[on] * 1.4)
    return (x * 0.12).astype(np.float32)


def bell(path, lines, freqs):
    v = np.concatenate([np.zeros(int(0.9 * SR), np.float32)] + sum(([say(l), np.zeros(int(0.45 * SR), np.float32)] for l in lines), []))
    c = chord(freqs, len(v) / SR + 2.5)
    x = c.copy()
    x[:len(v)] += v
    x = x / np.max(np.abs(x)) * 0.89
    x[-int(0.6 * SR):] *= np.linspace(1, 0, int(0.6 * SR))
    with tempfile.NamedTemporaryFile(suffix=".wav", delete=False) as f:
        tmp = f.name
    with wave.open(tmp, "wb") as w:
        w.setnchannels(1); w.setsampwidth(2); w.setframerate(SR); w.writeframes((x * 32767).astype(np.int16).tobytes())
    subprocess.run(["ffmpeg", "-y", "-loglevel", "error", "-i", tmp, "-c:a", "libvorbis", "-q:a", "3", path], check=True)
    os.unlink(tmp)


# Poems played at the top of every hour, one drawn each time.
HOUR = ["Ozymandias", "The Tyger", "Dover Beach", "The Road Not Taken", "Invictus", "Daffodils", "Kublai Khan", "Annabel Lee",
        "Jabberwocky", "The Lake Isle of Innisfree", "Because I Could Not Stop For Death", "Fire and Ice", "Dulce et Decorum",
        "When You See Millions Of The Mouthless Dead", "To His Coy Mistress", "My Last Duchess", "Batter my heart, three-person'd God",
        "O Captain! My Captain!", "Fog", "War is Kind", "When We Two Parted", "As Kingfishers Catch Fire", "A Poison Tree", "The Walrus and the Carpenter"]


def main():
    poems = {x["title"] for x in json.load(open(f"{HERE}/poets.json", encoding="utf-8"))}
    missing = [t for t in HOUR if t not in poems]
    assert not missing, missing
    p = f"{ROOT}/poets-corner"
    os.makedirs(p, exist_ok=True)
    bell(f"{p}/bell.ogg", ["Poets' Corner.", "On Pyrate Radio."], [220.0, 277.18, 329.63, 440.0])
    slots = [{"title": "The Hour's Poem", "about": "A poem everyone should hear once, at the top of the hour.",
              "span": {"days": 127, "start_min": h * 60, "len_min": 10}, "fill": {"kind": "tracks", "tracks": HOUR, "shuffle": True}, "live": False} for h in range(24)]
    json.dump({"utc_offset_min": 0, "zone_name": "UTC", "mode": {"kind": "shuffle"}, "hours": [], "slots": slots, "ident_every_min": 60},
              open(f"{p}/programme.json", "w"), indent=1)
    open(f"{p}/notices.txt", "w").write(
        "Short poems in the public domain, read by LibriVox's volunteers and given to the public domain with them. "
        "On the hour, one of the poems everyone should hear once.\n")
    n = f"{ROOT}/netlabel"
    os.makedirs(n, exist_ok=True)
    bell(f"{n}/bell.ogg", ["Netlabel.", "Free music, on Pyrate Radio."], [146.83, 220.0, 293.66, 349.23])
    json.dump({"utc_offset_min": 0, "zone_name": "UTC", "mode": {"kind": "shuffle"}, "hours": [], "slots": [], "ident_every_min": 60},
              open(f"{n}/programme.json", "w"), indent=1)
    open(f"{n}/notices.txt", "w").write(
        "Electronic, ambient and experimental music released by netlabels under Creative Commons licences, "
        "credited and licensed track by track. Every artist here chose to let their music travel.\n")


if __name__ == "__main__":
    main()
