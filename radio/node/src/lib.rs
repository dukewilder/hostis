//! pyrate-node: one install, one full node. It carries, relays, gossips the Log,
//! serves anyone who asks, and (in the app) plays.
//!
//! No rights reserved.

pub mod net;
#[cfg(feature = "playback")]
pub mod player;
pub mod store;

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

pub fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64
}

#[derive(Clone, Debug)]
pub struct Config {
    pub dir: PathBuf,
    pub quota: u64,
    /// Lanterns carry every station they hear of, whole.
    pub lantern: bool,
    pub bootstrap: Vec<EndpointId>,
    /// Ask public resolvers instead of the system's. For processes with no Android context.
    pub public_dns: bool,
}

#[derive(Default)]
struct Listing {
    announce: Option<Announce>,
    carriers: HashMap<[u8; 32], (u64, u64)>,
    sketches: HashMap<u64, Sketch>,
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
    store: Mutex<Store>,
    log: Mutex<HashMap<StationId, Listing>>,
    conns: Mutex<HashMap<EndpointId, Connection>>,
    gossip_tx: Mutex<Option<GossipSender>>,
    tuned: Mutex<Option<Tuned>>,
    jobs: Mutex<HashMap<u64, String>>,
    pow: Mutex<HashMap<StationId, u64>>,
    #[cfg(feature = "playback")]
    queue: player::SharedQueue,
    #[cfg(feature = "playback")]
    np: Mutex<Value>,
}

pub struct Node {
    pub rt: tokio::runtime::Runtime,
    pub inner: Arc<Inner>,
    _router: Router,
    #[cfg(feature = "playback")]
    _out: Option<player::Output>,
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
                let Ok(bytes) = recv.read_to_end(64 * 1024).await else { return };
                let Ok(req) = postcard::from_bytes::<Request>(&bytes) else { return };
                let resp = inner.serve(req);
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
        let store = self.store.lock().unwrap();
        match req {
            Request::Log { station, from } => {
                let out = store
                    .logs
                    .get(&station)
                    .map(|a| {
                        a.entries().iter().skip(from as usize).take(2000).filter_map(|e| e.to_bytes().ok()).collect()
                    })
                    .unwrap_or_default();
                Response::Entries(out)
            }
            Request::Chunk { hash } => Response::Chunk(store.get_chunk(&hash)),
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

    pub async fn sync_log(&self, station: StationId) -> Result<usize> {
        let mut added = 0;
        for node in self.carriers(&station) {
            let from = self.store.lock().unwrap().logs.get(&station).map(|a| a.len()).unwrap_or(0);
            let Ok(Response::Entries(raw)) = self.ask(node, &Request::Log { station, from }).await else { continue };
            let entries: Vec<SignedEntry> = raw.iter().filter_map(|b| SignedEntry::from_bytes(b).ok()).collect();
            if entries.is_empty() {
                continue;
            }
            match self.store.lock().unwrap().put_entries(station, entries) {
                Ok(n) => added += n,
                Err(e) => tracing::warn!("log from {node}: {e}"),
            }
            break;
        }
        Ok(added)
    }

    pub async fn chunk(&self, station: &StationId, hash: &Hash) -> Result<Vec<u8>> {
        if let Some(b) = self.store.lock().unwrap().get_chunk(hash) {
            return Ok(b);
        }
        for node in self.carriers(station) {
            if let Ok(Response::Chunk(Some(b))) = self.ask(node, &Request::Chunk { hash: *hash }).await {
                if self.store.lock().unwrap().put_chunk(hash, &b).is_ok() {
                    return Ok(b);
                }
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
                let l = log.entry(a.station).or_default();
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
            Gossip::Aboard { station, hour, sketch } => {
                if hour + 2 >= now / 3_600_000 {
                    log.entry(station).or_default().sketches.entry(hour).or_default().merge(&sketch);
                }
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
        let mut h = blake3::Hasher::new_keyed(&self.secret);
        h.update(&station.0);
        h.update(&hour.to_le_bytes());
        *h.finalize().as_bytes()
    }

    fn pow_for(&self, station: &StationId) -> u64 {
        if let Some(n) = self.pow.lock().unwrap().get(station) {
            return *n;
        }
        let p = self.store.lock().unwrap().dir().join("keys").join(format!("{}.pow", station.to_hex()));
        let n = std::fs::read_to_string(&p).ok().and_then(|s| s.trim().parse().ok()).unwrap_or_else(|| {
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
            let a = {
                let s = self.store.lock().unwrap();
                let Some(log) = s.logs.get(&id) else { continue };
                let Some(Body::Colours { name, about, port, .. }) = log.colours().cloned() else { continue };
                if log.is_struck() {
                    continue;
                }
                let live = log.entries().last().is_some_and(|e| matches!(e.entry.body, Body::Live { .. }) && now - e.entry.at_ms < 15_000);
                let key = s.keys.get(&id).unwrap();
                Announce { station: id, head: log.len(), name, about, port, live, at_ms: now, pow: 0, sig: [0; 64] }
                    .pipe_pow(self.pow_for(&id))
                    .sign(key)
            };
            self.on_gossip(Gossip::Station(a.clone()));
            self.broadcast(&Gossip::Station(a)).await;
        }
        for (station, head) in carried {
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
            let token = self.token(&station, hour);
            self.log.lock().unwrap().entry(station).or_default().sketches.entry(hour).or_default().add(&token);
        }
        let sketches: Vec<Gossip> = {
            let log = self.log.lock().unwrap();
            log.iter()
                .filter_map(|(st, l)| l.sketches.get(&hour).map(|s| Gossip::Aboard { station: *st, hour, sketch: s.0.to_vec() }))
                .filter(|g| matches!(g, Gossip::Aboard { station, .. } if self.cfg.lantern || Some(*station) == tuned || self.store.lock().unwrap().logs.contains_key(station)))
                .collect()
        };
        for g in sketches {
            self.broadcast(&g).await;
        }
    }

    /// Lanterns keep a whole copy of every station they carry.
    async fn mirror(self: &Arc<Self>) {
        let stations: Vec<StationId> = self.log.lock().unwrap().keys().copied().collect();
        for st in stations {
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
        }
    }
}

impl Inner {
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
        Ok(e.hash()?)
    }

}

trait PipePow {
    fn pipe_pow(self, pow: u64) -> Self;
}

impl PipePow for Announce {
    fn pipe_pow(mut self, pow: u64) -> Self {
        self.pow = pow;
        self
    }
}

impl Node {
    pub fn start(cfg: Config) -> Result<Self> {
        let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build()?;
        let store = Store::open(&cfg.dir, cfg.quota)?;
        let key_path = cfg.dir.join("node.key");
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
        let proto = Proto(Arc::new(Mutex::new(None)));
        let public_dns = cfg.public_dns;
        let (endpoint, router, gossip) = rt.block_on(async {
            let mut b = Endpoint::builder(presets::N0).secret_key(secret);
            if public_dns {
                b = b.dns_resolver(
                    iroh::dns::DnsResolver::builder()
                        .with_nameserver("1.1.1.1:53".parse()?, iroh::dns::DnsProtocol::Udp)
                        .with_nameserver("9.9.9.9:53".parse()?, iroh::dns::DnsProtocol::Udp)
                        .build(),
                );
            }
            if std::env::var_os("HTTPS_PROXY").is_some() {
                b = b.proxy_from_env().ca_tls_config(iroh::tls::CaTlsConfig::system());
            }
            let ep = b.bind().await.map_err(|e| anyhow!("bind: {e}"))?;
            let gossip = GossipNet::builder().spawn(ep.clone());
            let router = Router::builder(ep.clone()).accept(ALPN, proto.clone()).accept(GOSSIP_ALPN, gossip.clone()).spawn();
            anyhow::Ok((ep, router, gossip))
        })?;

        #[cfg(feature = "playback")]
        let queue: player::SharedQueue = Arc::new(Mutex::new(player::Queue { volume: 1.0, ..Default::default() }));
        let inner = Arc::new(Inner {
            cfg: cfg.clone(),
            endpoint: endpoint.clone(),
            secret: secret_bytes,
            store: Mutex::new(store),
            log: Mutex::new(HashMap::new()),
            conns: Mutex::new(HashMap::new()),
            gossip_tx: Mutex::new(None),
            tuned: Mutex::new(None),
            jobs: Mutex::new(HashMap::new()),
            pow: Mutex::new(HashMap::new()),
            #[cfg(feature = "playback")]
            queue: queue.clone(),
            #[cfg(feature = "playback")]
            np: Mutex::new(Value::Null),
        });
        *proto.0.lock().unwrap() = Some(inner.clone());

        // The Log.
        let topic = TopicId::from_bytes(net::log_topic());
        let i2 = inner.clone();
        rt.spawn(async move {
            let Ok(sub) = gossip.subscribe(topic, i2.cfg.bootstrap.clone()).await else { return };
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
                let Ok(Event::Received(msg)) = ev else { continue };
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
                if i2.cfg.lantern && n % 3 == 1 {
                    i2.mirror().await;
                }
                if n % 30 == 0 {
                    let s = i2.store.lock().unwrap();
                    let _ = s.enforce_quota();
                }
                n += 1;
                tokio::time::sleep(Duration::from_secs(20)).await;
            }
        });

        #[cfg(feature = "playback")]
        let out = {
            let i2 = inner.clone();
            rt.spawn(async move { playout::run(i2).await });
            match player::open(queue) {
                Ok(o) => Some(o),
                Err(e) => {
                    eprintln!("no audio output: {e}");
                    None
                }
            }
        };

        Ok(Self {
            rt,
            inner,
            _router: router,
            #[cfg(feature = "playback")]
            _out: out,
        })
    }

    pub fn id(&self) -> EndpointId {
        self.inner.endpoint.id()
    }

    /// Raises colours: a new station key, its proof of work, its first entry.
    pub fn raise(&self, name: &str, about: &str, port: Option<String>) -> Result<StationId> {
        let key = pyrate_core::StationKey::generate()?;
        let id = {
            let mut s = self.inner.store.lock().unwrap();
            let id = s.add_key(key)?;
            s.write(id, now_ms(), Body::Colours { name: name.into(), about: about.into(), port, tip: None })?;
            id
        };
        self.inner.pow_for(&id);
        Ok(id)
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
        s.logs.get(station).map(|a| a.entries().iter().filter_map(|e| match &e.entry.body {
            Body::Track { title, .. } => Some(title.clone()),
            _ => None,
        }).collect()).unwrap_or_default()
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
            "status" => {
                let s = inner.store.lock().unwrap();
                let mine = s.own().into_iter().find(|id| !s.logs.get(id).is_some_and(|a| a.is_struck())).map(|id| {
                    let a = s.logs.get(&id);
                    let name = a.and_then(|a| a.colours()).map(|c| match c {
                        Body::Colours { name, .. } => name.clone(),
                        _ => String::new(),
                    });
                    let tracks = a.map(|a| a.library().map(|l| l.len()).unwrap_or(0)).unwrap_or(0);
                    json!({ "id": id.to_hex(), "call_sign": id.call_sign(), "name": name, "tracks": tracks })
                });
                let jobs: Vec<Value> = inner.jobs.lock().unwrap().iter().map(|(k, v)| json!({ "id": k, "state": v })).collect();
                let tuned = inner.tuned.lock().unwrap().as_ref().map(|t| t.station.to_hex());
                #[cfg(feature = "playback")]
                let np = inner.np.lock().unwrap().clone();
                #[cfg(not(feature = "playback"))]
                let np = Value::Null;
                Ok(json!({ "node": self.id().to_string(), "mine": mine, "jobs": jobs, "tuned": tuned, "now": np }))
            }
            "stations" => {
                let now = now_ms();
                let log = inner.log.lock().unwrap();
                let mut v: Vec<(u64, Value)> = log
                    .iter()
                    .filter(|(_, l)| l.fresh(now))
                    .filter_map(|(id, l)| {
                        let a = l.announce.as_ref()?;
                        let aboard = l.aboard(now);
                        Some((aboard, json!({
                            "id": id.to_hex(), "call_sign": id.call_sign(), "name": a.name, "about": a.about,
                            "port": a.port, "live": a.live, "aboard": aboard,
                            "carriers": l.carriers.values().filter(|(_, s)| now - s < 10 * 60_000).count(),
                        })))
                    })
                    .collect();
                v.sort_by(|a, b| b.0.cmp(&a.0).then(a.1["name"].as_str().cmp(&b.1["name"].as_str())));
                Ok(json!(v.into_iter().map(|x| x.1).collect::<Vec<_>>()))
            }
            "tune" => {
                let st = station()?;
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
                let id = self.raise(name, req["about"].as_str().unwrap_or(""), req["port"].as_str().map(String::from))?;
                Ok(json!({ "id": id.to_hex(), "call_sign": id.call_sign() }))
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
            "clear_job" => {
                inner.jobs.lock().unwrap().remove(&req["job"].as_u64().unwrap_or(0));
                Ok(json!({ "ok": true }))
            }
            "library" => {
                let st = station()?;
                let s = inner.store.lock().unwrap();
                let a = s.logs.get(&st).context("unknown station")?;
                let v: Vec<Value> = a.library()?.into_iter().filter_map(|(h, e)| match &e.entry.body {
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
                let key = pyrate_core::StationKey::from_phrase(req["phrase"].as_str().context("phrase")?)?;
                let id = inner.store.lock().unwrap().add_key(key)?;
                let i2 = inner.clone();
                self.rt.spawn(async move {
                    let _ = i2.sync_log(id).await;
                });
                Ok(json!({ "id": id.to_hex(), "call_sign": id.call_sign() }))
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

pub fn unhex(s: &str) -> Result<Hash> {
    if s.len() != 64 {
        anyhow::bail!("bad hash");
    }
    let mut h = [0u8; 32];
    for i in 0..32 {
        h[i] = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16)?;
    }
    Ok(h)
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
        if !matches!(last.entry.body, Body::Live { .. }) || now.saturating_sub(last.entry.at_ms) > 15_000 {
            return None;
        }
        Some(a.entries().iter().filter_map(|e| match &e.entry.body {
            Body::Live { chunk, channels, .. } => Some((e.entry.seq, *chunk, *channels)),
            _ => None,
        }).collect())
    }

    fn describe(a: &Archive, track: &Hash) -> (String, String) {
        for e in a.entries() {
            if let Body::Track { title, by, .. } = &e.entry.body {
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
            let now = now_ms();
            if now - last_sync > 8_000 {
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
                let now = now_ms();
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
                let Some(np) = clock.now(at) else {
                    *inner.np.lock().unwrap() = json!({ "station": station.to_hex(), "state": "dead air" });
                    break;
                };
                let entry = archive.entries().iter().find(|e| e.hash().ok() == Some(np.track));
                let Some(Body::Track { chunks, channels, .. }) = entry.map(|e| &e.entry.body) else { break };
                let Some(chunk) = chunks.get(np.chunk).copied() else { break };
                let Ok(bytes) = inner.chunk(&station, &chunk.hash).await else {
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
                    np.chunk_offset_ms + now_ms().saturating_sub(at.min(now_ms()))
                } else {
                    np.chunk_offset_ms
                };
                let skip = if st.playhead.is_none() { np.chunk_offset_ms.max(skip.min(chunk.duration_ms())) } else { skip };
                push(&inner, &pcm, *channels, skip);
                let chunk_start = at - np.chunk_offset_ms;
                st.playhead = Some(chunk_start + chunk.duration_ms());
            }

            // What the listener hears now.
            if st.live_seq.is_none() {
                if let Some(np) = clock.now(now.saturating_sub(0)) {
                    let (title, by) = describe(&archive, &np.track);
                    let starved = inner.queue.lock().unwrap().starved;
                    *inner.np.lock().unwrap() = json!({
                        "station": station.to_hex(),
                        "state": if starved { "searching" } else { "playing" },
                        "title": title, "by": by,
                        "position_ms": np.offset_ms, "duration_ms": np.duration_ms,
                    });
                }
            }
        }
    }
}
