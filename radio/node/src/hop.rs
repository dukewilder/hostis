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

/// The most of a log one answer along a route carries.
const HOP_PAGE: usize = 256 * 1024;

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
    /// The middle of a three-hop route: pass `blob` (a `Hop::Sealed` for `next`) to `next`, and
    /// bring its answer back under this hop's own seal. The middle sees the radio before it and
    /// the one after, never the requester, and cannot read what is asked. Older nodes do not
    /// know this ask, so middles are drawn only from radios that have said they are of the crew.
    Relay { next: [u8; 32], blob: Vec<u8> },
    /// To the radio that leads to a deck: open `lead`, which was sealed to this radio and names
    /// the listener behind the deck, and pass `blob` (a `Hop::Sealed` for the deck's key) to it.
    Lead { lead: Vec<u8>, blob: Vec<u8> },
}

impl Ask {
    fn station(&self) -> Option<StationId> {
        match self {
            Ask::Log { station, .. } | Ask::Chunk { station, .. } | Ask::Token { station, .. } | Ask::Sealed { station, .. } | Ask::Stem { station, .. } => Some(*station),
            Ask::Relay { .. } | Ask::Lead { .. } => None,
        }
    }
}

/// The longest a middle or a way in waits on the radio after it.
const PASS_MS: u64 = 8_000;
/// How long a radio must have been known before it may be a first hop.
pub(crate) const NEWCOMER_MS: u64 = 10 * 60_000;
/// How long a radio's word that it is of the crew is believed: it says so every two minutes, so
/// one silent for four has most likely gone, and a route through it would only time out.
pub(crate) const CREW_FRESH_MS: u64 = 4 * 60_000;

/// One three-hop request, built by the requester: what goes to the first hop, and the ciphers
/// that open the answer, outermost first. `exit` is a radio's id and `ask` what it is asked; for
/// a deck, `lead` is the deck's lead and `intro` the radio it is sealed to, and the request goes
/// requester, first hop, middle, intro, deck.
///
/// What each sees: the first hop sees the requester and the middle, and a blob sealed to the
/// middle. The middle sees the first hop and the next radio (the exit, or the deck's way in),
/// and a blob sealed to it. Neither sees both who asks and what for. The answer comes back under
/// every hop's seal but the first's, so the bytes the first hop passes are not the bytes the
/// exit sent.
pub(crate) fn three_hops(middle: &[u8; 32], exit: &[u8; 32], ask_bytes: &[u8], deck: Option<(&[u8; 32], &[u8])>) -> Result<(Hop, Vec<ChaCha20Poly1305>)> {
    let bad = || anyhow!("bad node key");
    let mut ciphers = Vec::new();
    // Innermost: the ask, sealed to the exit (or to the deck's own key).
    let (to, inner) = match deck {
        None => {
            let (eph, ct, c) = pyrate_core::route::seal_to(exit, ask_bytes).ok_or_else(bad)?;
            ciphers.push(c);
            (*exit, postcard::to_stdvec(&Hop::Sealed { eph, ct })?)
        }
        Some((intro, lead)) => {
            let (eph, ct, c_deck) = pyrate_core::route::seal_to(exit, ask_bytes).ok_or_else(bad)?;
            let to_deck = postcard::to_stdvec(&Hop::Sealed { eph, ct })?;
            let (eph, ct, c_intro) = pyrate_core::route::seal_to(intro, &postcard::to_stdvec(&Ask::Lead { lead: lead.to_vec(), blob: to_deck })?).ok_or_else(bad)?;
            ciphers.push(c_deck);
            ciphers.push(c_intro);
            (*intro, postcard::to_stdvec(&Hop::Sealed { eph, ct })?)
        }
    };
    let (eph, ct, c_mid) = pyrate_core::route::seal_to(middle, &postcard::to_stdvec(&Ask::Relay { next: to, blob: inner })?).ok_or_else(bad)?;
    ciphers.push(c_mid);
    ciphers.reverse();
    Ok((Hop::Forward { next: *middle, blob: postcard::to_stdvec(&Hop::Sealed { eph, ct })? }, ciphers))
}

/// Opens a three-hop answer, seal by seal. None if any seal does not open, or a hop on the way
/// gave nothing back.
pub(crate) fn open_layers(ciphers: &[ChaCha20Poly1305], raw: &[u8]) -> Option<Vec<u8>> {
    let mut b = raw.to_vec();
    for c in ciphers {
        if b.is_empty() {
            return None;
        }
        b = pyrate_core::route::open_answer(c, &b)?;
    }
    Some(b)
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
    pub(crate) fn fetch_guard(&self, known: &[EndpointId]) -> Option<EndpointId> {
        let old = self.cfg.dir.join("fetch-guard");
        let held = self.hold.read_note(&self.cfg.dir, "fetch-guard", Some(&old)).and_then(|b| String::from_utf8(b).ok());
        if let Some(g) = held.and_then(|s| s.trim().parse::<EndpointId>().ok()) {
            if known.contains(&g) {
                return Some(g);
            }
        }
        // Not one that failed lately: a tab or a phone that relays for nobody turns every fetch away.
        let failed = self.failed.lock().unwrap().clone();
        let now = now_ms();
        // Never a newcomer. A first hop and a middle that work together learn who hears what,
        // and radios are cheap to make: someone who brings up a crowd of them would otherwise
        // soon hold both ends of somebody's route. A radio is a first hop only once this node
        // has known it for ten minutes, or met it in an earlier run; the middle is drawn from
        // any radio of the crew outside the guards. So a crowd that arrives today holds a
        // victim's middle now and then, and its first hop only after it has waited, and only
        // when the guard this node keeps has gone.
        let seasoned = self.seasoned();
        let mut others: Vec<EndpointId> = known.iter().filter(|n| !self.doors.contains(n) && seasoned(n) && !failed.get(n.as_bytes()).is_some_and(|t| now.saturating_sub(*t) < crate::net::FAILED_MS)).copied().collect();
        others.shuffle(&mut rand::thread_rng());
        // A peer met in an earlier run before a newcomer: a crowd of nodes that appeared today
        // cannot make one of their own a guard. Within each, one this node is talking to now
        // before one only heard of, so a new guard answers from its first fetch.
        others.sort_by_key(|n| (!self.cfg.bootstrap.contains(n), !self.live(n)));
        let g = others.first().copied().or_else(|| known.iter().find(|n| seasoned(n)).copied())?;
        let _ = self.hold.write_note(&self.cfg.dir, "fetch-guard", g.to_string().as_bytes());
        Some(g)
    }

    /// Whether a radio may be a first hop: a door or a peer met in an earlier run, or one this
    /// node has known for ten minutes.
    pub(crate) fn seasoned(&self) -> impl Fn(&EndpointId) -> bool + '_ {
        let seen = self.first_seen.lock().unwrap().clone();
        let now = now_ms();
        move |n: &EndpointId| self.cfg.bootstrap.contains(n) || self.doors.contains(n) || seen.get(n.as_bytes()).is_some_and(|t| now.saturating_sub(*t) >= NEWCOMER_MS)
    }

    /// The middle of a three-hop route: a radio of the crew heard from lately, drawn at random,
    /// never this node, never one of `not` (the first hop and the next radio), and never one of
    /// this node's guards, so a middle is never a radio that also sees this node's address.
    pub(crate) fn middle(&self, not: &[[u8; 32]]) -> Option<EndpointId> {
        let now = now_ms();
        let me = *self.endpoint.id().as_bytes();
        let guards: Vec<[u8; 32]> = ["fetch-guard", "stem-guard"]
            .iter()
            .filter_map(|k| self.hold.read_note(&self.cfg.dir, k, None).and_then(|b| String::from_utf8(b).ok()).and_then(|s| s.trim().parse::<EndpointId>().ok()))
            .map(|g| *g.as_bytes())
            .collect();
        let failed = self.failed.lock().unwrap().clone();
        let pool: Vec<[u8; 32]> = self
            .crew_radios
            .lock()
            .unwrap()
            .iter()
            .filter(|(n, (_, at))| now.saturating_sub(*at) < CREW_FRESH_MS && **n != me && !not.contains(n) && !guards.contains(n))
            .filter(|(n, _)| !failed.get(*n).is_some_and(|t| now.saturating_sub(*t) < crate::net::FAILED_MS))
            .map(|(n, _)| *n)
            .collect();
        pool.choose(&mut rand::thread_rng()).and_then(|n| EndpointId::from_bytes(n).ok())
    }

    /// A request to one of a station's decks, in three hops: requester, guard, middle, the
    /// deck's way in, the deck. The deck is the one ranked `rank` for `key` among those known
    /// (see `pyrate_core::route::keepers`), so every listener asks the same deck for the same
    /// chunk. Only with the switch on, and only when a middle can be drawn and the deck's way in
    /// is not this node's guard: a guard that was also the way in would see both ends.
    pub(crate) async fn ask_deck(&self, ask: &Ask, station: StationId, key: &[u8; 32], rank: usize) -> Result<Vec<u8>> {
        if !self.crew.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(anyhow!("crew off"));
        }
        let now = now_ms();
        let failed = self.failed.lock().unwrap().clone();
        let decks: Vec<([u8; 32], crate::net::DeckNote)> = self
            .log
            .lock()
            .unwrap()
            .get(&station)
            .map(|l| l.decks.iter().filter(|(d, n)| now.saturating_sub(n.at_ms) < crate::net::DECK_FRESH_MS && !failed.get(*d).is_some_and(|t| now.saturating_sub(*t) < 3 * 60_000)).map(|(d, n)| (*d, n.clone())).collect())
            .unwrap_or_default();
        let ids: Vec<[u8; 32]> = decks.iter().map(|d| d.0).collect();
        let pick = *pyrate_core::route::keepers(&ids, key, rank + 1).get(rank).ok_or_else(|| anyhow!("no deck"))?;
        let note = decks.into_iter().find(|d| d.0 == pick).map(|d| d.1).ok_or_else(|| anyhow!("no deck"))?;
        let nodes = self.known_nodes();
        let guard = self.fetch_guard(&nodes).ok_or_else(|| anyhow!("no guard"))?;
        if *guard.as_bytes() == note.intro {
            return Err(anyhow!("the guard leads to the deck"));
        }
        let middle = self.middle(&[*guard.as_bytes(), note.intro]).ok_or_else(|| anyhow!("no middle"))?;
        let (hop, ciphers) = three_hops(middle.as_bytes(), &pick, &postcard::to_stdvec(ask)?, Some((&note.intro, &note.lead)))?;
        let raw = tokio::time::timeout(Duration::from_millis(PASS_MS), self.hop(guard, &hop)).await.map_err(|_| anyhow!("slow deck"))??;
        open_layers(&ciphers, &raw).ok_or_else(|| {
            self.failed.lock().unwrap().insert(pick, now_ms());
            anyhow!("the deck gave nothing")
        })
    }

    /// Whether this node is talking to `n` now: a gossip neighbour or an open connection.
    fn live(&self, n: &EndpointId) -> bool {
        self.neighbours.lock().unwrap().contains(n.as_bytes()) || self.conns.lock().unwrap().get(n).is_some_and(|c| c.close_reason().is_none())
    }

    /// Two hops. One only when the network has a single other node and the owner has let the
    /// radio take weaker routes: a lone exit would see both who asks and what for.
    pub(crate) async fn ask_privately(&self, ask: &Ask) -> Result<Vec<u8>> {
        self.ask_route(ask, None).await
    }

    /// Exits for a request, as many as asked for and all different: nodes that have not failed
    /// lately first, then the rest, never one of `not`.
    fn exits(&self, nodes: &[EndpointId], not: &[EndpointId], n: usize) -> Vec<EndpointId> {
        self.exits_for(nodes, not, n, None)
    }

    /// As `exits`, with the nodes that carry `station` drawn first among those that have not
    /// failed: an exit that holds what is asked answers at once, where another fetches it first.
    fn exits_for(&self, nodes: &[EndpointId], not: &[EndpointId], n: usize, station: Option<StationId>) -> Vec<EndpointId> {
        let now = now_ms();
        let failed = self.failed.lock().unwrap().clone();
        let carrying: std::collections::HashSet<[u8; 32]> =
            station.and_then(|st| self.log.lock().unwrap().get(&st).map(|l| l.carriers.keys().copied().collect())).unwrap_or_default();
        let bad = |x: &EndpointId| failed.get(x.as_bytes()).is_some_and(|t| now.saturating_sub(*t) < crate::net::FAILED_MS);
        let mut tiers: [Vec<EndpointId>; 3] = [
            nodes.iter().filter(|x| !not.contains(x) && !bad(x) && carrying.contains(x.as_bytes())).copied().collect(),
            nodes.iter().filter(|x| !not.contains(x) && !bad(x) && !carrying.contains(x.as_bytes())).copied().collect(),
            nodes.iter().filter(|x| !not.contains(x) && bad(x)).copied().collect(),
        ];
        for t in tiers.iter_mut() {
            t.shuffle(&mut rand::thread_rng());
        }
        tiers.into_iter().flatten().take(n).collect()
    }

    /// Two hops by the guard and the given exit, or one drawn. An answer that does not open
    /// marks the exit as failing, so the next draw passes over it for a while. A guard this
    /// node is not talking to gets three seconds to answer a call before another is drawn, so
    /// a guard that has gone to sleep costs a moment, not the ten seconds of a slow route.
    async fn ask_route(&self, ask: &Ask, exit: Option<EndpointId>) -> Result<Vec<u8>> {
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
        if !self.live(&guard) && !matches!(tokio::time::timeout(Duration::from_secs(3), self.conn(guard)).await, Ok(Ok(_))) {
            self.hold.remove_note(&self.cfg.dir, "fetch-guard");
            return Err(anyhow!("the guard did not answer"));
        }
        let exit = match exit.filter(|e| *e != guard) {
            Some(e) => e,
            None => *self.exits_for(&nodes, &[guard], 1, ask.station()).first().ok_or_else(|| anyhow!("no route"))?,
        };
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
        // An empty answer is a guard that relays for nobody (a tab, a phone on mobile data), or
        // an exit that turned the hop away. Twice in a row with different exits, and the guard
        // is let go: the next fetch draws another, rather than every fetch failing on it.
        if raw.is_empty() {
            self.failed.lock().unwrap().insert(*exit.as_bytes(), now_ms());
            // A guard that has relayed before keeps its place: the exit said no.
            if self.relayed.lock().unwrap().contains(&guard) {
                return Err(anyhow!("turned away"));
            }
            if self.fetch_guard_misses.fetch_add(1, std::sync::atomic::Ordering::Relaxed) >= 1 {
                self.fetch_guard_misses.store(0, std::sync::atomic::Ordering::Relaxed);
                self.failed.lock().unwrap().insert(*guard.as_bytes(), now_ms());
                self.hold.remove_note(&self.cfg.dir, "fetch-guard");
            }
            return Err(anyhow!("turned away"));
        }
        self.fetch_guard_misses.store(0, std::sync::atomic::Ordering::Relaxed);
        self.relayed.lock().unwrap().insert(guard);
        c.decrypt(Nonce::from_slice(&ANSWER_NONCE), raw.as_slice()).map_err(|_| {
            self.failed.lock().unwrap().insert(*exit.as_bytes(), now_ms());
            anyhow!("open")
        })
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

    /// The first page of a log along four routes at once, through the guard to four different
    /// exits: the first answer with entries in it. The guard sees four sealed requests where it
    /// saw two; each exit sees what it always saw.
    async fn first_page(&self, station: StationId) -> Result<Vec<u8>> {
        use n0_future::StreamExt;
        let one = |exit: Option<EndpointId>| async move {
            let raw = self.ask_route(&Ask::Log { station, from: 0 }, exit).await?;
            match postcard::from_bytes::<Response>(&raw) {
                Ok(Response::Entries(list)) if !list.is_empty() => Ok(raw),
                _ => Err(anyhow!("an exit with nothing")),
            }
        };
        let nodes = self.known_nodes();
        let guard = if nodes.len() >= 2 { self.fetch_guard(&nodes) } else { None };
        let exits = guard.map(|g| self.exits_for(&nodes, &[g], 4, Some(station))).unwrap_or_default();
        let mut tries: n0_future::FuturesUnordered<_> = if exits.is_empty() {
            (0..2).map(|_| Box::pin(one(None))).collect()
        } else {
            exits.iter().map(|e| Box::pin(one(Some(*e)))).collect()
        };
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
        let open = move |raw: &[u8]| match postcard::from_bytes::<Response>(raw) {
            Ok(Response::Sealed(Some(s))) => pyrate_core::seal::unseal(&hash, &s).map(|b| (b, s)).ok_or_else(|| anyhow!("unopened")),
            _ => Err(anyhow!("no answer")),
        };
        // The first try goes to the crew's deck that keeps this chunk, when there is one, and
        // on to an ordinary exit at once if the deck has nothing.
        let one = |deck: bool| async move {
            if deck {
                if let Ok(r) = self.ask_deck(&Ask::Sealed { station, name }, station, &name, 0).await.and_then(|raw| open(&raw)) {
                    return Ok(r);
                }
            }
            let raw = tokio::time::timeout(Duration::from_secs(8), self.ask_privately(&Ask::Sealed { station, name })).await.map_err(|_| anyhow!("slow route"))??;
            open(&raw)
        };
        // A route that has not answered in two and a half seconds gets a second beside it, by
        // another exit, and the first answer wins: three at most. Each exit sees what it always
        // saw, a sealed name from a guard.
        let mut tries = n0_future::FuturesUnordered::new();
        tries.push(one(true));
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
                        tries.push(one(false));
                        started += 1;
                    }
                    Some(Err(_)) => {}
                    None => return Err(anyhow!("no private route answered")),
                },
                _ = wait, if started < 3 => {
                    tries.push(one(false));
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
                // Never to itself: a forward that loops back costs a connection and carries nothing.
                if !matches!(inner, Hop::Sealed { .. }) || next == self.endpoint.id() {
                    return vec![];
                }
                // A radio that could not be reached a moment ago is not tried again at once: tabs
                // close without a word, and every forward to one gone would hold a slot for seconds.
                if self.failed.lock().unwrap().get(next.as_bytes()).is_some_and(|t| now_ms().saturating_sub(*t) < 60_000) {
                    return vec![];
                }
                // As long as a stem's second node may take to land its cargo, and no longer.
                // Only a radio that could not be reached at all is passed over; a slow one is not.
                let r = match tokio::time::timeout(Duration::from_secs(25), self.hop(next, &inner)).await {
                    Ok(r) => {
                        if r.is_err() {
                            self.failed.lock().unwrap().insert(*next.as_bytes(), now_ms());
                        }
                        r
                    }
                    Err(_) => Err(anyhow!("timed out")),
                };
                match r {
                    Ok(b) => {
                        self.relayed_bytes.fetch_add(b.len() as u64, std::sync::atomic::Ordering::Relaxed);
                        b
                    }
                    Err(e) => {
                        // Said at most once a minute, so a Lantern's log shows a hop it cannot make.
                        static LAST: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
                        let now = now_ms();
                        if now.saturating_sub(LAST.swap(now, std::sync::atomic::Ordering::Relaxed)) > 60_000 {
                            tracing::warn!("a hop to {} failed: {e}", next.fmt_short());
                            say(&format!("a hop to {} failed: {e}", next.fmt_short()));
                        }
                        vec![]
                    }
                }
            }
            Hop::Sealed { eph, ct } => {
                let me = *self.endpoint.id().as_bytes();
                let shared = x_secret(&self.secret).diffie_hellman(&x25519_dalek::PublicKey::from(eph)).to_bytes();
                let c = cipher(&shared, &eph, &me);
                let Ok(plain) = c.decrypt(Nonce::from_slice(&ASK_NONCE), ct.as_slice()) else { return vec![] };
                let Ok(ask) = postcard::from_bytes::<Ask>(&plain) else { return vec![] };
                if matches!(ask, Ask::Relay { .. } | Ask::Lead { .. }) {
                    return self.pass_on(ask, &c).await;
                }
                let resp = match ask {
                    // What is here goes back at once, and the exit catches up behind it: a listener
                    // tuning in waits on one answer, not on every carrier this exit could ask first.
                    // With nothing here past `from`, it asks its carriers for a few seconds first.
                    Ask::Log { station, from } => {
                        let have = self.store.lock().unwrap().logs.get(&station).map(|a| a.len()).unwrap_or(0);
                        if have > from {
                            self.catch_up(station);
                        } else {
                            let _ = tokio::time::timeout(Duration::from_secs(4), self.sync_log_direct(station)).await;
                        }
                        // A page along a route crosses two relays: a quarter of a megabyte at a
                        // time, so each answer is back in a second or two and the next follows.
                        match self.serve(Request::Log { station, from }) {
                            Response::Entries(mut list) => {
                                let mut sum = 0;
                                let keep = list.iter().take_while(|b| {
                                    sum += b.len();
                                    sum <= HOP_PAGE || sum == b.len()
                                }).count();
                                list.truncate(keep);
                                Response::Entries(list)
                            }
                            other => other,
                        }
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
                    Ask::Relay { .. } | Ask::Lead { .. } => return vec![],
                };
                let out = postcard::to_stdvec(&resp).unwrap_or_default();
                if matches!(resp, Response::Entries(_) | Response::Chunk(_) | Response::Sealed(Some(_))) {
                    self.served_bytes.fetch_add(out.len() as u64, std::sync::atomic::Ordering::Relaxed);
                }
                c.encrypt(Nonce::from_slice(&ANSWER_NONCE), out.as_slice()).unwrap_or_default()
            }
            Hop::Stem { station, entries, chunks, announce, hops: _ } => {
                if self.on_stem(station, entries, chunks, announce).await { vec![1] } else { vec![] }
            }
        }
    }

    /// The middle of a three-hop route, or the way in to a deck: the blob goes on to the radio
    /// named, and its answer comes back under this hop's seal. Only with the switch on. Never to
    /// this node itself, and only a sealed hop goes on, so no one can make a middle start a stem
    /// or a chain of forwards. An empty answer goes back empty, so the requester knows the route
    /// broke past here.
    async fn pass_on(self: &Arc<Self>, ask: Ask, c: &ChaCha20Poly1305) -> Vec<u8> {
        if !self.crew.load(std::sync::atomic::Ordering::Relaxed) {
            return vec![];
        }
        let me = *self.endpoint.id().as_bytes();
        let (next, blob) = match ask {
            Ask::Relay { next, blob } => (next, blob),
            Ask::Lead { lead, blob } => {
                let Some(l) = pyrate_core::route::open_lead(&self.secret, &me, &lead) else { return vec![] };
                // The listener is reached through the relay its lead names, and nowhere else.
                let Ok(url) = l.relay.parse::<iroh::RelayUrl>() else { return vec![] };
                if !crate::net::relay_public(&l.relay) {
                    return vec![];
                }
                if let Ok(id) = EndpointId::from_bytes(&l.to) {
                    self.book.add_endpoint_info(iroh::EndpointAddr { id, addrs: [iroh::TransportAddr::Relay(url)].into_iter().collect() });
                }
                (l.to, blob)
            }
            _ => return vec![],
        };
        if next == me {
            return vec![];
        }
        let Ok(next) = EndpointId::from_bytes(&next) else { return vec![] };
        let Ok(inner) = postcard::from_bytes::<Hop>(&blob) else { return vec![] };
        if !matches!(inner, Hop::Sealed { .. }) {
            return vec![];
        }
        if self.failed.lock().unwrap().get(next.as_bytes()).is_some_and(|t| now_ms().saturating_sub(*t) < 60_000) {
            return vec![];
        }
        let raw = match tokio::time::timeout(Duration::from_millis(PASS_MS), self.hop(next, &inner)).await {
            Ok(Ok(b)) if !b.is_empty() => b,
            Ok(Ok(_)) | Err(_) => return vec![],
            Ok(Err(_)) => {
                self.failed.lock().unwrap().insert(*next.as_bytes(), now_ms());
                return vec![];
            }
        };
        self.relayed_bytes.fetch_add(raw.len() as u64, std::sync::atomic::Ordering::Relaxed);
        pyrate_core::route::seal_answer(c, &raw)
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
        let failed = self.failed.lock().unwrap().clone();
        let now = now_ms();
        let mut others: Vec<EndpointId> = known.iter().filter(|n| !self.doors.contains(n) && !failed.get(n.as_bytes()).is_some_and(|t| now.saturating_sub(*t) < crate::net::FAILED_MS)).copied().collect();
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
                // Up to three second nodes a tick, each a different one: a node that declines
                // (a phone that relays for nobody, a tab that has gone) costs a moment, not the
                // six seconds to the next tick, and a live show does not fall behind.
                let mut others: Vec<EndpointId> = self.known_nodes().into_iter().filter(|n| *n != g).collect();
                others.shuffle(&mut rand::thread_rng());
                let mut guard_answered = false;
                for x in others.into_iter().take(3) {
                    let ask = Ask::Stem { station, entries: entries.clone(), chunks: chunks.clone(), announce: announce.clone() };
                    let Ok((sealed, c)) = seal(x.as_bytes(), &ask) else { continue };
                    let Ok(blob) = postcard::to_stdvec(&sealed) else { continue };
                    let r = tokio::time::timeout(Duration::from_secs(25), self.hop(g, &Hop::Forward { next: *x.as_bytes(), blob })).await;
                    let Ok(Ok(raw)) = r else { break };
                    // An empty answer through a guard that has relayed before is the second node
                    // turning the cargo away: another second node is tried. Through one that has
                    // not, it is the guard that relays for nobody.
                    if raw.is_empty() {
                        if self.relayed.lock().unwrap().contains(&g) {
                            guard_answered = true;
                            self.failed.lock().unwrap().insert(*x.as_bytes(), now_ms());
                            continue;
                        }
                        break;
                    }
                    self.relayed.lock().unwrap().insert(g);
                    guard_answered = true;
                    let took = c.decrypt(Nonce::from_slice(&ANSWER_NONCE), raw.as_slice()).ok().and_then(|p| postcard::from_bytes::<Response>(&p).ok());
                    if matches!(took, Some(Response::Sealed(Some(_)))) {
                        self.guard_misses.store(0, std::sync::atomic::Ordering::Relaxed);
                        return true;
                    }
                }
                // A guard that does not answer, or relays for nobody, twice in a row is let go:
                // the next tick draws another, rather than waiting on it for good.
                if !guard_answered || self.guard_misses.fetch_add(1, std::sync::atomic::Ordering::Relaxed) >= 1 {
                    self.guard_misses.store(0, std::sync::atomic::Ordering::Relaxed);
                    self.failed.lock().unwrap().insert(*g.as_bytes(), now_ms());
                    self.hold.remove_note(&self.cfg.dir, "stem-guard");
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

#[cfg(test)]
mod tests {
    use super::*;
    use pyrate_core::StationKey;

    fn radio(n: u8) -> ([u8; 32], [u8; 32]) {
        let seed = [n; 32];
        (seed, StationKey::from_seed(seed).id().0)
    }

    fn holds(hay: &[u8], needle: &[u8]) -> bool {
        hay.windows(needle.len()).any(|w| w == needle)
    }

    /// What a radio reads of a hop sealed to it.
    fn open(seed: &[u8; 32], id: &[u8; 32], hop: &[u8]) -> (Ask, ChaCha20Poly1305, Vec<u8>) {
        let Ok(Hop::Sealed { eph, ct }) = postcard::from_bytes::<Hop>(hop) else { panic!("not sealed") };
        let (plain, c) = pyrate_core::route::open_sealed(seed, id, &eph, &ct).expect("opens for its own radio");
        (postcard::from_bytes::<Ask>(&plain).unwrap(), c, plain)
    }

    #[test]
    fn a_first_hop_sees_neither_the_exit_nor_the_station() {
        let (requester, (_, middle), (exit_seed, exit)) = (radio(1).1, radio(2), radio(3));
        let station = StationId([9; 32]);
        let ask = Ask::Sealed { station, name: [7; 32] };
        let (hop, ciphers) = three_hops(&middle, &exit, &postcard::to_stdvec(&ask).unwrap(), None).unwrap();
        assert_eq!(ciphers.len(), 2);
        let Hop::Forward { next, blob } = hop else { panic!("a forward") };
        assert_eq!(next, middle, "the first hop sees only the middle");
        for secret in [&exit[..], &station.0[..], &[7u8; 32][..], &requester[..]] {
            assert!(!holds(&blob, secret), "the first hop's blob names nothing it should not");
        }
        // Not even the exit opens what the first hop passes on.
        let Ok(Hop::Sealed { eph, ct }) = postcard::from_bytes::<Hop>(&blob) else { panic!() };
        assert!(pyrate_core::route::open_sealed(&exit_seed, &exit, &eph, &ct).is_none());
    }

    #[test]
    fn a_middle_sees_the_exit_and_never_the_requester_or_the_ask() {
        let (requester, (mid_seed, middle), (exit_seed, exit)) = (radio(1).1, radio(2), radio(3));
        let station = StationId([9; 32]);
        let ask = Ask::Log { station, from: 12 };
        let (Hop::Forward { blob, .. }, ciphers) = three_hops(&middle, &exit, &postcard::to_stdvec(&ask).unwrap(), None).unwrap() else { panic!() };
        let (seen, c_mid, plain) = open(&mid_seed, &middle, &blob);
        let Ask::Relay { next, blob: inner } = seen else { panic!("a relay") };
        assert_eq!(next, exit);
        assert!(!holds(&plain, &requester) && !holds(&plain, &station.0), "the middle reads no requester and no station");
        // The exit reads the ask and nothing of who sent it.
        let (got, c_exit, plain) = open(&exit_seed, &exit, &inner);
        assert!(matches!(got, Ask::Log { from: 12, .. }));
        assert!(!holds(&plain, &requester) && !holds(&plain, &middle));
        // The answer comes back under both seals, and only the requester opens it.
        let answer = pyrate_core::route::seal_answer(&c_exit, b"entries");
        let passed = pyrate_core::route::seal_answer(&c_mid, &answer);
        assert!(!holds(&passed, &answer), "the first hop passes other bytes than the exit sent");
        assert_eq!(open_layers(&ciphers, &passed).as_deref(), Some(&b"entries"[..]));
        assert_eq!(open_layers(&ciphers, &[]), None, "a broken route opens to nothing");
    }

    #[test]
    fn a_deck_is_reached_through_its_way_in_and_named_to_no_one_else() {
        let ((mid_seed, middle), (intro_seed, intro), (deck_seed, deck), (_, listener)) = (radio(2), radio(4), radio(5), radio(6));
        let station = StationId([9; 32]);
        let lead = pyrate_core::route::make_lead(&intro, &pyrate_core::route::Lead { to: listener, relay: "https://relay.example/".into() }).unwrap();
        assert!(!holds(&lead, &listener), "the deck's note never names the listener");
        let ask = Ask::Sealed { station, name: [7; 32] };
        let (Hop::Forward { next, blob }, ciphers) = three_hops(&middle, &deck, &postcard::to_stdvec(&ask).unwrap(), Some((&intro, &lead))).unwrap() else { panic!() };
        assert_eq!(next, middle);
        assert!(!holds(&blob, &intro) && !holds(&blob, &deck));
        let (Ask::Relay { next, blob }, c_mid, plain) = open(&mid_seed, &middle, &blob) else { panic!() };
        assert_eq!(next, intro, "the middle sees the way in");
        assert!(!holds(&plain, &listener) && !holds(&plain, &station.0));
        let (Ask::Lead { lead, blob }, c_intro, plain) = open(&intro_seed, &intro, &blob) else { panic!() };
        assert!(!holds(&plain, &station.0), "the way in never reads the station");
        assert_eq!(pyrate_core::route::open_lead(&intro_seed, &intro, &lead).unwrap().to, listener);
        assert!(pyrate_core::route::open_lead(&mid_seed, &middle, &lead).is_none(), "only the way in reads the lead");
        let (got, c_deck, _) = open(&deck_seed, &deck, &blob);
        assert!(matches!(got, Ask::Sealed { name, .. } if name == [7; 32]));
        let raw = pyrate_core::route::seal_answer(&c_mid, &pyrate_core::route::seal_answer(&c_intro, &pyrate_core::route::seal_answer(&c_deck, b"sound")));
        assert_eq!(open_layers(&ciphers, &raw).as_deref(), Some(&b"sound"[..]));
    }

    #[test]
    fn every_listener_asks_the_same_deck_for_the_same_chunk() {
        let decks: Vec<[u8; 32]> = (0..8u8).map(|i| radio(10 + i).1).collect();
        let mut other = decks.clone();
        other.reverse();
        for n in 0..20u8 {
            let name = [n; 32];
            assert_eq!(pyrate_core::route::keepers(&decks, &name, 2), pyrate_core::route::keepers(&other, &name, 2));
        }
        // And the chunks are shared out among them.
        let firsts: std::collections::HashSet<[u8; 32]> = (0..64u8).map(|n| pyrate_core::route::keepers(&decks, &[n; 32], 1)[0]).collect();
        assert!(firsts.len() >= 6);
    }

    #[test]
    fn crew_and_deck_notes_hold_only_under_their_own_keys() {
        let now = 10_000_000;
        let g = Gossip::crew(&[3; 32], "https://relay.example/", now);
        assert!(g.crew_checked(now).is_some());
        let Gossip::Crew { node, at_ms, sig, .. } = g else { panic!() };
        assert!(Gossip::Crew { node, relay: "https://elsewhere.example/".into(), at_ms, sig }.crew_checked(now).is_none());
        let d = Gossip::deck(&[5; 32], StationId([9; 32]), 40, radio(4).1, vec![1, 2, 3], now);
        assert!(d.deck_checked(now).is_some());
        assert!(d.deck_checked(now + crate::net::DECK_FRESH_MS + 1).is_none(), "a quiet deck has closed");
        let Gossip::Deck { deck, station, head, lead, at_ms, sig, .. } = d else { panic!() };
        // Someone who swaps the way in for its own cannot keep the deck's signature.
        assert!(Gossip::Deck { deck, station, head, intro: radio(7).1, lead, at_ms, sig }.deck_checked(now).is_none());
    }
}
