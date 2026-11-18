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
//! node at start. A Lantern keeps it sealed under its keeper's passphrase (`hold.sealed`,
//! Argon2id), so the disk alone opens nothing; see `lantern_key`. Only with
//! `--hold-key-on-disk` does it keep the key in the clear, in `hold.key` or the file
//! `PYRATE_HOLD_KEY` names, and it says so at every start.
//!
//! The carrier still cannot read what it carries: inside this layer every chunk is sealed as
//! before, under its own address, which only a station's log names.
//!
//! No rights reserved.

use std::path::{Path, PathBuf};

use anyhow::Result;
use chacha20poly1305::aead::{Aead, Payload};
use chacha20poly1305::{ChaCha20Poly1305, KeyInit, Nonce};
use zeroize::Zeroizing;

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
        let mut h = Self { name: Box::new([0; 32]), seal: Box::new([0; 32]) };
        lock_pages(&*h.name);
        lock_pages(&*h.seal);
        // Drawn straight into their locked boxes: no copy of either is left on the stack.
        *h.name = blake3::derive_key("pyrate/hold/name/v2", key);
        *h.seal = blake3::derive_key("pyrate/hold/seal/v2", key);
        h
    }

    /// The hold key for a node's folder: the one given, else `PYRATE_HOLD_KEY` (a file), else
    /// `hold.key` beside the data, made the first time. A Lantern hands its key in instead
    /// (`lantern_key`), so this is the computer app's path.
    pub fn for_dir(dir: &Path, given: Option<[u8; 32]>) -> Result<Self> {
        if let Some(mut k) = given {
            let h = Self::new(&k);
            zeroize::Zeroize::zeroize(&mut k);
            return Ok(h);
        }
        if std::env::var_os("PYRATE_HOLD_KEY").is_none() {
            // Whoever copies the whole folder has the key with it. Said once at every start.
            crate::say("the hold key lies beside the data it seals: PYRATE_HOLD_KEY puts it on another disk (LANTERN.md)");
        }
        Ok(Self::new(&*disk_key(dir)?))
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
    /// Wiped when it goes out of use, as the hold's own keys are.
    pub fn log_key(&self, station: &[u8; 32]) -> zeroize::Zeroizing<[u8; 32]> {
        let mut h = blake3::Hasher::new_keyed(&*self.seal);
        h.update(b"pyrate/hold/log/v2");
        h.update(station);
        zeroize::Zeroizing::new(*h.finalize().as_bytes())
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
        let mut h = Self { name: Box::new([0; 32]), seal: Box::new([0; 32]) };
        lock_pages(&*h.name);
        lock_pages(&*h.seal);
        h.name.copy_from_slice(&*self.name);
        h.seal.copy_from_slice(&*self.seal);
        h
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

/// Where a node keeps its hold key in the clear: the file `PYRATE_HOLD_KEY` names, else
/// `hold.key` beside the data.
fn disk_key_path(dir: &Path) -> PathBuf {
    std::env::var_os("PYRATE_HOLD_KEY").map(PathBuf::from).unwrap_or_else(|| dir.join("hold.key"))
}

/// The key a node keeps in the clear on disk: read, copied to `PYRATE_HOLD_KEY` the first time
/// it names a new place, or made the first time.
fn disk_key(dir: &Path) -> Result<Zeroizing<[u8; 32]>> {
    let path = disk_key_path(dir);
    if let Some(k) = read_plain(&path) {
        return Ok(k);
    }
    // A key moved to another disk for the first time: the one beside the data is copied
    // there, and left for the keeper to remove once the Lantern has opened its hold.
    let old = dir.join("hold.key");
    if path != old {
        if let Some(k) = read_plain(&old) {
            write_secret(&path, &k[..])?;
            if read_plain(&path).as_deref() == Some(&*k) {
                crate::say("the hold key is copied to PYRATE_HOLD_KEY; once this Lantern runs, remove the old hold.key beside the data");
                return Ok(k);
            }
            anyhow::bail!("the hold key could not be copied to PYRATE_HOLD_KEY");
        }
    }
    // A key named by PYRATE_HOLD_KEY that is missing while this folder already holds sealed
    // data means the key's disk is not there (unmounted, or a typo): a new key would make
    // everything here unreadable for good, so the Lantern stops instead.
    if path != old && sealed_here(dir) {
        anyhow::bail!("PYRATE_HOLD_KEY names {}, which is not there, and this folder already holds sealed data: is the key's disk mounted? Not starting, so nothing is lost", path.display());
    }
    let k = fresh_key();
    write_secret(&path, &k[..])?;
    Ok(k)
}

fn fresh_key() -> Zeroizing<[u8; 32]> {
    let mut k = Zeroizing::new([0u8; 32]);
    rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut k[..]);
    k
}

/// Whether this folder already holds anything sealed under some hold key.
fn sealed_here(dir: &Path) -> bool {
    ["hold", "keys", "notes", "logs"].iter().any(|d| std::fs::read_dir(dir.join(d)).is_ok_and(|mut r| r.next().is_some()))
}

/// A 32-byte key from a file, read into memory that is wiped after.
fn read_plain(path: &Path) -> Option<Zeroizing<[u8; 32]>> {
    let b = Zeroizing::new(std::fs::read(path).ok()?);
    let mut k = Zeroizing::new([0u8; 32]);
    if b.len() != 32 {
        return None;
    }
    k.copy_from_slice(&b);
    Some(k)
}

/// A key in the clear goes: written over with zeros, flushed, then removed.
fn destroy_plain(path: &Path) -> std::io::Result<()> {
    use std::io::Write;
    let len = std::fs::metadata(path)?.len() as usize;
    let mut f = std::fs::OpenOptions::new().write(true).open(path)?;
    f.write_all(&vec![0u8; len])?;
    f.sync_all()?;
    drop(f);
    std::fs::remove_file(path)
}

/// The file beside a Lantern's data that holds its hold key sealed under the keeper's
/// passphrase. Without the passphrase it is noise.
pub const SEALED_KEY: &str = "hold.sealed";
const SEALED_MAGIC: &[u8; 4] = b"PYH1";
/// Argon2id at 256 MiB and three passes: about a second on a small server, at every start and
/// at every guess anyone makes at the passphrase.
const KDF_KIB: u32 = 256 * 1024;
const KDF_PASSES: u32 = 3;
/// The shortest passphrase a new seal takes.
pub const PASS_MIN: usize = 12;

fn pass_key(pass: &[u8], salt: &[u8], kib: u32, passes: u32) -> Result<Zeroizing<[u8; 32]>> {
    // A file that names a cost far past any sane one is refused before any memory is taken.
    if !(8..=4 * 1024 * 1024).contains(&kib) || !(1..=64).contains(&passes) {
        anyhow::bail!("hold.sealed names a cost no Lantern uses");
    }
    let params = argon2::Params::new(kib, passes, 1, Some(32)).map_err(|e| anyhow::anyhow!("{e}"))?;
    let mut out = Zeroizing::new([0u8; 32]);
    argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params).hash_password_into(pass, salt, &mut out[..]).map_err(|e| anyhow::anyhow!("{e}"))?;
    Ok(out)
}

/// The hold key sealed under a passphrase: the mark, the cost, a fresh salt and nonce, then the
/// key under ChaCha20-Poly1305 bound to everything before it.
fn wrap_with(key: &[u8; 32], pass: &[u8], kib: u32, passes: u32) -> Result<Vec<u8>> {
    let mut head = SEALED_MAGIC.to_vec();
    head.extend(kib.to_le_bytes());
    head.extend(passes.to_le_bytes());
    let mut salt = [0u8; 16];
    rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut salt);
    head.extend(salt);
    let mut nonce = [0u8; 12];
    rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut nonce);
    let kek = pass_key(pass, &salt, kib, passes)?;
    let ct = ChaCha20Poly1305::new(&(*kek).into()).encrypt(Nonce::from_slice(&nonce), Payload { msg: key, aad: &head }).map_err(|_| anyhow::anyhow!("sealing the hold key failed"))?;
    let mut out = head;
    out.extend(nonce);
    out.extend(ct);
    Ok(out)
}

/// The hold key out of its seal: None when the passphrase is not the one it was sealed under.
fn unwrap_key(blob: &[u8], pass: &[u8]) -> Result<Option<Zeroizing<[u8; 32]>>> {
    if blob.len() != 4 + 8 + 16 + 12 + 48 || &blob[..4] != SEALED_MAGIC {
        anyhow::bail!("hold.sealed is not a sealed hold key");
    }
    let kib = u32::from_le_bytes(blob[4..8].try_into()?);
    let passes = u32::from_le_bytes(blob[8..12].try_into()?);
    let kek = pass_key(pass, &blob[12..28], kib, passes)?;
    let Ok(plain) = ChaCha20Poly1305::new(&(*kek).into()).decrypt(Nonce::from_slice(&blob[28..40]), Payload { msg: &blob[40..], aad: &blob[..28] }) else { return Ok(None) };
    let plain = Zeroizing::new(plain);
    let mut k = Zeroizing::new([0u8; 32]);
    k.copy_from_slice(&plain);
    Ok(Some(k))
}

/// A Lantern's hold key. With a passphrase it comes out of `hold.sealed`; an older Lantern's
/// key in the clear is sealed under the passphrase the first time and the clear copy goes; a new
/// Lantern makes one. Without a passphrase it opens nothing, unless its keeper chose to keep the
/// key on disk in the clear (`on_disk`), which it says at every start.
pub fn lantern_key(dir: &Path, pass: Option<&[u8]>, on_disk: bool) -> Result<Zeroizing<[u8; 32]>> {
    lantern_key_with(dir, pass, on_disk, KDF_KIB, KDF_PASSES, true)
}

/// A Lantern's hold key for a look only (`--id`, `--open-hails`): the key it already has, and
/// nothing made, sealed or removed. Asked while a new Lantern makes its first seal, a look that
/// made a key of its own would seal it over the Lantern's, and the Lantern's notes would open
/// under neither from then on.
pub fn lantern_key_existing(dir: &Path, pass: Option<&[u8]>, on_disk: bool) -> Result<Zeroizing<[u8; 32]>> {
    lantern_key_with(dir, pass, on_disk, KDF_KIB, KDF_PASSES, false)
}

const NO_KEY_YET: &str = "this Lantern has no hold key yet: start it once";

fn lantern_key_with(dir: &Path, pass: Option<&[u8]>, on_disk: bool, kib: u32, passes: u32, make: bool) -> Result<Zeroizing<[u8; 32]>> {
    let sealed = dir.join(SEALED_KEY);
    let mut plain_paths = vec![disk_key_path(dir)];
    if !plain_paths.contains(&dir.join("hold.key")) {
        plain_paths.push(dir.join("hold.key"));
    }
    let blob = std::fs::read(&sealed).ok();
    let Some(pass) = pass else {
        if blob.is_some() {
            anyhow::bail!("This Lantern's hold is sealed under a passphrase and none was given, so it opens nothing. Give the passphrase with PYRATE_HOLD_PASS_FILE, PYRATE_HOLD_PASS_CMD or --hold-pass-fd, or type it when asked (LANTERN.md).");
        }
        if !on_disk {
            anyhow::bail!("This Lantern has no passphrase for its hold key, so it opens nothing and starts nothing. Give one with PYRATE_HOLD_PASS_FILE, PYRATE_HOLD_PASS_CMD or --hold-pass-fd, or type it when asked. To keep the key on this disk in the clear instead, start with --hold-key-on-disk (LANTERN.md).");
        }
        if !make {
            return plain_paths.iter().find_map(|p| read_plain(p)).ok_or_else(|| anyhow::anyhow!(NO_KEY_YET));
        }
        let k = disk_key(dir)?;
        crate::say(&format!("WARNING the hold key lies in the clear at {}, so whoever takes that disk opens everything this Lantern keeps. A passphrase takes it off the disk (LANTERN.md).", disk_key_path(dir).display()));
        return Ok(k);
    };
    let key = match blob {
        Some(b) => match unwrap_key(&b, pass)? {
            Some(k) => k,
            None => anyhow::bail!("That passphrase does not open this Lantern's hold. Nothing was opened or changed."),
        },
        None => {
            let found = plain_paths.iter().find_map(|p| read_plain(p));
            if !make {
                return found.ok_or_else(|| anyhow::anyhow!(NO_KEY_YET));
            }
            if found.is_none() && sealed_here(dir) {
                anyhow::bail!("This folder holds sealed data, and neither hold.sealed nor a hold key in the clear is here to open it. Is the key's disk mounted? Nothing was started, so nothing is lost.");
            }
            if pass.len() < PASS_MIN {
                anyhow::bail!("A passphrase for the hold needs twelve characters or more. Four or five random words make a good one.");
            }
            let k = found.unwrap_or_else(fresh_key);
            // Never over a seal another start made meanwhile: that one stands, and is opened.
            if !write_secret_new(&sealed, &wrap_with(&k, pass, kib, passes)?)? {
                return lantern_key_with(dir, Some(pass), on_disk, kib, passes, false);
            }
            // The seal is read back and opened before anything in the clear is let go.
            let back = std::fs::read(&sealed).ok().and_then(|b| unwrap_key(&b, pass).ok().flatten());
            if back.as_deref() != Some(&*k) {
                let _ = std::fs::remove_file(&sealed);
                anyhow::bail!("The hold key could not be sealed under the passphrase. Nothing was changed.");
            }
            k
        }
    };
    if !make {
        return Ok(key);
    }
    // A key in the clear that the seal now holds goes; one that differs is left for the keeper.
    for p in &plain_paths {
        match read_plain(p) {
            Some(k) if *k == *key => match destroy_plain(p) {
                Ok(()) => crate::say(&format!("The hold key is sealed under the passphrase in {SEALED_KEY}, and the copy in the clear at {} is gone.", p.display())),
                Err(e) => crate::say(&format!("WARNING the hold key is sealed under the passphrase, but the copy in the clear at {} could not be removed ({e}). Remove it by hand.", p.display())),
            },
            Some(_) => crate::say(&format!("WARNING {} holds a different key in the clear and was left alone. Remove it once you know what it is.", p.display())),
            None if p.exists() => crate::say(&format!("WARNING {} is still here and holds no key this Lantern knows. Remove it once you know what it is.", p.display())),
            None => {}
        }
    }
    if on_disk {
        crate::say("--hold-key-on-disk is set, but a passphrase was given, so the hold key stays sealed and off the disk.");
    }
    Ok(key)
}

/// Destroys every form of a Lantern's hold key on this machine: the sealed one, and any in the
/// clear. Everything the hold kept is noise from then on. True when one was found.
pub fn scuttle(dir: &Path) -> std::io::Result<bool> {
    let mut found = false;
    for p in [dir.join(SEALED_KEY), disk_key_path(dir), dir.join("hold.key")] {
        if p.exists() {
            destroy_plain(&p)?;
            found = true;
        }
    }
    Ok(found)
}

/// Writes a secret as `write_secret` does, but only where there is none yet: false, and nothing
/// changed, when another start put one there first.
pub fn write_secret_new(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<bool> {
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
    // A link fails when the name is taken, so of two starts only one seal stands.
    let done = f.write_all(bytes).and_then(|_| f.sync_all()).and_then(|_| std::fs::hard_link(&tmp, path));
    let _ = std::fs::remove_file(&tmp);
    match done {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
        Err(e) => Err(e),
    }
}

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

#[cfg(test)]
mod lantern_key_tests {
    use super::*;

    fn fresh_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pyrate-lkey-{tag}-{}", rand::random::<u64>()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    // A small cost, so the tests are quick; the cost rides in the file, as it does for real.
    fn key(dir: &Path, pass: Option<&[u8]>, on_disk: bool) -> Result<Zeroizing<[u8; 32]>> {
        lantern_key_with(dir, pass, on_disk, 64, 1, true)
    }

    // The installer asks `--id` while a new Lantern makes its first seal. The look makes no key,
    // and a seal already there is never written over, so the Lantern's key is the one sealed.
    #[test]
    fn a_look_at_a_new_lantern_makes_no_key_and_never_seals_over_it() {
        let d = fresh_dir("look");
        let pass = b"four random words here";
        let e = lantern_key_with(&d, Some(pass), false, 64, 1, false).unwrap_err().to_string();
        assert!(e.contains("start it once"), "{e}");
        let e = lantern_key_with(&d, None, true, 64, 1, false).unwrap_err().to_string();
        assert!(e.contains("start it once"), "{e}");
        assert_eq!(std::fs::read_dir(&d).unwrap().count(), 0, "a look made no file");
        let k = key(&d, Some(pass), false).unwrap();
        assert_eq!(*lantern_key_with(&d, Some(pass), false, 64, 1, false).unwrap(), *k);
        // A second start that finds no seal when it looks, but one there when it writes, takes
        // that one and leaves it as it was.
        let blob = std::fs::read(d.join(SEALED_KEY)).unwrap();
        assert!(!write_secret_new(&d.join(SEALED_KEY), &[0; 88]).unwrap());
        assert_eq!(std::fs::read(d.join(SEALED_KEY)).unwrap(), blob);
        assert_eq!(*key(&d, Some(pass), false).unwrap(), *k);
        assert_eq!(std::fs::read_dir(&d).unwrap().count(), 1, "no temporary file is left");
        // Two starts at once on a new folder end with one key between them.
        let d2 = fresh_dir("race");
        let (a, b) = std::thread::scope(|s| {
            let x = s.spawn(|| key(&d2, Some(pass), false).map(|k| *k));
            let y = s.spawn(|| key(&d2, Some(pass), false).map(|k| *k));
            (x.join().unwrap().unwrap(), y.join().unwrap().unwrap())
        });
        assert_eq!(a, b, "both starts hold the key that is sealed");
        assert_eq!(*key(&d2, Some(pass), false).unwrap(), a);
        let _ = std::fs::remove_dir_all(&d2);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_passphrase_start_keeps_no_key_in_the_clear() {
        let d = fresh_dir("new");
        let pass = b"four random words here";
        let k = key(&d, Some(pass), false).unwrap();
        assert!(d.join(SEALED_KEY).exists() && !d.join("hold.key").exists(), "sealed, and nothing in the clear");
        let blob = std::fs::read(d.join(SEALED_KEY)).unwrap();
        assert!(!blob.windows(32).any(|w| w == &k[..]), "the key is not in the file as it is");
        assert_eq!(*key(&d, Some(pass), false).unwrap(), *k, "the same passphrase opens the same hold");
        let e = key(&d, Some(b"not the passphrase at all"), false).unwrap_err().to_string();
        assert!(e.contains("does not open"), "{e}");
        let e = key(&d, None, false).unwrap_err().to_string();
        assert!(e.contains("opens nothing"), "{e}");
        let e = key(&d, None, true).unwrap_err().to_string();
        assert!(e.contains("opens nothing"), "a sealed hold is not opened from disk either: {e}");
        assert!(!d.join("hold.key").exists(), "and no refusal leaves a key behind");
        assert!(scuttle(&d).unwrap() && !d.join(SEALED_KEY).exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn without_its_key_a_lantern_opens_nothing_and_makes_nothing() {
        let d = fresh_dir("none");
        let e = key(&d, None, false).unwrap_err().to_string();
        assert!(e.contains("--hold-key-on-disk"), "{e}");
        assert_eq!(std::fs::read_dir(&d).unwrap().count(), 0, "no file was made");
        let e = key(&d, Some(b"short"), false).unwrap_err().to_string();
        assert!(e.contains("twelve"), "{e}");
        assert_eq!(std::fs::read_dir(&d).unwrap().count(), 0);
        // Sealed data with no key anywhere: a new key would lose it all, so nothing starts.
        std::fs::create_dir_all(d.join("notes")).unwrap();
        std::fs::write(d.join("notes").join("x"), b"sealed").unwrap();
        let e = key(&d, Some(b"four random words here"), false).unwrap_err().to_string();
        assert!(e.contains("Nothing was started"), "{e}");
        assert!(!d.join(SEALED_KEY).exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn an_old_hold_key_is_sealed_and_the_clear_copy_goes() {
        let d = fresh_dir("move");
        let old = key(&d, None, true).unwrap();
        assert_eq!(std::fs::read(d.join("hold.key")).unwrap(), old.to_vec(), "on disk in the clear, as asked");
        std::fs::create_dir_all(d.join("notes")).unwrap();
        std::fs::write(d.join("notes").join("x"), b"sealed").unwrap();
        let pass = b"four random words here";
        let k = key(&d, Some(pass), true).unwrap();
        assert_eq!(*k, *old, "the same hold key, so everything sealed under it still opens");
        assert!(!d.join("hold.key").exists(), "the clear copy is gone");
        assert_eq!(*key(&d, Some(pass), false).unwrap(), *old);
        // A plain key put back that differs from the sealed one is left alone, never trusted.
        std::fs::write(d.join("hold.key"), [9u8; 32]).unwrap();
        assert_eq!(*key(&d, Some(pass), false).unwrap(), *old);
        assert!(d.join("hold.key").exists());
        let _ = std::fs::remove_dir_all(&d);
    }
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
