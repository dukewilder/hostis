#!/usr/bin/env python3
"""The browser player swept the way people use it: every feature, timed, checked by ear.

  browser_sweep.py [--width 390|1440] [--out DIR] [--keep] [test ...]

Tests (default: all, in this order): studio keeper listener throttled chat tuning sleep pocket
flock harbour landing. `studio` and `keeper` raise stations on the shared test network (four
in all, every run), so run them only when the keeper's side is what you are checking.
`keeper` times a waiting listener against the studio's status, records what a listener hears
second by second after End, and what a returning keeper sees. `throttled` opens the player as
a first visit over 4G and over slow 4G with a phone's processor. `listener` runs the whole listening flow (opening, Listen and Stop, switching by list,
dial, Spin, keys and media keys, the station page, Share, Keep, More, search and kinds, the map,
the mini player, every setting, hiding and Forget) as a first-time visitor and as a returning
one (a persistent profile, visited, closed and visited again), at 390 px (a phone, touch) and
1440 px. `studio` raises stations, puts generated recordings on the air (440 Hz, a stereo
file, an MP3 and a five-minute file), checks a stranger's tab hears each one as the tone it
is, goes live, restores from the 24 words and locks the key; it runs first, since `listener`
uses its mature and explicit stations. `pocket` and `flock` need target/release/pocket_test
and flock_test (prebuilt; nothing is compiled here).

Needs the private test network (net_up.sh) and serve_test.sh's two copies: the open player on
8099 and hostis.black's copy on 8097. What the speaker gets is read through window.pyrate.tap()
and analysed with numpy: the test network's recordings are four-note tone cycles, so the notes
heard must be the track the station's clock names, at the place it names.

Prints one line per check (ok / FAIL / SLOW / note), a table of timings, and exits 1 on any
FAIL. Anything over a second on this local network is marked SLOW. Screenshots, scaled down,
go to --out (default /tmp/sweep)."""
import sys, os, re, time, json, base64, wave, math, shutil, tempfile, subprocess, argparse, socket, threading, http.server, urllib.request, urllib.parse, functools
import numpy as np
from playwright.sync_api import sync_playwright

R = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
OPEN = os.environ.get("SWEEP_OPEN", "http://127.0.0.1:8099/")
HARB = os.environ.get("SWEEP_HARBOUR", "http://127.0.0.1:8097/")
MUSIC = os.environ.get("SWEEP_MUSIC", "/tmp/net")
RELAY, GANG = (open("/tmp/net-ids").read().split() + ["", ""])[:2] if os.path.exists("/tmp/net-ids") else ("", "")
SLOW = 1.0
ARGS = ["--autoplay-policy=no-user-gesture-required", "--use-fake-ui-for-media-stream", "--use-fake-device-for-media-stream"]
PHONE_UA = "Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Mobile Safari/537.36"
# The test network's recordings: each a cycle of four notes, two seconds apiece.
TRACKS = {"Harbour Lights": [220, 277, 330, 262], "Low Tide": [196, 247, 294, 220], "Lamplight": [247, 311, 370, 277], "Second Watch": [174, 220, 262, 196], "Secret Tone": [220, 277, 330, 262]}

ap = argparse.ArgumentParser()
ap.add_argument("tests", nargs="*")
ap.add_argument("--width", type=int, action="append")
ap.add_argument("--out", default="/tmp/sweep")
ap.add_argument("--keep", action="store_true", help="keep generated recordings and profiles")
A = ap.parse_args()
WIDTHS = A.width or [390, 1440]
OUT = A.out; os.makedirs(OUT, exist_ok=True)
SHARED = {}          # what one test leaves for another: the stations the studio raised
TEMP = []            # profile dirs and files to remove at the end

# ---- Reporting ---------------------------------------------------------------------------
RES, TIMES, CUR = [], [], {"t": ""}
def check(ok, what, detail=""):
    s = "ok  " if ok else "FAIL"
    line = what + (f" ({detail})" if detail else "")
    RES.append((CUR["t"], s.strip(), line)); print(f"{s} [{CUR['t']}] {line}", flush=True)
    return bool(ok)
def note(what):
    RES.append((CUR["t"], "note", what)); print(f"note [{CUR['t']}] {what}", flush=True)
def timed(what, secs, limit=SLOW):
    TIMES.append((CUR["t"], what, secs, limit))
    if secs is None: return check(False, what + ": never happened")
    s = "SLOW" if secs > limit else "time"
    if secs > limit: RES.append((CUR["t"], "SLOW", f"{what}: {secs:.2f} s (limit {limit} s)"))
    print(f"{s} [{CUR['t']}] {what}: {secs:.2f} s", flush=True)
    return secs <= limit
def poll(fn, timeout=30, every=0.05):
    t0 = time.time()
    while True:
        try: v = fn()
        except Exception: v = None
        if v: return v, time.time() - t0
        if time.time() - t0 > timeout: return None, None
        time.sleep(every)

# ---- Pages -------------------------------------------------------------------------------
def opts(w):
    o = dict(bypass_csp=True, permissions=["microphone", "clipboard-read", "clipboard-write"], accept_downloads=True)
    if w < 600: o.update(viewport={"width": w, "height": 844}, is_mobile=True, has_touch=True, device_scale_factor=1, user_agent=PHONE_UA)
    else: o.update(viewport={"width": w, "height": 900}, device_scale_factor=1)
    return o
# Media keys and headset buttons reach the page through navigator.mediaSession: the handlers the
# page sets are kept, so the test can press them as the phone's own controls would.
MEDIA = "(() => { const ms = navigator.mediaSession; if (!ms) return; const set = ms.setActionHandler.bind(ms); window.__ms = {}; ms.setActionHandler = (a, f) => { window.__ms[a] = f; return set(a, f) } })()"
def setup(ctx, w):
    ctx.add_init_script(MEDIA)
    ctx._w = w
    return ctx
def page(ctx):
    pg = ctx.new_page() if not getattr(ctx, "_first", None) else ctx._first
    ctx._first = None
    pg._w = ctx._w; pg._errs = []
    pg.on("pageerror", lambda e: pg._errs.append(f"page error: {str(e)[:200]}"))
    pg.on("console", lambda m: m.type == "error" and pg._errs.append(f"console error: {m.text[:200]}"))
    pg.on("dialog", lambda d: (pg.__dict__.setdefault("_dialogs", []).append(d.message), d.accept()))
    return pg
def fresh(browser, w):
    return setup(browser.new_context(**opts(w)), w)
def persistent(pw, d, w):
    ctx = pw.chromium.launch_persistent_context(d, headless=True, args=ARGS, **opts(w))
    setup(ctx, w); ctx._first = ctx.pages[0] if ctx.pages else None
    return ctx
def phone(pg): return pg._w < 600
def url(pg):
    """The address now: asked of the page, since page.url only changes while Playwright is called."""
    try: return pg.evaluate("location.href")
    except Exception: return ""
def press(pg, sel):
    if phone(pg): pg.tap(sel, timeout=8000)
    else: pg.click(sel, timeout=8000)
def txt(pg, sel): return pg.evaluate("s => { const e = document.querySelector(s); return e ? e.textContent.trim() : '' }", sel)
def st(pg): return pg.evaluate("window.pyrate ? window.pyrate.state() : null") or {}
def shown(pg, sel): return pg.evaluate("s => { const e = document.querySelector(s); if (!e || e.closest('[hidden]')) return false; const r = e.getBoundingClientRect(); return r.width > 0 && r.height > 0 && getComputedStyle(e).visibility !== 'hidden' }", sel)
def errors(pg, what):
    e = list(dict.fromkeys(pg._errs)); pg._errs.clear()
    check(not e, f"no console errors {what}", "; ".join(e[:3]))
def connected(pg, timeout=60):
    _, t = poll(lambda: "Connected" in txt(pg, "#signal"), timeout)
    return t
def rows(pg):
    return pg.eval_on_selector_all(".row:not(.skel)", "rs => rs.map(r => ({ id: r.dataset.id, name: r.querySelector('.name').firstChild.textContent, tags: [...r.querySelectorAll('.tag')].map(t => t.textContent) }))")
def view(pg, v):
    press(pg, f".tab[data-v={v}]")
    return poll(lambda: st(pg).get("view") == v, 5, 0.02)[1]

def shot(pg, name, full=False):
    p = f"{OUT}/{name}.png"
    try: pg.screenshot(path=p, full_page=full)
    except Exception as e: note(f"screenshot {name} failed: {e}"); return
    from PIL import Image
    im = Image.open(p); mw = 390 if im.width <= 400 else 480
    if im.width > mw: im = im.resize((mw, int(im.height * mw / im.width)), Image.LANCZOS)
    if im.height > 2400: im = im.crop((0, 0, im.width, 2400))
    im.convert("RGB").save(f"{OUT}/{name}.jpg", quality=70); os.remove(p)

# Layout: sideways scrolling, text that is clipped or runs off the screen, controls that sit on
# top of one another, names on the dial that overlap, and tap targets too small for a thumb.
LAYOUT = r"""() => {
  const W = innerWidth, out = [];
  const vis = e => { if (e.closest('[hidden]') || (e.closest('details:not([open])') && !e.closest('summary'))) return false; const s = getComputedStyle(e); if (s.visibility === 'hidden' || s.display === 'none' || +s.opacity === 0) return false; const r = e.getBoundingClientRect(); return r.width > 0 && r.height > 0 };
  const nm = e => e.tagName.toLowerCase() + (e.id ? '#' + e.id : '') + (typeof e.className === 'string' && e.className.trim() ? '.' + e.className.trim().split(/\s+/).join('.') : '') + ' "' + (e.textContent || e.value || e.placeholder || '').trim().replace(/\s+/g, ' ').slice(0, 28) + '"';
  if (document.documentElement.scrollWidth > W + 1) out.push('the page scrolls sideways: ' + document.documentElement.scrollWidth + ' px in ' + W);
  for (const e of document.querySelectorAll('body *')) {
    if (e.closest('#glass, svg, .skip-link, #mini[hidden], .vh, .sr-only, .visually-hidden') || !vis(e)) continue;
    const r = e.getBoundingClientRect(), s = getComputedStyle(e);
    // Inside a strip that scrolls sideways (the kinds on a phone), running past the edge is the design.
    const strip = (() => { for (let a = e.parentElement; a && a !== document.body; a = a.parentElement) { const o = getComputedStyle(a).overflowX; if (o === 'auto' || o === 'scroll') return true } return false })();
    if ((r.right > W + 1 || r.left < -1) && s.position !== 'fixed' && !strip) out.push('runs off the screen: ' + nm(e) + ' ' + Math.round(r.left) + '..' + Math.round(r.right));
    if (e.children.length === 0 && e.textContent.trim() && e.scrollWidth > e.clientWidth + 2 && s.overflowX !== 'visible') out.push((s.textOverflow === 'ellipsis' ? 'cut short with an ellipsis: ' : 'text clipped: ') + nm(e));
    if (e.children.length === 0 && e.textContent.trim() && e.scrollHeight > e.clientHeight + 3 && s.overflowY === 'hidden' && s.webkitLineClamp === 'none') out.push('text clipped at the bottom: ' + nm(e));
  }
  // The mini player is fixed over the page: what it covers counts only at the foot of the page,
  // where nothing can be scrolled out from under it.
  const mini = document.getElementById('mini'), atFoot = scrollY + innerHeight >= document.documentElement.scrollHeight - 2;
  if (mini && vis(mini) && atFoot) { const m = mini.getBoundingClientRect(); for (const e of document.querySelectorAll('a, button, input, select, textarea, p')) { if (mini.contains(e) || !vis(e)) continue; const r = e.getBoundingClientRect(); if (Math.min(r.right, m.right) - Math.max(r.left, m.left) > 2 && Math.min(r.bottom, m.bottom) - Math.max(r.top, m.top) > 2) out.push('at the foot of the page the mini player covers ' + nm(e)) } }
  // A bar that sticks to the top while the page scrolls covers whatever passes under it: only
  // two things that move together, or two that are both pinned, can truly overlap.
  // A menu that opens over the page (absolute, raised) is meant to cover what is under it.
  const pinned = e => { for (let a = e; a && a !== document.body; a = a.parentElement) { const s = getComputedStyle(a); if (s.position === 'sticky' || s.position === 'fixed' || (s.position === 'absolute' && +s.zIndex >= 10)) return true } return false };
  const ctl = [...document.querySelectorAll('button, a, input, select, textarea, h1, h2, .np, .by, .slot, .time, .ident, .cs, .tag')].filter(e => !e.closest('#glass, svg, .skip-link, #mini') && vis(e));
  for (let i = 0; i < ctl.length; i++) for (let j = i + 1; j < ctl.length; j++) {
    const a = ctl[i], b = ctl[j]; if (a.contains(b) || b.contains(a) || pinned(a) !== pinned(b)) continue;
    const p = a.getBoundingClientRect(), q = b.getBoundingClientRect();
    if (Math.min(p.right, q.right) - Math.max(p.left, q.left) > 2 && Math.min(p.bottom, q.bottom) - Math.max(p.top, q.top) > 2) out.push('overlap: ' + nm(a) + ' / ' + nm(b));
  }
  const dn = [...document.querySelectorAll('#names button')];
  for (let i = 0; i < dn.length; i++) for (let j = i + 1; j < dn.length; j++) { const p = dn[i].getBoundingClientRect(), q = dn[j].getBoundingClientRect(); if (Math.min(p.right, q.right) - Math.max(p.left, q.left) > 1 && Math.min(p.bottom, q.bottom) - Math.max(p.top, q.top) > 1) out.push('names overlap on the dial: ' + dn[i].textContent + ' / ' + dn[j].textContent) }
  // A box inside its label is tapped through the label: the label is what must be big enough.
  if (W < 600) for (const e of document.querySelectorAll('button, a, input, select, [role=button], summary, label')) { if (e.closest('.skip-link, footer, #glass') || !vis(e) || (e.matches('input[type=checkbox], input[type=radio]') && e.closest('label')) || (e.matches('label') && !e.querySelector('input[type=checkbox], input[type=radio]'))) continue; const r = e.getBoundingClientRect(); if (r.height < 30 || r.width < 30) out.push('small to tap: ' + nm(e) + ' ' + Math.round(r.width) + 'x' + Math.round(r.height)) }
  return [...new Set(out)].slice(0, 30);
}"""
def layout(pg, where):
    try: issues = pg.evaluate(LAYOUT)
    except Exception as e: note(f"layout check failed at {where}: {e}"); return
    check(not issues, f"layout at {pg._w} px, {where}", "; ".join(issues[:8]) + (f" (+{len(issues) - 8} more)" if len(issues) > 8 else ""))

# ---- Hearing -----------------------------------------------------------------------------
# What reaches the speaker, both channels, as 24 kHz samples, with the station's plan at the
# moment the capture began.
CAPTURE = r"""async (secs) => {
  const s = window.pyrate.tap(); if (!s) return null;
  const ac = new AudioContext({ sampleRate: 48000 });
  const src = ac.createMediaStreamSource(s), pr = ac.createScriptProcessor(4096, 2, 2), L = [], R = [];
  let p0 = null, t0 = 0;
  pr.onaudioprocess = e => { if (p0 === null) { p0 = window.pyrate.plan(); t0 = Date.now() } L.push(e.inputBuffer.getChannelData(0).slice()); R.push(e.inputBuffer.getChannelData(1).slice()) };
  src.connect(pr); pr.connect(ac.destination);
  await new Promise(r => setTimeout(r, secs * 1000));
  try { src.disconnect(); pr.disconnect() } catch (e) { }
  ac.close();
  const pack = parts => { let n = 0; for (const p of parts) n += p.length; const all = new Float32Array(n); let o = 0; for (const p of parts) { all.set(p, o); o += p.length }
    const out = new Int16Array(n >> 1); for (let i = 0; i + 1 < n; i += 2) out[i >> 1] = Math.max(-32768, Math.min(32767, Math.round((all[i] + all[i + 1]) * 16383.5)));
    const b = new Uint8Array(out.buffer); let t = ''; for (let i = 0; i < b.length; i += 0x8000) t += String.fromCharCode.apply(null, b.subarray(i, i + 0x8000)); return btoa(t) };
  return { l: pack(L), r: pack(R), plan: p0, at: t0 };
}"""
SR = 24000
def capture(pg, secs):
    a = pg.evaluate(CAPTURE, secs)
    if not a: return None
    dec = lambda s: np.frombuffer(base64.b64decode(s), dtype="<i2").astype(np.float32) / 32767
    a["L"], a["R"] = dec(a["l"]), dec(a["r"]); a["x"] = (a["L"] + a["R"]) / 2
    return a
def spectrum(x):
    sp = np.abs(np.fft.rfft(x * np.hanning(len(x)))) ** 2; f = np.fft.rfftfreq(len(x), 1 / SR)
    return sp, f
def describe(x):
    x = np.asarray(x, np.float32)
    if len(x) < SR // 4: return {"rms": 0.0, "tonal": 0.0, "hi": 0.0, "flat": 1.0}
    sp, f = spectrum(x); band = (f > 50) & (f < 8000); tot = sp[band].sum() + 1e-15
    # The share of the sound in a few narrow peaks: near 1 for tones, a few hundredths for noise.
    s2 = sp.copy(); s2[~band] = 0; df = f[1] - f[0]; w = max(1, int(6 / df)); got = 0.0
    for _ in range(10):
        i = int(np.argmax(s2)); lo, hi = max(0, i - w), i + w + 1; got += s2[lo:hi].sum(); s2[max(0, i - 3 * w):i + 3 * w + 1] = 0
    p = sp[band] + 1e-15
    return {"rms": float(np.sqrt(np.mean(x ** 2))), "tonal": float(got / tot), "hi": float(sp[f > 4000].sum() / (sp[f > 30].sum() + 1e-15)), "flat": float(np.exp(np.mean(np.log(p))) / np.mean(p))}
def peak(x):
    n = len(x); N = 8 * n
    sp = np.abs(np.fft.rfft(x * np.hanning(n), N)); f = np.fft.rfftfreq(N, 1 / SR); sp[(f < 60) | (f > 8000)] = 0
    return float(f[int(np.argmax(sp))])
def notes(x, win=0.4, hop=0.2):
    n, h = int(SR * win), int(SR * hop); out = []
    for i in range(0, len(x) - n + 1, h):
        seg = x[i:i + n]; out.append((i / SR + win / 2, peak(seg) if np.sqrt(np.mean(seg ** 2)) > 0.003 else 0.0))
    return out
def in_step(ns, seq, pos0, step_ms=2000, dur=None):
    """The best share of windows whose note is the one the clock names, and the lag that gives it.
    Windows past the end of the track the plan named (the next one has begun) are left out."""
    best = (0.0, None)
    for lag in range(-4000, 4001, 100):
        ok = tot = 0
        for t, f in ns:
            if not f or (dur and pos0 + t * 1000 > dur - 400): continue
            tot += 1; ok += abs(f - seq[int((pos0 + t * 1000 + lag) // step_ms) % len(seq)]) < 7
        if tot and ok / tot > best[0] + 1e-9: best = (ok / tot, lag)
    return best
def hear(a, what, seq=None, const=None):
    """Checks that what was captured is the recording: a tone, not noise or silence, and the
    notes the station's clock names."""
    if not a: return check(False, f"{what}: the tab gives its sound to the test")
    d = describe(a["x"]); plan = a.get("plan") or {}
    if d["rms"] < 0.004: return check(False, f"{what}: sound reaches the speaker", f"rms {d['rms']:.4f}, plan {plan.get('kind')} {plan.get('title')}")
    ok = check(d["tonal"] > 0.75 and d["hi"] < 0.03 and d["flat"] < 0.05, f"{what}: a clean tone, not noise",
               f"tonal {d['tonal']:.2f}, above 4 kHz {d['hi'] * 100:.2f}%, flatness {d['flat']:.4f}, rms {d['rms']:.3f}")
    title = plan.get("title"); seq = seq or TRACKS.get(title)
    if const:
        for ch, want in const.items():
            ns = [f for _, f in notes(a[ch]) if f]
            share = sum(abs(f - want) < 5 for f in ns) / max(1, len(ns))
            ok &= check(share > 0.9, f"{what}: channel {ch} holds {want} Hz", f"{share * 100:.0f}% of windows; median {np.median(ns) if ns else 0:.1f} Hz")
    elif seq and plan.get("position_ms") is not None:
        rate, lag = in_step(notes(a["x"]), seq, plan["position_ms"], dur=plan.get("duration_ms"))
        ok &= check(rate > 0.85, f"{what}: the notes of {title!r} at the place its clock names", f"{rate * 100:.0f}% of windows, lag {lag} ms")
        if lag is not None and abs(lag) > 1500: note(f"{what}: the speaker is {lag / 1000:.1f} s off the station's clock")
    return ok
def silent(pg, secs, what):
    a = capture(pg, secs); d = describe(a["x"]) if a else {"rms": 0}
    return check(d["rms"] < 0.002, what, f"rms {d['rms']:.4f}")

def until_heard(pg, sid=None, timeout=60):
    return poll(lambda: (lambda s: s.get("heard") and (sid is None or s.get("station") == sid))(st(pg)), timeout, 0.03)[1]
SOUNDING = ("Gangway", "Second Watch")      # the test network's stations with recordings
def row_id(pg, name):
    return next((r["id"] for r in rows(pg) if r["name"] == name), None)
def tune(pg, name):
    """Plays the named station from its row, and waits for its sound."""
    sid = row_id(pg, name)
    if not sid: return None
    if st(pg).get("station") != sid or not st(pg).get("playing"): press(pg, f".row[data-id='{sid}']")
    until_heard(pg, sid); return sid

# ---- Recordings for the studio -------------------------------------------------------------
def tone_wav(path, secs, sr, freq, ch=1, amp=0.3):
    t = np.arange(int(secs * sr)) / sr
    cols = [amp * np.sin(2 * np.pi * (freq(t) if callable(freq) else freq) * t) for _ in range(ch)] if not isinstance(freq, tuple) else [amp * np.sin(2 * np.pi * f * t) for f in freq]
    data = (np.stack(cols, 1) * 32767).astype("<i2")
    with wave.open(path, "wb") as w: w.setnchannels(len(cols)); w.setsampwidth(2); w.setframerate(sr); w.writeframes(data.tobytes())
def long_wav(path, secs=300, sr=48000):
    # A step of 20 Hz every 15 s, 300 Hz up to 680 Hz: the note says where in the file it is.
    with wave.open(path, "wb") as w:
        w.setnchannels(1); w.setsampwidth(2); w.setframerate(sr); ph = 0.0
        for k in range(secs // 15):
            f = 300 + 20 * k; t = np.arange(15 * sr)
            w.writeframes((0.3 * np.sin(ph + 2 * np.pi * f * t / sr) * 32767).astype("<i2").tobytes()); ph = (ph + 2 * np.pi * f * 15) % (2 * np.pi)
def recordings():
    d = f"{OUT}/files"; os.makedirs(d, exist_ok=True); TEMP.append(d)
    f = {"mono": f"{d}/Sweep - Concert A.wav", "stereo": f"{d}/Sweep - Two Channels.wav", "mp3": f"{d}/Sweep - Pressed Flat.mp3", "long": f"{d}/Sweep - The Long Watch.wav"}
    tone_wav(f["mono"], 20, 48000, 440.0)
    tone_wav(f["stereo"], 20, 44100, (523.25, 659.25))
    tone_wav(f"{d}/mp3src.wav", 20, 44100, 600.0)
    if shutil.which("ffmpeg"): subprocess.run(["ffmpeg", "-loglevel", "error", "-y", "-i", f"{d}/mp3src.wav", "-b:a", "192k", f["mp3"]], check=True)
    else: f.pop("mp3")
    os.remove(f"{d}/mp3src.wav")
    long_wav(f["long"])
    return f

# ==== Tests ================================================================================
def t_studio(pw, browser):
    """Raising stations from the open player, recordings on the air, live, restore and lock."""
    files = recordings()
    prof = tempfile.mkdtemp(prefix="sweep-keeper-", dir="/tmp"); TEMP.append(prof)
    ctx = persistent(pw, prof, 1440); pg = page(ctx)
    pg.goto(OPEN); connected(pg)
    view(pg, "studio"); shot(pg, "studio-1440-form"); layout(pg, "the Start a Station form")
    press(pg, "#st-raise")
    check(txt(pg, "#st-note") == "A station needs a name.", "Raise colours with no name says why", txt(pg, "#st-note"))
    pg.fill("#st-name", "Sweep Tones"); pg.fill("#st-about", "Test tones from the browser sweep")
    pg.click("summary:has-text('More options')"); pg.fill("#st-kinds", "test, tones, sweep")
    t0 = time.time(); press(pg, "#st-raise")
    _, t = poll(lambda: shown(pg, "#st-show"), 30, 0.02); timed("studio: Raise colours to the 24 words", t)
    words = pg.eval_on_selector_all("#st-wordlist li", "l => l.map(x => x.textContent)")
    check(len(words) == 24 and len(set(words)) > 16, "twenty-four words are shown", f"{len(words)} words")
    shot(pg, "studio-1440-words"); layout(pg, "the 24 words")
    check(pg.is_disabled("#st-done"), "Continue waits until the words are written down")
    pg.check("#st-written"); check(not pg.is_disabled("#st-done"), "ticking the box lets Continue go")
    press(pg, "#st-done")
    check(pg.eval_on_selector_all("#st-wordlist li", "l => l.length") == 0 and shown(pg, "#st-mine"), "the words leave the page and the station card shows")
    SHARED["words"] = " ".join(words)
    seen = []
    def ready():
        s = txt(pg, "#st-status")
        if s and (not seen or seen[-1] != s): seen.append(s)
        return s and "ready" not in s and "Looking" not in s
    _, t = poll(ready, 180, 0.1); timed("studio: the station gets ready (proof of work) after Continue", time.time() - t0 if t else None, 10)
    sid = pg.evaluate("window.pyrate.mine().slice(-1)[0]"); SHARED["tones"] = sid
    # Three recordings at once: 440 Hz mono at 48 kHz, a stereo file at 44.1 kHz, an MP3.
    order = [k for k in ("mono", "stereo", "mp3") if k in files]
    t0 = time.time(); pg.set_input_files("#st-files", [files[k] for k in order])
    qtimes = {}
    def queue():
        rs = pg.eval_on_selector_all(".qi", "rs => rs.map(r => [r.querySelector('.qt').textContent, r.querySelector('.qs').textContent, r.className])")
        for t_, s_, c_ in rs:
            if ("done" in c_ or "bad" in c_) and t_ not in qtimes: qtimes[t_] = (time.time() - t0, s_, "bad" not in c_)
        return len(qtimes) >= len(order)
    poll(queue, 300, 0.2)
    for title, (secs, s_, ok) in qtimes.items():
        check(ok, f"studio: {title!r} goes on the clock", s_); timed(f"studio: a 20 s recording ({title}) from choosing to on the clock (cumulative)", secs, 5)
    _, t = poll(lambda: txt(pg, "#st-status").startswith("On the network · ") and f"{len(order)} recordings" in txt(pg, "#st-status"), 240, 0.2)
    timed("studio: from choosing three recordings to 'On the air'", time.time() - t0 if t else None, 10)
    seen.append(txt(pg, "#st-status")); note("studio statuses seen: " + " | ".join(dict.fromkeys(seen)))
    shot(pg, "studio-1440-mine"); layout(pg, "the station card with its recordings")
    # A second station, mature, with a five-minute recording.
    press(pg, "#st-another")
    check(shown(pg, "#st-new") and pg.input_value("#st-name") == "", "Start another gives an empty form")
    pg.fill("#st-name", "Sweep Long"); pg.fill("#st-about", "Five minutes of rising tones")
    if not shown(pg, "#st-kinds"): pg.click("summary:has-text('More options')")
    pg.fill("#st-kinds", "test, drone"); pg.check("#st-mature")
    press(pg, "#st-raise"); poll(lambda: shown(pg, "#st-show"), 30, 0.05); pg.check("#st-written"); press(pg, "#st-done")
    SHARED["long_words"] = None
    poll(ready, 180, 0.1)
    lid = pg.evaluate("window.pyrate.mine().slice(-1)[0]"); SHARED["long"] = lid
    check(lid != sid, "Start another raised a second station", lid[:12])
    t0 = time.time(); pg.set_input_files("#st-files", files["long"])
    _, t = poll(lambda: pg.eval_on_selector_all(".qi.done, .qi.bad", "l => l.length") >= 1 and "The Long Watch" in txt(pg, ".qi.done .qt, .qi.bad .qt"), 600, 0.25)
    timed("studio: a five-minute recording from choosing to on the clock", t, 15)
    top = pg.evaluate("(() => { const q = document.querySelector('.qi'); return q ? [q.className, q.textContent] : null })()")
    check(top and "done" in top[0], "the five-minute recording goes on the clock", str(top))
    note(f"the studio's queue for the second station still lists the first station's recordings: {pg.locator('.qi').count()} rows")
    _, t = poll(lambda: txt(pg, "#st-status").startswith("On the network · "), 300, 0.25)
    timed("studio: the five-minute recording from choosing to 'On the air'", time.time() - t0 if t else None, 20)
    errors(pg, "while raising and sending")
    # A stranger's tab: the new station in its list, and each recording heard as the tone it is.
    lc = fresh(browser, 1440); lp = page(lc)
    t0 = time.time(); lp.goto(OPEN)
    _, t = poll(lambda: any(r["id"] == sid for r in rows(lp)), 90, 0.2); timed("a stranger's fresh tab lists the new station", t, 5)
    lp.goto(OPEN + "#" + sid); poll(lambda: txt(lp, "#name") == "Sweep Tones", 30)
    connected(lp); time.sleep(1.5)
    t0 = time.time(); press(lp, "#go"); timed("a stranger's tab: Listen to the new station's first sound", until_heard(lp, sid))
    meta = txt(lp, "#pmeta"); note(f"a stranger's station page header for it: {meta!r}")
    check("tones" in meta, "the kinds given when raising show on the station page", meta)
    want = {"Concert A": {"L": 440.0, "R": 440.0}, "Two Channels": {"L": 523.25, "R": 659.25}, "Pressed Flat": {"L": 600.0, "R": 600.0}}
    left = {k: v for k, v in want.items() if any(k in q for q in qtimes)}
    end = time.time() + 120
    while left and time.time() < end:
        p = lp.evaluate("window.pyrate.plan()") or {}
        if p.get("kind") == "track" and p.get("title") in left and 600 < p["position_ms"] < p["duration_ms"] - 5600:
            k = p["title"]; a = capture(lp, 4.5); hear(a, f"browser-encoded {k!r} heard in a stranger's tab", const=left.pop(k))
        else: time.sleep(0.4)
    check(not left, "every recording came round within two minutes", ", ".join(left))
    # The five-minute file: its note says where in the file the speaker is.
    lp.goto(OPEN + "#" + lid); poll(lambda: txt(lp, "#name") == "Sweep Long", 30); time.sleep(1)
    if not st(lp).get("playing"): press(lp, "#go")
    until_heard(lp, lid); time.sleep(0.5)
    a = capture(lp, 5)
    if a and (a.get("plan") or {}).get("kind") == "track":
        pos = a["plan"]["position_ms"]; got = peak(a["x"][: SR * 2]); exp = 300 + 20 * int(pos // 15000)
        hear(a, "the five-minute recording")
        check(abs(got - exp) < 6 or abs(got - (exp + 20)) < 6, "the five-minute recording plays from the place the clock names", f"heard {got:.0f} Hz at {pos / 1000:.0f} s in; expected {exp} Hz")
    else: check(False, "the five-minute recording plays", str((a or {}).get("plan")))
    # Live, basically: Go live from the fake microphone on the long station; the stranger hears it.
    t0 = time.time(); press(pg, "#st-live")
    _, t = poll(lambda: "ON AIR" in txt(pg, "#st-livetime"), 15, 0.05); timed("studio: Go live to ON AIR", t)
    _, t = poll(lambda: "Live" in txt(lp, "#np") and st(lp).get("heard"), 120, 0.1)
    timed("a listener hears the live broadcast after Go live", time.time() - t0 if t else None, 25)
    if t:
        a = capture(lp, 4); d = describe(a["x"]) if a else {"rms": 0}
        check(d["rms"] > 0.004, "the live sound reaches the listener's speaker", f"rms {d['rms']:.3f}, tonal {d.get('tonal', 0):.2f}, above 4 kHz {d.get('hi', 0) * 100:.1f}%")
        check(shown(lp, "#lamp.on") or "on" in (lp.get_attribute("#lamp", "class") or ""), "the On air lamp is lit for the listener")
    shot(pg, "studio-1440-live"); layout(pg, "the studio while live")
    t0 = time.time(); press(pg, "#st-live")
    check(txt(pg, "#st-livetime") == "" and txt(pg, "#st-live .lbl") == "Go live", "End the broadcast ends it")
    _, t = poll(lambda: (lp.evaluate("window.pyrate.plan()") or {}).get("kind") == "track" and st(lp).get("heard") and "Live" not in txt(lp, "#np"), 90, 0.2)
    timed("after End, the listener is back on the recordings", t, 20)
    _, t = poll(lambda: "On air" not in next((r["tags"] for r in rows(lp) if r["id"] == lid), ["On air"]), 90, 0.5)
    check(t is not None, "after End, the station's row stops saying On air", str(next((r["tags"] for r in rows(lp) if r["id"] == lid), None)))
    errors(pg, "in the studio"); errors(lp, "in the stranger's tab")
    lc.close(); ctx.close()
    # The returning keeper: the station is there again, without the words.
    ctx = persistent(pw, prof, 1440); pg = page(ctx); pg.goto(OPEN); t0 = time.time()
    connected(pg); view(pg, "studio")
    _, t = poll(lambda: shown(pg, "#st-mine") and txt(pg, "#st-mname") == "Sweep Tones", 120, 0.1)
    timed("returning keeper: the Broadcast tab shows the station again", time.time() - t0 if t else None, 3)
    _, t = poll(lambda: txt(pg, "#st-status").startswith("On the network · "), 60, 0.2)
    check(t is not None, "returning keeper: its status is on the air", txt(pg, "#st-status"))
    # Lock it under a passphrase, reload, unlock.
    pg.click("summary:has-text('Lock this station in this browser')")
    pg.fill("#st-newpass", "short"); press(pg, "#st-lockit")
    check("Eight characters" in txt(pg, "#st-locknote"), "a short passphrase is refused", txt(pg, "#st-locknote"))
    pg.fill("#st-newpass", "salt pork and hard tack"); t0 = time.time(); press(pg, "#st-lockit")
    _, t = poll(lambda: txt(pg, "#st-locknote").startswith("Locked"), 20, 0.02); timed("studio: Lock it", t)
    pg.reload(); connected(pg); view(pg, "studio")
    check(shown(pg, "#st-lock"), "after a reload the studio asks for the passphrase")
    pg.fill("#st-pass", "wrong passphrase"); press(pg, "#st-open")
    poll(lambda: txt(pg, "#st-locknote2") not in ("", "Unlocking"), 20, 0.05)
    check(txt(pg, "#st-locknote2") == "That is not the passphrase.", "a wrong passphrase is refused", txt(pg, "#st-locknote2"))
    pg.fill("#st-pass", "salt pork and hard tack"); t0 = time.time(); press(pg, "#st-open")
    _, t = poll(lambda: shown(pg, "#st-mine"), 30, 0.02); timed("studio: Unlock to the station card", t)
    errors(pg, "returning keeper")
    ctx.close()
    # The third station, raised on a phone: explicit, four kinds where three are asked for.
    pc = fresh(browser, 390); pp = page(pc); pp.goto(OPEN); connected(pp); view(pp, "studio")
    shot(pp, "studio-390-form", full=True); layout(pp, "the Start a Station form")
    pp.fill("#st-name", "Sweep Explicit"); pp.fill("#st-about", "Nothing to play, explicit, for the settings")
    press(pp, "summary:has-text('More options')"); pp.fill("#st-kinds", "one, two, three, four"); pp.check("#st-explicit")
    press(pp, "#st-raise"); _, t = poll(lambda: shown(pp, "#st-show") or txt(pp, "#st-note"), 30, 0.05)
    if shown(pp, "#st-show"):
        shot(pp, "studio-390-words"); layout(pp, "the 24 words")
        pp.check("#st-written"); press(pp, "#st-done"); poll(lambda: "ready" not in txt(pp, "#st-status") and txt(pp, "#st-status"), 180, 0.2)
        SHARED["explicit"] = pp.evaluate("window.pyrate.mine().slice(-1)[0]")
        note(f"four kinds where three are asked for: raised without a word ({txt(pp, '#st-note')!r})")
        shot(pp, "studio-390-mine", full=True); layout(pp, "the station card")
    else: check(False, "a station with four kinds can be raised, or is told why", txt(pp, "#st-note"))
    errors(pp, "raising on a phone")
    # Restore from the 24 words, in a fresh phone tab.
    rc = fresh(browser, 390); rp = page(rc); rp.goto(OPEN); connected(rp); view(rp, "studio")
    press(rp, "summary:has-text('Bring back a station')")
    rp.fill("#st-words", " ".join(SHARED["words"].split()[:23])); press(rp, "#st-restore")
    check(txt(rp, "#st-note") == "That needs all 24 words.", "23 words are refused, with the reason", txt(rp, "#st-note"))
    rp.fill("#st-words", "  " + SHARED["words"].upper().replace(" ", "\n ") + " "); t0 = time.time(); press(rp, "#st-restore")
    _, t = poll(lambda: shown(rp, "#st-mine") and txt(rp, "#st-mname") == "Sweep Tones", 120, 0.05)
    timed("studio: Bring it back from the 24 words to the station card", t, 3)
    check(t is not None and rp.evaluate("window.pyrate.mine()").count(sid) == 1, "the words bring back the same station", txt(rp, "#st-note"))
    _, t = poll(lambda: txt(rp, "#st-status").startswith("On the network · "), 60, 0.2)
    check(t is not None, "the restored station knows its recordings", txt(rp, "#st-status"))
    shot(rp, "studio-390-restored"); layout(rp, "the restored station")
    errors(rp, "restoring")
    pc.close(); rc.close()

# What reaches the speaker over a stretch, with what the set says every quarter second.
TIMELINE = CAPTURE.replace("let p0 = null, t0 = 0;", "let p0 = null, t0 = 0; const tl = [], T0 = Date.now(), iv = setInterval(() => { const p = window.pyrate.plan() || {}; tl.push([Date.now() - T0, p.kind, p.title || '', document.getElementById('np').textContent]) }, 250);").replace("ac.close();", "ac.close(); clearInterval(iv);").replace("return { l: pack(L), r: pack(R), plan: p0, at: t0 };", "return { l: pack(L), r: pack(R), plan: p0, at: t0, tl, T0 };")
def t_keeper(pw, browser):
    """A keeper in detail, on a station of its own: what a listener waiting on an empty station
    hears when its first recording goes up, against what the studio's status says; what the
    listener hears from End until the recordings come back; and what a returning keeper sees
    while the Broadcast tab looks for the station."""
    CUR["t"] = "keeper"
    d = f"{OUT}/files"; os.makedirs(d, exist_ok=True); TEMP.append(d); f = f"{d}/Sweep - Late Addition.wav"; tone_wav(f, 20, 48000, 700.0)
    prof = tempfile.mkdtemp(prefix="sweep-keeper2-", dir="/tmp"); TEMP.append(prof)
    ctx = persistent(pw, prof, 1440); pg = page(ctx); pg.goto(OPEN); connected(pg); view(pg, "studio")
    pg.fill("#st-name", "Sweep Keeper"); pg.fill("#st-about", "A keeper's station for the sweep"); press(pg, "#st-raise")
    poll(lambda: shown(pg, "#st-show"), 30); pg.check("#st-written"); press(pg, "#st-done")
    poll(lambda: txt(pg, "#st-status") and "ready" not in txt(pg, "#st-status"), 120, 0.1)
    sid = pg.evaluate("window.pyrate.mine().slice(-1)[0]"); note("an empty new station's status says " + repr(txt(pg, "#st-status")))
    lc = fresh(browser, 390); lp = page(lc); lp.goto(OPEN + "#" + sid); connected(lp)
    poll(lambda: txt(lp, "#name") == "Sweep Keeper", 60, 0.1); note(f"a listener's link to a station with nothing yet shows {txt(lp, '#name')!r}")
    press(lp, "#go"); time.sleep(3); note(f"Listen on it: the set says {txt(lp, '#np')!r}, the button {txt(lp, '#go .lbl')!r}, the tab title {lp.evaluate('document.title')!r}")
    t0 = time.time(); pg.set_input_files("#st-files", f); marks = {}
    while time.time() - t0 < 150 and len(marks) < 3:
        if "clock" not in marks and pg.locator(".qi.done").count(): marks["clock"] = time.time() - t0
        if "status" not in marks and txt(pg, "#st-status").startswith("On the network · "): marks["status"] = time.time() - t0
        if "heard" not in marks and (lp.evaluate("window.pyrate.plan()") or {}).get("kind") == "track" and st(lp).get("scheduled", 0) > 0: marks["heard"] = time.time() - t0
        time.sleep(0.2)
    timed("keeper: a 20 s recording from choosing to on the clock", marks.get("clock"))
    timed("keeper: from choosing it to the studio saying On the air", marks.get("status"), 10)
    timed("a listener already tuned to the empty station hears the new recording, from choosing it", marks.get("heard"), 10)
    if marks.get("heard"): hear(capture(lp, 4), "the new recording at the waiting listener", const={"L": 700.0, "R": 700.0})
    # Live for twenty seconds, then End: the listener's speaker and set, quarter second by quarter second.
    press(pg, "#st-live"); _, t = poll(lambda: "Live" in txt(lp, "#np") and st(lp).get("heard"), 90, 0.1)
    if not check(t is not None, "the listener hears the live broadcast"): return
    time.sleep(8)
    lp.evaluate("s => { window.__cap = (" + TIMELINE + ")(s); return 1 }", 60); time.sleep(4)
    end_at = lp.evaluate("Date.now()"); press(pg, "#st-live")
    a = lp.evaluate("window.__cap")
    dec = lambda s_: np.frombuffer(base64.b64decode(s_), dtype="<i2").astype(np.float32) / 32767
    x = (dec(a["l"]) + dec(a["r"])) / 2; base = a["at"] - a["T0"]; endt = (end_at - a["T0"]) / 1000
    segs = []
    for i in range(0, len(x) - SR // 2, SR // 2):
        seg = x[i:i + SR // 2]; r_ = float(np.sqrt(np.mean(seg ** 2))); tt = base / 1000 + i / SR - endt
        segs.append((round(tt, 1), round(r_, 3), round(peak(seg)) if r_ > 0.003 else 0))
    said = []
    for ms, kind, title, npx in a["tl"]:
        if not said or said[-1][1:] != (kind, npx): said.append((round(ms / 1000 - endt, 1), kind, npx))
    note("after End, what the listener's set said (s from End, plan, line): " + "; ".join(f"{t_:+.1f} {k} {n!r}" for t_, k, n in said))
    runs = []
    for t_, r_, f_ in segs:
        lab = "silence" if r_ < 0.003 else f"{f_} Hz"
        if runs and runs[-1][1] == lab: runs[-1][2] = t_
        else: runs.append([t_, lab, t_])
    note("after End, what the listener's speaker played (s from End): " + "; ".join(f"{a_:+.1f}..{b_:+.1f} {l_}" for a_, l_, b_ in runs))
    back = next((t_ for t_, r_, f_ in segs if t_ > 0 and abs(f_ - 700) < 5), None)
    gap = sum(0.5 for t_, r_, f_ in segs if t_ > 0 and r_ < 0.003 and (back is None or t_ < back))
    timed("from End to the listener hearing the recordings again", back, 18)
    check(gap <= 1.0, "no long silence between the live show and the recordings", f"{gap:.1f} s of silence")
    errors(pg, "keeper"); errors(lp, "the listener"); lc.close(); ctx.close()
    # The returning keeper: what the Broadcast tab shows while it looks for the station.
    ctx = persistent(pw, prof, 1440); pg = page(ctx); t0 = time.time(); pg.goto(OPEN); view(pg, "studio")
    time.sleep(max(0, 2.5 - (time.time() - t0)))
    note(f"returning keeper after {time.time() - t0:.1f} s: form {'shown' if shown(pg, '#st-new') else 'hidden'}, station card {'shown' if shown(pg, '#st-mine') else 'hidden'}, note {txt(pg, '#st-note')!r}, status {txt(pg, '#st-status')!r}")
    shot(pg, "keeper-returning-2s")
    _, t = poll(lambda: shown(pg, "#st-mine"), 60, 0.1)
    timed("returning keeper: from opening the player to the station card", time.time() - t0 if t is not None else None, 3)
    errors(pg, "returning keeper"); ctx.close()

def t_throttled(pw, browser):
    """A first visit as a phone on a real network would make it: the connection slowed in Chromium
    (4G, and slow 4G with the processor at a quarter speed), Listen pressed the moment a station shows."""
    for label, down, up, rtt, cpu in (("4G", 9e6, 1.5e6, 60, 1), ("slow 4G, phone CPU", 1.6e6, 0.75e6, 150, 4)):
        CUR["t"] = f"throttled {label}"
        ctx = fresh(browser, 390); pg = page(ctx); cdp = ctx.new_cdp_session(pg)
        cdp.send("Network.enable"); cdp.send("Network.emulateNetworkConditions", {"offline": False, "latency": rtt, "downloadThroughput": down / 8, "uploadThroughput": up / 8})
        if cpu > 1: cdp.send("Emulation.setCPUThrottlingRate", {"rate": cpu})
        t0 = time.time(); pg.goto(OPEN, wait_until="commit"); marks = {}
        while time.time() - t0 < 90 and len(marks) < 4:
            try: r = pg.evaluate("() => ({ shell: !!document.getElementById('set'), n: (document.getElementById('name') || {}).textContent || '', rows: document.querySelectorAll('.row:not(.skel)').length, sig: (document.getElementById('signal') || {}).textContent || '', h: window.pyrate ? window.pyrate.state().heard : false })")
            except Exception: time.sleep(0.05); continue
            el = time.time() - t0
            if r["shell"] and "shell" not in marks: marks["shell"] = el
            if r["n"].strip() and "name" not in marks:
                marks["name"] = el
                try: pg.click("#go", timeout=5000)
                except Exception: pass
            if r["rows"] and "row" not in marks: marks["row"] = el
            if r["h"] and "heard" not in marks: marks["heard"] = el
            time.sleep(0.05)
        timed(f"{label}: the page's frame is there", marks.get("shell"), 1)
        timed(f"{label}: a station's name on the set", marks.get("name"), 3)
        timed(f"{label}: the first row in the list", marks.get("row"), 3)
        timed(f"{label}: from opening the page to first sound, Listen pressed as soon as a station shows", marks.get("heard"), 5)
        res = pg.evaluate("() => performance.getEntriesByType('resource').map(e => [e.name.split('/').pop().split('?')[0], Math.round(e.responseEnd), e.transferSize]).filter(e => e[2] > 20000)")
        nav = pg.evaluate("() => { const n = performance.getEntriesByType('navigation')[0]; return [Math.round(n.responseEnd), n.transferSize] }")
        note(f"{label}: index.html {nav[1]} B done at {nav[0]} ms; large files: {res}")
        shot(pg, f"throttled-{label.split(',')[0].replace(' ', '')}")
        errors(pg, f"on {label}"); ctx.close()

def t_listener(pw, browser):
    for w in WIDTHS:
        for visitor in ("fresh", "returning"):
            CUR["t"] = f"listener {w} {visitor}"
            try: listener(pw, browser, w, visitor)
            except Exception as e: check(False, "the flow ran to the end", f"{type(e).__name__}: {str(e)[:300]}")

def listener(pw, browser, w, visitor):
    tag = f"{w}-{visitor}"
    if visitor == "returning":
        prof = tempfile.mkdtemp(prefix="sweep-listener-", dir="/tmp"); TEMP.append(prof)
        ctx = persistent(pw, prof, w); pg = page(ctx); pg.goto(OPEN); connected(pg); time.sleep(4)
        other = {"id": tune(pg, "Second Watch")}
        if other["id"]:
            time.sleep(2); press(pg, "#keep"); time.sleep(0.5)
        pg.select_option("#sort", "newest"); time.sleep(0.3)
        view(pg, "settings"); pg.uncheck("#sw-hiss"); time.sleep(0.3)
        SHARED[f"kept-{w}"] = other and other["id"]; SHARED[f"first-{w}"] = st(pg).get("station")
        ctx.close()
        ctx = persistent(pw, prof, w)
    else: ctx = fresh(browser, w)
    pg = page(ctx)
    # 1. Opening.
    t0 = time.time(); pg.goto(OPEN, wait_until="commit"); marks = {}; counts = []
    while time.time() - t0 < 45:
        try: r = pg.evaluate("() => ({ n: (document.getElementById('name') || {}).textContent || '', rows: document.querySelectorAll('.row:not(.skel)').length, skel: document.querySelectorAll('.row.skel').length, sig: (document.getElementById('signal') || {}).textContent || '' })")
        except Exception: time.sleep(0.03); continue
        el = time.time() - t0
        for k, c in (("shell", r["skel"] or r["n"] or r["rows"]), ("name", r["n"].strip()), ("row", r["rows"]), ("connected", "Connected" in r["sig"])):
            if c and k not in marks: marks[k] = el
        if not counts or counts[-1][1] != r["rows"]: counts.append((el, r["rows"]))
        if "connected" in marks and el - counts[-1][0] > 6: break
        time.sleep(0.03)
    timed("opening: the page shows its frame (rows drawn or a station)", marks.get("shell"))
    timed("opening: a station's name on the set", marks.get("name"))
    timed("opening: the first row in the list", marks.get("row"))
    timed("opening: 'Connected' in the signal line", marks.get("connected"), 2)
    timed("opening: the list stops changing (last change)", counts[-1][0] if counts else None, 3)
    perf = pg.evaluate("() => { const n = performance.getEntriesByType('navigation')[0], f = performance.getEntriesByName('first-contentful-paint')[0], w = performance.getEntriesByType('resource').find(e => /\\.wasm/.test(e.name)); return { dcl: n.domContentLoadedEventEnd, load: n.loadEventEnd, fcp: f && f.startTime, html: n.transferSize, wasm: w && w.responseEnd, wasmSize: w && w.transferSize } }")
    note(f"opening: first paint {perf['fcp'] and round(perf['fcp'])} ms, DOM ready {round(perf['dcl'])} ms, wasm fetched by {perf['wasm'] and round(perf['wasm'])} ms; html {perf['html']} B, wasm {perf['wasmSize']} B; rows over time {counts[:6]}")
    rs = rows(pg); names = [r["name"] for r in rs]
    note(f"the list: {len(rs)} stations: " + ", ".join(r["name"] + (f" [{'/'.join(r['tags'])}]" if r["tags"] else "") for r in rs[:12]))
    shot(pg, f"listener-{tag}-1-open"); layout(pg, "the set and the list after opening"); errors(pg, "while opening")
    if visitor == "returning":
        kid = SHARED.get(f"kept-{w}")
        on_set = st(pg).get("station")
        check(kid and rs and rs[0]["id"] == kid and "Kept" in rs[0]["tags"], "returning: the kept station is first in the list, tagged Kept", f"first row {rs[0]['name'] if rs else None}")
        note(f"returning: the set opens on {txt(pg, '#name')!r}" + (" (the kept station)" if on_set == kid else " (not the kept or last-played station)"))
        check(pg.input_value("#sort") == "newest", "returning: the list keeps its order (Newest)")
        check(not pg.is_checked("#sw-hiss"), "returning: the hiss setting is remembered")
        view(pg, "settings"); pg.check("#sw-hiss"); view(pg, "list"); pg.select_option("#sort", "aboard")
    # 2. Listening, as soon as the list is in.
    sid = st(pg).get("station"); t1 = time.time(); press(pg, "#go")
    _, t = poll(lambda: txt(pg, "#go .lbl") == "Stop", 3, 0.01); timed("Listen: the button says Stop", t, 0.2)
    t = until_heard(pg, sid); timed("Listen to first sound", t)
    hear(capture(pg, 8), "what Listen plays")
    m = pg.evaluate("navigator.mediaSession && navigator.mediaSession.metadata ? [navigator.mediaSession.metadata.title, navigator.mediaSession.metadata.artist] : null")
    check(m and m[0], "the phone's own controls name what plays", str(m))
    check(pg.evaluate("document.title").startswith("▶"), "the tab's title says it plays", pg.evaluate("document.title"))
    press(pg, "#go"); time.sleep(0.5); s = st(pg)
    check(not s.get("playing") and s.get("scheduled") == 0, "Stop stops at once", str({k: s.get(k) for k in ("playing", "scheduled")}))
    silent(pg, 2, "after Stop nothing reaches the speaker")
    time.sleep(1); check(st(pg).get("ctx") == "suspended", "and the sound card rests", st(pg).get("ctx"))
    press(pg, "#go"); time.sleep(0.25); press(pg, "#go"); time.sleep(3); s = st(pg)
    check(not s.get("playing") and s.get("scheduled") == 0, "Stop pressed while tuning in stays stopped")
    # 3. Switching: the list, the dial, Spin, the arrow keys and the media keys.
    press(pg, "#go"); until_heard(pg)
    silent_hits = []
    def switch(how, act, target=None):
        before = st(pg).get("station"); t0 = time.time(); act()
        _, tn = poll(lambda: st(pg).get("station") != before and (target is None or st(pg).get("station") == target), 5, 0.01)
        timed(f"switch by {how}: the set changes station", tn, 0.3)
        now = st(pg).get("station"); th = until_heard(pg, now); el = time.time() - t0
        kind = (pg.evaluate("window.pyrate.plan()") or {}).get("kind")
        if kind in ("dead air", "off air"):
            silent_hits.append(f"{how} -> {txt(pg, '#name')} ({txt(pg, '#np')!r})"); return now
        timed(f"switch by {how}: to the new station's sound", el if th is not None else None)
        if th is not None: hear(capture(pg, 5), f"after a switch by {how}")
        return now
    cur = st(pg).get("station"); other = next((n for n in SOUNDING if row_id(pg, n) and row_id(pg, n) != cur), None)
    if other:
        oid = row_id(pg, other); switch("the list", lambda: press(pg, f".row[data-id='{oid}']"), oid)
        cur = st(pg).get("station"); back = next((row_id(pg, n) for n in SOUNDING if row_id(pg, n) != cur), None)
        if back and pg.locator(f"#names button[data-id='{back}']").count(): switch("a name on the dial", lambda: press(pg, f"#names button[data-id='{back}']"), back)
        else: check(False, "the other sounding station has a name on the dial")
        switch("Spin", lambda: press(pg, "#spin"))
        if not phone(pg):
            pg.focus("#glass"); switch("the right arrow on the dial", lambda: pg.keyboard.press("ArrowRight"))
            pg.focus("#glass"); switch("the left arrow on the dial", lambda: pg.keyboard.press("ArrowLeft"))
        has = pg.evaluate("window.__ms ? Object.keys(window.__ms) : []")
        check({"nexttrack", "previoustrack", "play", "pause"} <= set(has), "the page answers the media keys", ", ".join(has))
        if "nexttrack" in has:
            switch("the Next media key", lambda: pg.evaluate("window.__ms.nexttrack()"))
            switch("the Previous media key", lambda: pg.evaluate("window.__ms.previoustrack()"))
            pg.evaluate("window.__ms.pause()"); time.sleep(0.4); check(not st(pg).get("playing"), "the Pause media key stops")
            t0 = time.time(); pg.evaluate("window.__ms.play()"); timed("the Play media key to sound", until_heard(pg))
        if not phone(pg):
            pg.evaluate("document.activeElement && document.activeElement.blur()"); pg.keyboard.press("k"); time.sleep(0.4)
            check(not st(pg).get("playing"), "K stops"); pg.keyboard.press("Space"); timed("Space plays again, to sound", until_heard(pg))
    else: check(False, "a second station with sound to switch to", ", ".join(r["name"] for r in rows(pg)))
    if silent_hits: note("switches that landed on a station with nothing to play: " + "; ".join(silent_hits))
    errors(pg, "while listening and switching")
    # 4. The station page, of a station with a picture and a log.
    tune(pg, "Gangway"); name = txt(pg, "#name"); t0 = time.time(); press(pg, "#pagego")
    _, t = poll(lambda: st(pg).get("view") == "page", 5, 0.01); timed("the Page button opens the station page", t, 0.3)
    _, t = poll(lambda: shown(pg, "#page"), 20, 0.05); timed("the station page: its log's schedule shows", time.time() - t0 if t else None)
    info = pg.evaluate("""() => ({ pname: pname.textContent, about: pabout.textContent, meta: pmeta.textContent, notices: notices.children.length, next: next.children.length, how: how.textContent,
      listings: listings.textContent.slice(0, 160), key: key.textContent.slice(0, 30), cover: !cover.hidden && cover.naturalWidth, banner: getComputedStyle(banner).backgroundImage.slice(0, 20), week: !!document.getElementById('week') })""")
    check(info["pname"] == name and "on the dial" in info["meta"], "the page is the station on the set", f"{info['pname']!r}; {info['meta']!r}")
    note(f"station page: about {info['about']!r}; {info['notices']} notices; {info['next']} up next; how {info['how']!r}; this week {info['listings']!r}; cover {info['cover']} px; banner {info['banner']!r}")
    check(info["cover"], "the station's picture shows on the set")
    if info["week"]: press(pg, "#week"); check(shown(pg, "#later"), "THE WHOLE WEEK opens the rest of the week")
    if pg.locator(".cal").count():
        with pg.expect_download(timeout=5000) as dl: press(pg, ".cal >> nth=0")
        body = open(dl.value.path()).read(); check("BEGIN:VCALENDAR" in body and "DTSTART" in body, "+ CAL gives a calendar file", dl.value.suggested_filename)
    shot(pg, f"listener-{tag}-4-page", full=True); layout(pg, "the station page")
    pg.go_back(); _, t = poll(lambda: st(pg).get("view") == "list", 3, 0.02); check(t is not None, "Back returns from the station page to the stations")
    press(pg, "#artgo"); check(poll(lambda: st(pg).get("view") == "page", 3, 0.02)[1] is not None, "the station's picture opens its page"); view(pg, "list")
    # 5. Share: the link, the code to scan, Copy.
    t0 = time.time(); press(pg, "#pass"); _, t = poll(lambda: shown(pg, "#passbox"), 3, 0.01); timed("Share opens", t, 0.3)
    link = txt(pg, "#plink"); sid = st(pg).get("station")
    check(re.match(r"^https://[^\s#]+/#[0-9a-f]{64}$", link) and link.endswith(sid), "the share link is well formed and names the station", link)
    try:
        import cv2
        pg.locator("#qr").screenshot(path=f"{OUT}/qr.png"); im = cv2.imread(f"{OUT}/qr.png"); im = cv2.resize(im, None, fx=3, fy=3, interpolation=cv2.INTER_NEAREST); im = cv2.copyMakeBorder(im, 20, 20, 20, 20, cv2.BORDER_CONSTANT, value=(255, 255, 255))
        got, _, _ = cv2.QRCodeDetector().detectAndDecode(im); os.remove(f"{OUT}/qr.png")
        check(got == link, "the QR code scans to the same link", got[:80])
    except ImportError: note("no OpenCV: the QR code was not scanned")
    press(pg, "#pcopy"); time.sleep(0.4)
    clip = pg.evaluate("navigator.clipboard.readText().catch(e => 'error: ' + e)")
    check(clip == link and txt(pg, "#pcopy") == "Copied", "Copy the link copies it and says so", clip[:80])
    note(f"Share button beside Copy: {'shown' if shown(pg, '#pshare') else 'hidden (no navigator.share here)'}")
    shot(pg, f"listener-{tag}-5-share"); layout(pg, "Share open")
    press(pg, "#pclose"); check(not shown(pg, "#passbox"), "Close closes Share")
    # 6. Keep.
    kept0 = txt(pg, "#keep span"); t0 = time.time(); press(pg, "#keep")
    _, t = poll(lambda: txt(pg, "#keep span") != kept0, 3, 0.01); timed("Keep changes the button", t, 0.3)
    if txt(pg, "#keep span") == "Kept":
        r0 = rows(pg)[0]; check(r0["id"] == sid and "Kept" in r0["tags"], "a kept station moves to the top of the list, tagged Kept", f"{r0['name']} {r0['tags']}")
        press(pg, "#keep"); check(txt(pg, "#keep span") == "Keep", "pressing again lets it go")
    # 7. More: the sleep timer and Recently played.
    t0 = time.time(); press(pg, "#more"); _, t = poll(lambda: shown(pg, "#morebox"), 3, 0.01); timed("More opens", t, 0.3)
    shot(pg, f"listener-{tag}-7-more"); layout(pg, "More open")
    press(pg, "#sleepbtns [data-m='30']")
    check(txt(pg, "#more span") == "Sleep" and txt(pg, "#sleepsay").startswith("until") and shown(pg, "#sleepoff"), "a 30-minute sleep timer is set and says when", f"{txt(pg, '#more span')!r} {txt(pg, '#sleepsay')!r}")
    press(pg, "#sleepoff"); check(txt(pg, "#more span") == "More" and not shown(pg, "#sleepoff"), "Off cancels the sleep timer")
    press(pg, "#playedgo"); _, t = poll(lambda: shown(pg, "#played"), 3, 0.01)
    played = pg.eval_on_selector_all("#played li", "l => l.map(x => x.textContent)")
    check(played and "Nothing yet" not in played[0], "Recently played lists what was heard", " | ".join(played[:4]))
    if phone(pg): pg.locator(".mh-sub.open-only").tap()
    else: pg.keyboard.press("Escape")
    time.sleep(0.3); check(not shown(pg, "#morebox"), "More closes " + ("on a tap outside" if phone(pg) else "with Escape"))
    # 8. Search and the kinds.
    def find(q):
        before = txt(pg, "#rows"); t0 = time.time(); pg.fill("#find", q)
        _, t = poll(lambda: txt(pg, "#rows") != before, 2, 0.01)
        if t is not None and t > 0.3: timed(f"search {q!r} updates the list", t, 0.3)
        return [r["name"] for r in rows(pg)], txt(pg, "#rows")
    all_names = [r["name"] for r in rows(pg)]
    got, _ = find("watch"); check(got and all("watch" in n.lower() or True for n in got) and "Second Watch" in got, "searching 'watch' finds Second Watch", ", ".join(got))
    got, _ = find("lisbon"); check("Second Watch" in got, "searching a home port (Lisbon) finds its station", ", ".join(got))
    got, _ = find("shanties"); check("Gangway" in got, "searching what a station says of itself finds it", ", ".join(got))
    f0 = txt(pg, "#freq"); got, _ = find(f0); check(len(got) >= 1, f"searching a place on the dial ({f0}) finds the station there", ", ".join(got))
    got, body = find("zzqx nothing"); check(not got and "No station on the air matches that." in body, "a search with no match says so", body[:60])
    t0 = time.time(); pg.fill("#find", ""); _, t = poll(lambda: len(rows(pg)) == len(all_names), 3, 0.01); timed("clearing the search brings the list back", t, 0.3)
    kinds = pg.eval_on_selector_all("#kinds button", "bs => bs.map(b => b.textContent)")
    note(f"kind filters offered: {kinds}")
    if kinds:
        k = kinds[0] if kinds[0] != "On air now" or len(kinds) == 1 else kinds[1]
        press(pg, f"#kinds button:has-text('{k}')"); time.sleep(0.2); got = [r["name"] for r in rows(pg)]
        check(0 < len(got) <= len(all_names), f"the kind filter {k!r} narrows the list", f"{len(got)} of {len(all_names)}")
        press(pg, f"#kinds button:has-text('{k}')"); time.sleep(0.2); check(len(rows(pg)) == len(all_names), "pressing the kind again shows every station")
    pg.select_option("#sort", "newest"); time.sleep(0.2); newest = [r["name"] for r in rows(pg)]
    pg.select_option("#sort", "aboard"); time.sleep(0.2)
    note(f"order by Newest: {newest[:6]}")
    t0 = time.time(); press(pg, "#vchart"); _, t = poll(lambda: pg.locator("#chart svg .dot").count() > 0, 15, 0.02); timed("Map shows the stations' home ports", t)
    note("map: " + txt(pg, "#chart .fine")); shot(pg, f"listener-{tag}-8-map"); layout(pg, "the map")
    if pg.locator("#chart .dot").count():
        d = pg.locator("#chart .dot").first; did = d.get_attribute("data-id"); t0 = time.time(); d.click()
        _, t = poll(lambda: st(pg).get("station") == did, 3, 0.01); check(t is not None, "a dot on the map tunes to its station")
    press(pg, "#vlist"); check(shown(pg, "#rows"), "List returns from the map")
    errors(pg, "on the station page, Share, Keep, More, search and the map")
    # 9. The mini player once the set scrolls away.
    pg.evaluate("document.body.style.minHeight = '4000px'; window.scrollTo(0, document.body.scrollHeight)")
    _, t = poll(lambda: shown(pg, "#mini"), 3, 0.05); check(t is not None, "the mini player shows once the set is out of view")
    shot(pg, f"listener-{tag}-9-mini"); layout(pg, "the mini player")
    if t is not None:
        was = st(pg).get("playing"); press(pg, "#mini-go"); time.sleep(0.4); check(st(pg).get("playing") != was, "the mini player's button plays and stops")
        if not st(pg).get("playing"): press(pg, "#mini-go"); until_heard(pg)
        press(pg, "#mini-up"); time.sleep(0.8)
    pg.evaluate("document.body.style.minHeight = ''; window.scrollTo(0, 0)")
    # 10. Settings, every control.
    tune(pg, "Gangway"); view(pg, "settings"); shot(pg, f"listener-{tag}-10-settings", full=True); layout(pg, "Settings")
    pg.evaluate("window.scrollTo(0, document.documentElement.scrollHeight)"); time.sleep(0.6)
    layout(pg, "the foot of Settings, scrolled to the end"); shot(pg, f"listener-{tag}-10-foot"); pg.evaluate("window.scrollTo(0, 0)")
    pics0 = (shown(pg, "#cover"), pg.eval_on_selector_all(".row .mini img", "l => l.length"))
    pg.uncheck("#sw-art"); time.sleep(1.5)
    imgs = pg.eval_on_selector_all(".row .mini img", "l => l.length")
    check(not shown(pg, "#cover") and imgs == 0, "Show station pictures off hides every picture", f"before: cover {pics0[0]}, {pics0[1]} row pictures; after: cover shown {shown(pg, '#cover')}, {imgs} row pictures")
    # Row pictures load as rows come into view, so they are looked for back on the list.
    pg.check("#sw-art"); view(pg, "list"); _, t = poll(lambda: shown(pg, "#cover") and pg.eval_on_selector_all(".row .mini img", "l => l.length") >= pics0[1], 10, 0.05)
    timed("Show station pictures on brings them back (on the list)", t); view(pg, "settings")
    ok_src = pg.evaluate("(() => { const c = document.getElementById('cover'); return !c.hidden && c.complete && c.naturalWidth > 0 })()")
    check(ok_src, "the picture on the set is a live image after pictures go off and on")
    # The studio test's mature and explicit stations, by id from this run or by name from an earlier one.
    longid = SHARED.get("long") or row_id(pg, "Sweep Long")
    if not SHARED.get("explicit"):
        pg.check("#sw-empty"); pg.check("#sw-explicit"); time.sleep(0.5); SHARED["explicit"] = row_id(pg, "Sweep Explicit"); pg.uncheck("#sw-explicit"); pg.uncheck("#sw-empty"); time.sleep(0.3)
    exid = SHARED.get("explicit")
    if longid:
        tags = next((r["tags"] for r in rows(pg) if r["id"] == longid), None)
        check(tags is not None and "Mature" in tags, "a mature station is listed, tagged Mature", str(tags))
        pg.check("#sw-mature"); time.sleep(0.4); check(all(r["id"] != longid for r in rows(pg)), "Hide mature stations hides it")
        pg.uncheck("#sw-mature"); time.sleep(0.4)
    else: note("no mature station from the studio test: Hide mature not checked")
    if exid:
        check(all(r["id"] != exid for r in rows(pg)), "an explicit station with nothing to play stays out of the list by default")
        pg.check("#sw-empty"); time.sleep(0.4); check(all(r["id"] != exid for r in rows(pg)), "showing empty stations still hides an explicit one")
        pg.check("#sw-explicit"); _, t = poll(lambda: any(r["id"] == exid for r in rows(pg)), 5, 0.05)
        tags = next((r["tags"] for r in rows(pg) if r["id"] == exid), [])
        check(t is not None and "Explicit" in tags, "Show explicit stations shows it, tagged Explicit", str(tags))
        check("Nothing to play yet" in tags, "Show stations with nothing to play yet shows an empty one, tagged so", str(tags))
        if t is not None:
            view(pg, "list"); press(pg, f".row[data-id='{exid}']"); time.sleep(1.5)
            note(f"the explicit station raised with four kinds shows: {txt(pg, '#pmeta')!r}; the set says {txt(pg, '#np')!r}")
            tune(pg, "Gangway"); view(pg, "settings")
        pg.uncheck("#sw-explicit"); pg.uncheck("#sw-empty"); time.sleep(0.4)
    else: note("no explicit station from the studio test: Show explicit not checked")
    pg.click("summary:has-text('Advanced')")
    pg.check("#sw-direct"); check(pg.evaluate("localStorage.getItem('pyrate.direct')") == "1", "Connect directly is kept when on")
    note("Advanced: " + txt(pg, "#v-settings details.fold:not(.keeps) p"))
    pg.uncheck("#sw-direct")
    victim = next((r for r in rows(pg) if r["id"] != st(pg).get("station")), None)
    press(pg, "#mlist"); check(shown(pg, "#mbox"), "Hidden stations opens the list")
    if victim:
        pg.fill("#mtext", f"{victim['id']}  # {victim['name']}\nnot a key\n"); press(pg, "#mtake")
        check(txt(pg, "#mnote") == "1 hidden.", "pasting a shared list hides its stations", txt(pg, "#mnote"))
        check(all(r["id"] != victim["id"] for r in rows(pg)), "the pasted station leaves the list")
        press(pg, "#mclear"); check(any(r["id"] == victim["id"] for r in rows(pg)), "Show every station again brings it back", txt(pg, "#mnote"))
        check(victim["id"] not in pg.input_value("#mtext"), "the hidden list on screen empties once every station is back", pg.input_value("#mtext")[:90].replace("\n", " / "))
        press(pg, "#mlist")
    errors(pg, "in Settings")
    # 11. Hide this station from More, and bring it back.
    view(pg, "list"); tune(pg, "Gangway"); hid = st(pg).get("station"); hname = txt(pg, "#name")
    press(pg, "#more"); t0 = time.time(); press(pg, "#mhide")
    _, t = poll(lambda: all(r["id"] != hid for r in rows(pg)), 5, 0.02); timed("Hide this station takes it off the list", t, 0.5)
    check(hid not in pg.eval_on_selector_all("#names button", "b => b.map(x => x.dataset.id)"), "and off the dial")
    time.sleep(4.5)
    check(st(pg).get("station") != hid and txt(pg, "#name") != hname, "the set moves off a station just hidden", f"set shows {txt(pg, '#name')!r}, hidden {hname!r}")
    note(f"after hiding, the set says {txt(pg, '#name')!r}, playing {st(pg).get('playing')}; the confirm read {getattr(pg, '_dialogs', [''])[-1]!r}")
    shot(pg, f"listener-{tag}-11-hidden")
    view(pg, "settings")
    if shown(pg, "#mbox"): press(pg, "#mlist")
    press(pg, "#mlist")
    check(hid in pg.input_value("#mtext"), "Settings lists the hidden station", pg.input_value("#mtext")[:120])
    press(pg, "#mclear"); _, t = poll(lambda: any(r["id"] == hid for r in rows(pg)), 6, 0.05)
    check(t is not None, "Show every station again unhides it")
    errors(pg, "hiding and unhiding")
    # 12. Forget this browser.
    if visitor == "returning" or w == WIDTHS[-1]:
        pg.check("#sw-direct"); press(pg, "#keep") if txt(pg, "#keep span") == "Keep" else None
        t0 = time.time(); press(pg, "#forget"); pg.wait_for_load_state("load"); time.sleep(1.5)
        left = pg.evaluate("Object.keys(localStorage).filter(k => k.startsWith('pyrate.'))")
        check(not pg.is_checked("#sw-direct") and txt(pg, "#keep span") != "Kept", "Forget this browser resets settings and kept stations", f"keys after the reload: {left}")
        errors(pg, "after Forget")
    ctx.close()

def t_chat(pw, browser):
    """Two tabs in one station's room: join, a line, cheers, a request, a voice message, mute, leave."""
    for w in WIDTHS:
        CUR["t"] = f"chat {w}"
        ca, cb = fresh(browser, w), fresh(browser, 1440 if w < 600 else 390)
        a, b = page(ca), page(cb)
        for p_ in (a, b): p_.goto(OPEN + "#" + GANG)
        for p_ in (a, b): connected(p_); poll(lambda: txt(p_, "#name") == "Gangway", 20); view(p_, "page")
        t0 = time.time(); press(a, "#chatgo"); _, t = poll(lambda: shown(a, "#chatroom"), 20, 0.02); timed("Join the chat to the room", t)
        press(b, "#chatgo"); poll(lambda: shown(b, "#chatroom"), 20)
        _, t = poll(lambda: re.search(r"[1-9]\d* others here", txt(a, "#chatwho")), 30, 0.1); timed("the first tab sees the second one join", time.time() - t0 if t else None, 3)
        note("who line: " + txt(a, "#chatwho"))
        a.fill("#myname", "Sweep Gull"); a.press("#myname", "Tab")
        line = f"Ahoy from the sweep {int(time.time()) % 10000}"
        a.fill("#sayt", line); t0 = time.time(); a.press("#sayt", "Enter")
        _, t = poll(lambda: line in txt(a, "#lines"), 5, 0.02); timed("a line shows in its own tab", t, 0.5)
        _, t = poll(lambda: line in txt(b, "#lines"), 20, 0.05); timed("a line reaches the other tab", t, 2)
        check("Sweep Gull" in txt(b, "#lines"), "the other tab sees the name chosen", txt(b, "#lines")[-120:])
        t0 = time.time(); press(b, "#cheer"); _, t = poll(lambda: "Cheers to what is on" in txt(a, "#lines"), 20, 0.05); timed("Cheers reaches the other tab", t, 2)
        press(b, "#askfor"); opts_ = b.eval_on_selector_all("#asktrack option", "o => o.map(x => x.textContent)")
        check(shown(b, "#askbox") and len(opts_) >= 2, "Ask for a song offers the station's library", ", ".join(opts_))
        if opts_:
            b.select_option("#asktrack", index=len(opts_) - 1); b.fill("#asknote", "for the watch"); t0 = time.time(); press(b, "#asksend")
            want = opts_[-1].split(",")[0]
            _, t = poll(lambda: f"Asks for {want}" in txt(a, "#lines"), 20, 0.05); timed("a request reaches the other tab", t, 2)
        press(a, "#hail"); _, t = poll(lambda: txt(a, "#hail") == "Send", 5, 0.05)
        check(t is not None, "Message the station starts recording from the microphone", txt(a, "#hail") + " / " + txt(a, "#chatwho"))
        time.sleep(3); t0 = time.time(); press(a, "#hail")
        _, t = poll(lambda: txt(a, "#chatwho").startswith("Sent") or ("rror" in txt(a, "#chatwho")), 30, 0.05)
        check(txt(a, "#chatwho").startswith("Sent"), "a three-second voice message is sent", txt(a, "#chatwho")); timed("Send to 'Sent'", t)
        time.sleep(2); note(f"the other tab's room after the voice message: {txt(b, '#lines')[-160:]!r}")
        shot(a, f"chat-{w}-room", full=True); layout(a, "the chat room")
        # Mute the voice that said this run's line (names are not unique: an earlier tab may have used the same one).
        who = b.locator("#lines .line", has_text=line).locator(".who")
        if who.count():
            same = b.eval_on_selector_all("#lines .line:not(.mine) .who", "l => l.map(x => x.textContent + '|' + x.dataset.v)")
            names = {}
            for x in same: n_, v_ = x.split("|", 1); names.setdefault(n_, set()).add(v_)
            if any(len(v) > 1 for v in names.values()): note("two different voices in the room wear the same name, told apart only by a tooltip: " + ", ".join(n_ for n_, v in names.items() if len(v) > 1))
            who.first.click(); time.sleep(0.5)
            check(line not in txt(b, "#lines"), "muting a voice hides its lines in this tab", getattr(b, "_dialogs", [""])[-1])
        t0 = time.time(); press(a, "#chatleave"); _, t = poll(lambda: shown(a, "#chatjoin"), 5, 0.02); timed("Leave the chat", t, 0.5)
        _, t = poll(lambda: "Looking for others" in txt(b, "#chatwho") or re.search(r"\b0 others", txt(b, "#chatwho")), 30, 0.2)
        note(f"after one tab leaves, the other says {txt(b, '#chatwho')!r} ({'within ' + str(round(t, 1)) + ' s' if t else 'still counting it after 30 s'})")
        errors(a, "in the chat (first tab)"); errors(b, "in the chat (second tab)")
        ca.close(); cb.close()

def patched(ctx, fast_sleep=False):
    """The test listens where the speaker does (after the hiss and the sleep fade join), and can
    make the sleep timer's minutes into fractions of a second. Nothing on disk changes."""
    done = {}
    def handle(route):
        r = route.fetch(); body = r.text(); b0 = body
        body = body.replace("gain.connect(tapped)", "bus.connect(tapped)")
        if fast_sleep: body = body.replace("}, min * 60000);", "}, min * 300);").replace("linearRampToValueAtTime(0, t + 30)", "linearRampToValueAtTime(0, t + 3)").replace("ctx && playing ? 30000 : 0", "ctx && playing ? 3000 : 0")
        done["ok"] = body != b0 and (not fast_sleep or "min * 300" in body)
        route.fulfill(response=r, body=body)
    ctx.route(re.compile(r".*/app\.js(\?.*)?$"), handle)
    return done

def t_tuning(pw, browser):
    """A link to a station nobody carries: what the set says while it searches, and the hiss."""
    w = WIDTHS[0]; CUR["t"] = f"tuning {w}"
    ctx = fresh(browser, w); ok = patched(ctx); pg = page(ctx)
    ghost = "ab" * 32
    pg.goto(OPEN + "#" + ghost); connected(pg)
    check(ok.get("ok"), "the test could listen where the speaker does")
    note(f"a passed-on link to an unknown station shows {txt(pg, '#name')!r}, call sign {txt(pg, '#cs')!r}")
    t0 = time.time(); press(pg, "#go"); time.sleep(1.2)
    note(f"while searching the set says {txt(pg, '#np')!r}, stage {st(pg).get('stage')}")
    a = capture(pg, 2); d = describe(a["x"]) if a else {"rms": 0}
    check(d["rms"] > 0.003, "the soft hiss plays while tuning in", f"rms {d['rms']:.4f}")
    view(pg, "settings"); pg.uncheck("#sw-hiss"); time.sleep(0.8)
    silent(pg, 1.5, "turning the hiss off silences it at once")
    pg.check("#sw-hiss"); view(pg, "list")
    _, t = poll(lambda: shown(pg, "#hint"), 20, 0.2); timed("a reason shows while it searches", time.time() - t0 if t else None, 11)
    note(f"after {time.time() - t0:.0f} s: {txt(pg, '#hint')!r}")
    _, t = poll(lambda: "No radio carrying" in txt(pg, "#hint"), 60, 0.5)
    note(f"after {time.time() - t0:.0f} s: {txt(pg, '#hint')!r}")
    shot(pg, f"tuning-{w}-searching")
    press(pg, "#go"); time.sleep(0.5); check(not st(pg).get("playing"), "Stop ends the search")
    errors(pg, "while searching for a station nobody carries")
    ctx.close()

def t_sleep(pw, browser):
    """The sleep timer runs out: the sound fades, the set stops, the level is back for next time.
    Minutes are made into fractions of a second in flight; the fade is ten times quicker."""
    w = WIDTHS[-1]; CUR["t"] = f"sleep {w}"
    ctx = fresh(browser, w); ok = patched(ctx, fast_sleep=True); pg = page(ctx)
    pg.goto(OPEN); connected(pg); time.sleep(2)
    check(ok.get("ok"), "the sleep timer could be hurried for the test")
    tune(pg, "Gangway"); time.sleep(1)
    lvl = describe(capture(pg, 1.5)["x"])["rms"]
    press(pg, "#more"); press(pg, "#sleepbtns [data-m='15']"); t0 = time.time()
    _, t = poll(lambda: not st(pg).get("playing"), 20, 0.05)
    check(t is not None, "the set stops when the sleep timer runs out", f"after {t and round(t, 1)} s (4.5 s timer, 3 s fade)")
    check(txt(pg, "#more span") == "More", "and More says More again", txt(pg, "#more span"))
    press(pg, "#go"); until_heard(pg); time.sleep(1.2)
    check(st(pg).get("station") == row_id(pg, "Gangway"), "the same station comes back on Listen")
    lvl2 = describe(capture(pg, 1.5)["x"])["rms"]
    check(lvl2 > 0.6 * lvl, "the next Listen plays at the old level", f"rms {lvl:.3f} before, {lvl2:.3f} after")
    errors(pg, "with the sleep timer"); ctx.close()

def t_pocket(pw, browser):
    """A private station by invitation: two browsers join from the keeper's link, hear it, talk."""
    CUR["t"] = "pocket"
    exe = f"{R}/target/release/pocket_test"
    if not os.path.exists(exe): return note("no target/release/pocket_test: pockets not checked")
    inv = f"{OUT}/invite.txt"
    if os.path.exists(inv): os.remove(inv)
    env = dict(os.environ, PYRATE_RELAYS="http://127.0.0.1:3340", PYRATE_BROWSER_INVITE=inv, PYRATE_BROWSER_WAIT="150")
    log = open(f"{OUT}/pocket_test.log", "w"); TEMP.append(f"{OUT}/pocket_test.log")
    proc = subprocess.Popen([exe, RELAY, f"{MUSIC}/gang/Test Band - Harbour Lights.wav"], env=env, stdout=log, stderr=subprocess.STDOUT)
    try:
        _, t = poll(lambda: os.path.exists(inv) and open(inv).read().strip(), 90, 0.5)
        if not check(t is not None, "the keeper's invitation is written"): return
        link = open(inv).read().strip(); frag = link[link.index("#"):]
        prof = tempfile.mkdtemp(prefix="sweep-pocket-", dir="/tmp"); TEMP.append(prof)
        ca = persistent(pw, prof, 390); cb = fresh(browser, 1440); a, b = page(ca), page(cb)
        t0 = time.time(); a.goto(OPEN + frag); b.goto(OPEN + frag); time.sleep(1)
        check("pocket=" not in url(a), "the invitation leaves the address bar at once")
        for p_, nm in ((a, "phone"), (b, "desktop")):
            _, t = poll(lambda: "Back Room" in txt(p_, "#name"), 60, 0.1); timed(f"the {nm} tab opens the private station from the invitation", time.time() - t0 if t else None, 5)
        check(not shown(a, "#pass") and not shown(a, "#keep"), "a private station offers no Share and no Keep")
        r = next((x for x in rows(a) if "Back Room" in x["name"]), None)
        check(r and "Private" in r["tags"], "it is listed, tagged Private", str(r))
        t0 = time.time(); press(a, "#go"); t = until_heard(a, timeout=90); timed("a private station: Listen to sound", t)
        p_ = a.evaluate("window.pyrate.plan()") or {}
        note(f"the private station on the set: {txt(a, '#name')!r}, line {txt(a, '#np')!r}, plan {p_.get('kind')}" + (f", back at {time.strftime('%H:%M:%S', time.localtime(p_['back_at'] / 1000))}" if p_.get('back_at') else "") + f"; now {time.strftime('%H:%M:%S')}")
        if p_.get("kind") in ("off air", "dead air"):
            _, t2 = poll(lambda: (a.evaluate("window.pyrate.plan()") or {}).get("kind") == "track" and st(a).get("scheduled", 0) > 0, 120, 0.5)
            timed("a private station: from Listen to its recording, once the set said it was off the air", time.time() - t0 if t2 is not None else None, 5)
            t = t2
        if t is not None: hear(capture(a, 6), "the private station")
        shot(a, "pocket-390-set"); layout(a, "a private station on the set")
        for p_ in (a, b): view(p_, "page"); press(p_, "#chatgo")
        for p_ in (a, b): poll(lambda: shown(p_, "#chatroom"), 30)
        a.fill("#sayt", "Ahoy from the browser"); t0 = time.time(); a.press("#sayt", "Enter")
        _, t = poll(lambda: "Ahoy from the browser" in txt(b, "#lines"), 30, 0.1); timed("a line in the private room reaches the other browser", t, 3)
        errors(a, "in the private station (phone)"); errors(b, "in the private station (desktop)")
        ca.close(); cb.close()
        ca = persistent(pw, prof, 390); a = page(ca); t0 = time.time(); a.goto(OPEN)
        _, t = poll(lambda: any("Back Room" in x["name"] for x in rows(a)), 60, 0.2)
        timed("returning: the private station is in the list again, from the kept invitation", t, 5)
        ca.close()
        _, t = poll(lambda: "the keeper hears the browser" in open(f"{OUT}/pocket_test.log").read(), max(10, 200 - (time.time() - os.path.getmtime(inv))), 1)
        k = [l for l in open(f"{OUT}/pocket_test.log").read().splitlines() if "keeper hears the browser" in l]
        check(k and k[0].startswith("PASS"), "the keeper (an app node) hears the browser in the private room", (k or ["no line"])[0][:120])
    finally:
        proc.kill(); proc.wait()
        for d in ("pocket-keeper", "pocket-member", "pocket-outsider"): shutil.rmtree(f"/tmp/{d}", ignore_errors=True)
        if os.path.exists(inv): os.remove(inv)

def t_flock(pw, browser):
    """Listening in step: a host's flock link opened in the open player, and in hostis.black's copy."""
    CUR["t"] = "flock"
    exe = f"{R}/target/release/flock_test"
    if not os.path.exists(exe): return note("no target/release/flock_test: flocks not checked")
    env = dict(os.environ, PYRATE_RELAYS="http://127.0.0.1:3340")
    proc = subprocess.Popen([exe, RELAY], env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    try:
        link = None; t0 = time.time()
        while time.time() - t0 < 40:
            l = proc.stdout.readline()
            if l.startswith("link "): link = l[5:].strip(); break
        if not check(link and "flock=" in link, "the host's flock link", (link or "")[:60]): return
        frag = link[link.index("#"):]
        ctx = fresh(browser, 390); pg = page(ctx); t0 = time.time(); pg.goto(OPEN + frag)
        _, t = poll(lambda: "in step with the host" in txt(pg, "#signal"), 30, 0.05); timed("a flock link: the set says it plays in step", t, 5)
        check("flock=" not in pg.evaluate("location.href"), "the flock link leaves the address bar")
        check(shown(pg, "#flockleave"), "Stop playing in step is offered", txt(pg, "#signal"))
        note(f"in a flock the set shows {txt(pg, '#name')!r}; signal {txt(pg, '#signal')!r}")
        shot(pg, "flock-390"); layout(pg, "in a flock")
        if shown(pg, "#flockleave"):
            press(pg, "#flockleave"); time.sleep(0.5)
            check(not shown(pg, "#flockleave") and "in step" not in txt(pg, "#signal"), "Stop playing in step leaves the flock")
        errors(pg, "in a flock"); ctx.close()
        ctx = fresh(browser, 390); pg = page(ctx); t0 = time.time(); pg.goto(HARB + frag)
        _, t = poll(lambda: url(pg).startswith(OPEN), 15, 0.05); timed("hostis.black's copy hands a flock link to the open player", t)
        ctx.close()
    finally:
        proc.kill(); proc.wait()
        for d in ("flock-host", "flock-guest"): shutil.rmtree(f"/tmp/{d}", ignore_errors=True)

def t_harbour(pw, browser):
    """hostis.black's copy: HOSTIS's stations only, no studio, everything else one tap away."""
    for w in WIDTHS:
        CUR["t"] = f"harbour {w}"
        ctx = fresh(browser, w); pg = page(ctx); t0 = time.time(); pg.goto(HARB, wait_until="commit")
        _, tn = poll(lambda: txt(pg, "#name"), 20, 0.02); timed("hostis.black copy: a station on the set", tn)
        _, tr = poll(lambda: rows(pg), 20, 0.02); timed("hostis.black copy: the first row", tr)
        connected(pg); time.sleep(1)
        names = [r["name"] for r in rows(pg)]
        check(names == ["Gangway"], "only HOSTIS's stations are listed", ", ".join(names))
        check(not shown(pg, ".tab[data-v=studio]") and not shown(pg, "#find") and not shown(pg, "#keep"), "no Broadcast tab, search or Keep")
        hrefs = pg.eval_on_selector_all("a.openlink", "l => l.map(a => a.href)")
        check(hrefs and all(h.startswith(OPEN) for h in hrefs), "every open player link goes to the open player", ", ".join(sorted(set(hrefs))))
        t0 = time.time(); press(pg, "#go"); t = until_heard(pg); timed("hostis.black copy: Listen to first sound", t)
        if t is not None: hear(capture(pg, 6), "hostis.black copy")
        view(pg, "page"); check(txt(pg, "#chatopen") and pg.get_attribute("#chatopen", "href").endswith("#" + GANG), "the chat points to this station's room in the open player", pg.get_attribute("#chatopen", "href"))
        press(pg, "#go"); view(pg, "list")
        press(pg, "#pass"); check(txt(pg, "#plink") == "https://hostis.black/pyrateradio/listen/#" + GANG, "Share gives the hostis.black link", txt(pg, "#plink")); press(pg, "#pclose")
        shot(pg, f"harbour-{w}", full=True); layout(pg, "hostis.black's copy")
        errors(pg, "on hostis.black's copy")
        ctx.close()
        # A link to a station that is not HOSTIS's goes straight to the open player.
        if not SHARED.get("watch"):
            c0 = fresh(browser, 1440); p0 = page(c0); p0.goto(OPEN); connected(p0); poll(lambda: row_id(p0, "Second Watch"), 20); SHARED["watch"] = row_id(p0, "Second Watch"); c0.close()
        other = SHARED.get("watch") or "cd" * 32
        ctx = fresh(browser, w); pg = page(ctx); t0 = time.time(); pg.goto(HARB + "#" + other)
        _, t = poll(lambda: url(pg).startswith(OPEN), 20, 0.05); timed("a stranger's station link on hostis.black lands in the open player", t)
        note(f"on the way, hostis.black's copy said {txt(pg, '.leavingnote')!r}" if t is None else "handed over")
        if t is not None and SHARED.get("watch"):
            _, t2 = poll(lambda: txt(pg, "#name") == "Second Watch", 30, 0.05); timed("and that station is on its set (from opening the hostis.black link)", time.time() - t0 if t2 else None, 3)
        errors(pg, "handing a link over"); ctx.close()

def t_landing(pw, browser):
    """The landing page, how it works and the handbook: every link and every button."""
    CUR["t"] = "landing"
    apk = "/tmp/pyrate-radio-0.9.14.apk"
    t0 = time.time(); r = subprocess.run([sys.executable, f"{R}/landing/build.py", apk, "0.9.14"], capture_output=True, text=True)
    if not check(r.returncode == 0, "the landing page builds", (r.stderr or r.stdout)[-200:]): return
    note(f"landing build took {time.time() - t0:.1f} s")
    root = f"{R}/landing/out"
    s = socket.socket(); s.bind(("127.0.0.1", 0)); port = s.getsockname()[1]; s.close()
    class Quiet(http.server.SimpleHTTPRequestHandler):
        def log_message(self, *a): pass
    srv = http.server.ThreadingHTTPServer(("127.0.0.1", port), functools.partial(Quiet, directory=root))
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    base = f"http://127.0.0.1:{port}/"
    def status(u):
        try:
            q = urllib.request.Request(u, method="HEAD"); return urllib.request.urlopen(q, timeout=10).status
        except urllib.error.HTTPError as e: return e.code
        except Exception as e: return str(e)[:40]
    try:
        for path in ("", "how-it-works/", "handbook/"):
            for w in WIDTHS:
                ctx = fresh(browser, w); pg = page(ctx); t0 = time.time(); pg.goto(base + path); pg.wait_for_load_state("load")
                note(f"{path or 'landing'} at {w}: loaded in {time.time() - t0:.2f} s, title {pg.title()!r}")
                shot(pg, f"landing-{(path or 'index').strip('/')}-{w}", full=True); layout(pg, f"{path or 'the landing page'}")
                if w == WIDTHS[0]:
                    links = pg.eval_on_selector_all("a[href]", "l => l.map(a => [a.getAttribute('href'), a.href, (a.textContent || a.getAttribute('aria-label') || '').trim().replace(/\\s+/g, ' ').slice(0, 40)])")
                    dead = []; ext = set()
                    for raw, absu, label in links:
                        u = urllib.parse.urlparse(absu)
                        if raw.startswith("#"):
                            if raw != "#" and not pg.evaluate("id => !!document.getElementById(id)", raw[1:]): dead.append(f"{label!r} -> {raw} (no such anchor)")
                            elif raw == "#": dead.append(f"{label!r} -> # (goes nowhere)")
                        elif u.scheme in ("http", "https") and u.netloc == f"127.0.0.1:{port}":
                            if not u.path.startswith("/listen") and status(absu.split("#")[0]) != 200: dead.append(f"{label!r} -> {raw} ({status(absu.split('#')[0])})")
                        elif u.scheme in ("http", "https"):
                            ext.add(u.netloc)
                            if not u.netloc or " " in absu: dead.append(f"{label!r} -> {raw} (malformed)")
                        elif u.scheme == "magnet":
                            if "xt=urn:btih:" not in absu: dead.append(f"{label!r} -> malformed magnet")
                        elif u.scheme not in ("mailto",): dead.append(f"{label!r} -> {raw} (scheme {u.scheme})")
                    check(not dead, f"every link on {path or 'the landing page'} goes somewhere ({len(links)} links)", "; ".join(dead[:6]))
                    note(f"{path or 'landing'}: external hosts linked: {sorted(ext)}")
                    nb = pg.locator("button:visible, summary:visible").count(); inert = []
                    for i in range(nb):
                        el = pg.locator("button:visible, summary:visible").nth(i)
                        try:
                            label = (el.text_content() or el.get_attribute("aria-label") or "").strip()[:40]
                            before = pg.evaluate("[document.body.innerHTML.length, location.href, document.querySelectorAll('[open]').length, [...document.querySelectorAll('*')].filter(e => !e.hidden && e.getBoundingClientRect().height > 0).length]")
                            el.scroll_into_view_if_needed(timeout=3000); el.evaluate("e => e.click()"); time.sleep(0.4)
                            after = pg.evaluate("[document.body.innerHTML.length, location.href, document.querySelectorAll('[open]').length, [...document.querySelectorAll('*')].filter(e => !e.hidden && e.getBoundingClientRect().height > 0).length]")
                            if after == before and not getattr(pg, "_dialogs", None): inert.append(label)
                            if pg.url.split("#")[0] != (base + path): pg.goto(base + path)
                        except Exception as e: inert.append(f"{label} ({str(e)[:40]})")
                    check(not inert, f"every button on {path or 'the landing page'} does something ({nb} buttons)", ", ".join(inert[:6]))
                errors(pg, f"on {path or 'the landing page'}")
                ctx.close()
    finally:
        srv.shutdown()

TESTS = {"studio": t_studio, "keeper": t_keeper, "listener": t_listener, "throttled": t_throttled, "chat": t_chat, "tuning": t_tuning, "sleep": t_sleep, "pocket": t_pocket, "flock": t_flock, "harbour": t_harbour, "landing": t_landing}

def main():
    want = A.tests or list(TESTS)
    for t in want:
        if t not in TESTS: sys.exit(f"no test {t!r}; tests: {' '.join(TESTS)}")
    for u in (OPEN, HARB):
        try: urllib.request.urlopen(u, timeout=5)
        except Exception as e: sys.exit(f"{u} does not answer ({e}): run tools/test/serve_test.sh")
    if not subprocess.run(["pgrep", "-f", "target/release/lantern"], capture_output=True).stdout: sys.exit("no Lantern is running: run tools/test/net_up.sh")
    with sync_playwright() as pw:
        browser = pw.chromium.launch(args=ARGS)
        for t in want:
            CUR["t"] = t; t0 = time.time(); print(f"==== {t}", flush=True)
            try: TESTS[t](pw, browser)
            except Exception as e: check(False, f"{t} ran to the end", f"{type(e).__name__}: {str(e)[:400]}")
            print(f"==== {t} took {time.time() - t0:.0f} s", flush=True)
        browser.close()
    if not A.keep:
        for p in TEMP: shutil.rmtree(p, ignore_errors=True) if os.path.isdir(p) else (os.path.exists(p) and os.remove(p))
    print("\n==== Timings (s; SLOW past the limit)")
    for t, what, secs, lim in TIMES:
        print(f"{'SLOW' if secs is None or secs > lim else '    '} {secs if secs is None else round(secs, 2)!s:>6}  {t:<24} {what}")
    bad = [r for r in RES if r[1] in ("FAIL", "SLOW")]
    print(f"\n==== {sum(r[1] == 'ok' for r in RES)} ok, {sum(r[1] == 'FAIL' for r in RES)} failed, {sum(r[1] == 'SLOW' for r in RES)} slow")
    for t, s, w in bad: print(f"{s:<4} [{t}] {w}")
    sys.exit(1 if any(r[1] == "FAIL" for r in RES) else 0)

if __name__ == "__main__":
    main()
