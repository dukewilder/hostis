// The encoder's hum comb (www/encode-worker.js): 33 harmonics of 50 and 60 Hz up to 1 kHz, each
// notched twice, so every one of them is taken down hard while speech between them passes.
// node tools/test/hum_comb.js. Prints PASS or FAIL lines.
const fs = require('fs'), vm = require('vm'), path = require('path');
const src = fs.readFileSync(path.join(__dirname, '../../browser/www/encode-worker.js'), 'utf8');
const ctx = { importScripts() { }, postMessage() { }, Module() { }, Math, Float64Array, Int16Array, Uint8Array, Uint16Array, Set };
vm.createContext(ctx); vm.runInContext(src + '\nthis.HUM = HUM; this.voice = voice;', ctx);
let ok = true; const check = (name, c) => { console.log((c ? 'PASS ' : 'FAIL ') + name); ok = ok && c };
const want = [...new Set([...Array(20)].map((_, i) => 50 * (i + 1)).concat([...Array(16)].map((_, i) => 60 * (i + 1))))].sort((a, b) => a - b);
check('33 frequencies, every multiple of 50 to 1000 and of 60 to 960, ascending', JSON.stringify(ctx.HUM) === JSON.stringify(want) && want.length === 33);
// The level of a tone after the comb, in dB, once the filters have settled.
function after(f) {
  const v = ctx.voice(true, 0), n = 48000 * 3, pcm = new Int16Array(n);
  for (let i = 0; i < n; i++) pcm[i] = Math.round(8000 * Math.sin(2 * Math.PI * f * i / 48000));
  v(pcm);
  let e = 0; for (let i = n - 48000; i < n; i++) e += pcm[i] * pcm[i];
  return 10 * Math.log10(Math.sqrt(e / 48000) / (8000 / Math.SQRT2));
}
const worst = Math.max(...[60, 360, 420, 480, 900, 960, 1000].map(after));
check('each harmonic comes down by more than 40 dB (worst ' + worst.toFixed(1) + ' dB)', worst < -40);
const speech = Math.min(...[225, 330, 1300, 2000].map(after));
check('tones between the harmonics and above 1 kHz pass within 3 dB (worst ' + speech.toFixed(1) + ' dB)', speech > -3);
process.exit(ok ? 0 : 1);
