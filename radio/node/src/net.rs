//! What passes between nodes.
//!
//! Requests go over `pyrate/0`: one request per stream, one answer back.
//! The Log goes over gossip, on one topic every node joins.

use pyrate_core::{Hash, StationId, StationKey};
use serde::{Deserialize, Serialize};

pub const ALPN: &[u8] = b"pyrate/0";
pub const MAX_FRAME: usize = 16 * 1024 * 1024;
/// Leading zero bits a station's proof of work must show. About a second on a phone, once.
pub const POW_BITS: u32 = 20;

pub fn log_topic() -> [u8; 32] {
    *blake3::hash(b"pyrate/log/v1").as_bytes()
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

/// A station's card in the Log, signed by the station.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Announce {
    pub station: StationId,
    pub head: u64,
    pub name: String,
    pub about: String,
    pub port: Option<String>,
    pub live: bool,
    pub at_ms: u64,
    pub pow: u64,
    /// When a deputy signs: the master-signed entry appointing it.
    pub proof: Option<Vec<u8>>,
    #[serde(with = "serde_bytes64")]
    pub sig: [u8; 64],
}

impl Announce {
    fn message(&self) -> Vec<u8> {
        let mut m = b"pyrate/announce/v1\0".to_vec();
        let body = (&self.station, self.head, &self.name, &self.about, &self.port, self.live, self.at_ms, self.pow, &self.proof);
        m.extend(postcard::to_stdvec(&body).unwrap_or_default());
        m
    }

    pub fn sign(mut self, key: &StationKey) -> Self {
        self.sig = key.sign(&self.message());
        self
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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Gossip {
    Station(Announce),
    /// A node saying it carries a station, and how far its copy of the log reaches.
    Carrier { station: StationId, node: [u8; 32], head: u64 },
    /// Listener tokens for one station in one hour, folded into a sketch nobody can read names from.
    Aboard { station: StationId, hour: u64, sketch: Vec<u8> },
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
        let i = token[0] as usize;
        let w = u64::from_le_bytes(token[1..9].try_into().unwrap());
        let rank = (w.leading_zeros() + 1).min(64) as u8;
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
        let a = Announce {
            station: k.id(), head: 3, name: "HOSTIS".into(), about: "".into(), port: None,
            live: false, at_ms: 1, pow: pow_find(&k.id()), proof: None, sig: [0; 64],
        }
        .sign(&k);
        assert!(a.verify());
        let mut forged = a.clone();
        forged.head = 4;
        assert!(!forged.verify());
    }
}
