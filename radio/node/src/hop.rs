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
use chacha20poly1305::{ChaCha20Poly1305, KeyInit, Nonce, aead::Aead};
use curve25519_dalek::edwards::CompressedEdwardsY;
use iroh::EndpointId;
use pyrate_core::{Body, Hash, SignedEntry, StationId};
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha512};

use crate::{Inner, net::{Announce, Gossip, Request, Response}, now_ms, say};

/// Chance a stem hop spreads the cargo rather than passing it on.
const FLUFF: f64 = 0.25;
const MAX_STEM: u8 = 8;

#[derive(Debug, Serialize, Deserialize)]
pub enum Hop {
    /// Pass `blob` to `next` and bring back its answer. The passer cannot read either.
    Forward { next: [u8; 32], blob: Vec<u8> },
    /// A request sealed to this node: an ephemeral X25519 key and the ciphertext.
    Sealed { eph: [u8; 32], ct: Vec<u8> },
    /// A station's new entries and chunks, travelling the stem.
    Stem { station: StationId, entries: Vec<Vec<u8>>, chunks: Vec<(Hash, Vec<u8>)>, announce: Option<Announce>, hops: u8 },
}

#[derive(Debug, Serialize, Deserialize)]
enum Ask {
    Log { station: StationId, from: u64 },
    Chunk { station: StationId, hash: Hash },
    /// A listener's hourly token, merged into the station's aboard sketch by the exit.
    Token { station: StationId, hour: u64, token: [u8; 32], nonce: u64 },
}

fn x_secret(seed: &[u8; 32]) -> x25519_dalek::StaticSecret {
    let h = Sha512::digest(seed);
    let mut b = [0u8; 32];
    b.copy_from_slice(&h[..32]);
    x25519_dalek::StaticSecret::from(b)
}

fn x_public(node: &[u8; 32]) -> Option<x25519_dalek::PublicKey> {
    let ed = CompressedEdwardsY(*node).decompress()?;
    Some(x25519_dalek::PublicKey::from(ed.to_montgomery().to_bytes()))
}

fn cipher(shared: &[u8; 32], eph: &[u8; 32], to: &[u8; 32]) -> ChaCha20Poly1305 {
    let mut m = Vec::with_capacity(96);
    m.extend_from_slice(shared);
    m.extend_from_slice(eph);
    m.extend_from_slice(to);
    ChaCha20Poly1305::new(&blake3::derive_key("pyrate/hop/v1", &m).into())
}

const ASK_NONCE: [u8; 12] = [0; 12];
const ANSWER_NONCE: [u8; 12] = [1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

fn seal(to: &[u8; 32], ask: &Ask) -> Result<(Hop, ChaCha20Poly1305)> {
    let to_x = x_public(to).ok_or_else(|| anyhow!("bad node key"))?;
    let mut seed = [0u8; 32];
    rand::Rng::fill(&mut rand::thread_rng(), &mut seed);
    let eph = x25519_dalek::StaticSecret::from(seed);
    let eph_pub = *x25519_dalek::PublicKey::from(&eph).as_bytes();
    let shared = eph.diffie_hellman(&to_x).to_bytes();
    let c = cipher(&shared, &eph_pub, to);
    let ct = c.encrypt(Nonce::from_slice(&ASK_NONCE), postcard::to_stdvec(ask)?.as_slice()).map_err(|_| anyhow!("seal"))?;
    Ok((Hop::Sealed { eph: eph_pub, ct }, c))
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

    pub(crate) fn known_nodes_pub(&self) -> Vec<EndpointId> {
        self.known_nodes()
    }

    async fn hop(&self, node: EndpointId, hop: &Hop) -> Result<Vec<u8>> {
        match self.ask(node, &Request::Hop(postcard::to_stdvec(hop)?)).await? {
            Response::Hop(b) => Ok(b),
            _ => Err(anyhow!("unexpected answer")),
        }
    }

    /// Two hops when the network allows, one when it has only one other node.
    async fn ask_privately(&self, ask: &Ask) -> Result<Vec<u8>> {
        let mut nodes = self.known_nodes();
        if nodes.is_empty() {
            return Err(anyhow!("no route"));
        }
        nodes.shuffle(&mut rand::thread_rng());
        let exit = nodes[0];
        let (sealed, c) = seal(exit.as_bytes(), ask)?;
        let raw = if nodes.len() >= 2 {
            let blob = postcard::to_stdvec(&sealed)?;
            self.hop(nodes[1], &Hop::Forward { next: *exit.as_bytes(), blob }).await?
        } else {
            self.hop(exit, &sealed).await?
        };
        c.decrypt(Nonce::from_slice(&ANSWER_NONCE), raw.as_slice()).map_err(|_| anyhow!("open"))
    }

    pub(crate) async fn private_log(&self, station: StationId) -> Result<usize> {
        for _ in 0..3 {
            let from = self.store.lock().unwrap().logs.get(&station).map(|a| a.len()).unwrap_or(0);
            let Ok(raw) = self.ask_privately(&Ask::Log { station, from }).await else { continue };
            let Ok(Response::Entries(list)) = postcard::from_bytes::<Response>(&raw) else { continue };
            let entries: Vec<SignedEntry> = list.iter().filter_map(|b| SignedEntry::from_bytes(b).ok()).collect();
            return self.store.lock().unwrap().put_entries(station, entries);
        }
        Err(anyhow!("no private route answered"))
    }

    pub(crate) async fn private_chunk(&self, station: StationId, hash: Hash, keep: bool) -> Result<Vec<u8>> {
        for _ in 0..3 {
            let Ok(raw) = self.ask_privately(&Ask::Chunk { station, hash }).await else { continue };
            if let Ok(Response::Chunk(Some(b))) = postcard::from_bytes::<Response>(&raw) {
                if blake3::hash(&b).as_bytes() != &hash {
                    continue;
                }
                if !keep || self.store.lock().unwrap().put_chunk(&hash, &b).is_ok() {
                    return Ok(b);
                }
            }
        }
        Err(anyhow!("no private route answered"))
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
                    Ask::Token { station, hour, token, nonce } => {
                        self.accept_token(station, hour, token, nonce, true);
                        Response::Entries(vec![])
                    }
                };
                let out = postcard::to_stdvec(&resp).unwrap_or_default();
                c.encrypt(Nonce::from_slice(&ANSWER_NONCE), out.as_slice()).unwrap_or_default()
            }
            Hop::Stem { station, entries, chunks, announce, hops } => {
                // A Lantern that carries only its own stations never ends a stranger's stem: it
                // hands the cargo on, so nothing of anyone else's lands on its disk.
                let declines = self.cfg.lantern && !self.cfg.carry_all && !self.is_own(&station);
                let pass = declines || (hops < MAX_STEM && rand::random::<f64>() > FLUFF);
                if pass {
                    let mut nodes = self.known_nodes();
                    nodes.shuffle(&mut rand::thread_rng());
                    let next = Hop::Stem { station, entries: entries.clone(), chunks: chunks.clone(), announce: announce.clone(), hops: hops + 1 };
                    for n in nodes.into_iter().take(3) {
                        // An empty answer is a node that declined the cargo.
                        if self.hop(n, &next).await.is_ok_and(|b| !b.is_empty()) {
                            return vec![1];
                        }
                    }
                }
                if declines {
                    return vec![];
                }
                self.fluff(station, entries, chunks, announce).await;
                vec![1]
            }
        }
    }

    /// The end of the stem: take the cargo into the hold and show it to the network as a carrier.
    async fn fluff(self: &Arc<Self>, station: StationId, entries: Vec<Vec<u8>>, chunks: Vec<(Hash, Vec<u8>)>, announce: Option<Announce>) {
        let list: Vec<SignedEntry> = entries.iter().filter_map(|b| SignedEntry::from_bytes(b).ok()).collect();
        let _ = self.store.lock().unwrap().put_entries(station, list);
        {
            // Only chunks the station's signed log names are taken aboard: no one can fill a
            // carrier's disk with junk that merely matches its own hash.
            let s = self.store.lock().unwrap();
            let named: std::collections::HashSet<Hash> = s.chunks_of(&station).into_iter().collect();
            let pics: std::collections::HashSet<Hash> = s.logs.get(&station).map(|a| a.art_hashes()).unwrap_or_default().into_iter().collect();
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
        let head = self.store.lock().unwrap().logs.get(&station).map(|a| a.len()).unwrap_or(0);
        let me = *self.endpoint.id().as_bytes();
        let carrier = Gossip::Carrier { station, node: me, head };
        self.on_gossip(carrier.clone());
        self.broadcast(&carrier).await;
        if let Some(a) = announce {
            self.on_gossip(Gossip::Station(a.clone()));
            self.broadcast(&Gossip::Station(a)).await;
        }
    }

    /// The guard: the first hop of every stem, kept for weeks so an attacker cannot wait for a lucky draw.
    fn guard(&self) -> Option<EndpointId> {
        let path = self.cfg.dir.join("guard");
        let known = self.known_nodes();
        if let Some(g) = std::fs::read_to_string(&path).ok().and_then(|s| s.trim().parse::<EndpointId>().ok()) {
            if known.contains(&g) {
                return Some(g);
            }
        }
        // Prefer anyone but the well-known Lanterns: a guard everyone shares would hold every
        // broadcaster's address in one place, and that place could be compelled to hand it over.
        let mut others: Vec<EndpointId> = known.iter().filter(|n| !self.cfg.bootstrap.contains(n)).copied().collect();
        others.shuffle(&mut rand::thread_rng());
        let g = others.first().copied().or_else(|| known.first().copied())?;
        let _ = std::fs::write(&path, g.to_string());
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
        if entries.is_empty() && announce.is_none() {
            return true;
        }
        let hop = Hop::Stem { station, entries, chunks, announce, hops: 0 };
        let mut tried = Vec::new();
        if let Some(g) = self.guard() {
            if tokio::time::timeout(Duration::from_secs(20), self.hop(g, &hop)).await.is_ok_and(|r| r.is_ok_and(|b| !b.is_empty())) {
                return true;
            }
            tried.push(g);
        }
        // The guard is down: try another, never spread it ourselves.
        for n in self.known_nodes().into_iter().filter(|n| !tried.contains(n)).take(3) {
            if self.hop(n, &hop).await.is_ok_and(|b| !b.is_empty()) {
                let _ = std::fs::write(self.cfg.dir.join("guard"), n.to_string());
                return true;
            }
        }
        say("stem: no node took the cargo; it waits for the next tick");
        false
    }
}
