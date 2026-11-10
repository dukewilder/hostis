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
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Response {
    Entries(Vec<Vec<u8>>),
    Chunk(Option<Vec<u8>>),
    Hop(Vec<u8>),
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
