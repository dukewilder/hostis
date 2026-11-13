#!/usr/bin/env python3
# The seed folders of HOSTIS's stations, less their recordings (fetch.py brings those): the bell,
# the programme and the notices. The Numbers and Shanties keep their own.
#   folders.py <voice.onnx> <stations folder> [station ...]      (every station, or those named)
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
    subprocess.run([sys.executable, "-m", "piper", "-m", VOICE, "-f", path, "--length-scale", "1.12", "--noise-scale", "0.4"], input=text.encode(), check=True, capture_output=True)
    with wave.open(path) as w:
        x = np.frombuffer(w.readframes(w.getnframes()), dtype=np.int16).astype(np.float32) / 32768
    os.unlink(path)
    loud = np.where(np.abs(x) > 0.02)[0]
    return x[max(0, loud[0] - 1500): loud[-1] + 1500]


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


# The canon Poets' Corner plays every night: twenty-four poems everyone should hear once.
CANON = ["Ozymandias", "The Tyger", "Dover Beach", "The Road Not Taken", "Invictus", "Daffodils", "Kubla Khan", "Annabel Lee",
         "Jabberwocky", "The Lake Isle of Innisfree", "Because I Could Not Stop For Death", "Fire and Ice", "Dulce et Decorum Est",
         "When You See Millions Of The Mouthless Dead", "To His Coy Mistress", "My Last Duchess", "Batter my heart, three-person'd God",
         "O Captain! My Captain!", "Fog", "War is Kind", "When We Two Parted", "As Kingfishers Catch Fire", "A Poison Tree", "The Walrus and the Carpenter"]


def chicago():
    """HOSTIS keeps Austin's hours: Central time, with its changes of clock to 2040."""
    import datetime as dt
    from zoneinfo import ZoneInfo
    tz, out, t = ZoneInfo("America/Chicago"), [], dt.datetime(2026, 1, 1, tzinfo=dt.timezone.utc)
    while t.year < 2040:
        off = int(t.astimezone(tz).utcoffset().total_seconds() // 60)
        if not out or out[-1][1] != off:
            out.append([int(t.timestamp() * 1000), off])
        t += dt.timedelta(hours=1)
    return out


def programme(path, mode, slots=()):
    json.dump({"zone_name": "America/Chicago", "zone": chicago(), "mode": {"kind": mode}, "hours": [], "slots": list(slots), "ident_every_min": 60},
              open(path, "w"), indent=1)


def slot(title, about, start_min, len_min, tracks):
    return {"title": title, "about": about, "span": {"days": 127, "start_min": start_min, "len_min": len_min},
            "fill": {"kind": "tracks", "tracks": tracks, "shuffle": True}, "live": False}


def notices(path, *paras):
    open(path, "w").write("\n\n".join(paras) + "\n")


def main():
    only = sys.argv[3:]
    want = lambda s: not only or s in only
    say_as = "On Pirate Radio."  # spelt for the voice; it says the same word
    if want("the-public-domain"):
        d = f"{ROOT}/the-public-domain"
        os.makedirs(d, exist_ok=True)
        bell(f"{d}/bell.ogg", ["The Public Domain.", say_as], [196.0, 246.94, 293.66, 392.0])
        programme(f"{d}/programme.json", "shuffle")
        notices(f"{d}/notices.txt",
                "Every record here was published in 1925 or earlier and is in the public domain in the United States. "
                "The records of 1926 follow on 1 January 2027.")
    if want("readings"):
        d = f"{ROOT}/readings"
        os.makedirs(d, exist_ok=True)
        bell(f"{d}/bell.ogg", ["Readings.", say_as], [174.61, 220.0, 261.63, 349.23])
        programme(f"{d}/programme.json", "in_order")
        notices(f"{d}/notices.txt",
                "Read by LibriVox volunteers, who give their recordings to the public domain. "
                "The books play in order, a chapter at a time, round the clock, and start again each Monday at midnight, Central time.",
                "Most of what is told of Blackbeard, Anne Bonny and Mary Read goes back to one book, "
                "Captain Charles Johnson's General History of the Pyrates, printed in London in 1724.")
    if want("netlabel"):
        from build_pass import NETLABEL
        night = {item for item, _, _, n in NETLABEL if n}
        tracks = [f"{x['by']} - {x['title']}" for x in json.load(open(f"{HERE}/netlabel.json", encoding="utf-8")) if x["item"] in night]
        d = f"{ROOT}/netlabel"
        os.makedirs(d, exist_ok=True)
        bell(f"{d}/bell.ogg", ["Net label.", "Free music. " + say_as], [146.83, 220.0, 293.66, 349.23])
        programme(f"{d}/programme.json", "shuffle", [slot("The Small Hours", "Ambient and drone, eleven at night to seven in the morning.", 23 * 60, 8 * 60, tracks)])
        notices(f"{d}/notices.txt",
                "Released by netlabels under Creative Commons Attribution or Attribution-ShareAlike, or given to the public domain. "
                "Every recording carries its artist, its release and its licence.",
                "From eleven at night to seven in the morning, Central time, the station plays its ambient and drone records alone.")
    if want("poets-corner"):
        poems = {x["title"] for x in json.load(open(f"{HERE}/poets.json", encoding="utf-8"))}
        assert not [t for t in CANON if t not in poems], [t for t in CANON if t not in poems]
        d = f"{ROOT}/poets-corner"
        os.makedirs(d, exist_ok=True)
        bell(f"{d}/bell.ogg", ["Poets' Corner.", say_as], [220.0, 277.18, 329.63, 440.0])
        programme(f"{d}/programme.json", "shuffle", [slot("Poems Everyone Should Hear Once", "Twenty-four of them, nine to ten every night.", 21 * 60, 60, CANON)])
        notices(f"{d}/notices.txt",
                "Every poem here, and every reading of it, is in the public domain. The readers are LibriVox volunteers.",
                "From nine to ten every night, Central time, the twenty-four poems everyone should hear once.")
    if want("contraband"):
        d = f"{ROOT}/contraband"
        os.makedirs(d, exist_ok=True)
        bell(f"{d}/bell.ogg", ["Contraband.", say_as], [130.81, 155.56, 196.0, 261.63])
        programme(f"{d}/programme.json", "in_order")
        notices(f"{d}/notices.txt",
                "Read by LibriVox volunteers, who give their recordings to the public domain. "
                "The books play in order, a chapter at a time, round the clock.",
                "Milton printed Areopagitica in 1644 without the licence it argued against. "
                "Common Sense came out in 1776 with no author's name on it. Candide was condemned in Geneva and Paris the year it appeared.",
                "Douglass sailed for Britain in 1845, months after his Narrative named the men who had owned him. "
                "Thoreau spent a night in the Concord jail for refusing his poll tax. Emma Goldman was deported from the United States in 1919.")


if __name__ == "__main__":
    main()
