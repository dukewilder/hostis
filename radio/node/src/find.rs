//! Finding the network with no box in charge.
//!
//! A node starts from what it already holds: the peers it met before (sealed in its hold), the
//! peers named in links it was handed, the network's own address book (carriers' signed harbour
//! notes) and the doors it was built with, HOSTIS's harbours among them. When none of those
//! answers, it climbs further, one rung at a time and only while it is alone:
//!
//! 1. the links and remembered peers again, which costs nothing outside;
//! 2. a seed list of volunteers' Lanterns, signed by two of the three release keys, fetched from
//!    any of several mirrors (or kept from the Releases station or an earlier fetch);
//! 3. the mainline BitTorrent DHT, where Lanterns leave signed notes of where they are reached.
//!
//! Phones and app computers only look up on the DHT, from a client in read-only mode that is
//! built for the lookup and dropped after it, so no DHT node ever takes their address into its
//! table and nothing they do writes their id anywhere. Lanterns that do not run dark publish. A
//! note in the DHT names a Lantern and its relay and nothing else: no station, no listener.
//! The keys they sit under are drawn from the day and a slot number, the same for everyone, so
//! anyone may write there; each note carries its own Lantern's signature, and a reader takes
//! only notes that are signed, fresh and name a relay it may use. No rights reserved.

use std::{sync::Arc, time::Duration};

use anyhow::{Result, anyhow};
use futures_lite::StreamExt;
use iroh::EndpointId;
use pyrate_core::StationKey;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{Inner, net, now_ms, say};

/// Where a seed list is fetched from when no list is held yet. A list names the mirrors the next
/// one may come from, and those are tried too. PYRATE_SEED_MIRRORS (commas) replaces these.
pub const SEED_MIRRORS: &[&str] = &["https://raw.githubusercontent.com/dukewilder/hostis/radio/radio/seeds.json"];

/// How many entries of a seed list are taken.
const SEEDS_MAX: usize = 48;
/// How big a fetched list may be.
const SEEDS_BYTES: usize = 256 * 1024;

/// One door of a list: an id, and the relay it is reached through if named.
pub fn parse_door(s: &str) -> Option<(EndpointId, Option<String>)> {
    let s = s.trim();
    let (id, relay) = s.split_once('@').map(|(a, b)| (a, Some(b.to_string()))).unwrap_or((s, None));
    let id = id.parse::<EndpointId>().ok()?;
    let relay = relay.filter(|r| r.len() <= 256 && net::relay_public(r));
    Some((id, relay))
}

/// The doors a trusted seed list names. Nothing for a list the keys do not stand behind.
pub fn seeds_of(keys: &[[u8; 32]], doc: &Value) -> Vec<(EndpointId, Option<String>)> {
    if doc["pyrate"] != "seeds" || !pyrate_core::papers::trusted(keys, doc) {
        return Vec::new();
    }
    doc["seeds"].as_array().into_iter().flatten().filter_map(|s| s.as_str()).filter_map(parse_door).take(SEEDS_MAX).collect()
}

/// The mirrors a list names, each an https address (or http on a private test network).
fn mirrors_of(doc: &Value) -> Vec<String> {
    doc["mirrors"].as_array().into_iter().flatten().filter_map(|m| m.as_str()).filter(|m| m.len() <= 512 && (m.starts_with("https://") || (m.starts_with("http://") && net::relay_public(m)))).map(String::from).take(8).collect()
}

/// One from each list in turn, then the next from each, with no id twice.
pub fn interleave<T: PartialEq + Copy>(lists: &[Vec<T>]) -> Vec<T> {
    let mut out = Vec::new();
    let longest = lists.iter().map(Vec::len).max().unwrap_or(0);
    for i in 0..longest {
        for l in lists {
            if let Some(x) = l.get(i).filter(|x| !out.contains(*x)) {
                out.push(*x);
            }
        }
    }
    out
}

// ---- The DHT -------------------------------------------------------------------------------

/// Slots a day. A Lantern writes to two of them, a reader reads them all.
pub const DHT_SLOTS: u64 = 8;
/// A mutable item's value may be no longer than this (BEP 44).
const DHT_VALUE: usize = 1000;
/// How long a Lantern's note in the DHT is believed.
const DHT_FRESH_MS: u64 = 6 * 3_600_000;

/// A Lantern's note in the DHT: its id, its relay, when, and its own signature over them.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DhtNote {
    pub node: [u8; 32],
    pub relay: String,
    pub at_ms: u64,
    pub sig: Vec<u8>,
}

fn dht_message(node: &[u8; 32], relay: &str, at_ms: u64) -> Vec<u8> {
    let mut m = b"pyrate/dht-seed/v1\0".to_vec();
    m.extend_from_slice(node);
    m.extend_from_slice(&at_ms.to_le_bytes());
    m.extend_from_slice(relay.as_bytes());
    m
}

impl DhtNote {
    pub fn new(secret: &[u8; 32], relay: &str, at_ms: u64) -> Self {
        let key = StationKey::from_seed(*secret);
        let node = key.id().0;
        DhtNote { node, relay: relay.into(), at_ms, sig: key.sign(&dht_message(&node, relay, at_ms)).to_vec() }
    }

    /// The id and relay, if the note is its node's own, fresh, and names a relay this node uses.
    pub fn checked(&self, now: u64) -> Option<(EndpointId, String)> {
        if self.relay.len() > 256 || now.saturating_sub(self.at_ms) > DHT_FRESH_MS || self.at_ms.saturating_sub(now) > 10 * 60_000 {
            return None;
        }
        let sig: [u8; 64] = self.sig.as_slice().try_into().ok()?;
        pyrate_core::StationId(self.node).verify(&dht_message(&self.node, &self.relay, self.at_ms), &sig).ok()?;
        if !net::relay_public(&self.relay) {
            return None;
        }
        Some((EndpointId::from_bytes(&self.node).ok()?, self.relay.clone()))
    }
}

/// The key of a slot on a day: drawn from public words alone, so every node finds the same.
pub fn slot_key(day: u64, slot: u64) -> mainline::SigningKey {
    let mut m = day.to_le_bytes().to_vec();
    m.extend_from_slice(&slot.to_le_bytes());
    mainline::SigningKey::from_bytes(&blake3::derive_key("pyrate/dht-rendezvous/v1", &m))
}

/// The slots a reader reads now: today's, and yesterday's in the first hours of the day.
pub fn slots_to_read(now: u64) -> Vec<mainline::SigningKey> {
    let day = now / 86_400_000;
    let mut out: Vec<_> = (0..DHT_SLOTS).map(|s| slot_key(day, s)).collect();
    if now % 86_400_000 < DHT_FRESH_MS && day > 0 {
        out.extend((0..DHT_SLOTS).map(|s| slot_key(day - 1, s)));
    }
    out
}

/// The two slots a Lantern writes to this hour, drawn from its id so Lanterns spread.
pub fn slots_to_write(node: &[u8; 32], now: u64) -> Vec<mainline::SigningKey> {
    let mut m = node.to_vec();
    m.extend_from_slice(&(now / 3_600_000).to_le_bytes());
    let first = u64::from_le_bytes(blake3::hash(&m).as_bytes()[..8].try_into().unwrap()) % DHT_SLOTS;
    let day = now / 86_400_000;
    vec![slot_key(day, first), slot_key(day, (first + 1) % DHT_SLOTS)]
}

/// The notes in a slot's value that hold, freshest first.
pub fn read_slot(value: &[u8], now: u64) -> Vec<DhtNote> {
    let mut notes: Vec<DhtNote> = postcard::from_bytes::<Vec<DhtNote>>(value).unwrap_or_default().into_iter().filter(|n| n.checked(now).is_some()).collect();
    notes.sort_by_key(|n| std::cmp::Reverse(n.at_ms));
    notes.dedup_by_key(|n| n.node);
    notes
}

/// A slot's value with `mine` in it: the others that still hold, freshest first, as many as fit.
pub fn merge_slot(old: &[u8], mine: DhtNote, now: u64) -> Vec<u8> {
    let mut notes = vec![mine.clone()];
    notes.extend(read_slot(old, now).into_iter().filter(|n| n.node != mine.node));
    loop {
        let v = postcard::to_stdvec(&notes).unwrap_or_default();
        if v.len() <= DHT_VALUE || notes.len() <= 1 {
            return v;
        }
        notes.pop();
    }
}

/// Where the DHT is reached, or None when this node must not use it: PYRATE_DHT=0, or a private
/// test network (PYRATE_RELAYS) with no DHT of its own named in PYRATE_DHT_BOOTSTRAP.
pub fn dht_bootstrap() -> Option<Option<Vec<String>>> {
    let var = |k: &str| std::env::var(k).unwrap_or_default();
    dht_bootstrap_from(&var("PYRATE_DHT"), &var("PYRATE_DHT_BOOTSTRAP"), &var("PYRATE_RELAYS"))
}

fn dht_bootstrap_from(switch: &str, own: &str, relays: &str) -> Option<Option<Vec<String>>> {
    if switch == "0" {
        return None;
    }
    let own: Vec<String> = own.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
    if !own.is_empty() {
        return Some(Some(own));
    }
    if !relays.trim().is_empty() {
        return None;
    }
    Some(None)
}

/// A DHT client in read-only mode, for one round of work. It never becomes a server: it is
/// dropped long before the quarter hour after which the crate would think of it.
async fn dht_client(boot: &Option<Vec<String>>) -> Result<mainline::async_dht::AsyncDht> {
    let mut b = mainline::Dht::builder();
    if let Some(list) = boot {
        b.bootstrap(list);
        if list.iter().all(|a| a.parse::<std::net::SocketAddr>().is_ok_and(|s| s.ip().is_loopback())) {
            b.bind_address(std::net::Ipv4Addr::LOCALHOST);
        }
    }
    let dht = b.build().map_err(|e| anyhow!("dht: {e}"))?.as_async();
    if !tokio::time::timeout(Duration::from_secs(30), dht.bootstrapped()).await.unwrap_or(false) {
        return Err(anyhow!("the DHT did not answer"));
    }
    if dht.info().await.server_mode() {
        return Err(anyhow!("the DHT client came up as a server"));
    }
    Ok(dht)
}

/// Every Lantern the DHT's slots name now, as checked.
pub async fn dht_lookup(boot: &Option<Vec<String>>) -> Result<Vec<(EndpointId, String)>> {
    let dht = dht_client(boot).await?;
    let now = now_ms();
    let mut set = tokio::task::JoinSet::new();
    for k in slots_to_read(now) {
        let dht = dht.clone();
        set.spawn(async move {
            let key = k.verifying_key().to_bytes();
            // Every copy any node holds, not only the newest: a vandal who overwrites a slot
            // reaches only the nodes it wrote to.
            let mut s = dht.get_mutable(&key, None, None);
            let mut out = Vec::new();
            let _ = tokio::time::timeout(Duration::from_secs(25), async {
                while let Some(item) = s.next().await {
                    out.extend(read_slot(item.value(), now));
                }
            })
            .await;
            out
        });
    }
    let mut found: Vec<DhtNote> = Vec::new();
    while let Some(r) = set.join_next().await {
        found.extend(r.unwrap_or_default());
    }
    found.sort_by_key(|n| std::cmp::Reverse(n.at_ms));
    found.dedup_by_key(|n| n.node);
    Ok(found.iter().filter_map(|n| n.checked(now)).take(SEEDS_MAX).collect())
}

/// Leaves this Lantern's note in its two slots for the hour. Lanterns only.
pub async fn dht_publish(boot: &Option<Vec<String>>, secret: &[u8; 32], relay: &str) -> Result<usize> {
    let dht = dht_client(boot).await?;
    let now = now_ms();
    let mine = DhtNote::new(secret, relay, now);
    let mut wrote = 0;
    for k in slots_to_write(&mine.node, now) {
        for _ in 0..3 {
            let key = k.verifying_key().to_bytes();
            let old = tokio::time::timeout(Duration::from_secs(25), dht.get_mutable_most_recent(&key, None)).await.ok().flatten();
            let (value, seq, cas) = match &old {
                Some(o) => (merge_slot(o.value(), mine.clone(), now), o.seq() + 1, Some(o.seq())),
                None => (merge_slot(&[], mine.clone(), now), 1, None),
            };
            let item = mainline::MutableItem::new(k.clone(), &value, seq, None);
            match tokio::time::timeout(Duration::from_secs(30), dht.put_mutable(item, cas)).await {
                Ok(Ok(_)) => {
                    wrote += 1;
                    break;
                }
                // Someone wrote between the read and the write: read again.
                Ok(Err(mainline::errors::PutMutableError::Concurrency(_))) => continue,
                _ => break,
            }
        }
    }
    Ok(wrote)
}

// ---- The ladder ----------------------------------------------------------------------------

impl Inner {
    /// The mirrors to fetch a seed list from: PYRATE_SEED_MIRRORS, or the built-in ones and those
    /// the list held now names, in a fresh order each time.
    fn seed_mirrors(&self) -> Vec<String> {
        let own: Vec<String> = std::env::var("PYRATE_SEED_MIRRORS").unwrap_or_default().split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
        let mut out = if own.is_empty() { SEED_MIRRORS.iter().map(|s| s.to_string()).collect() } else { own };
        if let Some(kept) = self.kept_seeds() {
            out.extend(mirrors_of(&kept));
        }
        out.sort();
        out.dedup();
        rand::seq::SliceRandom::shuffle(out.as_mut_slice(), &mut rand::thread_rng());
        out.truncate(8);
        out
    }

    /// The seed list this node holds, if the keys stand behind it.
    pub(crate) fn kept_seeds(&self) -> Option<Value> {
        self.hold.read_note(&self.cfg.dir, "seeds", None).and_then(|t| serde_json::from_slice::<Value>(&t).ok()).filter(|d| !seeds_of(&self.cfg.release_keys, d).is_empty())
    }

    /// Keeps a list if it is trusted and newer than the one held. True when it was kept.
    pub(crate) fn keep_seeds(&self, doc: &Value) -> bool {
        if seeds_of(&self.cfg.release_keys, doc).is_empty() {
            return false;
        }
        if self.kept_seeds().is_some_and(|k| k["at"].as_u64() >= doc["at"].as_u64()) {
            return false;
        }
        self.hold.write_note(&self.cfg.dir, "seeds", doc.to_string().as_bytes()).is_ok()
    }

    /// One fetch of a seed list from the mirrors, through Tor when this node goes through Tor.
    /// The first list the keys stand behind ends it. A list signed by fewer than two keys is
    /// refused and said so; an older list than the one held is passed over.
    async fn seeds_fetch(self: &Arc<Self>) -> Vec<(EndpointId, Option<String>)> {
        if self.cfg.release_keys.is_empty() {
            return Vec::new();
        }
        // A dark node with no tunnel to fetch through stays dark.
        if self.dark && self.proxy.is_none() {
            return self.kept_seeds().map(|d| seeds_of(&self.cfg.release_keys, &d)).unwrap_or_default();
        }
        let tls = match iroh::tls::CaTlsConfig::system().client_config(Arc::new(rustls::crypto::aws_lc_rs::default_provider())) {
            Ok(c) => c,
            Err(e) => {
                say(&format!("seed list: {e}"));
                return Vec::new();
            }
        };
        for url in self.seed_mirrors() {
            let local = url.strip_prefix("http://").and_then(|r| r.split(['/', ':']).next()).is_some_and(|h| h == "127.0.0.1" || h == "localhost" || h == "[::1]");
            let mut b = reqwest::Client::builder().tls_backend_preconfigured(tls.clone()).timeout(Duration::from_secs(20)).redirect(reqwest::redirect::Policy::limited(3)).user_agent("Mozilla/5.0");
            if let Some(p) = self.proxy.as_deref() {
                match reqwest::Proxy::all(p) {
                    Ok(p) => b = b.proxy(p),
                    Err(_) => continue,
                }
            } else if local {
                b = b.no_proxy();
            }
            let Ok(client) = b.build() else { continue };
            let Ok(resp) = client.get(&url).send().await else { continue };
            if !resp.status().is_success() || resp.content_length().is_some_and(|n| n as usize > SEEDS_BYTES) {
                continue;
            }
            let Ok(body) = resp.bytes().await else { continue };
            if body.len() > SEEDS_BYTES {
                continue;
            }
            let Ok(doc) = serde_json::from_slice::<Value>(&body) else { continue };
            if doc["pyrate"] != "seeds" {
                continue;
            }
            if !pyrate_core::papers::trusted(&self.cfg.release_keys, &doc) {
                say("a seed list was refused: it needs two of the three release signatures");
                continue;
            }
            if self.keep_seeds(&doc) {
                say("a new seed list, signed by the release keys, kept");
            }
            break;
        }
        self.kept_seeds().map(|d| seeds_of(&self.cfg.release_keys, &d)).unwrap_or_default()
    }

    /// Peers found by a rung of the ladder: into the address book with their relays, and asked
    /// into the Log. Their cards are asked for at once so the dial fills.
    pub(crate) async fn take_found(self: &Arc<Self>, found: Vec<(EndpointId, Option<String>)>, from: &str) -> usize {
        let me = self.endpoint.id();
        let mut ids = Vec::new();
        for (id, relay) in found {
            if id == me || ids.contains(&id) {
                continue;
            }
            if let Some(r) = relay.as_deref() {
                if let Ok(url) = r.parse::<iroh::RelayUrl>() {
                    self.book.add_endpoint_info(iroh::EndpointAddr { id, addrs: [iroh::TransportAddr::Relay(url)].into_iter().collect() });
                    self.met.lock().unwrap().insert(id, r.to_string());
                }
            }
            self.found.lock().unwrap().insert(*id.as_bytes());
            ids.push(id);
        }
        if ids.is_empty() {
            return 0;
        }
        say(&format!("{} peers to try, from {from}", ids.len()));
        let tx = self.gossip_tx.lock().unwrap().clone();
        if let Some(tx) = tx {
            let _ = tx.join_peers(ids.clone()).await;
        }
        for id in ids.iter().take(4) {
            self.cards_asked.lock().unwrap().remove(id.as_bytes());
            let (i2, id) = (self.clone(), *id);
            tokio::spawn(async move { i2.cards_from(id).await });
        }
        ids.len()
    }

    fn alone(&self) -> bool {
        self.neighbours.lock().unwrap().is_empty()
    }

    /// The ladder, climbed only while this node has no neighbour in the Log: links and
    /// remembered peers, then a seed list, then the DHT, then round again, more slowly each time.
    pub(crate) async fn find_loop(self: Arc<Self>) {
        let wait = std::env::var("PYRATE_FIND_WAIT_S").ok().and_then(|v| v.parse::<u64>().ok()).unwrap_or(30).max(2);
        let boot = dht_bootstrap();
        let mut rung = 0u32;
        tokio::time::sleep(Duration::from_secs(wait)).await;
        loop {
            if !self.alone() {
                rung = 0;
                tokio::time::sleep(Duration::from_secs(wait)).await;
                continue;
            }
            let (found, from) = match rung % 3 {
                0 => {
                    let mut f: Vec<(EndpointId, Option<String>)> = self.links.lock().unwrap().iter().map(|(i, r)| (*i, Some(r.clone()))).collect();
                    f.extend(self.met.lock().unwrap().iter().take(24).map(|(i, r)| (*i, Some(r.clone()))));
                    (f, "links and remembered peers")
                }
                1 => (self.seeds_fetch().await, "a seed list"),
                // Never from a node running dark: a lookup leaves from its own address.
                _ if self.dark => (Vec::new(), "the DHT"),
                _ => match &boot {
                    Some(b) => match dht_lookup(b).await {
                        Ok(f) => (f.into_iter().map(|(i, r)| (i, Some(r))).collect(), "the DHT"),
                        Err(e) => {
                            say(&format!("DHT lookup: {e}"));
                            (Vec::new(), "the DHT")
                        }
                    },
                    None => (Vec::new(), "the DHT"),
                },
            };
            rung += 1;
            self.take_found(found, from).await;
            // Each time round the ladder waits twice as long, up to half an hour.
            let round = (rung / 3).min(6);
            tokio::time::sleep(Duration::from_secs((wait << round).min(1800))).await;
        }
    }

    /// A Lantern not running dark leaves a note in the DHT soon after it is homed on a relay,
    /// then every half hour (PYRATE_DHT_EVERY_S), so a node with nothing else can find it.
    pub(crate) async fn dht_loop(self: Arc<Self>) {
        let Some(boot) = dht_bootstrap() else { return };
        if !self.cfg.lantern || self.dark {
            return;
        }
        let every = std::env::var("PYRATE_DHT_EVERY_S").ok().and_then(|v| v.parse::<u64>().ok()).unwrap_or(1800).max(5);
        tokio::time::sleep(Duration::from_secs(every.min(60))).await;
        loop {
            if let Some(relay) = self.public_relay() {
                match dht_publish(&boot, &self.secret, &relay).await {
                    Ok(n) if n > 0 => say(&format!("left a note in the DHT, in {n} slots")),
                    Ok(_) => say("the DHT took no note this time"),
                    Err(e) => say(&format!("DHT: {e}")),
                }
            }
            let jitter = rand::random::<u64>() % (every / 5 + 1);
            tokio::time::sleep(Duration::from_secs(every + jitter)).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dht_note_names_a_lantern_and_nothing_else() {
        let secret = [7u8; 32];
        let now = 1_800_000_000_000;
        let n = DhtNote::new(&secret, "https://relay.example.org/", now);
        let (id, relay) = n.checked(now).expect("a fresh note holds");
        assert_eq!(id.as_bytes(), &StationKey::from_seed(secret).id().0);
        assert_eq!(relay, "https://relay.example.org/");
        // What goes into the DHT is the id, the relay, the time and the signature: no station.
        let bytes = merge_slot(&[], n.clone(), now);
        let back: Vec<DhtNote> = postcard::from_bytes(&bytes).unwrap();
        assert_eq!(back, vec![n.clone()]);
        assert!(bytes.len() < 200, "{} bytes", bytes.len());
        // A changed relay, an old note and a note from the future are refused.
        let mut forged = n.clone();
        forged.relay = "https://elsewhere.example/".into();
        assert!(forged.checked(now).is_none());
        assert!(n.checked(now + DHT_FRESH_MS + 1).is_none());
        assert!(n.checked(now - 11 * 60_000).is_none());
        // A relay on a private address is refused off a test network.
        assert!(DhtNote::new(&secret, "https://10.0.0.1/", now).checked(now).is_none());
    }

    #[test]
    fn a_slot_keeps_the_freshest_that_fit() {
        let now = 1_800_000_000_000;
        let mut v = Vec::new();
        for i in 0..20u8 {
            v = merge_slot(&v, DhtNote::new(&[i + 1; 32], "https://relay.example.org/", now + i as u64), now + 20);
            assert!(v.len() <= DHT_VALUE);
        }
        let notes = read_slot(&v, now + 20);
        assert!(notes.len() >= 5, "{} notes fit", notes.len());
        assert_eq!(notes[0].node, StationKey::from_seed([20; 32]).id().0, "the newest first");
        // Writing again replaces this Lantern's own note, it does not add a second.
        let again = merge_slot(&v, DhtNote::new(&[20; 32], "https://relay.example.org/", now + 30), now + 30);
        assert_eq!(read_slot(&again, now + 30).iter().filter(|n| n.node == notes[0].node).count(), 1);
        // Junk in a slot is read as nothing.
        assert!(read_slot(b"not a list of notes", now).is_empty());
    }

    #[test]
    fn slots_are_the_same_for_everyone_and_move_by_day() {
        let now = 1_800_000_000_000;
        assert_eq!(slot_key(5, 1).verifying_key().to_bytes(), slot_key(5, 1).verifying_key().to_bytes());
        assert_ne!(slot_key(5, 1).verifying_key().to_bytes(), slot_key(6, 1).verifying_key().to_bytes());
        let read: Vec<[u8; 32]> = slots_to_read(now).iter().map(|k| k.verifying_key().to_bytes()).collect();
        for k in slots_to_write(&[9; 32], now) {
            assert!(read.contains(&k.verifying_key().to_bytes()), "a reader reads every slot a writer writes");
        }
    }

    #[test]
    fn a_seed_list_names_doors() {
        let k: Vec<StationKey> = (0..3).map(|_| StationKey::generate().unwrap()).collect();
        let ids: Vec<[u8; 32]> = k.iter().map(|k| k.id().0).collect();
        let door = StationKey::generate().unwrap().id().to_hex();
        let mut doc = serde_json::json!({ "pyrate": "seeds", "at": 1, "seeds": [format!("{door}@https://relay.example.org/"), "junk"], "mirrors": ["https://m.example/seeds.json", "ftp://no"] });
        pyrate_core::papers::sign(&k[0], 0, &mut doc).unwrap();
        assert!(seeds_of(&ids, &doc).is_empty(), "one signature is refused");
        pyrate_core::papers::sign(&k[2], 2, &mut doc).unwrap();
        let s = seeds_of(&ids, &doc);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].1.as_deref(), Some("https://relay.example.org/"));
        assert_eq!(mirrors_of(&doc), vec!["https://m.example/seeds.json".to_string()]);
    }

    #[test]
    fn no_one_source_comes_first() {
        assert_eq!(interleave(&[vec![1, 2, 3], vec![9], vec![2, 7]]), vec![1, 9, 2, 7, 3]);
        assert_eq!(interleave::<u8>(&[vec![], vec![]]), Vec::<u8>::new());
    }

    #[test]
    fn a_private_test_network_keeps_off_the_real_dht() {
        assert!(dht_bootstrap_from("", "", "http://127.0.0.1:3340").is_none());
        assert_eq!(dht_bootstrap_from("", "127.0.0.1:6881", "http://127.0.0.1:3340"), Some(Some(vec!["127.0.0.1:6881".to_string()])));
        assert!(dht_bootstrap_from("0", "127.0.0.1:6881", "").is_none());
        assert_eq!(dht_bootstrap_from("", "", ""), Some(None), "the real DHT, from its usual routers");
    }
}
