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

pub struct Hold {
    // Boxed, so the keys stay at one address that can be locked out of swap, and wiped there.
    name: Box<[u8; 32]>,
    seal: Box<[u8; 32]>,
}

impl std::fmt::Debug for Hold {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Hold")
    }
}

impl Hold {
    pub fn new(key: &[u8; 32]) -> Self {
        Self::from_parts(blake3::derive_key("pyrate/hold/name/v2", key), blake3::derive_key("pyrate/hold/seal/v2", key))
    }

    fn from_parts(name: [u8; 32], seal: [u8; 32]) -> Self {
        let h = Self { name: Box::new(name), seal: Box::new(seal) };
        lock_pages(&*h.name);
        lock_pages(&*h.seal);
        h
    }

    /// The hold key for a node's folder: the one given, else `PYRATE_HOLD_KEY` (a file), else
    /// `hold.key` beside the data, made the first time.
    pub fn for_dir(dir: &Path, given: Option<[u8; 32]>) -> Result<Self> {
        if let Some(k) = given {
            return Ok(Self::new(&k));
        }
        let path = std::env::var_os("PYRATE_HOLD_KEY").map(PathBuf::from).unwrap_or_else(|| dir.join("hold.key"));
        if std::env::var_os("PYRATE_HOLD_KEY").is_none() {
            // Whoever copies the whole folder has the key with it. Said once at every start.
            crate::say("the hold key lies beside the data it seals: PYRATE_HOLD_KEY puts it on another disk (LANTERN.md)");
        }
        if let Ok(b) = std::fs::read(&path) {
            if let Ok(k) = <[u8; 32]>::try_from(b.as_slice()) {
                return Ok(Self::new(&k));
            }
        }
        // A key moved to another disk for the first time: the one beside the data is copied
        // there, and left for the keeper to remove once the Lantern has opened its hold.
        let old = dir.join("hold.key");
        if path != old {
            if let Ok(k) = std::fs::read(&old).map_err(anyhow::Error::from).and_then(|b| <[u8; 32]>::try_from(b.as_slice()).map_err(anyhow::Error::from)) {
                write_secret(&path, &k)?;
                if std::fs::read(&path).ok().as_deref() == Some(&k[..]) {
                    crate::say("the hold key is copied to PYRATE_HOLD_KEY; once this Lantern runs, remove the old hold.key beside the data");
                    return Ok(Self::new(&k));
                }
                anyhow::bail!("the hold key could not be copied to PYRATE_HOLD_KEY");
            }
        }
        // A key named by PYRATE_HOLD_KEY that is missing while this folder already holds sealed
        // data means the key's disk is not there (unmounted, or a typo): a new key would make
        // everything here unreadable for good, so the Lantern stops instead.
        let sealed_here = ["hold", "keys", "notes"].iter().any(|d| std::fs::read_dir(dir.join(d)).is_ok_and(|mut r| r.next().is_some()));
        if path != old && sealed_here {
            anyhow::bail!("PYRATE_HOLD_KEY names {}, which is not there, and this folder already holds sealed data: is the key's disk mounted? Not starting, so nothing is lost", path.display());
        }
        let mut k = [0u8; 32];
        rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut k);
        write_secret(&path, &k)?;
        Ok(Self::new(&k))
    }

    /// The name a thing is filed under: nobody without the key can compute it.
    pub fn name(&self, what: &[u8]) -> String {
        blake3::keyed_hash(&*self.name, what).to_hex().to_string()
    }

    /// Sealed bytes: a fresh nonce, then the bytes under the hold key, bound to `what`.
    pub fn seal(&self, what: &[u8], plain: &[u8]) -> Vec<u8> {
        let mut nonce = [0u8; 12];
        rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut nonce);
        let ct = ChaCha20Poly1305::new(&(*self.seal).into()).encrypt(Nonce::from_slice(&nonce), Payload { msg: plain, aad: what }).expect("sealing cannot fail");
        let mut out = nonce.to_vec();
        out.extend(ct);
        out
    }

    pub fn open(&self, what: &[u8], sealed: &[u8]) -> Option<Vec<u8>> {
        if sealed.len() < 12 {
            return None;
        }
        ChaCha20Poly1305::new(&(*self.seal).into()).decrypt(Nonce::from_slice(&sealed[..12]), Payload { msg: &sealed[12..], aad: what }).ok()
    }

    /// A key of its own for one station's log records, drawn from the hold key and the id.
    pub fn log_key(&self, station: &[u8; 32]) -> [u8; 32] {
        let mut h = blake3::Hasher::new_keyed(&*self.seal);
        h.update(b"pyrate/hold/log/v2");
        h.update(station);
        *h.finalize().as_bytes()
    }

    pub(crate) fn note_path(&self, dir: &Path, name: &str) -> PathBuf {
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

impl Clone for Hold {
    fn clone(&self) -> Self {
        Self::from_parts(*self.name, *self.seal)
    }
}

/// The keys leave no copy behind: wiped where they lay, and the pages let go.
impl Drop for Hold {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.name.zeroize();
        self.seal.zeroize();
        unlock_pages(&*self.name);
        unlock_pages(&*self.seal);
    }
}

/// Keeps a key's page out of swap where the system allows it (Linux, Android, macOS). A refusal,
/// such as a low locked-memory limit, leaves the key working as before.
#[cfg(unix)]
fn lock_pages(k: &[u8; 32]) {
    unsafe {
        libc::mlock(k.as_ptr() as *const libc::c_void, k.len());
    }
}
#[cfg(unix)]
fn unlock_pages(k: &[u8; 32]) {
    unsafe {
        libc::munlock(k.as_ptr() as *const libc::c_void, k.len());
    }
}
#[cfg(not(unix))]
fn lock_pages(_: &[u8; 32]) {}
#[cfg(not(unix))]
fn unlock_pages(_: &[u8; 32]) {}

/// Writes a secret so that no one else can read it at any moment: a new file opened readable by
/// its owner alone, never through a link, then moved into place in one step.
pub fn write_secret(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let tmp = path.with_file_name(format!(".{}.{}.tmp", path.file_name().and_then(|n| n.to_str()).unwrap_or("secret"), rand::random::<u64>()));
    let mut o = std::fs::OpenOptions::new();
    o.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        o.mode(0o600);
    }
    let mut f = o.open(&tmp)?;
    let done = f.write_all(bytes).and_then(|_| f.sync_all()).and_then(|_| std::fs::rename(&tmp, path));
    if done.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    done
}

#[cfg(all(test, unix))]
mod secret_tests {
    #[test]
    fn secrets_are_never_readable_by_others() {
        use std::os::unix::fs::PermissionsExt;
        let d = std::env::temp_dir().join(format!("pyrate-secret-{}", rand::random::<u64>()));
        std::fs::create_dir_all(&d).unwrap();
        let p = d.join("hold.key");
        super::write_secret(&p, &[7; 32]).unwrap();
        assert_eq!(std::fs::metadata(&p).unwrap().permissions().mode() & 0o777, 0o600);
        super::write_secret(&p, &[8; 32]).unwrap();
        assert_eq!(std::fs::read(&p).unwrap(), vec![8; 32]);
        assert_eq!(std::fs::read_dir(&d).unwrap().count(), 1);
        let _ = std::fs::remove_dir_all(&d);
    }
}
