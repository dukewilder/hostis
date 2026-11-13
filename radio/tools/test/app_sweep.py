#!/usr/bin/env python3
"""The app swept the way people use it, on the computer (Linux) build: every screen timed,
read back from the screen and checked by ear.

  app_sweep.py [--size 460x900] [--out DIR] [--keep] [test ...]

The app runs under Xvfb with a fresh home for each first open, a session bus with a stand-in
file chooser (fake_portal.py), and PulseAudio with a virtual speaker (recorded, so what plays is
checked as the tones it should be) and a virtual microphone fed from files. Screens are read with
tesseract. Needs the private test network (net_up.sh), the browser copies (serve_test.sh) for
the listener's side, and app/build/linux. Prints ok / FAIL / SLOW / note lines and a timings
table; screenshots, scaled down, go to --out (default /tmp/appsweep/out)."""
import sys, os, re, time, json, shutil, signal, tempfile, subprocess, threading, argparse, difflib, wave, base64
import numpy as np
import pytesseract
from PIL import Image, ImageOps

R = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
BUNDLE = f"{R}/app/build/linux/x64/release/bundle/pyrate"
RELAY, GANG = (open("/tmp/net-ids").read().split() + ["", ""])[:2]
OPEN = "http://127.0.0.1:8099/"
SLOW = 1.0
TRACKS = {"Harbour Lights": [220, 277, 330, 262], "Low Tide": [196, 247, 294, 220], "Lamplight": [247, 311, 370, 277], "Second Watch": [174, 220, 262, 196]}

ap = argparse.ArgumentParser()
ap.add_argument("tests", nargs="*")
ap.add_argument("--size", action="append")
ap.add_argument("--out", default="/tmp/appsweep/out")
ap.add_argument("--keep", action="store_true")
A = ap.parse_args() if __name__ == "__main__" else ap.parse_args([])
SIZES = [tuple(map(int, s.split("x"))) for s in (A.size or ["460x900", "1280x800"])]
OUT = A.out; os.makedirs(OUT, exist_ok=True)
BASE = "/tmp/appsweep"

# ---- Reporting ---------------------------------------------------------------------------
RES, TIMES, CUR = [], [], {"t": ""}
def check(ok, what, detail=""):
    s = "ok  " if ok else "FAIL"; line = what + (f" ({detail})" if detail else "")
    RES.append((CUR["t"], s.strip(), line)); print(f"{s} [{CUR['t']}] {line}", flush=True)
    return bool(ok)
def note(what):
    RES.append((CUR["t"], "note", what)); print(f"note [{CUR['t']}] {what}", flush=True)
def timed(what, secs, limit=SLOW):
    TIMES.append((CUR["t"], what, secs, limit))
    if secs is None: return check(False, what + ": never happened")
    if secs > limit: RES.append((CUR["t"], "SLOW", f"{what}: {secs:.2f} s (limit {limit} s)"))
    print(f"{'SLOW' if secs > limit else 'time'} [{CUR['t']}] {what}: {secs:.2f} s", flush=True)
    return secs <= limit
def poll(fn, timeout=30, every=0.1):
    t0 = time.time()
    while True:
        try: v = fn()
        except Exception: v = None
        if v: return v, time.time() - t0
        if time.time() - t0 > timeout: return None, None
        time.sleep(every)

# ---- The machine around the app: screen, bus, file chooser, sound ---------------------------
class Env:
    def __init__(self, w, h, display=":79"):
        self.w, self.h, self.d = w, h, display
        self.procs = []
        self.bus = f"unix:path={BASE}/bus"
        self.pulse = f"unix:{BASE}/pulse/native"
        self.pick, self.plog, self.opened = f"{BASE}/pick.txt", f"{BASE}/portal.log", f"{BASE}/opened.txt"
    def start(self):
        os.makedirs(f"{BASE}/pulse", exist_ok=True); os.makedirs(f"{BASE}/bin", exist_ok=True)
        for f in (self.pick, self.plog, self.opened): open(f, "w").close()
        subprocess.run(["pkill", "-f", f"Xvfb {self.d}"], capture_output=True); time.sleep(0.3)
        self.procs.append(subprocess.Popen(["Xvfb", self.d, "-screen", "0", f"{self.w}x{self.h}x24", "-nolisten", "tcp"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL))
        if os.path.exists(f"{BASE}/bus"): os.remove(f"{BASE}/bus")
        self.procs.append(subprocess.Popen(["dbus-daemon", "--session", f"--address={self.bus}", "--nofork"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL))
        poll(lambda: os.path.exists(f"{BASE}/bus"), 5, 0.05)
        env = dict(os.environ, DBUS_SESSION_BUS_ADDRESS=self.bus, PORTAL_PICK=self.pick, PORTAL_LOG=self.plog)
        self.procs.append(subprocess.Popen(["/usr/bin/python3.12", f"{R}/tools/test/fake_portal.py"], env=env, stdout=subprocess.DEVNULL, stderr=open(f"{BASE}/portal.err", "w")))
        # A desktop that opens whatever it is handed, and says what it was.
        with open(f"{BASE}/bin/xdg-open", "w") as f: f.write(f'#!/bin/sh\necho "$@" >> {self.opened}\n')
        os.chmod(f"{BASE}/bin/xdg-open", 0o755)
        if subprocess.run(["pactl", "info"], env=dict(os.environ, PULSE_SERVER=self.pulse), capture_output=True).returncode != 0:
            with open(f"{BASE}/pulse.pa", "w") as f:
                f.write(f"load-module module-native-protocol-unix socket={BASE}/pulse/native auth-anonymous=1\n"
                        "load-module module-null-sink sink_name=speaker rate=48000 channels=2\n"
                        "load-module module-null-sink sink_name=micfeed rate=48000 channels=1\n"
                        "set-default-sink speaker\nset-default-source micfeed.monitor\n")
            self.procs.append(subprocess.Popen(["pulseaudio", "-n", "--daemonize=no", "--exit-idle-time=-1", "--disallow-exit", "-F", f"{BASE}/pulse.pa"],
                                               env=dict(os.environ, HOME=BASE, PULSE_RUNTIME_PATH=f"{BASE}/pulse"), stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL))
            poll(lambda: subprocess.run(["pactl", "info"], env=dict(os.environ, PULSE_SERVER=self.pulse), capture_output=True).returncode == 0, 8, 0.2)
        time.sleep(0.5)
        return self
    def choose(self, *paths):
        """What the user picks in the next file dialog."""
        with open(self.pick, "w") as f: f.write("\n".join(paths) + "\n")
    def picks(self):
        return [json.loads(l) for l in open(self.plog) if l.strip()]
    def stop(self):
        for p in reversed(self.procs):
            try: p.terminate(); p.wait(3)
            except Exception: p.kill()
        self.procs = []

class App:
    def __init__(self, env, home=None, extra=None):
        self.env, self.home, self.extra = env, home, extra or {}
        self.fresh = home is None
    def start(self, wait=True):
        if self.home is None: self.home = tempfile.mkdtemp(prefix="home-", dir=BASE)
        e = self.env
        env = dict(os.environ, DISPLAY=e.d, HOME=self.home, DBUS_SESSION_BUS_ADDRESS=e.bus, PULSE_SERVER=e.pulse, PATH=f"{BASE}/bin:" + os.environ["PATH"],
                   PYRATE_RELAYS="http://127.0.0.1:3340", PYRATE_BOOTSTRAP=RELAY, PYRATE_LAN_ANY="1", XDG_RUNTIME_DIR=f"{BASE}/pulse", **self.extra)
        env.pop("PYRATE_NULL_AUDIO", None)
        self.log = open(f"{BASE}/app-{int(time.time())}.log", "w")
        self.t0 = time.time()
        self.p = subprocess.Popen([BUNDLE], env=env, stdout=self.log, stderr=subprocess.STDOUT, start_new_session=True)
        # The toplevel the user sees: GTK also makes windows that are never shown.
        win, t = poll(lambda: subprocess.run(["xdotool", "search", "--onlyvisible", "--pid", str(self.p.pid)], env=dict(os.environ, DISPLAY=e.d), capture_output=True, text=True).stdout.split()[-1:], 20, 0.05)
        self.win = win[0] if win else None
        self.t_window = t
        # A window resized before it is mapped keeps the size it was built with: try until it takes.
        for _ in range(20):
            if not self.win: break
            xdo("windowsize", self.win, str(e.w), str(e.h), "windowmove", self.win, "0", "0"); time.sleep(0.25)
            g = xdo("getwindowgeometry", self.win)
            if f"{e.w}x{e.h}" in g: break
        self.geometry = xdo("getwindowgeometry", self.win).split("Geometry:")[-1].strip() if self.win else None
        return self
    def stop(self):
        try: os.killpg(self.p.pid, signal.SIGTERM); self.p.wait(5)
        except Exception:
            try: os.killpg(self.p.pid, signal.SIGKILL)
            except Exception: pass
    def alive(self): return self.p.poll() is None

def xdo(*a):
    return subprocess.run(["xdotool", *a], env=dict(os.environ, DISPLAY=ENV.d), capture_output=True, text=True).stdout

# ---- Seeing and touching -------------------------------------------------------------------
def grab():
    p = f"{BASE}/g.png"; subprocess.run(["import", "-display", ENV.d, "-window", "root", p], check=True)
    return Image.open(p).convert("RGB")
def read(img=None, box=None, scale=2):
    """The screen's text, line by line, each with its box on the screen."""
    img = img or grab(); x0, y0 = (box[0], box[1]) if box else (0, 0)
    im = img.crop(box) if box else img
    g = ImageOps.invert(im.convert("L").resize((im.width * scale, im.height * scale), Image.LANCZOS))
    d = pytesseract.image_to_data(g, output_type=pytesseract.Output.DICT, config="--psm 11")
    words = []
    for i, t in enumerate(d["text"]):
        if t.strip() and float(d["conf"][i]) > 20:
            words.append((t, x0 + d["left"][i] / scale, y0 + d["top"][i] / scale, x0 + (d["left"][i] + d["width"][i]) / scale, y0 + (d["top"][i] + d["height"][i]) / scale))
    # Lines: words whose middles sit within a few pixels of one another, left to right.
    words.sort(key=lambda w: ((w[2] + w[4]) / 2, w[1])); lines = []
    for w in words:
        mid = (w[2] + w[4]) / 2
        if lines and abs(lines[-1]["mid"] - mid) < 7: lines[-1]["w"].append(w)
        else: lines.append({"mid": mid, "w": [w]})
    out = []
    for l in lines:
        ws = sorted(l["w"], key=lambda w: w[1])
        out.append({"text": " ".join(w[0] for w in ws), "words": ws, "box": (min(w[1] for w in ws), min(w[2] for w in ws), max(w[3] for w in ws), max(w[4] for w in ws))})
    return out
def norm(t): return re.sub(r"[^a-z0-9: ]+", "", t.lower().replace("·", " ")).strip()
def finds(phrase, lines=None, box=None, cut=0.78):
    """Every place a phrase sits on the screen, top to bottom: the centres of the words that
    read most like it, one per line."""
    lines = lines if lines is not None else read(box=box); want = norm(phrase); out = []
    for l in lines:
        ws = l["words"]; best = (0, None)
        for i in range(len(ws)):
            for j in range(i + 1, min(len(ws), i + len(want.split()) + 2) + 1):
                got = norm(" ".join(w[0] for w in ws[i:j]))
                r = difflib.SequenceMatcher(None, got, want).ratio()
                if r > best[0]: best = (r, ((ws[i][1] + ws[j - 1][3]) / 2, (min(w[2] for w in ws[i:j]) + max(w[4] for w in ws[i:j])) / 2))
        if best[0] >= cut: out.append((best[0], best[1]))
    return [p for _, p in sorted(out, key=lambda o: o[1][1])]
def find(phrase, lines=None, box=None, cut=0.78, low=False):
    """Where a phrase sits: the best match, or with `low` the lowest on the screen."""
    lines = lines if lines is not None else read(box=box)
    if low:
        f = finds(phrase, lines, cut=cut); return f[-1] if f else None
    want = norm(phrase); best = (0, None)
    for l in lines:
        ws = l["words"]
        for i in range(len(ws)):
            for j in range(i + 1, min(len(ws), i + len(want.split()) + 2) + 1):
                got = norm(" ".join(w[0] for w in ws[i:j]))
                r = difflib.SequenceMatcher(None, got, want).ratio()
                if r > best[0]: best = (r, ((ws[i][1] + ws[j - 1][3]) / 2, (min(w[2] for w in ws[i:j]) + max(w[4] for w in ws[i:j])) / 2))
    return best[1] if best[0] >= cut else None
def dump(box=None):
    for l in read(box=box): print(f"  y{int(l['box'][1]):4d} x{int(l['box'][0]):4d} {l['text'][:70]}")
def back():
    """The arrow at the top left of a page pushed over the tabs."""
    top = [l for l in read(box=(0, 0, ENV.w, 60))]
    x = (top[0]["box"][0] - 55) if top and top[0]["box"][0] > 60 else 26
    tap(max(12, x), 27)
def text(box=None): return " | ".join(l["text"] for l in read(box=box))
def has(phrase, box=None, cut=0.78): return find(phrase, box=box, cut=cut) is not None
def tap(x, y): xdo("mousemove", str(int(x)), str(int(y)), "click", "1")
def tap_text(phrase, timeout=6, box=None, cut=0.78, low=False):
    at, t = poll(lambda: find(phrase, box=box, cut=cut, low=low), timeout, 0.2)
    if at: tap(*at)
    return at
def wait_text(phrase, timeout=20, box=None, cut=0.78, every=0.15):
    return poll(lambda: find(phrase, box=box, cut=cut), timeout, every)[1]
def gone_text(phrase, timeout=20, box=None):
    return poll(lambda: not has(phrase, box=box), timeout, 0.15)[1]
def key(*k): xdo("key", *k)
def typed(s): xdo("type", "--delay", "20", s)
def drag(x0, y0, x1, y1, steps=12, hold=0.0):
    xdo("mousemove", str(x0), str(y0), "mousedown", "1"); time.sleep(hold)
    for i in range(1, steps + 1): xdo("mousemove", str(int(x0 + (x1 - x0) * i / steps)), str(int(y0 + (y1 - y0) * i / steps))); time.sleep(0.015)
    xdo("mouseup", "1")
def wheel(x, y, n=5, down=True):
    xdo("mousemove", str(int(x)), str(int(y)))
    for _ in range(n): xdo("click", "5" if down else "4"); time.sleep(0.03)
def shot(name, img=None):
    img = img or grab(); mw = 460 if img.width <= 480 else 640
    if img.width > mw: img = img.resize((mw, int(img.height * mw / img.width)), Image.LANCZOS)
    img.save(f"{OUT}/{name}.jpg", quality=72)
def clipboard():
    """What the app put on the X clipboard, read through GTK as a desktop would."""
    code = "import gi; gi.require_version('Gtk','3.0'); from gi.repository import Gtk, Gdk; c = Gtk.Clipboard.get(Gdk.SELECTION_CLIPBOARD); t = c.wait_for_text(); print(t or '', end='')"
    return subprocess.run(["/usr/bin/python3.12", "-c", code], env=dict(os.environ, DISPLAY=ENV.d), capture_output=True, text=True, timeout=10).stdout

# ---- Hearing: the virtual speaker, recorded all the time -----------------------------------
SR = 24000
class Ear:
    def __init__(self, env):
        self.p = subprocess.Popen(["parec", "-d", "speaker.monitor", "--raw", "--format=s16le", f"--rate={SR}", "--channels=2", "--latency-msec=20"],
                                  env=dict(os.environ, PULSE_SERVER=env.pulse), stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
        self.buf, self.t0, self.lock = [], None, threading.Lock()
        threading.Thread(target=self.run, daemon=True).start()
        poll(lambda: self.t0, 5, 0.05)
    def run(self):
        while True:
            b = self.p.stdout.read(SR // 50 * 4)
            if not b: return
            now = time.time()
            x = np.frombuffer(b, dtype="<i2").astype(np.float32).reshape(-1, 2) / 32767
            with self.lock:
                if self.t0 is None: self.t0 = now - len(x) / SR
                self.buf.append(x)
                if len(self.buf) > 50 * 60 * 6: self.buf = self.buf[-50 * 60 * 5:]; self.t0 = None
    def now_samples(self):
        with self.lock: return np.concatenate(self.buf) if self.buf else np.zeros((0, 2), np.float32), self.t0
    def first_sound(self, since, timeout=60, thr=0.01):
        """When sound first came out of the speaker after `since` (wall time), or None."""
        def look():
            x, t0 = self.now_samples()
            if t0 is None or not len(x): return None
            i = max(0, int((since - t0) * SR)); seg = np.abs(x[i:]).max(axis=1)
            hit = np.argmax(seg > thr) if (seg > thr).any() else -1
            return t0 + (i + hit) / SR if hit >= 0 else None
        at, _ = poll(look, timeout, 0.05)
        return None if at is None else at - since
    def take(self, secs):
        """The next `secs` seconds from the speaker."""
        start = time.time(); time.sleep(secs + 0.1)
        x, t0 = self.now_samples(); i = max(0, int((start - t0) * SR))
        return x[i:i + int(secs * SR)]
    def stop(self): self.p.terminate()

def describe(x):
    x = np.asarray(x, np.float32)
    if x.ndim == 2: x = x.mean(axis=1)
    if len(x) < SR // 4: return {"rms": 0.0, "tonal": 0.0, "hi": 0.0}
    sp = np.abs(np.fft.rfft(x * np.hanning(len(x)))) ** 2; f = np.fft.rfftfreq(len(x), 1 / SR); band = (f > 50) & (f < 8000); tot = sp[band].sum() + 1e-15
    s2 = sp.copy(); s2[~band] = 0; w = max(1, int(6 / (f[1] - f[0]))); got = 0.0
    for _ in range(10):
        i = int(np.argmax(s2)); got += s2[max(0, i - w):i + w + 1].sum(); s2[max(0, i - 3 * w):i + 3 * w + 1] = 0
    return {"rms": float(np.sqrt(np.mean(x ** 2))), "tonal": float(got / tot), "hi": float(sp[f > 4000].sum() / (sp[f > 30].sum() + 1e-15))}
def peak(x):
    x = np.asarray(x, np.float32); x = x.mean(axis=1) if x.ndim == 2 else x
    n = len(x); sp = np.abs(np.fft.rfft(x * np.hanning(n), 8 * n)); f = np.fft.rfftfreq(8 * n, 1 / SR); sp[(f < 60) | (f > 8000)] = 0
    return float(f[int(np.argmax(sp))])
def notes(x, win=0.4, hop=0.2):
    x = np.asarray(x, np.float32); x = x.mean(axis=1) if x.ndim == 2 else x
    n, h = int(SR * win), int(SR * hop)
    return [(i / SR + win / 2, peak(x[i:i + n]) if np.sqrt(np.mean(x[i:i + n] ** 2)) > 0.003 else 0.0) for i in range(0, len(x) - n + 1, h)]
def hear(x, what, title=None, const=None):
    d = describe(x)
    if d["rms"] < 0.004: return check(False, f"{what}: sound comes out of the speaker", f"rms {d['rms']:.4f}")
    ok = check(d["tonal"] > 0.75 and d["hi"] < 0.03, f"{what}: a clean tone, not noise", f"tonal {d['tonal']:.2f}, above 4 kHz {d['hi'] * 100:.2f}%, rms {d['rms']:.3f}")
    ns = [f for _, f in notes(x) if f]
    if const:
        share = sum(abs(f - const) < 5 for f in ns) / max(1, len(ns))
        ok &= check(share > 0.9, f"{what}: it is the {const:.0f} Hz recording", f"{share * 100:.0f}% of windows, median {np.median(ns) if ns else 0:.0f} Hz")
    elif title in TRACKS:
        seq = set(TRACKS[title]); share = sum(any(abs(f - s) < 7 for s in seq) for f in ns) / max(1, len(ns))
        ok &= check(share > 0.85, f"{what}: the notes are {title!r}'s", f"{share * 100:.0f}% of windows")
    return ok

ENV = None

# ---- What the radio's face says ------------------------------------------------------------
FREQ = re.compile(r"\b(8[7-9]|9\d|10[0-8])[.,]\d{3}\b")
def on_set(lines=None):
    """(station name, frequency line) as the radio's face shows them."""
    lines = lines if lines is not None else read()
    for i, l in enumerate(lines):
        if FREQ.search(l["text"]) and i > 0: return lines[i - 1]["text"], FREQ.search(l["text"]).group(0)
    return None, None
def playing(lines=None):
    """(title, position s, duration s) from the line under the title, or Nones."""
    lines = lines if lines is not None else read()
    for i, l in enumerate(lines):
        m = re.search(r"(\d+):(\d\d)\s*[/|]\s*(\d+):(\d\d)", l["text"])
        if m and i > 0: return lines[i - 1]["text"], int(m[1]) * 60 + int(m[2]), int(m[3]) * 60 + int(m[4])
    return None, None, None
def title_like(t):
    """The test network's track a misread title most resembles."""
    best = max(TRACKS, key=lambda k: difflib.SequenceMatcher(None, norm(k), norm(t or "")).ratio())
    return best if difflib.SequenceMatcher(None, norm(best), norm(t or "")).ratio() > 0.6 else None
def nav(tab):
    """The bar at the foot of the window: Radio, Stations, Broadcast, Settings."""
    return tap_text(tab.upper(), box=(0, ENV.h - 70, ENV.w, ENV.h))
def quiet_after(ear, since, timeout=10, thr=0.004):
    """When the speaker went quiet (0.3 s under thr) after `since`."""
    def look():
        x, t0 = ear.now_samples(); i = max(0, int((since - t0) * SR)); seg = np.abs(x[i:]).max(axis=1)
        w = int(0.3 * SR)
        for j in range(0, len(seg) - w, int(0.05 * SR)):
            if seg[j:j + w].max() < thr: return t0 + (i + j) / SR
        return None
    at, _ = poll(look, timeout, 0.1)
    return None if at is None else at - since

def t_listener():
    for w, h in SIZES:
        CUR["t"] = f"listener {w}x{h}"
        global ENV
        ENV = Env(w, h).start(); ear = Ear(ENV)
        try: listener(w, h, ear)
        except Exception as e: check(False, "the flow ran to the end", f"{type(e).__name__}: {str(e)[:300]}")
        finally:
            for a in list(APPS): a.stop(); APPS.remove(a)
            ear.stop(); ENV.stop()
APPS = []
def launch(**kw):
    a = App(ENV, **kw).start(); APPS.append(a); return a

def listener(w, h, ear):
    tag = f"{w}x{h}"
    app = launch()
    timed("first open: the window shows", app.t_window, 0.5)
    _, t = poll(lambda: has("STATIONS ON THE AIR", box=(0, 0, w, 160)), 40, 0.1); timed("first open: '… stations on the air' from launch", time.time() - app.t0 if t is not None else None, 3)
    _, t = poll(lambda: on_set()[0], 40, 0.1); timed("first open: a station's name on the radio, from launch", time.time() - app.t0 if t is not None else None, 3)
    note(f"first open shows {on_set()[0]!r} before anything plays; the line reads {text(box=(0, 0, w, 160))!r}")
    shot(f"listener-{tag}-1-open")
    fs = ear.first_sound(app.t0, 60); timed("first open: first sound, the app playing by itself (from launch)", fs, 5)
    time.sleep(1.5); lines = read(); name, freq = on_set(lines); title, pos, dur = playing(lines)
    note(f"first open plays {name!r} ({freq}), {title!r} {pos}/{dur} s")
    hear(ear.take(5), "first open", title=title_like(title))
    shot(f"listener-{tag}-2-playing")
    # Stop, then Listen.
    t0 = time.time(); at = tap_text("Stop", box=(0, h * 0.6, w, h)); check(at, "a Stop button while playing")
    timed("Stop: the speaker goes quiet", quiet_after(ear, t0), 0.6)
    _, t = poll(lambda: has("Listen", box=(0, h * 0.6, w, h)), 5, 0.05); timed("Stop: the button says Listen", t, 0.5)
    time.sleep(2); d = describe(ear.take(2)); check(d["rms"] < 0.002, "after Stop nothing plays", f"rms {d['rms']:.4f}")
    t0 = time.time(); tap_text("Listen", box=(0, h * 0.6, w, h)); timed("Listen to first sound", ear.first_sound(t0, 30))
    # Swipe to the next station, and back.
    time.sleep(2); before = on_set(); ys = [l["box"][1] for l in read() if FREQ.search(l["text"])]
    y = int(ys[0] - 120) if ys else int(h * 0.28)
    t0 = time.time(); drag(int(w * 0.8), y, int(w * 0.2), y, steps=30)
    _, t = poll(lambda: on_set()[1] not in (None, before[1]), 6, 0.1)
    ways = {"a mouse drag": t is not None}
    if t is None:
        for how, act in (("the Right arrow key", lambda: (tap(w // 2, y), key("Right"))), ("the scroll wheel", lambda: wheel(w // 2, y, 3)), ("a sideways scroll", lambda: xdo("mousemove", str(w // 2), str(y), "click", "7"))):
            act(); time.sleep(1.5); ways[how] = on_set()[1] not in (None, before[1])
            if ways[how]: break
    check(any(ways.values()), "a computer can turn the radio's dial (the hint says 'swipe for the next station')", ", ".join(f"{k}: {'yes' if v else 'no'}" for k, v in ways.items()))
    if t is not None: timed("swipe: the next station on the radio", t, 1)
    q = quiet_after(ear, t0, 5); fs = ear.first_sound(t0 + (q or 0) + 0.05, 30)
    timed("swipe: to the next station's sound", (q or 0) + 0.05 + fs if fs is not None else None)
    time.sleep(1.5); after = on_set(); title = playing()[0]; note(f"swiped from {before} to {after}, playing {title!r}")
    if title: hear(ear.take(4), "after a swipe", title=title_like(title))
    shot(f"listener-{tag}-3-swiped")
    # Hold to scan, then tap to keep the station found.
    t0 = time.time(); drag(w // 2, y, w // 2 + 1, y, steps=2, hold=1.2)
    _, t = poll(lambda: has("SCANNING", box=(0, h * 0.6, w, h)), 5, 0.1)
    check(t is not None, "holding the dial starts a scan", text(box=(0, int(h * 0.6), w, h))[:120])
    seen = set()
    for _ in range(12): seen.add(on_set()[0]); time.sleep(0.6)
    note(f"scanning passed {len(seen - {None})} stations in 7 s: {sorted(s for s in seen if s)[:8]}")
    tap_text("SCANNING", box=(0, h * 0.6, w, h)); time.sleep(1); dead = has("SCANNING", box=(0, h * 0.6, w, h))
    check(not dead, "tapping 'SCANNING · TAP TO HOLD' holds the scan", "the caption ignores taps" if dead else "")
    tap(w // 2, y); time.sleep(1); check(not has("SCANNING", box=(0, h * 0.6, w, h)), "a tap on the station face holds the scan", on_set()[0])
    # The Stations list: tap Gangway.
    nav("Stations"); time.sleep(1); shot(f"listener-{tag}-4-stations")
    t0 = time.time(); tap_text("Find a station"); typed("Gangway"); _, t = poll(lambda: len(finds("Gangway", cut=0.7)) >= 2, 5, 0.1)
    timed("search: typing a name finds it in the list", time.time() - t0 if t is not None else None)
    t0 = time.time(); tap_text("Gangway", low=True, cut=0.7)
    q = quiet_after(ear, t0, 4); fs = ear.first_sound(t0 + (q or 0) + 0.05, 30)
    timed("the list: tap Gangway to its sound", (q or 0) + 0.05 + fs if fs is not None else None)
    time.sleep(1); onradio = has("Listen", box=(0, h * 0.6, w, h)) or has("Stop", box=(0, h * 0.6, w, h))
    note(f"after tapping a row the app shows {'the radio' if onradio else 'the list still'}")
    if not onradio: nav("Radio"); time.sleep(1)
    name, freq = on_set(); title = playing()[0]; check(name and "gangway" in norm(name), "Gangway is on the radio", f"{name!r} {title!r}")
    hear(ear.take(5), "Gangway from the list", title=title_like(title))
    APP_STATE.update(app=app)
    station_page(w, h, tag, ear)
    more_sheet(w, h, tag, ear)
    keep_hide(w, h, tag, ear)
    settings(w, h, tag, ear)
    # Back again tomorrow: the same home, a new start, left playing Gangway.
    nav("Stations"); time.sleep(1); f = find("Find a station") or find("Gangway", cut=0.7)
    if f: tap(*f); key("ctrl+a"); typed("Gangway"); time.sleep(1); tap_text("Gangway", low=True, cut=0.7)
    ear.first_sound(time.time(), 20); nav("Radio"); time.sleep(1.5)
    was = on_set()[0]; home = app.home; app.stop(); APPS.remove(app); time.sleep(1)
    app = launch(home=home)
    _, t = poll(lambda: on_set()[0], 30, 0.1); timed("returning: a station's name on the radio, from launch", time.time() - app.t0 if t is not None else None)
    fs = ear.first_sound(app.t0, 30); timed("returning: first sound, back where it was left (from launch)", fs, 3)
    note(f"returning opens on {on_set()[0]!r} (left on {was!r})"); shot(f"listener-{tag}-9-returning")
APP_STATE = {}

def read_band(y0, y1, thr=90, x0=0, x1=None):
    """One line of small capitals beside icons, read as a line: [(word, x centre, y centre)]."""
    x1 = x1 or ENV.w; img = grab().crop((x0, int(y0), x1, int(y1))).convert("L")
    b = img.point(lambda v: 255 if v > thr else 0); g = ImageOps.invert(b.resize((b.width * 3, b.height * 3), Image.NEAREST))
    d = pytesseract.image_to_data(g, output_type=pytesseract.Output.DICT, config="--psm 7")
    return [(t, x0 + (d["left"][i] + d["width"][i] / 2) / 3, y0 + (d["top"][i] + d["height"][i] / 2) / 3) for i, t in enumerate(d["text"]) if t.strip()]
def press_caps(label, near=None):
    """Taps a small-capitals button whose label OCR misreads in a full read: the band around
    the best guess (or `near`, a phrase on the same line) is read as one line."""
    at = near and find(near, cut=0.6) or find(label, cut=0.6, low=True)
    if not at: return None
    for word, x, y in read_band(at[1] - 16, at[1] + 16):
        if difflib.SequenceMatcher(None, word.upper(), label.upper().split()[0]).ratio() > 0.7: tap(x, y); return (x, y)
    tap(*at); return at
def into(field, words):
    """Types into the text field under a label or placeholder."""
    at = find(field, cut=0.7)
    if not at: return False
    tap(at[0], at[1]); time.sleep(0.3); typed(words); return True
def face(button):
    """Taps Station page, Share or More under the station's name on the radio."""
    fl = [l for l in read() if FREQ.search(l["text"])]
    if not fl: return None
    y = fl[0]["box"][3] + 4
    for word, x, yy in read_band(y, y + 44):
        if difflib.SequenceMatcher(None, word.upper(), button.upper().split()[0]).ratio() > 0.7: tap(x, yy); return (x, yy)
    return None
def col():
    """The app's column: a radio's width in the middle of a wide window."""
    l = max(0, (ENV.w - 620) // 2); return l, min(ENV.w, l + 620)
def switch_on(y):
    """Whether the switch on the row at height y is on: an on switch's knob is bone-bright,
    an off one's mid-grey."""
    img = grab().convert("L"); l, r = col()
    strip = np.asarray(img.crop((r - 90, int(y) - 12, r - 4, int(y) + 28)), np.float32)
    return int((strip > 200).sum()) > 15
def setting_row(label):
    at = find(label, cut=0.75)
    return at
def flip(label):
    """Taps a setting's row; returns (y, state before, state after)."""
    at = find(label, cut=0.75)
    if not at: return None, None, None
    before = switch_on(at[1] + 4); tap(at[0], at[1]); time.sleep(0.8)
    at2 = find(label, cut=0.75) or at
    return at2[1], before, switch_on(at2[1] + 4)
def scroll_to(label, down=True, tries=8):
    for _ in range(tries):
        if has(label, cut=0.75): return True
        l, r = col(); wheel((l + r) // 2, ENV.h // 2, 5, down); time.sleep(0.5)
    return has(label, cut=0.75)

def station_page(w, h, tag, ear):
    nav("Radio"); time.sleep(0.8)
    t0 = time.time(); face("STATION PAGE")
    _, t = poll(lambda: has("UP NEXT") or has("SCHEDULE", box=(0, 200, w, h)), 8, 0.1); timed("the station page opens with its schedule", t)
    lines = read(); body = " | ".join(l["text"] for l in lines)
    for part in ("ON NOW", "UP NEXT", "CHAT", "SCHEDULE"): check(find(part, lines, cut=0.8), f"the station page shows {part}")
    note(f"station page buttons: {[l['text'] for l in lines if 'LISTEN' in l['text'].upper() or 'POSTER' in l['text'].upper()]} while the station is playing")
    note("the station page shows no library: " + str(not any("RECORDING" in l["text"].upper() for l in lines)))
    shot(f"listener-{tag}-5-page")
    key("Escape"); time.sleep(0.6); still = has("UP NEXT"); note(f"Esc {'does not close' if still else 'closes'} the station page")
    if still: back(); time.sleep(0.8)
    check(not has("UP NEXT"), "the arrow at the top closes the station page")
    # Chat, in the station's room.
    face("STATION PAGE"); time.sleep(1.2); t0 = time.time(); tap_text("JOIN THE CHAT")
    _, t = poll(lambda: has("LEAVE THE CHAT") or has("CHEERS"), 15, 0.15); timed("Join the chat to the room", t)
    shot(f"listener-{tag}-6-chat"); note("chat: " + text()[-260:])
    back(); time.sleep(0.8)

def more_sheet(w, h, tag, ear):
    nav("Radio"); time.sleep(0.6)
    t0 = time.time(); face("MORE"); _, t = poll(lambda: has("SLEEP TIMER"), 4, 0.05); timed("More opens its sheet", t)
    items = [l["text"] for l in read(box=(0, int(h * 0.45), w, h))]; note(f"More holds: {items}")
    tap_text("SLEEP TIMER"); time.sleep(1); at = find("SLEEP TIMER", low=True)
    hit = [b for b in read_band(at[1] + 22, at[1] + 70, x1=w) if b[0].startswith("15")] if at else []
    if hit: tap(hit[0][1] + 12, hit[0][2])
    time.sleep(1)
    shot(f"listener-{tag}-7-sleep"); after = text(box=(0, int(h * 0.3), w, h)); note(f"after a 15-minute sleep timer the radio reads: {after[:200]!r}")
    check(not has("SLEEP TIMER", box=(0, int(h * 0.55), w, h)), "picking 15 MIN closes the sleep timer sheet")
    face("MORE"); time.sleep(0.8); more = text(box=(0, int(h * 0.45), w, h)); check("Until" in more, "More's sleep timer row says until when", more[:140])
    tap_text("SLEEP TIMER"); time.sleep(0.8)
    opts = text(box=(0, int(h * 0.45), w, h)); check("CANCEL" in opts.upper(), "the sleep timer can be cancelled", opts[:120])
    tap_text("CANCEL"); time.sleep(0.8)
    face("MORE"); time.sleep(0.8); tap_text("RECENTLY PLAYED"); time.sleep(1)
    rp = text(box=(0, int(h * 0.45), w, h)); check(any(t in rp for t in ("Low Tide", "Harbour", "Closing", "Lamplight", "Second")), "Recently played lists what was heard", rp[:160])
    key("Escape"); time.sleep(0.6)

def keep_hide(w, h, tag, ear):
    nav("Stations"); time.sleep(1)
    f = find("Find a station") or find("Gangway", cut=0.7)
    if f: tap(*f); key("ctrl+a"); typed("Gangway"); time.sleep(1)
    row = find("LISTENING", low=False, box=(0, 180, w, h - 120))
    if not row: return check(False, "Gangway's row in Stations", text()[:200])
    l, r = col(); tap(r - 18, row[1] - 8); time.sleep(1)
    t0 = time.time(); tap_text("KEEP", box=(0, int(h * 0.4), w, h)); time.sleep(1)
    rowtxt = text(box=(0, 180, w, int(h * 0.5))); check("KEPT" in rowtxt.upper() or "KEEP" in rowtxt.upper(), "a kept station is marked in the list", rowtxt[:160])
    shot(f"listener-{tag}-8-kept")
    # Hide it while it plays: the radio should move on.
    nav("Radio"); time.sleep(0.5); playing_name = on_set()[0]
    nav("Stations"); time.sleep(0.8); row = find("LISTENING", box=(0, 180, w, h - 120))
    tap(r - 18, row[1] - 8); time.sleep(1); scroll_to("HIDE THIS STATION")
    t0 = time.time(); tap_text("HIDE THIS STATION"); time.sleep(0.4)
    if has("HIDE", box=(0, int(h * 0.3), w, h)) and (has("Hide Gangway") or has("CANCEL")): note("Hide asks to confirm: " + text()[:120]); tap_text("HIDE", low=True)
    _, t = poll(lambda: not has("Gangway", box=(0, 180, w, h - 140), cut=0.7), 5, 0.1); timed("Hide this station takes it off the list", t)
    nav("Radio"); time.sleep(1.2); name = on_set()[0]
    check(not name or "gangway" not in norm(name), "the radio moves off a station just hidden", f"the radio shows {name!r}; playing {describe(ear.take(1.5))['rms'] > 0.004}")
    shot(f"listener-{tag}-8-hidden")
    nav("Settings"); time.sleep(1); tap_text("Hidden stations"); time.sleep(1)
    hid = text(box=(0, 150, w, h)); check("Gangway" in hid or "SHOW AGAIN" in hid.upper(), "Settings lists the hidden station", hid[:200])
    t0 = time.time(); tap_text("SHOW AGAIN"); time.sleep(1)
    nav("Stations"); time.sleep(1); _, t = poll(lambda: has("Gangway", box=(0, 180, w, h - 140), cut=0.7), 6, 0.1)
    check(t is not None, "Show again brings it back to the list")

def settings(w, h, tag, ear):
    nav("Settings"); time.sleep(1); shot(f"listener-{tag}-10-settings")
    defaults = {}
    for label in ("A soft hiss while tuning in", "Show stations with nothing to play yet", "Hide mature stations", "Show explicit stations", "Pictures only from stations I keep", "Save ahead on Wi-Fi"):
        at = find(label, cut=0.75); defaults[label] = switch_on(at[1] + 4) if at else None
    scroll_to("Help carry stations"); at = find("Help carry stations", cut=0.75); defaults["Help carry stations"] = switch_on(at[1] + 4) if at else None
    note(f"settings on a first open: {defaults}")
    # Pictures only from kept stations: Gangway (kept) keeps its cover; turn it on and off.
    l, r = col(); wheel((l + r) // 2, h // 2, 15, False); time.sleep(0.5)
    y, b, a = flip("Pictures only from stations I keep"); check(b is not None and a != b, "the pictures switch moves", f"{b} -> {a}")
    if y: flip("Pictures only from stations I keep")
    y, b, a = flip("A soft hiss while tuning in"); check(b is not None and a != b, "the hiss switch moves", f"{b} -> {a}")
    if y: flip("A soft hiss while tuning in")
    y, b, a = flip("Show stations with nothing to play yet"); check(b is not None and a != b, "the empty-stations switch moves", f"{b} -> {a}")
    count = text(box=(0, 0, w, 140)); note(f"with empty stations shown the header reads {count!r}")
    if y: flip("Show stations with nothing to play yet")
    # Sharing and advanced.
    scroll_to("Send the app to someone nearby"); open(ENV.opened, "w").close()
    tap_text("Send the app to someone nearby"); time.sleep(1.5); shot(f"listener-{tag}-11-sendapp")
    note(f"Send the app to someone nearby on a computer shows: {text(box=(0, int(h * 0.35), w, h))[:220]!r}; opened {open(ENV.opened).read().strip()!r}")
    key("Escape"); time.sleep(0.6)
    scroll_to("Connections"); tap_text("Connections"); time.sleep(1); shot(f"listener-{tag}-11-connections"); note("Connections: " + text(box=(0, int(h * 0.3), w, h))[:300])
    scroll_to("About this copy"); tap_text("About this copy"); time.sleep(1); scroll_to("ADVANCED", down=True, tries=3); shot(f"listener-{tag}-12-about"); note("About: " + text(box=(0, int(h * 0.3), w, h))[:300])

# ---- A stranger's view: the open player in a browser tab ------------------------------------
CAPTURE = r"""async (secs) => {
  const s = window.pyrate.tap(); if (!s) return null;
  const ac = new AudioContext({ sampleRate: 48000 }); const src = ac.createMediaStreamSource(s), pr = ac.createScriptProcessor(4096, 1, 1), L = [];
  pr.onaudioprocess = e => L.push(e.inputBuffer.getChannelData(0).slice()); src.connect(pr); pr.connect(ac.destination);
  await new Promise(r => setTimeout(r, secs * 1000)); try { src.disconnect(); pr.disconnect() } catch (e) { } ac.close();
  let n = 0; for (const p of L) n += p.length; const all = new Float32Array(n); let o = 0; for (const p of L) { all.set(p, o); o += p.length }
  const out = new Int16Array(n >> 1); for (let i = 0; i + 1 < n; i += 2) out[i >> 1] = Math.max(-32768, Math.min(32767, Math.round((all[i] + all[i + 1]) * 16383.5)));
  const b = new Uint8Array(out.buffer); let t = ''; for (let i = 0; i < b.length; i += 0x8000) t += String.fromCharCode.apply(null, b.subarray(i, i + 0x8000)); return btoa(t) }"""
class Stranger:
    """A listener elsewhere: a fresh browser tab on the open player."""
    def __init__(self):
        from playwright.sync_api import sync_playwright
        self.pw = sync_playwright().start()
        self.b = self.pw.chromium.launch(args=["--autoplay-policy=no-user-gesture-required"])
        self.pg = self.b.new_context(bypass_csp=True, viewport={"width": 1280, "height": 900}).new_page()
        self.pg.goto(OPEN); poll(lambda: "Connected" in self.pg.text_content("#signal"), 60, 0.2)
    def find(self, name, timeout=90):
        def look():
            for r in self.pg.eval_on_selector_all(".row:not(.skel)", "rs => rs.map(r => [r.dataset.id, r.querySelector('.name').firstChild.textContent, [...r.querySelectorAll('.tag')].map(t => t.textContent)])"):
                if r[1] == name: return r
        r, t = poll(look, timeout, 0.3); return r, t
    def tune(self, sid):
        self.pg.goto(OPEN + "#" + sid); time.sleep(1.5)
        if not self.pg.evaluate("window.pyrate.state().playing"): self.pg.click("#go")
        return poll(lambda: self.pg.evaluate("window.pyrate.state().heard"), 60, 0.1)[1]
    def listen(self, secs):
        b = self.pg.evaluate(CAPTURE, secs)
        return np.frombuffer(base64.b64decode(b), dtype="<i2").astype(np.float32) / 32767 if b else np.zeros(0, np.float32)
    def plan(self): return self.pg.evaluate("window.pyrate.plan()") or {}
    def page(self):
        return self.pg.evaluate("() => ({ name: pname.textContent, about: pabout.textContent, meta: pmeta.textContent, notices: [...document.querySelectorAll('#notices .notice')].map(n => n.textContent), next: next.textContent.slice(0, 200), listings: listings.textContent.slice(0, 300), cover: !cover.hidden && cover.naturalWidth, banner: getComputedStyle(banner).backgroundImage.slice(0, 12), np: np.textContent, live: lamp.className })")
    def close(self): self.b.close(); self.pw.stop()

def tone_wav(path, secs, freq, ch=1, sr=48000, amp=0.3):
    t = np.arange(int(secs * sr)) / sr; fs = freq if isinstance(freq, tuple) else (freq,) * ch
    data = (np.stack([amp * np.sin(2 * np.pi * f * t) for f in fs], 1) * 32767).astype("<i2")
    with wave.open(path, "wb") as w: w.setnchannels(len(fs)); w.setsampwidth(2); w.setframerate(sr); w.writeframes(data.tobytes())
def words_list():
    import glob
    f = glob.glob(os.path.expanduser("~/.cargo/registry/src/*/bip39-*/src/language/english.rs"))
    return re.findall(r'"([a-z]+)"', open(f[0]).read()) if f else []
def snap_words(text_):
    """Twenty-four words read off the screen, each set to the nearest word of the list."""
    wl = words_list(); out = []
    for w in re.findall(r"[a-z]{3,8}", text_.lower()):
        if not wl: out.append(w); continue
        m = difflib.get_close_matches(w, wl, 1, 0.7)
        if m: out.append(m[0])
    return out

def make_files():
    d = f"{BASE}/files"; os.makedirs(d, exist_ok=True)
    f = {k: f"{d}/{n}" for k, n in (("mono", "Sweep - Concert A.wav"), ("stereo", "Sweep - Two Channels.wav"), ("mp3", "Sweep - Pressed Flat.mp3"), ("deck", "Deck Band - Under The Mic.wav"), ("pad", "pad1 - Horn.wav"), ("mic", "mic.wav"), ("cover", "cover.jpg"), ("banner", "banner.jpg"))}
    tone_wav(f["mono"], 20, 440.0); tone_wav(f["stereo"], 20, (523.25, 659.25), ch=2); tone_wav(f"{d}/p.wav", 20, 600.0)
    subprocess.run(["ffmpeg", "-loglevel", "error", "-y", "-i", f"{d}/p.wav", "-b:a", "192k", f["mp3"]], check=True); os.remove(f"{d}/p.wav")
    tone_wav(f["deck"], 40, 330.0, ch=2); tone_wav(f["pad"], 2, 880.0); tone_wav(f["mic"], 120, 500.0)
    y, x = np.mgrid[0:600, 0:600]; Image.fromarray(np.stack([(x * 255 // 600), (y * 255 // 600), np.full_like(x, 90)], 2).astype("uint8")).save(f["cover"], quality=90)
    Image.new("RGB", (1500, 500), (40, 70, 110)).save(f["banner"], quality=90)
    return f
def mic_on(path):
    """The virtual microphone hears a file, round and round."""
    return subprocess.Popen(["sh", "-c", f"while true; do paplay -d micfeed '{path}'; done"], env=dict(os.environ, PULSE_SERVER=ENV.pulse), start_new_session=True)
def mic_off(p):
    try: os.killpg(p.pid, signal.SIGTERM)
    except Exception: pass
def share_of(x, f, width=5):
    sp = np.abs(np.fft.rfft(np.asarray(x, np.float32) * np.hanning(len(x)))) ** 2; fr = np.fft.rfftfreq(len(x), 1 / SR)
    return float(sp[np.abs(fr - f) < width].sum() / (sp[(fr > 50) & (fr < 8000)].sum() + 1e-15))

def t_keeper():
    w, h = SIZES[0]; CUR["t"] = f"keeper {w}x{h}"
    global ENV; ENV = Env(w, h).start(); ear = Ear(ENV); files = make_files(); st = None
    try:
        keeper(w, h, ear, files, None); st = KEEP.get("st")
    except Exception as e: check(False, "the keeper's flow ran to the end", f"{type(e).__name__}: {str(e)[:300]}")
    finally:
        for a in list(APPS): a.stop(); APPS.remove(a)
        st = KEEP.get("st")
        if st: st.close()
        ear.stop(); ENV.stop()

def keeper(w, h, ear, files, st):
    tag = f"{w}x{h}"; app = launch()
    wait_text("STATIONS ON THE AIR", 40); nav("Broadcast"); time.sleep(1)
    KEEP["name"] = f"App Sweep {int(time.time()) % 10000}"
    tap_text("Station name"); typed(KEEP["name"]); tap_text("What it plays, in a line"); typed("Tones from the app sweep")
    shot(f"keeper-{tag}-1-form")
    t0 = time.time(); tap_text("Raise colours"); _, t = poll(lambda: has("Write these down"), 40, 0.1); timed("raise: Raise colours to the 24 words", t)
    lines = read(); top = find("will ever ask for them", lines, cut=0.6) or find("Nobody from Pyrate Radio", lines, cut=0.6); bottom = find("I have written all 24 down", lines, cut=0.6)
    band = [l["text"] for l in lines if top and bottom and top[1] + 8 < (l["box"][1] + l["box"][3]) / 2 < bottom[1] - 8]
    words = snap_words(" ".join(band)); KEEP["words"] = words
    check(len(words) == 24, "the 24 words are shown", f"{len(words)} read")
    note("the words are shown as one unnumbered paragraph")
    shot(f"keeper-{tag}-2-words")
    tap_text("I have written all 24 down"); time.sleep(0.4); t0 = time.time(); tap_text("THEY ARE WRITTEN DOWN")
    _, t = poll(lambda: not has("Write these down"), 10, 0.1); timed("the words close once written down", t)
    _, t = poll(lambda: has("ADD RECORDINGS"), 10, 0.2); shot(f"keeper-{tag}-3-page")
    note("the new station's page reads: " + text(box=(0, 40, w, int(h * 0.45)))[:240])
    # Recordings, three at once from the computer's own file dialog.
    scroll_to("CHOOSE FILES"); ENV.choose(files["mono"], files["stereo"], files["mp3"]); t0 = time.time(); tap_text("CHOOSE FILES")
    _, t = poll(lambda: re.search(r"\b3 recordings", text(box=(0, 0, w, h))), 120, 0.5); timed("recordings: from choosing three files to '3 recordings'", t, 10)
    p = ENV.picks(); check(p and p[-1]["multiple"] and len(p[-1]["picked"]) == 3, "the file dialog takes several recordings at once", str(p[-1] if p else None))
    note("after the files, the page reads: " + text(box=(0, 0, w, h))[:300]); shot(f"keeper-{tag}-4-recordings")
    st = KEEP["st"] = Stranger(); KEEP["t_rec"] = time.time(); r, t = st.find(KEEP["name"], 150)
    note("while a stranger looked for it, the keeper's page said: " + text(box=(0, 40, w, int(h * 0.3)))[:160])
    sid = None
    if r: timed("a stranger's open player lists the new station (after its recordings)", t, 10); sid = r[0]; KEEP["sid"] = sid; t = st.tune(sid); timed("a stranger: Listen to the new station's sound", t)
    else: note("no stranger sees the station after 150 s: the steps go on, and the stranger looks again between them")
    want = {"Concert A": 440.0, "Two Channels": (523.25, 659.25), "Pressed Flat": 600.0} if sid else {}; end = time.time() + 100
    while want and time.time() < end:
        p = st.plan()
        if p.get("kind") == "track" and p.get("title") in want and 600 < p["position_ms"] < p["duration_ms"] - 5500:
            k = p["title"]; f = want.pop(k); x = st.listen(4.5)
            if isinstance(f, tuple): check(describe(x)["tonal"] > 0.75 and all(share_of(x, q) > 0.25 for q in f), f"the app's {k!r} reaches a stranger as both its tones", f"{[round(share_of(x, q), 2) for q in f]}")
            else: hear(x, f"the app's {k!r} at a stranger's speaker", const=f)
        else: time.sleep(0.4)
    check(not want, "every recording came round within 100 s", ", ".join(want))
    # Details: mature, saved; the stranger sees the tag.
    if scroll_to("ABOUT THE STATION"):
        shot(f"keeper-{tag}-5-details"); note("details panel: " + text(box=(0, int(h * 0.2), w, h))[:300])
        y, b, a = flip("Mature"); check(a is True, "the Mature switch turns on", f"{b} -> {a}")
        scroll_to("SAVE"); t0 = time.time(); press_caps("SAVE"); time.sleep(1); note("after SAVE the details read: " + text(box=(0, int(h * 0.3), w, h))[:160])
        if st and sid: st.pg.goto(OPEN); r, t = poll(lambda: "Mature" in (st.find(KEEP["name"], 1)[0] or [0, 0, []])[2], 240, 2); timed("details: a stranger's list tags the station Mature after SAVE", (time.time() - t0) if t is not None else None, 10)
    # Pictures.
    if scroll_to("PICTURES", down=False) or scroll_to("PICTURES"):
        n0 = len(ENV.picks()); ENV.choose(files["cover"]); t0 = time.time(); tap_text("COVER", low=True, cut=0.85); time.sleep(1.5)
        if len(ENV.picks()) == n0: press_caps("COVER", near="BANNER"); time.sleep(1.5)
        check(len(ENV.picks()) > n0, "COVER opens the computer's file dialog", str(ENV.picks()[-1:]))
        _, t = poll(lambda: not has("Redrawing the picture"), 30, 0.3); note(f"cover: the app said 'Redrawing the picture' for {t and round(t, 1)} s")
        n0 = len(ENV.picks()); ENV.choose(files["banner"]); press_caps("BANNER"); time.sleep(2); poll(lambda: not has("Redrawing the picture"), 30, 0.3)
        check(len(ENV.picks()) > n0, "BANNER opens the computer's file dialog", str(ENV.picks()[-1:]))
        shot(f"keeper-{tag}-6-pictures")
        if st and sid:
            st.pg.goto(OPEN + "#" + sid); _, t = poll(lambda: st.page()["cover"], 240, 1); timed("pictures: a stranger sees the new cover", (time.time() - t0) if t is not None else None, 15)
            _, t = poll(lambda: st.page()["banner"] not in ("none", ""), 120, 2); check(t is not None, "a stranger sees the new banner", st.page()["banner"])
    # The schedule, a premiere, a notice, a poster.
    for label, fn in (("CHANGE THE SCHEDULE", schedule_editor), ("PLAN A PREMIERE", premiere), ("POST A NOTICE", notice), ("MAKE A POSTER", poster)):
        if not sid:
            r, _ = st.find(KEEP["name"], 1)
            if r: sid = KEEP["sid"] = r[0]; timed("a stranger's open player lists the new station (after its recordings)", time.time() - KEEP["t_rec"], 10); st.tune(sid)
        scroll_to(label, down=False) or scroll_to(label)
        try: fn(w, h, tag, st if sid else None, sid)
        except Exception as e: check(False, f"{label} ran", f"{type(e).__name__}: {str(e)[:200]}")
    if not sid:
        r, t = st.find(KEEP["name"], max(1, 420 - (time.time() - KEEP["t_rec"])))
        if r: sid = KEEP["sid"] = r[0]; timed("a stranger's open player lists the new station (after its recordings)", time.time() - KEEP["t_rec"], 10); st.tune(sid)
        else: check(False, "a stranger can find the station within seven minutes of its recordings")
    st = st if sid else None
    live_desk(w, h, tag, st, sid, ear, files)
    restore(w, h, tag, st)
KEEP = {}

def schedule_editor(w, h, tag, st, sid):
    t0 = time.time(); tap_text("CHANGE THE SCHEDULE"); _, t = poll(lambda: has("SAVE THE SCHEDULE") or has("HOW IT PLAYS"), 8, 0.1); timed("CHANGE THE SCHEDULE opens the editor", t)
    shot(f"keeper-{tag}-7-schedule"); note("schedule editor: " + text()[:300])
    tap_text("IN ORDER"); scroll_to("SAVE THE SCHEDULE"); t0 = time.time(); tap_text("SAVE THE SCHEDULE"); time.sleep(1.5)
    note("after SAVE THE SCHEDULE the keeper's page says: " + text(box=(0, int(h * 0.2), w, h))[:160])
    if st:
        _, t = poll(lambda: "order" in (st.page()["listings"] + st.pg.text_content("#how")).lower(), 90, 1)
        timed("schedule: a stranger's page says the station plays in order", (time.time() - t0) if t is not None else None, 15)
    if has("SAVE THE SCHEDULE"): back(); time.sleep(1)
def premiere(w, h, tag, st, sid):
    t0 = time.time(); tap_text("PLAN A PREMIERE"); time.sleep(1.2); shot(f"keeper-{tag}-8-premiere"); note("premiere sheet: " + text(box=(0, int(h * 0.25), w, h))[:300])
    into("What it is called", "Sweep Premiere"); time.sleep(0.3); tap_text("Concert A", low=True, cut=0.7); time.sleep(0.5); shot(f"keeper-{tag}-8b-premiere-filled")
    scroll_to("SET IT"); t0 = time.time(); press_caps("SET IT"); time.sleep(2); note("after SET IT: " + text(box=(0, int(h * 0.4), w, h))[:160])
    if st: _, t = poll(lambda: "PREMIERE" in st.page()["listings"].upper(), 90, 1); timed("premiere: a stranger's page lists the PREMIERE", (time.time() - t0) if t is not None else None, 15)
    key("Escape"); time.sleep(0.6)
def notice(w, h, tag, st, sid):
    tap_text("POST A NOTICE"); time.sleep(1); into("What you want to say", "Sweep notice: the tones are back at noon"); time.sleep(0.3); shot(f"keeper-{tag}-9-notice")
    t0 = time.time(); press_caps("POST IT"); time.sleep(1)
    if st: _, t = poll(lambda: any("tones are back" in n for n in st.page()["notices"]), 90, 1); timed("notice: a stranger's page shows it", (time.time() - t0) if t is not None else None, 15)
    face_page = tap_text("SEE YOUR PAGE"); time.sleep(1.5); scroll_to("TAKE DOWN"); t0 = time.time(); tap_text("TAKE DOWN"); time.sleep(1)
    note("the keeper's own page with the notice: " + text(box=(0, 40, w, int(h * 0.6)))[:200])
    if st: _, t = poll(lambda: not any("tones are back" in n for n in st.page()["notices"]), 90, 1); timed("notice: TAKE DOWN removes it from a stranger's page", (time.time() - t0) if t is not None else None, 15)
    back(); time.sleep(1)
def poster(w, h, tag, st, sid):
    tap_text("MAKE A POSTER"); time.sleep(1.5); shot(f"keeper-{tag}-10-poster"); note("poster sheet: " + text(box=(0, int(h * 0.2), w, h))[:240])
    tap_text("THE LINK"); time.sleep(0.8); c = clipboard(); check((sid or "Tune in to") in c, "the poster's THE LINK copies the station's link", c[:90])
    open(ENV.opened, "w").close(); tap_text("AS A FILE"); time.sleep(2); o = open(ENV.opened).read().strip()
    note(f"AS A FILE opened {o!r} on the screen {text(box=(0, int(h * 0.6), w, h))[-120:]!r}")
    key("Escape"); time.sleep(0.6)

def live_desk(w, h, tag, st, sid, ear, files):
    scroll_to("LIVE DESK", down=False) or scroll_to("LIVE DESK")
    shot(f"keeper-{tag}-11-desk"); note("desk: " + text()[:300])
    mic = mic_on(files["mic"])
    try:
        t0 = time.time(); tap_text("CHECK LEVELS"); _, t = poll(lambda: has("CHECKING LEVELS") or has("STOP CHECKING"), 8, 0.1); timed("desk: CHECK LEVELS", t)
        time.sleep(2); shot(f"keeper-{tag}-12-checking"); heard_self = describe(ear.take(1.5))
        note(f"while checking levels the computer's speaker plays rms {heard_self['rms']:.3f} (the mic should not come back out unless asked)")
        tap_text("STOP CHECKING"); time.sleep(1)
        scroll_to("ADD MUSIC"); n0 = len(ENV.picks()); ENV.choose(files["deck"]); tap_text("ADD MUSIC", cut=0.7); time.sleep(2)
        if len(ENV.picks()) == n0: press_caps("ADD MUSIC", near="DECK"); time.sleep(2)
        check(len(ENV.picks()) > n0, "ADD MUSIC opens the computer's file dialog", str(ENV.picks()[-1:])); note("deck after ADD MUSIC: " + text(box=(0, int(h * 0.3), w, h))[:200])
        shot(f"keeper-{tag}-12b-deck")
        scroll_to("LOAD"); n0 = len(ENV.picks()); ENV.choose(files["pad"]); tap_text("LOAD", cut=0.8); time.sleep(2.5)
        check(len(ENV.picks()) > n0, "a pad's LOAD opens the computer's file dialog", str(ENV.picks()[-1:])); note("pads after LOAD: " + text(box=(0, int(h * 0.3), w, h))[:200])
        shot(f"keeper-{tag}-12c-pads"); scroll_to("GO LIVE", down=False)
        t0 = time.time(); tap_text("GO LIVE", low=True); _, t = poll(lambda: has("END THE BROADCAST"), 15, 0.1); timed("desk: GO LIVE to on the air", t)
        t = None
        if st:
            _, t = poll(lambda: "Live" in st.page()["np"] and st.pg.evaluate("window.pyrate.state().heard"), 120, 0.3)
            timed("a stranger hears the app's live show, from GO LIVE", (time.time() - t0) if t is not None else None, 25)
        if t is not None:
            x = st.listen(4); d = describe(x); check(d["rms"] > 0.004, "the live show reaches a stranger's speaker", f"rms {d['rms']:.3f}, tonal {d['tonal']:.2f}")
            check(share_of(x, 500) > 0.2, "the stranger hears the microphone (500 Hz)", f"share {share_of(x, 500):.2f}; deck 330 Hz share {share_of(x, 330):.2f}; loudest {peak(x):.0f} Hz")
        shot(f"keeper-{tag}-13-live")
        t0 = time.time(); tap_text("END THE BROADCAST"); _, t = poll(lambda: has("GO LIVE", box=(0, 0, w, h)) and not has("END THE BROADCAST"), 15, 0.1); timed("desk: END THE BROADCAST", t)
        if not st: return
        _, t = poll(lambda: st.plan().get("kind") == "track" and "Live" not in st.page()["np"], 90, 0.3); timed("after End, a stranger is back on the recordings", (time.time() - t0) if t is not None else None, 20)
        st.pg.goto(OPEN); time.sleep(6); r, _ = st.find(KEEP["name"], 30)
        _, t = poll(lambda: "On air" not in (st.find(KEEP["name"], 1)[0] or [0, 0, ["On air"]])[2], 60, 2)
        check(t is not None, "after End, the station stops saying On air in a stranger's list", str((st.find(KEEP["name"], 1)[0] or [0, 0, None])[2]))
    finally: mic_off(mic)

def restore(w, h, tag, st):
    words = KEEP.get("words") or []
    if len(words) != 24: return check(False, "restore: the words were read", f"{len(words)}")
    app = launch(); wait_text("STATIONS ON THE AIR", 40); nav("Broadcast"); time.sleep(1)
    scroll_to("Bring back a station"); tap_text("Bring back a station"); time.sleep(1); shot(f"keeper-{tag}-14-restore")
    f = find("24 words", low=True) or find("words", low=True)
    at = find("The 24 words", cut=0.7, low=True)
    if at: tap(at[0], at[1]); time.sleep(0.3)
    typed(" ".join(words)); time.sleep(0.5); shot(f"keeper-{tag}-14b-typed")
    scroll_to("BRING IT BACK"); t0 = time.time(); tap_text("BRING IT BACK")
    _, t = poll(lambda: has("ADD RECORDINGS") and has(KEEP["name"], box=(0, 0, w, int(h * 0.3)), cut=0.6), 120, 0.5); timed("restore: BRING IT BACK to the station's page", t, 5)
    note("restored page: " + text(box=(0, 40, w, int(h * 0.4)))[:200]); shot(f"keeper-{tag}-15-restored")

TESTS = {"keeper": t_keeper, "listener": t_listener}

def main():
    want = A.tests or list(TESTS)
    for t in want:
        if t not in TESTS: sys.exit(f"no test {t!r}; tests: {' '.join(TESTS)}")
    if not os.path.exists(BUNDLE): sys.exit(f"no app at {BUNDLE}: flutter build linux --release")
    if not subprocess.run(["pgrep", "-f", "target/release/lantern"], capture_output=True).stdout: sys.exit("no Lantern is running: tools/test/net_up.sh")
    for tool in ("Xvfb", "xdotool", "import", "tesseract", "pulseaudio", "parec", "paplay", "dbus-daemon", "/usr/bin/python3.12", "ffmpeg"):
        if not shutil.which(tool): sys.exit(f"{tool} is needed")
    t_all = time.time()
    for t in want:
        CUR["t"] = t; t0 = time.time(); print(f"==== {t}", flush=True)
        try: TESTS[t]()
        except Exception as e: check(False, f"{t} ran to the end", f"{type(e).__name__}: {str(e)[:300]}")
        print(f"==== {t} took {time.time() - t0:.0f} s", flush=True)
    if not A.keep:
        for p in [p for p in os.listdir(BASE) if p.startswith(("home-", "app-"))] + ["files"]: shutil.rmtree(f"{BASE}/{p}", ignore_errors=True) if os.path.isdir(f"{BASE}/{p}") else os.remove(f"{BASE}/{p}")
    subprocess.run(["pkill", "-f", f"pulseaudio -n --daemonize=no --exit-idle-time=-1 --disallow-exit -F {BASE}/pulse.pa"], capture_output=True)
    print("\n==== Timings (s; SLOW past the limit; read off the screen to about 0.3 s)")
    for t, what, secs, lim in TIMES: print(f"{'SLOW' if secs is None or secs > lim else '    '} {secs if secs is None else round(secs, 2)!s:>7}  {t:<22} {what}")
    bad = [r for r in RES if r[1] in ("FAIL", "SLOW")]
    print(f"\n==== {sum(r[1] == 'ok' for r in RES)} ok, {sum(r[1] == 'FAIL' for r in RES)} failed, {sum(r[1] == 'SLOW' for r in RES)} slow, in {time.time() - t_all:.0f} s")
    for t, s_, w in bad: print(f"{s_:<4} [{t}] {w}")
    sys.exit(1 if any(r[1] == "FAIL" for r in RES) else 0)

if __name__ == "__main__":
    main()
