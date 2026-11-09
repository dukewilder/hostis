//! Pyrate Radio in a browser tab.
//!
//! A tab is a full listener on the network. It joins the Log by gossip, checks every card
//! and every entry against the station's key, works out what is on from the station's own
//! clock, and fetches six-second chunks by their fingerprints from whoever carries them.
//! A browser cannot open its own ports, so everything goes through relays: the stations a
//! tab hears from never learn its address, and the relay sees only sealed traffic.
//!
//! What a tab holds lives in memory and goes when the tab closes. While it is open, it
//! answers other nodes for the chunks and logs it has.
//!
//! No rights reserved.

use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{Arc, Mutex},
};

use anyhow::{Result, anyhow};
use iroh::{
    Endpoint, EndpointId, SecretKey,
    endpoint::{Connection, presets},
    protocol::{AcceptError, ProtocolHandler, Router},
};
use iroh_gossip::{
    api::{Event, GossipSender},
    net::{GOSSIP_ALPN, Gossip as GossipNet},
    proto::TopicId,
};
use n0_future::{
    StreamExt,
    task::spawn,
    time::{Duration, sleep, timeout},
};
use pyrate_core::{Archive, Body, Hash, SignedEntry, StationId, clock::Clock};
use serde_json::{Value, json};
use wasm_bindgen::prelude::*;

#[path = "../../node/src/net.rs"]
#[allow(dead_code)]
mod net;
use net::{ALPN, Announce, Gossip, MAX_FRAME, Request, Response, Sketch};

/// The same relays the app uses: number0's, and the crew's own.
const CREW_RELAYS: &[&str] = &["https://relay.hostis.black"];
/// Chunks kept in memory: about forty minutes of music.
const HOLD_BYTES: usize = 24 * 1024 * 1024;

fn say(msg: &str) {
    web_sys::console::log_1(&format!("pyrate: {msg}").into());
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn unhex32(s: &str) -> Result<[u8; 32]> {
    if s.len() != 64 {
        return Err(anyhow!("not a 64-character id"));
    }
    let mut out = [0u8; 32];
    for (i, o) in out.iter_mut().enumerate() {
        *o = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16)?;
    }
    Ok(out)
}

fn js(e: anyhow::Error) -> JsValue {
    JsValue::from_str(&e.to_string())
}

#[derive(Default)]
struct Listing {
    announce: Option<Announce>,
    carriers: HashMap<[u8; 32], (u64, u64)>,
    sketches: HashMap<u64, Sketch>,
    heard: HashMap<u64, HashSet<[u8; 32]>>,
}

impl Listing {
    fn fresh(&self, now: u64) -> bool {
        self.announce.as_ref().is_some_and(|a| now.saturating_sub(a.at_ms) < 15 * 60_000)
            || self.carriers.values().any(|(_, seen)| now.saturating_sub(*seen) < 10 * 60_000)
    }

    fn aboard(&self, now: u64) -> u64 {
        let hour = now / 3_600_000;
        let mut s = self.sketches.get(&hour).cloned().unwrap_or_default();
        if now % 3_600_000 < 10 * 60_000 {
            if let Some(p) = self.sketches.get(&(hour.saturating_sub(1))) {
                s.merge(&p.0);
            }
        }
        net::round_aboard(s.estimate())
    }
}

/// Chunks and pictures this tab has fetched, oldest out first.
#[derive(Default)]
struct Hold {
    items: VecDeque<(Hash, Vec<u8>)>,
    bytes: usize,
}

impl Hold {
    fn get(&self, h: &Hash) -> Option<Vec<u8>> {
        self.items.iter().find(|(k, _)| k == h).map(|(_, v)| v.clone())
    }

    fn put(&mut self, h: Hash, b: Vec<u8>) {
        if self.items.iter().any(|(k, _)| *k == h) {
            return;
        }
        self.bytes += b.len();
        self.items.push_back((h, b));
        while self.bytes > HOLD_BYTES {
            let Some((_, old)) = self.items.pop_front() else { break };
            self.bytes -= old.len();
        }
    }
}

struct Ship {
    ep: Endpoint,
    harbour: Vec<EndpointId>,
    tx: Mutex<Option<GossipSender>>,
    log: Mutex<HashMap<StationId, Listing>>,
    archives: Mutex<HashMap<StationId, Archive>>,
    hold: Mutex<Hold>,
    conns: Mutex<HashMap<EndpointId, Connection>>,
    neighbours: Mutex<HashSet<EndpointId>>,
    /// Keyed hashes from this make the hourly listener tokens. It never leaves the tab.
    token_key: [u8; 32],
    tuned: Mutex<Option<StationId>>,
    tokens_sent: Mutex<HashMap<StationId, u64>>,
    syncing: Mutex<HashSet<StationId>>,
    skew: Mutex<Vec<i64>>,
}

fn raw_now() -> u64 {
    js_sys::Date::now() as u64
}

impl Ship {
    /// The tab's clock, corrected by the median gap to the stations' own signed times,
    /// as the app does.
    fn now(&self) -> u64 {
        let s = self.skew.lock().unwrap();
        let off = if s.len() < 7 {
            0
        } else {
            let mut v = s.clone();
            v.sort();
            let m = v[v.len() / 2];
            if m.abs() < 2_000 || m.abs() > 300_000 { 0 } else { m }
        };
        (raw_now() as i64 + off) as u64
    }

    fn serve(&self, req: Request) -> Response {
        match req {
            Request::Log { station, from } => Response::Entries(
                self.archives
                    .lock()
                    .unwrap()
                    .get(&station)
                    .map(|a| a.entries().iter().skip(from as usize).take(2000).filter_map(|e| e.to_bytes().ok()).collect())
                    .unwrap_or_default(),
            ),
            Request::Chunk { hash } => Response::Chunk(self.hold.lock().unwrap().get(&hash)),
            Request::Hop(_) => Response::Hop(vec![]),
        }
    }

    async fn conn(&self, node: EndpointId) -> Result<Connection> {
        if let Some(c) = self.conns.lock().unwrap().get(&node) {
            if c.close_reason().is_none() {
                return Ok(c.clone());
            }
        }
        let c = timeout(Duration::from_secs(15), self.ep.connect(node, ALPN))
            .await
            .map_err(|_| anyhow!("connect timed out"))?
            .map_err(|e| anyhow!("connect: {e}"))?;
        self.conns.lock().unwrap().insert(node, c.clone());
        Ok(c)
    }

    async fn ask(&self, node: EndpointId, req: &Request) -> Result<Response> {
        let c = self.conn(node).await?;
        let fut = async {
            let (mut send, mut recv) = c.open_bi().await?;
            send.write_all(&postcard::to_stdvec(req)?).await?;
            send.finish()?;
            let bytes = recv.read_to_end(MAX_FRAME).await?;
            anyhow::Ok(postcard::from_bytes(&bytes)?)
        };
        match timeout(Duration::from_secs(20), fut).await {
            Ok(Ok(r)) => Ok(r),
            Ok(Err(e)) => {
                self.conns.lock().unwrap().remove(&node);
                Err(e)
            }
            Err(_) => {
                self.conns.lock().unwrap().remove(&node);
                Err(anyhow!("request timed out"))
            }
        }
    }

    /// Who to ask for a station: its carriers, freshest first, then the harbour Lanterns,
    /// which carry everything they hear of.
    fn carriers(&self, station: &StationId) -> Vec<EndpointId> {
        let me = *self.ep.id().as_bytes();
        let mut v: Vec<([u8; 32], u64, u64)> = self
            .log
            .lock()
            .unwrap()
            .get(station)
            .map(|l| l.carriers.iter().filter(|(n, _)| **n != me).map(|(n, (h, s))| (*n, *h, *s)).collect())
            .unwrap_or_default();
        v.sort_by(|a, b| b.1.cmp(&a.1).then(b.2.cmp(&a.2)));
        let mut out: Vec<EndpointId> = v.into_iter().filter_map(|(n, _, _)| EndpointId::from_bytes(&n).ok()).collect();
        for h in &self.harbour {
            if !out.contains(h) {
                out.push(*h);
            }
        }
        out
    }

    async fn sync(&self, station: StationId) -> Result<usize> {
        let mut added = 0;
        for node in self.carriers(&station) {
            loop {
                let from = self.archives.lock().unwrap().get(&station).map(|a| a.len()).unwrap_or(0);
                let raw = match self.ask(node, &Request::Log { station, from }).await {
                    Ok(Response::Entries(raw)) => raw,
                    Ok(_) => break,
                    Err(e) => {
                        say(&format!("log from {}: {e}", node.fmt_short()));
                        break;
                    }
                };
                let got = raw.len();
                let mut archives = self.archives.lock().unwrap();
                let a = archives.entry(station).or_insert_with(|| Archive::new(station));
                for b in raw {
                    let Ok(e) = SignedEntry::from_bytes(&b) else { break };
                    if e.entry.seq < a.len() {
                        continue;
                    }
                    if let Err(err) = a.append(e) {
                        say(&format!("entry refused from {}: {err}", node.fmt_short()));
                        break;
                    }
                    added += 1;
                }
                if a.is_struck() {
                    archives.remove(&station);
                    return Ok(added);
                }
                if got < 2000 {
                    break;
                }
            }
            if self.archives.lock().unwrap().contains_key(&station) {
                break;
            }
        }
        if added > 0 {
            say(&format!("log {}: {added} entries", station.call_sign()));
        }
        Ok(added)
    }

    async fn fetch(&self, station: &StationId, hash: &Hash) -> Result<Vec<u8>> {
        if let Some(b) = self.hold.lock().unwrap().get(hash) {
            return Ok(b);
        }
        for node in self.carriers(station) {
            if let Ok(Response::Chunk(Some(b))) = self.ask(node, &Request::Chunk { hash: *hash }).await {
                if blake3::hash(&b).as_bytes() != hash {
                    continue;
                }
                self.hold.lock().unwrap().put(*hash, b.clone());
                return Ok(b);
            }
        }
        Err(anyhow!("no carrier answered for that chunk"))
    }

    async fn broadcast(&self, g: &Gossip) {
        let tx = self.tx.lock().unwrap().clone();
        if let (Some(tx), Ok(bytes)) = (tx, postcard::to_stdvec(g)) {
            let _ = tx.broadcast(bytes.into()).await;
        }
    }

    fn accept_token(&self, station: StationId, hour: u64, token: [u8; 32], nonce: u64) {
        let now_hour = self.now() / 3_600_000;
        if hour + 1 < now_hour || hour > now_hour + 1 || !net::token_ok(&station, hour, &token, nonce) {
            return;
        }
        let mut log = self.log.lock().unwrap();
        let l = log.entry(station).or_default();
        l.heard.retain(|h, _| h + 2 >= now_hour);
        l.sketches.retain(|h, _| h + 2 >= now_hour);
        if l.heard.entry(hour).or_default().insert(token) {
            l.sketches.entry(hour).or_default().add(&token);
        }
    }

    /// Takes in a gossip message. Returns a station whose log has grown past ours.
    fn on_gossip(&self, g: Gossip) -> Option<StationId> {
        let now = self.now();
        match g {
            Gossip::Station(a) => {
                if a.at_ms > raw_now() + 5 * 60_000 || !a.verify() {
                    return None;
                }
                if let Some(p) = a.proof.as_ref().and_then(|b| SignedEntry::from_bytes(b).ok()) {
                    if let Body::Deputy { key, .. } = &p.entry.body {
                        if let Some(arch) = self.archives.lock().unwrap().get(&a.station) {
                            if arch.len() > p.entry.seq && !arch.deputies().contains(key) {
                                return None;
                            }
                        }
                    }
                }
                let mut log = self.log.lock().unwrap();
                let l = log.entry(a.station).or_default();
                let fresh = l.announce.as_ref().is_none_or(|old| a.at_ms > old.at_ms);
                if !fresh {
                    return None;
                }
                if raw_now().abs_diff(a.at_ms) < 300_000 + 60_000 {
                    let mut s = self.skew.lock().unwrap();
                    s.push(a.at_ms as i64 - raw_now() as i64);
                    if s.len() > 101 {
                        s.remove(0);
                    }
                }
                let (s, head) = (a.station, a.head);
                l.announce = Some(a);
                drop(log);
                let ours = self.archives.lock().unwrap().get(&s).map(|x| x.len()).unwrap_or(0);
                (head > ours).then_some(s)
            }
            Gossip::Carrier { station, node, head } => {
                self.log.lock().unwrap().entry(station).or_default().carriers.insert(node, (head, now));
                let ours = self.archives.lock().unwrap().get(&station).map(|x| x.len()).unwrap_or(0);
                (head > ours).then_some(station)
            }
            Gossip::Aboard { station, hour, tokens } => {
                for (token, nonce) in tokens.into_iter().take(10_000) {
                    self.accept_token(station, hour, token, nonce);
                }
                None
            }
        }
    }

    /// Once an hour, for the station on: one token, worked for, so the station counts this
    /// tab once. The token is a keyed hash nobody can tie to the last hour's.
    async fn count_me(&self) {
        let Some(station) = *self.tuned.lock().unwrap() else { return };
        let hour = self.now() / 3_600_000;
        if self.tokens_sent.lock().unwrap().get(&station) == Some(&hour) {
            return;
        }
        self.tokens_sent.lock().unwrap().insert(station, hour);
        let mut h = blake3::Hasher::new_keyed(&self.token_key);
        h.update(&station.0);
        h.update(&hour.to_le_bytes());
        let token = *h.finalize().as_bytes();
        // The work goes in slices, so the page never freezes for it.
        let mut nonce = 0u64;
        'work: loop {
            for _ in 0..8192 {
                if net::token_ok(&station, hour, &token, nonce) {
                    break 'work;
                }
                nonce += 1;
            }
            sleep(Duration::from_millis(1)).await;
        }
        self.accept_token(station, hour, token, nonce);
        self.broadcast(&Gossip::Aboard { station, hour, tokens: vec![(token, nonce)] }).await;
    }

    fn sync_soon(self: &Arc<Self>, station: StationId) {
        if !self.syncing.lock().unwrap().insert(station) {
            return;
        }
        let me = self.clone();
        spawn(async move {
            let _ = me.sync(station).await;
            me.syncing.lock().unwrap().remove(&station);
        });
    }

    fn describe(a: &Archive, track: &Hash) -> Option<(String, String, Vec<pyrate_core::Chunk>, u8)> {
        a.entries().iter().find_map(|e| match e.entry.body() {
            Body::Track { title, by, chunks, channels, .. } if e.hash().ok().as_ref() == Some(track) => {
                Some((title.clone(), by.clone(), chunks.clone(), *channels))
            }
            _ => None,
        })
    }

    /// What a station plays at `at`: the chunk, where it starts, and what it belongs to.
    fn plan(&self, station: &StationId, at: u64) -> Value {
        let archives = self.archives.lock().unwrap();
        let Some(a) = archives.get(station) else { return json!({ "kind": "searching" }) };
        if let Some(last) = a.entries().last() {
            if matches!(last.entry.body(), Body::Live { .. }) && self.now().saturating_sub(last.entry.at_ms) < 25_000 {
                let tail: Vec<Value> = a
                    .entries()
                    .iter()
                    .filter_map(|e| match e.entry.body() {
                        Body::Live { chunk, channels, .. } => {
                            Some(json!({ "seq": e.entry.seq, "chunk": hex(&chunk.hash), "channels": channels, "ms": chunk.duration_ms() }))
                        }
                        _ => None,
                    })
                    .collect();
                let start = tail.len().saturating_sub(12);
                return json!({ "kind": "live", "tail": tail[start..] });
            }
        }
        let Ok(clock) = Clock::new(a) else { return json!({ "kind": "dead air" }) };
        let Some(np) = clock.now(at) else { return json!({ "kind": "dead air" }) };
        let Some((title, by, chunks, channels)) = Self::describe(a, &np.track) else { return json!({ "kind": "searching" }) };
        let Some(chunk) = chunks.get(np.chunk) else { return json!({ "kind": "dead air" }) };
        let art = a.track_art(&np.track).map(|r| hex(&r.hash));
        json!({
            "kind": "track", "track": hex(&np.track), "title": title, "by": by, "art": art,
            "index": np.chunk, "chunk": hex(&chunk.hash), "chunk_ms": chunk.duration_ms(),
            "chunk_start": at - np.chunk_offset_ms, "channels": channels,
            "position_ms": np.offset_ms, "duration_ms": np.duration_ms,
            "next": chunks.get(np.chunk + 1).map(|c| hex(&c.hash)),
        })
    }

    fn stations(&self) -> Value {
        let now = self.now();
        let banners: HashMap<StationId, String> = self
            .archives
            .lock()
            .unwrap()
            .iter()
            .filter_map(|(id, a)| Some((*id, hex(&a.art()?.1?.hash))))
            .collect();
        let log = self.log.lock().unwrap();
        let mut v: Vec<(u64, Value)> = log
            .iter()
            .filter(|(_, l)| l.fresh(now))
            .filter_map(|(id, l)| {
                let a = l.announce.as_ref()?;
                let aboard = l.aboard(now);
                let accent = a.accent.map(|c| format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2]));
                Some((aboard, json!({
                    "id": id.to_hex(), "call_sign": id.call_sign(), "name": a.name, "about": a.about,
                    "port": a.port, "live": a.live, "aboard": aboard, "since": a.since,
                    "kinds": a.kinds, "mature": a.mature, "tip": a.tip,
                    "cover": a.cover.map(|h| hex(&h)), "banner": banners.get(id), "accent": accent,
                    "carriers": l.carriers.values().filter(|(_, s)| now.saturating_sub(*s) < 10 * 60_000).count(),
                })))
            })
            .collect();
        v.sort_by(|a, b| b.0.cmp(&a.0).then(a.1["name"].as_str().cmp(&b.1["name"].as_str())));
        json!(v.into_iter().map(|x| x.1).collect::<Vec<_>>())
    }
}

#[derive(Debug, Clone)]
struct Proto(Arc<Mutex<Option<Arc<Ship>>>>);

impl ProtocolHandler for Proto {
    async fn accept(&self, conn: Connection) -> std::result::Result<(), AcceptError> {
        let Some(ship) = self.0.lock().unwrap().clone() else { return Ok(()) };
        while let Ok((mut send, mut recv)) = conn.accept_bi().await {
            let ship = ship.clone();
            spawn(async move {
                let Ok(bytes) = recv.read_to_end(MAX_FRAME).await else { return };
                let Ok(req) = postcard::from_bytes::<Request>(&bytes) else { return };
                if let Ok(out) = postcard::to_stdvec(&ship.serve(req)) {
                    let _ = send.write_all(&out).await;
                    let _ = send.finish();
                }
            });
        }
        Ok(())
    }
}

impl std::fmt::Debug for Ship {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Ship")
    }
}

/// The tab's node, as the page sees it.
#[wasm_bindgen]
pub struct Radio {
    ship: Arc<Ship>,
    _router: Router,
}

#[wasm_bindgen]
impl Radio {
    /// Comes aboard. `harbour` is the first peers' node ids, by commas or lines; `token_key`
    /// is 64 hex characters the page keeps, so a reload is not counted twice.
    ///
    /// A harbour entry may name its relay, as `<id>@<relay url>`, so it is found without a
    /// lookup. `relays`, when not empty, replaces the usual relays: for a private network.
    pub async fn start(harbour: String, token_key: String, relays: String) -> Result<Radio, JsValue> {
        let book = iroh::address_lookup::MemoryLookup::new();
        let mut ids = Vec::new();
        for item in harbour.split(|c: char| c == ',' || c.is_whitespace()).filter(|s| !s.is_empty()) {
            let (id, relay) = item.split_once('@').map(|(a, b)| (a, Some(b))).unwrap_or((item, None));
            let Ok(id) = id.parse::<EndpointId>() else { continue };
            if let Some(url) = relay.and_then(|r| r.parse::<iroh::RelayUrl>().ok()) {
                book.add_endpoint_info(iroh::EndpointAddr { id, addrs: [iroh::TransportAddr::Relay(url)].into_iter().collect() });
            }
            ids.push(id);
        }
        let harbour = ids;
        let token_key = unhex32(&token_key).map_err(js)?;
        let custom: Vec<iroh::RelayUrl> = relays.split(',').filter_map(|r| r.trim().parse().ok()).collect();
        let relays = if custom.is_empty() {
            let m = iroh::defaults::prod::default_relay_map();
            for r in CREW_RELAYS {
                if let Ok(url) = r.parse::<iroh::RelayUrl>() {
                    m.insert(url.clone(), Arc::new(iroh::RelayConfig::from(url)));
                }
            }
            m
        } else {
            iroh::RelayMap::from_iter(custom)
        };
        // A fresh node key every visit: nothing ties one visit to the next.
        let mut seed = [0u8; 32];
        getrandom::fill(&mut seed).map_err(|e| JsValue::from_str(&e.to_string()))?;
        let ep = Endpoint::builder(presets::N0)
            .secret_key(SecretKey::from_bytes(&seed))
            .relay_mode(iroh::RelayMode::Custom(relays))
            .address_lookup(book)
            .bind()
            .await
            .map_err(|e| JsValue::from_str(&format!("bind: {e}")))?;
        let gossip = GossipNet::builder().spawn(ep.clone());
        let proto = Proto(Arc::new(Mutex::new(None)));
        let router = Router::builder(ep.clone()).accept(ALPN, proto.clone()).accept(GOSSIP_ALPN, gossip.clone()).spawn();
        let ship = Arc::new(Ship {
            ep,
            harbour: harbour.clone(),
            tx: Mutex::new(None),
            log: Mutex::new(HashMap::new()),
            archives: Mutex::new(HashMap::new()),
            hold: Mutex::new(Hold::default()),
            conns: Mutex::new(HashMap::new()),
            neighbours: Mutex::new(HashSet::new()),
            token_key,
            tuned: Mutex::new(None),
            tokens_sent: Mutex::new(HashMap::new()),
            syncing: Mutex::new(HashSet::new()),
            skew: Mutex::new(Vec::new()),
        });
        *proto.0.lock().unwrap() = Some(ship.clone());

        let s2 = ship.clone();
        spawn(async move {
            say(&format!("node {} up, joining the Log through {} harbour peers", s2.ep.id().fmt_short(), s2.harbour.len()));
            let sub = match gossip.subscribe(TopicId::from_bytes(net::log_topic()), s2.harbour.clone()).await {
                Ok(s) => s,
                Err(e) => return say(&format!("gossip: {e}")),
            };
            let (tx, mut rx) = sub.split();
            *s2.tx.lock().unwrap() = Some(tx.clone());
            let s3 = s2.clone();
            spawn(async move {
                loop {
                    sleep(Duration::from_secs(45)).await;
                    let mut peers = s3.harbour.clone();
                    let carriers: Vec<[u8; 32]> = s3.log.lock().unwrap().values().flat_map(|l| l.carriers.keys().copied()).collect();
                    peers.extend(carriers.iter().filter_map(|n| EndpointId::from_bytes(n).ok()));
                    peers.retain(|p| *p != s3.ep.id());
                    peers.sort();
                    peers.dedup();
                    peers.truncate(16);
                    let _ = tx.join_peers(peers).await;
                    s3.count_me().await;
                }
            });
            while let Some(ev) = rx.next().await {
                let msg = match ev {
                    Ok(Event::Received(m)) => m,
                    Ok(Event::NeighborUp(n)) => {
                        s2.neighbours.lock().unwrap().insert(n);
                        continue;
                    }
                    Ok(Event::NeighborDown(n)) => {
                        s2.neighbours.lock().unwrap().remove(&n);
                        continue;
                    }
                    _ => continue,
                };
                let Ok(g) = postcard::from_bytes::<Gossip>(&msg.content) else { continue };
                if let Some(st) = s2.on_gossip(g) {
                    if *s2.tuned.lock().unwrap() == Some(st) {
                        s2.sync_soon(st);
                    }
                }
            }
        });
        Ok(Radio { ship, _router: router })
    }

    pub fn id(&self) -> String {
        self.ship.ep.id().to_string()
    }

    /// Milliseconds on the network's clock.
    pub fn now(&self) -> f64 {
        self.ship.now() as f64
    }

    pub fn neighbours(&self) -> u32 {
        self.ship.neighbours.lock().unwrap().len() as u32
    }

    /// Every station heard of lately, most aboard first, as JSON.
    pub fn stations(&self) -> String {
        self.ship.stations().to_string()
    }

    /// Puts a station on: its log is fetched and kept current, and this tab is counted aboard.
    pub fn tune(&self, id: &str) -> Result<(), JsValue> {
        let st = StationId(unhex32(id).map_err(js)?);
        *self.ship.tuned.lock().unwrap() = Some(st);
        self.ship.sync_soon(st);
        let s = self.ship.clone();
        spawn(async move { s.count_me().await });
        Ok(())
    }

    pub fn untune(&self) {
        *self.ship.tuned.lock().unwrap() = None;
    }

    /// Brings a station's log up to date. Resolves to how many entries came in.
    pub fn sync(&self, id: &str) -> Result<js_sys::Promise, JsValue> {
        let st = StationId(unhex32(id).map_err(js)?);
        let s = self.ship.clone();
        Ok(wasm_bindgen_futures::future_to_promise(async move {
            s.sync(st).await.map(|n| JsValue::from(n as u32)).map_err(js)
        }))
    }

    /// Whether this tab holds a station's log yet.
    pub fn has_log(&self, id: &str) -> bool {
        unhex32(id).is_ok_and(|b| self.ship.archives.lock().unwrap().contains_key(&StationId(b)))
    }

    /// What a station plays at `at` (network milliseconds), as JSON.
    pub fn plan(&self, id: &str, at: f64) -> Result<String, JsValue> {
        let st = StationId(unhex32(id).map_err(js)?);
        Ok(self.ship.plan(&st, at as u64).to_string())
    }

    /// A chunk or a picture by its fingerprint, checked against it. Resolves to its bytes.
    pub fn fetch(&self, id: &str, hash: &str) -> Result<js_sys::Promise, JsValue> {
        let st = StationId(unhex32(id).map_err(js)?);
        let h = unhex32(hash).map_err(js)?;
        let s = self.ship.clone();
        Ok(wasm_bindgen_futures::future_to_promise(async move {
            let b = s.fetch(&st, &h).await.map_err(js)?;
            Ok(js_sys::Uint8Array::from(&b[..]).into())
        }))
    }

    /// The station's whole log as JSON lines of what aired: for the station page.
    pub fn library(&self, id: &str) -> Result<String, JsValue> {
        let st = StationId(unhex32(id).map_err(js)?);
        let archives = self.ship.archives.lock().unwrap();
        let Some(a) = archives.get(&st) else { return Ok("[]".into()) };
        let v: Vec<Value> = a
            .library()
            .map_err(|e| js(anyhow!("{e}")))?
            .into_iter()
            .filter_map(|(h, e)| match e.entry.body() {
                Body::Track { title, by, .. } => Some(json!({ "track": hex(&h), "title": title, "by": by, "at": e.entry.at_ms })),
                _ => None,
            })
            .collect();
        Ok(json!(v).to_string())
    }
}
