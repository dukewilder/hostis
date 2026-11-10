#!/usr/bin/env python3
"""The HOSTIS bell: a short reading that plays on the hour on a HOSTIS station.

Each day it reads one entry from the Annals, in order, and the day's broadside if one is
given. It writes the script, speaks it with Piper (a voice that runs on the Lantern's own
machine), and drops bell.wav into a station's seed folder. The Lantern sets it on the hour.

  bell.py --site /path/to/hostis/index.html --out /srv/pyrate/stations/hostis/bell.wav \\
          [--broadside today.txt] [--voice /path/to/voice.onnx] [--text-only]

No rights reserved.
"""
import argparse, datetime, html, re, subprocess, sys, tempfile, os

def annals(path):
    page = open(path, encoding="utf-8").read()
    body = page.split('<section id="annals">', 1)[-1].split("</section>", 1)[0]
    out = []
    for m in re.finditer(r'class="entry-date">(.*?)</div>.*?class="entry-title">(.*?)</div>.*?class="entry-desc">(.*?)</div>', body, re.S):
        clean = lambda s: html.unescape(re.sub(r"<[^>]+>", "", s)).strip()
        out.append((clean(m.group(1)), clean(m.group(2)), clean(m.group(3))))
    return out

def script(entries, broadside=None, today=None):
    today = today or datetime.date.today()
    lines = [f"This is the bell. Pyrate Radio, under HOSTIS colours. {today.strftime('%A')}, the {ordinal(today.day)} of {today.strftime('%B')}."]
    if entries:
        date, title, desc = entries[today.toordinal() % len(entries)]
        lines.append(f"From the Annals. {date}. {title}. {desc}")
    if broadside:
        lines.append("The broadside for today. " + broadside.strip())
    lines.append("The Annals are at hostis dot black. No rights reserved.")
    return "\n\n".join(lines)

def ordinal(n):
    return f"{n}{'th' if 11 <= n % 100 <= 13 else {1: 'st', 2: 'nd', 3: 'rd'}.get(n % 10, 'th')}"

def main():
    a = argparse.ArgumentParser()
    a.add_argument("--site", required=True)
    a.add_argument("--out", required=True)
    a.add_argument("--broadside")
    a.add_argument("--voice", default=os.environ.get("PIPER_VOICE", ""))
    a.add_argument("--text-only", action="store_true")
    o = a.parse_args()
    text = script(annals(o.site), open(o.broadside, encoding="utf-8").read() if o.broadside and os.path.exists(o.broadside) else None)
    if o.text_only:
        print(text)
        return
    with tempfile.NamedTemporaryFile("w", suffix=".txt", delete=False, encoding="utf-8") as f:
        f.write(text)
    tmp = o.out + ".part.wav"
    try:
        with open(f.name, encoding="utf-8") as src:
            subprocess.run(["piper", "--model", o.voice, "--output_file", tmp], stdin=src, check=True)
    finally:
        os.remove(f.name)
    os.replace(tmp, o.out)
    print(f"bell written to {o.out}")

if __name__ == "__main__":
    main()
