//! The hold key: a secret of this device's own, under which everything the node keeps on disk
//! is sealed again and named.
//!
//! Chunks are already sealed under keys drawn from their own addresses, and logs under keys
//! drawn from their stations' ids. Those keys come from public facts, so someone who seizes a
//! disk and also runs a node could test it for every station on the Log. Under the hold key
//! the disk is noise to anyone without it: names that cannot be computed, contents that cannot
//! be opened. And because everything hangs from one key, destroying that key destroys the
//! hold at once, which deleting files on flash memory does not.
//!
//! On a phone the key lives wrapped by the phone's own secure hardware and is handed to the
//! node at start; a Lantern keeps it in a file of its own (`hold.key`), which its keeper may
//! put on a different disk with `PYRATE_HOLD_KEY`.
//!
//! The carrier still cannot read what it carries: inside this layer every chunk is sealed as
//! before, under its own address, which only a station's log names.
//!
//! No rights reserved.

use std::path::{Path, PathBuf};

use anyhow::Result;
use chacha20poly1305::aead::{Aead, Payload};
use chacha20poly1305::{ChaCha20Poly1305, KeyInit, Nonce};

#[derive(Clone)]
pub struct Hold {
    name: [u8; 32],
    seal: [u8; 32],
}

impl std::fmt::Debug for Hold {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Hold")
    }
}

impl Hold {
    pub fn new(key: &[u8; 32]) -> Self {
        Self { name: blake3::derive_key("pyrate/hold/name/v2", key), seal: blake3::derive_key("pyrate/hold/seal/v2", key) }
    }

    /// The hold key for a node's folder: the one given, else `PYRATE_HOLD_KEY` (a file), else
    /// `hold.key` beside the data, made the first time.
    pub fn for_dir(dir: &Path, given: Option<[u8; 32]>) -> Result<Self> {
        if let Some(k) = given {
            return Ok(Self::new(&k));
        }
        let path = std::env::var_os("PYRATE_HOLD_KEY").map(PathBuf::from).unwrap_or_else(|| dir.join("hold.key"));
        if let Ok(b) = std::fs::read(&path) {
            if let Ok(k) = <[u8; 32]>::try_from(b.as_slice()) {
                return Ok(Self::new(&k));
            }
        }
        let mut k = [0u8; 32];
        rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut k);
        std::fs::write(&path, k)?;
        Ok(Self::new(&k))
    }

    /// The name a thing is filed under: nobody without the key can compute it.
    pub fn name(&self, what: &[u8]) -> String {
        blake3::keyed_hash(&self.name, what).to_hex().to_string()
    }

    /// Sealed bytes: a fresh nonce, then the bytes under the hold key, bound to `what`.
    pub fn seal(&self, what: &[u8], plain: &[u8]) -> Vec<u8> {
        let mut nonce = [0u8; 12];
        rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut nonce);
        let ct = ChaCha20Poly1305::new(&self.seal.into()).encrypt(Nonce::from_slice(&nonce), Payload { msg: plain, aad: what }).expect("sealing cannot fail");
        let mut out = nonce.to_vec();
        out.extend(ct);
        out
    }

    pub fn open(&self, what: &[u8], sealed: &[u8]) -> Option<Vec<u8>> {
        if sealed.len() < 12 {
            return None;
        }
        ChaCha20Poly1305::new(&self.seal.into()).decrypt(Nonce::from_slice(&sealed[..12]), Payload { msg: &sealed[12..], aad: what }).ok()
    }

    /// A key of its own for one station's log records, drawn from the hold key and the id.
    pub fn log_key(&self, station: &[u8; 32]) -> [u8; 32] {
        let mut h = blake3::Hasher::new_keyed(&self.seal);
        h.update(b"pyrate/hold/log/v2");
        h.update(station);
        *h.finalize().as_bytes()
    }

    fn note_path(&self, dir: &Path, name: &str) -> PathBuf {
        dir.join("notes").join(self.name(format!("note/{name}").as_bytes()))
    }

    /// A small file the node keeps (its chart, its settings, its pockets), sealed and named
    /// under the hold key. One kept before the hold key, at `dir/<old>` in the clear, is
    /// sealed the first time it is read and the clear copy removed.
    pub fn read_note(&self, dir: &Path, name: &str, old: Option<&Path>) -> Option<Vec<u8>> {
        if let Ok(b) = std::fs::read(self.note_path(dir, name)) {
            return self.open(name.as_bytes(), &b);
        }
        let old = old?;
        let plain = std::fs::read(old).ok()?;
        if self.write_note(dir, name, &plain).is_ok() {
            let _ = std::fs::remove_file(old);
        }
        Some(plain)
    }

    pub fn write_note(&self, dir: &Path, name: &str, plain: &[u8]) -> Result<()> {
        let p = self.note_path(dir, name);
        std::fs::create_dir_all(p.parent().unwrap())?;
        let tmp = p.with_extension("part");
        std::fs::write(&tmp, self.seal(name.as_bytes(), plain))?;
        std::fs::rename(tmp, p)?;
        Ok(())
    }

    pub fn remove_note(&self, dir: &Path, name: &str) {
        let _ = std::fs::remove_file(self.note_path(dir, name));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sealed_under_the_hold_key_and_nothing_else() {
        let a = Hold::new(&[1; 32]);
        let b = Hold::new(&[2; 32]);
        let s = a.seal(b"chunk", b"bytes");
        assert_eq!(a.open(b"chunk", &s).as_deref(), Some(&b"bytes"[..]));
        assert_eq!(b.open(b"chunk", &s), None, "another device's key opens nothing");
        assert_eq!(a.open(b"other", &s), None, "bound to what it was filed as");
        assert_ne!(a.name(b"x"), b.name(b"x"));
        let dir = std::env::temp_dir().join(format!("pyrate-hold-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("chart.json"), b"{\"kept\":[\"ab\"]}").unwrap();
        assert_eq!(a.read_note(&dir, "chart.json", Some(&dir.join("chart.json"))).as_deref(), Some(&b"{\"kept\":[\"ab\"]}"[..]));
        assert!(!dir.join("chart.json").exists(), "the clear copy is gone");
        assert_eq!(a.read_note(&dir, "chart.json", None).as_deref(), Some(&b"{\"kept\":[\"ab\"]}"[..]));
        assert_eq!(b.read_note(&dir, "chart.json", None), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
