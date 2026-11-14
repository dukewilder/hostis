// Pyrate Radio's player, kept whole by the browser that installed it: every file, wasm included,
// so the home-screen radio opens with no website at all and joins through the peers and doors it
// remembers. It takes a new copy only when two of the three release keys have signed it, the
// same rule as the app's updates, and every file it keeps matches the signed list byte for byte.
// A copy with fewer signatures (an unsigned build, a hostile mirror's) is never taken: the radio
// goes on playing the copy it has. No rights reserved.
const KEYS = ["c33f56e169e507c2a4303a5f75a79340a058e310ac852013e583f5e657d8752f", "4ba81545a8e79b9856278b57b3ab1cc097ffa169b81cf56615dffee9ec3ce4df", "2cd5c5a95880cf7f2a7d4b5990f620e3ea1bc6e63ff3d60e542827416f5a862d"];
const META = 'pyrate-meta';
// A Lantern or app serving the player adds its doors to the page in one line; the signed list
// is of the page without it.
const DOORS_LINE = /\n<meta name="pyrate-doors" content="[0-9a-f,]*">/;

const hex = b => [...new Uint8Array(b)].map(x => x.toString(16).padStart(2, '0')).join('');
const unhex = h => Uint8Array.from(h.match(/../g) || [], x => parseInt(x, 16));
const sha256 = async b => new Uint8Array(await crypto.subtle.digest('SHA-256', b));

// The bytes each release key signs (pyrate_core::archive::release_message).
function message(code, sha) {
  const head = new TextEncoder().encode('pyrate/release/v1\0'), m = new Uint8Array(head.length + 4 + 32);
  m.set(head); new DataView(m.buffer).setUint32(head.length, code, true); m.set(sha, head.length + 4);
  return m;
}

// The signed list, if two different release keys signed it: { code, files: [[sha, path]] }.
async function signed(base) {
  const [mr, sr] = await Promise.all([fetch(base + 'player.manifest', { cache: 'no-store' }), fetch(base + 'player.sigs', { cache: 'no-store' })]);
  if (!mr.ok || !sr.ok) return null;
  const raw = new Uint8Array(await mr.arrayBuffer()), text = new TextDecoder().decode(raw), lines = text.split('\n').filter(Boolean);
  const m = /^pyrate-player\/v1 code (\d+)$/.exec(lines[0] || ''); if (!m) return null;
  const code = +m[1], sha = await sha256(raw), msg = message(code, sha), good = new Set();
  for (const l of (await sr.text()).split('\n')) {
    const [n, sig] = l.trim().split(/\s+/); const k = KEYS[+n];
    if (!k || !/^[0-9a-f]{128}$/.test(sig || '') || good.has(+n)) continue;
    try {
      const key = await crypto.subtle.importKey('raw', unhex(k), { name: 'Ed25519' }, false, ['verify']);
      if (await crypto.subtle.verify({ name: 'Ed25519' }, key, unhex(sig), msg)) good.add(+n);
    } catch (e) { return null }
  }
  if (good.size < 2) return null;
  const files = lines.slice(1).map(l => l.split('  ')).filter(x => x.length === 2 && /^[0-9a-f]{64}$/.test(x[0]) && !x[1].includes('..'));
  return { code, files };
}

async function current() {
  const r = await (await caches.open(META)).match('current'); return r ? r.json() : null;
}

// Fetches every file of a signed list into a cache of its own, each checked against the list;
// the new copy is taken only whole.
async function take(base, list) {
  const name = 'pyrate-' + list.code, c = await caches.open(name);
  for (const [want, path] of list.files) {
    const r = await fetch(base + path, { cache: 'no-store' }); if (!r.ok) throw path;
    const body = await r.arrayBuffer();
    let check = body;
    if (path === 'index.html') check = new TextEncoder().encode(new TextDecoder().decode(body).replace(DOORS_LINE, ''));
    if (hex(await sha256(check)) !== want) throw path + ' does not match';
    await c.put(new Request(base + path), new Response(body, { headers: { 'content-type': r.headers.get('content-type') || '' } }));
  }
  await (await caches.open(META)).put('current', new Response(JSON.stringify({ code: list.code, cache: name, at: Date.now() })));
  for (const k of await caches.keys()) if (k.startsWith('pyrate-') && k !== name && k !== META) await caches.delete(k);
}

const BASE = new URL('./', self.location).href;
let checking = null;
async function check() {
  if (checking) return checking;
  checking = (async () => {
    try {
      const list = await signed(BASE), cur = await current();
      if (list && (!cur || list.code > cur.code)) await take(BASE, list);
    } catch (e) { }
    checking = null;
  })();
  return checking;
}

self.addEventListener('install', e => { self.skipWaiting(); e.waitUntil(check()) });
self.addEventListener('activate', e => e.waitUntil(self.clients.claim()));
self.addEventListener('message', e => { if (e.data === 'check') e.waitUntil(check().then(async () => e.source?.postMessage({ pyrate: 'copy', ...(await current()) }))) });

self.addEventListener('fetch', e => {
  const u = new URL(e.request.url);
  if (e.request.method !== 'GET' || u.origin !== self.location.origin || !u.href.startsWith(BASE)) return;
  e.respondWith((async () => {
    const cur = await current();
    if (cur) {
      const c = await caches.open(cur.cache), path = u.pathname.endsWith('/') ? u.href.split('#')[0] + 'index.html' : u.href.split('?')[0].split('#')[0];
      const hit = await c.match(path);
      if (hit) {
        // A page opened: a newer signed copy is looked for, at most once an hour.
        if (e.request.mode === 'navigate' && Date.now() - cur.at > 3600e3) check();
        return hit;
      }
    }
    return fetch(e.request);
  })());
});
