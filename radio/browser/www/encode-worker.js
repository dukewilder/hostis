// Opus for recordings and live shows put on the air from this tab, off the page's thread: libopus itself,
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
// opusscript's _encode takes its input one byte to each 16-bit slot, low byte first, and packs
// the pairs back into samples itself (src/opusscript_encoder.cpp). Handed plain samples, it
// packs two samples into one, and the air is noise an octave up.
function frame(pcm) {
  const n2 = pcm.length, wide = job.wide;
  for (let k = 0; k < n2; k++) { const v = pcm[k] & 0xffff; wide[2 * k] = v & 255; wide[2 * k + 1] = v >> 8 }
  M.HEAPU16.set(wide.subarray(0, 2 * n2), job.inPtr >> 1);
  const n = job.h._encode(job.inPtr, n2, job.outPtr, FRAME);
  if (n < 0) throw new Error('the encoder refused a frame (' + n + ')');
  const out = new Uint8Array(n + 2); out[0] = n >> 8; out[1] = n & 255;
  out.set(M.HEAPU8.subarray(job.outPtr, job.outPtr + n), 2);
  job.parts.push(out); job.bytes += out.length; job.frames++;
  // Live: every six seconds of packets leaves as a chunk of its own, at once.
  if (job.live && job.frames === 300) postMessage({ live: true, out: take(), channels: job.ch, bitrate: job.bitrate });
}
function take() {
  const out = new Uint8Array(job.bytes); let at = 0;
  for (const p of job.parts) { out.set(p, at); at += p.length }
  job.parts = []; job.bytes = 0; job.frames = 0;
  return out;
}
function open(m) {
  // VOIP for talk, AUDIO for music, as the app chooses.
  const h = new M.OpusScriptHandler(48000, m.channels, m.talk ? 2048 : 2049);
  h._encoder_ctl(4002, m.bitrate);
  // Constant bitrate, as the app: every chunk the same size on the wire (OPUS_SET_VBR, 0).
  h._encoder_ctl(4006, 0);
  // The desk cleans the microphone itself, on its own path, so music never passes through voice().
  job = { voice: m.talk && m.channels === 1 && !m.desk ? voice(true, m.disguise) : null, live: !!m.live, desk: !!m.desk, h, ch: m.channels, bitrate: m.bitrate, inPtr: M._malloc(FRAME * m.channels * 4), wide: new Uint16Array(FRAME * m.channels * 2), mono: new Int16Array(FRAME), outPtr: M._malloc(MAX_PACKET), pending: new Int16Array(0), parts: [], bytes: 0, frames: 0 };
}
function close() { M.OpusScriptHandler.destroy_handler(job.h); M._free(job.inPtr); M._free(job.outPtr); job = null }
// The live desk: its worklet sends the air along a port of its own as it is made, 48 kHz stereo,
// interleaved, so nothing waits on the page. Talk is folded to mono here. What is left over
// between frames carries across a change of encoder, so a switch to music loses no sound.
let pend = new Int16Array(0);
function air(st) {
  if (!job) return;
  const all = new Int16Array(pend.length + st.length); all.set(pend); all.set(st, pend.length);
  const step = FRAME * 2; let at = 0;
  for (; at + step <= all.length; at += step) {
    const s = all.subarray(at, at + step);
    if (job.ch === 1) { const m = job.mono; for (let k = 0; k < FRAME; k++) m[k] = (s[2 * k] + s[2 * k + 1]) >> 1; frame(m) } else frame(s);
  }
  pend = all.slice(at);
}
// The last of a broadcast: the part-frame made up with quiet, and whatever part-chunk is left.
function airEnd(id) {
  if (!job) return;
  if (pend.length) { const last = new Int16Array(FRAME * 2); last.set(pend); pend = new Int16Array(0); air(last) }
  const frames = job.frames, channels = job.ch, bitrate = job.bitrate, out = take();
  close();
  postMessage({ id, ok: true, last: true, frames, out, channels, bitrate }, [out.buffer]);
}
onmessage = async e => {
  const m = e.data;
  try {
    await start();
    if (m.kind === 'begin') {
      open(m);
      postMessage({ id: m.id, ok: true });
    } else if (m.kind === 'port') {
      m.port.onmessage = ev => { try { if (ev.data instanceof Int16Array) air(ev.data); else if (ev.data && ev.data.end) airEnd(0) } catch (err) { postMessage({ error: String(err && err.message || err) }) } };
    } else if (m.kind === 'switch') {
      // Music joins a talk show: the chunk so far leaves as it is, and the next starts in stereo.
      if (job) { const live = job.live; if (job.frames) postMessage({ live: true, out: take(), channels: job.ch, bitrate: job.bitrate }); close(); open({ ...m, live, desk: true }) }
      postMessage({ id: m.id, ok: true });
    } else if (m.kind === 'end' && m.desk) {
      // Asked for when the worklet's own last word never came: the same ending, or word it is done.
      if (job) airEnd(m.id); else postMessage({ id: m.id, ok: true, last: true, out: new Uint8Array(0) });
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
      close();
      postMessage({ id: m.id, ok: true, frames, out }, [out.buffer]);
    }
  } catch (err) { postMessage({ id: m.id, error: String(err && err.message || err) }) }
};
