// Opus for recordings put on the air from this tab, off the page's thread: libopus itself,
// in wasm (opus/), the same encoder the app uses. In: 48 kHz interleaved 16-bit blocks. Out:
// 20 ms packets, each after its length as a big-endian u16, the way a station's chunks hold
// them. No rights reserved.
importScripts('opus/opusscript_native_wasm.js');
const FRAME = 960, MAX_PACKET = 1276 * 3;
let ready = null, M = null, job = null;
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
      job = { live: !!m.live, h, ch: m.channels, inPtr: M._malloc(FRAME * m.channels * 2), outPtr: M._malloc(MAX_PACKET), pending: new Int16Array(0), parts: [], bytes: 0, frames: 0 };
      postMessage({ id: m.id, ok: true });
    } else if (m.kind === 'feed') {
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
