//! Sealed sound: how every chunk is held and passed between nodes.
//!
//! A chunk is sealed under a key drawn from its own address (its plain hash, which only a
//! station's log names) and filed and requested under a second hash of that address. A
//! carrier holds and hands on sealed bytes under a name it cannot turn back into an address,
//! so it never has the sound in the clear and cannot say what it carries. A listener, who
//! has the log, asks by name, unseals, and checks the sound against its address. After
//! Freenet's content-hash keys.
//!
//! No rights reserved.

use chacha20poly1305::aead::Aead;
use chacha20poly1305::{ChaCha20Poly1305, KeyInit, Nonce};

use crate::Hash;

/// The name a chunk is filed and asked for under.
pub fn name(h: &Hash) -> Hash {
    blake3::derive_key("pyrate/hold-name/v1", h)
}

fn cipher(h: &Hash) -> ChaCha20Poly1305 {
    ChaCha20Poly1305::new(&blake3::derive_key("pyrate/hold-key/v1", h).into())
}

/// Seals a chunk whose plain hash is `h`. The same chunk always seals to the same bytes, so
/// carriers agree on what they hold without ever opening it.
pub fn seal(h: &Hash, plain: &[u8]) -> Vec<u8> {
    cipher(h).encrypt(Nonce::from_slice(&[0; 12]), plain).expect("sealing cannot fail")
}

/// Unseals bytes said to be the chunk `h`, and checks them against it.
pub fn unseal(h: &Hash, sealed: &[u8]) -> Option<Vec<u8>> {
    let plain = cipher(h).decrypt(Nonce::from_slice(&[0; 12]), sealed).ok()?;
    (blake3::hash(&plain).as_bytes() == h).then_some(plain)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sealed_sound_opens_only_for_its_address() {
        let plain = b"six seconds of sound".to_vec();
        let h = *blake3::hash(&plain).as_bytes();
        let s = seal(&h, &plain);
        assert_ne!(s[..plain.len()], plain[..]);
        assert_eq!(unseal(&h, &s), Some(plain.clone()));
        assert_eq!(seal(&h, &plain), s, "the same everywhere");
        let mut bad = s.clone();
        bad[0] ^= 1;
        assert_eq!(unseal(&h, &bad), None);
        assert_eq!(unseal(&[9; 32], &s), None);
        assert_ne!(name(&h), h);
    }
}
