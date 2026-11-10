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
    pyrate_core::unhex32(s).ok_or_else(|| anyhow!("not a 64-character id"))
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

/// A private route's hop, as the nodes read it: forward a sealed request, or open one. The
/// variants and their order are the node's (`node/src/hop.rs`); a tab only ever sends these two.
#[derive(serde::Serialize)]
enum Hop {
    Forward { next: [u8; 32], blob: Vec<u8> },
    Sealed { eph: [u8; 32], ct: Vec<u8> },
}

/// What a tab asks an exit for. The order is the node's; a tab only sends `Token`.
#[allow(dead_code)]
#[derive(serde::Serialize)]
enum Ask {
    Log { station: StationId, from: u64 },
    Chunk { station: StationId, hash: Hash },
    Token { station: StationId, hour: u64, token: [u8; 32], nonce: u64 },
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

/// A station's chatter as this tab holds it: the room's sender and the last lines.
struct Room {
    tx: GossipSender,
    /// When the tab joined, so the oldest room goes first when there are too many.
    since: u64,
    lines: VecDeque<pyrate_core::chat::Signed>,
    seen: pyrate_core::chat::Seen,
    neighbours: usize,
}

struct Ship {
    ep: Endpoint,
    gossip: GossipNet,
    /// Addresses learnt from links: harbours, and a flock's host.
    book: iroh::address_lookup::MemoryLookup,
    /// Flock: the host this tab follows, the clock it keeps, the station it plays.
    flock: Mutex<Option<(EndpointId, [u8; 8])>>,
    flock_off: std::sync::atomic::AtomicI64,
    flock_station: Mutex<Option<StationId>>,
    /// Pocket stations this tab was invited to: their keys and the hosts to start from.
    pockets: Mutex<HashMap<StationId, ([u8; 32], Vec<EndpointId>)>>,
    /// Each pocket's own topic, for telling its members only that this tab is aboard.
    pocket_tx: Mutex<HashMap<StationId, GossipSender>>,
    rooms: Mutex<HashMap<StationId, Room>>,
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
    drift: Mutex<pyrate_core::drift::Drift>,
    /// Nodes that failed to answer lately, and when: asked only after the others.
    failed: Mutex<HashMap<[u8; 32], u64>>,
    /// Whether the visitor is listening: only then is the tab counted aboard.
    listening: std::sync::atomic::AtomicBool,
    /// The first hop of this visit's private route.
    first_hop: Mutex<Option<EndpointId>>,
    /// Each station's clock, rebuilt only when its log grows: the player asks every second.
    clocks: Mutex<HashMap<StationId, (u64, Clock)>>,
    /// When each log was last played or synced, so the least used go first when trimming.
    touched: Mutex<HashMap<StationId, u64>>,
    /// Stations that struck their colours: never searched for, listed or played again.
    struck: Mutex<HashSet<StationId>>,
    last_trim: std::sync::atomic::AtomicU64,
}

/// A number below `n` from the browser's own randomness.
fn pick(n: usize) -> usize {
    let mut b = [0u8; 8];
    let _ = getrandom::fill(&mut b);
    (u64::from_le_bytes(b) % n.max(1) as u64) as usize
}

fn raw_now() -> u64 {
    js_sys::Date::now() as u64
}

impl Ship {
    /// The tab's clock, corrected by the median gap to the stations' own signed times,
    /// as the app does.
    /// The hour as this tab plays it: the network's clock, set to a flock's host if it follows one.
    fn now(&self) -> u64 {
        (self.base_now() as i64 + self.flock_off.load(std::sync::atomic::Ordering::Relaxed)) as u64
    }

    fn base_now(&self) -> u64 {
        (raw_now() as i64 + self.drift.lock().unwrap().offset()) as u64
    }

    fn serve(&self, req: Request) -> Response {
        match req {
            // A tab answers nobody about what it holds: whoever could ask it for a station's log
            // or the chunks just played would learn what this visitor is hearing. It takes from
            // the network and gives back its count, its chatter and nothing else.
            Request::Log { .. } => Response::Entries(vec![]),
            Request::Chunk { .. } => Response::Chunk(None),
            Request::SealedChunk { .. } => Response::Sealed(None),
            Request::Flock { .. } => Response::Flock { now_ms: 0, station: None },
            Request::PocketLog { .. } => Response::Entries(vec![]),
            Request::Hop(_) => Response::Hop(vec![]),
            // A pocket's lines go only to whoever asks by its tag, and sealed.
            Request::Chatter { station } if self.pockets.lock().unwrap().contains_key(&station) => Response::Chatter(vec![]),
            Request::Chatter { station } => Response::Chatter(
                self.rooms.lock().unwrap().get(&station).map(|r| r.lines.iter().rev().take(60).rev().filter_map(|l| postcard::to_allocvec(l).ok()).collect()).unwrap_or_default(),
            ),
            Request::PocketChatter { tag } => {
                let found = self.pockets.lock().unwrap().iter().find(|(_, (k, _))| pyrate_core::pocket::tag(k) == tag).map(|(st, (k, _))| (*st, *k));
                let Some((st, key)) = found else { return Response::Chatter(vec![]) };
                Response::Chatter(
                    self.rooms.lock().unwrap().get(&st).map(|r| r.lines.iter().rev().take(60).rev().filter_map(|l| postcard::to_allocvec(l).ok()).map(|b| pyrate_core::pocket::seal(&key, &b)).collect()).unwrap_or_default(),
                )
            }
        }
    }

    /// The tab's voice under a station's colours: from its own kept key, so the same voice
    /// comes back on the next visit from this browser.
    fn voice(&self, st: &StationId) -> pyrate_core::StationKey {
        let secret = *blake3::keyed_hash(&self.token_key, b"pyrate/chat/secret").as_bytes();
        pyrate_core::chat::voice(&secret, st)
    }

    fn hear(&self, st: StationId, l: pyrate_core::chat::Signed) -> bool {
        if !pyrate_core::chat::check(&l, &st, self.now()) || matches!(l.line.say, pyrate_core::chat::Say::Hail { .. }) {
            return false;
        }
        let mut rooms = self.rooms.lock().unwrap();
        let Some(r) = rooms.get_mut(&st) else { return false };
        if !r.seen.insert(pyrate_core::chat::id(&l), l.line.at_ms, self.now()) {
            return false;
        }
        r.lines.push_back(l);
        r.lines.make_contiguous().sort_by_key(|l| l.line.at_ms);
        while r.lines.len() > 200 {
            r.lines.pop_front();
        }
        true
    }

    async fn chat_join(self: &Arc<Self>, st: StationId) -> Result<()> {
        if self.rooms.lock().unwrap().contains_key(&st) {
            return Ok(());
        }
        // A pocket's room is its members' alone: a topic drawn from its key, met through its
        // hosts, every line sealed with the key.
        let pocket = self.pockets.lock().unwrap().get(&st).cloned();
        let key = pocket.as_ref().map(|p| p.0);
        let mut peers = match &pocket { Some((_, hosts)) => hosts.clone(), None => self.harbour.clone() };
        if let Some(l) = self.log.lock().unwrap().get(&st) {
            peers.extend(l.carriers.keys().filter_map(|n| EndpointId::from_bytes(n).ok()));
        }
        peers.retain(|p| *p != self.ep.id());
        peers.sort();
        peers.dedup();
        peers.truncate(12);
        let topic = key.map(|k| pyrate_core::pocket::chat_topic(&k)).unwrap_or_else(|| pyrate_core::chat::topic(&st));
        let sub = self.gossip.subscribe(TopicId::from_bytes(topic), peers.clone()).await.map_err(|e| anyhow!("{e}"))?;
        let (tx, mut rx) = sub.split();
        {
            // A tab keeps at most three rooms open; joining a fourth leaves the oldest.
            let mut rooms = self.rooms.lock().unwrap();
            while rooms.len() >= 3 {
                let Some(old) = rooms.iter().filter(|(k, _)| **k != st).min_by_key(|(_, r)| r.since).map(|(k, _)| *k) else { break };
                rooms.remove(&old);
            }
            rooms.insert(st, Room { tx, since: raw_now(), lines: VecDeque::new(), seen: Default::default(), neighbours: 0 });
        }
        let me = self.clone();
        spawn(async move {
            while let Some(ev) = rx.next().await {
                match ev {
                    Ok(Event::Received(m)) => {
                        let bytes = match &key {
                            Some(k) => pyrate_core::pocket::open(k, &m.content),
                            None => Some(m.content.to_vec()),
                        };
                        if let Some(l) = bytes.and_then(|b| postcard::from_bytes(&b).ok()) {
                            me.hear(st, l);
                        }
                    }
                    Ok(Event::NeighborUp(_)) => {
                        if let Some(r) = me.rooms.lock().unwrap().get_mut(&st) {
                            r.neighbours += 1;
                        }
                    }
                    Ok(Event::NeighborDown(_)) => {
                        if let Some(r) = me.rooms.lock().unwrap().get_mut(&st) {
                            r.neighbours = r.neighbours.saturating_sub(1);
                        }
                    }
                    _ => {}
                }
                if !me.rooms.lock().unwrap().contains_key(&st) {
                    break;
                }
            }
        });
        let me = self.clone();
        spawn(async move {
            let ask = match &key {
                Some(k) => Request::PocketChatter { tag: pyrate_core::pocket::tag(k) },
                None => Request::Chatter { station: st },
            };
            for p in peers.into_iter().take(3) {
                if let Ok(Response::Chatter(lines)) = me.ask(p, &ask).await {
                    let got = lines
                        .iter()
                        .filter_map(|b| match &key { Some(k) => pyrate_core::pocket::open(k, b), None => Some(b.clone()) })
                        .filter_map(|b| postcard::from_bytes::<pyrate_core::chat::Signed>(&b).ok())
                        .filter(|l| me.hear(st, l.clone()))
                        .count();
                    if got > 0 {
                        break;
                    }
                }
            }
        });
        Ok(())
    }

    /// A line as it goes out: sealed with the key in a pocket's room, as it is elsewhere.
    fn room_bytes(&self, st: &StationId, bytes: Vec<u8>) -> Vec<u8> {
        match self.pockets.lock().unwrap().get(st) {
            Some((key, _)) => pyrate_core::pocket::seal(key, &bytes),
            None => bytes,
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
        let c = match self.conn(node).await {
            Ok(c) => c,
            Err(e) => {
                let now = self.now();
                let mut f = self.failed.lock().unwrap();
                f.retain(|_, t| now.saturating_sub(*t) < net::FAILED_MS);
                f.insert(*node.as_bytes(), now);
                return Err(e);
            }
        };
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
                self.failed.lock().unwrap().insert(*node.as_bytes(), self.now());
                Err(anyhow!("request timed out"))
            }
        }
    }

    /// Who to ask for a station: its carriers, freshest first, then the harbour Lanterns,
    /// which carry everything they hear of. A pocket is asked of its own circle only.
    fn carriers(&self, station: &StationId) -> Vec<EndpointId> {
        let pocket = self.pockets.lock().unwrap().contains_key(station);
        let me = *self.ep.id().as_bytes();
        let now = self.now();
        let failed = self.failed.lock().unwrap().clone();
        let ranked = self
            .log
            .lock()
            .unwrap()
            .get(station)
            .map(|l| net::rank_carriers(&l.carriers, &me, l.announce.as_ref().map(|a| a.head), |n| failed.get(n).is_some_and(|t| now.saturating_sub(*t) < net::FAILED_MS)))
            .unwrap_or_default();
        let mut out: Vec<EndpointId> = ranked.into_iter().filter_map(|n| EndpointId::from_bytes(&n).ok()).collect();
        if pocket {
            return out;
        }
        for h in &self.harbour {
            if !out.contains(h) {
                out.push(*h);
            }
        }
        out
    }

    async fn sync(&self, station: StationId) -> Result<usize> {
        let pocket = self.pockets.lock().unwrap().get(&station).cloned();
        if let Some((key, hosts)) = pocket {
            return self.pocket_sync(station, key, hosts).await;
        }
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
                    drop(archives);
                    self.struck.lock().unwrap().insert(station);
                    self.log.lock().unwrap().remove(&station);
                    self.clocks.lock().unwrap().remove(&station);
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

    /// A pocket station's log from its hosts and members, sealed, asked for by its tag.
    async fn pocket_sync(&self, station: StationId, key: [u8; 32], hosts: Vec<EndpointId>) -> Result<usize> {
        let tag = pyrate_core::pocket::tag(&key);
        let mut asked = hosts;
        asked.extend(self.carriers(&station));
        asked.dedup();
        let mut added = 0;
        for node in asked.into_iter().take(6) {
            let from = self.archives.lock().unwrap().get(&station).map(|a| a.len()).unwrap_or(0);
            let Ok(Response::Entries(raw)) = self.ask(node, &Request::PocketLog { tag, from }).await else { continue };
            let mut archives = self.archives.lock().unwrap();
            let a = archives.entry(station).or_insert_with(|| Archive::new(station));
            for b in raw.iter().filter_map(|b| pyrate_core::pocket::open(&key, b)) {
                let Ok(e) = SignedEntry::from_bytes(&b) else { break };
                if e.entry.station != station || e.entry.seq < a.len() {
                    continue;
                }
                if a.append(e).is_err() {
                    break;
                }
                added += 1;
            }
        }
        Ok(added)
    }

    async fn fetch(&self, station: &StationId, hash: &Hash) -> Result<Vec<u8>> {
        if let Some(b) = self.hold.lock().unwrap().get(hash) {
            return Ok(b);
        }
        // Carriers that failed lately go to the back; the rest are asked two at a time, so
        // one dead carrier costs a chunk no more than the other's answer takes.
        let now = self.now();
        let failed = self.failed.lock().unwrap().clone();
        let (fresh, stale): (Vec<EndpointId>, Vec<EndpointId>) = self
            .carriers(station)
            .into_iter()
            .partition(|n| !failed.get(n.as_bytes()).is_some_and(|t| now.saturating_sub(*t) < net::FAILED_MS));
        let order: Vec<EndpointId> = fresh.into_iter().chain(stale).take(8).collect();
        for pair in order.chunks(2) {
            let mut tries: n0_future::FuturesUnordered<_> = pair.iter().map(|n| Box::pin(self.fetch_from(*n, hash))).collect();
            while let Some(r) = n0_future::StreamExt::next(&mut tries).await {
                if let Ok(b) = r {
                    self.hold.lock().unwrap().put(*hash, b.clone());
                    return Ok(b);
                }
            }
        }
        Err(anyhow!("no carrier answered for that chunk"))
    }

    async fn fetch_from(&self, node: EndpointId, hash: &Hash) -> Result<Vec<u8>> {
        let name = pyrate_core::seal::name(hash);
        if let Ok(Response::Sealed(Some(s))) = self.ask(node, &Request::SealedChunk { name }).await {
            if let Some(b) = pyrate_core::seal::unseal(hash, &s) {
                return Ok(b);
            }
        }
        // A keeper's own node hands out its station's sound in the clear.
        if let Ok(Response::Chunk(Some(b))) = self.ask(node, &Request::Chunk { hash: *hash }).await {
            if blake3::hash(&b).as_bytes() == hash {
                return Ok(b);
            }
        }
        Err(anyhow!("not from {}", node.fmt_short()))
    }

    /// Once a minute: what a long-open tab has gathered and no longer needs goes. Cards
    /// gone quiet for six hours, logs beyond the eight last played (the tuned station and
    /// pockets always stay), and connections that have closed.
    fn trim(&self) {
        let now = raw_now();
        if now.saturating_sub(self.last_trim.load(std::sync::atomic::Ordering::Relaxed)) < 60_000 {
            return;
        }
        self.last_trim.store(now, std::sync::atomic::Ordering::Relaxed);
        let keep: HashSet<StationId> = self.pockets.lock().unwrap().keys().copied().chain(*self.tuned.lock().unwrap()).collect();
        let net_now = self.now();
        self.log.lock().unwrap().retain(|id, l| {
            keep.contains(id)
                || l.fresh(net_now)
                || l.announce.as_ref().is_some_and(|a| net_now.saturating_sub(a.at_ms) < 6 * 3_600_000)
        });
        let touched = self.touched.lock().unwrap().clone();
        let mut archives = self.archives.lock().unwrap();
        if archives.len() > 8 {
            let mut by: Vec<(u64, StationId)> = archives.keys().filter(|k| !keep.contains(k)).map(|k| (touched.get(k).copied().unwrap_or(0), *k)).collect();
            by.sort();
            let extra = archives.len().saturating_sub(8);
            for (_, k) in by.into_iter().take(extra) {
                archives.remove(&k);
            }
        }
        let live: HashSet<StationId> = archives.keys().copied().collect();
        drop(archives);
        self.clocks.lock().unwrap().retain(|k, _| live.contains(k));
        self.touched.lock().unwrap().retain(|k, _| live.contains(k));
        self.conns.lock().unwrap().retain(|_, c| c.close_reason().is_none());
        self.tokens_sent.lock().unwrap().retain(|k, _| live.contains(k) || keep.contains(k));
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
        if !log.contains_key(&station) {
            return;
        }
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
                if self.struck.lock().unwrap().contains(&a.station) {
                    return None;
                }
                let mut log = self.log.lock().unwrap();
                let l = log.entry(a.station).or_default();
                let fresh = l.announce.as_ref().is_none_or(|old| a.at_ms > old.at_ms);
                if !fresh {
                    return None;
                }
                if raw_now().abs_diff(a.at_ms) < 300_000 + 60_000 {
                    self.drift.lock().unwrap().sample(a.station.0, a.at_ms, raw_now());
                }
                let (s, head) = (a.station, a.head);
                l.announce = Some(a);
                drop(log);
                let ours = self.archives.lock().unwrap().get(&s).map(|x| x.len()).unwrap_or(0);
                (head > ours).then_some(s)
            }
            Gossip::Carrier { station, node, head } => {
                // A claim for a station with no card here is dropped: junk ids grow nothing.
                let pocket = self.pockets.lock().unwrap().contains_key(&station);
                let mut log = self.log.lock().unwrap();
                if !log.contains_key(&station) && !pocket {
                    return None;
                }
                let l = log.entry(station).or_default();
                let card = l.announce.as_ref().map(|a| a.head);
                net::note_carrier(&mut l.carriers, node, head, card, now);
                drop(log);
                let ours = self.archives.lock().unwrap().get(&station).map(|x| x.len()).unwrap_or(0);
                (head > ours).then_some(station)
            }
            Gossip::Aboard { station, hour, tokens } => {
                for (token, nonce) in tokens.into_iter().take(10_000) {
                    self.accept_token(station, hour, token, nonce);
                }
                None
            }
            g @ Gossip::Harbour { .. } => {
                // A carrier's own word on where it can be reached: into the tab's address book,
                // for carriers it knows and harbours only.
                let (node, relay) = g.harbour_checked(raw_now())?;
                let known = self.log.lock().unwrap().values().any(|l| l.carriers.contains_key(&node)) || self.harbour.iter().any(|h| *h.as_bytes() == node);
                if let (true, Ok(id), Ok(url)) = (known, EndpointId::from_bytes(&node), relay.parse::<iroh::RelayUrl>()) {
                    self.book.add_endpoint_info(iroh::EndpointAddr { id, addrs: [iroh::TransportAddr::Relay(url)].into_iter().collect() });
                }
                None
            }
        }
    }

    /// Once an hour, for the station on: one token, worked for, so the station counts this
    /// tab once. The token is a keyed hash nobody can tie to the last hour's.
    async fn count_me(&self) {
        if !self.listening.load(std::sync::atomic::Ordering::Relaxed) {
            return;
        }
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
        // Two hops away, as a phone does: the first sees this tab and not the station, the exit
        // sees the station and not the tab, and counts it for everyone.
        let g = Gossip::Aboard { station, hour, tokens: vec![(token, nonce)] };
        // A pocket's listeners are counted among its members only: a token on the public Log
        // would name the station to everyone.
        let pocket = self.pockets.lock().unwrap().get(&station).map(|p| p.0);
        if let Some(key) = pocket {
            let tx = self.pocket_tx.lock().unwrap().get(&station).cloned();
            if let (Some(tx), Ok(bytes)) = (tx, postcard::to_stdvec(&g)) {
                let _ = tx.broadcast(pyrate_core::pocket::seal(&key, &bytes).into()).await;
            }
            return;
        }
        if self.token_privately(station, hour, token, nonce).await.is_err() {
            // No second node to route through yet: the harbour hears it, as it would anyway.
            self.broadcast(&g).await;
        }
    }

    /// Sends the hourly token to an exit through a first hop. The first hop is drawn once a
    /// visit; the exit afresh each hour, never the first hop.
    async fn token_privately(&self, station: StationId, hour: u64, token: [u8; 32], nonce: u64) -> Result<()> {
        let mut nodes: Vec<EndpointId> = self.harbour.clone();
        let now = self.now();
        for l in self.log.lock().unwrap().values() {
            nodes.extend(l.carriers.iter().filter(|(_, (_, seen))| now.saturating_sub(*seen) < 10 * 60_000).filter_map(|(n, _)| EndpointId::from_bytes(n).ok()));
        }
        nodes.retain(|n| *n != self.ep.id());
        nodes.sort();
        nodes.dedup();
        if nodes.len() < 2 {
            return Err(anyhow!("one node only"));
        }
        let first = {
            let mut g = self.first_hop.lock().unwrap();
            if g.is_none_or(|f| !nodes.contains(&f)) {
                *g = Some(nodes[pick(nodes.len())]);
            }
            g.unwrap()
        };
        let exits: Vec<EndpointId> = nodes.into_iter().filter(|n| *n != first).collect();
        let exit = exits[pick(exits.len())];
        let ask = postcard::to_stdvec(&Ask::Token { station, hour, token, nonce })?;
        let (eph, ct, c) = pyrate_core::route::seal_to(exit.as_bytes(), &ask).ok_or_else(|| anyhow!("seal"))?;
        let blob = postcard::to_stdvec(&Hop::Sealed { eph, ct })?;
        let hop = postcard::to_stdvec(&Hop::Forward { next: *exit.as_bytes(), blob })?;
        match self.ask(first, &Request::Hop(hop)).await? {
            Response::Hop(raw) if pyrate_core::route::open_answer(&c, &raw).is_some() => {
                say("counted aboard through two hops");
                Ok(())
            }
            _ => Err(anyhow!("no answer")),
        }
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
        if self.struck.lock().unwrap().contains(station) {
            return json!({ "kind": "struck" });
        }
        self.touched.lock().unwrap().insert(*station, raw_now());
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
        let len = a.len() as u64;
        let mut clocks = self.clocks.lock().unwrap();
        if !matches!(clocks.get(station), Some((n, _)) if *n == len) {
            match Clock::new(a) {
                Ok(c) => {
                    clocks.insert(*station, (len, c));
                }
                Err(_) => {
                    clocks.remove(station);
                    return json!({ "kind": "dead air" });
                }
            }
        }
        let clock = &clocks.get(station).expect("just set").1;
        let Some(np) = clock.now(at) else {
            // Off the air until its next spell, or nothing yet to play.
            return match clock.upcoming(at, at + 7 * 86_400_000).first() {
                Some(b) => json!({ "kind": "off air", "back_at": b.0 }),
                None => json!({ "kind": "dead air" }),
            };
        };
        let Some((title, by, chunks, channels)) = Self::describe(a, &np.track) else { return json!({ "kind": "searching" }) };
        let Some(chunk) = chunks.get(np.chunk) else { return json!({ "kind": "dead air" }) };
        let art = a.track_art(&np.track).map(|r| hex(&r.hash));
        json!({
            "kind": "track", "track": hex(&np.track), "title": title, "by": by, "art": art,
            "index": np.chunk, "chunk": hex(&chunk.hash), "chunk_ms": chunk.duration_ms(),
            "chunk_start": at - np.chunk_offset_ms, "channels": channels,
            "position_ms": np.offset_ms, "duration_ms": np.duration_ms, "slot": np.slot,
            "next": chunks.get(np.chunk + 1).map(|c| hex(&c.hash)),
        })
    }

    fn stations(&self) -> Value {
        self.trim();
        let now = self.now();
        let banners: HashMap<StationId, String> = self
            .archives
            .lock()
            .unwrap()
            .iter()
            .filter_map(|(id, a)| Some((*id, hex(&a.art()?.1?.hash))))
            .collect();
        let pockets: HashSet<StationId> = self.pockets.lock().unwrap().keys().copied().collect();
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
                    "kinds": a.kinds, "mature": a.mature, "explicit": a.explicit, "tip": a.tip,
                    "cover": a.cover.map(|h| hex(&h)), "banner": banners.get(id), "accent": accent,
                    "carriers": l.carriers.values().filter(|(_, s)| now.saturating_sub(*s) < 10 * 60_000).count(),
                    "pocket": pockets.contains(id),
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
        let book2 = book.clone();
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
        // The tab publishes its address nowhere: it reaches out through relays, and peers answer
        // on the same connection or learn of it through gossip. It looks peers up first in its
        // own book (harbours and carriers' signed notes), then through number0's pkarr server.
        let ep = Endpoint::builder(presets::Minimal)
            .secret_key(SecretKey::from_bytes(&seed))
            .relay_mode(iroh::RelayMode::Custom(relays))
            .address_lookup(book.clone())
            .address_lookup(iroh::address_lookup::PkarrResolver::n0_dns())
            .bind()
            .await
            .map_err(|e| JsValue::from_str(&format!("bind: {e}")))?;
        let gossip = GossipNet::builder().max_message_size(128 * 1024).spawn(ep.clone());
        let proto = Proto(Arc::new(Mutex::new(None)));
        let router = Router::builder(ep.clone()).accept(ALPN, proto.clone()).accept(GOSSIP_ALPN, gossip.clone()).spawn();
        let ship = Arc::new(Ship {
            ep,
            gossip: gossip.clone(),
            book: book2,
            flock: Mutex::new(None),
            flock_off: std::sync::atomic::AtomicI64::new(0),
            flock_station: Mutex::new(None),
            pockets: Mutex::new(HashMap::new()),
            pocket_tx: Mutex::new(HashMap::new()),
            rooms: Mutex::new(HashMap::new()),
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
            drift: Mutex::new(pyrate_core::drift::Drift::new()),
            failed: Mutex::new(HashMap::new()),
            listening: std::sync::atomic::AtomicBool::new(false),
            first_hop: Mutex::new(None),
            clocks: Mutex::new(HashMap::new()),
            touched: Mutex::new(HashMap::new()),
            struck: Mutex::new(HashSet::new()),
            last_trim: std::sync::atomic::AtomicU64::new(0),
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

    /// Whether the visitor is listening. A tab is counted aboard only while it plays: looking
    /// at a station on the dial tells nobody anything.
    pub fn listening(&self, on: bool) {
        self.ship.listening.store(on, std::sync::atomic::Ordering::Relaxed);
        if on {
            let s = self.ship.clone();
            spawn(async move { s.count_me().await });
        }
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

    /// A QR code for a link, as JSON: its width in modules and a string of 1s and 0s.
    pub fn qr(&self, text: &str) -> Result<String, JsValue> {
        let code = qrcode::QrCode::new(text.as_bytes()).map_err(|e| js(anyhow!("{e}")))?;
        let bits: String = code.to_colors().iter().map(|c| if *c == qrcode::Color::Dark { '1' } else { '0' }).collect();
        Ok(json!({ "size": code.width(), "bits": bits }).to_string())
    }

    /// A station's page as JSON: on now, up next, the week's schedule and its notices,
    /// worked out in this tab from the station's own log. Times are network milliseconds.
    pub fn page(&self, id: &str, days: u32) -> Result<String, JsValue> {
        let st = StationId(unhex32(id).map_err(js)?);
        let archives = self.ship.archives.lock().unwrap();
        let Some(a) = archives.get(&st) else { return Ok("null".into()) };
        let v = pyrate_core::page::page_of(a, self.ship.now(), days.min(28) as u64).map_err(|e| js(anyhow!("{e}")))?;
        Ok(v.to_string())
    }

    /// Joins a station's chatter. Resolves when the tab is in the room.
    pub fn chat_join(&self, id: &str) -> Result<js_sys::Promise, JsValue> {
        let st = StationId(unhex32(id).map_err(js)?);
        let s = self.ship.clone();
        Ok(wasm_bindgen_futures::future_to_promise(async move {
            s.chat_join(st).await.map_err(js)?;
            Ok(JsValue::TRUE)
        }))
    }

    pub fn chat_leave(&self, id: &str) {
        if let Ok(b) = unhex32(id) {
            self.ship.rooms.lock().unwrap().remove(&StationId(b));
        }
    }

    /// The room as JSON: whether the tab is in it, how many it is linked to, and the lines.
    pub fn chat_read(&self, id: &str) -> Result<String, JsValue> {
        let st = StationId(unhex32(id).map_err(js)?);
        let me = self.ship.voice(&st).id().0;
        let rooms = self.ship.rooms.lock().unwrap();
        let Some(r) = rooms.get(&st) else { return Ok(json!({ "joined": false, "lines": [] }).to_string()) };
        let archives = self.ship.archives.lock().unwrap();
        let a = archives.get(&st);
        let lines: Vec<Value> = r.lines.iter().map(|l| {
            let keeper = l.line.from == st.0 || a.is_some_and(|a| a.deputies().contains(&l.line.from));
            let (kind, text, track, title) = match &l.line.say {
                pyrate_core::chat::Say::Text(t) => ("text", t.clone(), None, None),
                pyrate_core::chat::Say::Cheer => ("cheer", String::new(), None, None),
                pyrate_core::chat::Say::Request { track, note } => ("request", note.clone(), Some(hex(track)), a.map(|a| pyrate_core::page::describe_track(a, track).0).filter(|t| !t.is_empty())),
                pyrate_core::chat::Say::Hail { .. } => ("hail", String::new(), None, None),
            };
            json!({ "id": hex(&pyrate_core::chat::id(l)), "voice": StationId(l.line.from).call_sign(), "name": l.line.name, "at_ms": l.line.at_ms,
                    "kind": kind, "text": text, "track": track, "title": title, "keeper": keeper, "mine": l.line.from == me })
        }).collect();
        Ok(json!({ "joined": true, "neighbours": r.neighbours, "voice": StationId(me).call_sign(), "lines": lines }).to_string())
    }

    /// Hails the keeper: a short voice note sealed so only the station's keys open it.
    pub fn chat_hail(&self, id: &str, audio: &[u8], mime: &str, name: &str) -> Result<js_sys::Promise, JsValue> {
        let st = StationId(unhex32(id).map_err(js)?);
        let mut to = vec![st.0];
        if let Some(a) = self.ship.archives.lock().unwrap().get(&st) {
            to.extend(a.deputies());
        }
        let say = pyrate_core::chat::hail(&to, audio, mime).ok_or_else(|| JsValue::from_str("a hail is twenty seconds at most"))?;
        let l = pyrate_core::chat::speak(&self.ship.voice(&st), &st, name, self.ship.now(), say);
        let bytes = self.ship.room_bytes(&st, postcard::to_allocvec(&l).map_err(|e| js(anyhow!("{e}")))?);
        let tx = self.ship.rooms.lock().unwrap().get(&st).map(|r| r.tx.clone()).ok_or_else(|| JsValue::from_str("join the chatter first"))?;
        Ok(wasm_bindgen_futures::future_to_promise(async move {
            tx.broadcast(bytes.into()).await.map_err(|e| js(anyhow!("{e}")))?;
            Ok(JsValue::TRUE)
        }))
    }

    /// Says something in a station's chatter: kind is text, cheer or request.
    pub fn chat_send(&self, id: &str, kind: &str, text: &str, track: &str, name: &str) -> Result<js_sys::Promise, JsValue> {
        let st = StationId(unhex32(id).map_err(js)?);
        let say = match kind {
            "cheer" => pyrate_core::chat::Say::Cheer,
            "request" => pyrate_core::chat::Say::Request { track: unhex32(track).map_err(js)?, note: text.into() },
            _ if text.trim().is_empty() => return Err(JsValue::from_str("nothing to say")),
            _ => pyrate_core::chat::Say::Text(text.into()),
        };
        let l = pyrate_core::chat::speak(&self.ship.voice(&st), &st, name, self.ship.now(), say);
        let bytes = self.ship.room_bytes(&st, postcard::to_allocvec(&l).map_err(|e| js(anyhow!("{e}")))?);
        let tx = self.ship.rooms.lock().unwrap().get(&st).map(|r| r.tx.clone()).ok_or_else(|| JsValue::from_str("join the chatter first"))?;
        self.ship.hear(st, l);
        Ok(wasm_bindgen_futures::future_to_promise(async move {
            tx.broadcast(bytes.into()).await.map_err(|e| js(anyhow!("{e}")))?;
            Ok(JsValue::TRUE)
        }))
    }

    /// Follows a flock's host from its link (#flock=<id>@<relay>~<code>): this tab's clock
    /// is set to the host's over pings, and it plays what the host plays.
    pub fn flock_join(&self, link: &str) -> Result<(), JsValue> {
        let tail = link.split("flock=").nth(1).ok_or_else(|| JsValue::from_str("not a flock link"))?;
        let (who, code_hex) = tail.split_once('~').ok_or_else(|| JsValue::from_str("not a flock link"))?;
        let (id, relay) = who.split_once('@').unwrap_or((who, ""));
        let host: EndpointId = id.parse().map_err(|_| JsValue::from_str("not a flock link"))?;
        let mut code = [0u8; 8];
        pyrate_core::unhex_into(code_hex, &mut code).ok_or_else(|| JsValue::from_str("bad code"))?;
        if let Ok(url) = relay.parse() {
            self.ship.book.add_endpoint_info(iroh::EndpointAddr { id: host, addrs: [iroh::TransportAddr::Relay(url)].into_iter().collect() });
        }
        *self.ship.flock.lock().unwrap() = Some((host, code));
        let s = self.ship.clone();
        spawn(async move {
            while *s.flock.lock().unwrap() == Some((host, code)) {
                let mut best: Option<(u64, i64, Option<StationId>)> = None;
                for _ in 0..8 {
                    let t0 = s.base_now();
                    let Ok(Response::Flock { now_ms: h, station }) = s.ask(host, &Request::Flock { code }).await else { continue };
                    let t1 = s.base_now();
                    if h == 0 {
                        break;
                    }
                    let rtt = t1.saturating_sub(t0);
                    let off = h as i64 + rtt as i64 / 2 - t1 as i64;
                    if best.as_ref().is_none_or(|b| rtt < b.0) {
                        best = Some((rtt, off, station));
                    }
                }
                if let Some((_, off, station)) = best {
                    let old = s.flock_off.load(std::sync::atomic::Ordering::Relaxed);
                    s.flock_off.store(if old == 0 { off } else { old + (off - old) / 4 }, std::sync::atomic::Ordering::Relaxed);
                    *s.flock_station.lock().unwrap() = station;
                }
                sleep(Duration::from_secs(20)).await;
            }
        });
        Ok(())
    }

    /// Joins a pocket station from its invitation (#pocket=...). Resolves to the station's id.
    pub fn pocket_join(&self, link: &str) -> Result<js_sys::Promise, JsValue> {
        let (st, key, host) = pyrate_core::pocket::read_invitation(link).ok_or_else(|| JsValue::from_str("not an invitation"))?;
        let st = StationId(st);
        let (id, relay) = host.split_once('@').unwrap_or((host.as_str(), ""));
        let hosts: Vec<EndpointId> = id.parse().ok().into_iter().collect();
        if let (Some(h), Ok(url)) = (hosts.first(), relay.parse()) {
            self.ship.book.add_endpoint_info(iroh::EndpointAddr { id: *h, addrs: [iroh::TransportAddr::Relay(url)].into_iter().collect() });
        }
        self.ship.pockets.lock().unwrap().insert(st, (key, hosts.clone()));
        let s = self.ship.clone();
        Ok(wasm_bindgen_futures::future_to_promise(async move {
            let sub = s.gossip.subscribe(TopicId::from_bytes(pyrate_core::pocket::topic(&key)), hosts.clone()).await.map_err(|e| js(anyhow!("{e}")))?;
            let (tx, mut rx) = sub.split();
            s.pocket_tx.lock().unwrap().insert(st, tx);
            let s2 = s.clone();
            spawn(async move {
                while let Some(ev) = rx.next().await {
                    let Ok(Event::Received(m)) = ev else { continue };
                    let Some(plain) = pyrate_core::pocket::open(&key, &m.content) else { continue };
                    let Ok(g) = postcard::from_bytes::<Gossip>(&plain) else { continue };
                    let ok = match &g {
                        Gossip::Station(a) => a.station == st,
                        Gossip::Carrier { station, .. } => *station == st,
                        Gossip::Aboard { station, .. } => *station == st,
                        Gossip::Harbour { .. } => true,
                    };
                    if ok {
                        s2.on_gossip(g);
                    }
                }
            });
            let _ = s.pocket_sync(st, key, hosts).await;
            Ok(JsValue::from_str(&st.to_hex()))
        }))
    }

    /// The flock host's station, as hex, or empty.
    pub fn flock_station(&self) -> String {
        self.ship.flock_station.lock().unwrap().map(|s| s.to_hex()).unwrap_or_default()
    }

    pub fn flock_leave(&self) {
        *self.ship.flock.lock().unwrap() = None;
        self.ship.flock_off.store(0, std::sync::atomic::Ordering::Relaxed);
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
