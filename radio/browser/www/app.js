// Pyrate Radio in a browser tab. The tab is a node: it hears the Log, checks every entry
// against its station's key, and plays six-second chunks from whoever carries them, to
// the second the station's own clock says. No rights reserved.
import init, { Radio } from './pkg/pyrate_browser.js';

const HARBOUR = '{{HARBOUR}}';
const RELAYS = '{{RELAYS}}';
const AHEAD_MS = 12000;
const $ = id => document.getElementById(id);
const sleep = ms => new Promise(r => setTimeout(r, ms));

let radio = null, stations = [], cur = null, playing = false, gen = 0;
let ctx = null, gain = null, sources = [];
const pics = new Map();

function store(k, v) { try { if (v === undefined) return localStorage.getItem(k); localStorage.setItem(k, v) } catch (e) { return null } }
function marooned() { try { return new Set(JSON.parse(store('pyrate.maroon') || '[]')) } catch (e) { return new Set() } }
function setMarooned(s) { store('pyrate.maroon', JSON.stringify([...s])) }
// The listener's own switches. Explicit stays off and art stays on until they say otherwise.
const sw = { mature: () => store('pyrate.hide_mature') === '1', explicit: () => store('pyrate.show_explicit') === '1', art: () => store('pyrate.hide_art') !== '1' };
function allowed(s) { return !((s.explicit && !sw.explicit()) || (sw.mature() && (s.mature || s.explicit))) }

// The flag every station flies until it raises a picture: its key, drawn.
function bits(id) { const b = []; for (const c of id) { const v = parseInt(c, 16); for (let i = 3; i >= 0; i--) b.push((v >> i) & 1) } return b }
function flag(el, id, live) { const b = bits(id); let h = ''; for (let r = 0; r < 6; r++) for (let c = 0; c < 9; c++) { const half = c < 5 ? c : 8 - c; h += b[(r * 5 + half) % b.length] ? '<b>1</b>' : '<span>0</span>' } el.innerHTML = h; el.classList.toggle('live', !!live) }

// Pictures come over the network like chunks, checked against their fingerprints.
async function pic(station, hash) {
  if (!hash || !sw.art()) return null;
  if (pics.has(hash)) return pics.get(hash);
  const p = radio.fetch(station, hash).then(b => URL.createObjectURL(new Blob([b], { type: 'image/jpeg' }))).catch(() => { pics.delete(hash); return null });
  pics.set(hash, p);
  return p;
}

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

function status(t) { $('np').textContent = t; $('by').textContent = '' }

// Fills twelve seconds ahead and keeps to the station's clock, the way the app does.
async function play(id, g) {
  let playhead = null, next = 0, track = null, dec = null, liveSeq = null, misses = 0;
  const reset = () => { stopAll(); playhead = null; track = null; liveSeq = null; dec?.close(); dec = null };
  while (g === gen) {
    const queued = Math.max(0, next - ctx.currentTime) * 1000;
    if (playhead !== null || liveSeq !== null) {
      if (queued > AHEAD_MS) { await sleep(400); continue }
      // Off the beat: something stalled. Drop what is queued and rejoin the clock.
      if (liveSeq === null && Math.abs(playhead - queued - radio.now()) > 4000) reset();
    }
    const at = playhead ?? radio.now() + 400;
    const p = JSON.parse(radio.plan(id, at));
    if (p.kind === 'searching') { if (!queued) status('Searching the Log'); await radio.sync(id).catch(() => { }); await sleep(1200); continue }
    if (p.kind === 'dead air') { reset(); status('Dead air'); await sleep(4000); continue }
    let chunk, ch, ms, seq = null;
    if (p.kind === 'live') {
      const t = p.tail;
      const n = liveSeq === null ? (t[t.length - 2] || t[t.length - 1]) : t.find(x => x.seq > liveSeq);
      if (!n) { await radio.sync(id).catch(() => { }); await sleep(1500); continue }
      if (liveSeq === null) { reset(); dec = await Decoder.make(n.channels) }
      ({ chunk, channels: ch, ms } = n); seq = n.seq;
    } else {
      if (liveSeq !== null) reset();
      chunk = p.chunk; ch = p.channels; ms = p.chunk_ms;
    }
    let bytes;
    try { bytes = await radio.fetch(id, chunk); misses = 0 } catch (e) {
      if (++misses > 2) status('No carrier answered yet');
      await sleep(1500); continue;
    }
    if (g !== gen) return;
    if (p.kind === 'track' && (track !== p.track || !dec || playhead === null)) { dec?.close(); dec = await Decoder.make(ch); track = p.track }
    let chs;
    try { chs = await dec.decode(bytes) } catch (e) { console.warn(e); reset(); await sleep(500); continue }
    if (g !== gen || !chs.length) return;
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
  }
  dec?.close();
}

// ---- The page ------------------------------------------------------------------------
async function nowPlaying() {
  if (!cur || !radio) return;
  const p = JSON.parse(radio.plan(cur.id, radio.now()));
  if (p.kind === 'live') { $('np').textContent = 'Live'; $('by').textContent = ''; $('air').innerHTML = '<span class="air">ON AIR</span>' }
  else if (p.kind === 'track') {
    $('np').textContent = p.title || ''; $('by').textContent = p.by || '';
    const url = await pic(cur.id, p.art || cur.cover); art(url);
    if ('mediaSession' in navigator && playing) navigator.mediaSession.metadata = new MediaMetadata({ title: p.title || cur.name, artist: p.by || '', album: cur.name, artwork: url ? [{ src: url, sizes: '512x512', type: 'image/jpeg' }] : [] });
  } else if (!playing) status(p.kind === 'dead air' ? 'Dead air' : '');
}

function art(url) { const c = $('cover'); if (url) { if (c.src !== url) c.src = url; c.hidden = false; $('flag').hidden = true } else { c.hidden = true; $('flag').hidden = false } }

async function show(s) {
  cur = s;
  flag($('flag'), s.id, s.live); art(await pic(s.id, s.cover));
  const b = await pic(s.id, s.banner); $('banner').style.backgroundImage = b ? 'url(' + b + ')' : '';
  $('name').textContent = s.name || s.call_sign; $('cs').textContent = s.call_sign + (s.aboard !== undefined ? ' · ' + s.aboard + ' aboard' : '');
  $('air').innerHTML = s.live ? '<span class="air">ON AIR</span>' : '';
  $('go').hidden = false; $('go').textContent = playing ? '■ STOP' : '▶ LISTEN';
  $('pass').hidden = false; $('maroon').hidden = false;
  document.querySelectorAll('.row').forEach(r => r.classList.toggle('on', r.dataset.id === s.id));
  if (location.hash.slice(1) !== s.id) history.replaceState(null, '', '#' + s.id);
  radio.tune(s.id);
  nowPlaying();
}

function listen(s) {
  audio(); ctx.resume();
  const g = ++gen; stopAll(); playing = true; show(s); $('go').textContent = '■ STOP';
  play(s.id, g);
}

function stop() { gen++; stopAll(); playing = false; $('go').textContent = '▶ LISTEN'; ctx?.suspend() }

function rows() {
  const m = marooned();
  const shown = stations.filter(s => !m.has(s.id) && allowed(s));
  const box = $('rows'); box.innerHTML = '';
  if (!shown.length) { box.innerHTML = '<p>Listening for station cards. The first come in within a minute.</p>'; return }
  for (const s of shown) {
    const r = document.createElement('div'); r.className = 'row'; r.dataset.id = s.id;
    r.innerHTML = '<div class="mini"></div><div class="meta"><div class="name"></div><div class="cs"></div><div class="about"></div></div><div class="count"></div>';
    const mini = r.querySelector('.mini');
    if (s.cover) { mini.classList.add('pic'); pic(s.id, s.cover).then(u => { if (u) mini.innerHTML = '<img alt="" src="' + u + '">'; else flag(mini, s.id, s.live) }) } else flag(mini, s.id, s.live);
    r.querySelector('.name').textContent = s.name + (s.live ? '  · ON AIR' : '') + (s.explicit ? '  · EXPLICIT' : s.mature ? '  · MATURE' : '');
    r.querySelector('.cs').textContent = s.call_sign; r.querySelector('.about').textContent = s.about || '';
    r.querySelector('.count').innerHTML = s.aboard + '<small>ABOARD</small>';
    r.classList.toggle('on', cur?.id === s.id);
    r.onclick = () => listen(s);
    box.appendChild(r);
  }
}

function refresh() {
  if (!radio) return;
  stations = JSON.parse(radio.stations());
  const n = radio.neighbours();
  $('signal').textContent = n ? n + (n === 1 ? ' neighbour' : ' neighbours') + ' · ' + stations.length + ' on the air' : 'Coming aboard';
  rows();
  if (cur) { const u = stations.find(x => x.id === cur.id); if (u) { const pic0 = cur.cover, ban0 = cur.banner; cur = u; $('cs').textContent = u.call_sign + ' · ' + u.aboard + ' aboard'; $('name').textContent = u.name; if (u.cover !== pic0 || u.banner !== ban0) show(u) } }
  else { const id = location.hash.slice(1); const s = stations.find(x => x.id === id) || (!/^[0-9a-f]{64}$/.test(id) && stations.find(x => allowed(x) && !marooned().has(x.id))); if (s) show(s) }
}

// A station link (#<id>) works before the station's card arrives: the harbour carries it.
function followLink() {
  const id = location.hash.slice(1);
  if (!/^[0-9a-f]{64}$/.test(id) || cur?.id === id) return;
  const s = stations.find(x => x.id === id) || { id, call_sign: id.slice(0, 8).toUpperCase(), name: 'A passed-on station' };
  show(s);
}

$('go').onclick = () => { if (playing) stop(); else if (cur) listen(cur) };
$('pass').onclick = async () => {
  const url = location.origin + location.pathname + '#' + cur.id;
  if (navigator.share) { try { await navigator.share({ title: cur.name, url }); return } catch (e) { } }
  try { await navigator.clipboard.writeText(url); $('pass').textContent = 'LINK COPIED'; setTimeout(() => $('pass').textContent = 'PASS IT ON', 2000) } catch (e) { prompt('Pass on this link', url) }
};
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
  el.onchange = () => { store(key, (el.checked ? on : (on === '1' ? '0' : '1'))); pics.clear(); refresh(); if (cur) show(cur) };
}
$('mclear').onclick = () => { setMarooned(new Set()); $('mnote').textContent = 'Every station is back in the Log.'; refresh() };

async function boot() {
  $('signal').textContent = 'Coming aboard';
  try { await init() } catch (e) { $('name').textContent = 'This browser cannot run the radio'; return }
  let key = store('pyrate.token');
  if (!/^[0-9a-f]{64}$/.test(key || '')) { const b = new Uint8Array(32); crypto.getRandomValues(b); key = [...b].map(x => x.toString(16).padStart(2, '0')).join(''); store('pyrate.token', key) }
  try { radio = await Radio.start(HARBOUR, key, RELAYS) } catch (e) { console.warn(e); $('name').textContent = 'Could not reach a relay'; $('signal').textContent = String(e); return }
  followLink(); refresh();
  setInterval(refresh, 4000); setInterval(nowPlaying, 1000);
  window.addEventListener('hashchange', followLink);
  if ('mediaSession' in navigator) { navigator.mediaSession.setActionHandler('play', () => cur && listen(cur)); navigator.mediaSession.setActionHandler('pause', stop); navigator.mediaSession.setActionHandler('stop', stop) }
}
// For anyone looking under the hood: what the player is doing.
window.pyrate = { tap: () => { const d = ctx.createMediaStreamDestination(); gain.connect(d); return d.stream }, plan: at => radio && cur ? JSON.parse(radio.plan(cur.id, at ?? radio.now())) : null, state: () => ({ playing, station: cur?.id, scheduled: sources.length, ahead_ms: ctx ? Math.round(Math.max(0, (sources.length ? 1 : 0) * 1)) : 0, ctx: ctx?.state, neighbours: radio?.neighbours(), stations: stations.length, webcodecs }) };
boot();
