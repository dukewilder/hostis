// Pyrate Radio in a browser tab. The tab is a node: it hears the Log, checks every entry
// against its station's key, and plays six-second chunks from whoever carries them, to
// the second the station's own clock says. No rights reserved.
import init, { Radio } from './pkg/pyrate_browser.js';

const HARBOUR = '{{HARBOUR}}';
// HOSTIS's own stations. Inside another page (the landing page's hero) the set offers these
// and nothing else, and it joins the network only when the visitor presses Listen.
const HARBOUR_STATIONS = '{{HARBOUR_STATIONS}}';
const EMBED = document.documentElement.classList.contains('embed');
// Where this copy plays. 'harbour' is hostis.black's own copy: HOSTIS's stations and nothing
// else, so nothing on that domain belongs to anyone else. Every other station, invitation and
// flock goes straight to an open player, the same set served where no one owns it (IPFS, a
// Lantern). 'open' is that copy: the whole Log, the chatter and the colours.
const SCOPE = '{{SCOPE}}';
const OPEN_PLAYERS = '{{OPEN_PLAYERS}}';
const HOSTIS_ONLY = SCOPE === 'harbour' || EMBED;
const isHarbour = id => HARBOUR_STATIONS.some(h => h.id === id);
const lastId = () => { if (SHARED) return ''; try { return sessionStorage.getItem('pyrate.cur') || '' } catch (e) { return '' } };
const passedId = () => { const id = location.hash.slice(1); return /^[0-9a-f]{64}$/.test(id) ? id : null };
const inSet = s => !HOSTIS_ONLY || isHarbour(s.id);
// The open player's fixed name if it answers within a few seconds, since a browser keeps what it
// keeps there from one release to the next; otherwise whichever other copy answers first.
async function openPlayer() {
  const ping = (u, ms) => fetch(u + 'favicon.png', { mode: 'no-cors', cache: 'no-store', signal: AbortSignal.timeout(ms) }).then(() => u);
  try { return await ping(OPEN_PLAYERS[0], 4000) } catch (e) { }
  try { return await Promise.any(OPEN_PLAYERS.slice(1).map(u => ping(u, 3500))) } catch (e) { return OPEN_PLAYERS[0] }
}
// A link for anything that is not HOSTIS's own leaves this copy before it is read: the part
// after the # never reaches a server, and this page never shows, names or plays what it holds.
const foreign = h => /pocket=|flock=/.test(h) || (/^#[0-9a-f]{64}$/.test(h) && !isHarbour(h.slice(1)));
function handOff() {
  const h = location.hash; if (!HOSTIS_ONLY || EMBED || !foreign(h)) return false;
  history.replaceState(null, '', location.pathname + location.search);
  document.documentElement.classList.add('leaving');
  openPlayer().then(u => location.replace(u + h));
  return true;
}
const RELAYS = '{{RELAYS}}';
const AHEAD_MS = 12000;
const $ = id => document.getElementById(id);
const sleep = ms => new Promise(r => setTimeout(r, ms));

let radio = null, stations = [], cur = null, playing = false, gen = 0, bootAt = 0, booting = null;
let ctx = null, gain = null, sources = [], meter = null;
const pics = new Map();

// An IPFS gateway that serves by path (/ipfs/<name>/) gives every page it serves one shared
// origin, so anything kept there could be read by any other page on it. There the tab keeps
// nothing past the visit, and offers the player's own address instead.
const SHARED = /^\/ip[fn]s\//.test(location.pathname);
const passing = new Map();
function store(k, v) {
  if (SHARED) { if (v === undefined) return passing.get(k) ?? null; passing.set(k, v); return }
  try { if (v === undefined) return localStorage.getItem(k); localStorage.setItem(k, v) } catch (e) { return null }
}

// Secrets this browser keeps (a station's deputy key, pocket invitations, the counting key) are
// sealed under a key the browser itself holds in IndexedDB and will never hand to a script as
// bytes. A copy of the profile's files, or anything reading its storage, finds only sealed
// text. Code running in this page could still ask the browser to open them, as it could ask
// for anything on the page.
const b64 = u => btoa(String.fromCharCode(...u)), unb64 = s => Uint8Array.from(atob(s), c => c.charCodeAt(0));
let devKey = null;
function deviceKey() {
  if (SHARED || !window.indexedDB || !crypto.subtle) return Promise.resolve(null);
  return devKey ??= new Promise(done => {
    const rq = indexedDB.open('pyrate', 1);
    rq.onupgradeneeded = () => rq.result.createObjectStore('keys');
    rq.onerror = () => done(null);
    rq.onsuccess = () => {
      const db = rq.result, get = db.transaction('keys').objectStore('keys').get('device');
      get.onerror = () => done(null);
      get.onsuccess = async () => {
        if (get.result) return done(get.result);
        try {
          const k = await crypto.subtle.generateKey({ name: 'AES-GCM', length: 256 }, false, ['encrypt', 'decrypt']);
          const put = db.transaction('keys', 'readwrite').objectStore('keys').put(k, 'device');
          put.onsuccess = () => done(k); put.onerror = () => done(null);
        } catch (e) { done(null) }
      };
    };
  });
}
async function dseal(text) {
  const k = await deviceKey(); if (!k) return text;
  const iv = crypto.getRandomValues(new Uint8Array(12));
  const ct = new Uint8Array(await crypto.subtle.encrypt({ name: 'AES-GCM', iv }, k, new TextEncoder().encode(text)));
  return JSON.stringify({ d: 1, iv: b64(iv), ct: b64(ct) });
}
// Plain text from an older build comes back as it is, and is sealed the next time it is kept.
async function dopen(text) {
  let o; try { o = JSON.parse(text) } catch (e) { return text }
  if (!o || o.d !== 1) return text;
  const k = await deviceKey(); if (!k) return null;
  try { return new TextDecoder().decode(await crypto.subtle.decrypt({ name: 'AES-GCM', iv: unb64(o.iv) }, k, unb64(o.ct))) } catch (e) { return null }
}
async function keepSealed(k, text) { store(k, await dseal(text)) }

// Every harbour this tab has ever been handed, kept so a returning listener never loses a
// door even if a later build ships a different set. Dead entries just fail to resolve.
// Only well-formed doors are kept, this build's own first, and at most twenty-four.
function rememberHarbours() {
  let saved = []; try { saved = JSON.parse(store('pyrate.harbours') || '[]') } catch (e) {}
  const door = h => typeof h === 'string' && /^[0-9a-f]{64}(@https?:\/\/[^\s,@]+)?$/.test(h);
  const union = [...new Set(HARBOUR.split(/[,\s]+/).filter(door).concat(Array.isArray(saved) ? saved.filter(door) : []))].slice(0, 24);
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
// Whether an Opus packet is one 20 ms frame, read from its table-of-contents byte (RFC 6716).
// A chunk is timed in 20 ms frames; a packet of any other length stands as 20 ms of quiet,
// so no chunk plays longer or shorter than its station says.
function oneFrame(p) {
  if (!p.length) return false;
  const cfg = p[0] >> 3, c = p[0] & 3, ms = cfg < 12 ? [10, 20, 40, 60][cfg & 3] : cfg < 16 ? [10, 20][cfg & 1] : [2.5, 5, 10, 20][cfg & 3];
  const n = c === 0 ? 1 : c < 3 ? 2 : p.length > 1 ? p[1] & 0x3f : 0;
  return n * ms === 20;
}
// Planar channels made exactly `n` samples long.
function fit(chs, n) { return chs.map(a => { if (a.length === n) return a; const b = new Float32Array(n); b.set(a.subarray(0, n)); return b }) }

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
    const all = packets(bytes), ps = all.filter(oneFrame), want = all.length * 960;
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
      return chs.length ? fit(chs, want) : chs;
    }
    const chs = ps.length ? this.od.decodeFrames(ps).channelData : [];
    return chs.length ? fit(chs, want) : chs;
  }
  close() { try { this.wc?.close() } catch (e) { } try { this.od?.free() } catch (e) { } }
}

// ---- Playing -------------------------------------------------------------------------
function audio() {
  if (ctx) return;
  ctx = new (window.AudioContext || window.webkitAudioContext)({ sampleRate: 48000, latencyHint: 'playback' });
  gain = ctx.createGain();
  meter = ctx.createAnalyser(); meter.fftSize = 1024; gain.connect(meter); vu();
  // On iPhones and iPads a page's own audio stops when the screen locks. Sent through an
  // audio element instead, it keeps playing and shows on the lock screen. An iPad's Safari
  // calls itself a Mac, and only its touch screen gives it away.
  const apple = /iPhone|iPad|iPod/.test(navigator.userAgent) || (navigator.platform === 'MacIntel' && navigator.maxTouchPoints > 1);
  // A call or another app can take the sound away; when it gives it back, the set picks up.
  ctx.onstatechange = () => { if (playing && ctx.state !== 'running' && ctx.state !== 'closed') ctx.resume().catch(() => { }) };
  if (apple && ctx.createMediaStreamDestination) {
    const dest = ctx.createMediaStreamDestination(); gain.connect(dest);
    const el = $('a'); el.srcObject = dest.stream; el.play().catch(() => { gain.disconnect(); gain.connect(meter); gain.connect(ctx.destination) });
    el.onpause = () => { if (playing) el.play().catch(() => { }) };
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
let trouble = null, aheadMs = 0;

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
  // The track stands in from the glass's ends (player.css), so a name at 88 or 108 is never cut.
  $('track').style.width = Math.max($('glass').clientWidth - 72, order.length * 92) + 'px';
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
  const g = $('glass'), x = $('track').offsetLeft + $('track').clientWidth * at(fq(id)) / 100;
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
// HOSTIS's own stations are passed on as hostis.black links. Every other station is passed on
// as an open player's link, so a stranger's station never wears hostis.black.
const shareBase = (st = cur) => {
  if (st && isHarbour(st.id) && !isPocket(st)) return 'https://hostis.black/pyrateradio/listen/';
  const h = location.hostname, pub = location.protocol === 'https:' && !/^[\d.]+$|^\[|^localhost$|\.local$|\.lan$|\.home$|\.internal$/.test(h);
  return SCOPE === 'open' && pub && !EMBED ? location.origin + location.pathname : OPEN_PLAYERS[0];
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
      const queued = Math.max(0, next - ctx.currentTime) * 1000; aheadMs = Math.round(queued);
      if (playhead !== null || liveSeq !== null) {
        // A hidden tab may be woken only once a minute: it keeps a minute and more queued.
        if (queued > (document.hidden ? 75000 : AHEAD_MS)) { await sleep(400); continue }
        // Off the beat: something stalled. Drop what is queued and rejoin the clock.
        if (liveSeq === null && Math.abs(playhead - queued - radio.now()) > 4000) reset();
      }
      const at = playhead ?? radio.now() + 400;
      const p = JSON.parse(radio.plan(id, at));
      if (p.kind === 'struck') { reset(); trouble = null; status('This station has struck its colours.'); refresh(); return }
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
        // Said only once the queue is nearly dry: a miss with seconds still queued is not heard.
        if (++misses > 2 && queued < 4000) trouble = 'No carrier answered yet';
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
  if (p.kind === 'struck') { status('This station has struck its colours.'); return }
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
  $('key').textContent = 'Key  ' + cur.id.match(/.{1,4}/g).join(' ');
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
  // On hostis.black the chatter is in the open player: this station's room, one tap away.
  if (SCOPE === 'harbour') $('chatopen').href = OPEN_PLAYERS[0] + '#' + s.id;
  flag($('flag'), s.id, s.live); art(await pic(s.id, s.cover));
  const b = await pic(s.id, s.banner); $('banner').style.backgroundImage = b ? 'url(' + b + ')' : '';
  $('name').textContent = s.name || s.call_sign; $('cs').textContent = s.call_sign + (s.aboard !== undefined ? '  ' + s.aboard + ' aboard' : '');
  lamps(s.live); needle(s.id); power(); $('tip').hidden = !s.tip; $('tipbox').hidden = true;
  // A pocket station is passed on only by its keeper's invitation.
  $('pass').hidden = !!s.pocket; $('maroon').hidden = false; if (s.pocket) $('clip').hidden = true;
  document.querySelectorAll('.row').forEach(r => r.classList.toggle('on', r.dataset.id === s.id));
  // A pocket's id never goes in the address bar or the history.
  if (isPocket(s)) history.replaceState(null, '', location.pathname);
  // The station tuned stays out of the address bar, so it never lands in history or a synced
  // browser. The tab alone remembers it, for a reload, and forgets it when it closes.
  else { if (location.hash) history.replaceState(null, '', location.pathname + location.search); if (!SHARED) try { sessionStorage.setItem('pyrate.cur', s.id) } catch (e) { } }
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
function power() { $('clip').hidden = !(playing && clipType()) || isPocket(cur); const g = $('go'); g.hidden = false; g.classList.toggle('on', playing); g.querySelector('.glyph').textContent = playing ? '■\uFE0E' : '▶\uFE0E'; g.querySelector('.lbl').textContent = playing ? 'Stop' : 'Listen';
  // The tab says what it is playing, so a listener with many tabs can find the radio.
  if (!EMBED && cur) document.title = (playing ? '\u25B6\uFE0E ' : '') + (cur.name || cur.call_sign || 'A station') + ' · Pyrate Radio' }

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
    const nm = r.querySelector('.name'); nm.textContent = s.name;
    // What a station says of itself, as hairline tags: the open air in bright red, the rest in bone.
    for (const [t, c] of [[s.live && 'On air', 'lv'], [s.mixed && !isHarbour(s.id) && 'False colours?', ''], [s.pocket && 'Private', ''], [s.explicit ? 'Explicit' : s.mature && 'Mature', '']]) if (t) { const g = document.createElement('span'); g.className = 'tag ' + c; g.textContent = t; nm.append(g) }
    r.querySelector('.cs').textContent = s.call_sign; r.querySelector('.about').textContent = s.about || '';
    r.querySelector('.count').innerHTML = s.aboard + '<small>ABOARD</small><span class="fq">' + fq(s.id).toFixed(1) + '</span>';
    r.classList.toggle('on', cur?.id === s.id);
    r.onclick = () => listen(s);
    box.appendChild(r);
  }
}

function refresh() {
  if (!radio) return;
  stations = JSON.parse(radio.stations()).filter(inSet);
  const n = radio.neighbours();
  $('signal').textContent = (flocking ? 'In a flock, in step with its host · ' : '') + (n ? n + (n === 1 ? ' neighbour' : ' neighbours') + ' · ' + stations.length + ' on the air' : 'Coming aboard');
  $('flockleave').hidden = !flocking;
  rows();
  if (cur) { const u = stations.find(x => x.id === cur.id); if (u) { const pic0 = cur.cover, ban0 = cur.banner; cur = u; $('cs').textContent = u.call_sign + '  ' + u.aboard + ' aboard'; $('name').textContent = u.name; if (u.cover !== pic0 || u.banner !== ban0) show(u) } }
  else { const id = wanted || location.hash.slice(1) || lastId(); const s = stations.find(x => x.id === id) || (!/^[0-9a-f]{64}$/.test(id) && stations.find(x => allowed(x) && inSet(x) && !marooned().has(x.id))); if (s) show(s) }
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
let pocketsMem = [];
function savedPockets() { return pocketsMem }
async function pocketJoin(link, open) {
  try {
    const id = await radio.pocket_join(link);
    pockets.add(id);
    const all = savedPockets(); if (!all.includes(link)) { all.push(link); keepSealed('pyrate.pockets', JSON.stringify(all)) }
    if (open) { wanted = id; followLink() }
  } catch (e) { $('signal').textContent = 'That invitation did not open: ' + e }
}

// A station link (#<id>) works before the station's card arrives: the harbour carries it.
function followLink() {
  const id = wanted || location.hash.slice(1) || lastId();
  if (!/^[0-9a-f]{64}$/.test(id) || cur?.id === id || (HOSTIS_ONLY && !isHarbour(id))) return;
  // On an Android phone, a passed station can go straight to the app if it is there.
  if (/Android/.test(navigator.userAgent) && !pockets.has(id)) { $('appopen').href = 'intent://station/' + id + '#Intent;scheme=pyrate;package=black.hostis.pyrate;S.browser_fallback_url=' + encodeURIComponent('https://hostis.black/pyrateradio/#set') + ';end'; $('appnote').hidden = false }
  const s = stations.find(x => x.id === id) || { id, call_sign: id.slice(0, 8).toUpperCase(), name: pockets.has(id) ? 'A private station' : 'A passed-on station', pocket: pockets.has(id) };
  show(s);
}

// Space or K turns the set on and off, as on any player, unless the listener is typing.
addEventListener('keydown', e => {
  if ((e.key !== ' ' && e.key !== 'k') || e.ctrlKey || e.metaKey || e.altKey || e.repeat) return;
  if (e.target.closest?.('input,textarea,select,button,a,[contenteditable],#glass') || $('go').hidden) return;
  e.preventDefault(); $('go').click();
});
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
let chartData = null, chartLoad = null; const ports = new Map();
const norm = t => String(t || '').normalize('NFD').replace(/[\u0300-\u036f]/g, '').toLowerCase().trim();
// "Austin" or "Austin, Texas" or "Paris, France": the most populous place that fits.
function place(port) {
  if (ports.has(port)) return ports.get(port);
  const [n, ...rest] = port.split(',').map(norm), q = rest.join(' ').trim();
  const hit = n && chartData.places.find(p => (norm(p[0]) === n || norm(p[1]) === n) && (!q || norm(p[2]).startsWith(q) || norm(p[3]).startsWith(q) || norm(p[4]) === q));
  const r = hit ? [hit[5], hit[6]] : null; ports.set(port, r); return r;
}
async function chart() {
  // One fetch of the chart at a time, however often the list redraws while it loads.
  if (!chartData) { try { chartData = await (chartLoad ??= fetch('chart.json').then(r => r.json())) } catch (e) { chartLoad = null; $('chart').innerHTML = '<p class="fine">The chart did not load. The list still works.</p>'; return } }
  const m = marooned(); let dots = '', n = 0;
  for (const s of stations.filter(s => !m.has(s.id) && allowed(s) && inSet(s) && s.port)) {
    const p = place(s.port); if (!p) continue; n++;
    const j = parseInt(s.id.slice(0, 4), 16), x = (p[1] + 180) * 2 + (j % 7 - 3) * .6, y = (90 - p[0]) * 2 + ((j >> 3) % 7 - 3) * .6;
    dots += '<g class="dot' + (s.live ? ' live' : '') + (cur?.id === s.id ? ' on' : '') + '" data-id="' + s.id + '" tabindex="0" role="button" aria-label="' + esc(s.name + ', ' + s.port) + '"><circle cx="' + x.toFixed(1) + '" cy="' + y.toFixed(1) + '" r="3.4"/><title>' + esc(s.name + ', ' + s.port) + '</title></g>';
  }
  $('chart').innerHTML = '<svg viewBox="0 20 720 290" role="img" aria-label="The chart of the seas"><path class="land" d="' + chartData.land + '"/>' + dots + '</svg><p class="fine">' + (n ? n + (n === 1 ? ' station names' : ' stations name') + ' a home port. The rest sail under no port.' : 'No station on the air has named a home port yet.') + '</p>';
  $('chart').querySelectorAll('.dot').forEach(d => { const go = () => { const s = stations.find(x => x.id === d.dataset.id); if (s) { tuneTo(s); scrollTo({ top: 0, behavior: 'smooth' }) } }; d.onclick = go; d.onkeydown = e => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); go() } } });
}
function view(c) { $('chart').hidden = !c; $('rows').hidden = c; $('vchart').classList.toggle('on', c); $('vlist').classList.toggle('on', !c); $('vchart').setAttribute('aria-pressed', c); $('vlist').setAttribute('aria-pressed', !c); if (c) chart() }
$('vlist').onclick = () => view(false); $('vchart').onclick = () => view(true);

// ---- Chatter: talk in the station's room, kept on no disk by the radio --------------------------------
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
    try { await radio.chat_hail(to, bytes, blob.type, myName()); $('chatwho').textContent = 'Your hail is on its way, sealed so only the keeper can open it. What the keeper does with it after is theirs.' }
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
  const st = cur, url = shareBase(st) + '#' + st.id, q = JSON.parse(radio.qr(url));
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
$('sw-direct').checked = store('pyrate.direct') === '1';
$('sw-direct').onchange = () => { store('pyrate.direct', $('sw-direct').checked ? '1' : '0'); radio?.set_direct_ok($('sw-direct').checked) };
for (const [id, key, on] of [['sw-mature', 'pyrate.hide_mature', '1'], ['sw-explicit', 'pyrate.show_explicit', '1'], ['sw-art', 'pyrate.hide_art', '0']]) {
  const el = $(id); el.checked = on === '1' ? store(key) === '1' : store(key) !== '1';
  el.onchange = () => { store(key, (el.checked ? on : (on === '1' ? '0' : '1'))); forgetPics(); rowsKey = ''; refresh(); if (cur) show(cur) };
}
$('forget').onclick = () => {
  if (!confirm('Forget everything this browser keeps for the radio? Pockets go too, unless you still have their invitations. A station raised here stays on the air, but this browser can speak for it again only from its 24 words.')) return;
  try { for (const k of Object.keys(localStorage)) if (k.startsWith('pyrate.')) localStorage.removeItem(k) } catch (e) { }
  try { for (const k of Object.keys(sessionStorage)) if (k.startsWith('pyrate.')) sessionStorage.removeItem(k) } catch (e) { }
  try { indexedDB.deleteDatabase('pyrate') } catch (e) { }
  location.replace(location.pathname + location.search);
};
$('mclear').onclick = () => { setMarooned(new Set()); $('mnote').textContent = 'Every station is back in the Log.'; refresh() };

// ---- Raising colours from the tab: the open player only ----
// A station raised here is a key made here. Its 24 words are shown once and kept nowhere; the
// tab signs with a deputy whose seed this browser keeps. Recordings are decoded by the browser,
// levelled and encoded with libopus off the page's thread, signed into the log, and sent along
// the stem as a phone sends them. Nothing goes to any server.
// The deputy seeds sit in this browser in one of three ways: plain, sealed under a passphrase
// (PBKDF2 and AES-GCM, so a copied profile gives up nothing), or in the tab alone, forgotten
// when it closes. The 24 words bring a station back in every case.
let mineMem = null, mineSeal = null;
const tabKeys = () => { if (SHARED) return false; try { return sessionStorage.getItem('pyrate.mine') !== null } catch (e) { return false } };
const sealed = () => { try { const o = JSON.parse(store('pyrate.mine') || 'null'); return o && o.ct && o.salt ? o : null } catch (e) { return null } };
const validKeys = l => Array.isArray(l) ? l.filter(k => /^[0-9a-f]{64}$/.test(k.id) && /^[0-9a-f]{64}$/.test(k.deputy)) : [];
const kept = () => {
  if (mineMem) return mineMem;
  try { return validKeys(JSON.parse((tabKeys() ? sessionStorage.getItem('pyrate.mine') : store('pyrate.mine')) || '[]')) } catch (e) { return [] }
};
const unstore = k => { if (SHARED) passing.delete(k); else try { localStorage.removeItem(k) } catch (e) { } };
function keep(l) {
  mineMem = l; const s = JSON.stringify(l);
  if (mineSeal) { sealWith(mineSeal, s).then(o => store('pyrate.mine', JSON.stringify(o))); return }
  if (tabKeys()) { try { sessionStorage.setItem('pyrate.mine', s) } catch (e) { } return }
  keepSealed('pyrate.mine', s);
}
async function sealKey(pass, salt) {
  const base = await crypto.subtle.importKey('raw', new TextEncoder().encode(pass), 'PBKDF2', false, ['deriveKey']);
  return { salt, key: await crypto.subtle.deriveKey({ name: 'PBKDF2', salt, iterations: 600000, hash: 'SHA-256' }, base, { name: 'AES-GCM', length: 256 }, false, ['encrypt', 'decrypt']) };
}
async function sealWith(k, text) {
  const iv = crypto.getRandomValues(new Uint8Array(12));
  const ct = new Uint8Array(await crypto.subtle.encrypt({ name: 'AES-GCM', iv }, k.key, new TextEncoder().encode(text)));
  return { v: 1, salt: b64(k.salt), iv: b64(iv), ct: b64(ct) };
}
let mineCur = null, airing = 0, adopted = new Set();
const stNote = t => { $('st-note').textContent = t };

function studioShow() {
  const on = !!mineCur && adopted.has(mineCur);
  $('st-mine').hidden = !on; $('st-new').hidden = on || !$('st-show').hidden;
  if (!on) return;
  const k = JSON.parse(radio.mine()).find(m => m.id === mineCur);
  $('st-mname').textContent = k?.name || 'Your station'; $('st-mcs').textContent = (k?.call_sign || '') + '  ' + fq(mineCur).toFixed(1);
  flag($('st-flag'), mineCur, false);
}
function studioStatus() {
  if (!radio || !mineCur) return;
  const k = JSON.parse(radio.mine()).find(m => m.id === mineCur);
  const el = $('st-status'); if (!k) { el.textContent = adopted.has(mineCur) ? '' : 'Looking for the station on the network'; return }
  el.textContent = !k.pow ? 'Working the station’s proof of work · a few seconds'
    : airing ? 'Setting a recording on the clock'
    : k.unsent ? 'Sailing · ' + (k.chunks - k.unsent) + ' of ' + k.chunks + ' chunks aboard the network'
    : k.carriers && !k.tracks ? 'On the Log \u00b7 carried by ' + k.carriers + (k.carriers === 1 ? ' node' : ' nodes') + ' \u00b7 drop recordings in to start it playing'
    : k.carriers ? 'On the air · carried by ' + k.carriers + (k.carriers === 1 ? ' node' : ' nodes') + ' · ' + k.tracks + (k.tracks === 1 ? ' recording' : ' recordings')
    : 'Waiting for the first carrier';
}
// The proof of work a station shows once, so ten thousand junk stations cost ten thousand times one.
async function powWork(id) {
  for (let n = 0; ; n += 150000) {
    const got = radio.pow_step(id, n, 150000);
    if (got >= 0) { const l = kept(); const k = l.find(x => x.id === id); if (k) { k.pow = got; keep(l) } studioStatus(); return }
    await sleep(0);
  }
}
async function studioBoot() {
  if (SCOPE !== 'open' || EMBED || !radio) return;
  $('st-tabonly').checked = tabKeys(); $('st-safer').hidden = SHARED;
  if (sealed() && !mineMem) { $('st-lock').hidden = false; $('st-new').hidden = true; setInterval(studioStatus, 1500); return }
  adoptAll(); setInterval(studioStatus, 1500);
}
function adoptAll() {
  for (const k of kept()) {
    mineCur ??= k.id;
    radio.adopt(k.id, k.deputy).then(ok => {
      if (!ok) return;
      adopted.add(k.id);
      if (!(typeof k.pow === 'number' && radio.set_pow(k.id, k.pow))) powWork(k.id);
      studioShow();
    }, () => { });
  }
  studioShow();
}
$('st-open').onclick = async () => {
  const o = sealed(); if (!o) return;
  $('st-locknote2').textContent = 'Opening';
  try {
    const k = await sealKey($('st-pass').value, unb64(o.salt));
    const text = new TextDecoder().decode(await crypto.subtle.decrypt({ name: 'AES-GCM', iv: unb64(o.iv) }, k.key, unb64(o.ct)));
    mineMem = validKeys(JSON.parse(text)); mineSeal = k; $('st-pass').value = ''; $('st-locknote2').textContent = '';
    $('st-lock').hidden = true; adoptAll();
  } catch (e) { $('st-locknote2').textContent = 'That is not the passphrase.' }
};
$('st-pass').onkeydown = e => { if (e.key === 'Enter') $('st-open').click() };
$('st-lockit').onclick = async () => {
  const p = $('st-newpass').value; if (p.length < 8) { $('st-locknote').textContent = 'Eight characters at least. A short line of words is best.'; return }
  $('st-locknote').textContent = 'Sealing';
  try { sessionStorage.removeItem('pyrate.mine') } catch (e) { }
  $('st-tabonly').checked = false;
  mineSeal = await sealKey(p, crypto.getRandomValues(new Uint8Array(16))); keep(kept()); $('st-newpass').value = '';
  $('st-locknote').textContent = 'Sealed. This browser asks for the passphrase before it broadcasts again.';
};
$('st-tabonly').onchange = () => {
  const l = kept(); mineSeal = null;
  if ($('st-tabonly').checked) { try { sessionStorage.setItem('pyrate.mine', JSON.stringify(l)) } catch (e) { } unstore('pyrate.mine'); $('st-locknote').textContent = 'The key goes when this tab closes. The 24 words bring it back.' }
  else { try { sessionStorage.removeItem('pyrate.mine') } catch (e) { } keepSealed('pyrate.mine', JSON.stringify(l)); $('st-locknote').textContent = 'The key stays in this browser, sealed under a key the browser holds.' }
};
$('raisego').onclick = () => $('studio').scrollIntoView({ behavior: 'smooth', block: 'start' });
$('st-raise').onclick = async () => {
  if (sealed() && !mineMem) { stNote('Open the sealed stations first.'); return }
  const name = $('st-name').value.trim(); if (!name) { stNote('A station needs a name.'); $('st-name').focus(); return }
  if (!radio) { stNote('Coming aboard first.'); booting ??= boot(); await booting; if (!radio) return }
  let r; try { r = JSON.parse(radio.raise(name, $('st-about').value, $('st-kinds').value, $('st-mature').checked, $('st-explicit').checked, $('st-ppass').value)) } catch (e) { stNote(String(e)); return }
  const l = kept(); l.push({ id: r.id, deputy: r.deputy, pow: null }); keep(l);
  mineCur = r.id; adopted.add(r.id); powWork(r.id);
  const withPass = !!$('st-ppass').value; $('st-ppass').value = '';
  $('st-passnote').hidden = !withPass;
  $('st-wordlist').innerHTML = r.words.split(/\s+/).map(w => '<li>' + esc(w) + '</li>').join('');
  $('st-written').checked = false; $('st-done').disabled = true;
  $('st-new').hidden = true; $('st-show').hidden = false; stNote('');
  $('st-show').scrollIntoView({ behavior: 'smooth', block: 'center' });
};
$('st-written').onchange = () => { $('st-done').disabled = !$('st-written').checked };
$('st-done').onclick = () => { $('st-wordlist').innerHTML = ''; $('st-show').hidden = true; studioShow(); studioStatus() };
$('st-restore').onclick = async () => {
  if (sealed() && !mineMem) { stNote('Open the sealed stations first.'); return }
  const words = $('st-words').value.trim().toLowerCase().split(/\s+/).join(' ');
  if (words.split(' ').length !== 24) { stNote('A station’s key is twenty-four words.'); return }
  if (!radio) { booting ??= boot(); await booting; if (!radio) return }
  stNote('Looking for the station’s log on the network.');
  try {
    const r = JSON.parse(await radio.restore(words, $('st-rpass').value)); $('st-rpass').value = '';
    $('st-words').value = '';
    const l = kept().filter(k => k.id !== r.id); l.push({ id: r.id, deputy: r.deputy, pow: null }); keep(l);
    mineCur = r.id; adopted.add(r.id); powWork(r.id); stNote(''); studioShow();
  } catch (e) { stNote(String(e)) }
};
$('st-another').onclick = () => { mineCur = null; $('st-name').value = $('st-about').value = $('st-kinds').value = ''; studioShow() };
$('st-tune').onclick = () => { const s = stations.find(x => x.id === mineCur) || { id: mineCur, name: $('st-mname').textContent, call_sign: '' }; tuneTo(s); scrollTo({ top: 0, behavior: 'smooth' }) };

// libopus in a worker, one job at a time.
let enc = null, encSeq = 0; const encWait = new Map();
function encode(kind, data = {}, transfer = []) {
  if (!enc) { enc = new Worker('encode-worker.js'); enc.onmessage = e => { const p = encWait.get(e.data.id); encWait.delete(e.data.id); e.data.error ? p?.rej(new Error(e.data.error)) : p?.res(e.data) }; enc.onerror = e => { for (const p of encWait.values()) p.rej(new Error('the encoder would not start')); encWait.clear() } }
  return new Promise((res, rej) => { const id = ++encSeq; encWait.set(id, { res, rej }); enc.postMessage({ id, kind, ...data }, transfer) });
}
// A title and a name from a file's own name: "Artist - Title.mp3" reads as both.
// As the app's plain_title: a recorder's date and time, and its own words around them, never
// become a title. A bare date stays, since a keeper may mean it.
const DEVICE = new Set(['vid', 'img', 'aud', 'rec', 'pxl', 'voice', 'recording', 'audio', 'video', 'sound', 'new', 'memo', 'screen', 'record']);
function plainTitle(s) {
  const t = s.split(/[\s_]+/).filter(Boolean), keep = t.map(() => true);
  const stampy = x => /^[0-9.:hms-]+$/.test(x) && (x.match(/\d/g) || []).length >= 2;
  let cut = false;
  for (let i = 0; i < t.length;) {
    if (!stampy(t[i])) { i++; continue }
    let j = i, d = 0; while (j < t.length && stampy(t[j])) { d += t[j].match(/\d/g).length; j++ }
    if (d >= 12) { for (let k = i; k < j; k++) keep[k] = false; cut = true }
    i = j;
  }
  if (!cut) return { text: s.trim(), cut };
  t.forEach((x, k) => { if (keep[k] && DEVICE.has(x.replace(/[^\p{L}\p{N}]/gu, '').toLowerCase())) keep[k] = false });
  const out = t.filter((_, k) => keep[k]).join(' ').replace(/^[\s.-]+|[\s.-]+$/g, '');
  return { text: out || 'Untitled', cut };
}
function titled(f) {
  const p = plainTitle(f.name.replace(/\.[a-z0-9]{1,5}$/i, '')), base = p.text.replace(/_+/g, ' ').replace(/\s+/g, ' ').trim();
  const m = base.match(/^(.{1,80}?)\s+[-–—]\s+(.+)$/);
  return m ? { title: m[2], by: m[1], cut: p.cut } : { title: base, by: '', cut: p.cut };
}
const BLOCK = 48000 * 4;
function block(chans, at, n, ch, gain = 1) {
  const out = new Float32Array(n * ch), L = chans[0], R = chans[1] || chans[0];
  for (let i = 0; i < n; i++) {
    if (ch === 1) out[i] = (L[at + i] + R[at + i]) * 0.5 * gain;
    else { out[2 * i] = L[at + i] * gain; out[2 * i + 1] = R[at + i] * gain }
  }
  return out;
}
async function airFile(f, row) {
  const st = (t, p) => { row.querySelector('.qs').textContent = t; if (p !== undefined) row.querySelector('.bar i').style.width = Math.round(p * 100) + '%' };
  const talk = $('st-talk').checked, ch = talk ? 1 : 2, bitrate = talk ? 24000 : 64000;
  try {
    st('Reading', 0);
    const raw = await f.arrayBuffer();
    st('Decoding', 0.05);
    // A context at 48 kHz decodes and resamples in one step.
    const ab = await new OfflineAudioContext(1, 1, 48000).decodeAudioData(raw);
    const chans = [ab.getChannelData(0)]; if (ab.numberOfChannels > 1) chans.push(ab.getChannelData(1));
    const frames = ab.length;
    if (frames < 48000) throw new Error('shorter than a second');
    st('Levelling', 0.1);
    radio.level_begin(ch);
    for (let i = 0; i < frames; i += BLOCK) { radio.level_feed(block(chans, i, Math.min(BLOCK, frames - i), ch)); if ((i / BLOCK) % 8 === 7) await sleep(0) }
    const lv = JSON.parse(radio.level_end());
    st('Encoding', 0.2);
    await encode('begin', { channels: ch, bitrate, talk, disguise: $('st-disguise').checked ? -4 : 0 });
    for (let i = lv.start; i < lv.end; i += BLOCK) {
      const fb = block(chans, i, Math.min(BLOCK, lv.end - i), ch, lv.gain), pcm = new Int16Array(fb.length);
      for (let j = 0; j < fb.length; j++) pcm[j] = Math.max(-1, Math.min(1, fb[j])) * 32767;
      await encode('feed', { pcm }, [pcm.buffer]);
      st('Encoding', 0.2 + 0.7 * (i - lv.start) / Math.max(1, lv.end - lv.start));
    }
    const done = await encode('end');
    st('Signing', 0.95);
    const t = titled(f);
    radio.add_track(mineCur, t.title, t.by, $('st-source').value, ch, bitrate, done.out);
    st('On the clock', 1); row.classList.add('done');
  } catch (e) { st('Not set: ' + (e.message || e)); row.classList.add('bad') }
}
async function airFiles(list) {
  if (!mineCur || !adopted.has(mineCur)) return;
  const files = [...list].filter(f => f.size > 0);
  const rows = files.map(f => { const r = document.createElement('div'); r.className = 'qi'; r.innerHTML = '<span class="qt"></span><span class="bar"><i></i></span><span class="qs">Waiting</span>'; const tf = titled(f); r.querySelector('.qt').textContent = tf.title; if (tf.cut) r.title = 'The date and time the file was made are left out of its title.'; $('st-queue').prepend(r); return r });
  for (const [i, f] of files.entries()) { airing++; try { await airFile(f, rows[i]) } finally { airing-- } }
  studioStatus();
}
// Live from the tab: the microphone, nothing else, only while the button is held on.
let live = null;
$('st-live').onmouseenter = () => { $('st-livenote').hidden = false };
$('st-live').onclick = async () => {
  if (live) return endLive();
  if (!mineCur || !adopted.has(mineCur)) return;
  let stream;
  try { stream = await navigator.mediaDevices.getUserMedia({ audio: { channelCount: 1, echoCancellation: true, noiseSuppression: true, autoGainControl: true } }) }
  catch (e) { $('st-livenote').hidden = false; $('st-livenote').textContent = window !== parent ? 'A page holding the radio cannot lend it the microphone. Open the open player itself.' : 'The microphone was not allowed. Your browser’s site settings can allow it.'; return }
  const w = new Worker('encode-worker.js'), id = mineCur;
  const ac = new AudioContext({ sampleRate: 48000 }), src = ac.createMediaStreamSource(stream), proc = ac.createScriptProcessor(4096, 1, 1), mute = ac.createGain();
  mute.gain.value = 0; src.connect(proc); proc.connect(mute); mute.connect(ac.destination);
  w.onmessage = e => { if (e.data.live || (e.data.out && e.data.frames !== undefined)) { try { if (e.data.out?.length) radio.live_chunk(id, e.data.out, 1, 24000) } catch (err) { console.warn(err) } } if (e.data.out && !e.data.live) w.terminate() };
  w.postMessage({ id: 0, kind: 'begin', channels: 1, bitrate: 24000, talk: true, live: true, disguise: $('st-disguise').checked ? -4 : 0 });
  proc.onaudioprocess = ev => { const f = ev.inputBuffer.getChannelData(0), pcm = new Int16Array(f.length); for (let i = 0; i < f.length; i++) pcm[i] = Math.max(-1, Math.min(1, f[i])) * 32767; w.postMessage({ id: 0, kind: 'feed', pcm }, [pcm.buffer]) };
  const t0 = Date.now();
  live = { stream, ac, proc, w, timer: setInterval(() => { const s = Math.floor((Date.now() - t0) / 1000); $('st-livetime').textContent = 'ON AIR ' + String(Math.floor(s / 60)).padStart(2, '0') + ':' + String(s % 60).padStart(2, '0') }, 500) };
  $('st-live').classList.add('on'); $('st-live').querySelector('.lbl').textContent = 'End the broadcast';
};
function endLive() {
  const l = live; live = null; if (!l) return;
  l.stream.getTracks().forEach(t => t.stop()); l.proc.disconnect(); l.ac.close();
  // What is left of the last six seconds goes out too.
  l.w.postMessage({ id: 1, kind: 'end' });
  clearInterval(l.timer); $('st-livetime').textContent = '';
  $('st-live').classList.remove('on'); $('st-live').querySelector('.lbl').textContent = 'Go live';
}
$('st-files').onchange = e => { airFiles(e.target.files); e.target.value = '' };
const drop = $('st-drop');
drop.addEventListener('dragover', e => { e.preventDefault(); drop.classList.add('over') });
drop.addEventListener('dragleave', () => drop.classList.remove('over'));
drop.addEventListener('drop', e => { e.preventDefault(); drop.classList.remove('over'); airFiles(e.dataTransfer.files) });
// Closing the tab while recordings are still sailing would leave them half aboard.
addEventListener('beforeunload', e => {
  if (!radio || SCOPE !== 'open') return;
  const busy = airing > 0 || JSON.parse(radio.mine()).some(m => m.unsent > 0);
  if (busy) { e.preventDefault(); e.returnValue = '' }
});

// A copy of the set served anywhere but hostis.black/pyrateradio/listen/ (a Lantern's
// /player/, a mirror) points its links to the landing at the landing itself.
if (!/\/pyrateradio\/listen\/$/.test(location.pathname)) document.querySelectorAll('a[href^="../"]').forEach(a => a.href = 'https://hostis.black/pyrateradio/' + a.getAttribute('href').slice(3));

async function boot() {
  if (handOff()) return;
  // An invitation's key leaves the address bar before anything else happens.
  const invite = location.hash.includes('pocket=') ? location.hash : null;
  if (invite) history.replaceState(null, '', location.pathname);
  scale();
  $('signal').textContent = 'Coming aboard';
  try { await init() } catch (e) { $('name').textContent = 'This browser cannot run the radio'; return }
  // The sealed secrets come back before anything uses them.
  try { pocketsMem = JSON.parse(await dopen(store('pyrate.pockets') || '[]') || '[]') } catch (e) { pocketsMem = [] }
  if (!sealed() && !tabKeys()) { const m = store('pyrate.mine'); if (m) { const t = await dopen(m); try { const l = validKeys(JSON.parse(t || '[]')); if (l.length) { mineMem = l; if (t === m) keepSealed('pyrate.mine', m) } } catch (e) { } } }
  // The counting key is replaced every thirty days, so a key taken from this browser cannot
  // tie its tokens back further than that.
  let key = null;
  try { const t = JSON.parse(await dopen(store('pyrate.token') || '') || 'null'); if (t && /^[0-9a-f]{64}$/.test(t.k) && Date.now() - t.at < 30 * 86400e3) key = t.k } catch (e) { }
  if (!key) { const b = new Uint8Array(32); crypto.getRandomValues(b); key = [...b].map(x => x.toString(16).padStart(2, '0')).join(''); keepSealed('pyrate.token', JSON.stringify({ k: key, at: Date.now() })) }
  // No relay answering is not the end: the set says so and tries again, more slowly each time.
  for (let tries = 0; !radio; tries++) {
    try { radio = await Radio.start(rememberHarbours(), key, RELAYS) } catch (e) {
      console.warn(e);
      $('name').textContent = 'Could not reach a relay';
      $('signal').textContent = 'Trying again. If it never connects, this network may block the radio. Try another network, or the app.';
      await sleep(Math.min(60000, 5000 * 2 ** tries));
    }
  }
  $('name').textContent = '';
  radio.set_direct_ok(store('pyrate.direct') === '1');
  bootAt = Date.now();
  if (!HOSTIS_ONLY) for (const l of savedPockets()) pocketJoin(l, false);
  if (invite) pocketJoin(invite, true);
  else if (!flockFollow()) followLink();
  refresh();
  setInterval(refresh, 4000); setInterval(nowPlaying, 1000); setInterval(() => page(false), 5000);
  studioBoot();
  window.addEventListener('hashchange', () => handOff() || followLink());
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

// Back in view, or back from another app's sound: the set makes sure it is still sounding.
document.addEventListener('visibilitychange', () => { if (!document.hidden && playing && ctx && ctx.state !== 'running') ctx.resume().catch(() => { }) });

// For anyone looking under the hood: what the player is doing.
let tapped = null;
window.pyrate = { mine: () => kept().map(k => k.id), tap: () => { if (!ctx) return null; if (!tapped) { tapped = ctx.createMediaStreamDestination(); gain.connect(tapped) } return tapped.stream }, plan: at => radio && cur ? JSON.parse(radio.plan(cur.id, at ?? radio.now())) : null, state: () => ({ playing, station: cur?.id, scheduled: sources.length, ahead_ms: playing ? aheadMs : 0, ctx: ctx?.state, neighbours: radio?.neighbours(), stations: stations.length, webcodecs }) };
// The set at rest, before it joins the network: a HOSTIS station, or the one passed on.
function dormant(s) {
  cur = s; flag($('flag'), s.id, false); art(null);
  $('name').textContent = s.name; $('cs').textContent = s.call_sign || ''; $('freq').textContent = fq(s.id).toFixed(1);
  dial(HARBOUR_STATIONS);
  needle(s.id, false); power(); $('pass').hidden = true; $('np').textContent = 'Press Listen to come aboard.'; $('by').textContent = '';
}
// Inside the landing page, the set tells its host how tall it is.
if (document.documentElement.classList.contains('embed') && parent !== window) {
  // The body's own height, not the document's: a document is never shorter than its frame, so it could grow the frame but never let it shrink back.
  const tell = () => parent.postMessage({ pyrate: 'height', h: Math.ceil(document.body.getBoundingClientRect().height) }, location.origin);
  new ResizeObserver(tell).observe(document.body); addEventListener('load', tell);
}
document.querySelectorAll('a.openlink').forEach(a => { if (!a.id) a.href = OPEN_PLAYERS[0] });
if (SHARED) { const m = location.pathname.match(/^\/ipfs\/([a-z2-7]{20,})\//); $('sharedgo').href = m ? 'https://' + m[1] + '.ipfs.dweb.link/' + location.hash : OPEN_PLAYERS[0] || '#'; $('sharednote').hidden = false }
if (EMBED) { scale(); const p = passedId(); dormant(p && isHarbour(p) ? HARBOUR_STATIONS.find(h => h.id === p) : HARBOUR_STATIONS[0]); $('signal').textContent = '' }
else booting = boot();
// In the page, the radio's engine loads from this site while the page is read, so Listen
// starts at once. Nothing goes to the network itself until Listen is pressed.
if (EMBED) { const warm = () => init().catch(() => { }); if ('requestIdleCallback' in window) requestIdleCallback(warm, { timeout: 2500 }); else setTimeout(warm, 1200) }
