#!/usr/bin/env python3
"""The test network's recordings, made where they are missing: net_music.py [<music dir>]

gang/ (Gangway) gets Harbour Lights and Low Tide, watch/ (Second Watch) Lamplight and Second
Watch, each a cycle of four notes two seconds apiece as browser_sweep.py's TRACKS names them,
and each folder a station.jpg. A folder that already holds a recording is left as it is."""
import os, sys, wave, subprocess
import numpy as np

M = sys.argv[1] if len(sys.argv) > 1 else "/tmp/net"
# Three minutes a recording: long enough that a crowd of tabs (browser_crew.py) is still
# fetching new sound when it is measured, not playing the loop again from what it holds.
SR, SECS = 48000, 184
FOLDERS = {
    "gang": {"Harbour Lights": [220, 277, 330, 262], "Low Tide": [196, 247, 294, 220]},
    "watch": {"Lamplight": [247, 311, 370, 277], "Second Watch": [174, 220, 262, 196]},
}
COLOURS = {"gang": "0x1d3557", "watch": "0x3d2c4e"}

def cycle(path, notes):
    t = np.arange(2 * SR) / SR
    # Each note eased in and out over 10 ms, so a step between notes is no click.
    env = np.minimum(1.0, np.minimum(t, 2.0 - t) / 0.01)
    one = np.concatenate([0.3 * np.sin(2 * np.pi * f * t) * env for f in notes])
    x = np.tile(one, SECS // 8)
    with wave.open(path, "wb") as w:
        w.setnchannels(1); w.setsampwidth(2); w.setframerate(SR); w.writeframes((x * 32767).astype("<i2").tobytes())

for folder, tracks in FOLDERS.items():
    d = os.path.join(M, folder); os.makedirs(d, exist_ok=True)
    if any(f.lower().endswith((".wav", ".mp3", ".flac", ".ogg")) for f in os.listdir(d)):
        continue
    for title, notes in tracks.items():
        cycle(os.path.join(d, f"Test Band - {title}.wav"), notes)
    subprocess.run(["ffmpeg", "-hide_banner", "-loglevel", "error", "-y", "-f", "lavfi", "-i", f"color=c={COLOURS[folder]}:s=600x600",
                    "-frames:v", "1", os.path.join(d, "station.jpg")], check=True)
    print(f"{d}: {', '.join(tracks)}")
