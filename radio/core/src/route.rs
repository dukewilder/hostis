//! The seal on a private route's request: sealed to the exit node's own key, so the first
//! hop, which sees who asks, cannot read what is asked, and the exit, which reads it, never
//! learns who asked. Shared by phones and browser tabs; the exits' side is in the node.
//!
//! No rights reserved.

use chacha20poly1305::aead::Aead;
use chacha20poly1305::{ChaCha20Poly1305, KeyInit, Nonce};

pub const ASK_NONCE: [u8; 12] = [0; 12];
pub const ANSWER_NONCE: [u8; 12] = [1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

/// A node's ed25519 id as an X25519 key.
pub fn x_public(node: &[u8; 32]) -> Option<x25519_dalek::PublicKey> {
    let ed = curve25519_dalek::edwards::CompressedEdwardsY(*node).decompress()?;
    Some(x25519_dalek::PublicKey::from(ed.to_montgomery().to_bytes()))
}

/// The cipher both ends of one sealed request use.
pub fn cipher(shared: &[u8; 32], eph: &[u8; 32], to: &[u8; 32]) -> ChaCha20Poly1305 {
    let mut m = Vec::with_capacity(96);
    m.extend_from_slice(shared);
    m.extend_from_slice(eph);
    m.extend_from_slice(to);
    ChaCha20Poly1305::new(&blake3::derive_key("pyrate/hop/v1", &m).into())
}

/// Seals a request to node `to`: the ephemeral key, the sealed bytes, and the cipher that opens
/// the answer.
pub fn seal_to(to: &[u8; 32], plain: &[u8]) -> Option<([u8; 32], Vec<u8>, ChaCha20Poly1305)> {
    let to_x = x_public(to)?;
    let mut seed = [0u8; 32];
    getrandom::fill(&mut seed).ok()?;
    let eph = x25519_dalek::StaticSecret::from(seed);
    let eph_pub = *x25519_dalek::PublicKey::from(&eph).as_bytes();
    let c = cipher(&eph.diffie_hellman(&to_x).to_bytes(), &eph_pub, to);
    let ct = c.encrypt(Nonce::from_slice(&ASK_NONCE), plain).ok()?;
    Some((eph_pub, ct, c))
}

/// Opens the exit's answer to a request sealed with `c`.
pub fn open_answer(c: &ChaCha20Poly1305, raw: &[u8]) -> Option<Vec<u8>> {
    c.decrypt(Nonce::from_slice(&ANSWER_NONCE), raw).ok()
}

/// The X25519 secret behind an ed25519 seed: the half of the key `x_public` reaches.
pub fn x_secret(seed: &[u8; 32]) -> x25519_dalek::StaticSecret {
    use sha2::Digest;
    let h = sha2::Sha512::digest(seed);
    let mut b = [0u8; 32];
    b.copy_from_slice(&h[..32]);
    x25519_dalek::StaticSecret::from(b)
}

/// Opens a request sealed to the key whose seed is `seed` and whose id is `me`: the plain
/// bytes, and the cipher its answer goes back under.
pub fn open_sealed(seed: &[u8; 32], me: &[u8; 32], eph: &[u8; 32], ct: &[u8]) -> Option<(Vec<u8>, ChaCha20Poly1305)> {
    let shared = x_secret(seed).diffie_hellman(&x25519_dalek::PublicKey::from(*eph)).to_bytes();
    let c = cipher(&shared, eph, me);
    let plain = c.decrypt(Nonce::from_slice(&ASK_NONCE), ct).ok()?;
    Some((plain, c))
}

/// Seals an answer under the cipher its request came with.
pub fn seal_answer(c: &ChaCha20Poly1305, plain: &[u8]) -> Vec<u8> {
    c.encrypt(Nonce::from_slice(&ANSWER_NONCE), plain).unwrap_or_default()
}

/// What a deck's lead holds, sealed to the radio that leads to it: the listener's own node and
/// the relay it is reached through. Only that radio can read it, so the deck's note names the
/// way in and never the listener.
#[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq)]
pub struct Lead {
    pub to: [u8; 32],
    pub relay: String,
}

/// A lead's `relay` for a deck no relay reaches (a tab behind a door). Its `to` is then a
/// cookie made for the deck alone, not the listener's node: the deck keeps its way in by
/// pulling (`Ask::Pull` in the node), through its own guard and a middle, so the way in learns
/// neither the listener's node nor its address.
pub const LEAD_PULL: &str = "pull";

/// A lead for `intro`: the ephemeral key and the sealed `Lead`, as postcard bytes.
pub fn make_lead(intro: &[u8; 32], lead: &Lead) -> Option<Vec<u8>> {
    let (eph, ct, _) = seal_to(intro, &postcard::to_stdvec(lead).ok()?)?;
    postcard::to_stdvec(&(eph, ct)).ok()
}

/// Opens a lead sealed to this radio.
pub fn open_lead(seed: &[u8; 32], me: &[u8; 32], raw: &[u8]) -> Option<Lead> {
    let (eph, ct): ([u8; 32], Vec<u8>) = postcard::from_bytes(raw).ok()?;
    let (plain, _) = open_sealed(seed, me, &eph, &ct)?;
    postcard::from_bytes(&plain).ok()
}

/// The deck that keeps a chunk for the crew: of the decks known, the one whose key ranks
/// first for this chunk's name. Every listener on the station asks the same deck for the same
/// chunk, so one fetch from the Lanterns serves them all. `n` decks in order of rank.
pub fn keepers(decks: &[[u8; 32]], name: &[u8; 32], n: usize) -> Vec<[u8; 32]> {
    let mut r: Vec<([u8; 32], [u8; 32])> = decks.iter().map(|d| {
        let mut h = blake3::Hasher::new();
        h.update(b"pyrate/keeper/v1");
        h.update(d);
        h.update(name);
        (*h.finalize().as_bytes(), *d)
    }).collect();
    r.sort();
    r.into_iter().take(n).map(|x| x.1).collect()
}

/// The cipher of one sealed request, for code that keeps them.
pub type Cipher = ChaCha20Poly1305;

/// Whether a deck answers an ask that opened under its key: only with the switch on, and only
/// when it came from the deck's own way in. Anyone else who sends a deck's key an ask is
/// guessing which tab is behind the deck, and gets nothing to confirm it.
pub fn deck_answers<T: PartialEq>(crew_on: bool, from: &T, intro: Option<&T>) -> bool {
    crew_on && intro == Some(from)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(seed: [u8; 32]) -> [u8; 32] {
        crate::StationKey::from_seed(seed).id().0
    }

    #[test]
    fn a_deck_answers_only_its_way_in_and_only_with_the_switch_on() {
        let (way_in, stranger) = (id([4; 32]), id([5; 32]));
        assert!(deck_answers(true, &way_in, Some(&way_in)));
        assert!(!deck_answers(true, &stranger, Some(&way_in)), "nothing outside a route");
        assert!(!deck_answers(false, &way_in, Some(&way_in)), "the switch off serves nothing");
        assert!(!deck_answers(true, &way_in, None));
    }

    #[test]
    fn a_request_opens_only_for_its_radio_and_a_lead_only_for_its_way_in() {
        let (a, b) = ([1u8; 32], [2u8; 32]);
        let (eph, ct, c) = seal_to(&id(a), b"ask").unwrap();
        let (plain, c2) = open_sealed(&a, &id(a), &eph, &ct).unwrap();
        assert_eq!(plain, b"ask");
        assert!(open_sealed(&b, &id(b), &eph, &ct).is_none());
        assert_eq!(open_answer(&c, &seal_answer(&c2, b"answer")).as_deref(), Some(&b"answer"[..]));
        let lead = make_lead(&id(a), &Lead { to: id(b), relay: "https://relay.example/".into() }).unwrap();
        assert_eq!(open_lead(&a, &id(a), &lead).unwrap().to, id(b));
        assert!(open_lead(&b, &id(b), &lead).is_none());
    }
}
