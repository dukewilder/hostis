//! Chatter: a station's listeners talking under its colours, live, kept by nobody.
//!
//! Each station has its own gossip topic. A line is signed with a voice drawn from the
//! listener's own secret and the station, so a listener is the same voice each time they
//! speak under one station's colours and tied to nothing under any other. A line carries a
//! little work, so flooding a station costs the flooder real time. Lines are not written to
//! any log: nodes hold the last few in memory and they are gone when the node stops.
//!
//! The keeper speaks with the station's own key or a deputy's, and listeners see that it
//! is the station speaking. There is no moderator: each listener mutes whoever they choose.
//!
//! No rights reserved.

use serde::{Deserialize, Serialize};

use crate::{Hash, StationId, StationKey};

/// Leading zero bits a line's hash must carry. About 65,000 hashes: a moment on a phone.
pub const WORK_BITS: u32 = 16;
pub const MAX_TEXT: usize = 280;
pub const MAX_NOTE: usize = 140;
pub const MAX_NAME: usize = 32;
/// A line dated further than this from the reader's clock is not taken.
pub const SKEW_MS: u64 = 10 * 60_000;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Say {
    Text(String),
    /// Applause for what is on the air.
    Cheer,
    /// A listener asks for a recording from the station's library.
    Request { track: Hash, note: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Line {
    pub station: [u8; 32],
    pub from: [u8; 32],
    pub name: String,
    pub at_ms: u64,
    pub say: Say,
    pub nonce: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Signed {
    pub line: Line,
    pub sig: Vec<u8>,
}

/// The gossip topic a station's chatter travels on.
pub fn topic(station: &StationId) -> [u8; 32] {
    let mut h = blake3::Hasher::new();
    h.update(b"pyrate/chat/v1");
    h.update(&station.0);
    *h.finalize().as_bytes()
}

/// The voice a listener speaks with under one station's colours: the same every visit from
/// the same secret, and unrelated to their voice under any other station.
pub fn voice(secret: &[u8; 32], station: &StationId) -> StationKey {
    let mut h = blake3::Hasher::new_keyed(secret);
    h.update(b"pyrate/chat/voice/v1");
    h.update(&station.0);
    StationKey::from_seed(*h.finalize().as_bytes())
}

fn zeros(h: &[u8; 32]) -> u32 {
    let mut n = 0;
    for b in h {
        if *b == 0 {
            n += 8;
        } else {
            return n + b.leading_zeros();
        }
    }
    n
}

fn worked(bytes: &[u8]) -> bool {
    zeros(blake3::hash(bytes).as_bytes()) >= WORK_BITS
}

fn trim(s: &str, n: usize) -> String {
    s.trim().chars().filter(|c| !c.is_control() || *c == '\n').take(n).collect()
}

/// Signs a line, doing its work first.
pub fn speak(key: &StationKey, station: &StationId, name: &str, at_ms: u64, say: Say) -> Signed {
    let say = match say {
        Say::Text(t) => Say::Text(trim(&t, MAX_TEXT)),
        Say::Request { track, note } => Say::Request { track, note: trim(&note, MAX_NOTE) },
        s => s,
    };
    let mut line = Line { station: station.0, from: key.id().0, name: trim(name, MAX_NAME), at_ms, say, nonce: 0 };
    let mut bytes = postcard::to_allocvec(&line).expect("a line encodes");
    while !worked(&bytes) {
        line.nonce += 1;
        bytes = postcard::to_allocvec(&line).expect("a line encodes");
    }
    Signed { sig: key.sign(&bytes).to_vec(), line }
}

/// Whether a line is whole, worked, signed by the voice it names, for this station, and
/// dated near `now`.
pub fn check(s: &Signed, station: &StationId, now: u64) -> bool {
    let l = &s.line;
    if l.station != station.0 || l.name.chars().count() > MAX_NAME || l.at_ms.abs_diff(now) > SKEW_MS {
        return false;
    }
    match &l.say {
        Say::Text(t) if t.is_empty() || t.chars().count() > MAX_TEXT => return false,
        Say::Request { note, .. } if note.chars().count() > MAX_NOTE => return false,
        _ => {}
    }
    let Ok(sig) = <[u8; 64]>::try_from(s.sig.as_slice()) else { return false };
    let Ok(bytes) = postcard::to_allocvec(l) else { return false };
    worked(&bytes) && StationId(l.from).verify(&bytes, &sig).is_ok()
}

/// A line's own fingerprint, for telling repeats apart.
pub fn id(s: &Signed) -> Hash {
    *blake3::hash(&s.sig).as_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_spoken_line_checks_and_a_tampered_one_does_not() {
        let st = StationKey::generate().unwrap().id();
        let me = voice(&[7; 32], &st);
        let s = speak(&me, &st, "Gull", 1_000_000, Say::Text("Fair winds".into()));
        assert!(check(&s, &st, 1_000_000));
        assert!(!check(&s, &st, 1_000_000 + SKEW_MS + 1), "too far from the reader's clock");
        let other = StationKey::generate().unwrap().id();
        assert!(!check(&s, &other, 1_000_000), "for another station");
        let mut t = s.clone();
        t.line.say = Say::Text("Foul winds".into());
        assert!(!check(&t, &st, 1_000_000), "altered after signing");
        let mut lazy = s.clone();
        lazy.line.nonce += 1;
        assert!(!check(&lazy, &st, 1_000_000));
    }

    #[test]
    fn a_voice_is_steady_under_one_station_and_different_under_another() {
        let a = StationKey::generate().unwrap().id();
        let b = StationKey::generate().unwrap().id();
        assert_eq!(voice(&[1; 32], &a).id(), voice(&[1; 32], &a).id());
        assert_ne!(voice(&[1; 32], &a).id(), voice(&[1; 32], &b).id());
        assert_ne!(voice(&[1; 32], &a).id(), voice(&[2; 32], &a).id());
    }
}
