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
