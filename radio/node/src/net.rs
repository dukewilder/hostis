//! What passes between nodes.
//!
//! Requests go over `pyrate/0`: one request per stream, one answer back.
//! The Log goes over gossip, on one topic every node joins.

use pyrate_core::{Hash, StationId, StationKey};
use serde::{Deserialize, Serialize};

pub const ALPN: &[u8] = b"pyrate/1";
pub const MAX_FRAME: usize = 16 * 1024 * 1024;
/// Leading zero bits a station's proof of work must show. About a second on a phone, once.
pub const POW_BITS: u32 = 24;
/// Work behind each listener token: a second or two on a phone, once an hour.
pub const TOKEN_BITS: u32 = 20;

pub fn log_topic() -> [u8; 32] {
    *blake3::hash(b"pyrate/log/v2").as_bytes()
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Request {
    /// Signed entries from `from` onward, as postcard bytes.
    Log { station: StationId, from: u64 },
    Chunk { hash: Hash },
    /// A hop of a private route or a stem. See `hop.rs`.
    Hop(Vec<u8>),
    /// The last lines of a station's chatter that a node in the room still holds.
    Chatter { station: StationId },
    /// A chunk, sealed, by the name it is filed under. The carrier never learns its address
    /// and never opens it. See pyrate_core::seal.
    SealedChunk { name: Hash },
    /// A flock's host's clock and station, for a phone holding the flock's code.
    Flock { code: [u8; 8] },
    /// A pocket station's log, sealed, asked for by the tag its key draws: never by its id.
    PocketLog { tag: Hash, from: u64 },
    /// The last lines of a pocket's chatter, sealed, asked for by its tag. Answered with
    /// `Response::Chatter`. Older nodes do not know this request and answer nothing.
    PocketChatter { tag: Hash },
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Response {
    Entries(Vec<Vec<u8>>),
    Chunk(Option<Vec<u8>>),
    Hop(Vec<u8>),
    Chatter(Vec<Vec<u8>>),
    Sealed(Option<Vec<u8>>),
    Flock { now_ms: u64, station: Option<StationId> },
}

/// The signed face of a station's card. Fields are only ever added at the end: an older
/// node reads the ones it knows, ignores the rest, and still checks the signature over every
/// byte, so new versions can say more without splitting the Log.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Card {
    pub station: StationId,
    pub head: u64,
    pub name: String,
    pub about: String,
    pub port: Option<String>,
    pub live: bool,
    pub at_ms: u64,
    pub pow: u64,
    /// When the station first went on the air, from its first entry.
    pub since: u64,
    /// When a deputy signs: the master-signed entry appointing it.
    pub proof: Option<Vec<u8>>,
    /// The station's cover and page colour.
    pub cover: Option<[u8; 32]>,
    pub accent: Option<[u8; 3]>,
    /// The station's own word on what it is: its kinds, and whether it calls itself mature.
    pub kinds: Vec<String>,
    pub mature: bool,
    /// Where tips go, paid from the listener's own wallet.
    pub tip: Option<String>,
    /// Whether the station calls itself explicit. Written after the fields above, so an
    /// older app reads the card as it always has; a card from before it reads as not explicit.
    #[serde(skip)]
    pub explicit: bool,
}

const CARD_DOMAIN: &[u8] = b"pyrate/announce/v2\0";

impl Card {
    pub fn sign(self, key: &StationKey) -> Announce {
        let mut raw = postcard::to_stdvec(&self).unwrap_or_default();
        raw.extend(postcard::to_stdvec(&self.explicit).unwrap_or_default());
        let mut m = CARD_DOMAIN.to_vec();
        m.extend_from_slice(&raw);
        Announce { sig: key.sign(&m), card: self, raw }
    }
}

/// A station's card in the Log, signed by the station.
#[derive(Clone, Debug)]
pub struct Announce {
    pub card: Card,
    raw: Vec<u8>,
    pub sig: [u8; 64],
}

impl std::ops::Deref for Announce {
    type Target = Card;
    fn deref(&self) -> &Card {
        &self.card
    }
}

#[derive(Serialize, Deserialize)]
struct AnnounceWire {
    #[serde(with = "serde_bytes_vec")]
    card: Vec<u8>,
    #[serde(with = "serde_bytes64")]
    sig: [u8; 64],
}

mod serde_bytes_vec {
    use serde::{Deserialize, Deserializer, Serializer};
    pub fn serialize<S: Serializer>(v: &Vec<u8>, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_bytes(v)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        Vec::<u8>::deserialize(d)
    }
}

impl Serialize for Announce {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        AnnounceWire { card: self.raw.clone(), sig: self.sig }.serialize(s)
    }
}

impl<'de> Deserialize<'de> for Announce {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let w = AnnounceWire::deserialize(d)?;
        // Trailing fields from later versions are ignored here and kept in `raw`.
        let (mut card, rest): (Card, &[u8]) = postcard::take_from_bytes(&w.card).map_err(serde::de::Error::custom)?;
        card.explicit = postcard::take_from_bytes::<bool>(rest).map(|(e, _)| e).unwrap_or(false);
        Ok(Announce { card, raw: w.card, sig: w.sig })
    }
}

impl Announce {
    fn message(&self) -> Vec<u8> {
        let mut m = CARD_DOMAIN.to_vec();
        m.extend_from_slice(&self.raw);
        m
    }

    pub fn verify(&self) -> bool {
        if !pow_ok(&self.station, self.pow) {
            return false;
        }
        let signer = match &self.proof {
            None => self.station,
            Some(bytes) => {
                let Ok(p) = pyrate_core::SignedEntry::from_bytes(bytes) else { return false };
                let pyrate_core::Body::Deputy { key, .. } = &p.entry.body else { return false };
                if p.entry.station != self.station || p.verify().is_err() {
                    return false;
                }
                StationId(*key)
            }
        };
        signer.verify(&self.message(), &self.sig).is_ok()
    }
}

fn pow_hash(station: &StationId, nonce: u64) -> u32 {
    let mut h = blake3::Hasher::new();
    h.update(b"pyrate/pow/v1");
    h.update(&station.0);
    h.update(&nonce.to_le_bytes());
    let b = h.finalize();
    u32::from_be_bytes(b.as_bytes()[..4].try_into().unwrap()).leading_zeros()
}

/// How much work a nonce shows for a station: the leading zero bits of its hash.
pub fn pow_bits(station: &StationId, nonce: u64) -> u32 {
    pow_hash(station, nonce)
}

/// A better nonce than `from` shows, searched over `span` nonces after `start`: what a
/// station does when the Log is full and a newcomer must outwork the weakest card in it.
pub fn pow_improve(station: &StationId, from: u64, start: u64, span: u64) -> Option<u64> {
    let have = pow_hash(station, from);
    (start..start.saturating_add(span)).find(|n| pow_hash(station, *n) > have)
}

pub fn pow_ok(station: &StationId, nonce: u64) -> bool {
    pow_hash(station, nonce) >= POW_BITS
}

/// The work a station does once, so raising ten thousand junk stations costs ten thousand times as much.
pub fn pow_find(station: &StationId) -> u64 {
    (0u64..).find(|n| pow_ok(station, *n)).unwrap()
}

fn token_work(station: &StationId, hour: u64, token: &[u8; 32], nonce: u64) -> u32 {
    let mut h = blake3::Hasher::new();
    h.update(b"pyrate/token/v1");
    h.update(&station.0);
    h.update(&hour.to_le_bytes());
    h.update(token);
    h.update(&nonce.to_le_bytes());
    u32::from_be_bytes(h.finalize().as_bytes()[..4].try_into().unwrap()).leading_zeros()
}

pub fn token_ok(station: &StationId, hour: u64, token: &[u8; 32], nonce: u64) -> bool {
    token_work(station, hour, token, nonce) >= TOKEN_BITS
}

pub fn token_find(station: &StationId, hour: u64, token: &[u8; 32]) -> u64 {
    (0u64..).find(|n| token_ok(station, hour, token, *n)).unwrap()
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Gossip {
    Station(Announce),
    /// A node saying it carries a station, and how far its copy of the log reaches.
    Carrier { station: StationId, node: [u8; 32], head: u64 },
    /// Listener tokens for one station in one hour, each with its proof of work. Every node
    /// checks them and counts for itself; nobody's sketch is taken on trust.
    Aboard { station: StationId, hour: u64, tokens: Vec<([u8; 32], u64)> },
    /// A carrier saying which relay it can be reached through, signed with its own node key, so
    /// nodes find each other inside the network without asking anyone's directory. Older nodes
    /// do not know this message and pass over it.
    Harbour { node: [u8; 32], relay: String, at_ms: u64, sig: Vec<u8> },
}

fn harbour_message(node: &[u8; 32], relay: &str, at_ms: u64) -> Vec<u8> {
    let mut m = b"pyrate/harbour/v1\0".to_vec();
    m.extend_from_slice(node);
    m.extend_from_slice(&at_ms.to_le_bytes());
    m.extend_from_slice(relay.as_bytes());
    m
}

impl Gossip {
    /// This node's relay, signed with the node's own secret.
    pub fn harbour(secret: &[u8; 32], relay: &str, at_ms: u64) -> Self {
        let key = StationKey::from_seed(*secret);
        let node = key.id().0;
        Gossip::Harbour { node, relay: relay.into(), at_ms, sig: key.sign(&harbour_message(&node, relay, at_ms)).to_vec() }
    }

    /// For a harbour note: the node and its relay, if the node itself signed it near `now`.
    pub fn harbour_checked(&self, now: u64) -> Option<([u8; 32], String)> {
        let Gossip::Harbour { node, relay, at_ms, sig } = self else { return None };
        let sig: [u8; 64] = sig.as_slice().try_into().ok()?;
        if relay.len() > 256 || at_ms.abs_diff(now) > 15 * 60_000 {
            return None;
        }
        StationId(*node).verify(&harbour_message(node, relay, *at_ms), &sig).ok()?;
        Some((*node, relay.clone()))
    }
}

/// Copies of every chunk the fleet aims to keep between its ballast holds.
pub const SHARE_COPIES: f64 = 8.0;

/// Where a sealed chunk falls for a node, from 0 to 1: the node holds it as ballast when this
/// is under its share. Anyone can work it out from the node's id and the chunk's sealed name,
/// so a listener asks the likely holders first, and nobody had to agree on anything.
pub fn share_score(node: &[u8; 32], name: &Hash) -> f64 {
    let mut h = blake3::Hasher::new_derive_key("pyrate/share/v1");
    h.update(node);
    h.update(name);
    let b = h.finalize();
    u64::from_le_bytes(b.as_bytes()[..8].try_into().unwrap()) as f64 / u64::MAX as f64
}

/// The share of the Log each node holds so the whole keeps about SHARE_COPIES copies:
/// everything while the fleet is small, less as it grows.
pub fn share_of(fleet: usize) -> f64 {
    // PYRATE_SHARE_COPIES: fewer copies, so a test fleet of five already splits the Log.
    let copies = std::env::var("PYRATE_SHARE_COPIES").ok().and_then(|v| v.parse().ok()).unwrap_or(SHARE_COPIES);
    (copies / fleet.max(1) as f64).min(1.0)
}

/// A node's ballast from the chunks on offer, (sealed name, length): those whose score falls
/// under the share, lowest first, up to the budget. A node with less room keeps the start of
/// the same list, so a listener asking by score still finds what it kept.
pub fn pick_shares(node: &[u8; 32], chunks: &[(Hash, u64)], share: f64, budget: u64) -> Vec<(Hash, u64)> {
    let mut v: Vec<(f64, Hash, u64)> = chunks.iter().map(|(n, l)| (share_score(node, n), *n, *l)).filter(|(s, _, _)| *s < share).collect();
    v.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut used = 0u64;
    v.into_iter()
        .take_while(|(_, _, l)| {
            used += l;
            used <= budget
        })
        .map(|(_, n, l)| (n, l))
        .collect()
}

/// Carriers a station's listing keeps: the freshest claims, the oldest giving way.
pub const MAX_CARRIERS: usize = 64;
/// How long a carrier that failed to answer is tried only after the others.
pub const FAILED_MS: u64 = 10 * 60_000;

/// Takes in a carrier's claim. Anyone can say they carry anything, so a claim never reaches
/// past the station's own card, and a listing keeps only the freshest few.
pub fn note_carrier(carriers: &mut std::collections::HashMap<[u8; 32], (u64, u64)>, node: [u8; 32], head: u64, card_head: Option<u64>, now: u64) {
    carriers.insert(node, (card_head.map_or(head, |c| head.min(c)), now));
    while carriers.len() > MAX_CARRIERS {
        let Some(oldest) = carriers.iter().min_by_key(|(_, (_, seen))| *seen).map(|(n, _)| *n) else { break };
        carriers.remove(&oldest);
    }
}

/// Carriers in the order to ask them: the furthest copy of the log first (no claim counting
/// past the card), the freshest word next, and any that failed lately last.
pub fn rank_carriers(carriers: &std::collections::HashMap<[u8; 32], (u64, u64)>, me: &[u8; 32], card_head: Option<u64>, failed: impl Fn(&[u8; 32]) -> bool) -> Vec<[u8; 32]> {
    let mut v: Vec<(bool, u64, u64, [u8; 32])> = carriers
        .iter()
        .filter(|(n, _)| *n != me)
        .map(|(n, (h, s))| (failed(n), card_head.map_or(*h, |c| (*h).min(c)), *s, *n))
        .collect();
    v.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)).then(b.2.cmp(&a.2)));
    v.into_iter().map(|x| x.3).collect()
}

/// HyperLogLog with 256 registers. 256 bytes on the wire, about 6.5 percent error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sketch(pub [u8; 256]);

impl Default for Sketch {
    fn default() -> Self {
        Self([0; 256])
    }
}

impl Sketch {
    pub fn add(&mut self, token: &[u8; 32]) {
        let t = blake3::derive_key("pyrate/sketch/v1", token);
        let i = t[0] as usize;
        let w = u64::from_le_bytes(t[1..9].try_into().unwrap());
        let rank = (w.leading_zeros() + 1).min(32) as u8;
        self.0[i] = self.0[i].max(rank);
    }

    pub fn merge(&mut self, other: &[u8]) {
        for (a, b) in self.0.iter_mut().zip(other) {
            *a = (*a).max(*b);
        }
    }

    pub fn estimate(&self) -> u64 {
        let m = 256f64;
        let alpha = 0.7213 / (1.0 + 1.079 / m);
        let sum: f64 = self.0.iter().map(|r| 2f64.powi(-(*r as i32))).sum();
        let raw = alpha * m * m / sum;
        let zeros = self.0.iter().filter(|r| **r == 0).count();
        if raw <= 2.5 * m && zeros > 0 {
            (m * (m / zeros as f64).ln()).round() as u64
        } else {
            raw.round() as u64
        }
    }
}

/// "About 40". Counts are approximate, and the app says so.
pub fn round_aboard(n: u64) -> u64 {
    match n {
        0..=10 => n,
        11..=100 => (n + 2) / 5 * 5,
        101..=1000 => (n + 5) / 10 * 10,
        _ => (n + 50) / 100 * 100,
    }
}

mod serde_bytes64 {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(sig: &[u8; 64], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_bytes(sig)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<[u8; 64], D::Error> {
        let v: Vec<u8> = Deserialize::deserialize(d)?;
        v.as_slice().try_into().map_err(|_| serde::de::Error::invalid_length(v.len(), &"64 bytes"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shares_split_the_log_and_shrink_with_the_fleet() {
        let chunks: Vec<(Hash, u64)> = (0..4000u32).map(|i| (*blake3::hash(&i.to_le_bytes()).as_bytes(), 1000)).collect();
        assert_eq!(share_of(3), 1.0);
        let fleet = 80usize;
        let p = share_of(fleet);
        let nodes: Vec<[u8; 32]> = (0..fleet as u32).map(|i| *blake3::hash(&(i + 9_000_000).to_le_bytes()).as_bytes()).collect();
        let mut copies = std::collections::HashMap::<Hash, usize>::new();
        for n in &nodes {
            let mine = pick_shares(n, &chunks, p, u64::MAX);
            // Each node holds about a tenth of the Log, not all of it.
            assert!(mine.len() > 250 && mine.len() < 550, "{}", mine.len());
            for (c, _) in mine {
                *copies.entry(c).or_default() += 1;
            }
        }
        let mean = copies.values().sum::<usize>() as f64 / chunks.len() as f64;
        assert!((7.0..9.0).contains(&mean), "{mean}");
        // Nearly every chunk is held somewhere (a Poisson eight leaves about 1 in 3000 bare).
        assert!(copies.len() >= chunks.len() - 10);
        // A smaller budget keeps the start of the same list.
        let full = pick_shares(&nodes[0], &chunks, p, u64::MAX);
        let half = pick_shares(&nodes[0], &chunks, p, (full.len() as u64 / 2) * 1000);
        assert_eq!(half[..], full[..half.len()]);
    }


    /// A stranger's bytes, random and mutated from real ones, into every decoder the wire
    /// reaches: none may panic, hang or allocate without bound. A small fuzzer that runs with
    /// the ordinary tests, so every build is checked.
    #[test]
    fn strangers_bytes_never_panic_a_decoder() {
        let mut x = 0x9e37_79b9_7f4a_7c15u64;
        let mut rnd = move || {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x
        };
        let k = StationKey::generate().unwrap();
        let card = Card {
            station: k.id(), head: 3, name: "HOSTIS".into(), about: "a line".into(), port: Some("Tortuga".into()),
            live: false, at_ms: 1, pow: pow_find(&k.id()), since: 1, proof: None, cover: None, accent: None,
            kinds: vec!["talk".into()], mature: false, tip: None, explicit: false,
        }
        .sign(&k);
        let entry = pyrate_core::SignedEntry::sign(&k, pyrate_core::Entry { station: k.id(), seq: 0, prev: [0; 32], at_ms: 1, body: pyrate_core::Body::Colours { name: "x".into(), about: "y".into(), port: None, tip: None } }).unwrap().to_bytes().unwrap();
        let seeds: Vec<Vec<u8>> = vec![
            postcard::to_stdvec(&Gossip::Station(card.clone())).unwrap(),
            postcard::to_stdvec(&card).unwrap(),
            entry.clone(),
            postcard::to_stdvec(&Request::Log { station: k.id(), from: 0 }).unwrap(),
        ];
        let mut dec = pyrate_core::audio::ChunkDecoder::new(2).unwrap();
        // PYRATE_FUZZ_ROUNDS runs it longer, for a deep pass.
        let rounds: u32 = std::env::var("PYRATE_FUZZ_ROUNDS").ok().and_then(|v| v.parse().ok()).unwrap_or(6_000);
        for i in 0..rounds {
            let mut b: Vec<u8> = if i % 2 == 0 {
                (0..(rnd() % 2048) as usize).map(|_| rnd() as u8).collect()
            } else {
                let mut b = seeds[(rnd() % seeds.len() as u64) as usize].clone();
                for _ in 0..1 + rnd() % 6 {
                    if b.is_empty() {
                        break;
                    }
                    let at = (rnd() % b.len() as u64) as usize;
                    match rnd() % 3 {
                        0 => b[at] = rnd() as u8,
                        1 => b.truncate(at),
                        _ => b.insert(at, 0xff),
                    }
                }
                b
            };
            let _ = postcard::from_bytes::<Gossip>(&b);
            if let Ok(a) = postcard::from_bytes::<Announce>(&b) {
                let _ = a.verify();
            }
            let _ = postcard::from_bytes::<Request>(&b);
            let _ = postcard::from_bytes::<Response>(&b);
            let _ = postcard::from_bytes::<crate::hop::Hop>(&b);
            let _ = postcard::from_bytes::<pyrate_core::chat::Signed>(&b);
            if let Ok(e) = pyrate_core::SignedEntry::from_bytes(&b) {
                let _ = e.verify();
            }
            let _ = pyrate_core::audio::packets(&b);
            let _ = dec.decode_as(&b, 300);
            if i % 50 == 0 {
                let _ = pyrate_core::art::decode(&b);
                b.splice(0..0, [0xff, 0xd8, 0xff, 0xe0]);
                let _ = pyrate_core::art::decode(&b);
            }
        }
    }

    #[test]
    fn a_harbour_note_is_the_node_s_own_word() {
        let secret = [5u8; 32];
        let g = Gossip::harbour(&secret, "https://relay.example/", 1_000_000);
        let (node, relay) = g.harbour_checked(1_000_000).unwrap();
        assert_eq!(node, StationKey::from_seed(secret).id().0);
        assert_eq!(relay, "https://relay.example/");
        assert!(g.harbour_checked(1_000_000 + 16 * 60_000).is_none(), "stale");
        let Gossip::Harbour { node, at_ms, sig, .. } = g else { unreachable!() };
        let forged = Gossip::Harbour { node, relay: "https://elsewhere.example/".into(), at_ms, sig };
        assert!(forged.harbour_checked(1_000_000).is_none(), "another relay under the same signature");
    }

    #[test]
    fn a_false_claim_jumps_no_queue_and_a_flood_grows_nothing() {
        let mut c = std::collections::HashMap::new();
        note_carrier(&mut c, [1; 32], 10, Some(10), 1_000);
        note_carrier(&mut c, [2; 32], u64::MAX, Some(10), 2_000);
        let order = rank_carriers(&c, &[0; 32], Some(10), |_| false);
        assert_eq!(order, vec![[2; 32], [1; 32]], "a claim past the card counts as the card, and the fresher word goes first");
        let order = rank_carriers(&c, &[0; 32], Some(10), |n| *n == [2; 32]);
        assert_eq!(order[0], [1; 32], "a carrier that failed lately goes last");
        for i in 0..1_000u32 {
            let mut n = [9u8; 32];
            n[..4].copy_from_slice(&i.to_le_bytes());
            note_carrier(&mut c, n, 10, Some(10), 3_000 + i as u64);
        }
        assert_eq!(c.len(), MAX_CARRIERS);
    }

    #[test]
    fn sketch_counts_roughly() {
        let mut s = Sketch::default();
        for i in 0u32..1000 {
            s.add(blake3::hash(&i.to_le_bytes()).as_bytes());
        }
        let e = s.estimate();
        assert!((850..1150).contains(&e), "estimate {e}");
        let mut small = Sketch::default();
        for i in 0u32..7 {
            small.add(blake3::hash(&i.to_le_bytes()).as_bytes());
        }
        assert_eq!(small.estimate(), 7);
    }

    #[test]
    fn announce_signs_and_proves() {
        let k = StationKey::generate().unwrap();
        let a = Card {
            station: k.id(), head: 3, name: "HOSTIS".into(), about: "".into(), port: None,
            live: false, at_ms: 1, pow: pow_find(&k.id()), since: 1, proof: None, cover: None, accent: None,
            kinds: vec!["talk".into()], mature: false, tip: None, explicit: false,
        }
        .sign(&k);
        assert!(a.verify());
        let wire = postcard::to_stdvec(&a).unwrap();
        let back: Announce = postcard::from_bytes(&wire).unwrap();
        assert!(back.verify());
        assert_eq!(back.kinds, vec!["talk".to_string()]);
        let mut forged = back.clone();
        forged.card.head = 4;
        forged.raw = postcard::to_stdvec(&forged.card).unwrap();
        assert!(!forged.verify());
    }

    #[test]
    fn explicit_rides_at_the_end_and_old_cards_read_as_not() {
        let k = StationKey::generate().unwrap();
        let mut card = Card {
            station: k.id(), head: 1, name: "X".into(), about: "".into(), port: None,
            live: false, at_ms: 1, pow: pow_find(&k.id()), since: 1, proof: None, cover: None, accent: None,
            kinds: vec![], mature: true, tip: None, explicit: true,
        };
        let back: Announce = postcard::from_bytes(&postcard::to_stdvec(&card.clone().sign(&k)).unwrap()).unwrap();
        assert!(back.verify() && back.explicit && back.mature);
        // A card signed before explicit existed: the same bytes, minus the last one.
        card.explicit = false;
        let raw = postcard::to_stdvec(&card).unwrap();
        let mut m = CARD_DOMAIN.to_vec();
        m.extend_from_slice(&raw);
        let wire = postcard::to_stdvec(&AnnounceWire { card: raw, sig: k.sign(&m) }).unwrap();
        let old: Announce = postcard::from_bytes(&wire).unwrap();
        assert!(old.verify() && !old.explicit);
    }

    #[test]
    fn a_card_from_a_later_version_still_verifies() {
        // A later version appends a field to the card. This one reads what it knows.
        #[derive(Serialize)]
        struct Later<'a> {
            card: &'a Card,
            extra: u32,
        }
        let k = StationKey::generate().unwrap();
        let card = Card {
            station: k.id(), head: 1, name: "Later".into(), about: "".into(), port: None,
            live: false, at_ms: 1, pow: pow_find(&k.id()), since: 1, proof: None, cover: None, accent: None,
            kinds: vec![], mature: false, tip: None, explicit: false,
        };
        let mut raw = postcard::to_stdvec(&card).unwrap();
        raw.extend(postcard::to_stdvec(&Later { card: &card, extra: 9 }.extra).unwrap());
        let mut m = CARD_DOMAIN.to_vec();
        m.extend_from_slice(&raw);
        let wire = postcard::to_stdvec(&AnnounceWire { card: raw, sig: k.sign(&m) }).unwrap();
        let a: Announce = postcard::from_bytes(&wire).unwrap();
        assert!(a.verify());
        assert_eq!(a.name, "Later");
        assert_eq!(postcard::to_stdvec(&a).unwrap(), wire, "passed on byte for byte");
    }
}
