#!/usr/bin/env python3
"""Turns a podcast feed into a Lantern seed folder: each new episode lands as
"Show - Episode.mp3" with a .source.txt beside it, and the Lantern puts it on the clock
within ten minutes. For broadcasters bringing their own catalogue aboard.

  feed2folder.py https://example.com/feed.xml /srv/pyrate/stations/myshow [--keep 50]

Only feeds you have the right to rebroadcast belong here: your own show, or one that says so.
No rights reserved.
"""
import argparse, os, re, sys, urllib.request, xml.etree.ElementTree as ET

def safe(s):
    return re.sub(r'\s+', ' ', re.sub(r'[\\/:*?"<>|]+', ' ', s)).strip()[:120]

def main():
    a = argparse.ArgumentParser()
    a.add_argument("feed")
    a.add_argument("folder")
    a.add_argument("--keep", type=int, default=50, help="newest episodes to hold")
    o = a.parse_args()
    os.makedirs(o.folder, exist_ok=True)
    req = urllib.request.Request(o.feed, headers={"User-Agent": "Pyrate Radio feed2folder"})
    root = ET.fromstring(urllib.request.urlopen(req, timeout=60).read())
    channel = root.find("channel")
    show = safe(channel.findtext("title") or "Podcast")
    got = 0
    for item in channel.findall("item")[: o.keep]:
        enc = item.find("enclosure")
        if enc is None or not (enc.get("type", "").startswith("audio") or enc.get("url", "").lower().endswith((".mp3", ".m4a", ".ogg", ".opus"))):
            continue
        url = enc.get("url")
        ext = os.path.splitext(url.split("?")[0])[1] or ".mp3"
        name = f"{show} - {safe(item.findtext('title') or 'Episode')}"
        path = os.path.join(o.folder, name + ext)
        if os.path.exists(path):
            continue
        tmp = path + ".part"
        urllib.request.urlretrieve(url, tmp)
        os.replace(tmp, path)
        with open(os.path.join(o.folder, name + ".source.txt"), "w", encoding="utf-8") as f:
            f.write(f"From the feed of {show}: {item.findtext('link') or o.feed}")
        got += 1
        print("aboard:", name)
    print(f"{got} new episodes")

if __name__ == "__main__":
    main()
