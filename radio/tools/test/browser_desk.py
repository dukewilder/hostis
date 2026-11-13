"""The live desk, heard from another tab: browser_desk.py [port]
Serve a test build first (serve_test.sh). A tab raises a station and broadcasts from the desk with
Chromium's fake microphone (a 396 Hz beep, twice a second); a second tab tunes in, and what reaches
its speaker is measured. The keeper's show runs in phases a few seconds apart: the microphone
alone, the microphone disguised, the microphone closed, music from the deck (a 523 Hz tone), the
music under the open mic, the music back, a pad (880 Hz), then a second input. The disguise stays
on from its phase to the end, so the music shows it never touches music. Checks that each is
heard as it should be,
that nothing of the desk reaches the keeper's own speakers, that going live stops the keeper's tab
playing its own station, and that ending lets go of every device and leaves the air. Screenshots
of the desk at 1440 and 390 wide go to /tmp/desk-1440.png and /tmp/desk-390.png.
Prints PASS or FAIL for each check and exits non-zero on any failure."""
import sys, time, os, wave, base64, tempfile, shutil, math
import numpy as np
from playwright.sync_api import sync_playwright
port = sys.argv[1] if len(sys.argv) > 1 else "8099"
URL = f"http://127.0.0.1:{port}/"
ok = True
def check(c, what):
    global ok; ok &= bool(c); print(("PASS " if c else "FAIL ") + what, flush=True)
# The page runs under its own Content-Security-Policy, as served, so the test polls from outside
# instead of asking the page to evaluate a function of its own.
def wait(pg, js, secs):
    end = time.time() + secs
    while time.time() < end:
        if pg.evaluate(js): return
        time.sleep(0.25)
    raise TimeoutError(js)

tmp = tempfile.mkdtemp(prefix="desk-")
def tone(name, f, secs, amp=0.3):
    path = os.path.join(tmp, name); w = wave.open(path, "wb"); w.setnchannels(1); w.setsampwidth(2); w.setframerate(48000)
    w.writeframes((amp * 32767 * np.sin(2 * np.pi * f * np.arange(int(48000 * secs)) / 48000)).astype("<i2").tobytes()); w.close()
    return path
MUSIC = tone("Test Band - Middle C.wav", 523.25, 70)
PAD = tone("Ident.wav", 880, 3)

# Every device the keeper's tab opens, and every connection any of its sound makes to its speakers.
HOOKS = """
window.__csp = []; document.addEventListener('securitypolicyviolation', e => window.__csp.push(e.violatedDirective + ' ' + e.blockedURI));
window.__tracks = [];
for (const k of ['getUserMedia', 'getDisplayMedia']) { const f = MediaDevices.prototype[k]; if (f) MediaDevices.prototype[k] = async function (c) { const s = await f.call(this, c); window.__tracks.push(...s.getTracks()); return s } }
window.__dest = [];
const con = AudioNode.prototype.connect, dis = AudioNode.prototype.disconnect;
AudioNode.prototype.connect = function (d, ...r) { if (d instanceof AudioDestinationNode) window.__dest.push({ ctx: this.context, on: true }); return con.call(this, d, ...r) };
AudioNode.prototype.disconnect = function (...a) { if (a[0] instanceof AudioDestinationNode) window.__dest.push({ ctx: this.context, on: false }); return dis.apply(this, a) };
"""
DESK_TO_SPEAKERS = "(() => { const c = window.pyrate.deskCtx(); let n = 0; for (const x of window.__dest) if (x.ctx === c) n += x.on ? 1 : -1; return n })()"
CAPTURE = """() => { window.__cap = []; window.__capT0 = null; const st = window.pyrate.tap(), ac = new AudioContext({ sampleRate: 48000 });
  const src = ac.createMediaStreamSource(st), pr = ac.createScriptProcessor(4096, 2, 1); src.connect(pr); pr.connect(ac.destination);
  pr.onaudioprocess = e => { if (window.__capT0 === null) window.__capT0 = Date.now() - 4096 / 48; window.__cap.push(Float32Array.from(e.inputBuffer.getChannelData(0))) };
  window.__capStop = () => { pr.onaudioprocess = null; src.disconnect(); ac.close() } }"""
TAKE = """() => { window.__capStop(); const n = window.__cap.reduce((s, x) => s + x.length, 0), out = new Int16Array(n); let at = 0;
  for (const x of window.__cap) { for (let i = 0; i < x.length; i++) out[at + i] = Math.max(-32768, Math.min(32767, Math.round(x[i] * 8192))); at += x.length }
  const b = new Uint8Array(out.buffer); let s = ''; for (let i = 0; i < b.length; i += 32768) s += String.fromCharCode(...b.subarray(i, i + 32768)); return [window.__capT0, btoa(s)] }"""

errs = []
try:
    with sync_playwright() as p:
        b = p.chromium.launch(args=["--autoplay-policy=no-user-gesture-required", "--use-fake-ui-for-media-stream", "--use-fake-device-for-media-stream"])
        ctx = b.new_context(permissions=["microphone"], viewport={"width": 1440, "height": 900})
        ctx.add_init_script(HOOKS); a = ctx.new_page()
        for pg in (a,):
            pg.on("pageerror", lambda e: errs.append("keeper: " + str(e))); pg.on("dialog", lambda d: d.accept())
        a.goto(URL); wait(a, "document.getElementById('signal').textContent.includes('Connected')", 90)
        a.click(".tab[data-v=studio]"); a.fill("#st-name", "Desk Test"); a.click("#st-raise"); a.wait_for_selector("#st-show:not([hidden])")
        a.check("#st-written"); a.click("#st-done")
        wait(a, "!document.getElementById('st-status').textContent.includes('ready') && document.getElementById('st-status').textContent.length > 0", 120)
        sid = a.evaluate("window.pyrate.mine()[0]")
        # The keeper's tab plays its own station, then goes live with the microphone alone.
        a.click("#st-tune"); time.sleep(1.5)
        playing_before = a.evaluate("window.pyrate.state().playing")
        now = lambda: a.evaluate("Date.now()") / 1000
        T = {}
        a.click(".tab[data-v=studio]"); a.click("#st-live")
        wait(a, "window.pyrate.desk().live", 30); T["live"] = now(); time.sleep(1)
        check(playing_before and not a.evaluate("window.pyrate.state().playing"), "going live stops the tab playing its own station")
        check("stopped playing" in (a.text_content("#st-livenote") or ""), "and says so: " + repr(a.text_content("#st-livenote")))
        check("ON AIR" in (a.text_content("#st-livetime") or ""), "the desk is on the air: " + repr(a.text_content("#st-livetime")))
        d = a.evaluate("window.pyrate.desk()")
        check(d["mic"] and d["open"] and not d["music"], "a microphone alone goes out as talk (mono, 24 kbps)")
        a.click("#st-tune"); time.sleep(1)
        check(not a.evaluate("window.pyrate.state().playing"), "Listen on its own station stays off while live: " + repr(a.text_content("#np")))
        a.click(".tab[data-v=studio]")
        check(a.evaluate(DESK_TO_SPEAKERS) == 0, "nothing of the desk is connected to the keeper's speakers")
        # A second tab tunes in and hears it live.
        c = ctx.new_page(); c.on("pageerror", lambda e: errs.append("listener: " + str(e)))
        c.goto(URL + "#" + sid); c.wait_for_selector("#go:not([hidden])", timeout=90000); time.sleep(2); c.click("#go")
        t = time.time()
        wait(c, "document.getElementById('np').textContent.includes('Live') && window.pyrate.state().heard", 180)
        print("live heard after", round(time.time() - t, 1), "s", flush=True)
        c.evaluate(CAPTURE)
        T["A"] = now(); time.sleep(10)
        # The disguise goes on and stays on: the voice shifts, and the music to come must not.
        a.click("#dk-disguise"); T["V"] = now()
        check(a.is_checked("#st-disguise"), "the desk's Disguise is the studio's own switch")
        time.sleep(10)
        a.click("#dk-mic"); T["B"] = now()
        check(not a.evaluate("window.pyrate.desk()").get("open"), "the mic closes")
        a.set_input_files("#dk-files", [MUSIC])
        with a.expect_file_chooser() as fc: a.click("#dk-pads .dk-pad:nth-child(1) .go")
        fc.value.set_files(PAD)
        wait(a, "window.pyrate.desk().pads[0] === 'Ident' && window.pyrate.desk().deck.tracks === 1", 20)
        time.sleep(max(0, T["B"] + 10 - now()))
        a.click("#dk-play"); T["C"] = now(); time.sleep(1)
        d = a.evaluate("window.pyrate.desk()")
        check(d["deck"]["playing"] and d["music"], "the deck plays and the show turns to music (stereo, 64 kbps)")
        time.sleep(max(0, T["C"] + 12 - now()))
        # Space opens the mic, even with the deck's Play button focused, and leaves the deck playing.
        a.keyboard.press("Space"); T["D"] = now(); time.sleep(0.6)
        d = a.evaluate("window.pyrate.desk()")
        check(d["open"] and d["deck"]["playing"], "Space opens the mic and leaves the deck alone")
        check(d["duck"] is not None and d["duck"] < 0.5, "the music ducks under the open mic (gain %s)" % d["duck"])
        time.sleep(max(0, T["D"] + 12 - now()))
        a.keyboard.press("Space"); T["E"] = now(); time.sleep(max(0, T["E"] + 12 - now()))
        a.click("#dk-play"); a.keyboard.press("Digit1"); T["F"] = now(); time.sleep(0.5)
        check(a.evaluate("window.pyrate.desk().deck.playing") is False, "the deck pauses")
        time.sleep(max(0, T["F"] + 8 - now()))
        # Hearing the mix connects it to the speakers, and turning it off takes it away again.
        a.check("#dk-hear"); time.sleep(0.5); on = a.evaluate(DESK_TO_SPEAKERS)
        a.uncheck("#dk-hear"); time.sleep(0.3); off = a.evaluate(DESK_TO_SPEAKERS)
        check(on == 1 and off == 0, f"Hear the mix reaches the speakers only while on ({on}, then {off})")
        # A second input: another of the fake microphones.
        a.click(".dk-more summary")
        opts = a.eval_on_selector_all("#dk-twosrc option", "os => os.map(o => o.value)")
        dev = [v for v in opts if v and v != "tab"]
        if dev:
            a.select_option("#dk-twosrc", dev[-1]); T["G"] = now()
            wait(a, "!!window.pyrate.desk().two", 15)
        check(bool(dev) and a.is_visible("#dk-two"), "a second input opens with a strip of its own")
        time.sleep(1.2)
        # The page's own sticky tabs and mini player would lie over the desk in a picture of it.
        a.evaluate("document.querySelectorAll('.tabs,#mini').forEach(e => e.style.visibility = 'hidden')")
        # A screen tall enough to hold the desk whole: a picture stitched from pieces would carry
        # the page's fixed vignette into it in bands.
        a.set_viewport_size({"width": 1440, "height": 1100}); time.sleep(0.6)
        a.locator("#desk").screenshot(path="/tmp/desk-1440.png")
        a.set_viewport_size({"width": 390, "height": 1600}); time.sleep(0.8)
        a.locator("#desk").screenshot(path="/tmp/desk-390.png")
        a.set_viewport_size({"width": 1440, "height": 900}); time.sleep(0.4)
        a.evaluate("document.querySelectorAll('.tabs,#mini').forEach(e => e.style.visibility = '')")
        # The end: every device let go at once, and the station leaves the air for listeners.
        a.click("#st-live"); T["end"] = now(); time.sleep(1)
        tracks = a.evaluate("window.__tracks.map(t => t.readyState)")
        check(len(tracks) >= 2 and all(s == "ended" for s in tracks), f"ending releases every device ({len(tracks)} tracks: {tracks})")
        check(not a.evaluate("window.pyrate.desk().live") and not (a.text_content("#st-livetime") or ""), "the desk is off the air")
        # The end is signed into the log after the last chunk, so a listener leaves Live at once.
        t_off = None
        while now() - T["end"] < 15:
            if c.evaluate("window.pyrate.plan()?.kind !== 'live' || !document.getElementById('np').textContent.includes('Live from the station')"): t_off = now() - T["end"]; break
            time.sleep(0.25)
        check(t_off is not None and t_off < 10, f"a listener leaves Live within 10 s of the end ({t_off and round(t_off, 1)} s)")
        time.sleep(max(0, T["end"] + 24 - now()))
        t0, raw = c.evaluate(TAKE)
        csp = a.evaluate("window.__csp") + c.evaluate("window.__csp")
        b.close()

    # What the listener heard, phase by phase.
    # Kept at a quarter scale on the way out, so a peak over full scale shows as it is.
    x = np.frombuffer(base64.b64decode(raw), dtype=np.int16).astype(np.float32) / 8192
    sr = 48000; t0 /= 1000
    print(f"heard {len(x) / sr:.1f} s", flush=True)
    def seg(a0, a1): i, j = int(max(0, a0) * sr), int(max(0, a1) * sr); return x[i:j]
    def spec(s): w = np.hanning(len(s)); return np.abs(np.fft.rfft(s * w)) / (w.sum() / 2), np.fft.rfftfreq(len(s), 1 / sr)
    def band(s, f, wd=6):
        if len(s) < sr // 2: return 0.0
        m, fq = spec(s); return float(m[(fq > f - wd) & (fq < f + wd)].max())
    def top(s): m, fq = spec(s); m[fq < 60] = 0; return float(fq[np.argmax(m)])
    def hf(s): m, fq = spec(s); p = m ** 2; return float(p[fq > 4000].sum() / max(1e-12, p.sum()))
    # The listener's delay, read from the moment the music starts.
    win = int(0.25 * sr); env = np.array([band(x[i:i + sr // 2], 523.25, 8) if i + sr // 2 <= len(x) else 0 for i in range(0, len(x), win)])
    on_i = int(np.argmax(env > 0.5 * env.max())) if env.max() > 0.01 else None
    D = (t0 + on_i * 0.25) - T["C"] if on_i is not None else None
    print("the listener is", D and round(D, 1), "s behind", flush=True)
    check(D is not None and 4 < D < 40, "the music is heard (and times the delay)")
    if D is not None:
        at = lambda k, off=0: T[k] + D - t0 + off
        A, V, B, C, Dk, E, F = seg(at("A", 1.5), at("V", -1.5)), seg(at("V", 1.5), at("B", -1.5)), seg(at("B", 1.5), at("C", -1.0)), seg(at("C", 2.5), at("D", -1.5)), seg(at("D", 1.5), at("E", -1.5)), seg(at("E", 2.0), at("F", -1.5)), seg(at("F", 0.4), at("F", 2.6))
        # Four semitones under the voice as the listener heard it undisguised.
        SHIFT = (top(A) if len(A) > sr else 396) * 2 ** (-4 / 12)
        pk = lambda s: float(np.abs(s).max()) if len(s) else 0.0
        rms = lambda s: float(np.sqrt(np.mean(s ** 2))) if len(s) else 0.0
        print("phases, seconds and peak: " + ", ".join(f"{k} {len(s) / sr:.1f} {pk(s):.3f}" for k, s in zip("AVBCDEF", (A, V, B, C, Dk, E, F))), flush=True)
        check(len(A) > sr * 4 and abs(top(A) - 396) < 15, f"mic alone: the listener hears the microphone's tone ({top(A) if len(A) else 0:.0f} Hz)")
        check(len(A) > 0 and hf(A) < 0.05, f"mic alone: nothing piles up above 4 kHz ({hf(A) * 100 if len(A) else 0:.2f}% of the energy)")
        check(len(A) > 0 and pk(A) < 1.0, f"mic alone: the peak stays under full scale ({pk(A):.3f})")
        check(len(V) > sr * 4 and abs(top(V) - SHIFT) < 15 and band(V, 396) < 0.3 * band(A, 396), f"disguise: the voice is shifted down ({top(V) if len(V) else 0:.0f} Hz, near {SHIFT:.0f})")
        check(len(B) > sr * 4 and rms(B) < 0.003 and band(B, 396) < 0.01 * max(1e-9, band(A, 396)), f"mic closed: no microphone on the air (rms {rms(B):.4f}, tone {band(B, 396):.5f} against {band(A, 396):.4f})")
        check(len(C) > sr * 4 and abs(top(C) - 523.25) < 10, f"music: the listener hears the deck, untouched by the disguise ({top(C) if len(C) else 0:.0f} Hz)")
        # The music's level in each phase: the middle of its half-second readings, so a moment
        # the listener stalled, or a phase edge read a second early or late, does not move it.
        def level(k0, o0, k1, o1):
            i0, i1 = int(math.ceil((T[k0] + D - t0 + o0) / 0.25)), int((T[k1] + D - t0 + o1) / 0.25) - 2
            v = env[max(0, i0):max(0, i1)]; return float(np.median(v)) if len(v) else 0.0, v
        (cm, cv), (dm, dv), (em, ev) = level("C", 2.5, "D", -1.5), level("D", 1.5, "E", -1.5), level("E", 2.0, "F", -1.5)
        gaps = int((cv < 0.1 * cm).sum() + (ev < 0.1 * cm).sum())
        print(f"music level: {cm:.3f} alone, {dm:.3f} under the mic, {em:.3f} after; {gaps} quiet readings where it should play", flush=True)
        check(cm > 0 and dm / cm <= 0.5, f"ducking: the music drops under the open mic ({dm / max(cm, 1e-9):.2f} of its level)")
        check(len(Dk) > 0 and band(Dk, SHIFT) > 0.1 * band(V, SHIFT), "ducking: the mic is heard over it")
        check(cm > 0 and em / cm >= 0.8, f"ducking: the music comes back when the mic closes ({em / max(cm, 1e-9):.2f})")
        check(len(F) > sr and abs(top(F) - 880) < 10, f"a pad fires and is heard ({top(F) if len(F) else 0:.0f} Hz)")
        w = t0 + int(np.argmax(np.abs(x))) / sr - D; k = max((k for k in T if T[k] <= w), key=lambda k: T[k], default=None)
        check(pk(x) < 1.0, f"the air never clips (peak {pk(x):.3f}, {k and round(w - T[k], 1)} s into phase {k})")
    check(not csp, "nothing is refused by the page's own policy: %s" % csp[:3])
    check(not errs, "no page errors: %s" % errs[:3])
finally:
    shutil.rmtree(tmp, ignore_errors=True)
sys.exit(0 if ok else 1)
