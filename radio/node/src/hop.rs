//! Privacy without delay.
//!
//! Listeners fetch through two hops. The first sees who asks and not what; the
//! second sees what and not who. The clock tells a phone what it will need well
//! before it needs it, so the extra hops cost nothing a listener can hear.
//!
//! Broadcasters publish along a stem, after Dandelion++ in Bitcoin: new entries and
//! chunks pass from node to node and only then spread, so the first node to show
//! them to the network is several hops from the station. Every node relays stems.
//!
//! No rights reserved.

use std::{sync::Arc, time::Duration};

use anyhow::{Result, anyhow};
use chacha20poly1305::{ChaCha20Poly1305, Nonce, aead::Aead};

use iroh::EndpointId;
use pyrate_core::{Body, Hash, SignedEntry, StationId};
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha512};

use crate::{Inner, net::{Announce, Gossip, Request, Response}, now_ms, say};

/// Chance a stem hop spreads the cargo rather than passing it on. The stem's length falls out
/// of this draw alone: four hops on average, and past twenty only once in three hundred.
const FLUFF: f64 = 0.25;
/// Cargo this node has relayed lately, so a stem that comes round again spreads here.
const STEM_MEMORY: usize = 512;
/// Stems waiting to be shown, and their bytes in KiB: 32 and 32 MiB on a phone, 256 and
/// 128 MiB on a Lantern.
static WAITING: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(32);
static WAITING_BYTES: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(32 * 1024);
static WAITING_LANTERN: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(256);
static WAITING_BYTES_LANTERN: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(128 * 1024);

/// Whether a stem's cargo is what it claims: entries that parse, belong to the station and
/// carry a valid signature, a card that is the station's and verifies, and chunks that match
/// their hashes. At least one entry or a card.
pub(crate) fn cargo_sound(station: &StationId, entries: &[Vec<u8>], chunks: &[(Hash, Vec<u8>)], announce: &Option<Announce>) -> bool {
    if entries.is_empty() && announce.is_none() {
        return false;
    }
    let entries_ok = entries.iter().all(|b| SignedEntry::from_bytes(b).is_ok_and(|e| e.entry.station == *station && e.verify().is_ok()));
    let card_ok = announce.as_ref().is_none_or(|a| a.station == *station && a.verify());
    let chunks_ok = chunks.iter().all(|(h, b)| blake3::hash(b).as_bytes() == h);
    entries_ok && card_ok && chunks_ok
}

/// What one stem carries, as a fingerprint: the same cargo is the same stem.
fn cargo_id(station: &StationId, entries: &[Vec<u8>], announce: &Option<Announce>) -> Hash {
    let mut h = blake3::Hasher::new();
    h.update(b"pyrate/stem-cargo/v1");
    h.update(&station.0);
    for e in entries {
        h.update(blake3::hash(e).as_bytes());
    }
    if let Some(a) = announce {
        h.update(&a.sig);
    }
    *h.finalize().as_bytes()
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Hop {
    /// Pass `blob` to `next` and bring back its answer. The passer cannot read either.
    Forward { next: [u8; 32], blob: Vec<u8> },
    /// A request sealed to this node: an ephemeral X25519 key and the ciphertext.
    Sealed { eph: [u8; 32], ct: Vec<u8> },
    /// A station's new entries and chunks, travelling the stem. `hops` is always sent as zero,
    /// by the broadcaster and by every relay alike, so no node can tell which one it heard
    /// from. Older nodes still count it up; nothing here reads it.
    Stem { station: StationId, entries: Vec<Vec<u8>>, chunks: Vec<(Hash, Vec<u8>)>, announce: Option<Announce>, hops: u8 },
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) enum Ask {
    Log { station: StationId, from: u64 },
    Chunk { station: StationId, hash: Hash },
    /// A listener's hourly token, merged into the station's aboard sketch by the exit.
    Token { station: StationId, hour: u64, token: [u8; 32], nonce: u64 },
    /// A chunk by its sealed name: the exit passes it on unopened.
    Sealed { station: StationId, name: Hash },
    /// A stem's cargo, sealed to the node that starts the stem: the first hop forwards a blob it
    /// cannot read, so it never learns which station the device in front of it keeps.
    Stem { station: StationId, entries: Vec<Vec<u8>>, chunks: Vec<(Hash, Vec<u8>)>, announce: Option<Announce> },
}

fn x_secret(seed: &[u8; 32]) -> x25519_dalek::StaticSecret {
    let h = Sha512::digest(seed);
    let mut b = [0u8; 32];
    b.copy_from_slice(&h[..32]);
    x25519_dalek::StaticSecret::from(b)
}

use pyrate_core::route::{ANSWER_NONCE, ASK_NONCE, cipher};

fn seal(to: &[u8; 32], ask: &Ask) -> Result<(Hop, ChaCha20Poly1305)> {
    let (eph, ct, c) = pyrate_core::route::seal_to(to, &postcard::to_stdvec(ask)?).ok_or_else(|| anyhow!("bad node key"))?;
    Ok((Hop::Sealed { eph, ct }, c))
}

impl Inner {
    /// Nodes this one knows of, other than itself: bootstrap peers and every carrier in the Log.
    fn known_nodes(&self) -> Vec<EndpointId> {
        let me = self.endpoint.id();
        let mut v: Vec<EndpointId> = self.cfg.bootstrap.clone();
        let log = self.log.lock().unwrap();
        let now = now_ms();
        for l in log.values() {
            for (n, (_, seen)) in &l.carriers {
                if now.saturating_sub(*seen) < 10 * 60_000 {
                    if let Ok(id) = EndpointId::from_bytes(n) {
                        v.push(id);
                    }
                }
            }
        }
        v.retain(|n| *n != me);
        v.sort();
        v.dedup();
        v
    }

    pub fn known_nodes_pub(&self) -> Vec<EndpointId> {
        self.known_nodes()
    }

    async fn hop(&self, node: EndpointId, hop: &Hop) -> Result<Vec<u8>> {
        match self.ask(node, &Request::Hop(postcard::to_stdvec(hop)?)).await? {
            Response::Hop(b) => Ok(b),
            _ => Err(anyhow!("unexpected answer")),
        }
    }

    /// The first hop of every private fetch, kept as long as it answers. A first hop drawn
    /// afresh for each fetch would, over a day of listening, sooner or later be drawn together
    /// with a hostile exit; a guard kept for weeks is either hostile from the start or never.
    /// After Tor's entry guards. Well-known Lanterns are passed over, as for the stem's guard.
    fn fetch_guard(&self, known: &[EndpointId]) -> Option<EndpointId> {
        let old = self.cfg.dir.join("fetch-guard");
        let held = self.hold.read_note(&self.cfg.dir, "fetch-guard", Some(&old)).and_then(|b| String::from_utf8(b).ok());
        if let Some(g) = held.and_then(|s| s.trim().parse::<EndpointId>().ok()) {
            if known.contains(&g) {
                return Some(g);
            }
        }
        let mut others: Vec<EndpointId> = known.iter().filter(|n| !self.doors.contains(n)).copied().collect();
        others.shuffle(&mut rand::thread_rng());
        // A peer met in an earlier run before a newcomer: a crowd of nodes that appeared today
        // cannot make one of their own a guard.
        let veterans: Vec<EndpointId> = others.iter().filter(|n| self.cfg.bootstrap.contains(n)).copied().collect();
        let g = veterans.first().copied().or_else(|| others.first().copied()).or_else(|| known.first().copied())?;
        let _ = self.hold.write_note(&self.cfg.dir, "fetch-guard", g.to_string().as_bytes());
        Some(g)
    }

    /// Two hops. One only when the network has a single other node and the owner has let the
    /// radio take weaker routes: a lone exit would see both who asks and what for.
    pub(crate) async fn ask_privately(&self, ask: &Ask) -> Result<Vec<u8>> {
        let mut nodes = self.known_nodes();
        if nodes.is_empty() {
            return Err(anyhow!("no route"));
        }
        nodes.shuffle(&mut rand::thread_rng());
        if nodes.len() < 2 {
            if self.strict.load(std::sync::atomic::Ordering::Relaxed) {
                return Err(anyhow!("one node known: no two-hop route yet"));
            }
            let exit = nodes[0];
            let (sealed, c) = seal(exit.as_bytes(), ask)?;
            let raw = self.hop(exit, &sealed).await?;
            return c.decrypt(Nonce::from_slice(&ANSWER_NONCE), raw.as_slice()).map_err(|_| anyhow!("open"));
        }
        let guard = self.fetch_guard(&nodes).ok_or_else(|| anyhow!("no route"))?;
        let exit = *nodes.iter().find(|n| **n != guard).ok_or_else(|| anyhow!("no route"))?;
        let (sealed, c) = seal(exit.as_bytes(), ask)?;
        let blob = postcard::to_stdvec(&sealed)?;
        let raw = match self.hop(guard, &Hop::Forward { next: *exit.as_bytes(), blob }).await {
            Ok(r) => r,
            Err(e) => {
                // The guard is gone: the next fetch draws another.
                self.hold.remove_note(&self.cfg.dir, "fetch-guard");
                return Err(e);
            }
        };
        c.decrypt(Nonce::from_slice(&ANSWER_NONCE), raw.as_slice()).map_err(|_| anyhow!("open"))
    }

    pub(crate) async fn private_log(&self, station: StationId) -> Result<usize> {
        let card = self.log.lock().unwrap().get(&station).and_then(|l| l.announce.as_ref().map(|a| a.head)).unwrap_or(0);
        let mut added = 0;
        let mut answered = false;
        let mut tries = 0;
        // A page at a time, until the log reaches the card or a route gives nothing more:
        // pages are limited by bytes, so a long log comes in several.
        for _ in 0..32 {
            let from = self.store.lock().unwrap().logs.get(&station).map(|a| a.len()).unwrap_or(0);
            // A log this phone has never seen is asked for along two routes at once, and the
            // first to bring entries is taken: tuning a new station waits on the faster exit.
            let asked = if from == 0 { self.first_page(station).await } else { self.ask_privately(&Ask::Log { station, from }).await };
            let Ok(raw) = asked else {
                tries += 1;
                if tries >= 3 {
                    break;
                }
                continue;
            };
            let Ok(Response::Entries(list)) = postcard::from_bytes::<Response>(&raw) else { continue };
            let entries: Vec<SignedEntry> = list.iter().filter_map(|b| SignedEntry::from_bytes(b).ok()).collect();
            // An exit that found no carrier answers with nothing. For a log this phone has
            // never seen, that is no answer at all, so another exit is asked.
            if entries.is_empty() && from == 0 {
                tries += 1;
                if tries >= 3 {
                    break;
                }
                continue;
            }
            answered = true;
            let n = self.store.lock().unwrap().put_entries(station, entries)?;
            added += n;
            let now_len = self.store.lock().unwrap().logs.get(&station).map(|a| a.len()).unwrap_or(0);
            if n == 0 || now_len >= card {
                break;
            }
        }
        if answered {
            Ok(added)
        } else {
            Err(anyhow!("no private route answered"))
        }
    }

    /// The first page of a log along two routes at once: the first answer with entries in it.
    async fn first_page(&self, station: StationId) -> Result<Vec<u8>> {
        use n0_future::StreamExt;
        let one = || async {
            let raw = self.ask_privately(&Ask::Log { station, from: 0 }).await?;
            match postcard::from_bytes::<Response>(&raw) {
                Ok(Response::Entries(list)) if !list.is_empty() => Ok(raw),
                _ => Err(anyhow!("an exit with nothing")),
            }
        };
        let mut tries: n0_future::FuturesUnordered<_> = (0..2).map(|_| Box::pin(one())).collect();
        let mut last = Err(anyhow!("no route"));
        while let Some(r) = tries.next().await {
            if r.is_ok() {
                return r;
            }
            last = r;
        }
        last
    }

    pub(crate) async fn private_chunk(&self, station: StationId, hash: Hash, keep: bool) -> Result<Vec<u8>> {
        use n0_future::StreamExt;
        let name = pyrate_core::seal::name(&hash);
        let one = || async {
            let raw = tokio::time::timeout(Duration::from_secs(8), self.ask_privately(&Ask::Sealed { station, name })).await.map_err(|_| anyhow!("slow route"))??;
            match postcard::from_bytes::<Response>(&raw) {
                Ok(Response::Sealed(Some(s))) => pyrate_core::seal::unseal(&hash, &s).map(|b| (b, s)).ok_or_else(|| anyhow!("unopened")),
                _ => Err(anyhow!("no answer")),
            }
        };
        // A route that has not answered in two and a half seconds gets a second beside it, by
        // another exit, and the first answer wins: three at most. Each exit sees what it always
        // saw, a sealed name from a guard.
        let mut tries = n0_future::FuturesUnordered::new();
        tries.push(one());
        let mut started = 1;
        loop {
            let wait = tokio::time::sleep(Duration::from_millis(2_500));
            tokio::select! {
                r = tries.next() => match r {
                    Some(Ok((b, s))) => {
                        if !keep || self.store.lock().unwrap().put_sealed(&hash, &s).is_ok() {
                            return Ok(b);
                        }
                    }
                    Some(Err(_)) if started < 3 => {
                        tries.push(one());
                        started += 1;
                    }
                    Some(Err(_)) => {}
                    None => return Err(anyhow!("no private route answered")),
                },
                _ = wait, if started < 3 => {
                    tries.push(one());
                    started += 1;
                }
            }
            if tries.is_empty() {
                return Err(anyhow!("no private route answered"));
            }
        }
    }

    pub(crate) async fn private_token(&self, station: StationId, hour: u64, token: [u8; 32], nonce: u64) {
        let _ = self.ask_privately(&Ask::Token { station, hour, token, nonce }).await;
    }

    /// Answers a hop. Forward passes it on; Sealed is opened and answered; Stem moves on or spreads.
    pub(crate) async fn on_hop(self: &Arc<Self>, bytes: &[u8]) -> Vec<u8> {
        let Ok(hop) = postcard::from_bytes::<Hop>(bytes) else { return vec![] };
        if !self.relay_ok.load(std::sync::atomic::Ordering::Relaxed) {
            return vec![];
        }
        match hop {
            Hop::Forward { next, blob } => {
                let Ok(next) = EndpointId::from_bytes(&next) else { return vec![] };
                let Ok(inner) = postcard::from_bytes::<Hop>(&blob) else { return vec![] };
                if !matches!(inner, Hop::Sealed { .. }) {
                    return vec![];
                }
                self.hop(next, &inner).await.unwrap_or_default()
            }
            Hop::Sealed { eph, ct } => {
                let me = *self.endpoint.id().as_bytes();
                let shared = x_secret(&self.secret).diffie_hellman(&x25519_dalek::PublicKey::from(eph)).to_bytes();
                let c = cipher(&shared, &eph, &me);
                let Ok(plain) = c.decrypt(Nonce::from_slice(&ASK_NONCE), ct.as_slice()) else { return vec![] };
                let Ok(ask) = postcard::from_bytes::<Ask>(&plain) else { return vec![] };
                let resp = match ask {
                    Ask::Log { station, from } => {
                        let _ = self.sync_log_direct(station).await;
                        self.serve(Request::Log { station, from })
                    }
                    // An exit passes what it fetches straight through: nothing it relays for a stranger
                    // lands on its own device.
                    Ask::Chunk { station, hash } => Response::Chunk(self.chunk_direct(&station, &hash, false).await.ok()),
                    Ask::Sealed { station, name } => Response::Sealed(self.sealed_direct(&station, &name).await.ok()),
                    Ask::Token { station, hour, token, nonce } => {
                        self.accept_token(station, hour, token, nonce, true);
                        Response::Entries(vec![])
                    }
                    Ask::Stem { station, entries, chunks, announce } => {
                        let took = Box::pin(self.on_stem(station, entries, chunks, announce)).await;
                        Response::Sealed(took.then(|| vec![1]))
                    }
                };
                let out = postcard::to_stdvec(&resp).unwrap_or_default();
                c.encrypt(Nonce::from_slice(&ANSWER_NONCE), out.as_slice()).unwrap_or_default()
            }
            Hop::Stem { station, entries, chunks, announce, hops: _ } => {
                if self.on_stem(station, entries, chunks, announce).await { vec![1] } else { vec![] }
            }
        }
    }

    /// A stem's cargo, arrived plain or sealed: passed on, or kept and shown after a wait.
    /// True when this node took it.
    pub(crate) async fn on_stem(self: &Arc<Self>, station: StationId, entries: Vec<Vec<u8>>, chunks: Vec<(Hash, Vec<u8>)>, announce: Option<Announce>) -> bool {
        self.stems_in.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        // Cargo is checked before it is relayed or held: every entry signed and the station's
        // own, the card the station's, every chunk matching its hash. Junk goes no further.
        if !cargo_sound(&station, &entries, &chunks, &announce) {
            return false;
        }
        // A Lantern that carries only its own stations never ends a stranger's stem: it
        // hands the cargo on, so nothing of anyone else's lands on its disk. Nor does any
        // node end a stem for a station its owner chose not to carry.
        let declines = (self.cfg.lantern && !self.cfg.carry_all && !self.is_own(&station)) || !self.will_carry(&station, Some((&announce, &entries)));
        // Cargo that has been here before has gone round: it spreads now.
        let id = cargo_id(&station, &entries, &announce);
        let again = {
            let mut seen = self.stem_seen.lock().unwrap();
            let again = seen.contains(&id);
            if !again {
                seen.push_back(id);
                while seen.len() > STEM_MEMORY {
                    seen.pop_front();
                }
            }
            again
        };
        let pass = declines || (!again && rand::random::<f64>() > FLUFF);
        if pass {
            let mut nodes = self.known_nodes();
            nodes.shuffle(&mut rand::thread_rng());
            let next = Hop::Stem { station, entries: entries.clone(), chunks: chunks.clone(), announce: announce.clone(), hops: 0 };
            for n in nodes.into_iter().take(3) {
                // An empty answer is a node that declined the cargo.
                if self.hop(n, &next).await.is_ok_and(|b| !b.is_empty()) {
                    return true;
                }
            }
        }
        if declines {
            return false;
        }
        // The end of the stem waits a while before it shows anything, so the moment the
        // cargo appears says nothing of the moment its keeper sent it: twenty seconds to a
        // minute and a half for a recording, under a second and a half for the live air.
        let live = entries.iter().any(|b| SignedEntry::from_bytes(b).is_ok_and(|e| matches!(e.entry.body(), Body::Live { .. })));
        let wait = if live { rand::random::<u64>() % 1_500 } else { 20_000 + rand::random::<u64>() % 70_000 };
        // What waits is held in memory, so the waiting is budgeted: a few hundred stems and a
        // fixed number of bytes at once. Past that this node takes no more until some are shown.
        let bytes = entries.iter().map(Vec::len).sum::<usize>() + chunks.iter().map(|c| c.1.len()).sum::<usize>();
        let (count, budget) = if self.cfg.lantern { (&WAITING_LANTERN, &WAITING_BYTES_LANTERN) } else { (&WAITING, &WAITING_BYTES) };
        let Ok(slot) = count.try_acquire() else { return false };
        let Ok(held) = budget.try_acquire_many((bytes / 1024 + 1) as u32) else { return false };
        let me = self.clone();
        tokio::spawn(async move {
            let (_slot, _held) = (slot, held);
            tokio::time::sleep(Duration::from_millis(wait)).await;
            me.fluff(station, entries, chunks, announce).await;
        });
        true
    }

    /// The end of the stem: take the cargo into the hold and show it to the network as a carrier.
    async fn fluff(self: &Arc<Self>, station: StationId, entries: Vec<Vec<u8>>, chunks: Vec<(Hash, Vec<u8>)>, announce: Option<Announce>) {
        // A station is something only once its card, with its work, is known: entries for a
        // station with no card are taken by no one, so new stations are not free to make.
        let carded = announce.is_some() || self.log.lock().unwrap().get(&station).is_some_and(|l| l.announce.is_some());
        if !carded && self.store.lock().unwrap().is_carried(&station) {
            return;
        }
        let list: Vec<SignedEntry> = entries.iter().filter_map(|b| SignedEntry::from_bytes(b).ok()).collect();
        let _ = self.store.lock().unwrap().put_entries(station, list);
        {
            // Only chunks the station's signed log names are taken aboard: no one can fill a
            // carrier's disk with junk that merely matches its own hash.
            // The log is searched from its end for the chunks this stem carries, and no
            // further than 200,000 names back: a stem brings new sound, and building a set of
            // every chunk a long log names for each stem cost a carrier a second and tens of
            // megabytes a stem.
            let s = self.store.lock().unwrap();
            let carried: std::collections::HashSet<Hash> = chunks.iter().map(|(h, _)| *h).collect();
            let mut named: std::collections::HashSet<Hash> = std::collections::HashSet::new();
            let mut pics: std::collections::HashSet<Hash> = std::collections::HashSet::new();
            if let Some(a) = s.logs.get(&station) {
                let mut looked = 0usize;
                for e in a.entries().iter().rev() {
                    if looked > 200_000 || named.len() + pics.len() >= carried.len() {
                        break;
                    }
                    looked += 1;
                    match e.entry.body() {
                        Body::Track { chunks, .. } | Body::Release { chunks, .. } => {
                            looked += chunks.len();
                            named.extend(chunks.iter().map(|c| c.hash).filter(|h| carried.contains(h)));
                        }
                        Body::Live { chunk, .. } => {
                            looked += 1;
                            if carried.contains(&chunk.hash) {
                                named.insert(chunk.hash);
                            }
                        }
                        Body::Art { cover, banner, .. } => pics.extend(cover.iter().chain(banner.iter()).map(|a| a.hash).filter(|h| carried.contains(h))),
                        Body::TrackArt { art, .. } => {
                            if carried.contains(&art.hash) {
                                pics.insert(art.hash);
                            }
                        }
                        _ => {}
                    }
                }
            }
            for (h, b) in &chunks {
                if named.contains(h) {
                    let _ = s.put_chunk(h, b);
                } else if pics.contains(h) && b.len() <= pyrate_core::art::MAX_BYTES {
                    // Art reaches a Lantern's disk; a phone holds it in memory only, long
                    // enough for the Lanterns to take it aboard.
                    if self.cfg.lantern {
                        let _ = s.put_chunk(h, b);
                    } else {
                        self.art_mem.lock().unwrap().put(*h, b.clone());
                    }
                }
            }
        }
        // The card first, so whoever hears the carrier already knows the station.
        if let Some(a) = announce {
            self.on_gossip(Gossip::Station(a.clone()));
            self.broadcast(&Gossip::Station(a)).await;
        }
        let head = self.store.lock().unwrap().logs.get(&station).map(|a| a.len()).unwrap_or(0);
        let me = *self.endpoint.id().as_bytes();
        let carrier = Gossip::Carrier { station, node: me, head };
        self.on_gossip(carrier.clone());
        self.broadcast(&Gossip::carried(&self.secret, station, head, now_ms())).await;
        self.broadcast(&carrier).await;
    }

    /// The guard: the first hop of every stem, kept for weeks so an attacker cannot wait for a lucky draw.
    fn guard(&self) -> Option<EndpointId> {
        let old = self.cfg.dir.join("guard");
        let known = self.known_nodes();
        let held = self.hold.read_note(&self.cfg.dir, "stem-guard", Some(&old)).and_then(|b| String::from_utf8(b).ok());
        if let Some(g) = held.and_then(|s| s.trim().parse::<EndpointId>().ok()) {
            if known.contains(&g) {
                return Some(g);
            }
        }
        // Prefer anyone but the well-known Lanterns: a guard everyone shares would hold every
        // broadcaster's address in one place, and that place could be compelled to hand it over.
        let mut others: Vec<EndpointId> = known.iter().filter(|n| !self.doors.contains(n)).copied().collect();
        others.shuffle(&mut rand::thread_rng());
        // A peer met in an earlier run before a newcomer: a crowd of nodes that appeared today
        // cannot make one of their own a guard.
        let veterans: Vec<EndpointId> = others.iter().filter(|n| self.cfg.bootstrap.contains(n)).copied().collect();
        let g = veterans.first().copied().or_else(|| others.first().copied()).or_else(|| known.first().copied())?;
        let _ = self.hold.write_note(&self.cfg.dir, "stem-guard", g.to_string().as_bytes());
        Some(g)
    }

    /// Sends what other nodes have not yet landed of one of our stations, along the stem.
    /// Returns whether a node took the cargo (or there was nothing to send).
    pub(crate) async fn stem_push(self: &Arc<Self>, station: StationId, announce: Option<Announce>) -> bool {
        let landed = {
            let me = *self.endpoint.id().as_bytes();
            let log = self.log.lock().unwrap();
            log.get(&station).map(|l| l.carriers.iter().filter(|(n, _)| **n != me).map(|(_, (h, _))| *h).max().unwrap_or(0)).unwrap_or(0)
        };
        let (entries, chunks) = {
            let s = self.store.lock().unwrap();
            let Some(a) = s.logs.get(&station) else { return true };
            let fresh: Vec<&SignedEntry> = a.entries().iter().skip(landed as usize).take(32).collect();
            let mut chunks = Vec::new();
            for e in &fresh {
                let hashes: Vec<Hash> = match e.entry.body() {
                    Body::Track { chunks, .. } => chunks.iter().map(|c| c.hash).collect(),
                    Body::Live { chunk, .. } => vec![chunk.hash],
                    Body::Art { cover, banner, .. } => cover.iter().chain(banner.iter()).map(|r| r.hash).collect(),
                    Body::TrackArt { art, .. } => vec![art.hash],
                    _ => vec![],
                };
                for h in hashes {
                    if let Some(b) = s.get_chunk(&h) {
                        chunks.push((h, b));
                    }
                }
                if chunks.len() >= 64 {
                    break;
                }
            }
            (fresh.iter().filter_map(|e| e.to_bytes().ok()).collect::<Vec<_>>(), chunks)
        };
        // The end of the stem holds cargo a while before it shows it (see `fluff`), so what was
        // sent lately is not sent again for four minutes, and a card already sent goes once.
        // Sending it again sooner would cost a phone its data and make the guard, which would
        // know it, spread it at once. A resend goes round the guard.
        let now = now_ms();
        let card_at = announce.as_ref().map(|a| a.at_ms).unwrap_or(0);
        let (resend, entries, chunks) = {
            let sent = self.stem_sent.lock().unwrap();
            match sent.get(&station) {
                Some((from, n, at, _)) if *from == landed && *n == entries.len() && now.saturating_sub(*at) < 4 * 60_000 => (false, Vec::new(), Vec::new()),
                Some((from, n, _, _)) if *from == landed && *n == entries.len() => (true, entries, chunks),
                _ => (false, entries, chunks),
            }
        };
        // The card rides with every stem: a node takes a stranger's entries only for a station
        // whose card it holds, and the card's work is what makes stations cost something.
        let card_new = announce.is_some() && self.stem_sent.lock().unwrap().get(&station).is_none_or(|s| s.3 != card_at);
        if entries.is_empty() && !card_new {
            return true;
        }
        {
            let mut sent = self.stem_sent.lock().unwrap();
            let e = sent.entry(station).or_insert((landed, 0, 0, 0));
            if !entries.is_empty() {
                *e = (landed, entries.len(), now, e.3);
            }
            e.3 = card_at.max(e.3);
        }
        // Two hops out: the guard forwards a sealed blob to a second node, which opens it and
        // starts the stem from there. The guard sees this device and never the station; the
        // second node sees the station and never this device. A phone that never connects
        // directly sends its station no other way, resends included: with no second node, or
        // none that takes it, the cargo waits for the next tick.
        let strict = self.cfg.private && self.strict.load(std::sync::atomic::Ordering::Relaxed);
        if !resend || strict {
            if let Some(g) = self.guard() {
                let others: Vec<EndpointId> = self.known_nodes().into_iter().filter(|n| *n != g).collect();
                let x = (!others.is_empty()).then(|| others[rand::random::<usize>() % others.len()]);
                if let Some(x) = x {
                    let ask = Ask::Stem { station, entries: entries.clone(), chunks: chunks.clone(), announce: announce.clone() };
                    if let Ok((sealed, c)) = seal(x.as_bytes(), &ask) {
                        if let Ok(blob) = postcard::to_stdvec(&sealed) {
                            let r = tokio::time::timeout(Duration::from_secs(25), self.hop(g, &Hop::Forward { next: *x.as_bytes(), blob })).await;
                            if let Ok(Ok(raw)) = r {
                                let took = c.decrypt(Nonce::from_slice(&ANSWER_NONCE), raw.as_slice()).ok().and_then(|p| postcard::from_bytes::<Response>(&p).ok());
                                if matches!(took, Some(Response::Sealed(Some(_)))) {
                                    return true;
                                }
                            }
                        }
                    }
                }
            }
        }
        if strict {
            // Nothing went, so nothing counts as sent: the next tick sends it all again.
            self.stem_sent.lock().unwrap().remove(&station);
            say("stem: no two-hop route took the cargo; it waits for the next tick");
            return false;
        }
        let hop = Hop::Stem { station, entries, chunks, announce, hops: 0 };
        let mut tried = Vec::new();
        if let Some(g) = self.guard().filter(|_| !resend) {
            if tokio::time::timeout(Duration::from_secs(20), self.hop(g, &hop)).await.is_ok_and(|r| r.is_ok_and(|b| !b.is_empty())) {
                return true;
            }
            tried.push(g);
        }
        if resend {
            tried.extend(self.guard());
        }
        // The guard is down (or this is a resend): try another, never spread it ourselves.
        for n in self.known_nodes().into_iter().filter(|n| !tried.contains(n)).take(3) {
            if self.hop(n, &hop).await.is_ok_and(|b| !b.is_empty()) {
                if resend {
                    return true;
                }
                let _ = self.hold.write_note(&self.cfg.dir, "stem-guard", n.to_string().as_bytes());
                return true;
            }
        }
        self.stem_sent.lock().unwrap().remove(&station);
        say("stem: no node took the cargo; it waits for the next tick");
        false
    }
}
