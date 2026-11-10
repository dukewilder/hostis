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
    /// A voice note to the keeper, sealed so that only the station's own keys open it. It
    /// passes through the room live; nobody else can hear it and nobody keeps it.
    Hail { eph: [u8; 32], wraps: Vec<([u8; 32], Vec<u8>)>, sealed: Vec<u8>, mime: String },
}

/// Largest hail: twenty seconds of a voice, low and plain.
pub const MAX_HAIL: usize = 90_000;

fn x_secret(seed: &[u8; 32]) -> x25519_dalek::StaticSecret {
    use sha2::Digest;
    let h = sha2::Sha512::digest(seed);
    let mut b = [0u8; 32];
    b.copy_from_slice(&h[..32]);
    x25519_dalek::StaticSecret::from(b)
}

fn x_public(ed: &[u8; 32]) -> Option<x25519_dalek::PublicKey> {
    let p = curve25519_dalek::edwards::CompressedEdwardsY(*ed).decompress()?;
    Some(x25519_dalek::PublicKey::from(p.to_montgomery().to_bytes()))
}

fn wrap_cipher(shared: &[u8; 32], to: &[u8; 32]) -> chacha20poly1305::ChaCha20Poly1305 {
    use chacha20poly1305::KeyInit;
    let mut k = [0u8; 64];
    k[..32].copy_from_slice(shared);
    k[32..].copy_from_slice(to);
    chacha20poly1305::ChaCha20Poly1305::new(&blake3::derive_key("pyrate/hail-wrap/v1", &k).into())
}

/// Seals a voice note to each of the station's keys: its own and its deputies'.
pub fn hail(recipients: &[[u8; 32]], audio: &[u8], mime: &str) -> Option<Say> {
    use chacha20poly1305::aead::Aead;
    use chacha20poly1305::KeyInit;
    if audio.len() > MAX_HAIL || recipients.is_empty() {
        return None;
    }
    let mut seed = [0u8; 32];
    let mut key = [0u8; 32];
    getrandom::fill(&mut seed).ok()?;
    getrandom::fill(&mut key).ok()?;
    let eph = x25519_dalek::StaticSecret::from(seed);
    let nonce = chacha20poly1305::Nonce::from_slice(&[0; 12]);
    let sealed = chacha20poly1305::ChaCha20Poly1305::new(&key.into()).encrypt(nonce, audio).ok()?;
    let mut wraps = Vec::new();
    for to in recipients.iter().take(16) {
        let Some(pk) = x_public(to) else { continue };
        let shared = eph.diffie_hellman(&pk).to_bytes();
        wraps.push((*to, wrap_cipher(&shared, to).encrypt(nonce, key.as_slice()).ok()?));
    }
    Some(Say::Hail { eph: x25519_dalek::PublicKey::from(&eph).to_bytes(), wraps, sealed, mime: mime.chars().take(40).collect() })
}

/// Opens a hail with one of the station's keys, given as its seed. The sound and its type.
pub fn open_hail(say: &Say, seed: &[u8; 32]) -> Option<(Vec<u8>, String)> {
    use chacha20poly1305::aead::Aead;
    use chacha20poly1305::KeyInit;
    let Say::Hail { eph, wraps, sealed, mime } = say else { return None };
    let me = StationKey::from_seed(*seed).id().0;
    let (_, w) = wraps.iter().find(|(to, _)| *to == me)?;
    let shared = x_secret(seed).diffie_hellman(&x25519_dalek::PublicKey::from(*eph)).to_bytes();
    let nonce = chacha20poly1305::Nonce::from_slice(&[0; 12]);
    let key: [u8; 32] = wrap_cipher(&shared, &me).decrypt(nonce, w.as_slice()).ok()?.try_into().ok()?;
    let audio = chacha20poly1305::ChaCha20Poly1305::new(&key.into()).decrypt(nonce, sealed.as_slice()).ok()?;
    Some((audio, mime.clone()))
}

/// Whether a hail is for one of these keys.
pub fn hail_for(say: &Say, keys: &[[u8; 32]]) -> bool {
    matches!(say, Say::Hail { wraps, .. } if wraps.iter().any(|(to, _)| keys.contains(to)))
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

/// The work rides on a digest of everything but the nonce, so a long line (a hail) costs
/// no more to work than a short one.
fn digest(l: &Line) -> [u8; 32] {
    let body = postcard::to_allocvec(&(&l.station, &l.from, &l.name, &l.at_ms, &l.say)).expect("a line encodes");
    *blake3::hash(&body).as_bytes()
}

fn worked(d: &[u8; 32], nonce: u64) -> bool {
    let mut h = blake3::Hasher::new();
    h.update(d);
    h.update(&nonce.to_le_bytes());
    zeros(h.finalize().as_bytes()) >= WORK_BITS
}

/// A stranger's words as they are shown: no control characters, no marks that turn text
/// round or hide it (bidi overrides and isolates, zero-width joins), so a line cannot run
/// backwards over its neighbours or pass for something else.
pub fn shown(s: &str) -> String {
    s.chars()
        .filter(|c| (!c.is_control() || *c == '\n') && !matches!(*c, '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2060}'..='\u{2069}' | '\u{061c}' | '\u{feff}'))
        .collect()
}

/// A title made from a file name, without what a phone or recorder writes into one: the
/// moment it was made (a date with a time, which gives away the keeper's day and zone) and the
/// device's own words around it ("VID", "Recording", "PXL"). A bare date stays, since a keeper
/// may mean it. Returns the title and whether anything was taken out.
pub fn plain_title(s: &str) -> (String, bool) {
    let toks: Vec<&str> = s.split(|c: char| c.is_whitespace() || c == '_').filter(|t| !t.is_empty()).collect();
    let stampy = |t: &str| t.chars().all(|c| c.is_ascii_digit() || "-.:hms".contains(c)) && t.chars().filter(|c| c.is_ascii_digit()).count() >= 2;
    let mut keep = vec![true; toks.len()];
    let mut i = 0;
    let mut cut = false;
    while i < toks.len() {
        if !stampy(toks[i]) {
            i += 1;
            continue;
        }
        let mut j = i;
        let mut digits = 0;
        while j < toks.len() && stampy(toks[j]) {
            digits += toks[j].chars().filter(|c| c.is_ascii_digit()).count();
            j += 1;
        }
        if digits >= 12 {
            keep[i..j].iter_mut().for_each(|k| *k = false);
            cut = true;
        }
        i = j;
    }
    if cut {
        const DEVICE: [&str; 14] = ["vid", "img", "aud", "rec", "pxl", "voice", "recording", "audio", "video", "sound", "new", "memo", "screen", "record"];
        for (k, t) in toks.iter().enumerate() {
            if keep[k] && DEVICE.contains(&t.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase().as_str()) {
                keep[k] = false;
            }
        }
    }
    let out: Vec<&str> = toks.iter().zip(&keep).filter(|(_, k)| **k).map(|(t, _)| *t).collect();
    let t = out.join(" ").trim_matches(|c: char| c == '-' || c == '.' || c.is_whitespace()).to_string();
    if !cut {
        return (s.trim().to_string(), false);
    }
    (if t.is_empty() { "Untitled".to_string() } else { t }, true)
}

/// True when a name mixes Latin with Greek or Cyrillic letters, the usual way to pass for
/// another station ("HОSTIS" with a Cyrillic O). Shown as a mark, never hidden.
pub fn mixed_script(s: &str) -> bool {
    let (mut latin, mut greek, mut cyr) = (false, false, false);
    for c in s.chars() {
        match c as u32 {
            0x41..=0x5a | 0x61..=0x7a | 0xc0..=0x24f => latin = true,
            0x370..=0x3ff | 0x1f00..=0x1fff => greek = true,
            0x400..=0x52f => cyr = true,
            _ => {}
        }
    }
    [latin, greek, cyr].iter().filter(|b| **b).count() > 1
}

/// A speaker's name as shown: as [`shown`], and never wearing the station's own mark, which
/// only the keeper's key earns.
pub fn shown_name(s: &str) -> String {
    let mut n = shown(s).replace('\n', " ");
    loop {
        let low = n.to_lowercase();
        let Some(at) = low.find("(the station)") else { break };
        n.replace_range(at..at + "(the station)".len(), "");
    }
    n.trim().to_string()
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
    let d = digest(&line);
    while !worked(&d, line.nonce) {
        line.nonce += 1;
    }
    let bytes = postcard::to_allocvec(&line).expect("a line encodes");
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
        Say::Hail { wraps, sealed, mime, .. } if wraps.is_empty() || wraps.len() > 16 || sealed.len() > MAX_HAIL + 16 || mime.len() > 40 => return false,
        _ => {}
    }
    let Ok(sig) = <[u8; 64]>::try_from(s.sig.as_slice()) else { return false };
    let Ok(bytes) = postcard::to_allocvec(l) else { return false };
    worked(&digest(l), l.nonce) && StationId(l.from).verify(&bytes, &sig).is_ok()
}

/// The line ids a room has taken, each forgotten once a line that old would be refused anyway,
/// so a room that runs for months holds only the last few minutes of them.
#[derive(Default)]
pub struct Seen {
    order: std::collections::VecDeque<(u64, Hash)>,
    ids: std::collections::HashSet<Hash>,
}

impl Seen {
    /// True if the id is new. `at_ms` is the line's own time, `now` the reader's.
    pub fn insert(&mut self, id: Hash, at_ms: u64, now: u64) -> bool {
        while let Some((t, old)) = self.order.front() {
            if now.saturating_sub(*t) <= 2 * SKEW_MS {
                break;
            }
            self.ids.remove(old);
            self.order.pop_front();
        }
        if !self.ids.insert(id) {
            return false;
        }
        self.order.push_back((at_ms, id));
        true
    }

    pub fn len(&self) -> usize {
        self.ids.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }
}

/// A line's own fingerprint, for telling repeats apart.
pub fn id(s: &Signed) -> Hash {
    *blake3::hash(&s.sig).as_bytes()
}

#[cfg(test)]
mod tests {
    #[test]
    fn names_cannot_wear_the_station_mark_or_turn_text_round() {
        assert_eq!(super::shown_name("Gangway (THE STATION)"), "Gangway");
        assert_eq!(super::shown_name("a\u{202e}b\u{2066}c"), "abc");
        assert!(super::mixed_script("H\u{41e}STIS"));
        assert!(!super::mixed_script("HOSTIS"));
        assert_eq!(super::plain_title("Recording 2026-11-10 21-03-12"), ("Untitled".into(), true));
        assert_eq!(super::plain_title("VID_20261110_210312 harbour song"), ("harbour song".into(), true));
        assert_eq!(super::plain_title("Sea shanty 20261110-2103"), ("Sea shanty".into(), true));
        assert_eq!(super::plain_title("1999-12-31 Party"), ("1999-12-31 Party".into(), false));
        assert_eq!(super::plain_title("Track 01"), ("Track 01".into(), false));
        assert!(!super::mixed_script("\u{41f}\u{438}\u{440}\u{430}\u{442}"));
        assert_eq!(super::shown("one\ntwo\u{200b}"), "one\ntwo");
    }

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
    fn a_hail_opens_only_for_the_station_s_keys() {
        let master = StationKey::generate().unwrap();
        let deputy = StationKey::generate().unwrap();
        let stranger = StationKey::generate().unwrap();
        let say = hail(&[master.id().0, deputy.id().0], b"ahoy, play the shanty", "audio/mp4").unwrap();
        assert_eq!(open_hail(&say, &master.seed()).unwrap().0, b"ahoy, play the shanty");
        assert_eq!(open_hail(&say, &deputy.seed()).unwrap().1, "audio/mp4");
        assert!(open_hail(&say, &stranger.seed()).is_none());
        assert!(hail_for(&say, &[deputy.id().0]) && !hail_for(&say, &[stranger.id().0]));
        let me = voice(&[3; 32], &master.id());
        let s = speak(&me, &master.id(), "Gull", 5_000_000, say);
        assert!(check(&s, &master.id(), 5_000_000));
    }

    #[test]
    fn seen_ids_are_forgotten_once_a_line_that_old_is_refused() {
        let mut s = Seen::default();
        assert!(s.insert([1; 32], 1_000, 1_000));
        assert!(!s.insert([1; 32], 1_000, 2_000), "a repeat is caught");
        assert!(s.insert([2; 32], 10_000_000, 10_000_000));
        assert_eq!(s.len(), 1, "the old id went once it could no longer be taken");
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
