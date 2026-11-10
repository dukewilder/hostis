//! pyrate-node: one install, one full node. It carries, relays, gossips the Log,
//! serves anyone who asks, and (in the app) plays.
//!
//! No rights reserved.

pub mod art;
mod chat;
mod flock;
mod pocket;
pub mod hop;
pub mod trust;
pub mod clockfix;
pub mod net;
pub mod page;
#[cfg(feature = "playback")]
pub mod player;
pub mod store;
pub mod web;

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, anyhow};
use futures_lite::StreamExt;
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
use net::{ALPN, Announce, Gossip, MAX_FRAME, Request, Response, Sketch};
use pyrate_core::{Archive, Body, Hash, Profile, SignedEntry, StationId, clock::Clock};
use serde_json::{Value, json};
use store::Store;

/// A line in the system log: logcat on Android, stderr elsewhere.
pub fn say(msg: &str) {
    #[cfg(target_os = "android")]
    {
        #[link(name = "log")]
        unsafe extern "C" {
            fn __android_log_write(prio: i32, tag: *const std::ffi::c_char, text: *const std::ffi::c_char) -> i32;
        }
        let tag = std::ffi::CString::new("pyrate").unwrap();
        let text = std::ffi::CString::new(msg.replace('\0', "")).unwrap();
        unsafe {
            __android_log_write(4, tag.as_ptr(), text.as_ptr());
        }
    }
    #[cfg(not(target_os = "android"))]
    eprintln!("{msg}");
}

pub fn raw_now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64
}

/// The network's time: our clock, corrected by the median of the stations we hear.
pub fn now_ms() -> u64 {
    (raw_now_ms() as i64 + clockfix::offset_ms()) as u64
}

#[derive(Clone, Debug)]
pub struct Config {
    pub dir: PathBuf,
    pub quota: u64,
    /// An always-on node: serves, relays and keeps its own stations whole.
    pub lantern: bool,
    /// A Lantern that also carries every station in the Log, and takes strangers' stems
    /// aboard. Off by default: a Lantern answers for what sits on its disk, so it carries
    /// its own stations unless its keeper chooses the whole fleet.
    pub carry_all: bool,
    pub bootstrap: Vec<EndpointId>,
    /// Ask public resolvers instead of the system's. For processes with no Android context.
    pub public_dns: bool,
    /// Phones: fetch through two hops and publish along the stem. Lanterns: direct.
    pub private: bool,
    /// Bytes donated to the fleet as ballast, filled only when the app says conditions allow.
    pub ballast: u64,
    /// The three release keys. The first is also the Releases station, the second the
    /// Empty means this build trusts no updates and follows no list.
    pub release_keys: Vec<[u8; 32]>,
}

/// Relays run by HOSTIS and the crew, used alongside number0's. Any iroh relay will do;
/// a relay only forwards traffic it cannot read.
pub const CREW_RELAYS: &[&str] = &["https://relay.hostis.black"];

/// Points a rewritten entry's references at the entries' new hashes.
pub(crate) fn remap(body: Body, moved: &HashMap<Hash, Hash>) -> Body {
    let m = |h: Hash| *moved.get(&h).unwrap_or(&h);
    match body {
        Body::Withdraw { track } => Body::Withdraw { track: m(track) },
        Body::TrackArt { track, art } => Body::TrackArt { track: m(track), art },
        Body::Rota(mut r) => {
            for b in &mut r.blocks {
                for t in &mut b.tracks {
                    *t = m(*t);
                }
            }
            r.ident = r.ident.map(m);
            Body::Rota(r)
        }
        b => b,
    }
}

/// A track's title and artist, by its entry hash.
pub(crate) fn describe_track(a: &Archive, track: &Hash) -> (String, String) {
    for e in a.entries() {
        if let Body::Track { title, by, .. } = e.entry.body() {
            if e.hash().ok().as_ref() == Some(track) {
                return (title.clone(), by.clone());
            }
        }
    }
    (String::new(), String::new())
}

/// One chunk as it played, kept briefly for Tape it.
pub(crate) struct Heard {
    station: StationId,
    channels: u8,
    bytes: Vec<u8>,
    title: String,
}

fn keep_heard(inner: &Inner, station: StationId, channels: u8, bytes: &[u8], title: String) {
    let mut h = inner.heard.lock().unwrap();
    h.push_back(Heard { station, channels, bytes: bytes.to_vec(), title });
    while h.len() > 25 {
        h.pop_front();
    }
}

#[derive(Default)]
struct Listing {
    announce: Option<Announce>,
    carriers: HashMap<[u8; 32], (u64, u64)>,
    sketches: HashMap<u64, Sketch>,
    /// Tokens already counted, by hour, so a token gossiped twice counts once.
    heard: HashMap<u64, std::collections::HashSet<[u8; 32]>>,
    /// Tokens this node vouches for and has yet to gossip.
    outbox: Vec<([u8; 32], u64, u64)>,
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
            if let Some(p) = self.sketches.get(&(hour - 1)) {
                s.merge(&p.0);
            }
        }
        net::round_aboard(s.estimate())
    }
}

struct Tuned {
    station: StationId,
    since: u64,
    #[allow(dead_code)]
    playhead: Option<u64>,
}

pub struct Inner {
    cfg: Config,
    endpoint: Endpoint,
    secret: [u8; 32],
    /// Steady across address changes: ballast by lot, chatter voices, listener tokens.
    seed: [u8; 32],
    store: Mutex<Store>,
    log: Mutex<HashMap<StationId, Listing>>,
    conns: Mutex<HashMap<EndpointId, Connection>>,
    gossip_tx: Mutex<Option<GossipSender>>,
    gossip: GossipNet,
    /// Station chatter this node has joined, held in memory only.
    chats: Mutex<HashMap<StationId, chat::Room>>,
    /// Flock: the code this phone hosts under, or the host it follows and the code.
    flock_host: Mutex<Option<[u8; 8]>>,
    flock_with: Mutex<Option<(EndpointId, [u8; 8])>>,
    flock_offset: std::sync::atomic::AtomicI64,
    flock_trim: std::sync::atomic::AtomicI64,
    flock_rtt: std::sync::atomic::AtomicU64,
    /// Pocket stations this node keeps or was invited to: their keys and their own topics.
    pockets: Mutex<pocket::Pockets>,
    tuned: Mutex<Option<Tuned>>,
    jobs: Mutex<HashMap<u64, String>>,
    pow: Mutex<HashMap<StationId, u64>>,
    #[cfg(feature = "playback")]
    queue: player::SharedQueue,
    #[cfg(feature = "playback")]
    np: Mutex<Value>,
    /// Art seen while browsing: held in memory only, never written to the device.
    pub(crate) art_mem: Mutex<art::Shelf>,
    /// The live broadcast in progress: station and when it began.
    live: Mutex<Option<(StationId, u64)>>,
    handle: tokio::runtime::Handle,
    ballast_ok: std::sync::atomic::AtomicBool,
    /// What this phone gives the fleet, in bytes: 1 GB, or more as a drawer Lantern.
    ballast_bytes: std::sync::atomic::AtomicU64,
    tokens_sent: Mutex<HashMap<StationId, u64>>,
    /// Our stations no stem would take, now on the air straight from this device, and how
    /// many stems in a row have found no taker.
    pub(crate) direct: Mutex<std::collections::HashSet<StationId>>,
    /// The maroon list as last read, and when.
    pub(crate) refused_cache: Mutex<(u64, std::collections::HashSet<StationId>)>,
    /// The last two and a half minutes of what played, for Tape it.
    pub(crate) heard: Mutex<std::collections::VecDeque<Heard>>,
    /// Never fall back to asking a carrier directly; stutter rather than show this address.
    pub(crate) strict: std::sync::atomic::AtomicBool,
    stem_misses: Mutex<HashMap<StationId, u32>>,
    /// Stem cargo relayed lately, by fingerprint: a stem that comes round again spreads.
    pub(crate) stem_seen: Mutex<std::collections::VecDeque<Hash>>,
    /// On mobile data a phone listens and broadcasts, and carries nobody else's traffic.
    relay_ok: std::sync::atomic::AtomicBool,
}

pub struct Node {
    pub rt: tokio::runtime::Runtime,
    pub inner: Arc<Inner>,
    _router: Router,
}

#[derive(Debug, Clone)]
struct Proto(Arc<Mutex<Option<Arc<Inner>>>>);

impl ProtocolHandler for Proto {
    async fn accept(&self, conn: Connection) -> std::result::Result<(), AcceptError> {
        let inner = self.0.lock().unwrap().clone();
        let Some(inner) = inner else { return Ok(()) };
        while let Ok((mut send, mut recv)) = conn.accept_bi().await {
            let inner = inner.clone();
            tokio::spawn(async move {
                let Ok(bytes) = recv.read_to_end(MAX_FRAME).await else { return };
                let Ok(req) = postcard::from_bytes::<Request>(&bytes) else { return };
                let resp = match req {
                    Request::Hop(b) => Response::Hop(inner.on_hop(&b).await),
                    other => inner.serve(other),
                };
                if let Ok(out) = postcard::to_stdvec(&resp) {
                    let _ = send.write_all(&out).await;
                    let _ = send.finish();
                }
            });
        }
        Ok(())
    }
}

impl std::fmt::Debug for Inner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Inner")
    }
}

impl Inner {
    fn serve(&self, req: Request) -> Response {
        if let Request::PocketLog { tag, from } = &req {
            return Response::Entries(self.pocket_entries(tag, *from));
        }
        if let Request::PocketChatter { tag } = &req {
            return Response::Chatter(self.pocket_chatter(tag));
        }
        let pocket = matches!(&req, Request::Log { station, .. } if self.is_pocket(station));
        let mut store = self.store.lock().unwrap();
        match req {
            // A pocket station's log is never handed out by its id, sealed or not.
            Request::Log { .. } if pocket => Response::Entries(vec![]),
            Request::Log { station, from } if !self.refused_quick(&station) => {
                store.open_log(&station);
                let out = store
                    .logs
                    .get(&station)
                    .map(|a| {
                        a.entries().iter().skip(from as usize).take(2000).filter_map(|e| e.to_bytes().ok()).collect()
                    })
                    .unwrap_or_default();
                Response::Entries(out)
            }
            Request::Log { .. } => Response::Entries(vec![]),
            // In the clear, a node hands out only its own stations' sound, and art it is showing.
            // Everything it carries for others goes out sealed, by name, unopened.
            Request::Chunk { hash } => {
                let own = store.own().iter().any(|id| store.logs.get(id).is_some_and(|a| a.art_hashes().contains(&hash))) || store.own().iter().any(|id| store.chunks_of(id).contains(&hash));
                Response::Chunk(if own { store.get_chunk(&hash) } else { None }.or_else(|| self.art_mem.lock().unwrap().get(&hash)))
            }
            // On mobile data a phone serves strangers nothing: it is the owner's data plan.
            Request::SealedChunk { .. } if !self.cfg.lantern && !self.relay_ok.load(std::sync::atomic::Ordering::Relaxed) => Response::Sealed(None),
            Request::SealedChunk { name } => Response::Sealed(store.get_sealed(&name)),
            Request::Flock { code } => self.flock_answer(&code),
            Request::PocketLog { .. } => Response::Entries(vec![]),
            Request::Hop(_) => Response::Hop(vec![]),
            Request::Chatter { station } => Response::Chatter(self.chatter_for(&station)),
            Request::PocketChatter { tag } => Response::Chatter(self.pocket_chatter(&tag)),
        }
    }

    async fn conn(&self, node: EndpointId) -> Result<Connection> {
        if let Some(c) = self.conns.lock().unwrap().get(&node) {
            if c.close_reason().is_none() {
                return Ok(c.clone());
            }
        }
        let c = tokio::time::timeout(Duration::from_secs(10), self.endpoint.connect(node, ALPN))
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
        match tokio::time::timeout(Duration::from_secs(20), fut).await {
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

    fn carriers(&self, station: &StationId) -> Vec<EndpointId> {
        let me = *self.endpoint.id().as_bytes();
        let log = self.log.lock().unwrap();
        let mut v: Vec<([u8; 32], u64, u64)> = log
            .get(station)
            .map(|l| l.carriers.iter().filter(|(n, _)| **n != me).map(|(n, (h, s))| (*n, *h, *s)).collect())
            .unwrap_or_default();
        v.sort_by(|a, b| b.1.cmp(&a.1).then(b.2.cmp(&a.2)));
        v.into_iter().filter_map(|(n, _, _)| EndpointId::from_bytes(&n).ok()).collect()
    }

    /// One of our own stations, which another phone (a hand, or the captain) may also write.
    /// Our copy is checked against the network's from the start: newer entries come aboard,
    /// and if two phones wrote at once, ours are rewritten on top of the network's.
    async fn sync_own(&self, station: StationId) -> Result<usize> {
        // A pocket is asked of its members, by its tag: its id never leaves the circle.
        if let Some(k) = self.pocket_key(&station) {
            let remote = self.pocket_fetch(station, k, 0).await;
            if remote.is_empty() {
                return Ok(0);
            }
            return self.reconcile(station, remote);
        }
        let remote: Vec<SignedEntry> = {
            let mut got = None;
            if self.cfg.private {
                if let Ok(raw) = self.ask_privately(&hop::Ask::Log { station, from: 0 }).await {
                    if let Ok(Response::Entries(list)) = postcard::from_bytes::<Response>(&raw) {
                        got = Some(list);
                    }
                }
            }
            if got.is_none() && !self.strict.load(std::sync::atomic::Ordering::Relaxed) {
                for node in self.carriers(&station) {
                    if let Ok(Response::Entries(list)) = self.ask(node, &Request::Log { station, from: 0 }).await {
                        got = Some(list);
                        break;
                    }
                }
            }
            got.unwrap_or_default().iter().filter_map(|b| SignedEntry::from_bytes(b).ok()).collect()
        };
        if remote.is_empty() {
            return Ok(0);
        }
        self.reconcile(station, remote)
    }

    /// Brings our copy of an own station's log level with the network's.
    pub(crate) fn reconcile(&self, station: StationId, remote: Vec<SignedEntry>) -> Result<usize> {
        let mut s = self.store.lock().unwrap();
        let ours: Vec<SignedEntry> = s.logs.get(&station).map(|a| a.entries().to_vec()).unwrap_or_default();
        let k = ours.iter().zip(remote.iter()).take_while(|(a, b)| a == b).count();
        if k == ours.len() {
            // Behind or level: take what is new.
            return s.put_entries(station, remote[k..].to_vec());
        }
        if k == remote.len() {
            // Ahead: the network will catch up from us.
            return Ok(0);
        }
        // Two phones wrote at once. The network's copy stands; ours are written again after it.
        let mut fresh = Archive::new(station);
        for e in &remote {
            fresh.append(e.clone())?;
        }
        s.replace_log(station, fresh)?;
        let mut moved: HashMap<Hash, Hash> = HashMap::new();
        let mut redone = 0;
        for e in &ours[k..] {
            let old = e.hash()?;
            let body = remap(e.entry.body().clone(), &moved);
            if let Ok(n) = s.write(station, now_ms(), body) {
                moved.insert(old, n.hash()?);
                redone += 1;
            }
        }
        say(&format!("{}: two phones wrote at once; {redone} of ours rewritten after the network's", station.call_sign()));
        Ok(redone)
    }

    pub async fn sync_log_direct(&self, station: StationId) -> Result<usize> {
        let mut added = 0;
        for node in self.carriers(&station) {
            let from = self.store.lock().unwrap().logs.get(&station).map(|a| a.len()).unwrap_or(0);
            let Ok(Response::Entries(raw)) = self.ask(node, &Request::Log { station, from }).await else { continue };
            let entries: Vec<SignedEntry> = raw.iter().filter_map(|b| SignedEntry::from_bytes(b).ok()).collect();
            if entries.is_empty() {
                continue;
            }
            match self.store.lock().unwrap().put_entries(station, entries) {
                Ok(n) => {
                    say(&format!("log {}: {n} new entries from {}", station.call_sign(), node.fmt_short()));
                    added += n
                }
                Err(e) => tracing::warn!("log from {node}: {e}"),
            }
            break;
        }
        Ok(added)
    }

    fn is_own(&self, station: &StationId) -> bool {
        self.store.lock().unwrap().keys.contains_key(station)
    }

    /// Brings a station's log up to date: privately on phones, directly on Lanterns.
    pub async fn sync_log(&self, station: StationId) -> Result<usize> {
        if self.is_own(&station) {
            return self.sync_own(station).await;
        }
        if let Some(k) = self.pocket_key(&station) {
            return self.pocket_sync(station, k).await;
        }
        if self.cfg.private {
            if let Ok(n) = self.private_log(station).await {
                return Ok(n);
            }
            say("private route failed for a log; asking a carrier directly");
        }
        self.sync_log_direct(station).await
    }

    /// A chunk from the hold, or from the network: privately on phones, directly on Lanterns.
    pub async fn chunk(&self, station: &StationId, hash: &Hash) -> Result<Vec<u8>> {
        self.fetch(station, hash, true).await
    }

    /// Fetches a chunk by its fingerprint. `keep` writes it to the hold; without it, the
    /// bytes pass through memory and nothing lands on the device.
    pub async fn fetch(&self, station: &StationId, hash: &Hash, keep: bool) -> Result<Vec<u8>> {
        if let Some(b) = self.store.lock().unwrap().get_chunk(hash) {
            return Ok(b);
        }
        // A pocket's sound comes from its members, who already know each other: a private
        // route would hand a stranger at the exit the pocket's id.
        if self.cfg.private && !self.is_pocket(station) {
            if let Ok(b) = self.private_chunk(*station, *hash, keep).await {
                return Ok(b);
            }
            if self.strict.load(std::sync::atomic::Ordering::Relaxed) {
                return Err(anyhow!("private route failed, and this node never asks directly"));
            }
            say("private route failed for a chunk; asking a carrier directly");
        }
        self.chunk_direct(station, hash, keep).await
    }

    pub async fn chunk_direct(&self, station: &StationId, hash: &Hash, keep: bool) -> Result<Vec<u8>> {
        if let Some(b) = self.store.lock().unwrap().get_chunk(hash) {
            return Ok(b);
        }
        let name = pyrate_core::seal::name(hash);
        for node in self.carriers(station) {
            // Sealed first: the carrier hands on bytes it never opened.
            if let Ok(Response::Sealed(Some(s))) = self.ask(node, &Request::SealedChunk { name }).await {
                let Some(b) = pyrate_core::seal::unseal(hash, &s) else { continue };
                if !keep || self.store.lock().unwrap().put_sealed(hash, &s).is_ok() {
                    return Ok(b);
                }
            }
            // A keeper hands out its own station's sound in the clear.
            if let Ok(Response::Chunk(Some(b))) = self.ask(node, &Request::Chunk { hash: *hash }).await {
                if blake3::hash(&b).as_bytes() != hash {
                    continue;
                }
                if !keep || self.store.lock().unwrap().put_chunk(hash, &b).is_ok() {
                    return Ok(b);
                }
            }
        }
        Err(anyhow!("no carrier has that chunk"))
    }

    /// Sealed bytes by name, from this disk or a carrier, passed on unopened. An exit fetches
    /// this way for a listener on a private route: it never learns the chunk's address.
    pub async fn sealed_direct(&self, station: &StationId, name: &Hash) -> Result<Vec<u8>> {
        if let Some(b) = self.store.lock().unwrap().get_sealed(name) {
            return Ok(b);
        }
        for node in self.carriers(station) {
            if let Ok(Response::Sealed(Some(s))) = self.ask(node, &Request::SealedChunk { name: *name }).await {
                return Ok(s);
            }
        }
        Err(anyhow!("no carrier has that chunk"))
    }

    async fn broadcast(&self, g: &Gossip) {
        let tx = self.gossip_tx.lock().unwrap().clone();
        if let (Some(tx), Ok(bytes)) = (tx, postcard::to_stdvec(g)) {
            let _ = tx.broadcast(bytes.into()).await;
        }
    }

    fn on_gossip(&self, g: Gossip) -> Option<StationId> {
        let now = now_ms();
        let mut log = self.log.lock().unwrap();
        match g {
            Gossip::Station(a) => {
                if a.at_ms > now + 5 * 60_000 || !a.verify() {
                    return None;
                }
                // A card signed by a deputy the station has since revoked is refused.
                if let Some(p) = a.proof.as_ref().and_then(|b| SignedEntry::from_bytes(b).ok()) {
                    if let Body::Deputy { key, .. } = &p.entry.body {
                        let store = self.store.lock().unwrap();
                        if let Some(arch) = store.logs.get(&a.station) {
                            if arch.len() > p.entry.seq && !arch.deputies().contains(key) {
                                return None;
                            }
                        }
                    }
                }
                self.store.lock().unwrap().open_log(&a.station);
                let fresh = log.get(&a.station).and_then(|l| l.announce.as_ref()).is_none_or(|old| a.at_ms > old.at_ms);
                if fresh && now.saturating_sub(a.at_ms) < 60_000 {
                    clockfix::sample(a.station.0, a.at_ms, raw_now_ms());
                }
                // The Log keeps at most 5,000 cards: past that, a new station must outdo the weakest.
                if log.len() >= 5_000 && !log.contains_key(&a.station) {
                    let weakest = log.iter().min_by_key(|(_, l)| (l.aboard(now), l.announce.as_ref().map(|x| x.at_ms).unwrap_or(0))).map(|(k, _)| *k);
                    if let Some(w) = weakest {
                        log.remove(&w);
                    }
                }
                let l = log.entry(a.station).or_default();
                if l.announce.is_none() {
                    // A node keeps no record of what passes through it: only its own stations are named.
                    if self.store.lock().unwrap().keys.contains_key(&a.station) {
                        say(&format!("in the Log: {} {}", a.station.call_sign(), a.name));
                    }
                }
                if l.announce.as_ref().is_none_or(|old| a.at_ms > old.at_ms) {
                    let s = a.station;
                    let head = a.head;
                    l.announce = Some(a);
                    let ours = self.store.lock().unwrap().logs.get(&s).map(|x| x.len()).unwrap_or(0);
                    if head > ours {
                        return Some(s);
                    }
                }
            }
            Gossip::Carrier { station, node, head } => {
                log.entry(station).or_default().carriers.insert(node, (head, now));
                let ours = self.store.lock().unwrap().logs.get(&station).map(|x| x.len()).unwrap_or(0);
                if head > ours {
                    return Some(station);
                }
            }
            Gossip::Aboard { station, hour, tokens } => {
                drop(log);
                for (token, nonce) in tokens.into_iter().take(10_000) {
                    self.accept_token(station, hour, token, nonce, false);
                }
                return None;
            }
        }
        None
    }

    fn wants(&self, station: &StationId) -> bool {
        if self.cfg.lantern {
            return true;
        }
        let tuned = self.tuned.lock().unwrap().as_ref().map(|t| t.station);
        tuned == Some(*station) || self.store.lock().unwrap().logs.contains_key(station)
    }

    fn token(&self, station: &StationId, hour: u64) -> [u8; 32] {
        let mut h = blake3::Hasher::new_keyed(&self.seed);
        h.update(&station.0);
        h.update(&hour.to_le_bytes());
        *h.finalize().as_bytes()
    }

    fn pow_for(&self, station: &StationId) -> u64 {
        if let Some(n) = self.pow.lock().unwrap().get(station) {
            return *n;
        }
        // Never touch the store lock here: callers may hold it.
        let p = self.cfg.dir.join("keys").join(format!("{}.pow", station.to_hex()));
        // A cached answer is checked before it is trusted: a stale one would keep the
        // station out of every Log without a word.
        let cached = std::fs::read_to_string(&p).ok().and_then(|s| s.trim().parse().ok()).filter(|n| net::pow_ok(station, *n));
        let n = cached.unwrap_or_else(|| {
            let n = net::pow_find(station);
            let _ = std::fs::write(&p, n.to_string());
            n
        });
        self.pow.lock().unwrap().insert(*station, n);
        n
    }

    /// Every twenty seconds: fly our colours, say what we carry, count who is aboard.
    async fn tick(self: &Arc<Self>) {
        let now = now_ms();
        let hour = now / 3_600_000;
        let me = *self.endpoint.id().as_bytes();

        let (own, carried): (Vec<StationId>, Vec<(StationId, u64)>) = {
            let s = self.store.lock().unwrap();
            (s.own(), s.logs.iter().filter(|(_, a)| !a.is_struck()).map(|(id, a)| (*id, a.len())).collect())
        };
        for id in own {
            let pow = self.pow_for(&id);
            let a = {
                let s = self.store.lock().unwrap();
                let Some(log) = s.logs.get(&id) else { continue };
                let Some(Body::Colours { name, about, port, .. }) = log.colours().cloned() else { continue };
                if log.is_struck() {
                    continue;
                }
                let live = log.entries().last().is_some_and(|e| matches!(e.entry.body(), Body::Live { .. }) && now.saturating_sub(e.entry.at_ms) < 25_000);
                let key = s.keys.get(&id).unwrap();
                let proof = (key.id() != id).then(|| {
                    log.entries().iter().find(|e| matches!(&e.entry.body, Body::Deputy { key: k, .. } if *k == key.id().0)).and_then(|e| e.to_bytes().ok())
                }).flatten();
                let since = log.entries().first().map(|e| e.entry.at_ms).unwrap_or(now);
                let art = log.art();
                let cover = art.and_then(|a| a.0).map(|a| a.hash);
                let accent = art.map(|a| a.2);
                let (kinds, mature) = log.details().unwrap_or_default();
                let tip = match log.colours() {
                    Some(Body::Colours { tip, .. }) => tip.clone(),
                    _ => None,
                };
                let explicit = log.explicit();
                net::Card { station: id, head: log.len(), name, about, port, live, at_ms: now, pow, since, proof, cover, accent, kinds, mature, tip, explicit }
                    .sign(key)
            };
            // A pocket station's card goes only to its members, sealed, on its own topic.
            if self.is_pocket(&id) {
                self.pocket_card(id, a).await;
                continue;
            }
            if self.cfg.private && !self.direct.lock().unwrap().contains(&id) {
                // Never show the network our own station first: send it down the stem.
                if self.stem_push(id, Some(a.clone())).await {
                    self.stem_misses.lock().unwrap().remove(&id);
                    continue;
                }
                // A station nobody can hear is worse than one heard from a known address.
                // After three stems with no taker it goes on the air from here, and the
                // owner is told.
                let misses = {
                    let mut m = self.stem_misses.lock().unwrap();
                    let e = m.entry(id).or_insert(0);
                    *e += 1;
                    *e
                };
                if misses < 3 || self.strict.load(std::sync::atomic::Ordering::Relaxed) {
                    continue;
                }
                self.direct.lock().unwrap().insert(id);
                say("no node took the stem: the station is on the air directly from this device");
            }
            self.on_gossip(Gossip::Station(a.clone()));
            self.broadcast(&Gossip::Station(a)).await;
        }
        let own_set: std::collections::HashSet<StationId> = self.store.lock().unwrap().own().into_iter().collect();
        for (station, head) in carried {
            // A pocket's carriers say so only to its members.
            if self.is_pocket(&station) {
                let g = Gossip::Carrier { station, node: me, head };
                self.on_gossip(g.clone());
                self.pocket_say(station, &g).await;
                continue;
            }
            // A phone never says what it listened to: it announces only its ballast, which
            // the hash chose for it, and only while it is on Wi-Fi and charging. Away from the
            // charger it serves nobody, and its ballast set, which would tie one day's fresh id
            // to the next and one network to another, goes unsaid.
            let direct = self.direct.lock().unwrap().contains(&station);
            if self.cfg.private && !direct {
                let ballast = !own_set.contains(&station) && self.store.lock().unwrap().pinned.contains(&station);
                if !ballast || !self.ballast_ok.load(std::sync::atomic::Ordering::Relaxed) {
                    continue;
                }
            }
            let g = Gossip::Carrier { station, node: me, head };
            self.on_gossip(g.clone());
            self.broadcast(&g).await;
            let a = self.log.lock().unwrap().get(&station).and_then(|l| l.announce.clone());
            if let Some(a) = a {
                self.broadcast(&Gossip::Station(a)).await;
            }
        }
        let tuned = self.tuned.lock().unwrap().as_ref().map(|t| t.station);
        if let Some(station) = tuned {
            let due = self.tokens_sent.lock().unwrap().get(&station) != Some(&hour);
            if due {
                self.tokens_sent.lock().unwrap().insert(station, hour);
                let token = self.token(&station, hour);
                let nonce = tokio::task::spawn_blocking(move || net::token_find(&station, hour, &token)).await.unwrap_or(0);
                if self.is_pocket(&station) {
                    // A pocket's listeners are counted among its members only, sealed on its own
                    // topic: a token on the public Log would name the station to everyone.
                    self.accept_token(station, hour, token, nonce, false);
                    self.pocket_say(station, &Gossip::Aboard { station, hour, tokens: vec![(token, nonce)] }).await;
                } else if self.cfg.private {
                    // Counted by a stranger two hops away, so neighbours never see what we hear.
                    self.private_token(station, hour, token, nonce).await;
                } else {
                    self.accept_token(station, hour, token, nonce, true);
                }
            }
        }
        let outgoing: Vec<Gossip> = {
            let pockets: std::collections::HashSet<StationId> = self.pockets.lock().unwrap().keys().copied().collect();
            let mut log = self.log.lock().unwrap();
            log.iter_mut()
                .filter(|(st, l)| !l.outbox.is_empty() && !pockets.contains(st))
                .flat_map(|(st, l)| {
                    let mut by_hour: HashMap<u64, Vec<([u8; 32], u64)>> = HashMap::new();
                    for (t, n, h) in l.outbox.drain(..) {
                        by_hour.entry(h).or_default().push((t, n));
                    }
                    by_hour.into_iter().map(|(hour, tokens)| Gossip::Aboard { station: *st, hour, tokens }).collect::<Vec<_>>()
                })
                .collect()
        };
        for g in outgoing {
            self.broadcast(&g).await;
        }
    }

    /// Tunes to a station, as the dial does: used when a flock follows its host.
    pub(crate) fn tune_to(self: &Arc<Self>, st: StationId) -> Result<()> {
        if self.refused().contains(&st) {
            anyhow::bail!("marooned");
        }
        *self.tuned.lock().unwrap() = Some(Tuned { station: st, since: now_ms(), playhead: None });
        #[cfg(feature = "playback")]
        playout::reset(self);
        let i2 = self.clone();
        self.handle.spawn(async move {
            let _ = i2.sync_log(st).await;
        });
        Ok(())
    }

    /// Lanterns keep a whole copy of every station they carry.
    async fn mirror(self: &Arc<Self>) {
        let mut stations: Vec<StationId> = if self.cfg.carry_all {
            self.log.lock().unwrap().keys().copied().collect()
        } else {
            self.store.lock().unwrap().own()
        };
        stations.extend(self.special_ids());
        let refused = self.refused();
        for st in stations {
            if refused.contains(&st) {
                continue;
            }
            let _ = self.sync_log(st).await;
            let missing: Vec<Hash> = {
                let s = self.store.lock().unwrap();
                s.chunks_of(&st).into_iter().filter(|h| !s.has_chunk(h)).collect()
            };
            for h in missing.into_iter().take(200) {
                if self.chunk(&st, &h).await.is_err() {
                    break;
                }
            }
            if self.cfg.lantern {
                let pics: Vec<Hash> = {
                    let s = self.store.lock().unwrap();
                    s.logs.get(&st).map(|a| a.art_hashes()).unwrap_or_default().into_iter().filter(|h| !s.has_chunk(h)).collect()
                };
                for h in pics.into_iter().take(50) {
                    let _ = self.fetch(&st, &h, true).await;
                }
            }
        }
    }
}

impl Inner {
    /// Provisions for the voyage: on Wi-Fi, the next six hours of up to three kept stations
    /// come aboard ahead of time, sealed in the hold, so they play through a flight or a tunnel.
    async fn provision(self: &Arc<Self>) {
        if self.cfg.dir.join("provisions-off").exists() {
            return;
        }
        let kept: Vec<StationId> = self.chart().0.iter().filter_map(|h| StationId::from_hex(h).ok()).take(3).collect();
        let now = now_ms();
        let mut budget = 200usize;
        for st in kept {
            let _ = self.sync_log(st).await;
            let want: Vec<Hash> = {
                let s = self.store.lock().unwrap();
                let Some(a) = s.logs.get(&st) else { continue };
                let Ok(clock) = pyrate_core::clock::Clock::new(a) else { continue };
                let mut want = Vec::new();
                for (start, track, _) in clock.upcoming(now, now + 6 * 3_600_000) {
                    let Some(Body::Track { chunks, .. }) = a.entries().iter().find(|e| e.hash().ok() == Some(track)).map(|e| e.entry.body()) else { continue };
                    let mut t = start;
                    for c in chunks {
                        let end = t + c.duration_ms();
                        if end > now && !s.has_chunk(&c.hash) {
                            want.push(c.hash);
                        }
                        t = end;
                    }
                }
                want.dedup();
                want
            };
            for h in want {
                if budget == 0 {
                    return;
                }
                budget -= 1;
                if self.fetch(&st, &h, true).await.is_err() {
                    break;
                }
            }
        }
    }

    /// An hourly note of how many are aboard each of our own stations, for the owner's chart.
    fn log_aboard(&self) {
        let own = self.store.lock().unwrap().own();
        let now = now_ms();
        for st in own {
            let aboard = self.log.lock().unwrap().get(&st).map(|l| l.aboard(now)).unwrap_or(0);
            let p = self.cfg.dir.join(format!("aboard-{}", store::filed_as(&st)));
            let mut text = std::fs::read_to_string(&p).unwrap_or_default();
            text.push_str(&format!("{now} {aboard}\n"));
            let lines: Vec<&str> = text.lines().collect();
            let keep = lines[lines.len().saturating_sub(24 * 60)..].join("\n");
            let _ = std::fs::write(&p, keep + "\n");
        }
    }

    /// Ingests a file and adds it to one of our stations' libraries.
    pub fn add_track(&self, station: StationId, path: &std::path::Path, title: &str, by: &str, source: &str, profile: Profile) -> Result<Hash> {
        let got = pyrate_core::ingest::ingest(path, profile)?;
        let mut s = self.store.lock().unwrap();
        for (c, bytes) in &got.chunks {
            s.put_chunk(&c.hash, bytes)?;
        }
        let e = s.write(station, now_ms(), Body::Track {
            title: title.into(),
            by: by.into(),
            source: source.into(),
            channels: got.channels,
            bitrate: got.bitrate,
            chunks: got.chunks.iter().map(|(c, _)| *c).collect(),
        })?;
        let track = e.hash()?;
        // The album cover inside the file, or a cover picture beside it in its folder.
        let picture = got.picture.or_else(|| art::folder_cover(path));
        if let Some(p) = picture.and_then(|raw| pyrate_core::art::prepare(&raw, pyrate_core::art::Kind::Track).ok()) {
            let hash = *blake3::hash(&p.jpeg).as_bytes();
            s.put_chunk(&hash, &p.jpeg)?;
            s.write(station, now_ms(), Body::TrackArt { track, art: pyrate_core::ArtRef { hash, w: p.w, h: p.h } })?;
        }
        Ok(track)
    }

}

impl Inner {
    /// The Log as the app and the web listener show it: fresh stations, most aboard first.
    /// Kept and marooned stations: the owner's own chart, on this device only.
    fn chart(&self) -> (std::collections::HashSet<String>, std::collections::HashSet<String>) {
        let v: Value = std::fs::read(self.cfg.dir.join("chart.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or(json!({}));
        let set = |k: &str| v[k].as_array().into_iter().flatten().filter_map(|x| x.as_str().map(String::from)).collect();
        (set("kept"), set("marooned"))
    }

    fn set_chart(&self, id: &str, kind: &str, on: bool) -> Result<()> {
        let (mut kept, mut marooned) = self.chart();
        let target = if kind == "kept" { &mut kept } else { &mut marooned };
        if on { target.insert(id.to_string()); } else { target.remove(id); }
        if kind == "marooned" && on { kept.remove(id); }
        if kind == "kept" && on { marooned.remove(id); }
        std::fs::write(self.cfg.dir.join("chart.json"), json!({ "kept": kept, "marooned": marooned }).to_string())?;
        Ok(())
    }

    pub fn stations(&self) -> Value {
        self.stations_with(false)
    }

    /// Kept stations first, then by how many are aboard. Marooned stations are left out
    /// unless asked for.
    pub fn stations_with(&self, marooned_too: bool) -> Value {
        let (kept, marooned) = self.chart();
        let refused = self.refused();
        let now = now_ms();
        let kept_art_only = self.cfg.dir.join("art-kept-only").exists();
        // Explicit stations stay out of the Log and off the dial until the listener asks for them.
        let prefs = std::fs::read(self.cfg.dir.join("prefs.json")).ok().and_then(|b| serde_json::from_slice::<Value>(&b).ok()).unwrap_or(Value::Null);
        let show_explicit = prefs["show_explicit"] == json!(true);
        let hide_mature = prefs["hide_mature"] == json!(true);
        let own: std::collections::HashSet<StationId> = self.store.lock().unwrap().own().into_iter().collect();
        let banners: HashMap<StationId, String> = {
            let s = self.store.lock().unwrap();
            s.logs.iter().filter_map(|(id, a)| Some((*id, hex(&a.art()?.1?.hash)))).collect()
        };
        let log = self.log.lock().unwrap();
        let mut v: Vec<(u64, Value)> = log
            .iter()
            .filter(|(_, l)| l.fresh(now))
            .filter_map(|(id, l)| {
                let a = l.announce.as_ref()?;
                let aboard = l.aboard(now);
                let hex = id.to_hex();
                let is_marooned = marooned.contains(&hex);
                if (refused.contains(id) && !is_marooned) || self.special(id) {
                    return None;
                }
                if is_marooned && !marooned_too {
                    return None;
                }
                if !own.contains(id) && ((a.explicit && !show_explicit) || (hide_mature && (a.mature || a.explicit))) {
                    return None;
                }
                let is_kept = kept.contains(&hex);
                let show = !is_marooned && (is_kept || !kept_art_only);
                let accent = a.accent.map(|c| format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2]));
                Some((aboard + if is_kept { 1 << 40 } else { 0 }, json!({
                    "kinds": a.kinds, "mature": a.mature, "explicit": a.explicit, "tip": a.tip,
                    "cover": a.cover.filter(|_| show).map(|h| crate::hex(&h)),
                    "banner": banners.get(id).filter(|_| show),
                    "accent": accent,
                    "id": hex, "call_sign": id.call_sign(), "name": a.name, "about": a.about,
                    "port": a.port, "live": a.live, "aboard": aboard, "kept": is_kept, "marooned": is_marooned, "since": a.since,
                    "carriers": l.carriers.values().filter(|(_, s)| now.saturating_sub(*s) < 10 * 60_000).count(),
                    "pocket": self.is_pocket(id),
                })))
            })
            .collect();
        v.sort_by(|a, b| b.0.cmp(&a.0).then(a.1["name"].as_str().cmp(&b.1["name"].as_str())));
        json!(v.into_iter().map(|x| x.1).collect::<Vec<_>>())
    }

    /// Whether this node takes a stranger's station aboard for the fleet. Never one its owner
    /// marooned; and on a phone, never an explicit station while the owner keeps them hidden,
    /// nor a mature one while they hide those. Lanterns carry the whole Log unless their keeper
    /// maroons. `entries` are a stem's cargo, read for the station's word on itself when no
    /// card has said it.
    pub(crate) fn will_carry(&self, st: &StationId, cargo: Option<(&Option<Announce>, &[Vec<u8>])>) -> bool {
        if self.refused().contains(st) {
            return false;
        }
        if self.cfg.lantern || self.is_own(st) {
            return true;
        }
        let prefs = std::fs::read(self.cfg.dir.join("prefs.json")).ok().and_then(|b| serde_json::from_slice::<Value>(&b).ok()).unwrap_or(Value::Null);
        let (show_explicit, hide_mature) = (prefs["show_explicit"] == json!(true), prefs["hide_mature"] == json!(true));
        if show_explicit && !hide_mature {
            return true;
        }
        // The station's own word: its card, else its log here, else the cargo itself.
        let card = cargo.and_then(|(a, _)| a.clone()).or_else(|| self.log.lock().unwrap().get(st).and_then(|l| l.announce.clone()));
        let (explicit, mature) = match card {
            Some(c) => (c.explicit, c.mature),
            None => {
                let mut a = self.store.lock().unwrap().logs.get(st).cloned().unwrap_or_else(|| Archive::new(*st));
                for b in cargo.map(|(_, e)| e).unwrap_or_default() {
                    if let Ok(e) = SignedEntry::from_bytes(b) {
                        if e.entry.seq >= a.len() {
                            let _ = a.append(e);
                        }
                    }
                }
                (a.explicit(), a.details().is_some_and(|d| d.1))
            }
        };
        !((explicit && !show_explicit) || (hide_mature && (mature || explicit)))
    }

    /// Counts a token once, if its work is real and its hour current. Returns whether it was new.
    pub fn accept_token(&self, station: StationId, hour: u64, token: [u8; 32], nonce: u64, vouch: bool) -> bool {
        let now_hour = now_ms() / 3_600_000;
        if hour + 1 < now_hour || hour > now_hour + 1 || !net::token_ok(&station, hour, &token, nonce) {
            return false;
        }
        let mut log = self.log.lock().unwrap();
        let l = log.entry(station).or_default();
        l.heard.retain(|h, _| h + 2 >= now_hour);
        l.sketches.retain(|h, _| h + 2 >= now_hour);
        if !l.heard.entry(hour).or_default().insert(token) {
            return false;
        }
        l.sketches.entry(hour).or_default().add(&token);
        if vouch {
            l.outbox.push((token, nonce, hour));
        }
        true
    }

    /// Counts a listener who has no node of their own, such as a browser tab.
    pub fn count_guest(self: &Arc<Self>, station: &StationId, guest: &[u8; 32]) {
        let hour = now_ms() / 3_600_000;
        let mut h = blake3::Hasher::new_keyed(&self.seed);
        h.update(guest);
        h.update(&hour.to_le_bytes());
        let token = *h.finalize().as_bytes();
        let (me, st) = (self.clone(), *station);
        // One guest at a time does the work, so a flood of tabs cannot flood the CPU.
        static WORK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        std::thread::spawn(move || {
            let _g = WORK.lock().unwrap();
            let nonce = net::token_find(&st, hour, &token);
            me.accept_token(st, hour, token, nonce, true);
        });
    }

    pub fn archive(&self, id: &StationId) -> Option<Archive> {
        self.store.lock().unwrap().logs.get(id).cloned()
    }

    /// ON AIR. Samples from `source` become six-second Talk chunks and Live entries
    /// until the broadcast is ended. `source` is the microphone, or a file played in real time.
    pub fn go_live(self: &Arc<Self>, station: StationId, file: Option<PathBuf>) -> Result<()> {
        if self.live.lock().unwrap().is_some() {
            anyhow::bail!("already on air");
        }
        if !self.store.lock().unwrap().keys.contains_key(&station) {
            anyhow::bail!("not our station");
        }
        let started = now_ms();
        *self.live.lock().unwrap() = Some((station, started));
        let buf: Arc<Mutex<Vec<i16>>> = Arc::new(Mutex::new(Vec::new()));
        let me = self.clone();
        let b2 = buf.clone();
        std::thread::spawn(move || {
            // Hold the source open for as long as the broadcast lasts.
            #[cfg(feature = "playback")]
            let _mic = if file.is_none() {
                match player::open_input(b2.clone()) {
                    Ok(i) => Some(i),
                    Err(e) => {
                        say(&format!("no microphone: {e}"));
                        *me.live.lock().unwrap() = None;
                        return;
                    }
                }
            } else {
                None
            };
            let file_pcm: Option<Vec<i16>> = file.and_then(|p| {
                let got = pyrate_core::ingest::ingest(&p, Profile::Talk).ok()?;
                let mut d = pyrate_core::audio::ChunkDecoder::new(1).ok()?;
                Some(got.chunks.iter().flat_map(|(_, b)| d.decode(b).unwrap_or_default()).collect())
            });
            let Ok(mut enc) = pyrate_core::ChunkEncoder::new(Profile::Talk) else { return };
            let mut fed = 0usize;
            say(&format!("ON AIR {}", station.call_sign()));
            while me.live.lock().unwrap().is_some_and(|(s, t)| s == station && t == started) {
                std::thread::sleep(Duration::from_millis(200));
                let pcm: Vec<i16> = match &file_pcm {
                    Some(f) => {
                        let due = (now_ms().saturating_sub(started) * 48) as usize;
                        let end = due.min(f.len());
                        let out = f[fed.min(end)..end].to_vec();
                        fed = end;
                        if fed >= f.len() {
                            *me.live.lock().unwrap() = None;
                        }
                        out
                    }
                    None => std::mem::take(&mut *b2.lock().unwrap()),
                };
                let Ok(chunks) = enc.push(&pcm) else { break };
                for (c, bytes) in chunks {
                    let mut s = me.store.lock().unwrap();
                    if s.put_chunk(&c.hash, &bytes).is_ok() {
                        let _ = s.write(station, now_ms(), Body::Live { chunk: c, channels: 1, bitrate: 24_000 });
                    }
                    drop(s);
                    // A pocket station never goes down the public stem: its members fetch from
                    // the keeper, who tells them it is on the air on the pocket's own topic.
                    if me.cfg.private && !me.is_pocket(&station) {
                        let m2 = me.clone();
                        me.handle.spawn(async move { m2.stem_push(station, None).await });
                    }
                }
            }
            say(&format!("off the air {}", station.call_sign()));
            let _ = buf;
        });
        Ok(())
    }
}

impl Inner {
    /// Ballast: every node carries the stations nearest its own id by hash, up to what it
    /// donated, whether or not anyone listens to them. No coordination: each node ranks the
    /// Log for itself, and the spread falls out of the hashing.
    async fn ballast(self: &Arc<Self>) {
        const PER_STATION: u64 = 300_000_000;
        let me = blake3::derive_key("pyrate/ballast-seat/v1", &self.seed);
        let mut stations: Vec<StationId> = self.log.lock().unwrap().iter().filter(|(s, l)| l.announce.is_some() && !self.is_pocket(s)).map(|(s, _)| *s).collect();
        stations.sort_by_key(|s| {
            let mut h = blake3::Hasher::new();
            h.update(b"pyrate/ballast/v1");
            h.update(&me);
            h.update(&s.0);
            *h.finalize().as_bytes()
        });
        let mut budget = self.ballast_bytes.load(std::sync::atomic::Ordering::Relaxed);
        let mut pinned = std::collections::HashSet::new();
        for st in stations {
            if budget < 10_000_000 || self.is_own(&st) || !self.will_carry(&st, None) {
                continue;
            }
            let _ = self.sync_log_direct(st).await;
            let wanted: Vec<(Hash, u64)> = {
                let s = self.store.lock().unwrap();
                let Some(a) = s.logs.get(&st) else { continue };
                let mut v = Vec::new();
                let mut size = 0;
                for (_, e) in a.library().unwrap_or_default() {
                    if let Body::Track { chunks, .. } = e.entry.body() {
                        for c in chunks {
                            if size + c.len as u64 > PER_STATION.min(budget) {
                                break;
                            }
                            size += c.len as u64;
                            v.push((c.hash, c.len as u64));
                        }
                    }
                }
                v
            };
            for (h, len) in &wanted {
                budget = budget.saturating_sub(*len);
                if self.store.lock().unwrap().has_chunk(h) {
                    continue;
                }
                if self.chunk_direct(&st, h, true).await.is_err() || !self.ballast_ok.load(std::sync::atomic::Ordering::Relaxed) {
                    break;
                }
            }
            pinned.insert(st);
        }
        self.store.lock().unwrap().pinned = pinned;
    }
}

impl Node {
    pub fn start(cfg: Config) -> Result<Self> {
        let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build()?;
        let store = Store::open(&cfg.dir, cfg.quota + cfg.ballast)?;
        let key_path = cfg.dir.join("node.key");
        // A seed of its own, kept: what this node carries by lot, its voices in chatter and its
        // listener tokens come from here, so they hold steady while its address changes.
        let seed_path = cfg.dir.join("seed.key");
        let seed: [u8; 32] = match std::fs::read(&seed_path).ok().and_then(|b| <[u8; 32]>::try_from(b.as_slice()).ok()) {
            Some(s) => s,
            None => {
                // Drawn from the old node key where there is one, so an upgrade changes nothing.
                let base = std::fs::read(&key_path).ok().filter(|b| b.len() == 32).unwrap_or_else(|| {
                    let mut b = vec![0u8; 32];
                    let _ = getrandom(&mut b);
                    b
                });
                let s = blake3::derive_key("pyrate/seed/v1", &base);
                std::fs::write(&seed_path, s)?;
                s
            }
        };
        // A phone's address on the network is new every day, so no one can follow one phone
        // from day to day by its id. Lanterns keep theirs: others start from them.
        if !cfg.lantern {
            let stale = std::fs::metadata(&key_path).and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|age| age > Duration::from_secs(24 * 3600));
            if stale {
                let _ = std::fs::remove_file(&key_path);
            }
        }
        let secret = match std::fs::read(&key_path).ok().and_then(|b| <[u8; 32]>::try_from(b.as_slice()).ok()) {
            Some(b) => SecretKey::from_bytes(&b),
            None => {
                let mut b = [0u8; 32];
                getrandom(&mut b)?;
                std::fs::write(&key_path, b)?;
                SecretKey::from_bytes(&b)
            }
        };
        let secret_bytes = secret.to_bytes();
        // Every peer met before is a starting point next time, so no single Lantern is the door.
        let mut cfg = cfg;
        if let Ok(t) = std::fs::read_to_string(cfg.dir.join("peers")) {
            for p in t.lines().filter_map(|l| l.trim().parse::<EndpointId>().ok()).take(24) {
                if !cfg.bootstrap.contains(&p) {
                    cfg.bootstrap.push(p);
                }
            }
        }
        let proto = Proto(Arc::new(Mutex::new(None)));
        let public_dns = cfg.public_dns;
        let (endpoint, router, gossip) = rt.block_on(async {
            let relays = iroh::defaults::prod::default_relay_map();
            for r in CREW_RELAYS {
                if let Ok(url) = r.parse::<iroh::RelayUrl>() {
                    relays.insert(url.clone(), Arc::new(iroh::RelayConfig::from(url)));
                }
            }
            // PYRATE_RELAYS, a list of relay URLs, replaces these: for a private network.
            let custom: Vec<iroh::RelayUrl> = std::env::var("PYRATE_RELAYS").unwrap_or_default().split(',').filter_map(|r| r.trim().parse().ok()).collect();
            let private_net = !custom.is_empty();
            let relays = if custom.is_empty() { relays } else { iroh::RelayMap::from_iter(custom) };
            let dark = std::env::var_os("PYRATE_DARK").is_some_and(|v| v != "0");
            // A dark node resolves others but never publishes its own address: no signed pkarr or
            // DNS record ties this node id to a relay or an IP. It stays reachable through relays
            // and through the addresses shared inside each station's gossip.
            let mut b = if dark {
                Endpoint::builder(presets::Minimal)
                    .secret_key(secret)
                    .address_lookup(iroh::address_lookup::PkarrResolver::n0_dns())
                    .address_lookup(iroh::address_lookup::DnsAddressLookup::n0_dns())
                    .relay_mode(iroh::RelayMode::Custom(relays))
            } else {
                Endpoint::builder(presets::N0).secret_key(secret).relay_mode(iroh::RelayMode::Custom(relays))
            };
            if public_dns {
                b = b.dns_resolver(
                    iroh::dns::DnsResolver::builder()
                        .with_nameserver("1.1.1.1:53".parse()?, iroh::dns::DnsProtocol::Udp)
                        .with_nameserver("9.9.9.9:53".parse()?, iroh::dns::DnsProtocol::Udp)
                        .build(),
                );
            }
            // Dark running (PYRATE_DARK=1): no direct paths at all, so no peer ever learns this
            // node's address, and every relay connection goes through PYRATE_PROXY, such as
            // Tor's HTTP tunnel port. Relays then see only Tor.
            if dark {
                b = b.clear_ip_transports();
                say("dark running: relays only, no address published");
            }
            if let Some(url) = std::env::var("PYRATE_PROXY").ok().and_then(|u| u.parse().ok()) {
                b = b.proxy_url(url).ca_tls_config(iroh::tls::CaTlsConfig::system());
            } else if std::env::var_os("HTTPS_PROXY").is_some() && !private_net {
                b = b.proxy_from_env().ca_tls_config(iroh::tls::CaTlsConfig::system());
            }
            let ep = b.bind().await.map_err(|e| anyhow!("bind: {e}"))?;
            // Room for a hail: twenty seconds of a voice, sealed.
            let gossip = GossipNet::builder().max_message_size(128 * 1024).spawn(ep.clone());
            let router = Router::builder(ep.clone()).accept(ALPN, proto.clone()).accept(GOSSIP_ALPN, gossip.clone()).spawn();
            anyhow::Ok((ep, router, gossip))
        })?;

        #[cfg(feature = "playback")]
        let queue: player::SharedQueue = Arc::new(Mutex::new(player::Queue { volume: 1.0, ..Default::default() }));
        let inner = Arc::new(Inner {
            cfg: cfg.clone(),
            endpoint: endpoint.clone(),
            secret: secret_bytes,
            seed,
            store: Mutex::new(store),
            log: Mutex::new(HashMap::new()),
            conns: Mutex::new(HashMap::new()),
            gossip_tx: Mutex::new(None),
            gossip: gossip.clone(),
            chats: Mutex::new(HashMap::new()),
            flock_host: Mutex::new(None),
            flock_with: Mutex::new(None),
            flock_offset: std::sync::atomic::AtomicI64::new(0),
            flock_trim: std::sync::atomic::AtomicI64::new(0),
            flock_rtt: std::sync::atomic::AtomicU64::new(0),
            pockets: Mutex::new(HashMap::new()),
            tuned: Mutex::new(None),
            jobs: Mutex::new(HashMap::new()),
            pow: Mutex::new(HashMap::new()),
            #[cfg(feature = "playback")]
            queue: queue.clone(),
            #[cfg(feature = "playback")]
            np: Mutex::new(Value::Null),
            art_mem: Mutex::new(art::Shelf::default()),
            live: Mutex::new(None),
            handle: rt.handle().clone(),
            ballast_ok: std::sync::atomic::AtomicBool::new(false),
            ballast_bytes: std::sync::atomic::AtomicU64::new(cfg.ballast),
            tokens_sent: Mutex::new(HashMap::new()),
            direct: Mutex::new(std::collections::HashSet::new()),
            refused_cache: Mutex::new((0, std::collections::HashSet::new())),
            heard: Mutex::new(std::collections::VecDeque::new()),
            strict: std::sync::atomic::AtomicBool::new(cfg.dir.join("strict").exists()),
            stem_misses: Mutex::new(HashMap::new()),
            stem_seen: Mutex::new(std::collections::VecDeque::new()),
            relay_ok: std::sync::atomic::AtomicBool::new(!cfg.private),
        });
        *proto.0.lock().unwrap() = Some(inner.clone());
        {
            let i2 = inner.clone();
            rt.spawn(async move { i2.open_pockets().await });
        }

        // The Log.
        let topic = TopicId::from_bytes(net::log_topic());
        let i2 = inner.clone();
        rt.spawn(async move {
            say(&format!("node {} up, joining the Log through {} bootstrap peers", i2.endpoint.id(), i2.cfg.bootstrap.len()));
            let sub = match gossip.subscribe(topic, i2.cfg.bootstrap.clone()).await {
                Ok(s) => s,
                Err(e) => {
                    say(&format!("gossip subscribe failed: {e}"));
                    return;
                }
            };
            let (tx, mut rx) = sub.split();
            *i2.gossip_tx.lock().unwrap() = Some(tx.clone());
            let boot = i2.cfg.bootstrap.clone();
            let i3 = i2.clone();
            tokio::spawn(async move {
                loop {
                    tokio::time::sleep(Duration::from_secs(60)).await;
                    let mut peers = boot.clone();
                    let carriers: Vec<[u8; 32]> = i3.log.lock().unwrap().values().flat_map(|l| l.carriers.keys().copied()).collect();
                    peers.extend(carriers.iter().filter_map(|n| EndpointId::from_bytes(n).ok()));
                    peers.retain(|p| *p != i3.endpoint.id());
                    peers.sort();
                    peers.dedup();
                    peers.truncate(16);
                    let _ = tx.join_peers(peers).await;
                }
            });
            while let Some(ev) = rx.next().await {
                let msg = match ev {
                    Ok(Event::Received(m)) => m,
                    Ok(Event::NeighborUp(n)) => {
                        say(&format!("neighbour up {}", n.fmt_short()));
                        continue;
                    }
                    Ok(Event::NeighborDown(n)) => {
                        say(&format!("neighbour down {}", n.fmt_short()));
                        continue;
                    }
                    _ => continue,
                };
                let Ok(g) = postcard::from_bytes::<Gossip>(&msg.content) else { continue };
                if let Some(st) = i2.on_gossip(g) {
                    if i2.wants(&st) {
                        let i4 = i2.clone();
                        tokio::spawn(async move {
                            let _ = i4.sync_log(st).await;
                        });
                    }
                }
            }
        });

        let i2 = inner.clone();
        rt.spawn(async move {
            let mut n = 0u64;
            loop {
                i2.tick().await;
                if n % 6 == 2 {
                    for st in i2.special_ids() {
                        let _ = i2.sync_log(st).await;
                    }
                    i2.drop_refused();
                    if i2.cfg.lantern {
                        i2.lantern_duties().await;
                    }
                }
                // Pockets this node was invited to: their logs, from their members, each minute.
                if n % 6 == 4 {
                    let pockets: Vec<(StationId, [u8; 32])> = i2.pockets.lock().unwrap().iter().map(|(st, p)| (*st, p.key)).collect();
                    for (st, k) in pockets {
                        if !i2.is_own(&st) {
                            let _ = i2.pocket_sync(st, k).await;
                        }
                    }
                }
                if i2.cfg.lantern && n % 3 == 1 {
                    i2.mirror().await;
                } else if !i2.cfg.lantern && n % 6 == 3 && i2.ballast_ok.load(std::sync::atomic::Ordering::Relaxed) {
                    i2.ballast().await;
                }
                if !i2.cfg.lantern && n % 60 == 5 && i2.relay_ok.load(std::sync::atomic::Ordering::Relaxed) {
                    i2.provision().await;
                }
                if n % 360 == 7 {
                    i2.log_aboard();
                }
                if n % 30 == 0 {
                    {
                        let s = i2.store.lock().unwrap();
                        let _ = s.enforce_quota();
                    }
                    let peers: Vec<String> = i2.known_nodes_pub().into_iter().take(24).map(|p| p.to_string()).collect();
                    let _ = std::fs::write(i2.cfg.dir.join("peers"), peers.join("\n"));
                }
                n += 1;
                tokio::time::sleep(Duration::from_secs(10)).await;
            }
        });

        #[cfg(feature = "playback")]
        {
            let i2 = inner.clone();
            rt.spawn(async move { playout::run(i2).await });
            // The output stream lives on its own thread for the life of the process.
            std::thread::spawn(move || match player::open(queue) {
                Ok(out) => loop {
                    std::thread::park();
                    let _ = &out;
                },
                Err(e) => say(&format!("no audio output: {e}")),
            });
        }

        Ok(Self { rt, inner, _router: router })
    }

    pub fn id(&self) -> EndpointId {
        self.inner.endpoint.id()
    }

    /// Raises colours with the master key in hand: it flies them, appoints this
    /// device's deputy, and is then forgotten. Returns the station and the master's
    /// 24 words, shown once. A Lantern (`keep_master`) keeps the master itself.
    pub fn raise_with(&self, name: &str, about: &str, port: Option<String>, keep_master: bool) -> Result<(StationId, String)> {
        let master = pyrate_core::StationKey::generate()?;
        let id = master.id();
        let words = master.phrase();
        {
            let mut s = self.inner.store.lock().unwrap();
            let mut a = s.logs.remove(&id).unwrap_or_else(|| Archive::new(id));
            let now = now_ms();
            a.write(&master, now, Body::Colours { name: name.into(), about: about.into(), port, tip: None })?;
            if keep_master {
                s.put_entries(id, a.entries().to_vec())?;
                s.add_key_for(id, master)?;
            } else {
                let deputy = pyrate_core::StationKey::generate()?;
                a.write(&master, now, Body::Deputy { key: deputy.id().0, label: "phone".into() })?;
                s.put_entries(id, a.entries().to_vec())?;
                s.add_key_for(id, deputy)?;
            }
        }
        self.inner.pow_for(&id);
        Ok((id, words))
    }

    pub fn raise(&self, name: &str, about: &str, port: Option<String>) -> Result<StationId> {
        Ok(self.raise_with(name, about, port, true)?.0)
    }

    /// Master acts from the 24 words: strike, or revoke every deputy and appoint this device.
    /// Appoints a co-host's key as a deputy, with the station's 24 words.
    fn appoint(&self, station: StationId, words: &str, key: Hash, label: &str) -> Result<()> {
        let master = pyrate_core::StationKey::from_phrase(words)?;
        if master.id() != station {
            anyhow::bail!("those words are not this station's key");
        }
        let mut s = self.inner.store.lock().unwrap();
        let mut a = s.logs.get(&station).cloned().unwrap_or_else(|| Archive::new(station));
        let before = a.len();
        let now = now_ms().max(a.entries().last().map(|e| e.entry.at_ms + 1).unwrap_or(0));
        a.write(&master, now, Body::Deputy { key, label: label.chars().take(40).collect() })?;
        s.put_entries(station, a.entries()[before as usize..].to_vec())?;
        Ok(())
    }

    fn with_master(&self, station: StationId, words: &str, act: &str) -> Result<()> {
        let master = pyrate_core::StationKey::from_phrase(words)?;
        if master.id() != station {
            anyhow::bail!("those words are not this station's key");
        }
        let mut s = self.inner.store.lock().unwrap();
        let mut a = s.logs.get(&station).cloned().unwrap_or_else(|| Archive::new(station));
        let before = a.len();
        let now = now_ms().max(a.entries().last().map(|e| e.entry.at_ms).unwrap_or(0));
        match act {
            "strike" => {
                a.write(&master, now, Body::Strike)?;
            }
            _ => {
                for d in a.deputies() {
                    a.write(&master, now, Body::Revoke { key: d })?;
                }
                let deputy = pyrate_core::StationKey::generate()?;
                a.write(&master, now, Body::Deputy { key: deputy.id().0, label: "phone".into() })?;
                s.add_key_for(station, deputy)?;
            }
        }
        s.put_entries(station, a.entries()[before as usize..].to_vec())?;
        Ok(())
    }

    /// Ingests a file and adds it to one of our stations' libraries.
    pub fn add_track(&self, station: StationId, path: &std::path::Path, title: &str, by: &str, source: &str, profile: Profile) -> Result<Hash> {
        self.inner.add_track(station, path, title, by, source, profile)
    }

    pub fn archive(&self, id: &StationId) -> Option<Archive> {
        self.inner.store.lock().unwrap().logs.get(id).cloned()
    }

    /// One of our own stations, by name.
    pub fn inner_store_find(&self, name: &str) -> Option<StationId> {
        let s = self.inner.store.lock().unwrap();
        s.own().into_iter().find(|id| {
            s.logs.get(id).and_then(|a| a.colours()).is_some_and(|c| matches!(c, Body::Colours { name: n, .. } if n == name))
        })
    }

    pub fn titles(&self, station: &StationId) -> Vec<String> {
        let s = self.inner.store.lock().unwrap();
        s.logs.get(station).map(|a| a.entries().iter().filter_map(|e| match e.entry.body() {
            Body::Track { title, .. } => Some(title.clone()),
            _ => None,
        }).collect()).unwrap_or_default()
    }

    /// Whether this node would take the station aboard for the fleet, by its owner's choices.
    pub fn will_carry(&self, id: &pyrate_core::StationId) -> bool {
        self.inner.will_carry(id, None)
    }

    /// Whether this node has heard of a station at all: a card, a carrier, a listener token or
    /// a log. The pocket test uses it to prove an outsider never learns a pocket exists.
    pub fn heard_of(&self, id: &pyrate_core::StationId) -> bool {
        self.inner.log.lock().unwrap().contains_key(id) || self.inner.store.lock().unwrap().logs.contains_key(id)
    }

    /// Whether a chunk sits in this device's hold (tests use it to prove art stayed in memory).
    pub fn on_disk(&self, hash: &str) -> bool {
        unhex(hash).is_ok_and(|h| self.inner.store.lock().unwrap().has_chunk(&h))
    }

    /// A picture decoded to RGBA, if this node has it. Width, height, pixels.
    pub fn art_pixels(&self, hash: &str) -> Option<(u32, u32, Vec<u8>)> {
        self.inner.art_pixels(&unhex(hash).ok()?)
    }

    /// The JSON face the app talks to.
    pub fn call(&self, req: &Value) -> Value {
        match self.call_inner(req) {
            Ok(v) => v,
            Err(e) => json!({ "error": e.to_string() }),
        }
    }

    fn call_inner(&self, req: &Value) -> Result<Value> {
        let cmd = req["cmd"].as_str().context("cmd")?;
        let station = || -> Result<StationId> { Ok(StationId::from_hex(req["station"].as_str().context("station")?)?) };
        let inner = &self.inner;
        match cmd {
            "flock_host" | "flock_join" | "flock_leave" | "flock_trim" | "flock_status" => inner.flock_call(cmd, req),
            "pocket_invite" | "pocket_join" | "pocket_leave" => inner.pocket_call(cmd, req),
            "chat_join" | "chat_leave" | "chat_read" | "chat_send" | "chat_hail_open" => {
                let st = station()?;
                inner.chat_call(cmd, st, req)
            }
            "page" | "programme" | "programme_set" | "show_add" | "show_cancel" | "notice_post" | "notice_remove" => {
                let st = station()?;
                if cmd == "page" && inner.store.lock().unwrap().logs.get(&st).is_none_or(|a| a.len() == 0) {
                    let _ = self.rt.block_on(async { tokio::time::timeout(Duration::from_secs(8), inner.sync_log(st)).await });
                }
                inner.page_call(cmd, st, req)
            }
            "status" => {
                let mine = {
                let s = inner.store.lock().unwrap();
                let mine_v = s.own().into_iter().find(|id| !s.logs.get(id).is_some_and(|a| a.is_struck())).map(|id| {
                    let a = s.logs.get(&id);
                    let name = a.and_then(|a| a.colours()).map(|c| match c {
                        Body::Colours { name, .. } => name.clone(),
                        _ => String::new(),
                    });
                    let tracks = a.map(|a| a.library().map(|l| l.len()).unwrap_or(0)).unwrap_or(0);
                    let art = a.and_then(|a| a.art());
                    json!({ "id": id.to_hex(), "call_sign": id.call_sign(), "name": name, "tracks": tracks,
                        "direct": inner.direct.lock().unwrap().contains(&id),
                        "cover": art.and_then(|x| x.0).map(|r| hex(&r.hash)),
                        "banner": art.and_then(|x| x.1).map(|r| hex(&r.hash)),
                        "accent": art.map(|x| format!("#{:02x}{:02x}{:02x}", x.2[0], x.2[1], x.2[2])) })
                });
                mine_v
                };
                let jobs: Vec<Value> = inner.jobs.lock().unwrap().iter().map(|(k, v)| json!({ "id": k, "state": v })).collect();
                let tuned = inner.tuned.lock().unwrap().as_ref().map(|t| t.station.to_hex());
                #[cfg(feature = "playback")]
                let np = inner.np.lock().unwrap().clone();
                #[cfg(not(feature = "playback"))]
                let np = Value::Null;
                let live = inner.live.lock().unwrap().map(|(s, t)| json!({ "station": s.to_hex(), "since_ms": now_ms().saturating_sub(t) }));
                Ok(json!({ "node": self.id().to_string(), "mine": mine, "jobs": jobs, "tuned": tuned, "now": np, "live": live }))
            }
            "stations" => Ok(inner.stations_with(req["marooned"].as_bool().unwrap_or(false))),
            "keep" | "maroon" => {
                let st = station()?;
                let kind = if cmd == "keep" { "kept" } else { "marooned" };
                inner.set_chart(&st.to_hex(), kind, req["value"].as_bool().unwrap_or(true))?;
                inner.refused_changed();
                inner.drop_refused();
                Ok(json!({ "ok": true }))
            }
            "details_set" => {
                let st = station()?;
                let kinds: Vec<String> = req["kinds"].as_array().into_iter().flatten().filter_map(|k| k.as_str().map(|s| s.to_lowercase())).take(5).collect();
                let mut s = inner.store.lock().unwrap();
                s.write(st, now_ms(), Body::Details { kinds, mature: req["mature"].as_bool().unwrap_or(false) })?;
                if let Some(explicit) = req["explicit"].as_bool() {
                    if s.logs.get(&st).map(|a| a.explicit()) != Some(explicit) {
                        s.write(st, now_ms(), Body::Rating { explicit })?;
                    }
                }
                Ok(json!({ "ok": true }))
            }
            "colours_set" => {
                let st = station()?;
                let mut s = inner.store.lock().unwrap();
                let Some(Body::Colours { name, about, port, tip }) = s.logs.get(&st).and_then(|a| a.colours().cloned()) else { anyhow::bail!("no colours yet") };
                let pick = |k: &str, old: Option<String>| match &req[k] {
                    Value::Null => old,
                    Value::String(v) if v.trim().is_empty() => None,
                    Value::String(v) => Some(v.trim().to_string()),
                    _ => old,
                };
                let name = pick("name", Some(name)).unwrap_or_default();
                if name.is_empty() {
                    anyhow::bail!("a station needs a name");
                }
                let body = Body::Colours { name, about: pick("about", Some(about)).unwrap_or_default(), port: pick("port", port), tip: pick("tip", tip) };
                s.write(st, now_ms(), body)?;
                Ok(json!({ "ok": true }))
            }
            "details" => {
                let st = station()?;
                let s = inner.store.lock().unwrap();
                let a = s.logs.get(&st).context("no log")?;
                let (kinds, mature) = a.details().unwrap_or_default();
                let (port, tip) = match a.colours() {
                    Some(Body::Colours { port, tip, .. }) => (port.clone(), tip.clone()),
                    _ => (None, None),
                };
                Ok(json!({ "kinds": kinds, "mature": mature, "explicit": a.explicit(), "port": port, "tip": tip }))
            }
            "schedule" => {
                let st = station()?;
                if inner.store.lock().unwrap().logs.get(&st).is_none_or(|a| a.len() == 0) {
                    let _ = self.rt.block_on(async { tokio::time::timeout(Duration::from_secs(8), inner.sync_log(st)).await });
                }
                let hours = req["hours"].as_u64().unwrap_or(24).min(168);
                let s = inner.store.lock().unwrap();
                let a = s.logs.get(&st).context("this station's log has not arrived yet")?;
                let clock = pyrate_core::clock::Clock::new(a)?;
                let now = now_ms();
                let bill: Vec<Value> = clock.upcoming(now, now + hours * 3_600_000).into_iter().map(|(start, track, dur)| {
                    let (title, by) = describe_track(a, &track);
                    json!({ "start_ms": start, "duration_ms": dur, "title": title, "by": by, "track": hex(&track), "art": a.track_art(&track).map(|r| hex(&r.hash)) })
                }).collect();
                Ok(json!(bill))
            }
            "now_for" => {
                let s = inner.store.lock().unwrap();
                let now = now_ms();
                let mut out = serde_json::Map::new();
                for id in req["stations"].as_array().into_iter().flatten().filter_map(|v| v.as_str()).take(200) {
                    let Ok(st) = StationId::from_hex(id) else { continue };
                    let Some(a) = s.logs.get(&st) else { continue };
                    let Some(np) = pyrate_core::clock::Clock::new(a).ok().and_then(|c| c.now(now)) else { continue };
                    let (title, by) = describe_track(a, &np.track);
                    out.insert(id.to_string(), json!({ "title": title, "by": by, "slot": np.slot }));
                }
                Ok(Value::Object(out))
            }
            "tune_next" => {
                let list = inner.stations_with(false);
                let ids: Vec<String> = list.as_array().into_iter().flatten().filter_map(|s| s["id"].as_str().map(String::from)).collect();
                if ids.is_empty() {
                    anyhow::bail!("no stations yet");
                }
                let cur = inner.tuned.lock().unwrap().as_ref().map(|t| t.station.to_hex());
                let i = cur.and_then(|c| ids.iter().position(|x| *x == c)).map(|i| i as i64).unwrap_or(-1);
                let step = if req["dir"].as_i64().unwrap_or(1) < 0 { -1 } else { 1 };
                let next = &ids[((i + step).rem_euclid(ids.len() as i64)) as usize];
                return Ok(self.call(&json!({ "cmd": "tune", "station": next })));
            }
            "rota" => {
                let st = station()?;
                let s = inner.store.lock().unwrap();
                let r = s.logs.get(&st).map(|a| a.rota()).unwrap_or(pyrate_core::Rota { blocks: vec![], ident: None, ident_every_min: 60 });
                Ok(json!({
                    "blocks": r.blocks.iter().map(|b| json!({ "start_min": b.start_min, "end_min": b.end_min, "tracks": b.tracks.iter().map(|h| hex(h)).collect::<Vec<_>>(), "shuffle": b.shuffle })).collect::<Vec<_>>(),
                    "ident": r.ident.map(|h| hex(&h)), "ident_every_min": r.ident_every_min,
                }))
            }
            "rota_set" => {
                let st = station()?;
                let mut blocks = Vec::new();
                for b in req["blocks"].as_array().into_iter().flatten() {
                    let start = b["start_min"].as_u64().unwrap_or(0).min(1439) as u16;
                    let end = b["end_min"].as_u64().unwrap_or(0).min(1440) as u16;
                    let tracks = b["tracks"].as_array().into_iter().flatten().filter_map(|h| h.as_str().and_then(|h| unhex(h).ok())).collect();
                    blocks.push(pyrate_core::clock::Block { start_min: start, end_min: end, tracks, shuffle: b["shuffle"].as_bool().unwrap_or(true) });
                }
                let ident = req["ident"].as_str().and_then(|h| unhex(h).ok());
                let every = req["ident_every_min"].as_u64().unwrap_or(60).clamp(5, 1440) as u16;
                inner.store.lock().unwrap().write(st, now_ms(), Body::Rota(pyrate_core::Rota { blocks, ident, ident_every_min: every }))?;
                Ok(json!({ "ok": true }))
            }
            "tape" => {
                let secs = req["seconds"].as_u64().unwrap_or(120).clamp(6, 150);
                let path = PathBuf::from(req["path"].as_str().context("path")?);
                let h = inner.heard.lock().unwrap();
                let Some(last) = h.back() else { anyhow::bail!("nothing has played yet") };
                let (st, ch) = (last.station, last.channels);
                let mut take: Vec<&Heard> = h.iter().rev().take_while(|x| x.station == st && x.channels == ch).take((secs / 6) as usize).collect();
                take.reverse();
                let title = take.last().map(|x| x.title.clone()).unwrap_or_default();
                let name = inner.log.lock().unwrap().get(&st).and_then(|l| l.announce.as_ref().map(|a| a.name.clone())).unwrap_or_else(|| st.call_sign());
                let chunks: Vec<Vec<u8>> = take.iter().map(|x| x.bytes.clone()).collect();
                std::fs::write(&path, pyrate_core::tape::ogg_opus(&chunks, ch, &title, &name))?;
                Ok(json!({ "path": path, "seconds": chunks.len() * 6, "title": title, "station": name }))
            }
            "hand_request" => {
                let key = pyrate_core::StationKey::generate()?;
                std::fs::write(inner.cfg.dir.join("hand.key"), key.seed())?;
                Ok(json!({ "code": key.id().to_hex() }))
            }
            "hand_join" => {
                let st = station()?;
                let seed = std::fs::read(inner.cfg.dir.join("hand.key")).context("ask for a hand's code first")?;
                let seed: [u8; 32] = seed.as_slice().try_into().context("hand key")?;
                inner.store.lock().unwrap().add_key_for(st, pyrate_core::StationKey::from_seed(seed))?;
                let _ = std::fs::remove_file(inner.cfg.dir.join("hand.key"));
                let i2 = inner.clone();
                self.rt.spawn(async move {
                    let _ = i2.sync_log(st).await;
                });
                Ok(json!({ "ok": true }))
            }
            "hand_appoint" => {
                let st = station()?;
                let code = unhex(req["code"].as_str().context("code")?.trim())?;
                self.appoint(st, req["words"].as_str().context("words")?, code, req["label"].as_str().unwrap_or("hand"))?;
                Ok(json!({ "ok": true }))
            }
            "maroon_export" => {
                let (_, marooned) = inner.chart();
                let log = inner.log.lock().unwrap();
                let lines: Vec<String> = marooned.iter().map(|h| {
                    let name = StationId::from_hex(h).ok().and_then(|id| log.get(&id).and_then(|l| l.announce.as_ref().map(|a| a.name.clone()))).unwrap_or_default();
                    if name.is_empty() { h.clone() } else { format!("{h}  # {name}") }
                }).collect();
                Ok(json!({ "text": format!("# A Pyrate Radio maroon list: one station per line. Take it aboard under Colours, or drop it in a Lantern's maroon.txt.\n{}\n", lines.join("\n")), "count": lines.len() }))
            }
            "maroon_import" => {
                let text = req["text"].as_str().context("text")?;
                let own: std::collections::HashSet<StationId> = inner.store.lock().unwrap().own().into_iter().collect();
                let mut n = 0;
                for line in text.lines() {
                    let Some(tok) = line.split_whitespace().next() else { continue };
                    let Ok(id) = StationId::from_hex(tok) else { continue };
                    if own.contains(&id) {
                        continue;
                    }
                    inner.set_chart(&id.to_hex(), "marooned", true)?;
                    n += 1;
                }
                inner.refused_changed();
                inner.drop_refused();
                Ok(json!({ "added": n }))
            }
            "aboard_history" => {
                let st = station()?;
                let text = std::fs::read_to_string(inner.cfg.dir.join(format!("aboard-{}", store::filed_as(&st)))).unwrap_or_default();
                let pts: Vec<Value> = text.lines().filter_map(|l| {
                    let (t, c) = l.split_once(' ')?;
                    Some(json!([t.parse::<u64>().ok()?, c.parse::<u64>().ok()?]))
                }).collect();
                Ok(json!(pts))
            }
            "strict" => {
                if let Some(on) = req["value"].as_bool() {
                    let p = inner.cfg.dir.join("strict");
                    if on { std::fs::write(&p, b"")?; } else { let _ = std::fs::remove_file(&p); }
                    inner.strict.store(on, std::sync::atomic::Ordering::Relaxed);
                }
                Ok(json!({ "value": inner.strict.load(std::sync::atomic::Ordering::Relaxed) }))
            }
            "provisions" => {
                let p = inner.cfg.dir.join("provisions-off");
                if let Some(on) = req["value"].as_bool() {
                    if on { let _ = std::fs::remove_file(&p); } else { std::fs::write(&p, b"")?; }
                }
                Ok(json!({ "value": !p.exists() }))
            }
            "pin_set" => {
                let p = inner.cfg.dir.join("pin");
                match req["pin"].as_str().filter(|s| s.len() >= 4) {
                    Some(pin) => {
                        let salt: [u8; 16] = rand::random();
                        let h = blake3::keyed_hash(blake3::hash(&salt).as_bytes(), pin.as_bytes());
                        std::fs::write(&p, [salt.as_slice(), h.as_bytes()].concat())?;
                    }
                    None => { let _ = std::fs::remove_file(&p); }
                }
                Ok(json!({ "ok": true }))
            }
            "pin_check" => {
                let Ok(b) = std::fs::read(inner.cfg.dir.join("pin")) else { return Ok(json!({ "ok": true, "set": false })) };
                let (salt, want) = b.split_at(16.min(b.len()));
                let h = blake3::keyed_hash(blake3::hash(salt).as_bytes(), req["pin"].as_str().unwrap_or("").as_bytes());
                Ok(json!({ "ok": req["pin"].is_string() && h.as_bytes().as_slice() == want, "set": true }))
            }
            "qr" => {
                let code = qrcode::QrCode::new(req["text"].as_str().context("text")?.as_bytes())?;
                let w = code.width();
                let bits: String = code.to_colors().iter().map(|c| if *c == qrcode::Color::Dark { '1' } else { '0' }).collect();
                Ok(json!({ "size": w, "bits": bits }))
            }
            "pref" => {
                // Small settings the app keeps beside the node: sort order, filters, and the like.
                let p = inner.cfg.dir.join("prefs.json");
                let mut all: serde_json::Map<String, Value> = std::fs::read(&p).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
                let key = req["key"].as_str().context("key")?.to_string();
                if !req["value"].is_null() {
                    all.insert(key.clone(), req["value"].clone());
                    std::fs::write(&p, Value::Object(all.clone()).to_string())?;
                }
                Ok(all.get(&key).cloned().unwrap_or(Value::Null))
            }
            "art_want" => {
                let st = station()?;
                let h = unhex(req["hash"].as_str().context("hash")?)?;
                inner.art_want(st, h);
                Ok(json!({ "ok": true }))
            }
            "art_set" => {
                let st = station()?;
                let raw = std::fs::read(req["path"].as_str().context("path")?)?;
                inner.set_art(st, req["kind"].as_str().unwrap_or("cover"), &raw)?;
                Ok(json!({ "ok": true }))
            }
            "art_clear" => {
                inner.clear_art(station()?, req["kind"].as_str().unwrap_or("cover"))?;
                Ok(json!({ "ok": true }))
            }
            "art_kept_only" => {
                let p = inner.cfg.dir.join("art-kept-only");
                match req["value"].as_bool() {
                    Some(true) => std::fs::write(&p, b"")?,
                    Some(false) => { let _ = std::fs::remove_file(&p); }
                    None => {}
                }
                Ok(json!({ "value": p.exists() }))
            }
            "clear_hold" => {
                let tuned = inner.tuned.lock().unwrap().is_some();
                if tuned {
                    anyhow::bail!("stop listening first");
                }
                let n = inner.store.lock().unwrap().clear_hold()?;
                Ok(json!({ "cleared": n }))
            }
            "tune" => {
                let st = station()?;
                if inner.refused().contains(&st) {
                    anyhow::bail!("you marooned this station; take it back first");
                }
                say(&format!("tune {}", st.call_sign()));
                *inner.tuned.lock().unwrap() = Some(Tuned { station: st, since: now_ms(), playhead: None });
                #[cfg(feature = "playback")]
                playout::reset(inner);
                let i2 = inner.clone();
                self.rt.spawn(async move {
                    let _ = i2.sync_log(st).await;
                });
                Ok(json!({ "ok": true }))
            }
            "stop" => {
                *inner.tuned.lock().unwrap() = None;
                #[cfg(feature = "playback")]
                playout::reset(inner);
                Ok(json!({ "ok": true }))
            }
            "volume" => {
                #[cfg(feature = "playback")]
                {
                    inner.queue.lock().unwrap().volume = req["value"].as_f64().unwrap_or(1.0) as f32;
                }
                Ok(json!({ "ok": true }))
            }
            "raise" => {
                let name = req["name"].as_str().context("name")?.trim();
                if name.is_empty() {
                    anyhow::bail!("a station needs a name");
                }
                let (id, words) = self.raise_with(name, req["about"].as_str().unwrap_or(""), req["port"].as_str().map(String::from), false)?;
                // A pocket station: for invited listeners only, never on the public Log.
                if req["pocket"].as_bool() == Some(true) {
                    inner.pocket_new(id)?;
                }
                Ok(json!({ "id": id.to_hex(), "call_sign": id.call_sign(), "words": words }))
            }
            "add_track" => {
                let st = station()?;
                let path = PathBuf::from(req["path"].as_str().context("path")?);
                let title = req["title"].as_str().map(String::from).unwrap_or_else(|| {
                    path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()
                });
                let by = req["by"].as_str().unwrap_or("").to_string();
                let source = req["source"].as_str().unwrap_or("").to_string();
                let profile = if req["talk"].as_bool().unwrap_or(false) { Profile::Talk } else { Profile::Music };
                let job = now_ms();
                inner.jobs.lock().unwrap().insert(job, format!("Setting {title}"));
                let i2 = inner.clone();
                std::thread::spawn(move || {
                    let r = i2.add_track(st, &path, &title, &by, &source, profile);
                    let msg = match r {
                        Ok(_) => format!("On the clock: {title}"),
                        Err(e) => format!("Could not set {title}: {e}"),
                    };
                    i2.jobs.lock().unwrap().insert(job, msg);
                });
                Ok(json!({ "job": job }))
            }
            // A drawer Lantern gives more: 1 to 256 GB, carried on Wi-Fi while it charges.
            "ballast_size" => {
                let gb = req["gb"].as_u64().unwrap_or(1).clamp(1, 256);
                inner.ballast_bytes.store(gb * 1_000_000_000, std::sync::atomic::Ordering::Relaxed);
                inner.store.lock().unwrap().quota = inner.cfg.quota + gb * 1_000_000_000;
                Ok(json!({ "gb": gb }))
            }
            "ballast_ok" => {
                inner.ballast_ok.store(req["value"].as_bool().unwrap_or(false), std::sync::atomic::Ordering::Relaxed);
                inner.relay_ok.store(req["unmetered"].as_bool().unwrap_or(false), std::sync::atomic::Ordering::Relaxed);
                Ok(json!({ "ok": true, "carrying": inner.store.lock().unwrap().pinned.len() }))
            }
            "update" => Ok(match self.rt.block_on(inner.check_update(req["code"].as_u64().unwrap_or(0) as u32)) {
                Ok(v) => v,
                Err(e) => json!({ "available": false, "why": e.to_string() }),
            }),
            "clear_job" => {
                inner.jobs.lock().unwrap().remove(&req["job"].as_u64().unwrap_or(0));
                Ok(json!({ "ok": true }))
            }
            "library" => {
                let st = station()?;
                let s = inner.store.lock().unwrap();
                let a = s.logs.get(&st).context("unknown station")?;
                let v: Vec<Value> = a.library()?.into_iter().filter_map(|(h, e)| match e.entry.body() {
                    Body::Track { title, by, chunks, .. } => Some(json!({
                        "hash": hex(&h), "title": title, "by": by,
                        "duration_ms": chunks.iter().map(|c| c.duration_ms()).sum::<u64>(),
                    })),
                    _ => None,
                }).collect();
                Ok(json!(v))
            }
            "withdraw" => {
                let st = station()?;
                let h = unhex(req["track"].as_str().context("track")?)?;
                inner.store.lock().unwrap().write(st, now_ms(), Body::Withdraw { track: h })?;
                Ok(json!({ "ok": true }))
            }
            "phrase" => {
                let st = station()?;
                let s = inner.store.lock().unwrap();
                Ok(json!({ "phrase": s.keys.get(&st).context("not our station")?.phrase() }))
            }
            "restore" => {
                // The station's log comes aboard first, so the revocation follows its last entry;
                // then every old deputy is shut out and this phone gets a new one.
                let phrase = req["phrase"].as_str().context("phrase")?;
                let id = pyrate_core::StationKey::from_phrase(phrase)?.id();
                let _ = self.rt.block_on(async { tokio::time::timeout(Duration::from_secs(25), inner.sync_log(id)).await });
                if inner.store.lock().unwrap().logs.get(&id).is_none_or(|a| a.len() == 0) {
                    anyhow::bail!("the station's log could not be found yet; try again in a minute, ideally on Wi-Fi");
                }
                self.with_master(id, phrase, "restore")?;
                Ok(json!({ "id": id.to_hex(), "call_sign": id.call_sign() }))
            }
            "go_live" => {
                let st = station()?;
                inner.go_live(st, req["file"].as_str().map(PathBuf::from))?;
                Ok(json!({ "ok": true }))
            }
            "end_live" => {
                *inner.live.lock().unwrap() = None;
                Ok(json!({ "ok": true }))
            }
            "strike" => {
                let st = station()?;
                inner.store.lock().unwrap().write(st, now_ms(), Body::Strike)?;
                Ok(json!({ "ok": true }))
            }
            other => Err(anyhow!("unknown command {other}")),
        }
    }
}

pub fn hex(h: &[u8]) -> String {
    h.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn store_hex32(s: &str) -> Option<[u8; 32]> {
    unhex(s).ok()
}

pub fn unhex(s: &str) -> Result<Hash> {
    pyrate_core::unhex32(s).ok_or_else(|| anyhow!("bad hash"))
}

fn getrandom(b: &mut [u8]) -> Result<()> {
    // The station key generator already pulls from the OS; reuse it for the node key.
    let k = pyrate_core::StationKey::generate()?;
    b.copy_from_slice(&k.seed());
    Ok(())
}

/// Turns a station's log and the hour into sound in the queue.
#[cfg(feature = "playback")]
mod playout {
    use super::*;
    use pyrate_core::audio::ChunkDecoder;

    const AHEAD_MS: u64 = 12_000;

    struct State {
        station: Option<StationId>,
        playhead: Option<u64>,
        track: Option<Hash>,
        decoder: Option<(u8, ChunkDecoder)>,
        live_seq: Option<u64>,
    }

    static RESET: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

    pub fn reset(inner: &Inner) {
        let mut q = inner.queue.lock().unwrap();
        q.samples.clear();
        q.played = 0;
        RESET.store(true, std::sync::atomic::Ordering::SeqCst);
        *inner.np.lock().unwrap() = Value::Null;
    }

    fn queued_ms(inner: &Inner) -> u64 {
        inner.queue.lock().unwrap().samples.len() as u64 / 2 * 1000 / 48_000
    }

    fn push(inner: &Inner, pcm: &[i16], channels: u8, skip_ms: u64) {
        let skip = (skip_ms * 48) as usize * channels as usize;
        let pcm = if skip < pcm.len() { &pcm[skip..] } else { &[][..] };
        let mut q = inner.queue.lock().unwrap();
        for f in pcm.chunks(channels as usize) {
            let l = f[0] as f32 / 32768.0;
            let r = if channels == 2 { f[1] as f32 / 32768.0 } else { l };
            q.samples.push_back(l);
            q.samples.push_back(r);
        }
    }

    fn live_tail(a: &Archive, now: u64) -> Option<Vec<(u64, pyrate_core::Chunk, u8)>> {
        let last = a.entries().last()?;
        if !matches!(last.entry.body(), Body::Live { .. }) || now.saturating_sub(last.entry.at_ms) > 25_000 {
            return None;
        }
        Some(a.entries().iter().filter_map(|e| match e.entry.body() {
            Body::Live { chunk, channels, .. } => Some((e.entry.seq, *chunk, *channels)),
            _ => None,
        }).collect())
    }

    fn describe(a: &Archive, track: &Hash) -> (String, String) {
        for e in a.entries() {
            if let Body::Track { title, by, .. } = e.entry.body() {
                if e.hash().ok().as_ref() == Some(track) {
                    return (title.clone(), by.clone());
                }
            }
        }
        (String::new(), String::new())
    }

    pub async fn run(inner: Arc<Inner>) {
        let mut st = State { station: None, playhead: None, track: None, decoder: None, live_seq: None };
        let mut last_sync = 0u64;
        // Off the air: when the station comes back, worked out at most once a minute.
        let mut off_air: Option<(StationId, u64, Option<u64>)> = None;
        loop {
            tokio::time::sleep(Duration::from_millis(250)).await;
            if RESET.swap(false, std::sync::atomic::Ordering::SeqCst) {
                st = State { station: None, playhead: None, track: None, decoder: None, live_seq: None };
            }
            let tuned = inner.tuned.lock().unwrap().as_ref().map(|t| (t.station, t.since));
            let Some((station, _since)) = tuned else { continue };
            if st.station != Some(station) {
                st = State { station: Some(station), playhead: None, track: None, decoder: None, live_seq: None };
            }
            let now = inner.play_now();
            if now.saturating_sub(last_sync) > if st.live_seq.is_some() { 2_500 } else { 8_000 } {
                last_sync = now;
                let _ = inner.sync_log(station).await;
            }
            let archive = inner.store.lock().unwrap().logs.get(&station).cloned();
            let Some(archive) = archive else {
                *inner.np.lock().unwrap() = json!({ "station": station.to_hex(), "state": "searching" });
                continue;
            };
            let clock = match Clock::new(&archive) {
                Ok(c) => c,
                Err(_) => continue,
            };
            let q = queued_ms(&inner);

            // Off the beat: something stalled. Drop the queue and rejoin the clock.
            if let Some(ph) = st.playhead {
                let heard_at = ph.saturating_sub(q);
                if st.live_seq.is_none() && (heard_at + 4_000 < now || heard_at > now + AHEAD_MS + 4_000) {
                    inner.queue.lock().unwrap().samples.clear();
                    st.playhead = None;
                    st.track = None;
                }
            }

            // Fill ahead.
            let mut guard = 0;
            while queued_ms(&inner) < AHEAD_MS && guard < 4 {
                guard += 1;
                let now = inner.play_now();
                if let Some(tail) = live_tail(&archive, now) {
                    // ON AIR: walk the live chunks, starting two back for a cushion.
                    let next = match st.live_seq {
                        Some(s) => tail.iter().find(|(seq, _, _)| *seq > s).cloned(),
                        None => tail.iter().rev().nth(1).or(tail.last()).cloned(),
                    };
                    let Some((seq, chunk, ch)) = next else { break };
                    let Ok(bytes) = inner.chunk(&station, &chunk.hash).await else { break };
                    if st.decoder.as_ref().is_none_or(|(c, _)| *c != ch) || st.live_seq.is_none() {
                        st.decoder = ChunkDecoder::new(ch).ok().map(|d| (ch, d));
                    }
                    let Some((_, dec)) = st.decoder.as_mut() else { break };
                    if let Ok(pcm) = dec.decode(&bytes) {
                        push(&inner, &pcm, ch, 0);
                        keep_heard(&inner, station, ch, &bytes, "Live".into());
                    }
                    if st.live_seq.is_none() {
                        say(&format!("sound: live, entry {seq}"));
                    }
                    st.live_seq = Some(seq);
                    st.track = None;
                    st.playhead = None;
                    *inner.np.lock().unwrap() = json!({ "station": station.to_hex(), "state": "live", "title": "ON AIR" });
                    continue;
                }
                if st.live_seq.take().is_some() {
                    // The broadcast ended. Back to the clock, at the hour it has reached.
                    st.decoder = None;
                }
                let at = st.playhead.unwrap_or(now + 400);
                // Nothing on the clock: off the air, or nothing yet to play. Said below.
                let Some(np) = clock.now(at) else { break };
                let entry = archive.entries().iter().find(|e| e.hash().ok() == Some(np.track));
                let Some(Body::Track { chunks, channels, .. }) = entry.map(|e| e.entry.body()) else { break };
                let Some(chunk) = chunks.get(np.chunk).copied() else { break };
                let Ok(bytes) = inner.chunk(&station, &chunk.hash).await else {
                    say("no carrier answered for the chunk on now");
                    *inner.np.lock().unwrap() = json!({ "station": station.to_hex(), "state": "searching" });
                    break;
                };
                if st.track != Some(np.track) || st.decoder.is_none() {
                    st.decoder = ChunkDecoder::new(*channels).ok().map(|d| (*channels, d));
                    st.track = Some(np.track);
                }
                let Some((_, dec)) = st.decoder.as_mut() else { break };
                let Ok(pcm) = dec.decode(&bytes) else { break };
                // A first chunk starts where the hour has reached by the time it lands.
                let skip = if st.playhead.is_none() {
                    np.chunk_offset_ms + inner.play_now().saturating_sub(at.min(inner.play_now()))
                } else {
                    np.chunk_offset_ms
                };
                let skip = if st.playhead.is_none() { np.chunk_offset_ms.max(skip.min(chunk.duration_ms())) } else { skip };
                if st.playhead.is_none() {
                    say(&format!("sound: chunk {} of a track, {} ms in", np.chunk, skip));
                }
                push(&inner, &pcm, *channels, skip);
                keep_heard(&inner, station, *channels, &bytes, describe(&archive, &np.track).0);
                let chunk_start = at - np.chunk_offset_ms;
                st.playhead = Some(chunk_start + chunk.duration_ms());
            }

            // What the listener hears now.
            if st.live_seq.is_none() {
                if let Some(np) = clock.now(now) {
                    let (title, by) = describe(&archive, &np.track);
                    let starved = inner.queue.lock().unwrap().starved;
                    let pic = archive.track_art(&np.track).map(|a| hex(&a.hash));
                    *inner.np.lock().unwrap() = json!({
                        "station": station.to_hex(),
                        "state": if starved { "searching" } else { "playing" },
                        "title": title, "by": by, "art": pic, "slot": np.slot,
                        "position_ms": np.offset_ms, "duration_ms": np.duration_ms,
                    });
                    off_air = None;
                } else if queued_ms(&inner) < 300 {
                    let stale = off_air.as_ref().is_none_or(|(s, at, _)| *s != station || now.saturating_sub(*at) > 60_000);
                    if stale {
                        let back = clock.upcoming(now, now + 7 * 86_400_000).first().map(|b| b.0);
                        off_air = Some((station, now, back));
                    }
                    *inner.np.lock().unwrap() = match off_air.as_ref().and_then(|o| o.2) {
                        Some(back) => json!({ "station": station.to_hex(), "state": "off air", "back_at": back }),
                        None => json!({ "station": station.to_hex(), "state": "dead air" }),
                    };
                }
            }
        }
    }
}
