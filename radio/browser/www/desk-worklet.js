// The live desk's sound, worked on the audio thread so a busy page never leaves a gap in the air.
// Two processors. pyrate-voice is the microphone's: as encode-worker.js's voice(), the mains hum
// comes out and a keeper who asks has the voice shifted. pyrate-air is the air itself: a limiter
// that looks ahead, so the air never clips, and a tap that hands it to the encoder as it is made.
// No rights reserved.
const HUM = [50, 60, 100, 120, 150, 180, 200, 240, 250, 300, 350, 360], GRAIN = 1920;
// The keeper's microphone comes in as one channel. A second microphone or an interface may bring
// two: each loses its hum, and none of it is shifted.
class Voice extends AudioWorkletProcessor {
  constructor(o) {
    super();
    const p = o.processorOptions || {};
    this.line = new Float64Array(4 * GRAIN); this.at = 0; this.ph = 0; this.st = [];
    this.tune(p.semis || 0);
    this.port.onmessage = e => this.tune(e.data.semis ?? this.semis);
  }
  notches() {
    return HUM.map(f => { const w = 2 * Math.PI * f / sampleRate, al = Math.sin(w) / 60, a0 = 1 + al;
      return { b0: 1 / a0, b1: -2 * Math.cos(w) / a0, b2: 1 / a0, a1: -2 * Math.cos(w) / a0, a2: (1 - al) / a0, x1: 0, x2: 0, y1: 0, y2: 0 } });
  }
  tune(semis) { this.semis = semis; this.ratio = Math.pow(2, Math.max(-12, Math.min(12, semis || 0)) / 12); this.shift = Math.abs(this.ratio - 1) > 1e-6 }
  tap(d) { const n = this.line.length; let p = this.at - d; if (p < 0) p += n; const i = Math.floor(p) % n, f = p - Math.floor(p); return this.line[i] * (1 - f) + this.line[(i + 1) % n] * f }
  process(ins, outs) {
    const x = ins[0] || [], y = outs[0], n = this.line.length;
    for (let c = 0; c < y.length; c++) {
      const xc = x[c], yc = y[c];
      if (!xc) { yc.fill(0); continue }
      const nb = this.st[c] ??= this.notches(), shift = this.shift && y.length === 1;
      for (let k = 0; k < yc.length; k++) {
        let v = xc[k];
        for (const b of nb) { const o = b.b0 * v + b.b1 * b.x1 + b.b2 * b.x2 - b.a1 * b.y1 - b.a2 * b.y2; b.x2 = b.x1; b.x1 = v; b.y2 = b.y1; b.y1 = o; v = o }
        if (shift) {
          this.line[this.at] = v; const ph = this.ph, p2 = (ph + 0.5) % 1;
          v = this.tap(1 + ph * GRAIN) * Math.sin(Math.PI * ph) ** 2 + this.tap(1 + p2 * GRAIN) * Math.sin(Math.PI * p2) ** 2;
          this.ph = ((ph + (1 - this.ratio) / GRAIN) % 1 + 1) % 1; this.at = (this.at + 1) % n;
        }
        yc[k] = v;
      }
    }
    return true;
  }
}
registerProcessor('pyrate-voice', Voice);

// The ceiling is three decibels under full scale, and four while a show is talk: Opus rings past a
// sharp edge by as much as two at music's rate and more at talk's, and a listener would hear that
// clip. The gain each sample needs is held at its lowest for 5 ms and averaged over 5 ms, and the
// sound is delayed to match, so the gain is down before the peak arrives, never after.
const LA = 240, BLOCK = 1920;
class Air extends AudioWorkletProcessor {
  constructor() {
    super();
    this.dl = [new Float32Array(LA), new Float32Array(LA)]; this.dp = 0;
    this.qv = new Float32Array(LA); this.qi = new Float64Array(LA); this.qh = 0; this.qn = 0; this.t = 0;
    this.box = new Float32Array(LA).fill(1); this.bp = 0; this.sum = LA; this.r = 1;
    this.rel = 1 - Math.exp(-1 / (0.15 * sampleRate));
    this.out = null; this.buf = new Int16Array(2 * BLOCK); this.n = 0; this.ceil = Math.pow(10, -3 / 20);
    this.port.onmessage = e => {
      if (e.data.ceil !== undefined) this.ceil = Math.pow(10, e.data.ceil / 20);
      if (e.data.port) { this.out = e.data.port; this.n = 0 }
      // The end of a broadcast: what is made so far goes, then word that there is no more.
      if (e.data.stop && this.out) { if (this.n) this.out.postMessage(this.buf.slice(0, 2 * this.n)); this.out.postMessage({ end: true }); this.out = null; this.n = 0 }
    };
  }
  process(ins, outs) {
    const i = ins[0] || [], L = i[0], R = i[1] || i[0], oL = outs[0][0], oR = outs[0][1] || oL, dL = this.dl[0], dR = this.dl[1], CEIL = this.ceil;
    for (let k = 0; k < oL.length; k++) {
      const l = L ? L[k] : 0, r = R ? R[k] : 0, pk = Math.max(Math.abs(l), Math.abs(r)), need = pk > CEIL ? CEIL / pk : 1, t = this.t++;
      if (this.qn && this.qi[this.qh] <= t - LA) { this.qh = (this.qh + 1) % LA; this.qn-- }
      while (this.qn && this.qv[(this.qh + this.qn - 1) % LA] >= need) this.qn--;
      const q = (this.qh + this.qn) % LA; this.qv[q] = need; this.qi[q] = t; this.qn++;
      const h = this.qv[this.qh];
      this.r = h < this.r ? h : this.r + (h - this.r) * this.rel;
      this.sum += this.r - this.box[this.bp]; this.box[this.bp] = this.r; this.bp = (this.bp + 1) % LA;
      const g = Math.min(1, this.sum / LA);
      dL[this.dp] = l; dR[this.dp] = r; this.dp = (this.dp + 1) % LA;
      const a = Math.max(-CEIL, Math.min(CEIL, dL[this.dp] * g)), b = Math.max(-CEIL, Math.min(CEIL, dR[this.dp] * g));
      oL[k] = a; oR[k] = b;
      if (this.out) {
        this.buf[2 * this.n] = Math.round(a * 32767); this.buf[2 * this.n + 1] = Math.round(b * 32767);
        if (++this.n === BLOCK) { this.out.postMessage(this.buf, [this.buf.buffer]); this.buf = new Int16Array(2 * BLOCK); this.n = 0 }
      }
    }
    return true;
  }
}
registerProcessor('pyrate-air', Air);
