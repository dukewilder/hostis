//! Pocket stations: stations for the people their keeper invites, and nobody else.
//!
//! A pocket station never shows on the public Log. Its card and its carriers travel on a
//! gossip topic of its own, drawn from a secret key that only the invitation carries, and
//! everything on that topic is sealed with the key. Its log is handed out only sealed, and
//! asked for by a tag drawn from the key, never by the station's id. Its sound is sealed
//! like every station's, under addresses that only its log names. A node without the key
//! cannot find the station, read its card, or say what it carries for it.
//!
//! No rights reserved.

use chacha20poly1305::aead::Aead;
use chacha20poly1305::{ChaCha20Poly1305, KeyInit, Nonce};

use crate::Hash;

/// The gossip topic a pocket station's card and carriers travel on.
pub fn topic(key: &[u8; 32]) -> Hash {
    blake3::derive_key("pyrate/pocket/topic/v1", key)
}

/// The topic a pocket's chatter travels on, sealed with its key like everything else of it.
pub fn chat_topic(key: &[u8; 32]) -> Hash {
    blake3::derive_key("pyrate/pocket/chat/v1", key)
}

/// What a member asks carriers for in place of the station's id.
pub fn tag(key: &[u8; 32]) -> Hash {
    blake3::derive_key("pyrate/pocket/tag/v1", key)
}

fn cipher(key: &[u8; 32]) -> ChaCha20Poly1305 {
    ChaCha20Poly1305::new(&blake3::derive_key("pyrate/pocket/seal/v1", key).into())
}

/// Seals bytes for the members: a fresh nonce, then the sealed bytes.
pub fn seal(key: &[u8; 32], plain: &[u8]) -> Vec<u8> {
    let mut nonce = [0u8; 12];
    getrandom::fill(&mut nonce).expect("randomness");
    let mut out = nonce.to_vec();
    out.extend(cipher(key).encrypt(Nonce::from_slice(&nonce), plain).expect("sealing cannot fail"));
    out
}

pub fn open(key: &[u8; 32], sealed: &[u8]) -> Option<Vec<u8>> {
    if sealed.len() < 12 {
        return None;
    }
    cipher(key).decrypt(Nonce::from_slice(&sealed[..12]), &sealed[12..]).ok()
}

fn unhex32(s: &str) -> Option<[u8; 32]> {
    if s.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(s.get(i * 2..i * 2 + 2)?, 16).ok()?;
    }
    Some(out)
}

/// An invitation: hostis.black/pyrateradio/listen/#pocket=<station>.<key>.<host>@<relay>.
/// The part after # never leaves the browser or the phone it is opened on.
pub fn invitation(station: &[u8; 32], key: &[u8; 32], host: &str) -> String {
    let h = |b: &[u8]| b.iter().map(|x| format!("{x:02x}")).collect::<String>();
    format!("https://hostis.black/pyrateradio/listen/#pocket={}.{}.{}", h(station), h(key), host)
}

/// The station, its key, and the host to start from, out of an invitation.
pub fn read_invitation(link: &str) -> Option<([u8; 32], [u8; 32], String)> {
    let tail = link.split("pocket=").nth(1)?;
    let mut parts = tail.splitn(3, '.');
    let station = unhex32(parts.next()?)?;
    let key = unhex32(parts.next()?)?;
    Some((station, key, parts.next().unwrap_or("").to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_invitation_carries_the_key_and_the_seal_holds() {
        let link = invitation(&[1; 32], &[2; 32], "abc@https://relay.example/");
        let (s, k, h) = read_invitation(&link).unwrap();
        assert_eq!((s, k, h.as_str()), ([1; 32], [2; 32], "abc@https://relay.example/"));
        let sealed = seal(&k, b"the card");
        assert_eq!(open(&k, &sealed).unwrap(), b"the card");
        assert!(open(&[3; 32], &sealed).is_none());
        assert_ne!(topic(&k), tag(&k));
    }
}
