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
// HOSTIS's stations carry their pictures in the page, so they show before the network answers.
const baked = (id, k = 'art') => !sw.art() ? null : HARBOUR_STATIONS.find(h => h.id === id)?.[k] || null;
const lastId = () => { if (SHARED) return ''; try { const id = sessionStorage.getItem('pyrate.cur') || ''; return marooned().has(id) ? '' : id } catch (e) { return '' } };
const passedId = () => { const id = location.hash.slice(1); return /^[0-9a-f]{64}$/.test(id) ? id : null };
const inSet = s => !HOSTIS_ONLY || isHarbour(s.id);
// The open player's fixed name if it answers within a few seconds, since a browser keeps what it
// keeps there from one release to the next; otherwise whichever other copy answers first.
// A copy that checks every file against its IPFS name in the visitor's own browser (inbrowser.link),
// or a test build's own. Other gateways serve whatever they are given, unchecked.
const checked = u => /\.inbrowser\.link\//.test(u) || /^http:\/\/(127\.0\.0\.1|localhost)[:/]/.test(u);
// `secret`: the link carries an invitation or a flock, so it goes only to a checked copy.
async function openPlayer(secret) {
  const list = secret ? OPEN_PLAYERS.filter(checked) : OPEN_PLAYERS;
  const ping = (u, ms) => fetch(u + 'favicon.png', { mode: 'no-cors', cache: 'no-store', signal: AbortSignal.timeout(ms) }).then(() => u);
  try { return await ping(list[0], 4000) } catch (e) { }
  try { return await Promise.any(list.slice(1).map(u => ping(u, 3500))) } catch (e) { return list[0] }
}
// A link for anything that is not HOSTIS's own leaves this copy before it is read: the part
// after the # never reaches a server, and this page never shows, names or plays what it holds.
const foreign = h => /pocket=|flock=/.test(h) || (/^#[0-9a-f]{64}$/.test(h) && !isHarbour(h.slice(1)));
function handOff() {
  const h = location.hash; if (!HOSTIS_ONLY || EMBED || !foreign(h)) return false;
  history.replaceState(null, '', location.pathname + location.search);
  document.documentElement.classList.add('leaving');
  openPlayer(/pocket=|flock=/.test(h)).then(u => location.replace(u + h));
  return true;
}
const RELAYS = '{{RELAYS}}';
const AHEAD_MS = 12000;
const $ = id => document.getElementById(id);
const sleep = ms => new Promise(r => setTimeout(r, ms));

let radio = null, stations = [], cur = null, playing = false, gen = 0, bootAt = 0, booting = null;
let ctx = null, gain = null, bus = null, sources = [], meter = null;
// Where the set is with the station it plays, said the same way on the set, in the mini player,
// in the tab's title and on the phone's own controls. `stage` is how far tuning in has got;
// `firstAt` is when the first sound of this tuning starts, in the sound card's time. The
// title shows only from that moment, so nothing names a song before it is heard.
let stage = null, firstAt = null, fadeNext = false, busySince = 0, showSeq = 0, hissG = null;
// Whether the player is following a live broadcast, its last words included once it has ended:
// the set says Live for as long as it is, whatever the log says by then.
let liveOn = false;
const STAGES = { connecting: 'Connecting to the radio network', finding: 'Finding the station', tuning: 'Tuning in' };
const heard = () => playing && firstAt !== null && !!ctx && ctx.currentTime >= firstAt - 0.02;
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
// With no device key there is nothing to seal under, and nothing is sealed: the caller keeps
// the secret for the tab's life only, never in lasting storage in the clear.
async function dseal(text) {
  const k = await deviceKey(); if (!k) return null;
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
let unsealable = false;
async function keepSealed(k, text) {
  const sealedText = SHARED ? text : await dseal(text);
  if (sealedText !== null) { store(k, sealedText); return }
  try { sessionStorage.setItem(k, text) } catch (e) { }
  try { localStorage.removeItem(k) } catch (e) { }
  if (!unsealable) { unsealable = true; tabOnlyNote() }
}
// A secret kept for the tab alone, when this browser could not seal it.
const keptAnywhere = k => store(k) ?? (() => { try { return sessionStorage.getItem(k) } catch (e) { return null } })();
function tabOnlyNote() {
  const n = document.getElementById('st-locknote'); if (!n) return;
  n.textContent = 'This browser cannot lock keys away, so your station\u2019s key is kept only until this tab closes. Your 24 words bring it back.';
  const f = document.getElementById('st-safer'); if (f) f.open = true;
}

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
const sw = { mature: () => store('pyrate.hide_mature') === '1', explicit: () => store('pyrate.show_explicit') === '1', art: () => store('pyrate.hide_art') !== '1', hiss: () => store('pyrate.hiss') !== '0', empty: () => store('pyrate.show_empty') === '1' };
// A station that says it has no recordings and is not live has nothing to play yet: it stays
// out of the list and the dial until it does, unless the listener keeps it or asks to see them.
const empty = s => s.tracks === 0 && !s.live;
// Somewhere to land: Spin, the dial's steps and the station a visit opens on pass over one with
// nothing to play, by its card or, where this tab has its log, by what the log says is on now.
function playable(s) {
  if (!s || empty(s)) return false;
  if (!radio || !radio.has_log(s.id)) return true;
  const k = JSON.parse(radio.plan(s.id, radio.now())).kind;
  return k !== 'dead air' && k !== 'struck';
}
function allowed(s) { return !((s.explicit && !sw.explicit()) || (sw.mature() && (s.mature || s.explicit)) || (empty(s) && !sw.empty() && !keptSet.has(s.id))) }

// ---- What this browser keeps of its own choosing ------------------------------------------
// Kept stations come first in the list. The list is sealed under the browser's own key, like
// invitations, so a copy of the profile does not say what its owner listens to.
let keptSet = new Set();
function saveKept() { keepSealed('pyrate.kept', JSON.stringify([...keptSet])) }

// Two stations whose names read alike, letter for letter or by look-alike letters from other
// alphabets: the one that has worn the name longer is shown as it is, and the newer ones say
// so beside their names. Nothing is hidden or judged; the listener sees which came first.
const LOOK = { 'а': 'a', 'е': 'e', 'о': 'o', 'р': 'p', 'с': 'c', 'у': 'y', 'х': 'x', 'і': 'i', 'ј': 'j', 'ѕ': 's', 'к': 'k', 'м': 'm', 'н': 'h', 'т': 't', 'в': 'b', 'ο': 'o', 'α': 'a', 'ν': 'v', 'ρ': 'p', 'τ': 't', 'ι': 'i', 'κ': 'k', 'ε': 'e', 'υ': 'u', '0': 'o', '1': 'l' };
const skel = name => [...norm(name)].map(c => LOOK[c] || c).join('').replace(/[^a-z0-9]/g, '');
function twins(list) {
  const first = new Map();
  const age = s => s.named || s.since || 0;
  for (const s of list) { const k = skel(s.name); if (!k) continue; const f = first.get(k); if (!f || age(s) < age(f)) first.set(k, s) }
  for (const s of list) { const f = first.get(skel(s.name)); s.twin = !!f && f.id !== s.id }
}
// Kinds as stations write them, gathered into one spelling: "Hip-Hop", "hip hop" and "hiphop"
// are one kind.
const kindOf = k => norm(k).replace(/[-_./]+/g, ' ').replace(/\s+/g, ' ').trim();
const played = []; let sleepAt = null, sleepT = null; // What this tab has heard, kept in memory only, and the sleep timer.
let kindOn = null, sortBy = store('pyrate.sort') || 'aboard', kindsKey = '';

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
    // In the first seconds the radio may not have its routes yet: a picture that does not come
    // then is asked for again at the next look, not held back for two minutes.
    .catch(() => { pics.delete(hash); if (Date.now() - bootAt > 15000) picFailed.set(hash, Date.now()); return null });
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
  gain = ctx.createGain(); bus = ctx.createGain(); gain.connect(bus);
  meter = ctx.createAnalyser(); meter.fftSize = 1024; gain.connect(meter); vu();
  // On iPhones and iPads a page's own audio stops when the screen locks. Sent through an
  // audio element instead, it keeps playing and shows on the lock screen. An iPad's Safari
  // calls itself a Mac, and only its touch screen gives it away.
  const apple = /iPhone|iPad|iPod/.test(navigator.userAgent) || (navigator.platform === 'MacIntel' && navigator.maxTouchPoints > 1);
  // A call or another app can take the sound away; when it gives it back, the set picks up.
  ctx.onstatechange = () => { if (playing && ctx.state !== 'running' && ctx.state !== 'closed') ctx.resume().catch(() => { }) };
  if (apple && ctx.createMediaStreamDestination) {
    const dest = ctx.createMediaStreamDestination(); bus.connect(dest);
    const el = $('a'); el.srcObject = dest.stream; el.play().catch(() => { bus.disconnect(); bus.connect(ctx.destination) });
    el.onpause = () => { if (playing) el.play().catch(() => { }) };
  } else bus.connect(ctx.destination);
}

function stopAll() { for (const s of sources) { try { s.stop() } catch (e) { } } sources = [] }
// The sound already queued fades out over a moment instead of cutting off mid-note.
function fadeOut() {
  if (!ctx) return;
  const t = ctx.currentTime, g = gain.gain;
  g.cancelScheduledValues(t); g.setValueAtTime(g.value, t); g.linearRampToValueAtTime(0, t + 0.15);
  for (const s of sources) { try { s.stop(t + 0.16) } catch (e) { } }
  sources = [];
}
// A soft hiss while the set tunes in, so pressing Listen is heard at once. It is made here,
// from noise, and gives way to the station's sound the moment that starts.
function hiss(on, at) {
  if (!ctx) return;
  if (!hissG) {
    hissG = ctx.createGain(); hissG.gain.value = 0;
    const n = ctx.createBuffer(1, 96000, 48000), d = n.getChannelData(0);
    for (let i = 0, b = 0; i < d.length; i++) { b = 0.86 * b + 0.14 * (Math.random() * 2 - 1); d[i] = b * 2.2 }
    const src = ctx.createBufferSource(); src.buffer = n; src.loop = true;
    const f = ctx.createBiquadFilter(); f.type = 'bandpass'; f.frequency.value = 1800; f.Q.value = 0.4;
    src.connect(f); f.connect(hissG); hissG.connect(bus); src.start();
  }
  const g = hissG.gain, t = at ?? ctx.currentTime;
  if (at === undefined) { g.cancelScheduledValues(t); g.setValueAtTime(g.value, t) }
  g.setTargetAtTime(on && sw.hiss() ? 0.05 : 0, t, on ? 0.12 : 0.18);
}

function schedule(chs, skipMs, when) {
  const skip = Math.min(Math.round(skipMs * 48), chs[0].length);
  const n = chs[0].length - skip;
  if (n <= 0) return when;
  const buf = ctx.createBuffer(2, n, 48000);
  buf.copyToChannel(chs[0].subarray(skip), 0);
  buf.copyToChannel((chs[1] || chs[0]).subarray(skip), 1);
  const src = ctx.createBufferSource(); src.buffer = buf; src.connect(gain); src.start(when);
  sources.push(src); src.onended = () => { sources = sources.filter(x => x !== src) };
  if (fadeNext) {
    fadeNext = false;
    const g = gain.gain; g.cancelScheduledValues(when); g.setValueAtTime(0, when); g.linearRampToValueAtTime(1, when + 0.3);
    hiss(false, when);
    if (firstAt === null) { firstAt = when; setTimeout(() => { power(); paint() }, Math.max(0, (when - ctx.currentTime) * 1000) + 30) }
  }
  return when + n / 48000;
}

// A word the set keeps showing after it has stopped, until Listen is pressed again.
let note = null;
function status(t) { note = t; paint() }
// A trouble the player is in, shown in place of the title until it clears.
let trouble = null, aheadMs = 0;

// ---- The set: frequencies, the dial glass, the lamps and the meter ----------------------
// Every station sits at the same place on every dial: its frequency comes from its key.
// Every station has a frequency of its own, to the kilohertz: the one its card claims, or for a
// card from before claims the one its key points at (the tenth it always had, and two figures
// more). If two ever claim one, the one this browser keeps holds it, else the one on the air
// longer, and the other moves to the next free place up the dial.
const prefFreq = id => (875 + parseInt(id.slice(0, 6), 16) % 206) * 100 + parseInt(id.slice(6, 10), 16) % 100;
const FLOW = 87500, FHIGH = 108099;
let freqs = new Map();
function resolveFreqs(list) {
  const m = new Map(), taken = new Set();
  const order = [...list].sort((a, b) => (keptSet.has(b.id) - keptSet.has(a.id)) || (a.since || 9e15) - (b.since || 9e15) || (a.id < b.id ? -1 : 1));
  for (const s of order) {
    let f = s.freq >= FLOW && s.freq <= FHIGH ? s.freq : prefFreq(s.id);
    for (let i = 0; taken.has(f) && i < 20600; i++) f = f >= FHIGH ? FLOW : f + 1;
    taken.add(f); m.set(s.id, f);
  }
  freqs = m;
}
const fq = id => (freqs.get(id) ?? prefFreq(id)) / 1000;
const fqs = id => fq(id).toFixed(3);
// Inset from the glass's edges, so the end numerals and names stay readable.
const at = f => 3 + (f - 87.5) / 20.6 * 94;
let dialIds = '', order = [];
function scale() {
  let h = '';
  for (let f = 88; f <= 108; f += 0.5) h += '<i style="left:' + at(f) + '%"' + (f % 2 === 0 ? ' class="major"' : '') + '></i>' + (f % 4 === 0 ? '<b style="left:' + at(f) + '%">' + f + '</b>' : '');
  $('scale').innerHTML = h;
  let t = ''; for (let a = -48; a <= 48; a += 12) { const r = a * Math.PI / 180; t += '<line x1="' + (60 + 46 * Math.sin(r)) + '" y1="' + (64 - 46 * Math.cos(r)) + '" x2="' + (60 + 52 * Math.sin(r)) + '" y2="' + (64 - 52 * Math.cos(r)) + '"/>' }
  $('ticks').innerHTML = t;
}
// With hundreds of stations on the air the glass names the eighty most listened and the one
// on the set; the rest are a search away in the list.
const DIAL_NAMES = 80;
function dial(list) {
  // The eighty most listened, the station on the set, and the stations this browser keeps.
  const named = list.slice(0, DIAL_NAMES);
  for (const s of list.slice(DIAL_NAMES)) if (s.id === cur?.id || keptSet.has(s.id)) named.push(s);
  const ids = named.map(s => s.id + (s.live ? '*' : '')).join() + '#' + (cur?.id || '');
  if (ids === dialIds) return;
  dialIds = ids; order = named.sort((a, b) => fq(a.id) - fq(b.id));
  // The track stands in from the glass's ends (player.css), so a name at 88 or 108 is never cut.
  $('track').style.width = Math.max($('glass').clientWidth - 72, Math.min(order.length, DIAL_NAMES) * 60) + 'px';
  $('names').innerHTML = '';
  // Each name goes in the first of three rows with room for it, so no two names overlap; a
  // name with no room left stays off the glass (the station on the set always has one), and
  // the needle still finds it.
  const wide = $('track').clientWidth || 1, taken = [[], [], []], xOf = new Map();
  const place = s => {
    const w = Math.min(176, 14 + 7.4 * (s.name || s.call_sign || '').length), x = Math.min(Math.max(wide * at(fq(s.id)) / 100, w / 2), wide - w / 2), l = x - w / 2 - 4, h = x + w / 2 + 4;
    xOf.set(s.id, x);
    const r = taken.findIndex(row => row.every(([a, b]) => h <= a || l >= b));
    if (r >= 0) taken[r].push([l, h]);
    return r;
  };
  const onSet = order.find(s => s.id === cur?.id), rowOf = new Map();
  if (onSet) rowOf.set(onSet.id, place(onSet));
  for (const s of order) if (!rowOf.has(s.id)) rowOf.set(s.id, place(s));
  for (const s of order) {
    const r = rowOf.get(s.id); if (r < 0) continue;
    const b = document.createElement('button'); b.textContent = s.name || s.call_sign; b.dataset.id = s.id; b.title = fqs(s.id) + '  ' + (s.name || '');
    b.style.left = (xOf.has(s.id) ? xOf.get(s.id) / wide * 100 : at(fq(s.id))) + '%'; b.style.top = r * 24 + 'px'; b.classList.toggle('live', !!s.live); b.setAttribute('role', 'option');
    b.onclick = e => { e.stopPropagation(); tuneTo(s) };
    b.onpointerenter = () => prepare(s.id);
    $('names').appendChild(b);
  }
  if (cur) needle(cur.id, false);
}
function needle(id, smooth = true) {
  $('needle').style.left = at(fq(id)) + '%';
  document.querySelectorAll('#names button').forEach(b => { const on = b.dataset.id === id; b.classList.toggle('on', on); b.setAttribute('aria-selected', on) });
  const g = $('glass'), x = $('track').offsetLeft + $('track').clientWidth * at(fq(id)) / 100;
  if (g.scrollWidth > g.clientWidth) g.scrollTo({ left: x - g.clientWidth / 2, behavior: smooth ? 'smooth' : 'auto' });
  $('freq').textContent = fqs(id);
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
  stepDial(e.key === 'ArrowRight' ? 1 : -1);
});
function stepDial(d) {
  if (!order.length) return;
  const i = order.findIndex(s => s.id === cur?.id), n = order.length;
  for (let k = 1; k <= n; k++) { const s = order[((i + d * k) % n + n) % n]; if (s.id !== cur?.id && playable(s)) { landed(s, () => stepDial(d)); return tuneTo(s) } }
}
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
  let playhead = null, next = 0, track = null, dec = null, liveSeq = null, misses = 0, wait = 600, quiet = 2000, stumbles = 0, syncing = null, syncedAt = 0;
  const reset = () => { stopAll(); fadeNext = true; playhead = null; next = 0; track = null; liveSeq = null; liveOn = false; dec?.close(); dec = null };
  const said = (st, t) => { if (stage !== st || trouble !== t) { stage = st; trouble = t; paint() } };
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
      let p = JSON.parse(radio.plan(id, at));
      if (p.kind === 'struck') { reset(); hiss(false); trouble = null; stop(); refresh(); return }
      // A log not here yet: the set plays the moment it arrives, and asks again every second
      // or three while it does not.
      if (p.kind === 'searching') {
        if (!queued) said('finding', null);
        // The log is asked for in the background and the plan looked at every 150 ms, so the
        // first page of the log plays as soon as it lands, while the rest still comes in.
        if (!syncing && Date.now() - syncedAt > wait) syncing = radio.sync(id).catch(() => 0).finally(() => { syncing = null; syncedAt = Date.now(); wait = Math.min(3000, wait * 1.5) });
        await sleep(150);
        continue;
      }
      // The broadcast has ended. What is queued of it is its last words, the sign-off: they play
      // out, with the set still saying Live, before the station's own schedule comes back.
      // Chunks written before the end that this tab has not fetched yet come first.
      if (liveSeq !== null && p.kind !== 'live') {
        let rest = []; try { rest = JSON.parse(radio.live_after(id, liveSeq)) } catch (e) { }
        if (rest.length) p = { kind: 'live', tail: rest };
        else { if (queued > 60) { await sleep(Math.min(500, queued)); continue } reset() }
      }
      wait = 600;
      if (!queued && stage === 'finding') said('tuning', null);
      // Off the air or silent: nothing to hear, so the set stops tuning and says why. The log is
      // asked for again meanwhile, less often the longer nothing comes, so a recording the
      // station adds is heard within seconds.
      if (p.kind === 'dead air' || p.kind === 'off air') {
        if (!queued) { reset(); hiss(false); if (firstAt === null) firstAt = ctx.currentTime; said(null, null); moveOn() }
        if (!syncing && Date.now() - syncedAt > quiet) syncing = radio.sync(id).catch(() => 0).finally(() => { syncing = null; syncedAt = Date.now(); quiet = Math.min(10000, quiet * 1.5) });
        await sleep(1000); continue;
      }
      let chunk, ch, ms, seq = null;
      if (p.kind === 'live') {
        const t = p.tail;
        const n = liveSeq === null ? (t[t.length - 2] || t[t.length - 1]) : t.find(x => x.seq > liveSeq);
        if (!n) { await radio.sync(id).catch(() => { }); await sleep(1500); continue }
        // From a track to the live broadcast: nothing of the track stays queued ahead of it.
        if (liveSeq === null) { reset(); dec = await Decoder.make(n.channels) }
        // A talk show that brings in music goes from mono to stereo at a chunk's edge, with an
        // encoder of its own: a decoder of its own meets it, and nothing queued is dropped.
        else if (dec?.ch !== n.channels) { dec?.close(); dec = await Decoder.make(n.channels) }
        ({ chunk, channels: ch, ms } = n); seq = n.seq; liveOn = true;
      } else {
        if (liveSeq !== null) reset();
        chunk = p.chunk; ch = p.channels; ms = p.chunk_ms;
      }
      let bytes;
      try { bytes = await radio.fetch(id, chunk); misses = 0; if (trouble) said(stage, null) } catch (e) {
        if (g !== gen) return;
        // Said only once the queue is nearly dry: a miss with seconds still queued is not heard.
        if (++misses > 2 && queued < 4000 && firstAt !== null) { said(stage, 'Signal lost. Tuning in again'); if (queued < 1000) hiss(true) }
        await sleep(misses > 4 ? 1500 : 700); continue;
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
      if (++stumbles > 5) { stop(); status('This browser could not play the station. Try reloading the page.'); return }
      said(stage, 'The player stumbled. Starting again');
      await sleep(800 * stumbles);
    }
  }
  dec?.close();
}

// ---- What the set says ------------------------------------------------------------------
// One reading of the state, put everywhere at once. While the set tunes in, it says how far it
// has got, with a moving line, and never a title: the title shows once its sound is heard.
let artWant = null, prefetched = '', showAt = 0, soundy = false, lastUse = Date.now();
for (const ev of ['pointerdown', 'keydown', 'scroll', 'touchstart']) addEventListener(ev, () => { lastUse = Date.now() }, { passive: true, capture: true });

// A station the listener is about to choose (a row pointed at or touched, the stations on the
// dial either side): its log and the sound on now come aboard first, so the tap plays at once.
// Each at most once in half a minute, and nothing is counted or said about it.
const prepared = new Map();
async function prepare(id) {
  if (!radio || !id || id === cur?.id && playing || Date.now() - (prepared.get(id) || 0) < 30000) return;
  // The station on the set comes first: nothing else is fetched while it is still tuning in, or
  // before its log is here, so the others never slow the one about to play.
  if (cur && id !== cur.id && (playing && !heard() || !radio.has_log(cur.id))) return;
  prepared.set(id, Date.now());
  try { radio.warm(id) } catch (e) { return }
  for (let i = 0; i < 20; i++) {
    const p = JSON.parse(radio.plan(id, radio.now()));
    if (p.kind === 'track') { radio.fetch(id, p.chunk).catch(() => { }); if (p.next) radio.fetch(id, p.next).catch(() => { }); return }
    if (p.kind !== 'searching') return;
    await sleep(500);
  }
}
// A link to a station nobody carries: once the radio has had the network's cards for a while and
// no radio speaks for it, the set says so within seconds, instead of tuning in for a minute.
const lost = () => playing && stage === 'finding' && busySince > 0 && Date.now() - busySince > 12000 && Date.now() - bootAt > 10000 && !stations.some(s => s.id === cur?.id && !s.cached && s.carriers);
// A station with no recordings yet, by its card or by its own log.
const bare = () => !!cur && ((pageFor === cur.id && pageV?.tracks === 0) || (cur.tracks === 0 && !cur.cached));
function paint() {
  if (!cur) return;
  const p0 = radio ? JSON.parse(radio.plan(cur.id, radio.now())) : null, on = heard();
  // The last words of a broadcast that has ended are still Live, and no title shows over them.
  const p = playing && liveOn ? { kind: 'live' } : p0;
  lamps(p?.kind === 'live');
  let line = null, busy = false;
  if (!playing && note) line = note;
  else if (playing && trouble) { line = trouble; busy = true }
  else if (playing && !on) { line = lost() ? 'No radio carries this station right now' : STAGES[stage] || STAGES.tuning; busy = true }
  else if (p?.kind === 'struck') line = 'This station has ended.';
  // A station with no recordings is not off the air until a time: it has nothing yet.
  else if ((p?.kind === 'off air' || p?.kind === 'dead air') && bare()) line = 'Nothing on the air yet';
  else if (p?.kind === 'off air') line = offAir(p.back_at);
  else if (p?.kind === 'dead air') line = 'Nothing on the air right now';
  const track = !line && p?.kind === 'track', live = !line && p?.kind === 'live';
  soundy = track || live;
  if (on && track && p.title && played.at(-1)?.key !== cur.id + p.track) { played.push({ key: cur.id + p.track, at: Date.now(), title: p.title, by: p.by || '', station: cur.name || '' }); if (played.length > 50) played.shift(); if (!$('morebox').hidden) playedList() }
  $('np').textContent = line ?? (track ? p.title || '' : live ? 'Live from the station' : '');
  $('by').textContent = track ? p.by || '' : '';
  // Before Listen, the line says what is on now, so a title is never mistaken for one playing.
  $('slot').textContent = !(track || live) ? '' : playing ? p.slot || '' : p.slot ? 'On now · ' + p.slot : 'On now';
  $('prog').style.width = track ? Math.min(100, p.position_ms / p.duration_ms * 100) + '%' : live ? '100%' : '0';
  $('time').textContent = track ? mmss(p.position_ms) + ' of ' + mmss(p.duration_ms) : '';
  $('progbar').classList.toggle('busy', busy);
  $('set').classList.toggle('tuning', busy);
  // Past ten seconds a wait gets a reason, and past forty-five a way out.
  if (!busy) busySince = 0; else if (!busySince) busySince = Date.now();
  const waited = busySince ? Date.now() - busySince : 0;
  $('hint').hidden = waited < 10000;
  $('hint').textContent = lost() ? 'It may come back on the air. Try another station meanwhile.' : waited > 45000 && stage === 'finding' ? 'No radio carrying this station has answered yet. Keep waiting, or try another station.' : stage === 'connecting' ? 'Still connecting. Some networks block the radio; another network, or the app, may get through.' : 'Still tuning in. The first station takes longest.';
  // The picture: the recording's own if it has one, the station's otherwise.
  const want = (track && p.art) || cur.cover || null, c = cur.id;
  if (want !== artWant) { artWant = want; if (!want) art(null); else pic(c, want).then(u => { if (cur?.id === c && artWant === want) { if (u) art(u); else artWant = undefined } }) }
  // A station on the set and not yet playing: its sound comes aboard ahead, the piece on now and
  // the one after, for as long as someone is using the page, so Listen starts at once.
  if (!playing && track && p.chunk !== prefetched && !document.hidden && Date.now() - Math.max(showAt, lastUse) < 600000) { prefetched = p.chunk; radio.fetch(c, p.chunk).catch(() => { }); if (p.next) radio.fetch(c, p.next).catch(() => { }) }
  mini(line ?? (track ? [p.title, p.by].filter(Boolean).join(' · ') : live ? 'Live' : ''));
  if ('mediaSession' in navigator) {
    if (playing && (on || stage)) {
      const m = navigator.mediaSession.metadata, title = track && on ? p.title || cur.name : cur.name, by = track && on ? p.by || '' : line || '';
      if (!m || m.title !== title || m.artist !== by) pic(c, want).then(u => { u = u || baked(c); if (cur?.id === c) navigator.mediaSession.metadata = new MediaMetadata({ title, artist: by, album: cur.name, artwork: u ? [{ src: u, sizes: '512x512', type: 'image/jpeg' }] : [] }) });
    }
    navigator.mediaSession.playbackState = playing ? 'playing' : 'paused';
  }
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

// The station page's heading: what the station says of itself, whether or not its log is here.
function pageHead() {
  if (!cur) return;
  $('pname').textContent = cur.name || cur.call_sign || 'A station';
  $('pabout').textContent = cur.about || '';
  $('pmeta').textContent = [cur.call_sign, fqs(cur.id) + ' on the dial', (cur.kinds || []).join(', '), cur.port, cur.aboard !== undefined && !cur.cached ? cur.aboard + ' listening' : '', cur.carriers ? cur.carriers + (cur.carriers === 1 ? ' radio carries it' : ' radios carry it') : ''].filter(Boolean).join('  ·  ');
  $('key').textContent = 'Key  ' + cur.id.match(/.{1,4}/g).join(' ');
  // What a listener may want to know before staying: a newer station with an older one's name,
  // nothing to play yet, or a keeper long gone (the station plays on from those who carry it).
  const away = cur.signed_ms && radio ? radio.now() - cur.signed_ms : 0, notes = [];
  if (cur.twin && !isHarbour(cur.id)) notes.push('Another station with this name has been on the air longer. Check the call sign before you share this one.');
  if (empty(cur)) notes.push('This station has nothing to play yet.');
  if (away > 365 * 86400e3) notes.push('Its keeper has not been aboard for over a year.');
  else if (away > 30 * 86400e3) notes.push('Its keeper has not been aboard for over a month.');
  $('pnote').hidden = !notes.length; $('pnote').textContent = notes.join(' ');
}

let pageFor = null, pageAt = 0, pageV = null;
function page(force) {
  if (!cur || !radio) return;
  if (force || pageFor !== cur.id || Date.now() - pageAt > 30000) { pageV = JSON.parse(radio.page(cur.id, 7)); pageFor = cur.id; pageAt = Date.now() }
  const v = pageV;
  $('page').hidden = $('week-box').hidden = !v; $('pagewait').hidden = !!v;
  if (!v) return;
  $('notices').innerHTML = (v.notices || []).map(n => '<div class="notice">' + esc(n.text) + '<small>' + esc(dayOf(n.at_ms) + ' ' + hm(n.at_ms)) + '</small></div>').join('');
  const next = (v.next || []).filter(n => n.start_ms > radio.now()).slice(0, 5);
  // Nothing up next: its heading and caption go together.
  $('nexth').parentElement.hidden = !next.length;
  $('next').innerHTML = next.map(n => '<div class="li"><span class="t">' + hm(n.start_ms) + '</span><span class="w">' + esc(n.title) + (n.by ? ' <em>' + esc(n.by) + '</em>' : '') + '</span></div>').join('');
  $('how').textContent = (v.tracks === 0 ? 'Nothing on the air yet. ' : !v.now && v.back_at ? offAir(v.back_at) + ' ' : '') + howItPlays(v.programme);
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

function art(url) { const c = $('cover'); url = url || baked(cur?.id); if (url) { if (c.src !== url) c.src = url; c.hidden = false; $('flag').hidden = true } else { c.hidden = true; $('flag').hidden = false } }

// Puts a station on the set. The name, the dial and the controls change at once; its pictures
// follow when they come, and a picture that comes after the set has moved on is dropped.
async function show(s) {
  if (hailFor && hailFor !== s.id) dropHail();
  const seq = ++showSeq, moved = cur?.id !== s.id;
  cur = s;
  if (moved) { showAt = Date.now(); flag($('flag'), s.id, s.live); artWant = undefined; art(null); $('banner').style.backgroundImage = ''; $('passbox').hidden = $('tipbox').hidden = $('clipbox').hidden = true }
  // On hostis.black the chatter is in the open player: this station's room, one tap away.
  if (SCOPE === 'harbour') $('chatopen').href = OPEN_PLAYERS[0] + '#' + s.id;
  $('name').textContent = s.name || s.call_sign || 'A station';
  $('cs').textContent = (s.call_sign || '') + (s.provisional ? '  unconfirmed' : '') + (s.aboard !== undefined ? '  ' + s.aboard + ' listening' : '');
  $('cs').title = s.provisional ? 'Signed by a co-host. Shown as unconfirmed until this player has the station\'s own log.' : '';
  lamps(s.live); needle(s.id); $('tip').hidden = !s.tip;
  $('keep').hidden = HOSTIS_ONLY || !!s.pocket; $('keep').setAttribute('aria-pressed', keptSet.has(s.id)); $('keep').querySelector('span').textContent = keptSet.has(s.id) ? 'Kept' : 'Keep';
  // A pocket station is passed on only by its keeper's invitation.
  $('pass').hidden = !!s.pocket; $('maroon').hidden = false;
  document.querySelectorAll('.row').forEach(r => { const on = r.dataset.id === s.id; r.classList.toggle('on', on); r.setAttribute('aria-current', on) });
  // A pocket's id never goes in the address bar or the history.
  if (isPocket(s)) history.replaceState(history.state, '', location.pathname);
  // The station tuned stays out of the address bar, so it never lands in history or a synced
  // browser. The tab alone remembers it, for a reload, and forgets it when it closes.
  else { if (location.hash) history.replaceState(history.state, '', location.pathname + location.search); if (!SHARED && !EMBED) try { sessionStorage.setItem('pyrate.cur', s.id) } catch (e) { } }
  pageHead(); power();
  if (moved && stations.length) rows();
  if (radio) {
    radio.tune(s.id);
    // The stations either side on the dial come aboard ahead, so turning to one plays at once.
    const shown = stations.filter(inSet), i = shown.findIndex(x => x.id === s.id);
    if (i >= 0 && shown.length > 1) for (const d of [1, -1]) { const n = shown[(i + d + shown.length) % shown.length]; if (n.id !== s.id) prepare(n.id) }
    page(true); chatter();
  }
  paint();
  const b = await pic(s.id, s.banner);
  const bb = b || baked(s.id, 'banner_art');
  if (seq === showSeq) $('banner').style.backgroundImage = bb ? 'url(' + bb + ')' : '';
}

// Listen: the set says so at once, with the hiss, and plays the moment it can. Turning to
// another station fades the last one out first. Pressed before the radio has joined the
// network, it joins and then plays.
async function listen(s) {
  // A station broadcast from this tab is never played aloud in it: its own microphone would
  // take it in again, fifteen seconds late, round and round.
  if (onAir(s.id)) { if (playing) stop(); show(s); status(OWN); return }
  audio(); ctx.resume().catch(() => { });
  const g = ++gen;
  fadeOut();
  playing = true; trouble = null; note = null; firstAt = null; fadeNext = true; busySince = 0; liveOn = false; stage = radio ? 'tuning' : 'connecting';
  hiss(true); show(s); vu(); power();
  if (!radio) {
    await (booting ??= boot());
    if (g !== gen) return;
    if (!radio) { stop(); return }
    s = stations.find(x => x.id === s.id) || s; stage = 'tuning'; show(s);
  }
  radio.listening(true);
  play(s.id, g);
}

// Stop always stops: whatever the set was doing, tuning in, searching or playing, ends here.
function stop() {
  gen++; playing = false; trouble = null; stage = null; firstAt = null; liveOn = false;
  hiss(false); fadeOut();
  const g = gen; setTimeout(() => { if (g === gen && !playing) ctx?.suspend() }, 300);
  radio?.listening(false); power(); paint();
}
function power() {
  const busy = playing && !heard();
  for (const b of [$('go'), $('mini-go')]) { b.classList.toggle('on', playing); b.classList.toggle('busy', busy); b.querySelector('.glyph').textContent = playing ? '■︎' : '▶︎' }
  $('go').querySelector('.lbl').textContent = playing ? 'Stop' : 'Listen';
  $('mini-go').setAttribute('aria-label', playing ? 'Stop' : 'Listen');
  $('clip').hidden = !(playing && heard() && soundy && clipType()) || isPocket(cur);
  // The tab says what it is playing, so a listener with many tabs can find the radio.
  if (!EMBED && cur) document.title = (playing ? '▶︎ ' : '') + (cur.name || cur.call_sign || 'A station') + ' · Pyrate Radio';
}

// A row's cover is fetched only once the row comes into view.
const lazy = 'IntersectionObserver' in window ? new IntersectionObserver(es => es.forEach(e => {
  if (!e.isIntersecting) return; lazy.unobserve(e.target);
  const mini = e.target.querySelector('.mini'), s = stations.find(x => x.id === e.target.dataset.id);
  if (s?.cover) pic(s.id, s.cover).then(u => { if (u) { mini.classList.add('pic'); mini.innerHTML = '<img alt="" src="' + u + '">' } else if (e.target.isConnected) setTimeout(() => lazy.observe(e.target), 4000) });
}), { rootMargin: '200px' }) : null;
let rowsKey = '', rowsMax = 60;
const query = () => norm($('find')?.value || '');
function rows() {
  const m = marooned(), q = query();
  let shown = stations.filter(s => !m.has(s.id) && allowed(s) && inSet(s));
  kinds(shown);
  if (kindOn) shown = shown.filter(s => kindOn === 'live' ? s.live : (s.kinds || []).some(k => kindOf(k) === kindOn));
  const by = { newest: (a, b) => (b.since || 0) - (a.since || 0), longest: (a, b) => (a.since || 0) - (b.since || 0), carried: (a, b) => (b.carriers || 0) - (a.carriers || 0) }[sortBy];
  if (by) shown = [...shown].sort(by);
  if (keptSet.size) shown = [...shown.filter(s => keptSet.has(s.id)), ...shown.filter(s => !keptSet.has(s.id))];
  const box = $('rows');
  dial(shown); if (!$('chart').hidden) chart();
  // A search reads names, call signs, what a station says of itself, its kinds, its home port and
  // its place on the dial ("91.1" finds every station there).
  const list = q ? shown.filter(s => norm([s.name, s.call_sign, s.about, (s.kinds || []).join(' '), s.port, fqs(s.id)].join(' ')).includes(q)) : shown;
  // The list is rebuilt only when something in it changed, not on every refresh.
  // Sixty rows at a time: a long Log is paged, and the rest is a search or a tap away.
  const more = list.length - rowsMax;
  const page = list.slice(0, rowsMax);
  const key = page.map(s => [s.id, s.name, s.live, s.aboard, s.cover, s.pocket, s.explicit, s.mature, s.about, s.cached, s.twin, keptSet.has(s.id)].join('|')).join('\n') + '#' + list.length + '#' + (cur?.id || '') + playing + '#' + q + '#' + (shown.length ? '' : Math.floor(Date.now() / 30000));
  if (key === rowsKey) return;
  rowsKey = key; box.innerHTML = '';
  if (!shown.length) {
    const long = bootAt && Date.now() - bootAt > 90000;
    // Rows drawn empty while the first cards come, then a reason if none do.
    box.innerHTML = long ? '<p class="fine">' + (radio?.neighbours() ? 'The network is here, but no station has reached this tab yet. Your settings may be hiding them.' : 'Nothing on this network has answered. It may block the radio. Another network, or the app, may get through.') + '</p>'
      : '<div class="row skel"></div><div class="row skel"></div><div class="row skel"></div><p class="fine">Finding stations on the air</p>';
    return;
  }
  if (!list.length) { box.innerHTML = '<p class="fine">No station on the air matches that.</p>'; return }
  for (const s of page) {
    const r = document.createElement('div'); r.className = 'row'; r.dataset.id = s.id;
    r.setAttribute('role', 'button'); r.tabIndex = 0;
    r.innerHTML = '<div class="mini"></div><div class="meta"><div class="name"></div><div class="about"></div><div class="cs"></div></div><div class="count"></div>';
    const mini = r.querySelector('.mini');
    flag(mini, s.id, s.live);
    if (baked(s.id)) { mini.classList.add('pic'); mini.innerHTML = '<img alt="" src="' + baked(s.id) + '">' }
    else if (s.cover) { if (lazy) lazy.observe(r); else pic(s.id, s.cover).then(u => { if (u) { mini.classList.add('pic'); mini.innerHTML = '<img alt="" src="' + u + '">' } }) }
    const nm = r.querySelector('.name'); nm.textContent = s.name || s.call_sign;
    // What a station says of itself, as hairline tags: the open air in bright red, the rest in bone.
    for (const [t, c] of [[s.live && 'On air', 'lv'], [keptSet.has(s.id) && 'Kept', 'kp'], [s.mixed && !isHarbour(s.id) && 'False colours?', ''], [s.twin && !isHarbour(s.id) && 'Same name as an older station', ''], [s.pocket && 'Private', ''], [s.explicit ? 'Explicit' : s.mature && 'Mature', ''], [empty(s) && 'Nothing to play yet', '']]) if (t) { const g = document.createElement('span'); g.className = 'tag ' + c; g.textContent = t; nm.append(g) }
    r.querySelector('.about').textContent = s.about || ''; r.querySelector('.cs').textContent = s.call_sign + '  ' + fqs(s.id);
    r.querySelector('.count').innerHTML = s.aboard !== undefined && !s.cached ? s.aboard + '<small>listening</small>' : '';
    const on = cur?.id === s.id;
    r.classList.toggle('on', on); r.classList.toggle('playing', on && playing); r.setAttribute('aria-current', on);
    r.setAttribute('aria-label', (s.name || s.call_sign) + (s.live ? ', on air' : '') + (on && playing ? ', playing' : ''));
    // A tap plays it. The station already playing is left playing, and the set comes into view.
    const go = () => { if (on && playing) $('set').scrollIntoView({ behavior: 'smooth' }); else listen(s) };
    r.onclick = go; r.onkeydown = e => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); go() } };
    r.onpointerenter = r.ontouchstart = r.onfocus = () => prepare(s.id);
    box.appendChild(r);
  }
  if (more > 0) {
    const b = document.createElement('p'); b.className = 'centre morerows';
    b.innerHTML = '<button class="btn">Show ' + Math.min(60, more) + ' more</button><span class="fine">' + list.length + ' stations' + (q ? ' match' : '') + '</span>';
    b.querySelector('button').onclick = () => { rowsMax += 60; rows() };
    box.appendChild(b);
  }
}
$('find')?.addEventListener('input', () => { rowsMax = 60; rows() });
// The kinds the stations on the air give themselves, the ten most given, as filters; and On air.
function kinds(list) {
  const box = $('kinds'); if (!box || HOSTIS_ONLY) return;
  const n = new Map();
  for (const s of list) for (const k of new Set((s.kinds || []).map(kindOf))) if (k) n.set(k, (n.get(k) || 0) + 1);
  const top = [...n.entries()].filter(([, c]) => c >= 2 || list.length < 20).sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0])).slice(0, 10).map(([k]) => k);
  if (kindOn && kindOn !== 'live' && !top.includes(kindOn)) top.push(kindOn);
  const live = list.some(s => s.live);
  const key = top.join('|') + live + kindOn;
  if (key === kindsKey) return;
  kindsKey = key; box.innerHTML = '';
  for (const k of [...(live ? ['live'] : []), ...top]) {
    const b = document.createElement('button'); b.className = 'btn' + (kindOn === k ? ' on' : ''); b.setAttribute('aria-pressed', kindOn === k);
    b.textContent = k === 'live' ? 'On air now' : k;
    b.onclick = () => { kindOn = kindOn === k ? null : k; rowsMax = 60; rowsKey = ''; rows() };
    box.appendChild(b);
  }
}
$('sort').value = sortBy;
$('sort').onchange = () => { sortBy = $('sort').value; store('pyrate.sort', sortBy); rowsMax = 60; rows() };

// The stations shown before the network answers: HOSTIS's own on hostis.black, and on the
// open player the ones this browser saw on the air last time, kept for three days.
const CARDS = 'pyrate.cards';
function cachedCards() {
  if (HOSTIS_ONLY) return HARBOUR_STATIONS;
  try { const c = JSON.parse(store(CARDS) || 'null'); if (c && Date.now() - c.at < 3 * 86400e3 && Array.isArray(c.l)) return c.l.filter(s => /^[0-9a-f]{64}$/.test(s.id)).map(s => ({ ...s, cached: true })) } catch (e) { }
  return [];
}
let cardsAt = 0;
function keepCards(live) {
  if (HOSTIS_ONLY || live.length < 3 || Date.now() - cardsAt < 30000) return;
  cardsAt = Date.now();
  store(CARDS, JSON.stringify({ at: cardsAt, l: live.filter(s => !s.pocket).slice(0, 60).map(s => ({ id: s.id, name: s.name, call_sign: s.call_sign, about: s.about, kinds: s.kinds, port: s.port, explicit: s.explicit, mature: s.mature, cover: s.cover })) }));
}
// Kept cards stand in until the network has given as many, or for a minute.
function merged(live) {
  const have = new Set(live.map(s => s.id)), keep = HOSTIS_ONLY || (Date.now() - bootAt < 60000 && live.length < 12);
  return keep ? [...live, ...cachedCards().filter(s => !have.has(s.id))] : live;
}

// The stations a listener is likely to play next come aboard as soon as the radio joins:
// on hostis.black every HOSTIS station, elsewhere the last one played and the three most heard.
// Asked again at every refresh for the first minute and a half, until each log is here.
function warmUp() {
  if (!radio || Date.now() - bootAt > 90000) return;
  const ids = HOSTIS_ONLY ? HARBOUR_STATIONS.map(s => s.id) : [lastId(), ...stations.filter(s => !s.cached && !s.pocket).slice(0, 3).map(s => s.id)];
  for (const id of new Set(ids)) if (id && id !== cur?.id && /^[0-9a-f]{64}$/.test(id)) prepare(id);
}

function refresh() {
  if (!radio) return;
  const live = JSON.parse(radio.stations()).filter(inSet);
  keepCards(live);
  stations = merged(live);
  twins(stations); resolveFreqs(stations);
  // The count is of the stations the list shows: hidden ones and those the settings leave out are not counted.
  const n = radio.neighbours(), m = marooned(), on = stations.filter(s => !m.has(s.id) && allowed(s) && inSet(s)).length;
  $('signal').textContent = (flocking ? 'Playing in step with the host · ' : '') + (n ? 'Connected · ' + on + (on === 1 ? ' station' : ' stations') + ' on the air' : Date.now() - bootAt > 30000 ? 'Still connecting' : 'Connecting');
  $('flockleave').hidden = !flocking;
  rows(); warmUp();
  if (cur) { const u = stations.find(x => x.id === cur.id); if (u && !u.cached) { const pic0 = cur.cover, ban0 = cur.banner, name0 = cur.name; cur = u; if (u.cover !== pic0 || u.banner !== ban0 || u.name !== name0) show(u); else { $('cs').textContent = u.call_sign + (u.aboard !== undefined ? '  ' + u.aboard + ' listening' : ''); pageHead() } } }
  else opening();
  moveOn();
}
// Where the set lands by itself: the station a visit opens on, a Spin, a step along the dial. A
// card can say a station plays when its log, once here, says it has nothing on: then it gives way
// to the next the same way, a few times at most, until the listener chooses for themselves.
let auto = null, hops = 0, hopping = false;
function landed(s, again) { if (!hopping) hops = 0; auto = { id: s.id, again } }
function moveOn() {
  if (!cur || auto?.id !== cur.id || playable(cur)) return;
  const again = auto.again; auto = null;
  if (++hops > 6) return;
  hopping = true; try { again() } finally { hopping = false }
}
function opening() { const s = firstStation(); if (s && s.id !== cur?.id) { show(s); if (!(wanted || passedId())) landed(s, opening) } }
// The station the set opens on: one passed in a link, the last one this tab played, the first
// station this browser keeps, or the most heard. Only a link opens on a station with nothing to
// play. Which stations this browser keeps is all it goes on: no listening is written down.
function firstStation() {
  const linked = wanted || passedId(), id = linked || lastId(), m = marooned(), ok = x => allowed(x) && inSet(x) && !m.has(x.id) && playable(x);
  const named = stations.find(x => x.id === id);
  if (named && (linked || ok(named))) return named;
  // One not on the list yet comes by its link a moment later (followLink).
  if (/^[0-9a-f]{64}$/.test(id || '') && !named) return null;
  // A station with recordings always has something on; one that is only ever live has, only
  // while it is, and its card can still say live after a show that ended badly.
  return stations.find(x => keptSet.has(x.id) && ok(x)) || stations.find(x => ok(x) && x.tracks > 0) || stations.find(ok);
}

// A flock link (#flock=...): this tab plays in step with the phone that showed it.
function flockFollow() {
  const h = location.hash; if (!radio || !h.includes('flock=')) return false;
  try { radio.flock_join(h) } catch (e) { return false }
  history.replaceState(history.state, '', location.pathname);
  flocking = setInterval(() => { if (!radio) return; const id = radio.flock_station(); if (id && cur?.id !== id) { const s = stations.find(x => x.id === id) || { id, call_sign: '', name: 'The host\'s station' }; playing ? listen(s) : show(s) } }, 3000);
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
  const linked = wanted || location.hash.slice(1), id = linked || lastId();
  if (!/^[0-9a-f]{64}$/.test(id) || cur?.id === id || (HOSTIS_ONLY && !isHarbour(id))) return;
  // The tab's last station, known now to have nothing to play, is passed over; a link is not.
  if (!linked && stations.some(x => x.id === id && !playable(x))) return;
  // On an Android phone, a passed station can go straight to the app if it is there.
  if (/Android/.test(navigator.userAgent) && !pockets.has(id) && id !== lastId()) { $('appopen').href = 'intent://station/' + id + '#Intent;scheme=pyrate;package=black.hostis.pyrate;S.browser_fallback_url=' + encodeURIComponent('https://hostis.black/pyrateradio/#set') + ';end'; $('appnote').hidden = false }
  const s = stations.find(x => x.id === id) || { id, call_sign: id.slice(0, 8).toUpperCase(), name: pockets.has(id) ? 'A private station' : 'A passed-on station', pocket: pockets.has(id) };
  if (playing) listen(s); else show(s);
}

// Space or K turns the set on and off, as on any player, unless the listener is typing.
addEventListener('keydown', e => {
  if ((e.key !== ' ' && e.key !== 'k') || e.ctrlKey || e.metaKey || e.altKey || e.repeat) return;
  if (e.target.closest?.('input,textarea,select,button,a,[contenteditable],#glass') || $('go').hidden) return;
  e.preventDefault(); $('go').click();
});
const toggle = () => { if (playing) stop(); else if (cur) listen(cur) };
$('go').onclick = toggle; $('mini-go').onclick = toggle;
// Hovering over Listen brings the station's sound aboard, so the press plays at once.
$('go').addEventListener('pointerenter', () => { prefetched = ''; showAt = Date.now() });
function spin() {
  const m = marooned(), pool = (stations.length ? stations : HARBOUR_STATIONS).filter(s => !m.has(s.id) && allowed(s) && inSet(s) && s.id !== cur?.id && !empty(s));
  while (pool.length) { const s = pool.splice(Math.floor(Math.random() * pool.length), 1)[0]; if (playable(s)) { landed(s, spin); return listen(s) } }
}
$('spin').onclick = spin;
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
    dots += '<g class="dot' + (s.live ? ' live' : '') + (cur?.id === s.id ? ' on' : '') + '" data-id="' + s.id + '" tabindex="0" role="button" aria-label="' + esc(s.name + ', ' + s.port) + '"><circle class="hit" cx="' + x.toFixed(1) + '" cy="' + y.toFixed(1) + '" r="11"/><circle cx="' + x.toFixed(1) + '" cy="' + y.toFixed(1) + '" r="3.4"/><title>' + esc(s.name + ', ' + s.port) + '</title></g>';
  }
  $('chart').innerHTML = '<svg viewBox="0 20 720 290" role="img" aria-label="The chart of the seas"><path class="land" d="' + chartData.land + '"/>' + dots + '</svg><p class="fine">' + (n ? n + (n === 1 ? ' station names' : ' stations name') + ' a home town. The rest name none.' : 'No station on the air has named a home port yet.') + '</p>';
  $('chart').querySelectorAll('.dot').forEach(d => { const go = () => { const s = stations.find(x => x.id === d.dataset.id); if (s) { listen(s); $('set').scrollIntoView({ behavior: 'smooth' }) } }; d.onclick = go; d.onkeydown = e => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); go() } } });
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
  $('chatwho').textContent = 'You speak here as ' + (myName() || c.voice) + '. ' + (c.neighbours ? c.neighbours + ' others here.' : 'Looking for others.');
  const box = $('lines'), atEnd = box.scrollTop + box.clientHeight >= box.scrollHeight - 30;
  const shown = c.lines.filter(l => !mutes.has(l.voice));
  const key = cur.id + shown.map(l => l.id).join() + '#' + [...mutes].join();
  if (key === chatKey) return;
  chatKey = key;
  // Two voices wearing one name are told apart by their call signs, shown beside the name.
  const voices = new Map(); for (const l of shown) { const k = norm(l.name); if (k) { if (!voices.has(k)) voices.set(k, new Set()); voices.get(k).add(l.voice) } }
  const mark = l => l.name && voices.get(norm(l.name))?.size > 1 ? '<span class="vm">' + esc(l.voice) + '</span>' : '';
  box.innerHTML = shown.map(l => '<div class="line' + (l.keeper ? ' keeper' : '') + (l.mine ? ' mine' : '') + '"><button class="who" data-v="' + esc(l.voice) + '" title="' + esc(l.voice) + '">' + esc(l.name || l.voice) + (l.keeper ? ' (the station)' : '') + '</button>' + mark(l) + '<small>' + hm(l.at_ms) + '</small><div class="said">' +
    (l.kind === 'cheer' ? '<span class="cheer">Cheers to what is on</span>' : l.kind === 'request' ? '<span class="req">Asks for ' + esc(l.title || 'a recording') + '</span>' + (l.text ? ', <em>' + esc(l.text) + '</em>' : '') : esc(l.text)) + '</div></div>').join('') || '<p class="fine">Nobody has said anything yet.</p>';
  if (atEnd) box.scrollTop = box.scrollHeight;
  box.querySelectorAll('.who').forEach(w => w.onclick = () => { if (!w.closest('.mine') && confirm('Mute ' + w.textContent + '? Only you stop hearing them, in this browser.')) { mutes.add(w.dataset.v); store('pyrate.mutes', JSON.stringify([...mutes])); chatKey = ''; chatter() } });
}
const speak = (kind, text, track) => radio.chat_send(cur.id, kind, text || '', track || '', myName()).then(chatter, e => { $('chatwho').textContent = String(e) });
$('chatgo').onclick = async () => { $('chatgo').textContent = 'Joining'; try { await radio.chat_join(cur.id) } catch (e) { $('chatgo').textContent = 'Join the chat'; return } chatter() };
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
// station's own keys open it. It passes through the room live; the room keeps none of it, though the
// station may keep what it opens.
// A hail goes to the station it was started for. Switching station mid-hail drops it, and the
// microphone is let go however the hail ends.
let hailRec = null, hailFor = null, hailDrop = false;
function dropHail() { if (hailRec) { hailDrop = true; hailRec.stop() } }
$('hail').onclick = async () => {
  if (hailRec) { hailRec.stop(); return }
  if (!window.MediaRecorder || !navigator.mediaDevices) { $('chatwho').textContent = 'This browser cannot record a message. The app can.'; return }
  let stream; try { stream = await navigator.mediaDevices.getUserMedia({ audio: true }) } catch (e) { $('chatwho').textContent = window !== parent ? 'This page cannot use the microphone. Open the full player to send a message.' : 'Allow the microphone, then press again.'; return }
  const type = ['audio/webm;codecs=opus', 'audio/ogg;codecs=opus', 'audio/mp4'].find(t => MediaRecorder.isTypeSupported(t)) || '';
  const parts = [], to = cur.id; let rec;
  try { rec = new MediaRecorder(stream, type ? { mimeType: type, audioBitsPerSecond: 24000 } : { audioBitsPerSecond: 24000 }) } catch (e) { stream.getTracks().forEach(t => t.stop()); $('chatwho').textContent = 'This browser cannot record a message. The app can.'; return }
  hailRec = rec; hailFor = to; hailDrop = false;
  rec.ondataavailable = e => e.data.size && parts.push(e.data);
  rec.onstop = async () => {
    stream.getTracks().forEach(t => t.stop());
    const dropped = hailDrop; hailRec = null; hailFor = null; hailDrop = false; $('hail').textContent = 'Message the station';
    if (dropped) return;
    const blob = new Blob(parts, { type: (rec.mimeType || type).split(';')[0] }), bytes = new Uint8Array(await blob.arrayBuffer());
    try { await radio.chat_hail(to, bytes, blob.type, myName()); $('chatwho').textContent = 'Sent. Only the station can hear it.' }
    catch (e) { $('chatwho').textContent = String(e) }
  };
  rec.start(); $('hail').textContent = 'Send'; setTimeout(() => rec.state === 'recording' && rec.stop(), 20000);
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
    g.fillStyle = faint; g.font = '28px "JetBrains Mono"'; g.fillText(fqs(st.id) + ' on the dial', W / 2, 700);
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
// Hiding the station on the set stops it, and the set moves along the dial to the next one with
// something to play.
$('maroon').onclick = () => {
  if (!cur || !confirm('Hide ' + (cur.name || 'this station') + '? It stays out of your list and off your dial in this browser until you show it again in Settings.')) return;
  const gone = cur.id, m = marooned(), i = order.findIndex(s => s.id === gone), n = order.length;
  m.add(gone); setMarooned(m); stop(); history.replaceState(history.state, '', location.pathname);
  let next = null;
  for (let k = 1; k <= n && !next; k++) { const s = order[(i + k) % n]; if (s && s.id !== gone && !m.has(s.id) && playable(s)) next = s }
  cur = null; if (next) show(next);
  goView('list'); refresh();
};
$('mhide').onclick = () => $('maroon').onclick();
// The hidden stations, each with its own Show, and the list as text to share or paste.
function hiddenBox() {
  const m = marooned(), names = new Map(stations.map(s => [s.id, s.name || s.call_sign]));
  $('mrows').innerHTML = [...m].map(id => '<div class="mrow"><span>' + esc(names.get(id) || id.slice(0, 8).toUpperCase()) + '</span><button class="btn quiet" data-id="' + id + '">Show</button></div>').join('') || '<p class="fine">No station is hidden.</p>';
  $('mtext').value = '# Pyrate Radio hidden stations, one per line. Paste into Settings to hide them.\n' + [...m].map(id => names.has(id) ? id + '  # ' + names.get(id) : id).join('\n') + '\n';
}
$('mrows').onclick = e => { const b = e.target.closest('button[data-id]'); if (!b) return; const m = marooned(); m.delete(b.dataset.id); setMarooned(m); $('mnote').textContent = ''; hiddenBox(); refresh() };
$('mlist').onclick = () => { hiddenBox(); $('mbox').hidden = !$('mbox').hidden };
$('mtake').onclick = () => {
  const m = marooned(); let n = 0;
  for (const line of $('mtext').value.split('\n')) { const t = (line.trim().split(/\s+/)[0] || '').toLowerCase(); if (/^[0-9a-f]{64}$/.test(t) && !m.has(t)) { m.add(t); n++ } }
  setMarooned(m); hiddenBox(); $('mnote').textContent = n + ' hidden.'; refresh();
};
$('sw-direct').checked = store('pyrate.direct') === '1';
$('sw-direct').onchange = () => { store('pyrate.direct', $('sw-direct').checked ? '1' : '0'); radio?.set_direct_ok($('sw-direct').checked) };
for (const [id, key, on] of [['sw-mature', 'pyrate.hide_mature', '1'], ['sw-explicit', 'pyrate.show_explicit', '1'], ['sw-art', 'pyrate.hide_art', '0'], ['sw-empty', 'pyrate.show_empty', '1']]) {
  const el = $(id); el.checked = on === '1' ? store(key) === '1' : store(key) !== '1';
  el.onchange = () => {
    store(key, (el.checked ? on : (on === '1' ? '0' : '1'))); forgetPics(); rowsKey = '';
    // Pictures off takes the set's cover and banner and the mini player's at once; on brings them back.
    if (id === 'sw-art') { artWant = undefined; art(null); $('banner').style.backgroundImage = '' }
    refresh(); if (cur) show(cur);
  };
}
$('forget').onclick = () => {
  if (!confirm('Forget everything this browser keeps for the radio? Pockets go too, unless you still have their invitations. A station raised here stays on the air, but this browser can speak for it again only from its 24 words.')) return;
  try { for (const k of Object.keys(localStorage)) if (k.startsWith('pyrate.')) localStorage.removeItem(k) } catch (e) { }
  try { for (const k of Object.keys(sessionStorage)) if (k.startsWith('pyrate.')) sessionStorage.removeItem(k) } catch (e) { }
  try { indexedDB.deleteDatabase('pyrate') } catch (e) { }
  location.replace(location.pathname + location.search);
};
$('mclear').onclick = () => { setMarooned(new Set()); hiddenBox(); $('mnote').textContent = 'Every station is back in your list.'; refresh() };

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
  const on = !!mineCur && adopted.has(mineCur), looking = !on && !!mineCur && seeking > 0;
  $('st-mine').hidden = !on; $('st-finding').hidden = !looking; $('st-new').hidden = on || looking || !$('st-show').hidden;
  if (!on) return;
  const k = JSON.parse(radio.mine()).find(m => m.id === mineCur);
  $('st-mname').textContent = k?.name || 'Your station'; $('st-mcs').textContent = (k?.call_sign || '') + '  ' + fqs(mineCur);
  flag($('st-flag'), mineCur, false);
}
function studioStatus() {
  if (!radio || !mineCur) return;
  const k = JSON.parse(radio.mine()).find(m => m.id === mineCur);
  const el = $('st-status'); if (!k) { el.textContent = adopted.has(mineCur) ? '' : 'Looking for the station on the network'; return }
  el.textContent = !k.pow ? 'Getting your station ready · a few seconds'
    : airing ? 'Adding a recording'
    : k.unsent ? 'Sending · ' + Math.round(100 * (k.chunks - k.unsent) / k.chunks) + '%'
    : k.carriers && !k.tracks ? 'On the Log \u00b7 add recordings to start it playing'
    : k.carriers ? 'On the air · ' + k.tracks + (k.tracks === 1 ? ' recording' : ' recordings') + ' · ' + k.carriers + (k.carriers === 1 ? ' radio helping' : ' radios helping')
    : 'Waiting for other radios';
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
// A returning keeper's stations are looked for at once and again within seconds, then every
// fifteen for a while, since a log can take a few tries to find on a quiet network. Meanwhile
// the studio says it is looking, instead of offering an empty form.
let seeking = 0;
const SEEK = [1000, 2000, 4000, 8000, 15000, 15000, 15000, 15000];
function adoptAll() {
  for (const k of kept()) {
    mineCur ??= k.id; seeking++;
    const done = () => { seeking--; studioShow() };
    const retry = n => n < SEEK.length ? setTimeout(() => go(n + 1), SEEK[n]) : done();
    const go = n => radio.adopt(k.id, k.deputy).then(ok => {
      if (!ok) return retry(n);
      adopted.add(k.id);
      if (!(typeof k.pow === 'number' && radio.set_pow(k.id, k.pow))) powWork(k.id);
      done();
    }, () => retry(n));
    go(0);
  }
  studioShow();
}
$('st-open').onclick = async () => {
  const o = sealed(); if (!o) return;
  $('st-locknote2').textContent = 'Unlocking';
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
  $('st-locknote').textContent = 'Locking';
  try { sessionStorage.removeItem('pyrate.mine') } catch (e) { }
  $('st-tabonly').checked = false;
  mineSeal = await sealKey(p, crypto.getRandomValues(new Uint8Array(16))); keep(kept()); $('st-newpass').value = '';
  $('st-locknote').textContent = 'Locked. This browser asks for the passphrase before it broadcasts again.';
};
$('st-tabonly').onchange = () => {
  const l = kept(); mineSeal = null;
  if ($('st-tabonly').checked) { try { sessionStorage.setItem('pyrate.mine', JSON.stringify(l)) } catch (e) { } unstore('pyrate.mine'); $('st-locknote').textContent = 'The key goes when this tab closes. The 24 words bring it back.' }
  else { try { sessionStorage.removeItem('pyrate.mine') } catch (e) { } keepSealed('pyrate.mine', JSON.stringify(l)); $('st-locknote').textContent = 'The key stays in this browser, locked.' }
};

$('st-raise').onclick = async () => {
  if (sealed() && !mineMem) { stNote('Unlock your stations first.'); return }
  const name = $('st-name').value.trim(); if (!name) { stNote('A station needs a name.'); $('st-name').focus(); return }
  if (!radio) { stNote('Connecting first.'); booting ??= boot(); await booting; if (!radio) return }
  let r; try { r = JSON.parse(radio.raise(name, $('st-about').value, $('st-kinds').value, $('st-mature').checked, $('st-explicit').checked, $('st-ppass').value)) } catch (e) { stNote(String(e)); return }
  const l = kept(); l.push({ id: r.id, deputy: r.deputy, pow: null }); keep(l);
  mineCur = r.id; adopted.add(r.id); powWork(r.id); $('st-queue').innerHTML = '';
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
  if (sealed() && !mineMem) { stNote('Unlock your stations first.'); return }
  const words = $('st-words').value.trim().toLowerCase().split(/\s+/).join(' ');
  if (words.split(' ').length !== 24) { stNote('That needs all 24 words.'); return }
  if (!radio) { booting ??= boot(); await booting; if (!radio) return }
  stNote('Looking for your station.');
  try {
    const r = JSON.parse(await radio.restore(words, $('st-rpass').value)); $('st-rpass').value = '';
    $('st-words').value = '';
    const l = kept().filter(k => k.id !== r.id); l.push({ id: r.id, deputy: r.deputy, pow: null }); keep(l);
    mineCur = r.id; adopted.add(r.id); powWork(r.id); $('st-queue').innerHTML = ''; stNote(''); studioShow();
  } catch (e) { stNote(String(e)) }
};
// A new station starts with an empty form and none of the last one's recordings in its queue.
$('st-another').onclick = () => { mineCur = null; $('st-name').value = $('st-about').value = $('st-kinds').value = ''; $('st-kindnote').hidden = true; $('st-queue').innerHTML = ''; studioShow() };
$('st-kinds').oninput = () => { $('st-kindnote').hidden = $('st-kinds').value.split(',').filter(k => k.trim()).length <= 5 };
$('st-tune').onclick = () => { const s = stations.find(x => x.id === mineCur) || { id: mineCur, name: $('st-mname').textContent, call_sign: '' }; listen(s); $('set').scrollIntoView({ behavior: 'smooth' }) };

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
// ---- The live desk: a show from the tab ----------------------------------------------------
// The microphone, a deck of music, six pads and a second input, mixed in WebAudio, held three or
// four decibels under full scale and handed to libopus in a worker as the sound is made. The mixing
// and the limiter run on the audio thread (desk-worklet.js), so a busy page leaves no gap.
// None of it reaches this tab's speakers unless the keeper asks to hear the mix, and the tab
// never plays its own station aloud while it broadcasts it.
// A show is talk (mono, 24 kbps, as the app's talk) while only a voice is on it. Music, a pad
// or a second input makes it music (stereo, 64 kbps): from the start if any is loaded when it
// goes live, or from the moment one is first used, by a fresh encoder at the edge of a chunk.
let live = null;
// Each profile has its limiter's ceiling, in decibels under full scale (desk-worklet.js says why).
const TALK = { channels: 1, bitrate: 24000, talk: true, ceil: -4 }, MUSIC = { channels: 2, bitrate: 64000, talk: false, ceil: -3 };
const OWN = 'This tab is broadcasting it. Use Hear the mix on the desk.';
const dk = { ac: null, up: null, mic: null, two: null, list: [], at: -1, want: false, pads: Array(6).fill(null), padFor: 0, micOpen: true, twoOpen: true, checking: false, starting: false, hearing: false, monOn: false, lock: null, meters: [], el: new Audio() };
dk.el.preload = 'auto';
function onAir(id) { return !!live && live.id === id }
const disguised = () => $('st-disguise').checked;
// A fader is level at 80, ten decibels hotter at the top, and off at the bottom.
const fdb = v => (v - 80) / 2, fgain = v => +v ? 10 ** (fdb(v) / 20) : 0;
const fsay = v => !+v ? 'Off' : (fdb(v) > 0 ? '+' : fdb(v) < 0 ? '−' : '') + Math.abs(Math.round(fdb(v))) + ' dB';
const LIVENOTE = 'Listeners hear you about fifteen seconds behind. Sounds in the background can give away where you are.';
function deskSay(t) { $('st-livenote').textContent = t || (live ? '' : LIVENOTE) }

// The desk's sound is made on the first press that needs it and kept for the page's life.
function deskUp() {
  return dk.up ??= (async () => {
    if (!window.AudioWorkletNode) throw new Error('This browser cannot run the desk. Try another, or the app.');
    const ac = new AudioContext({ sampleRate: 48000, latencyHint: 'interactive' });
    ac.resume().catch(() => { });
    try { await ac.audioWorklet.addModule('desk-worklet.js') } catch (e) { ac.close(); throw new Error('The desk would not start. Try reloading the page.') }
    const gn = (v = 1) => { const n = ac.createGain(); n.gain.value = v; return n };
    const an = () => { const a = ac.createAnalyser(); a.fftSize = 1024; return a };
    // A strip: its fader, a meter after the fader, and a gate that opens and closes it.
    const strip = (id, to, open = 1) => { const f = gn(fgain($(id).value)), m = an(), gate = gn(open); f.connect(m); f.connect(gate); if (to) gate.connect(to); return { f, m, gate } };
    const master = gn(); master.channelCount = 2; master.channelCountMode = 'explicit';
    const air = new AudioWorkletNode(ac, 'pyrate-air', { outputChannelCount: [2], channelCount: 2, channelCountMode: 'explicit' });
    master.connect(air);
    const split = ac.createChannelSplitter(2), mL = an(), mR = an(); air.connect(split); split.connect(mL, 0); split.connect(mR, 1);
    // A sink of its own keeps the air running while none of it reaches the speakers.
    air.connect(ac.createMediaStreamDestination());
    dk.mon = gn(); air.connect(dk.mon);
    dk.duck = gn(); dk.duck.connect(master);
    dk.voice = new AudioWorkletNode(ac, 'pyrate-voice', { outputChannelCount: [1], channelCount: 1, channelCountMode: 'explicit', processorOptions: { semis: disguised() ? -4 : 0 } });
    dk.micS = strip('dk-mic-f', master, dk.micOpen ? 1 : 0); dk.voice.connect(dk.micS.f);
    dk.deckS = strip('dk-deck-f', dk.duck); ac.createMediaElementSource(dk.el).connect(dk.deckS.f);
    dk.padS = strip('dk-pads-f', master);
    dk.twoS = strip('dk-two-f', null, dk.twoOpen ? 1 : 0);
    dk.air = air; dk.master = master; dk.ac = ac;
    dk.meters = [['dk-mic-m', dk.micS.m], ['dk-two-m', dk.twoS.m], ['dk-deck-m', dk.deckS.m], ['dk-pads-m', dk.padS.m], ['dk-ml', mL], ['dk-mr', mR]].map(([id, a]) => ({ cv: $(id), a, db: -99, hold: -99, at: 0, drawn: '' }));
    if (dk.hearing) { dk.mon.connect(ac.destination); dk.monOn = true }
    // A call or another app can take the sound away mid-show; when it gives it back, the desk picks up.
    ac.onstatechange = () => { if ((live || dk.checking) && ac.state !== 'running' && ac.state !== 'closed') ac.resume().catch(() => { }) };
    requestAnimationFrame(meters);
    return ac;
  })().catch(e => { dk.up = null; throw e });
}
async function deskWake() { const ac = await deskUp(); if (ac.state !== 'running') await ac.resume().catch(() => { }); return ac }
// With nothing on and nobody hearing the mix, the desk's sound rests.
function deskRest() { if (dk.ac && !live && !dk.checking && !dk.hearing && !dk.mic && !dk.two && dk.el.paused && !dk.pads.some(p => p?.src)) dk.ac.suspend().catch(() => { }) }

// The keeper's microphone: echo and noise held down by the browser, then the hum out and the
// voice shifted if asked (pyrate-voice), on a path no music takes.
const micErr = e => !navigator.mediaDevices?.getUserMedia ? 'This copy cannot reach a microphone. Open one served over https.' : window !== parent ? 'A page holding the radio cannot lend it the microphone. Open the open player itself.' : e?.name === 'NotFoundError' ? 'This device has no microphone.' : 'The microphone was not allowed. Your browser’s site settings can allow it.';
async function micUp() {
  if (dk.mic) return;
  const n = dk.micSeq = (dk.micSeq || 0) + 1, want = $('dk-micdev').value;
  const ask = id => navigator.mediaDevices.getUserMedia({ audio: { ...(id ? { deviceId: { exact: id } } : {}), channelCount: 1, echoCancellation: true, noiseSuppression: true, autoGainControl: true } });
  let s;
  try { s = await ask(want).catch(e => want && e.name === 'OverconstrainedError' ? ask('') : Promise.reject(e)) } catch (e) { throw new Error(micErr(e)) }
  const ac = await deskWake();
  // Closed again, or asked for again, while it was opening: this one lets go at once.
  if (dk.mic || n !== dk.micSeq) { s.getTracks().forEach(t => t.stop()); return }
  const src = ac.createMediaStreamSource(s); src.connect(dk.voice);
  dk.mic = { s, src };
  for (const t of s.getAudioTracks()) t.onended = () => { if (dk.mic?.s === s) { micDown(); deskSay('The microphone went away.'); deskPaint() } };
  duck(); devices();
}
function micDown() { dk.micSeq = (dk.micSeq || 0) + 1; const m = dk.mic; if (!m) return; dk.mic = null; m.s.getTracks().forEach(t => t.stop()); try { m.src.disconnect() } catch (e) { } duck() }
// The second input: a guest's microphone or an interface, which loses its hum and nothing
// else, or where the browser can share one, a tab's sound, which goes under the mic with the
// deck. The shared tab is kept quiet on this device, so the keeper's microphone never takes it in.
const canTab = () => !!navigator.mediaDevices?.getDisplayMedia;
let devs = [];
async function twoUp(v) {
  twoDown();
  const n = dk.twoSeq;
  if (!v) return;
  const ac = await deskWake();
  let s;
  if (v === 'tab') {
    s = await navigator.mediaDevices.getDisplayMedia({ video: true, audio: { suppressLocalAudioPlayback: true }, selfBrowserSurface: 'exclude', systemAudio: 'include' });
    s.getVideoTracks().forEach(t => t.stop());
    if (!s.getAudioTracks().length) { s.getTracks().forEach(t => t.stop()); throw new Error('That share carried no sound. Choose a tab and allow its audio.') }
  } else s = await navigator.mediaDevices.getUserMedia({ audio: { deviceId: { exact: v }, echoCancellation: false, noiseSuppression: false, autoGainControl: false, channelCount: { ideal: 2 } } });
  // Another choice made while this one was opening wins, and this one lets go.
  if (n !== dk.twoSeq) { s.getTracks().forEach(t => t.stop()); return }
  const src = ac.createMediaStreamSource(new MediaStream(s.getAudioTracks()));
  let hum = null;
  if (v === 'tab') src.connect(dk.twoS.f); else { hum = new AudioWorkletNode(ac, 'pyrate-voice'); src.connect(hum); hum.connect(dk.twoS.f) }
  dk.twoS.gate.connect(v === 'tab' ? dk.duck : dk.master);
  dk.two = { s, src, hum, name: v === 'tab' ? 'A tab’s sound' : devs.find(d => d.deviceId === v)?.label || 'Second input' };
  for (const t of s.getAudioTracks()) t.onended = () => { if (dk.two?.s === s) { twoDown(); $('dk-twosrc').value = ''; deskSay(v === 'tab' ? 'The tab stopped sharing.' : 'The second input went away.'); deskPaint() } };
  musicJoins();
}
function twoDown() { dk.twoSeq = (dk.twoSeq || 0) + 1; const t = dk.two; if (!t) return; dk.two = null; t.s.getTracks().forEach(x => x.stop()); for (const n of [t.src, t.hum, dk.twoS?.gate]) try { n?.disconnect() } catch (e) { } }
function inputsDown() { micDown(); twoDown(); $('dk-twosrc').value = '' }
async function devices() {
  try { devs = (await navigator.mediaDevices.enumerateDevices()).filter(d => d.kind === 'audioinput' && d.deviceId && d.deviceId !== 'communications') } catch (e) { devs = [] }
  const opt = (v, t) => '<option value="' + esc(v) + '">' + esc(t) + '</option>', name = (d, i) => d.label || 'Input ' + (i + 1);
  const m = $('dk-micdev'), was = dk.mic?.s.getAudioTracks()[0]?.getSettings().deviceId || m.value;
  m.innerHTML = devs.map((d, i) => opt(d.deviceId, name(d, i))).join(''); m.value = devs.some(d => d.deviceId === was) ? was : devs[0]?.deviceId || ''; m.hidden = devs.length < 2;
  const t = $('dk-twosrc'), had = t.value;
  t.innerHTML = opt('', 'Off') + devs.map((d, i) => opt(d.deviceId, name(d, i))).join('') + (canTab() ? opt('tab', 'A tab’s sound') : '');
  t.value = [...t.options].some(o => o.value === had) ? had : '';
}

// The mic open takes the music down to the depth chosen; closed, it comes back, more slowly.
function duck() {
  if (!dk.ac) return;
  const d = +$('dk-duck').value, down = dk.micOpen && !!dk.mic && d < 1, g = dk.duck.gain, t = dk.ac.currentTime;
  g.cancelScheduledValues(t); g.setValueAtTime(g.value, t); g.setTargetAtTime(down ? d : 1, t, down ? 0.08 : 0.3);
}
function micSet(open) {
  dk.micOpen = open;
  if (dk.ac) { const g = dk.micS.gate.gain, t = dk.ac.currentTime; g.cancelScheduledValues(t); g.setValueAtTime(g.value, t); g.setTargetAtTime(open ? 1 : 0, t, 0.015) }
  duck(); deskPaint();
}

// The deck plays files from this device through an audio element, so an hour-long set is read
// as it plays and never held whole in memory. Its sound goes only into the desk.
function deckAdd(files) {
  for (const f of [...files]) {
    if (!f.size) continue;
    const t = titled(f), it = { url: URL.createObjectURL(f), title: t.title, by: t.by, dur: 0, bad: false };
    dk.list.push(it);
    const p = new Audio(); p.preload = 'metadata';
    p.onloadedmetadata = () => { it.dur = p.duration; p.removeAttribute('src'); p.load(); deckList() };
    p.onerror = () => { if (!it.dur) { it.bad = true; deckList() } };
    p.src = it.url;
  }
  deckList(); deskPaint();
}
function deckCue(i) { dk.at = i; if (i < 0) { dk.el.removeAttribute('src'); dk.el.load() } else dk.el.src = dk.list[i].url; deckList() }
async function deckPlay() {
  if (dk.at < 0) { const f = dk.list.findIndex(x => !x.bad); if (f < 0) return; deckCue(f) }
  dk.want = true;
  try { await deskWake(); await dk.el.play(); musicJoins() } catch (e) { if (e.name !== 'AbortError') deskSay('That track would not play.') }
  deskPaint();
}
function deckPause() { dk.want = false; dk.el.pause(); deskPaint(); deskRest() }
function deckNext(go) {
  let n = dk.at + 1; while (n < dk.list.length && dk.list[n].bad) n++;
  if (n >= dk.list.length) { dk.el.pause(); deckCue(-1); dk.want = false; deskPaint(); deskRest(); return }
  deckCue(n); if (go) deckPlay(); else { dk.want = false; deskPaint() }
}
function deckPrev() {
  if (dk.at < 0) return;
  if (dk.el.currentTime > 3 || dk.at === 0) { dk.el.currentTime = 0; return }
  let n = dk.at - 1; while (n > 0 && dk.list[n].bad) n--;
  const go = !dk.el.paused; deckCue(n); if (go) deckPlay();
}
function deckDrop(i) {
  const [x] = dk.list.splice(i, 1);
  if (i === dk.at) { dk.el.pause(); dk.at = i - 1; deckNext(false) } else if (i < dk.at) dk.at--;
  URL.revokeObjectURL(x.url); deckList(); deskPaint();
}
function deckList() {
  $('dk-list').innerHTML = dk.list.map((x, i) => '<li' + (i === dk.at ? ' class="on"' : x.bad ? ' class="bad"' : '') + ' data-i="' + i + '"><span class="n">' + (i + 1) + '</span><span class="t">' + esc(x.title) + (x.by ? '<em>' + esc(x.by) + '</em>' : '') + '</span><span class="d">' + (x.bad ? 'Will not play' : x.dur ? mmss(x.dur * 1000) : '') + '</span><button class="x" data-x="' + i + '" aria-label="Remove">×</button></li>').join('');
  $('dk-empty').hidden = dk.list.length > 0;
}
dk.el.onended = () => deckNext(!$('dk-each').checked);
dk.el.onerror = () => { if (dk.at < 0 || !dk.el.getAttribute('src')) return; dk.list[dk.at].bad = true; deskSay('That track would not play.'); deckNext(dk.want) };
dk.el.onplay = dk.el.onpause = () => deskPaint();
$('dk-list').onclick = e => { const x = e.target.closest('.x'), li = e.target.closest('li'); if (x) deckDrop(+x.dataset.x); else if (li && !dk.list[+li.dataset.i]?.bad) { deckCue(+li.dataset.i); deckPlay() } };
$('dk-play').onclick = () => dk.el.paused ? deckPlay() : deckPause();
$('dk-next').onclick = () => deckNext(!dk.el.paused);
$('dk-prev').onclick = deckPrev;
$('dk-prog').onclick = e => { const r = e.currentTarget.getBoundingClientRect(), d = dk.el.duration; if (dk.at >= 0 && isFinite(d) && d > 0) dk.el.currentTime = Math.max(0, Math.min(1, (e.clientX - r.left) / r.width)) * d };
$('dk-files').onchange = e => { deckAdd(e.target.files); e.target.value = '' };
const deckZone = $('dk-deck');
deckZone.addEventListener('dragover', e => { e.preventDefault(); deckZone.classList.add('over') });
deckZone.addEventListener('dragleave', e => { if (!deckZone.contains(e.relatedTarget)) deckZone.classList.remove('over') });
deckZone.addEventListener('drop', e => { e.preventDefault(); deckZone.classList.remove('over'); deckAdd(e.dataTransfer.files) });

// Six pads for jingles, stings and idents, up to thirty seconds each, decoded once and held
// ready. A tap fires one and a tap while it plays stops it; keys 1 to 6 do the same.
function padsPaint() {
  $('dk-pads').innerHTML = dk.pads.map((p, i) => '<div class="dk-pad' + (p ? ' full' : '') + (p?.src ? ' on' : '') + '"><button class="go" data-i="' + i + '"' + (p ? '' : ' aria-label="Load pad ' + (i + 1) + '"') + '><span class="k">' + (i + 1) + '</span><span class="nm">' + (p ? esc(p.name) : 'Load') + '</span><i></i></button>' + (p ? '<button class="x" data-x="' + i + '" aria-label="Clear pad ' + (i + 1) + '">×</button>' : '') + '</div>').join('');
}
async function padLoad(i, f) {
  try {
    const ab = await new OfflineAudioContext(2, 1, 48000).decodeAudioData(await f.arrayBuffer());
    if (ab.duration > 30.5) { deskSay('Pads take up to thirty seconds.'); return }
    padStop(i); dk.pads[i] = { name: titled(f).title, buf: ab, src: null, g: null, t0: 0 }; deskSay('');
  } catch (e) { deskSay('That sound would not load.') }
  padsPaint();
}
async function padFire(i) {
  const p = dk.pads[i];
  if (!p) { dk.padFor = i; $('dk-padfile').click(); return }
  if (p.src) return padStop(i);
  const ac = await deskWake(), s = ac.createBufferSource(), g = ac.createGain();
  s.buffer = p.buf; s.connect(g); g.connect(dk.padS.f); s.start(); p.src = s; p.g = g; p.t0 = ac.currentTime;
  s.onended = () => { g.disconnect(); if (p.src === s) { p.src = null; padsPaint(); deskRest() } };
  musicJoins(); padsPaint();
}
function padStop(i) { const p = dk.pads[i]; if (!p?.src) return; const t = dk.ac.currentTime; p.g.gain.setTargetAtTime(0, t, 0.02); p.src.stop(t + 0.12); p.src = null; padsPaint() }
$('dk-pads').onclick = e => { const x = e.target.closest('.x'), g = e.target.closest('.go'); if (x) { padStop(+x.dataset.x); dk.pads[+x.dataset.x] = null; padsPaint() } else if (g) padFire(+g.dataset.i) };
$('dk-padfile').onchange = e => { const f = e.target.files[0]; if (f) padLoad(dk.padFor, f); e.target.value = '' };
$('dk-pads').addEventListener('dragover', e => { const p = e.target.closest('.dk-pad'); if (p) { e.preventDefault(); p.classList.add('over') } });
$('dk-pads').addEventListener('dragleave', e => e.target.closest('.dk-pad')?.classList.remove('over'));
$('dk-pads').addEventListener('drop', e => { const p = e.target.closest('.dk-pad'); if (!p) return; e.preventDefault(); p.classList.remove('over'); const f = e.dataTransfer.files[0]; if (f) padLoad([...$('dk-pads').children].indexOf(p), f) });

// The first music, pad or second input in a talk show turns it to music at a chunk's edge.
function musicJoins() {
  if (!live || live.music) return;
  live.music = true;
  dk.air.port.postMessage({ ceil: MUSIC.ceil });
  live.ask('switch', MUSIC).catch(e => { console.warn(e); deskSay('The music could not join the broadcast. End it and go live again.') });
}

// Going live. Each chunk is signed into the station's log and sails the moment it is made.
$('st-live').onclick = async () => {
  if (live) return endLive();
  if (!mineCur || !adopted.has(mineCur) || dk.starting) return;
  const id = mineCur;
  dk.starting = true; deskSay(''); deskPaint();
  const fail = t => { dk.starting = false; if (!dk.checking) inputsDown(); deskSay(t); deskPaint(); deskRest() };
  // The desk's sound starts in the same press as the microphone, as a phone's browser wants.
  const up = deskWake(), mic = micUp().then(() => null, e => e);
  try { await up } catch (e) { return fail(e.message) }
  const music = dk.list.length > 0 || dk.pads.some(Boolean) || !!dk.two, noMic = await mic;
  // A show of music alone can go out without a microphone; a show of nothing cannot.
  if (noMic && !music) return fail(noMic.message);
  // Its own station playing here would come back in through the microphone.
  const said = [playing && cur?.id === id ? (stop(), 'Your station stopped playing here while you broadcast.') : '', noMic ? noMic.message + ' On the air without it.' : ''].filter(Boolean).join(' ');
  const w = new Worker('encode-worker.js'), L = { id, w, music, t0: 0, seq: 0, sent: 0, wait: new Map(), done: null, timer: 0 };
  L.ask = (kind, data = {}) => new Promise((res, rej) => { const i = ++L.seq; L.wait.set(i, { res, rej }); w.postMessage({ id: i, kind, ...data }) });
  w.onmessage = e => {
    const d = e.data, p = d.id ? L.wait.get(d.id) : null;
    if (d.out?.length && (d.live || d.last)) try { radio.live_chunk(id, d.out, d.channels, d.bitrate); L.sent++ } catch (err) { console.warn(err); deskSay('Part of the broadcast was lost (' + (err.message || err) + ').') }
    if (d.last) L.done?.();
    if (p) { L.wait.delete(d.id); d.error ? p.rej(new Error(d.error)) : p.res(d) } else if (d.error) console.warn(d.error);
  };
  w.onerror = () => { for (const p of L.wait.values()) p.rej(new Error('the encoder would not start')); L.wait.clear() };
  try { await L.ask('begin', { ...(music ? MUSIC : TALK), live: true, desk: true }) } catch (e) { console.warn(e); w.terminate(); return fail('The encoder would not start. Try reloading the page.') }
  // The worklet hands the air straight to the worker: nothing on the way waits for the page.
  const mc = new MessageChannel();
  w.postMessage({ kind: 'port', port: mc.port2 }, [mc.port2]);
  dk.air.port.postMessage({ port: mc.port1, ceil: (music ? MUSIC : TALK).ceil }, [mc.port1]);
  L.t0 = Date.now(); live = L; dk.starting = false; dk.checking = false;
  L.timer = setInterval(deskTick, 500);
  wake(true); duck(); deskTick(); deskSay(said); deskPaint();
};
function endLive() {
  const L = live; if (!L) return;
  live = null; clearInterval(L.timer);
  // Every input stops and lets go of its device at once: the microphone's light goes out.
  inputsDown(); dk.want = false; dk.el.pause(); dk.pads.forEach((_, i) => padStop(i));
  // What is left of the last chunk goes out, then the end of the broadcast is signed into the
  // log after it, so listeners go back to the schedule at once, and the encoder closes.
  let shut = false;
  L.done = () => { if (shut) return; shut = true; try { radio.live_end(L.id) } catch (e) { console.warn(e) } L.w.terminate(); deskRest() };
  try { dk.air.port.postMessage({ stop: true, ceil: MUSIC.ceil }) } catch (e) { }
  setTimeout(() => { if (!shut) L.ask('end', { desk: true }).catch(() => { }) }, 1500);
  setTimeout(L.done, 5000);
  if (!playing && note === OWN) status(null);
  wake(false); deskTick(); deskSay(''); deskPaint();
}
// A phone left alone would sleep mid-show and take the broadcast down with it.
async function wake(on) {
  if (!on) { const l = dk.lock; dk.lock = null; l?.release().catch(() => { }); return }
  if (dk.lock || !navigator.wakeLock || document.hidden) return;
  try { const l = await navigator.wakeLock.request('screen'); if (!live) { l.release().catch(() => { }); return } dk.lock = l; l.onrelease = () => { if (dk.lock === l) dk.lock = null } } catch (e) { }
}
document.addEventListener('visibilitychange', () => { if (!document.hidden && live) wake(true) });
const clock = s => (s >= 3600 ? Math.floor(s / 3600) + ':' + String(Math.floor(s / 60) % 60).padStart(2, '0') : String(Math.floor(s / 60)).padStart(2, '0')) + ':' + String(s % 60).padStart(2, '0');
function deskTick() {
  $('st-livetime').textContent = live ? 'ON AIR ' + clock(Math.floor((Date.now() - live.t0) / 1000)) : '';
  const st = live && stations.find(x => x.id === live.id);
  $('dk-aboard').textContent = st && typeof st.aboard === 'number' && !st.cached ? st.aboard + ' listening' : '';
}
let nowWas = '';
function deckClock() {
  const x = dk.list[dk.at], d = dk.el.duration, t = dk.el.currentTime, timed = !!x && isFinite(d) && d > 0;
  $('dk-title').textContent = x ? x.title : '';
  $('dk-times').textContent = timed ? mmss(t * 1000) + '  −' + mmss((d - t) * 1000) : '';
  $('dk-prog').firstElementChild.style.width = timed ? Math.min(100, t / d * 100) + '%' : '0'; $('dk-prog').classList.toggle('idle', !timed);
  const now = !x ? '<b>Deck</b>' + (dk.list.length ? 'Nothing playing' : 'Empty') : '<b>' + (dk.el.paused ? 'Deck paused' : 'Deck') + '</b>' + esc(x.title + (x.by ? ' · ' + x.by : '')) + (timed && !dk.el.paused ? '<span>' + mmss((d - t) * 1000) + ' left</span>' : '');
  if (now !== nowWas) { nowWas = now; $('dk-now').innerHTML = now }
}
function padsClock() {
  const cells = $('dk-pads').children;
  dk.pads.forEach((p, i) => { const bar = cells[i]?.querySelector('i'); if (bar) bar.style.width = p?.src && dk.ac ? Math.min(100, (dk.ac.currentTime - p.t0) / p.buf.duration * 100) + '%' : '0' });
}

// The meters: peaks, falling back slowly, with the highest held a moment. They are drawn only
// while the desk is in view.
const mbuf = new Float32Array(1024);
let mLast = 0, mText = 0;
function meters(now) {
  if (!dk.ac || dk.ac.state === 'closed') return;
  requestAnimationFrame(meters);
  if (viewOn !== 'studio' || $('st-mine').hidden) return;
  const dt = Math.min(0.1, Math.max(0, (now - mLast) / 1000)), run = dk.ac.state === 'running'; mLast = now;
  for (const m of dk.meters) {
    let pk = 0;
    if (run) { m.a.getFloatTimeDomainData(mbuf); for (let i = 0; i < mbuf.length; i++) { const v = mbuf[i] < 0 ? -mbuf[i] : mbuf[i]; if (v > pk) pk = v } }
    const db = pk > 1e-4 ? 20 * Math.log10(pk) : -99;
    m.db = Math.max(db, m.db - 24 * dt);
    if (db >= m.hold) { m.hold = db; m.at = now } else if (now - m.at > 1500) m.hold = Math.max(m.db, m.hold - 24 * dt);
    meterDraw(m);
  }
  if (now - mText > 250) { mText = now; deckClock(); padsClock() }
}
function meterDraw(m) {
  const cv = m.cv, r = devicePixelRatio || 1, cw = cv.clientWidth, w = Math.round(cw * r), h = Math.round(cv.clientHeight * r);
  if (!w || !h) return;
  const n = Math.max(12, Math.min(40, Math.floor(cw / 7))), lit = Math.max(0, Math.min(n, Math.round((m.db + 48) / 48 * n))), hold = m.hold > -48 ? Math.min(n - 1, Math.floor((m.hold + 48) / 48 * n)) : -1, key = w + ':' + h + ':' + lit + ':' + hold;
  if (key === m.drawn) return; m.drawn = key;
  if (cv.width !== w || cv.height !== h) { cv.width = w; cv.height = h }
  const g = cv.getContext('2d'), gap = Math.max(1, Math.round(r)), sw = (w - gap * (n - 1)) / n;
  g.clearRect(0, 0, w, h);
  for (let i = 0; i < n; i++) {
    g.fillStyle = i < lit || i === hold ? (i >= n * 0.9 ? '#ed5555' : i >= n * 0.7 ? '#ede4ce' : 'rgba(237,228,206,.55)') : 'rgba(237,228,206,.09)';
    g.fillRect(Math.round(i * (sw + gap)), 0, Math.max(1, Math.round(sw)), h);
  }
}
// Before the desk first sounds, its meters stand dark at their full length.
if ('ResizeObserver' in window) {
  const dark = new Map(), ro = new ResizeObserver(es => { for (const e of es) { const m = dk.meters.find(x => x.cv === e.target) || dark.get(e.target) || dark.set(e.target, { cv: e.target, db: -99, hold: -99 }).get(e.target); m.drawn = ''; meterDraw(m) } });
  document.querySelectorAll('.desk .mtr').forEach(c => ro.observe(c));
}

// Hearing the mix is the one way any of the desk reaches this tab's speakers, and only when
// asked: on open speakers the microphone would take it in again.
function hear(on) {
  dk.hearing = on; $('dk-hear').checked = on;
  if (!on) { if (dk.monOn) { dk.monOn = false; try { dk.mon.disconnect(dk.ac.destination) } catch (e) { } } deskRest(); return }
  deskWake().then(ac => { if (dk.hearing && !dk.monOn) { dk.mon.connect(ac.destination); dk.monOn = true } }, e => { dk.hearing = $('dk-hear').checked = false; deskSay(e.message) });
}
$('dk-hear').onchange = () => hear($('dk-hear').checked);
// Checking levels opens the inputs and the meters, with nothing sent.
$('dk-check').onclick = async () => {
  if (live || dk.starting) return;
  if (dk.checking) { dk.checking = false; inputsDown(); deskPaint(); deskRest(); return }
  dk.checking = true; deskSay(''); deskPaint();
  try { await Promise.all([deskWake(), micUp()]) } catch (e) { dk.checking = false; inputsDown(); deskSay(e.message) }
  deskPaint();
};
$('dk-mic').onclick = () => micSet(!dk.micOpen);
const disguiseSay = () => { $('dk-disguise').setAttribute('aria-pressed', disguised()); dk.voice?.port.postMessage({ semis: disguised() ? -4 : 0 }) };
$('st-disguise').addEventListener('change', disguiseSay);
$('dk-disguise').onclick = () => { $('st-disguise').checked = !disguised(); disguiseSay() };
for (const [k, s] of [['mic', 'micS'], ['deck', 'deckS'], ['pads', 'padS'], ['two', 'twoS']]) {
  const f = $('dk-' + k + '-f');
  f.oninput = () => { $('dk-' + k + '-db').textContent = fsay(f.value); if (dk[s]) dk[s].f.gain.setTargetAtTime(fgain(f.value), dk.ac.currentTime, 0.02) };
}
$('dk-duck').onchange = duck;
$('dk-micdev').onchange = async () => { if (!dk.mic) return; micDown(); try { await micUp() } catch (e) { deskSay(e.message) } deskPaint() };
$('dk-twoopen').onclick = () => { dk.twoOpen = !dk.twoOpen; if (dk.ac) dk.twoS.gate.gain.setTargetAtTime(dk.twoOpen ? 1 : 0, dk.ac.currentTime, 0.015); deskPaint() };
$('dk-twooff').onclick = () => { twoDown(); $('dk-twosrc').value = ''; deskPaint(); deskRest() };
$('dk-twosrc').onchange = async () => {
  const v = $('dk-twosrc').value, up = twoUp(v);
  // A second input opens the desk's ears as Check levels does.
  if (v && !live && !dk.checking) { dk.checking = true; micUp().catch(e => deskSay(e.message)).finally(deskPaint) }
  try { await up } catch (e) { $('dk-twosrc').value = ''; if (!(v === 'tab' && e.name === 'NotAllowedError')) deskSay(e.name === 'Error' ? e.message : 'That input would not open.') }
  deskPaint();
};
// Space opens and closes the microphone and 1 to 6 fire the pads, while the desk is live or
// checking levels and nothing is being typed.
let spaced = false;
addEventListener('keydown', e => {
  if (!(live || dk.checking) || viewOn !== 'studio' || e.ctrlKey || e.metaKey || e.altKey) return;
  if (e.target.closest?.('textarea,select,[contenteditable],input:not([type=checkbox]):not([type=range])')) return;
  if (e.code === 'Space') { e.preventDefault(); spaced = true; if (!e.repeat) micSet(!dk.micOpen) }
  else if (/^(Digit|Numpad)[1-6]$/.test(e.code) && !e.repeat) { e.preventDefault(); padFire(+e.code.slice(-1) - 1) }
});
addEventListener('keyup', e => { if (e.code === 'Space' && spaced) { spaced = false; e.preventDefault() } });
function deskPaint() {
  const b = $('st-live');
  b.classList.toggle('on', !!live); b.classList.toggle('busy', dk.starting); b.querySelector('.lbl').textContent = live ? 'End the broadcast' : 'Go live';
  $('dk-check').hidden = !!live; $('dk-check').textContent = dk.checking ? 'Stop checking' : 'Check levels';
  $('dk-lamp').classList.toggle('on', !!live); $('dk-off').hidden = !!live; $('dk-off').textContent = dk.checking ? 'Off the air · checking levels' : 'Off the air';
  $('desk').classList.toggle('onair', !!live);
  $('dk-mic').setAttribute('aria-pressed', dk.micOpen); $('dk-mic').querySelector('.lbl').textContent = dk.micOpen ? 'Mic open' : 'Mic closed';
  $('dk-micstrip').classList.toggle('hot', !!live && dk.micOpen && !!dk.mic);
  $('dk-disguise').setAttribute('aria-pressed', disguised());
  $('dk-two').hidden = !dk.two; $('desk').classList.toggle('has2', !!dk.two); if (dk.two) $('dk-twoname').textContent = dk.two.name;
  $('dk-twoopen').setAttribute('aria-pressed', dk.twoOpen); $('dk-twoopen').querySelector('.lbl').textContent = dk.twoOpen ? 'Open' : 'Closed';
  $('dk-play').classList.toggle('playing', !dk.el.paused); $('dk-play').setAttribute('aria-label', dk.el.paused ? 'Play' : 'Pause');
  $('st-another').disabled = !!live;
  deckClock();
}
padsPaint(); deckList(); deskSay(''); deskPaint();
if (SCOPE === 'open' && !EMBED && navigator.mediaDevices?.enumerateDevices) { devices(); navigator.mediaDevices.addEventListener?.('devicechange', devices) }
$('st-files').onchange = e => { airFiles(e.target.files); e.target.value = '' };
const drop = $('st-drop');
drop.addEventListener('dragover', e => { e.preventDefault(); drop.classList.add('over') });
drop.addEventListener('dragleave', () => drop.classList.remove('over'));
drop.addEventListener('drop', e => { e.preventDefault(); drop.classList.remove('over'); airFiles(e.dataTransfer.files) });
// Closing the tab while recordings are still sailing would leave them half aboard, and closing
// it on the air would end the show mid-word.
addEventListener('beforeunload', e => {
  if (!radio || SCOPE !== 'open') return;
  const busy = !!live || airing > 0 || JSON.parse(radio.mine()).some(m => m.unsent > 0);
  if (busy) { e.preventDefault(); e.returnValue = '' }
});

// A copy of the set served anywhere but hostis.black/pyrateradio/listen/ (a Lantern's
// /player/, a mirror) points its links to the landing at the landing itself.
if (!/\/pyrateradio\/listen\/$/.test(location.pathname)) document.querySelectorAll('a[href^="../"]').forEach(a => a.href = 'https://hostis.black/pyrateradio/' + a.getAttribute('href').slice(3));

async function boot() {
  if (handOff()) return;
  // An invitation's key leaves the address bar before anything else happens.
  const invite = location.hash.includes('pocket=') ? location.hash : null;
  if (invite) history.replaceState(history.state, '', location.pathname);
  scale();
  $('signal').textContent = 'Connecting';
  try { await init() } catch (e) { $('signal').textContent = 'This browser cannot run the radio. Try another browser, or the app.'; stop(); return }
  // The sealed secrets come back before anything uses them.
  try { pocketsMem = JSON.parse(await dopen(keptAnywhere('pyrate.pockets') || '[]') || '[]') } catch (e) { pocketsMem = [] }
  try { keptSet = new Set(JSON.parse(await dopen(keptAnywhere('pyrate.kept') || '[]') || '[]').filter(id => /^[0-9a-f]{64}$/.test(id))) } catch (e) { keptSet = new Set() }
  if (!sealed() && !tabKeys()) { const m = store('pyrate.mine'); if (m) { const t = await dopen(m); try { const l = validKeys(JSON.parse(t || '[]')); if (l.length) { mineMem = l; if (t === m) keepSealed('pyrate.mine', m) } } catch (e) { } } }
  // The counting key is replaced every thirty days, so a key taken from this browser cannot
  // tie its tokens back further than that.
  let key = null;
  try { const t = JSON.parse(await dopen(keptAnywhere('pyrate.token') || '') || 'null'); if (t && /^[0-9a-f]{64}$/.test(t.k) && Date.now() - t.at < 30 * 86400e3) key = t.k } catch (e) { }
  if (!key) { const b = new Uint8Array(32); crypto.getRandomValues(b); key = [...b].map(x => x.toString(16).padStart(2, '0')).join(''); keepSealed('pyrate.token', JSON.stringify({ k: key, at: Date.now() })) }
  // No relay answering is not the end: the set says so and tries again, more slowly each time.
  for (let tries = 0; !radio; tries++) {
    try { radio = await Radio.start(rememberHarbours(), key, RELAYS) } catch (e) {
      console.warn(e);
      $('signal').textContent = 'Could not reach the network. Trying again. If it never connects, this network may block the radio; try another, or the app.';
      await sleep(Math.min(60000, 5000 * 2 ** tries));
    }
  }
  radio.set_direct_ok(store('pyrate.direct') === '1');
  bootAt = Date.now();
  if (!HOSTIS_ONLY) for (const l of savedPockets()) pocketJoin(l, false);
  if (invite) pocketJoin(invite, true);
  else if (!flockFollow()) followLink();
  refresh();
  setInterval(refresh, 4000); setInterval(() => { paint(); power() }, 1000); setInterval(() => page(false), 5000);
  // The first cards come within seconds: the list is looked at more often until they do.
  for (const ms of [700, 1500, 2500]) setTimeout(refresh, ms);
  studioBoot();
  window.addEventListener('hashchange', () => handOff() || followLink());
  if ('mediaSession' in navigator) {
    const act = (a, f) => { try { navigator.mediaSession.setActionHandler(a, f) } catch (e) { } };
    act('play', () => cur && !playing && listen(cur)); act('pause', stop); act('stop', stop);
    // Next and previous on a lock screen or headset turn the dial, as the arrow keys do.
    act('nexttrack', () => stepDial(1)); act('previoustrack', () => stepDial(-1));
  }
}
// The node in the tab is built to stop dead on a fault rather than carry on in a bad state.
// When it does, the set says so and comes back aboard once by itself, on the same station
// (never a pocket, whose id stays out of the address bar), and offers a reload after that.
function wrecked() {
  if (wrecked.done) return; wrecked.done = true;
  gen++; stopAll(); hiss(false); playing = false; firstAt = null; power(); radio = null;
  let last = 0; try { last = +(sessionStorage.getItem('pyrate.wrecked') || 0) } catch (e) { }
  const again = Date.now() - last > 300000;
  status(again ? 'Something went wrong. Reconnecting.' : 'Something went wrong and the radio stopped. Reload the page.');
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
window.pyrate = { node: () => radio?.id(), routes: (id, chunk) => radio?.route_check(id ?? cur?.id, chunk ?? ''), sizes: () => radio && JSON.parse(radio.log_sizes()), mine: () => kept().map(k => k.id), tap: () => { if (!ctx) return null; if (!tapped) { tapped = ctx.createMediaStreamDestination(); gain.connect(tapped) } return tapped.stream }, plan: at => radio && cur ? JSON.parse(radio.plan(cur.id, at ?? radio.now())) : null, state: () => ({ playing, heard: heard(), stage, trouble, station: cur?.id, scheduled: sources.length, ahead_ms: playing ? aheadMs : 0, ctx: ctx?.state, neighbours: radio?.neighbours(), stations: stations.length, view: viewOn, webcodecs }) };
window.pyrate.desk = () => ({ live: !!live, music: !!live?.music, sent: live?.sent ?? 0, checking: dk.checking, mic: !!dk.mic, open: dk.micOpen, hearing: dk.monOn, two: dk.two?.name ?? null, deck: { tracks: dk.list.length, at: dk.at, playing: !dk.el.paused }, pads: dk.pads.map(p => p?.name ?? null), ctx: dk.ac?.state ?? null, duck: dk.duck ? +dk.duck.gain.value.toFixed(3) : null });
window.pyrate.deskCtx = () => dk.ac;
window.pyrate.card = id => { const s = stations.find(x => x.id === id); return s ? { ...s, playable: playable(s) } : null };// ---- Finding the way: the sections under the set -------------------------------------------
// Stations, the station page, starting a station and settings each have their own view. The
// browser's Back button steps back through them, and nothing about them goes in the address.
const VIEWS = ['list', 'page', 'studio', 'settings'];
let viewOn = 'list';
function goView(v, push = true) {
  if (!VIEWS.includes(v) || (v === 'studio' && SCOPE !== 'open')) v = 'list';
  const again = v === viewOn;
  viewOn = v;
  for (const x of VIEWS) $('v-' + x).hidden = x !== v;
  document.querySelectorAll('.tab').forEach(t => t.setAttribute('aria-current', t.dataset.v === v));
  if (push && !again) history.pushState({ v }, '');
  if (v === 'page') { pageHead(); page(true) }
  // The section comes into view under the tabs, unless it already is.
  if (push && $('tabs').getBoundingClientRect().top > innerHeight * 0.45 || push && again) $('tabs').scrollIntoView({ behavior: 'smooth', block: 'start' });
}
document.querySelectorAll('.tab').forEach(t => t.onclick = () => goView(t.dataset.v));
addEventListener('popstate', e => goView(e.state?.v || 'list', false));
$('pagego').onclick = $('artgo').onclick = () => goView('page');
$('keep').onclick = () => {
  if (!cur) return;
  if (keptSet.has(cur.id)) keptSet.delete(cur.id); else keptSet.add(cur.id);
  saveKept(); show(cur); rowsKey = ''; rows();
};

// ---- The mini player: the station and Listen, kept in reach once the set scrolls away ----
function mini(line) {
  if (!cur) return;
  $('mini-name').textContent = cur.name || cur.call_sign || '';
  $('mini-np').textContent = line || '';
  const m = $('mini-art'), u = !$('cover').hidden && $('cover').src;
  if (u) { if (m.dataset.u !== u) { m.dataset.u = u; m.innerHTML = '<img alt="" src="' + u + '">' } }
  else if (m.dataset.u !== cur.id) { m.dataset.u = cur.id; flag(m, cur.id, cur.live) }
  $('mini').classList.toggle('live', !!cur.live);
}
if (!EMBED && 'IntersectionObserver' in window) new IntersectionObserver(es => { for (const e of es) $('mini').hidden = e.intersectionRatio > 0.15 || !cur }, { threshold: [0, 0.15, 0.3] }).observe($('set'));
$('mini-up').onclick = () => $('set').scrollIntoView({ behavior: 'smooth' });

// The set at rest, before it joins the network: a HOSTIS station, or the one passed on.
function dormant(s) {
  dial(HARBOUR_STATIONS); show(s);
  $('pass').hidden = true;
}
// Inside the landing page, the set tells its host how tall it is.
if (document.documentElement.classList.contains('embed') && parent !== window) {
  // The body's own height, not the document's: a document is never shorter than its frame, so it could grow the frame but never let it shrink back.
  const tell = () => parent.postMessage({ pyrate: 'height', h: Math.ceil(document.body.getBoundingClientRect().height) }, location.origin);
  new ResizeObserver(tell).observe(document.body); addEventListener('load', tell);
}
document.querySelectorAll('a.openlink').forEach(a => { if (!a.id) a.href = OPEN_PLAYERS[0] });
// The studio keeps a station's key on this origin: warn before anyone does so on a copy served
// unchecked, and point to one the browser checks.
if (SCOPE === 'open' && !EMBED && !checked(location.origin + '/')) { const c = OPEN_PLAYERS.find(checked); if (c) { $('st-checked').href = c; $('st-unchecked').hidden = false } }
if (SHARED) { const m = location.pathname.match(/^\/ipfs\/([a-z2-7]{20,})\//); $('sharedgo').href = m ? 'https://' + m[1] + '.ipfs.inbrowser.link/' + location.hash : OPEN_PLAYERS[0] || '#'; $('sharednote').hidden = false }
$('sw-hiss').checked = sw.hiss();
$('sw-hiss').onchange = () => { store('pyrate.hiss', $('sw-hiss').checked ? '1' : '0'); if (playing && !heard()) hiss(true) };
// On hostis.black's own page the station tiles turn this set, from the page that holds it.
if (EMBED) addEventListener('message', e => { if (e.origin !== location.origin || e.data?.pyrate !== 'tune' || !isHarbour(e.data.id)) return; const s = stations.find(x => x.id === e.data.id) || HARBOUR_STATIONS.find(h => h.id === e.data.id); if (s) tuneTo(s) });
if (EMBED) { scale(); const p = passedId(); dormant(p && isHarbour(p) ? HARBOUR_STATIONS.find(h => h.id === p) : HARBOUR_STATIONS[0]); $('signal').textContent = '' }
else {
  // The page shows its stations and a station on the set before the network answers, so
  // Listen works from the first moment; pressed early, it waits for the network and plays.
  // A reload keeps the section it was on.
  scale(); const v0 = history.state?.v; history.replaceState({ v: v0 || 'list' }, ''); if (v0 && v0 !== 'list') goView(v0, false);
  stations = cachedCards().filter(inSet); rows();
  if (!foreign(location.hash)) { const s = firstStation() || (passedId() && !HOSTIS_ONLY ? { id: passedId(), call_sign: passedId().slice(0, 8).toUpperCase(), name: 'A passed-on station' } : null); if (s) { show(s); if (!(wanted || passedId())) landed(s, opening) } }
  booting = boot();
}
// In the page, the radio's engine loads from this site while the page is read, so Listen
// starts at once. Nothing goes to the network itself until Listen is pressed.
if (EMBED) { const warm = () => init().catch(() => { }); if ('requestIdleCallback' in window) requestIdleCallback(warm, { timeout: 2500 }); else setTimeout(warm, 1200) }

// ---- More: clip, sleep timer, recently played, hide ------------------------------------
function playedList() {
  const hm = t => new Date(t).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  $('played').innerHTML = played.length ? [...played].reverse().map(h => '<li><time>' + hm(h.at) + '</time><span>' + esc(h.title) + '<small>' + esc([h.by, h.station].filter(Boolean).join(' · ')) + '</small></span></li>').join('') : '<li class="none">Nothing yet.</li>';
}
function moreOpen(o) { $('morebox').hidden = !o; $('more').setAttribute('aria-expanded', o); if (o) sleepSay(); else { $('played').hidden = true; $('playedgo').setAttribute('aria-expanded', false) } }
$('more').onclick = () => moreOpen($('morebox').hidden);
addEventListener('keydown', e => { if (e.key === 'Escape' && !$('morebox').hidden) { moreOpen(false); $('more').focus() } });
addEventListener('pointerdown', e => { if (!$('morebox').hidden && !e.target.closest('#morebox,#more')) moreOpen(false) });
$('clip').addEventListener('click', () => moreOpen(false));
$('mhide').addEventListener('click', () => moreOpen(false));
$('playedgo').onclick = () => { const o = $('played').hidden; $('played').hidden = !o; $('playedgo').setAttribute('aria-expanded', o); if (o) playedList() };
let sleepMin = 0;
function sleepSay() {
  $('sleepsay').textContent = sleepAt ? 'until ' + new Date(sleepAt).toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' }) : '';
  $('sleepoff').hidden = !sleepAt; $('more').querySelector('span').textContent = sleepAt ? 'Sleep' : 'More';
  for (const b of $('sleepbtns').querySelectorAll('[data-m]')) b.setAttribute('aria-pressed', !!sleepAt && +b.dataset.m === sleepMin);
}
function sleepIn(min) {
  clearTimeout(sleepT); sleepT = null; sleepAt = min ? Date.now() + min * 60000 : null; sleepMin = min || 0;
  if (min) sleepT = setTimeout(() => {
    // Thirty seconds down to silence, then Stop, then the level back for next time.
    const t = ctx?.currentTime ?? 0;
    if (ctx && playing) { bus.gain.cancelScheduledValues(t); bus.gain.setValueAtTime(bus.gain.value, t); bus.gain.linearRampToValueAtTime(0, t + 30) }
    setTimeout(() => { if (playing) stop(); if (ctx) { bus.gain.cancelScheduledValues(ctx.currentTime); bus.gain.setValueAtTime(1, ctx.currentTime + 0.4) } sleepAt = null; sleepMin = 0; sleepSay() }, ctx && playing ? 30000 : 0);
  }, min * 60000);
  sleepSay();
}
for (const b of $('sleepbtns').querySelectorAll('[data-m]')) b.onclick = () => sleepIn(+b.dataset.m);
$('sleepoff').onclick = () => sleepIn(null);
