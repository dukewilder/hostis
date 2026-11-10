// Pyrate Radio in a browser tab. The tab is a node: it hears the Log, checks every entry
// against its station's key, and plays six-second chunks from whoever carries them, to
// the second the station's own clock says. No rights reserved.
import init, { Radio } from './pkg/pyrate_browser.js?v=999b95c661';

const HARBOUR = "a324c679b0dda368e23f21d6abbdaec0ef541a4ac317fa0643802b2734c9a442";
// HOSTIS's own stations. Inside another page (the landing page's hero) the set offers these
// and nothing else, and it joins the network only when the visitor presses Listen.
const HARBOUR_STATIONS = [{"id": "f33eb5c67e9dd58ca8f85aa4c730f23081937ad9e99746829f7233dadff0803f", "name": "The Public Domain", "call_sign": "GAHAGU·3695"}, {"id": "110243ae29e24f4f77bf4e5b0b9bb115ee4f2a4dfb76d6091fc913c54d97e44a", "name": "Readings", "call_sign": "DEVAKA·F42C"}];
const EMBED = document.documentElement.classList.contains('embed');
const passedId = () => { const id = location.hash.slice(1); return /^[0-9a-f]{64}$/.test(id) ? id : null };
const inSet = s => !EMBED || HARBOUR_STATIONS.some(h => h.id === s.id) || s.id === passedId();
const RELAYS = "";
const AHEAD_MS = 12000;
const $ = id => document.getElementById(id);
const sleep = ms => new Promise(r => setTimeout(r, ms));

let radio = null, stations = [], cur = null, playing = false, gen = 0, bootAt = 0, booting = null;
let ctx = null, gain = null, sources = [], meter = null;
const pics = new Map();

function store(k, v) { try { if (v === undefined) return localStorage.getItem(k); localStorage.setItem(k, v) } catch (e) { return null } }

// Every harbour this tab has ever been handed, kept so a returning listener never loses a
// door even if a later build ships a different set. Dead entries just fail to resolve.
function rememberHarbours() {
  let saved = []; try { saved = JSON.parse(store('pyrate.harbours') || '[]') } catch (e) {}
  const union = [...new Set(HARBOUR.split(/[,\s]+/).filter(Boolean).concat(saved))];
  try { store('pyrate.harbours', JSON.stringify(union)) } catch (e) {}
  return union.join(',');
}
function marooned() { try { return new Set(JSON.parse(store('pyrate.maroon') || '[]')) } catch (e) { return new Set() } }
function setMarooned(s) { store('pyrate.maroon', JSON.stringify([...s])) }
// The listener's own switches. Explicit stays off and art stays on until they say otherwise.
const sw = { mature: () => store('pyrate.hide_mature') === '1', explicit: () => store('pyrate.show_explicit') === '1', art: () => store('pyrate.hide_art') !== '1' };
function allowed(s) { return !((s.explicit && !sw.explicit()) || (sw.mature() && (s.mature || s.explicit))) }

// The flag every station flies until it raises a picture: its key, drawn.
function bits(id) { const b = []; for (const c of id) { const v = parseInt(c, 16); for (let i = 3; i >= 0; i--) b.push((v >> i) & 1) } return b }
function flag(el, id, live) { const b = bits(id); let h = ''; for (let r = 0; r < 6; r++) for (let c = 0; c < 9; c++) { const half = c < 5 ? c : 8 - c; h += b[(r * 5 + half) % b.length] ? '<b>1</b>' : '<span>0</span>' } el.innerHTML = h; el.classList.toggle('live', !!live) }

// Pictures come over the network like chunks, checked against their fingerprints. One that
// will not come is not asked for again for two minutes, none over 2 MB is shown, and the
// tab keeps the last two hundred, letting the rest go.
const picFailed = new Map();
async function pic(station, hash) {
  if (!hash || !sw.art() || !radio) return null;
  if (pics.has(hash)) { const p = pics.get(hash); pics.delete(hash); pics.set(hash, p); return p }
  if (Date.now() - (picFailed.get(hash) || 0) < 120000) return null;
  const p = radio.fetch(station, hash).then(b => { if (b.length > 2000000) throw 'too large'; return URL.createObjectURL(new Blob([b], { type: 'image/jpeg' })) })
    .catch(() => { pics.delete(hash); picFailed.set(hash, Date.now()); return null });
  pics.set(hash, p);
  while (pics.size > 200) { const [k, old] = pics.entries().next().value; pics.delete(k); old.then(u => u && u !== $('cover').src && URL.revokeObjectURL(u)) }
  return p;
}
function forgetPics() { for (const p of pics.values()) p.then(u => u && URL.revokeObjectURL(u)); pics.clear(); picFailed.clear() }

// ---- Decoding ------------------------------------------------------------------------
// A chunk is its Opus frames, each after its length as a big-endian u16.
function packets(b) { const out = []; let at = 0; while (at + 2 <= b.length) { const n = (b[at] << 8) | b[at + 1]; at += 2; if (at + n > b.length) break; out.push(b.subarray(at, at + n)); at += n } return out }

// ?decoder=wasm forces the portable decoder, for testing the path older browsers take.
let webcodecs = /[?&]decoder=wasm/.test(location.search) ? false : null;
async function canWebCodecs(ch) {
  if (!('AudioDecoder' in window)) return false;
  try { return (await AudioDecoder.isConfigSupported({ codec: 'opus', sampleRate: 48000, numberOfChannels: ch })).supported } catch (e) { return false }
}

// One decoder per track keeps a track seamless across its chunks.
class Decoder {
  static async make(ch) {
    const d = new Decoder(); d.ch = ch;
    if (webcodecs === null) webcodecs = await canWebCodecs(2);
    if (webcodecs) {
      d.out = [];
      d.wc = new AudioDecoder({ output: f => { const n = f.numberOfFrames, chs = []; for (let c = 0; c < f.numberOfChannels; c++) { const a = new Float32Array(n); f.copyTo(a, { planeIndex: c, format: 'f32-planar' }); chs.push(a) } d.out.push(chs); d.got += n; f.close(); if (d.got >= d.want) d.done?.() }, error: e => console.warn('decoder', e) });
      d.wc.configure({ codec: 'opus', sampleRate: 48000, numberOfChannels: ch });
      d.ts = 0;
    } else {
      if (!window['opus-decoder']) await new Promise((ok, no) => { const s = document.createElement('script'); s.src = './opus-decoder.min.js'; s.onload = ok; s.onerror = no; document.head.appendChild(s) });
      d.od = new window['opus-decoder'].OpusDecoder({ channels: ch, preSkip: 0 });
      await d.od.ready;
    }
    return d;
  }
  // Planar 48 kHz samples for one chunk.
  async decode(bytes) {
    const ps = packets(bytes);
    if (this.wc) {
      // No flush between chunks: a flush starts the decoder cold, and the seam would show.
      // Wait instead for every frame of the chunk to come out, and flush only if it stalls.
      this.out = []; this.got = 0; this.want = ps.length * 960;
      const all = new Promise(r => this.done = r);
      for (const p of ps) { this.wc.decode(new EncodedAudioChunk({ type: 'key', timestamp: this.ts, duration: 20000, data: p })); this.ts += 20000 }
      if (await Promise.race([all.then(() => true), sleep(1500).then(() => false)]) === false) await this.wc.flush();
      this.done = null;
      const n = this.out.reduce((s, x) => s + x[0].length, 0), chs = [];
      for (let c = 0; c < this.out[0]?.length || 0; c++) { const a = new Float32Array(n); let at = 0; for (const x of this.out) { a.set(x[c], at); at += x[c].length } chs.push(a) }
      return chs;
    }
    return this.od.decodeFrames(ps).channelData;
  }
  close() { try { this.wc?.close() } catch (e) { } try { this.od?.free() } catch (e) { } }
}

// ---- Playing -------------------------------------------------------------------------
function audio() {
  if (ctx) return;
  ctx = new (window.AudioContext || window.webkitAudioContext)({ sampleRate: 48000, latencyHint: 'playback' });
  gain = ctx.createGain();
  meter = ctx.createAnalyser(); meter.fftSize = 1024; gain.connect(meter); vu();
  // On iPhones a page's own audio stops when the screen locks. Sent through an audio
  // element instead, it keeps playing and shows on the lock screen.
  if (/iPhone|iPad|iPod/.test(navigator.userAgent) && ctx.createMediaStreamDestination) {
    const dest = ctx.createMediaStreamDestination(); gain.connect(dest);
    const el = $('a'); el.srcObject = dest.stream; el.play().catch(() => { gain.disconnect(); gain.connect(ctx.destination) });
  } else gain.connect(ctx.destination);
}

function stopAll() { for (const s of sources) { try { s.stop() } catch (e) { } } sources = [] }

function schedule(chs, skipMs, when) {
  const skip = Math.min(Math.round(skipMs * 48), chs[0].length);
  const n = chs[0].length - skip;
  if (n <= 0) return when;
  const buf = ctx.createBuffer(2, n, 48000);
  buf.copyToChannel(chs[0].subarray(skip), 0);
  buf.copyToChannel((chs[1] || chs[0]).subarray(skip), 1);
  const src = ctx.createBufferSource(); src.buffer = buf; src.connect(gain); src.start(when);
  sources.push(src); src.onended = () => { sources = sources.filter(x => x !== src) };
  return when + n / 48000;
}

function status(t) { $('np').textContent = t; $('by').textContent = ''; $('slot').textContent = ''; $('prog').style.width = '0'; $('time').textContent = '' }
// A trouble the player is in, shown in place of the title until it clears.
let trouble = null;

// ---- The set: frequencies, the dial glass, the lamps and the meter ----------------------
// Every station sits at the same place on every dial: its frequency comes from its key.
const fq = id => (875 + parseInt(id.slice(0, 6), 16) % 206) / 10;
// Inset from the glass's edges, so the end numerals and names stay readable.
const at = f => 3 + (f - 87.5) / 20.5 * 94;
let dialIds = '', order = [];
function scale() {
  let h = '';
  for (let f = 88; f <= 108; f += 0.5) h += '<i style="left:' + at(f) + '%"' + (f % 2 === 0 ? ' class="major"' : '') + '></i>' + (f % 4 === 0 ? '<b style="left:' + at(f) + '%">' + f + '</b>' : '');
  $('scale').innerHTML = h;
  let t = ''; for (let a = -48; a <= 48; a += 12) { const r = a * Math.PI / 180; t += '<line x1="' + (60 + 46 * Math.sin(r)) + '" y1="' + (64 - 46 * Math.cos(r)) + '" x2="' + (60 + 52 * Math.sin(r)) + '" y2="' + (64 - 52 * Math.cos(r)) + '"/>' }
  $('ticks').innerHTML = t;
}
function dial(list) {
  const ids = list.map(s => s.id + (s.live ? '*' : '')).join();
  if (ids === dialIds) return;
  dialIds = ids; order = [...list].sort((a, b) => fq(a.id) - fq(b.id));
  $('track').style.width = Math.max($('glass').clientWidth, order.length * 92) + 'px';
  $('names').innerHTML = '';
  order.forEach((s, i) => {
    const b = document.createElement('button'); b.textContent = s.name || s.call_sign; b.dataset.id = s.id; b.title = fq(s.id).toFixed(1) + '  ' + (s.name || '');
    b.style.left = at(fq(s.id)) + '%'; b.style.top = (i % 3) * 20 + 'px'; b.classList.toggle('live', !!s.live); b.setAttribute('role', 'option');
    b.onclick = e => { e.stopPropagation(); tuneTo(s) };
    $('names').appendChild(b);
  });
  if (cur) needle(cur.id, false);
}
function needle(id, smooth = true) {
  $('needle').style.left = at(fq(id)) + '%';
  document.querySelectorAll('#names button').forEach(b => { const on = b.dataset.id === id; b.classList.toggle('on', on); b.setAttribute('aria-selected', on) });
  const g = $('glass'), x = $('track').clientWidth * at(fq(id)) / 100;
  if (g.scrollWidth > g.clientWidth) g.scrollTo({ left: x - g.clientWidth / 2, behavior: smooth ? 'smooth' : 'auto' });
  $('freq').textContent = fq(id).toFixed(1);
}
function tuneTo(s) { if (playing) listen(s); else show(s) }
$('glass').addEventListener('click', e => {
  if (!order.length) return;
  const r = $('track').getBoundingClientRect(), f = 87.5 + ((e.clientX - r.left) / r.width * 100 - 3) / 94 * 20.5;
  tuneTo(order.reduce((a, b) => Math.abs(fq(b.id) - f) < Math.abs(fq(a.id) - f) ? b : a));
});
$('glass').addEventListener('keydown', e => {
  if (!order.length || (e.key !== 'ArrowLeft' && e.key !== 'ArrowRight')) return;
  e.preventDefault();
  const i = order.findIndex(s => s.id === cur?.id), n = e.key === 'ArrowRight' ? i + 1 : i - 1;
  tuneTo(order[(n + order.length) % order.length]);
});
function lamps(live) { $('lamp').classList.toggle('on', !!live); $('air').classList.toggle('on', !!live); $('rx').classList.toggle('on', playing && sources.length > 0) }
// The meter follows the sound, with a needle's weight.
// It rests once nothing plays and the needle has fallen, and wakes when the radio does.
let vuAngle = -48, vuOn = false;
function vu() {
  if (vuOn || !meter) return;
  vuOn = true;
  const buf = new Float32Array(meter.fftSize);
  const step = () => {
    let rms = 0;
    if (playing) { meter.getFloatTimeDomainData(buf); for (const x of buf) rms += x * x; rms = Math.sqrt(rms / buf.length) }
    const want = -48 + Math.min(96, Math.max(0, (20 * Math.log10(rms + 1e-6) + 42) / 42 * 96));
    vuAngle += (want - vuAngle) * (want > vuAngle ? 0.35 : 0.08);
    $('vun').style.transform = 'rotate(' + vuAngle.toFixed(1) + 'deg)';
    if (playing || vuAngle > -47.5) requestAnimationFrame(step); else vuOn = false;
  };
  requestAnimationFrame(step);
}
const mmss = ms => { const s = Math.floor(ms / 1000); return Math.floor(s / 60) + ':' + String(s % 60).padStart(2, '0') };

// ---- Times, in the listener's own clock ------------------------------------------------
const hm = ms => new Date(ms).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
const DAYS = ['Sunday', 'Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday'];
function dayOf(ms) {
  const d = new Date(ms), t = new Date(), a = new Date(d.getFullYear(), d.getMonth(), d.getDate()), b = new Date(t.getFullYear(), t.getMonth(), t.getDate());
  const n = Math.round((a - b) / 86400000);
  return n === 0 ? 'Today' : n === 1 ? 'Tomorrow' : DAYS[d.getDay()];
}
const when = ms => { const d = dayOf(ms); return d === 'Today' || d === 'Tomorrow' ? d.toLowerCase() : 'on ' + d };
const offAir = back => back ? 'Off the air. Back ' + when(back) + ' at ' + hm(back) + '.' : 'Off the air';
// Where a passed-on link points: this copy of the radio if it is a public one on its own
// HTTPS name, so links still work if hostis.black is gone; otherwise hostis.black. A copy
// reached by a bare address, on plain HTTP or at a private name is never named in a link,
// since that address may be someone's house.
const shareBase = () => {
  const h = location.hostname, pub = location.protocol === 'https:' && !/^[\d.]+$|^\[|^localhost$|\.local$|\.lan$|\.home$|\.internal$/.test(h);
  return pub && !EMBED ? location.origin + location.pathname : 'https://hostis.black/pyrateradio/listen/';
};
const esc = t => String(t ?? '').replace(/[&<>"]/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[c]);

// Fills twelve seconds ahead and keeps to the station's clock, the way the app does. Any
// stumble (a decoder that will not load, a chunk that will not decode) starts the step again
// rather than ending the loop, and says so on the set.
async function play(id, g) {
  let playhead = null, next = 0, track = null, dec = null, liveSeq = null, misses = 0, wait = 1200, stumbles = 0;
  const reset = () => { stopAll(); playhead = null; next = 0; track = null; liveSeq = null; dec?.close(); dec = null };
  while (g === gen) {
    try {
      const queued = Math.max(0, next - ctx.currentTime) * 1000;
      if (playhead !== null || liveSeq !== null) {
        if (queued > AHEAD_MS) { await sleep(400); continue }
        // Off the beat: something stalled. Drop what is queued and rejoin the clock.
        if (liveSeq === null && Math.abs(playhead - queued - radio.now()) > 4000) reset();
      }
      const at = playhead ?? radio.now() + 400;
      const p = JSON.parse(radio.plan(id, at));
      if (p.kind === 'struck') { reset(); trouble = null; status('This station has struck its colours. It has ended.'); refresh(); return }
      // A log not found yet is looked for less and less often, up to every fifteen seconds.
      if (p.kind === 'searching') { if (!queued) status('Searching the Log'); await radio.sync(id).catch(() => { }); await sleep(wait); wait = Math.min(15000, wait * 1.6); continue }
      wait = 1200;
      if (p.kind === 'dead air' || p.kind === 'off air') { if (!queued) { reset(); status(p.kind === 'off air' ? offAir(p.back_at) : 'Dead air') } await sleep(4000); continue }
      let chunk, ch, ms, seq = null;
      if (p.kind === 'live') {
        const t = p.tail;
        const n = liveSeq === null ? (t[t.length - 2] || t[t.length - 1]) : t.find(x => x.seq > liveSeq);
        if (!n) { await radio.sync(id).catch(() => { }); await sleep(1500); continue }
        // From a track to the live broadcast: nothing of the track stays queued ahead of it.
        if (liveSeq === null) { reset(); dec = await Decoder.make(n.channels) }
        ({ chunk, channels: ch, ms } = n); seq = n.seq;
      } else {
        if (liveSeq !== null) reset();
        chunk = p.chunk; ch = p.channels; ms = p.chunk_ms;
      }
      let bytes;
      try { bytes = await radio.fetch(id, chunk); misses = 0; trouble = null } catch (e) {
        if (++misses > 2) trouble = 'No carrier answered yet';
        await sleep(1500); continue;
      }
      if (g !== gen) return;
      if (p.kind === 'track' && (track !== p.track || !dec || playhead === null)) { dec?.close(); dec = await Decoder.make(ch); track = p.track }
      const chs = await dec.decode(bytes);
      if (g !== gen) return;
      // A chunk that decodes to nothing is skipped, as a missing one is.
      if (!chs.length || !chs[0].length) { if (seq !== null) liveSeq = seq; else playhead = (playhead ?? p.chunk_start) + ms; continue }
      stumbles = 0;
      if (seq !== null) {
        next = schedule(chs, 0, Math.max(next, ctx.currentTime + 0.3)); liveSeq = seq;
        continue;
      }
      let when, skip = 0;
      if (playhead === null) {
        const lead = 0.3;
        skip = radio.now() + lead * 1000 - p.chunk_start;
        if (skip >= ms) continue;
        skip = Math.max(0, skip); when = ctx.currentTime + lead;
      } else when = Math.max(next, ctx.currentTime + 0.05);
      next = schedule(chs, skip, when);
      playhead = p.chunk_start + ms;
      if (p.next) radio.fetch(id, p.next).catch(() => { });
    } catch (e) {
      if (e instanceof WebAssembly.RuntimeError) return wrecked();
      console.warn(e); reset();
      // The portable decoder that would not load is the one stumble worth giving up on.
      if (++stumbles > 5) { trouble = null; status('This browser could not play the station. Try reloading the page.'); stop(); return }
      trouble = 'The player stumbled. Starting again';
      await sleep(800 * stumbles);
    }
  }
  dec?.close();
}

// ---- The page ------------------------------------------------------------------------
async function nowPlaying() {
  if (!cur || !radio) return;
  const p = JSON.parse(radio.plan(cur.id, radio.now()));
  lamps(p.kind === 'live');
  if (playing && trouble) { status(trouble); return }
  if (p.kind === 'struck') { status('This station has struck its colours. It has ended.'); return }
  if (p.kind === 'live') { $('np').textContent = 'Live from the station'; $('by').textContent = ''; $('slot').textContent = ''; $('prog').style.width = '100%'; $('time').textContent = '' }
  else if (p.kind === 'track') {
    $('np').textContent = p.title || ''; $('by').textContent = p.by || ''; $('slot').textContent = p.slot || '';
    $('prog').style.width = Math.min(100, p.position_ms / p.duration_ms * 100) + '%'; $('time').textContent = mmss(p.position_ms) + ' of ' + mmss(p.duration_ms);
    const url = await pic(cur.id, p.art || cur.cover); art(url);
    if ('mediaSession' in navigator && playing) navigator.mediaSession.metadata = new MediaMetadata({ title: p.title || cur.name, artist: p.by || '', album: cur.name, artwork: url ? [{ src: url, sizes: '512x512', type: 'image/jpeg' }] : [] });
  } else if (p.kind === 'off air') status(offAir(p.back_at));
  else if (!playing) status(p.kind === 'dead air' ? 'Dead air' : '');
}

// ---- The station's page: notices, up next and the week, from its own log ---------------
function howItPlays(p) {
  if (!p) return 'Plays its whole library shuffled, round the clock.';
  const z = p.zone_name ? ' ' + p.zone_name : '', m = p.mode || {}, at = String(Math.floor((m.start_min || 0) / 60)).padStart(2, '0') + ':' + String((m.start_min || 0) % 60).padStart(2, '0');
  const WEEK = ['Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday', 'Sunday'];
  let s = { in_order: 'Plays its library in order, from the top each Monday', newest_first: 'Plays its newest recordings first', daily: 'Plays its library in order, from the top every day at ' + at + z, weekly: 'Plays its library in order, from the top every ' + WEEK[(m.day || 0) % 7] + ' at ' + at + z }[m.kind] || 'Plays its whole library shuffled';
  s += (p.hours || []).length ? ', on the air for set hours' + z + '.' : ', round the clock.';
  if ((p.slots || []).length) s += ' ' + p.slots.length + (p.slots.length === 1 ? ' regular show.' : ' regular shows.');
  return s;
}

// A show for the listener's own calendar: a small .ics, made here, sent nowhere.
function ics(l, name) {
  const f = ms => new Date(ms).toISOString().replace(/[-:]/g, '').replace(/\.\d+/, '');
  // Every line break, a bare CR included, becomes the calendar's own escape: nothing in a
  // title can start a line of its own in the file.
  const t = s => String(s || '').replace(/[\\;,]/g, c => '\\' + c).replace(/\r\n|\r|\n/g, '\\n');
  const body = ['BEGIN:VCALENDAR', 'VERSION:2.0', 'PRODID:-//Pyrate Radio//EN', 'BEGIN:VEVENT', 'UID:' + l.start_ms + '-' + cur.id.slice(0, 16) + '@pyrateradio', 'DTSTAMP:' + f(Date.now()), 'DTSTART:' + f(l.start_ms), 'DTEND:' + f(l.end_ms),
    'SUMMARY:' + t(l.title + ' on ' + name), 'DESCRIPTION:' + t((l.about ? l.about + '\n' : '') + shareBase() + '#' + cur.id), 'URL:' + shareBase() + '#' + cur.id, 'END:VEVENT', 'END:VCALENDAR'].join('\r\n');
  const a = document.createElement('a'); a.href = URL.createObjectURL(new Blob([body], { type: 'text/calendar' })); a.download = (l.title || 'show').replace(/[^\w ]+/g, '').trim().slice(0, 40) + '.ics'; a.click();
  setTimeout(() => URL.revokeObjectURL(a.href), 4000);
}

let pageFor = null, pageAt = 0, pageV = null;
function page(force) {
  if (!cur || !radio) return;
  if (force || pageFor !== cur.id || Date.now() - pageAt > 30000) { pageV = JSON.parse(radio.page(cur.id, 7)); pageFor = cur.id; pageAt = Date.now() }
  const v = pageV;
  if (!v) { $('page').hidden = true; return }
  $('page').hidden = false;
  $('notices').innerHTML = (v.notices || []).map(n => '<div class="notice">' + esc(n.text) + '<small>' + esc(dayOf(n.at_ms) + ' ' + hm(n.at_ms)) + '</small></div>').join('');
  const next = (v.next || []).filter(n => n.start_ms > radio.now()).slice(0, 5);
  $('nexth').hidden = !next.length;
  $('next').innerHTML = next.map(n => '<div class="li"><span class="t">' + hm(n.start_ms) + '</span><span class="w">' + esc(n.title) + (n.by ? ' <em>' + esc(n.by) + '</em>' : '') + '</span></div>').join('');
  $('how').textContent = (!v.now && v.back_at ? offAir(v.back_at) + ' ' : '') + howItPlays(v.programme);
  const now = radio.now(); let last = null, h = '', days = 0;
  const ls = (v.listings || []).filter(l => l.kind !== 'hours');
  for (const [i, l] of ls.entries()) {
    const d = dayOf(l.start_ms);
    // Today and tomorrow show; the rest of the week waits behind a button.
    if (d !== last) { if (++days === 3) h += '<button class="small" id="week">THE WHOLE WEEK</button><div id="later" hidden>'; h += '<div class="day">' + d + '</div>'; last = d }
    const on = l.start_ms <= now && now < l.end_ms;
    h += '<div class="li' + (on ? ' on' : '') + '"><span class="t">' + hm(l.start_ms) + '–' + hm(l.end_ms) + '</span><span class="w">' + esc(l.title) +
      (l.live ? ' <b class="lv">LIVE</b>' : '') + (l.kind === 'show' ? ' <b class="pr">PREMIERE</b>' : '') + (l.about ? '<em>' + esc(l.about) + '</em>' : '') + '</span>' +
      (l.start_ms > now ? '<button class="small cal" data-i="' + i + '" title="Add to your calendar">+ CAL</button>' : '') + '</div>';
  }
  if (days >= 3) h += '</div>';
  const open = !$('later')?.hidden && $('later') !== null;
  $('listings').innerHTML = h || '<p>No shows on the bill this week.</p>';
  if ($('week')) { if (open) { $('later').hidden = false; $('week').hidden = true } $('week').onclick = () => { $('later').hidden = false; $('week').hidden = true } }
  $('listings').querySelectorAll('.cal').forEach(b => b.onclick = () => ics(ls[+b.dataset.i], v.name || cur.name));
}

function art(url) { const c = $('cover'); if (url) { if (c.src !== url) c.src = url; c.hidden = false; $('flag').hidden = true } else { c.hidden = true; $('flag').hidden = false } }

async function show(s) {
  if (hailFor && hailFor !== s.id) dropHail();
  cur = s;
  flag($('flag'), s.id, s.live); art(await pic(s.id, s.cover));
  const b = await pic(s.id, s.banner); $('banner').style.backgroundImage = b ? 'url(' + b + ')' : '';
  $('name').textContent = s.name || s.call_sign; $('cs').textContent = s.call_sign + (s.aboard !== undefined ? '  ' + s.aboard + ' aboard' : '');
  lamps(s.live); needle(s.id); power(); $('tip').hidden = !s.tip; $('tipbox').hidden = true;
  // A pocket station is passed on only by its keeper's invitation.
  $('pass').hidden = !!s.pocket; $('maroon').hidden = false; if (s.pocket) $('clip').hidden = true;
  document.querySelectorAll('.row').forEach(r => r.classList.toggle('on', r.dataset.id === s.id));
  // A pocket's id never goes in the address bar or the history.
  if (isPocket(s)) history.replaceState(null, '', location.pathname);
  else if (location.hash.slice(1) !== s.id) history.replaceState(null, '', '#' + s.id);
  radio.tune(s.id);
  nowPlaying(); page(true); chatter();
}

function listen(s) {
  audio(); ctx.resume();
  const g = ++gen; stopAll(); playing = true; show(s); power(); vu();
  radio.listening(true);
  play(s.id, g);
}

function stop() { gen++; stopAll(); playing = false; trouble = null; power(); lamps(false); ctx?.suspend(); radio?.listening(false) }
function power() { $('clip').hidden = !(playing && clipType()) || isPocket(cur); const g = $('go'); g.hidden = false; g.classList.toggle('on', playing); g.querySelector('.glyph').textContent = playing ? '■\uFE0E' : '▶\uFE0E'; g.querySelector('.lbl').textContent = playing ? 'Stop' : 'Listen' }

// A row's cover is fetched only once the row comes into view.
const lazy = 'IntersectionObserver' in window ? new IntersectionObserver(es => es.forEach(e => {
  if (!e.isIntersecting) return; lazy.unobserve(e.target);
  const mini = e.target.querySelector('.mini'), s = stations.find(x => x.id === e.target.dataset.id);
  if (s?.cover) pic(s.id, s.cover).then(u => { if (u) mini.innerHTML = '<img alt="" src="' + u + '">' });
}), { rootMargin: '200px' }) : null;
let rowsKey = '';
function rows() {
  const m = marooned();
  const shown = stations.filter(s => !m.has(s.id) && allowed(s) && inSet(s));
  const box = $('rows');
  dial(shown); if (!$('chart').hidden) chart();
  // The list is rebuilt only when something in it changed, not on every refresh.
  const key = shown.map(s => [s.id, s.name, s.live, s.aboard, s.cover, s.pocket, s.explicit, s.mature, s.about].join('|')).join('\n') + '#' + (cur?.id || '') + '#' + (shown.length ? '' : Math.floor(Date.now() / 30000));
  if (key === rowsKey) return;
  rowsKey = key; box.innerHTML = '';
  if (!shown.length) {
    const long = bootAt && Date.now() - bootAt > 90000;
    box.innerHTML = '<p class="fine">' + (long ? 'No station has reached this tab yet. ' + (radio?.neighbours() ? 'The network is here but quiet; stations may be hidden by your switches below.' : 'Nothing on this network has answered: it may block peer-to-peer traffic. Another network, or the app, may get through.') : 'Listening for station cards. The first come in within a minute.') + '</p>';
    return;
  }
  for (const s of shown) {
    const r = document.createElement('div'); r.className = 'row'; r.dataset.id = s.id;
    r.innerHTML = '<div class="mini"></div><div class="meta"><div class="name"></div><div class="cs"></div><div class="about"></div></div><div class="count"></div>';
    const mini = r.querySelector('.mini');
    flag(mini, s.id, s.live);
    if (s.cover) { mini.classList.add('pic'); if (lazy) lazy.observe(r); else pic(s.id, s.cover).then(u => { if (u) mini.innerHTML = '<img alt="" src="' + u + '">' }) }
    r.querySelector('.name').textContent = s.name + (s.live ? '  ON AIR' : '') + (s.pocket ? '  PRIVATE' : '') + (s.explicit ? '  EXPLICIT' : s.mature ? '  MATURE' : '');
    r.querySelector('.cs').textContent = s.call_sign; r.querySelector('.about').textContent = s.about || '';
    r.querySelector('.count').innerHTML = s.aboard + '<small>ABOARD</small><span class="fq">' + fq(s.id).toFixed(1) + '</span>';
    r.classList.toggle('on', cur?.id === s.id);
    r.onclick = () => listen(s);
    box.appendChild(r);
  }
}

function refresh() {
  if (!radio) return;
  stations = JSON.parse(radio.stations());
  const n = radio.neighbours();
  $('signal').textContent = (flocking ? 'In a flock, in step with its host · ' : '') + (n ? n + (n === 1 ? ' neighbour' : ' neighbours') + ' · ' + stations.length + ' on the air' : 'Coming aboard');
  $('flockleave').hidden = !flocking;
  rows();
  if (cur) { const u = stations.find(x => x.id === cur.id); if (u) { const pic0 = cur.cover, ban0 = cur.banner; cur = u; $('cs').textContent = u.call_sign + '  ' + u.aboard + ' aboard'; $('name').textContent = u.name; if (u.cover !== pic0 || u.banner !== ban0) show(u) } }
  else { const id = wanted || location.hash.slice(1); const s = stations.find(x => x.id === id) || (!/^[0-9a-f]{64}$/.test(id) && stations.find(x => allowed(x) && inSet(x) && !marooned().has(x.id))); if (s) show(s) }
}

// A flock link (#flock=...): this tab plays in step with the phone that showed it.
function flockFollow() {
  const h = location.hash; if (!radio || !h.includes('flock=')) return false;
  try { radio.flock_join(h) } catch (e) { return false }
  history.replaceState(null, '', location.pathname);
  flocking = setInterval(() => { if (!radio) return; const id = radio.flock_station(); if (id && cur?.id !== id) { const s = stations.find(x => x.id === id) || { id, call_sign: '', name: 'The flock\'s station' }; playing ? listen(s) : show(s) } }, 3000);
  refresh();
  return true;
}
let flocking = null;
$('flockleave').onclick = () => { clearInterval(flocking); flocking = null; radio?.flock_leave(); refresh() };

// A pocket station's invitation (#pocket=...): the tab joins, keeps the invitation in this
// browser only, and clears the key from the address bar.
const pockets = new Set();
let wanted = null;
const isPocket = s => !!s && (!!s.pocket || pockets.has(s.id));
function savedPockets() { try { return JSON.parse(store('pyrate.pockets') || '[]') } catch (e) { return [] } }
async function pocketJoin(link, open) {
  try {
    const id = await radio.pocket_join(link);
    pockets.add(id);
    const all = savedPockets(); if (!all.includes(link)) { all.push(link); store('pyrate.pockets', JSON.stringify(all)) }
    if (open) { wanted = id; followLink() }
  } catch (e) { $('signal').textContent = 'That invitation did not open: ' + e }
}

// A station link (#<id>) works before the station's card arrives: the harbour carries it.
function followLink() {
  const id = wanted || location.hash.slice(1);
  if (!/^[0-9a-f]{64}$/.test(id) || cur?.id === id) return;
  const s = stations.find(x => x.id === id) || { id, call_sign: id.slice(0, 8).toUpperCase(), name: pockets.has(id) ? 'A private station' : 'A passed-on station', pocket: pockets.has(id) };
  show(s);
}

$('go').onclick = async () => {
  if (playing) return stop();
  if (!cur) return;
  if (!radio) { audio(); ctx.resume(); $('np').textContent = 'Coming aboard'; await (booting ??= boot()); if (!radio) return; cur = stations.find(x => x.id === cur.id) || cur }
  listen(cur);
};
$('spin').onclick = () => {
  const m = marooned(), pool = (radio ? stations : HARBOUR_STATIONS).filter(s => !m.has(s.id) && allowed(s) && inSet(s) && s.id !== cur?.id);
  if (!radio) { if (pool.length) dormant(pool[Math.floor(Math.random() * pool.length)]); return }
  if (pool.length) { listen(pool[Math.floor(Math.random() * pool.length)]); scrollTo({ top: 0, behavior: 'smooth' }) }
};
// ---- The chart of the seas: stations by the home port they name ------------------------
let chartData = null; const ports = new Map();
const norm = t => String(t || '').normalize('NFD').replace(/[\u0300-\u036f]/g, '').toLowerCase().trim();
// "Austin" or "Austin, Texas" or "Paris, France": the most populous place that fits.
function place(port) {
  if (ports.has(port)) return ports.get(port);
  const [n, ...rest] = port.split(',').map(norm), q = rest.join(' ').trim();
  const hit = n && chartData.places.find(p => (norm(p[0]) === n || norm(p[1]) === n) && (!q || norm(p[2]).startsWith(q) || norm(p[3]).startsWith(q) || norm(p[4]) === q));
  const r = hit ? [hit[5], hit[6]] : null; ports.set(port, r); return r;
}
async function chart() {
  if (!chartData) { try { chartData = await (await fetch('chart.json')).json() } catch (e) { $('chart').innerHTML = '<p class="fine">The chart did not load. The list still works.</p>'; return } }
  const m = marooned(); let dots = '', n = 0;
  for (const s of stations.filter(s => !m.has(s.id) && allowed(s) && s.port)) {
    const p = place(s.port); if (!p) continue; n++;
    const j = parseInt(s.id.slice(0, 4), 16), x = (p[1] + 180) * 2 + (j % 7 - 3) * .6, y = (90 - p[0]) * 2 + ((j >> 3) % 7 - 3) * .6;
    dots += '<g class="dot' + (s.live ? ' live' : '') + (cur?.id === s.id ? ' on' : '') + '" data-id="' + s.id + '" tabindex="0" role="button" aria-label="' + esc(s.name + ', ' + s.port) + '"><circle cx="' + x.toFixed(1) + '" cy="' + y.toFixed(1) + '" r="3.4"/><title>' + esc(s.name + ', ' + s.port) + '</title></g>';
  }
  $('chart').innerHTML = '<svg viewBox="0 20 720 290" role="img" aria-label="The chart of the seas"><path class="land" d="' + chartData.land + '"/>' + dots + '</svg><p class="fine">' + (n ? n + (n === 1 ? ' station names' : ' stations name') + ' a home port. The rest sail under no port.' : 'No station on the air has named a home port yet.') + '</p>';
  $('chart').querySelectorAll('.dot').forEach(d => { const go = () => { const s = stations.find(x => x.id === d.dataset.id); if (s) { tuneTo(s); scrollTo({ top: 0, behavior: 'smooth' }) } }; d.onclick = go; d.onkeydown = e => e.key === 'Enter' && go() });
}
function view(c) { $('chart').hidden = !c; $('rows').hidden = c; $('vchart').classList.toggle('on', c); $('vlist').classList.toggle('on', !c); $('vchart').setAttribute('aria-pressed', c); $('vlist').setAttribute('aria-pressed', !c); if (c) chart() }
$('vlist').onclick = () => view(false); $('vchart').onclick = () => view(true);

// ---- Chatter: talk in the station's room, kept by nobody --------------------------------
const myName = () => store('pyrate.name') || '';
let chatKey = '';
function chatter() {
  if (!cur || !radio) return;
  const c = JSON.parse(radio.chat_read(cur.id));
  $('chatjoin').hidden = c.joined; $('chatroom').hidden = !c.joined;
  if (!c.joined) return;
  let mutes; try { mutes = new Set(JSON.parse(store('pyrate.mutes') || '[]')) } catch (e) { mutes = new Set() }
  $('chatwho').textContent = 'You speak here as ' + (myName() || c.voice) + '. ' + (c.neighbours ? 'Linked to ' + c.neighbours + ' in the room.' : 'Looking for others in the room.');
  const box = $('lines'), atEnd = box.scrollTop + box.clientHeight >= box.scrollHeight - 30;
  const shown = c.lines.filter(l => !mutes.has(l.voice));
  const key = cur.id + shown.map(l => l.id).join() + '#' + [...mutes].join();
  if (key === chatKey) return;
  chatKey = key;
  box.innerHTML = shown.map(l => '<div class="line' + (l.keeper ? ' keeper' : '') + (l.mine ? ' mine' : '') + '"><button class="who" data-v="' + esc(l.voice) + '" title="' + esc(l.voice) + '">' + esc(l.name || l.voice) + (l.keeper ? ' (the station)' : '') + '</button><small>' + hm(l.at_ms) + '</small><div class="said">' +
    (l.kind === 'cheer' ? '<span class="cheer">Cheers to what is on</span>' : l.kind === 'request' ? '<span class="req">Asks for ' + esc(l.title || 'a recording') + '</span>' + (l.text ? ', <em>' + esc(l.text) + '</em>' : '') : esc(l.text)) + '</div></div>').join('') || '<p class="fine">Nobody has said anything yet.</p>';
  if (atEnd) box.scrollTop = box.scrollHeight;
  box.querySelectorAll('.who').forEach(w => w.onclick = () => { if (!w.closest('.mine') && confirm('Mute ' + w.textContent + '? Only you stop hearing them, in this browser.')) { mutes.add(w.dataset.v); store('pyrate.mutes', JSON.stringify([...mutes])); chatKey = ''; chatter() } });
}
const speak = (kind, text, track) => radio.chat_send(cur.id, kind, text || '', track || '', myName()).then(chatter, e => { $('chatwho').textContent = String(e) });
$('chatgo').onclick = async () => { $('chatgo').textContent = 'Joining'; try { await radio.chat_join(cur.id) } catch (e) { $('chatgo').textContent = 'Join the chatter'; return } chatter() };
$('say').onsubmit = e => { e.preventDefault(); const t = $('sayt').value.trim(); if (!t) return; $('sayt').value = ''; speak('text', t) };
$('cheer').onclick = () => speak('cheer');
$('myname').value = myName(); $('myname').onchange = () => { store('pyrate.name', $('myname').value.trim()); chatter() };
$('askfor').onclick = () => {
  const lib = JSON.parse(radio.library(cur.id));
  $('asktrack').innerHTML = lib.map(t => '<option value="' + t.track + '">' + esc(t.title) + (t.by ? ', ' + esc(t.by) : '') + '</option>').join('');
  $('askbox').hidden = !$('askbox').hidden || !lib.length;
};
$('asksend').onclick = () => { speak('request', $('asknote').value.trim(), $('asktrack').value); $('asknote').value = ''; $('askbox').hidden = true };
setInterval(() => { if (!$('chatroom').hidden || !$('chatjoin').hidden) chatter() }, 2000);
// A hail: up to twenty seconds of the listener's voice, sealed in this tab so that only the
// station's own keys open it. It passes through the room live and nobody keeps it.
// A hail goes to the station it was started for. Switching station mid-hail drops it, and the
// microphone is let go however the hail ends.
let hailRec = null, hailFor = null, hailDrop = false;
function dropHail() { if (hailRec) { hailDrop = true; hailRec.stop() } }
$('hail').onclick = async () => {
  if (hailRec) { hailRec.stop(); return }
  if (!window.MediaRecorder || !navigator.mediaDevices) { $('chatwho').textContent = 'This browser cannot record a hail. The app can.'; return }
  let stream; try { stream = await navigator.mediaDevices.getUserMedia({ audio: true }) } catch (e) { $('chatwho').textContent = window !== parent ? 'A page holding the radio cannot lend it the microphone. Open the full player to send a hail.' : 'Allow the microphone, then press again.'; return }
  const type = ['audio/webm;codecs=opus', 'audio/ogg;codecs=opus', 'audio/mp4'].find(t => MediaRecorder.isTypeSupported(t)) || '';
  const parts = [], to = cur.id; let rec;
  try { rec = new MediaRecorder(stream, type ? { mimeType: type, audioBitsPerSecond: 24000 } : { audioBitsPerSecond: 24000 }) } catch (e) { stream.getTracks().forEach(t => t.stop()); $('chatwho').textContent = 'This browser cannot record a hail. The app can.'; return }
  hailRec = rec; hailFor = to; hailDrop = false;
  rec.ondataavailable = e => e.data.size && parts.push(e.data);
  rec.onstop = async () => {
    stream.getTracks().forEach(t => t.stop());
    const dropped = hailDrop; hailRec = null; hailFor = null; hailDrop = false; $('hail').textContent = 'Hail the keeper';
    if (dropped) return;
    const blob = new Blob(parts, { type: (rec.mimeType || type).split(';')[0] }), bytes = new Uint8Array(await blob.arrayBuffer());
    try { await radio.chat_hail(to, bytes, blob.type, myName()); $('chatwho').textContent = 'Your hail is on its way. Only the keeper can hear it, and nobody keeps it.' }
    catch (e) { $('chatwho').textContent = String(e) }
  };
  rec.start(); $('hail').textContent = 'Send the hail'; setTimeout(() => rec.state === 'recording' && rec.stop(), 20000);
};
$('chatleave').onclick = () => { if (cur) radio.chat_leave(cur.id); chatKey = ''; chatter() };

// ---- Clip it: twenty seconds of the station as a video to post anywhere ----------------
const CLIP_S = 20;
function clipType() { for (const t of ['video/mp4;codecs=avc1.42E01E,mp4a.40.2', 'video/mp4', 'video/webm;codecs=vp9,opus', 'video/webm']) if (window.MediaRecorder && MediaRecorder.isTypeSupported(t)) return t; return null }
async function clipIt() {
  const type = clipType(); if (!type || !playing || !cur) return;
  const W = 720, H = 1280, cv = document.createElement('canvas'); cv.width = W; cv.height = H; const g = cv.getContext('2d');
  const st = cur, url = shareBase() + '#' + st.id, q = JSON.parse(radio.qr(url));
  const coverUrl = await pic(st.id, st.cover); let img = null;
  if (coverUrl) { img = new Image(); img.src = coverUrl; await img.decode().catch(() => { img = null }) }
  const dest = ctx.createMediaStreamDestination(); gain.connect(dest);
  const stream = new MediaStream([...cv.captureStream(30).getVideoTracks(), ...dest.stream.getAudioTracks()]);
  const rec = new MediaRecorder(stream, { mimeType: type, videoBitsPerSecond: 2500000 }), parts = [];
  rec.ondataavailable = e => e.data.size && parts.push(e.data);
  const buf = new Float32Array(meter.fftSize), t0 = performance.now(); let lv = 0;
  const bone = '#ede4ce', faint = 'rgba(237,228,206,.55)', ghost = 'rgba(237,228,206,.22)', blood = '#ed5555';
  const fit = (t, font, max) => { g.font = font; let s = t; while (g.measureText(s).width > max && s.length > 3) s = s.slice(0, -2) + '…'; return s };
  const draw = () => {
    const el = (performance.now() - t0) / 1000;
    g.fillStyle = '#030303'; g.fillRect(0, 0, W, H);
    g.strokeStyle = faint; g.lineWidth = 2; g.strokeRect(40, 40, W - 80, H - 80);
    g.textAlign = 'center'; g.fillStyle = faint; g.font = '600 22px Cinzel'; g.fillText('P Y R A T E   R A D I O', W / 2, 110);
    const A = 400, ax = (W - A) / 2, ay = 160;
    g.strokeStyle = ghost; g.strokeRect(ax, ay, A, A);
    if (img) g.drawImage(img, ax + 1, ay + 1, A - 2, A - 2);
    else { const b = bits(st.id); g.font = '700 30px "JetBrains Mono"'; for (let r = 0; r < 6; r++) for (let c = 0; c < 9; c++) { const half = c < 5 ? c : 8 - c, on = b[(r * 5 + half) % b.length]; g.fillStyle = on ? bone : 'rgba(237,228,206,.12)'; g.fillText(on ? '1' : '0', ax + 60 + c * 35, ay + 120 + r * 38) } }
    g.fillStyle = bone; g.font = '800 54px Cinzel'; g.fillText(fit((st.name || st.call_sign).toUpperCase(), '800 54px Cinzel', W - 120), W / 2, 650);
    g.fillStyle = faint; g.font = '28px "JetBrains Mono"'; g.fillText(fq(st.id).toFixed(1) + ' on the dial', W / 2, 700);
    g.fillStyle = 'rgba(237,228,206,.82)'; g.font = 'italic 38px "IM Fell English"'; g.fillText(fit($('np').textContent, 'italic 38px "IM Fell English"', W - 140), W / 2, 780);
    g.fillStyle = faint; g.font = '600 20px Cinzel'; g.fillText(fit($('by').textContent.toUpperCase(), '600 20px Cinzel', W - 140), W / 2, 822);
    // The meter, from the sound itself.
    meter.getFloatTimeDomainData(buf); let rms = 0; for (const x of buf) rms += x * x; rms = Math.sqrt(rms / buf.length);
    lv += (Math.min(1, Math.max(0, (20 * Math.log10(rms + 1e-6) + 42) / 42)) - lv) * 0.3;
    const cx = W / 2, cy = 960, R = 90, a = (-48 + lv * 96) * Math.PI / 180;
    g.strokeStyle = faint; g.lineWidth = 2; g.beginPath(); g.arc(cx, cy, R, Math.PI * 1.23, Math.PI * 1.77); g.stroke();
    g.strokeStyle = blood; g.lineWidth = 4; g.beginPath(); g.arc(cx, cy, R, Math.PI * 1.62, Math.PI * 1.77); g.stroke();
    g.strokeStyle = bone; g.lineWidth = 3; g.beginPath(); g.moveTo(cx, cy); g.lineTo(cx + Math.sin(a) * (R + 6), cy - Math.cos(a) * (R + 6)); g.stroke();
    // A code to scan, and where it goes.
    const n = q.size, QS = 150, cell = QS / (n + 8), qx = W / 2 - QS / 2, qy = 1010;
    g.fillStyle = '#fbf8f1'; g.fillRect(qx, qy, QS, QS); g.fillStyle = '#0a0a0a';
    for (let y = 0; y < n; y++) for (let x = 0; x < n; x++) if (q.bits[y * n + x] === '1') g.fillRect(qx + (x + 4) * cell, qy + (y + 4) * cell, cell + .3, cell + .3);
    g.fillStyle = faint; g.font = '22px "JetBrains Mono"'; g.fillText(url.replace(/^https:\/\//, '').replace(/\/listen\/#.*$/, '').replace(/#.*$/, ''), W / 2, 1200);
    g.fillStyle = bone; g.fillRect(40, H - 46, (W - 80) * Math.min(1, el / CLIP_S), 4);
    if (el < CLIP_S && rec.state === 'recording') requestAnimationFrame(draw); else if (rec.state === 'recording') rec.stop();
  };
  rec.onstop = () => {
    gain.disconnect(dest);
    const blob = new Blob(parts, { type: type.split(';')[0] }), ext = type.includes('mp4') ? 'mp4' : 'webm', name = (st.name || 'Pyrate Radio').replace(/[^\w ]+/g, '').trim() + ' clip.' + ext;
    const u = URL.createObjectURL(blob); $('clipv').src = u; $('clipsave').href = u; $('clipsave').download = name; $('clipbox').hidden = false; $('clip').textContent = 'Clip it'; $('clip').disabled = false;
    const file = new File([blob], name, { type: blob.type });
    $('clipshare').hidden = !(navigator.canShare && navigator.canShare({ files: [file] }));
    $('clipshare').onclick = () => navigator.share({ files: [file], title: st.name, text: 'On Pyrate Radio: ' + url }).catch(() => { });
    $('clipbox').scrollIntoView({ behavior: 'smooth', block: 'nearest' });
  };
  $('clip').disabled = true; $('clip').textContent = 'Clipping';
  rec.start(500); requestAnimationFrame(draw);
}
$('clip').onclick = clipIt;
$('clipclose').onclick = () => { $('clipbox').hidden = true; URL.revokeObjectURL($('clipv').src) };

// ---- Tips: from the listener's wallet to the station, with a note if they like ----------
function tipUri(addr, note) {
  const a = addr.trim(), n = encodeURIComponent(note.trim());
  if (/^(lightning|bitcoin|monero):/i.test(a)) return a;
  if (a.includes('@') || /^(lnurl|lno)/i.test(a)) return 'lightning:' + a;
  if (/^[48]/.test(a) && (a.length === 95 || a.length === 106)) return 'monero:' + a + (n ? '?tx_description=' + n : '');
  return 'bitcoin:' + a + (n ? '?message=' + n : '');
}
$('tip').onclick = () => {
  const a = cur?.tip || ''; if (!a) return;
  const ln = tipUri(a, '').startsWith('lightning:');
  $('tipaddr').textContent = a; $('tipbox').hidden = false;
  $('tiphow').textContent = ln ? 'Your wallet asks for the note in its own comment box: copy it there.' : 'Your wallet carries the note with the payment.';
  const upd = () => { $('tipgo').href = tipUri(a, $('tipnote').value) }; $('tipnote').oninput = upd; upd();
  $('tipcopy').onclick = async () => { try { await navigator.clipboard.writeText(a); $('tipcopy').textContent = 'Copied' } catch (e) { prompt('The address', a) } };
  $('tipbox').scrollIntoView({ behavior: 'smooth', block: 'nearest' });
};
$('tipclose').onclick = () => { $('tipbox').hidden = true; $('tipcopy').textContent = 'Copy the address' };

// Pass it on: the station's code to scan, its link to copy or share.
$('pass').onclick = () => {
  const url = shareBase() + '#' + cur.id, q = JSON.parse(radio.qr(url)), n = q.size;
  let d = ''; for (let y = 0; y < n; y++) for (let x = 0; x < n; x++) if (q.bits[y * n + x] === '1') d += 'M' + (x + 4) + ' ' + (y + 4) + 'h1v1h-1z';
  $('qr').innerHTML = '<svg viewBox="0 0 ' + (n + 8) + ' ' + (n + 8) + '" role="img" aria-label="QR code for this station"><rect width="100%" height="100%" fill="#fbf8f1"/><path d="' + d + '" fill="#0a0a0a"/></svg>';
  $('plink').textContent = url; $('passbox').hidden = false; $('passbox').scrollIntoView({ behavior: 'smooth', block: 'nearest' });
  $('pcopy').onclick = async () => { try { await navigator.clipboard.writeText(url); $('pcopy').textContent = 'Copied' } catch (e) { prompt('The link', url) } };
  $('pshare').hidden = !navigator.share; $('pshare').onclick = () => navigator.share({ title: cur.name, text: 'Tune in to ' + cur.name + ' on Pyrate Radio', url }).catch(() => { });
};
$('pclose').onclick = () => { $('passbox').hidden = true; $('pcopy').textContent = 'Copy the link' };
$('maroon').onclick = () => {
  if (!cur || !confirm('Maroon ' + cur.name + '? It leaves this browser\'s Log and stays gone until you bring it back.')) return;
  const m = marooned(); m.add(cur.id); setMarooned(m); stop(); cur = null; history.replaceState(null, '', location.pathname); refresh();
};
$('mlist').onclick = () => {
  const m = marooned();
  const names = new Map(stations.map(s => [s.id, s.name]));
  $('mtext').value = '# A Pyrate Radio maroon list: one station per line. Take it aboard under Colours, or drop it in a Lantern\'s maroon.txt.\n' + [...m].map(id => names.has(id) ? id + '  # ' + names.get(id) : id).join('\n') + '\n';
  $('mbox').hidden = !$('mbox').hidden;
};
$('mtake').onclick = () => {
  const m = marooned(); let n = 0;
  for (const line of $('mtext').value.split('\n')) { const t = (line.trim().split(/\s+/)[0] || '').toLowerCase(); if (/^[0-9a-f]{64}$/.test(t) && !m.has(t)) { m.add(t); n++ } }
  setMarooned(m); $('mnote').textContent = n + ' taken aboard.'; refresh();
};
for (const [id, key, on] of [['sw-mature', 'pyrate.hide_mature', '1'], ['sw-explicit', 'pyrate.show_explicit', '1'], ['sw-art', 'pyrate.hide_art', '0']]) {
  const el = $(id); el.checked = on === '1' ? store(key) === '1' : store(key) !== '1';
  el.onchange = () => { store(key, (el.checked ? on : (on === '1' ? '0' : '1'))); forgetPics(); rowsKey = ''; refresh(); if (cur) show(cur) };
}
$('forget').onclick = () => {
  if (!confirm('Forget everything this browser keeps for the radio? Pockets go too, unless you still have their invitations.')) return;
  try { for (const k of Object.keys(localStorage)) if (k.startsWith('pyrate.')) localStorage.removeItem(k) } catch (e) { }
  try { for (const k of Object.keys(sessionStorage)) if (k.startsWith('pyrate.')) sessionStorage.removeItem(k) } catch (e) { }
  location.replace(location.pathname + location.search);
};
$('mclear').onclick = () => { setMarooned(new Set()); $('mnote').textContent = 'Every station is back in the Log.'; refresh() };

// A copy of the set served anywhere but hostis.black/pyrateradio/listen/ (a Lantern's
// /player/, a mirror) points its links to the landing at the landing itself.
if (!/\/pyrateradio\/listen\/$/.test(location.pathname)) document.querySelectorAll('a[href^="../"]').forEach(a => a.href = 'https://hostis.black/pyrateradio/' + a.getAttribute('href').slice(3));

async function boot() {
  // An invitation's key leaves the address bar before anything else happens.
  const invite = location.hash.includes('pocket=') ? location.hash : null;
  if (invite) history.replaceState(null, '', location.pathname);
  scale();
  $('signal').textContent = 'Coming aboard';
  try { await init() } catch (e) { $('name').textContent = 'This browser cannot run the radio'; return }
  let key = store('pyrate.token');
  if (!/^[0-9a-f]{64}$/.test(key || '')) { const b = new Uint8Array(32); crypto.getRandomValues(b); key = [...b].map(x => x.toString(16).padStart(2, '0')).join(''); store('pyrate.token', key) }
  // No relay answering is not the end: the set says so and tries again, more slowly each time.
  for (let tries = 0; !radio; tries++) {
    try { radio = await Radio.start(rememberHarbours(), key, RELAYS) } catch (e) {
      console.warn(e);
      $('name').textContent = 'Could not reach a relay';
      $('signal').textContent = 'Trying again. If it never comes, this network may block the radio: try another, or the app.';
      await sleep(Math.min(60000, 5000 * 2 ** tries));
    }
  }
  $('name').textContent = '';
  bootAt = Date.now();
  for (const l of savedPockets()) pocketJoin(l, false);
  if (invite) pocketJoin(invite, true);
  else if (!flockFollow()) followLink();
  refresh();
  setInterval(refresh, 4000); setInterval(nowPlaying, 1000); setInterval(() => page(false), 5000);
  window.addEventListener('hashchange', followLink);
  if ('mediaSession' in navigator) { navigator.mediaSession.setActionHandler('play', () => cur && listen(cur)); navigator.mediaSession.setActionHandler('pause', stop); navigator.mediaSession.setActionHandler('stop', stop) }
}
// The node in the tab is built to stop dead on a fault rather than carry on in a bad state.
// When it does, the set says so and comes back aboard once by itself, on the same station
// (never a pocket, whose id stays out of the address bar), and offers a reload after that.
function wrecked() {
  if (wrecked.done) return; wrecked.done = true;
  gen++; stopAll(); playing = false; power(); radio = null;
  let last = 0; try { last = +(sessionStorage.getItem('pyrate.wrecked') || 0) } catch (e) { }
  const again = Date.now() - last > 300000;
  status(again ? 'The radio stumbled badly. Coming back aboard.' : 'The radio stumbled badly and stopped. Reload the page to come back aboard.');
  if (!again) return;
  try { sessionStorage.setItem('pyrate.wrecked', String(Date.now())) } catch (e) { }
  const back = cur && !isPocket(cur) ? '#' + cur.id : '';
  setTimeout(() => { location.replace(location.pathname + location.search + back); location.reload() }, 2500);
}
addEventListener('error', e => { if (e.error instanceof WebAssembly.RuntimeError) wrecked() });
addEventListener('unhandledrejection', e => { if (e.reason instanceof WebAssembly.RuntimeError) wrecked() });

// For anyone looking under the hood: what the player is doing.
window.pyrate = { tap: () => { const d = ctx.createMediaStreamDestination(); gain.connect(d); return d.stream }, plan: at => radio && cur ? JSON.parse(radio.plan(cur.id, at ?? radio.now())) : null, state: () => ({ playing, station: cur?.id, scheduled: sources.length, ahead_ms: ctx ? Math.round(Math.max(0, (sources.length ? 1 : 0) * 1)) : 0, ctx: ctx?.state, neighbours: radio?.neighbours(), stations: stations.length, webcodecs }) };
// The set at rest, before it joins the network: a HOSTIS station, or the one passed on.
function dormant(s) {
  cur = s; flag($('flag'), s.id, false); art(null);
  $('name').textContent = s.name; $('cs').textContent = s.call_sign || ''; $('freq').textContent = fq(s.id).toFixed(1);
  dial(HARBOUR_STATIONS.concat(passedId() && !HARBOUR_STATIONS.some(h => h.id === passedId()) ? [{ id: passedId(), name: 'Passed on' }] : []));
  needle(s.id, false); power(); $('pass').hidden = true; $('np').textContent = 'Press Listen to come aboard.'; $('by').textContent = '';
}
// Inside the landing page, the set tells its host how tall it is.
if (document.documentElement.classList.contains('embed') && parent !== window) {
  const tell = () => parent.postMessage({ pyrate: 'height', h: document.documentElement.scrollHeight }, location.origin);
  new ResizeObserver(tell).observe(document.body); addEventListener('load', tell);
}
if (EMBED) { scale(); const p = passedId(); dormant(p ? { id: p, name: 'A passed-on station', call_sign: '' } : HARBOUR_STATIONS[0]); $('signal').textContent = '' }
else booting = boot();
