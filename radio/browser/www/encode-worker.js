// Opus for recordings put on the air from this tab, off the page's thread: libopus itself,
// in wasm (opus/), the same encoder the app uses. In: 48 kHz interleaved 16-bit blocks. Out:
// 20 ms packets, each after its length as a big-endian u16, the way a station's chunks hold
// them. No rights reserved.
importScripts('opus/opusscript_native_wasm.js');
const FRAME = 960, MAX_PACKET = 1276 * 3;
let ready = null, M = null, job = null;
// As the app's voice.rs: talk loses its mains hum (50 and 60 Hz and their harmonics, whose
// drift dates and places a recording), and a keeper who asks has the voice shifted.
const HUM = [50, 60, 100, 120, 150, 180, 200, 240, 250, 300, 350, 360], GRAIN = 1920;
function voice(dehum, semis) {
  const nb = dehum ? HUM.map(f => { const w = 2 * Math.PI * f / 48000, al = Math.sin(w) / 60, a0 = 1 + al;
    return { b0: 1 / a0, b1: -2 * Math.cos(w) / a0, b2: 1 / a0, a1: -2 * Math.cos(w) / a0, a2: (1 - al) / a0, x1: 0, x2: 0, y1: 0, y2: 0 } }) : [];
  const ratio = Math.pow(2, Math.max(-12, Math.min(12, semis || 0)) / 12), shift = Math.abs(ratio - 1) > 1e-6;
  const line = new Float64Array(4 * GRAIN), n = line.length; let at = 0, ph = 0;
  const tap = d => { let p = at - d; if (p < 0) p += n; const i = Math.floor(p) % n, f = p - Math.floor(p); return line[i] * (1 - f) + line[(i + 1) % n] * f };
  return pcm => {
    if (!nb.length && !shift) return;
    for (let k = 0; k < pcm.length; k++) {
      let x = pcm[k];
      for (const b of nb) { const y = b.b0 * x + b.b1 * b.x1 + b.b2 * b.x2 - b.a1 * b.y1 - b.a2 * b.y2; b.x2 = b.x1; b.x1 = x; b.y2 = b.y1; b.y1 = y; x = y }
      if (shift) {
        line[at] = x; const p2 = (ph + 0.5) % 1;
        x = tap(1 + ph * GRAIN) * Math.sin(Math.PI * ph) ** 2 + tap(1 + p2 * GRAIN) * Math.sin(Math.PI * p2) ** 2;
        ph = ((ph + (1 - ratio) / GRAIN) % 1 + 1) % 1; at = (at + 1) % n;
      }
      pcm[k] = Math.max(-32768, Math.min(32767, Math.round(x)));
    }
  };
}
function start() { return ready ??= Module({ locateFile: f => 'opus/' + f }).ready.then(m => (M = m)) }
function frame(pcm) {
  M.HEAP16.set(pcm, job.inPtr >> 1);
  const n = job.h._encode(job.inPtr, pcm.length, job.outPtr, FRAME);
  if (n < 0) throw new Error('the encoder refused a frame (' + n + ')');
  const out = new Uint8Array(n + 2); out[0] = n >> 8; out[1] = n & 255;
  out.set(M.HEAPU8.subarray(job.outPtr, job.outPtr + n), 2);
  job.parts.push(out); job.bytes += out.length; job.frames++;
  // Live: every six seconds of packets leaves as a chunk of its own, at once.
  if (job.live && job.frames === 300) postMessage({ live: true, out: take() });
}
function take() {
  const out = new Uint8Array(job.bytes); let at = 0;
  for (const p of job.parts) { out.set(p, at); at += p.length }
  job.parts = []; job.bytes = 0; job.frames = 0;
  return out;
}
onmessage = async e => {
  const m = e.data;
  try {
    await start();
    if (m.kind === 'begin') {
      // VOIP for talk, AUDIO for music, as the app chooses.
      const h = new M.OpusScriptHandler(48000, m.channels, m.talk ? 2048 : 2049);
      h._encoder_ctl(4002, m.bitrate);
      // Constant bitrate, as the app: every chunk the same size on the wire (OPUS_SET_VBR, 0).
      h._encoder_ctl(4006, 0);
      job = { voice: m.talk && m.channels === 1 ? voice(true, m.disguise) : null, live: !!m.live, h, ch: m.channels, inPtr: M._malloc(FRAME * m.channels * 2), outPtr: M._malloc(MAX_PACKET), pending: new Int16Array(0), parts: [], bytes: 0, frames: 0 };
      postMessage({ id: m.id, ok: true });
    } else if (m.kind === 'feed') {
      if (job.voice) job.voice(m.pcm);
      const all = new Int16Array(job.pending.length + m.pcm.length); all.set(job.pending); all.set(m.pcm, job.pending.length);
      const step = FRAME * job.ch; let at = 0;
      for (; at + step <= all.length; at += step) frame(all.subarray(at, at + step));
      job.pending = all.slice(at);
      postMessage({ id: m.id, ok: true, frames: job.frames });
    } else if (m.kind === 'end') {
      if (job.pending.length) { const last = new Int16Array(FRAME * job.ch); last.set(job.pending); frame(last) }
      const frames = job.frames, out = take();
      M.OpusScriptHandler.destroy_handler(job.h); M._free(job.inPtr); M._free(job.outPtr);
      job = null;
      postMessage({ id: m.id, ok: true, frames, out }, [out.buffer]);
    }
  } catch (err) { postMessage({ id: m.id, error: String(err && err.message || err) }) }
};
