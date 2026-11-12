//! What a node holds: station logs and the chunks they name, on its own disk.
//! Its own stations are never evicted. Everything else it carries lives under a
//! quota and leaves oldest-touched first.
//!
//! Nothing on the disk says what it is. Chunks are sealed under keys drawn from their own
//! addresses, and logs under keys drawn from their stations' ids, and then everything is
//! sealed again and named under the device's own hold key (see `hold.rs`). Without the hold
//! key a seized disk is noise, even to someone holding every station's id; with it, a node
//! still opens a log only when the station's id reaches it, and never holds a stranger's
//! sound in the clear.
//!
//! Files from before the hold key (logs under `.sl`, chunks under `chunks/`, keys under the
//! station's hex) are carried over the first time each is read.

use std::{
    collections::{HashMap, HashSet},
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::SystemTime,
};

use anyhow::Result;
use pyrate_core::{Archive, Body, Hash, SignedEntry, StationId, StationKey};

use crate::hold::Hold;

fn hex(h: &[u8]) -> String {
    h.iter().map(|b| format!("{b:02x}")).collect()
}

fn derive(tag: &[u8], id: &StationId) -> [u8; 32] {
    let mut h = blake3::Hasher::new();
    h.update(tag);
    h.update(&id.0);
    *h.finalize().as_bytes()
}

/// The name a station's files go under: a hash of its id that cannot be turned back.
pub fn filed_as(id: &StationId) -> String {
    hex(&derive(b"pyrate/log-name/v1", id))
}

fn log_cipher(key: &[u8; 32]) -> chacha20poly1305::ChaCha20Poly1305 {
    use chacha20poly1305::KeyInit;
    chacha20poly1305::ChaCha20Poly1305::new(&(*key).into())
}

/// The key a log from before the hold key was sealed under: drawn from the id alone.
fn old_log_key(id: &StationId) -> [u8; 32] {
    derive(b"pyrate/log-key/v1", id)
}

/// One sealed record: its length, a fresh nonce, and the entry under the log's key.
fn seal_record(key: &[u8; 32], plain: &[u8]) -> Result<Vec<u8>> {
    use chacha20poly1305::aead::Aead;
    let mut nonce = [0u8; 12];
    rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut nonce);
    let ct = log_cipher(key).encrypt(chacha20poly1305::Nonce::from_slice(&nonce), plain).map_err(|_| anyhow::anyhow!("seal"))?;
    let mut out = Vec::with_capacity(4 + 12 + ct.len());
    out.extend(((12 + ct.len()) as u32).to_be_bytes());
    out.extend(nonce);
    out.extend(ct);
    Ok(out)
}

fn read_sealed(id: &StationId, key: &[u8; 32], bytes: &[u8]) -> Archive {
    use chacha20poly1305::aead::Aead;
    let c = log_cipher(key);
    let mut a = Archive::new(*id);
    let mut at = 0;
    while at + 4 <= bytes.len() {
        let n = u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
        at += 4;
        if n < 12 || at + n > bytes.len() {
            break;
        }
        let Ok(plain) = c.decrypt(chacha20poly1305::Nonce::from_slice(&bytes[at..at + 12]), &bytes[at + 12..at + n]) else { break };
        if SignedEntry::from_bytes(&plain).and_then(|e| a.append(e)).is_err() {
            break;
        }
        at += n;
    }
    a
}

pub struct Store {
    dir: PathBuf,
    pub hold: Hold,
    pub quota: u64,
    pub logs: HashMap<StationId, Archive>,
    pub keys: HashMap<StationId, StationKey>,
    /// Stations carried as ballast for the fleet: kept through quota sweeps.
    pub pinned: HashSet<StationId>,
    /// The chunks this node holds as its ballast share: kept through quota sweeps.
    pub ballast: HashSet<Hash>,
}

impl Store {
    pub fn open(dir: &Path, quota: u64) -> Result<Self> {
        Self::open_with(dir, quota, None)
    }

    /// Opens the store under a hold key handed in (a phone's, from its secure hardware), or
    /// the folder's own.
    pub fn open_with(dir: &Path, quota: u64, hold: Option<[u8; 32]>) -> Result<Self> {
        for d in ["logs", "chunks", "keys", "hold"] {
            fs::create_dir_all(dir.join(d))?;
        }
        let hold = Hold::for_dir(dir, hold)?;
        let mut s = Self { dir: dir.to_path_buf(), hold, quota, logs: HashMap::new(), keys: HashMap::new(), pinned: HashSet::new(), ballast: HashSet::new() };
        // keys/<name>.k2 holds a station's id and its master seed, or a deputy's, sealed. From
        // before the hold key, keys/<station hex> held the seed in the clear: sealed now.
        for f in fs::read_dir(dir.join("keys"))? {
            let f = f?;
            let name = f.file_name().to_string_lossy().to_string();
            if name.ends_with(".k2") {
                let Some(b) = fs::read(f.path()).ok().and_then(|b| s.hold.open(b"key", &b)) else { continue };
                if b.len() == 64 {
                    let station = StationId(b[..32].try_into().unwrap());
                    s.keys.insert(station, StationKey::from_seed(b[32..].try_into().unwrap()));
                }
                continue;
            }
            let Ok(station) = StationId::from_hex(&name) else { continue };
            let seed = fs::read(f.path())?;
            if let Ok(seed) = <[u8; 32]>::try_from(seed.as_slice()) {
                s.save_key(station, &StationKey::from_seed(seed))?;
                let _ = fs::remove_file(f.path());
                s.keys.insert(station, StationKey::from_seed(seed));
            }
        }
        let own: Vec<StationId> = s.keys.keys().copied().collect();
        for id in own {
            s.open_log(&id);
        }
        let mut legacy = Vec::new();
        for f in fs::read_dir(dir.join("logs"))? {
            let f = f?;
            if f.path().extension().is_none_or(|e| e != "log") {
                continue;
            }
            let name = f.file_name().to_string_lossy().trim_end_matches(".log").to_string();
            let Ok(id) = StationId::from_hex(&name) else { continue };
            let bytes = fs::read(f.path())?;
            let mut a = Archive::new(id);
            let mut at = 0;
            while at + 4 <= bytes.len() {
                let n = u32::from_be_bytes(bytes[at..at + 4].try_into()?) as usize;
                at += 4;
                if at + n > bytes.len() {
                    break;
                }
                match SignedEntry::from_bytes(&bytes[at..at + n]).and_then(|e| a.append(e)) {
                    Ok(()) => {}
                    Err(_) => break,
                }
                at += n;
            }
            if a.len() == 0 && !bytes.is_empty() {
                legacy.push((id, f.path(), bytes));
                continue;
            }
            // A log from before sealing: sealed now, and the plain file goes.
            if s.replace_log(id, a).is_ok() {
                let _ = fs::remove_file(f.path());
            }
        }
        // Logs from before bodies were framed. A station whose master key is here is written
        // again, entry by entry and at the same times, so it keeps its library, its clock and
        // its place in the hour; its recordings are already aboard. Anything else is set aside,
        // to be fetched again in the new form.
        for (id, path, bytes) in legacy {
            let _ = fs::rename(&path, path.with_extension("legacy"));
            let master = s.keys.get(&id).is_some_and(|k| k.id() == id);
            if !master {
                continue;
            }
            let old = pyrate_core::archive::legacy::entries(&bytes);
            if old.iter().any(|(_, b, _)| *b == Body::Strike) {
                continue;
            }
            s.logs.insert(id, Archive::new(id));
            let mut moved = std::collections::HashMap::new();
            let mut kept = 0;
            for (at, body, was) in old {
                if matches!(body, Body::Release { .. }) {
                    continue;
                }
                if let Ok(e) = s.write(id, at, crate::remap(body, &moved)) {
                    if let Ok(h) = e.hash() {
                        moved.insert(was, h);
                    }
                    kept += 1;
                }
            }
            eprintln!("carried {} into the new log format: {kept} entries", id.call_sign());
        }
        Ok(s)
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn own(&self) -> Vec<StationId> {
        self.keys.keys().copied().collect()
    }

    pub fn add_key(&mut self, key: StationKey) -> Result<StationId> {
        let id = key.id();
        self.add_key_for(id, key)
    }

    /// Keeps a key (master or deputy) for a station.
    pub fn add_key_for(&mut self, id: StationId, key: StationKey) -> Result<StationId> {
        self.save_key(id, &key)?;
        self.keys.insert(id, key);
        self.logs.entry(id).or_insert_with(|| Archive::new(id));
        Ok(id)
    }

    /// A station's key, sealed under the hold key in a file whose name says nothing.
    fn save_key(&self, id: StationId, key: &StationKey) -> Result<()> {
        let mut what = id.0.to_vec();
        what.extend(key.id().0);
        let p = self.dir.join("keys").join(format!("{}.k2", self.hold.name(&what)));
        let mut plain = id.0.to_vec();
        plain.extend(key.seed());
        crate::hold::write_secret(&p, &self.hold.seal(b"key", &plain))?;
        Ok(())
    }

    fn log_path(&self, id: &StationId) -> PathBuf {
        self.dir.join("logs").join(format!("{}.sl2", self.hold.name(&derive(b"pyrate/log-name/v1", id))))
    }

    /// Where a log lay before the hold key.
    fn old_log_path(&self, id: &StationId) -> PathBuf {
        self.dir.join("logs").join(format!("{}.sl", filed_as(id)))
    }

    /// Opens a station's sealed log, if this disk holds one and it is not open yet. The node
    /// calls this when the station's id reaches it, from its own keys or from the network.
    pub fn open_log(&mut self, id: &StationId) -> bool {
        if self.logs.contains_key(id) {
            return true;
        }
        if let Ok(bytes) = fs::read(self.log_path(id)) {
            let a = read_sealed(id, &self.hold.log_key(&id.0), &bytes);
            self.logs.insert(*id, a);
            return true;
        }
        // A log from before the hold key: sealed again under it, and the old file goes.
        let old = self.old_log_path(id);
        let Ok(bytes) = fs::read(&old) else { return false };
        let a = read_sealed(id, &old_log_key(id), &bytes);
        if self.replace_log(*id, a).is_ok() {
            let _ = fs::remove_file(old);
        }
        true
    }

    /// Appends whatever entries extend the log, in order. Returns how many were new.
    pub fn put_entries(&mut self, id: StationId, entries: Vec<SignedEntry>) -> Result<usize> {
        self.open_log(&id);
        let path = self.log_path(&id);
        let a = self.logs.entry(id).or_insert_with(|| Archive::new(id));
        let mut file = fs::OpenOptions::new().create(true).append(true).open(&path)?;
        let mut added = 0;
        for e in entries {
            if e.entry.seq < a.len() {
                continue;
            }
            let bytes = e.to_bytes()?;
            a.append(e)?;
            file.write_all(&seal_record(&self.hold.log_key(&id.0), &bytes)?)?;
            added += 1;
        }
        if a.is_struck() {
            drop(file);
            self.drop_station(&id)?;
        }
        Ok(added)
    }

    /// Signs a new entry for one of this node's own stations.
    pub fn write(&mut self, id: StationId, at_ms: u64, body: Body) -> Result<SignedEntry> {
        self.open_log(&id);
        let key = self.keys.get(&id).ok_or_else(|| anyhow::anyhow!("not our station"))?;
        let a = self.logs.entry(id).or_insert_with(|| Archive::new(id));
        // A deputy wraps what it writes, so carriers can check the deputy's signature.
        let body = if key.id() == id { body } else { Body::Deputed { by: key.id().0, body: Box::new(body) } };
        // Two entries in one millisecond would break the log's order; the second waits a tick.
        let at_ms = a.entries().last().map_or(at_ms, |l| at_ms.max(l.entry.at_ms + 1));
        let e = pyrate_core::Entry { station: id, seq: a.len(), prev: a.head(), at_ms, body };
        let signed = SignedEntry::sign(key, e)?;
        self.put_entries(id, vec![signed.clone()])?;
        Ok(signed)
    }

    /// A struck station's log, chunks and pictures go. The strike entry itself is kept,
    /// so the station is not fetched again.
    fn drop_station(&mut self, id: &StationId) -> Result<()> {
        let mut hashes = self.chunks_of(id);
        hashes.extend(self.logs.get(id).map(|a| a.art_hashes()).unwrap_or_default());
        for h in hashes {
            let _ = fs::remove_file(self.chunk_path(&h));
            let _ = fs::remove_file(self.legacy_path(&h));
        }
        Ok(())
    }

    pub fn chunks_of(&self, id: &StationId) -> Vec<Hash> {
        let Some(a) = self.logs.get(id) else { return vec![] };
        let mut out = Vec::new();
        for e in a.entries() {
            match e.entry.body() {
                Body::Track { chunks, .. } => out.extend(chunks.iter().map(|c| c.hash)),
                Body::Live { chunk, .. } => out.push(chunk.hash),
                Body::Release { chunks, .. } => out.extend(chunks.iter().map(|c| c.hash)),
                _ => {}
            }
        }
        out
    }

    /// Chunks are sealed on disk twice. Inside, after Freenet's content-hash keys, under a key
    /// drawn from the chunk's address, which only a station's log names. Outside, under the
    /// hold key, in a file named by the hold key from the chunk's sealed name: nobody without
    /// the device's key can say what any file is, or test the disk for any station.
    fn hold_path(&self, name: &Hash) -> PathBuf {
        let x = self.hold.name(name);
        self.dir.join("hold").join(&x[..2]).join(x)
    }

    fn chunk_path(&self, h: &Hash) -> PathBuf {
        self.hold_path(&pyrate_core::seal::name(h))
    }

    /// Where a chunk lay before the hold key: under its sealed name, sealed once.
    fn old_sealed_path(&self, name: &Hash) -> PathBuf {
        let x = hex(name);
        self.dir.join("chunks").join(&x[..2]).join(x)
    }

    /// Writes a chunk's sealed bytes under the hold key.
    fn file_sealed(&self, name: &Hash, sealed: &[u8]) -> Result<()> {
        let p = self.hold_path(name);
        fs::create_dir_all(p.parent().unwrap())?;
        let tmp = p.with_extension("part");
        fs::write(&tmp, self.hold.seal(name, sealed))?;
        fs::rename(tmp, p)?;
        Ok(())
    }

    /// A chunk's sealed bytes by the name it is asked for under: opened from the hold key's
    /// layer only, never from its own. This is what a carrier hands on.
    pub fn get_sealed(&self, name: &Hash) -> Option<Vec<u8>> {
        let p = self.hold_path(name);
        if let Ok(b) = fs::read(&p) {
            if let Ok(f) = fs::File::options().append(true).open(&p) {
                let _ = f.set_modified(SystemTime::now());
            }
            return self.hold.open(name, &b);
        }
        let old = self.old_sealed_path(name);
        let b = fs::read(&old).ok()?;
        // A chunk from before sealing lies under its plain hash. Asked for by that hash as a
        // name, it would go out in the clear: it is sealed in place instead, and nothing is said.
        if blake3::hash(&b).as_bytes() == name {
            if self.put_chunk(name, &b).is_ok() {
                let _ = fs::remove_file(&old);
            }
            return None;
        }
        // From before the hold key: filed under it now.
        if self.file_sealed(name, &b).is_ok() {
            let _ = fs::remove_file(&old);
        }
        Some(b)
    }

    /// Keeps sealed bytes fetched for the chunk `h`, once they are checked against it.
    pub fn put_sealed(&self, h: &Hash, sealed: &[u8]) -> Result<()> {
        if pyrate_core::seal::unseal(h, sealed).is_none() {
            anyhow::bail!("sealed chunk does not match its hash");
        }
        self.file_sealed(&pyrate_core::seal::name(h), sealed)
    }

    /// Where unsealed chunks lived before sealing. Read once, sealed, removed.
    fn legacy_path(&self, h: &Hash) -> PathBuf {
        let x = hex(h);
        self.dir.join("chunks").join(&x[..2]).join(x)
    }

    pub fn has_chunk(&self, h: &Hash) -> bool {
        let name = pyrate_core::seal::name(h);
        self.hold_path(&name).exists() || self.old_sealed_path(&name).exists() || self.legacy_path(h).exists()
    }

    pub fn put_chunk(&self, h: &Hash, bytes: &[u8]) -> Result<()> {
        if blake3::hash(bytes).as_bytes() != h {
            anyhow::bail!("chunk does not match its hash");
        }
        self.file_sealed(&pyrate_core::seal::name(h), &pyrate_core::seal::seal(h, bytes))
    }

    pub fn get_chunk(&self, h: &Hash) -> Option<Vec<u8>> {
        let name = pyrate_core::seal::name(h);
        if let Some(sealed) = self.get_sealed(&name) {
            return pyrate_core::seal::unseal(h, &sealed);
        }
        let legacy = self.legacy_path(h);
        let bytes = fs::read(&legacy).ok()?;
        if self.put_chunk(h, &bytes).is_ok() {
            let _ = fs::remove_file(legacy);
        }
        Some(bytes)
    }

    /// Replaces a log outright: used only when two phones wrote one station at once.
    pub fn replace_log(&mut self, id: StationId, a: Archive) -> Result<()> {
        let key = self.hold.log_key(&id.0);
        let mut bytes = Vec::new();
        for e in a.entries() {
            bytes.extend(seal_record(&key, &e.to_bytes()?)?);
        }
        let tmp = self.log_path(&id).with_extension("tmp");
        fs::write(&tmp, bytes)?;
        fs::rename(&tmp, self.log_path(&id))?;
        self.logs.insert(id, a);
        Ok(())
    }

    /// Drops a log this node no longer carries.
    pub fn forget_log(&mut self, id: &StationId) {
        self.logs.remove(id);
        let _ = fs::remove_file(self.log_path(id));
        let _ = fs::remove_file(self.old_log_path(id));
        let _ = fs::remove_file(self.dir.join("logs").join(format!("{}.log", id.to_hex())));
    }

    pub fn remove_chunk(&self, h: &Hash) {
        let name = pyrate_core::seal::name(h);
        let _ = fs::remove_file(self.hold_path(&name));
        let _ = fs::remove_file(self.old_sealed_path(&name));
        let _ = fs::remove_file(self.legacy_path(h));
    }

    /// Every name a chunk may lie under on this disk, now or from before.
    fn file_names(&self, h: &Hash) -> [String; 3] {
        let name = pyrate_core::seal::name(h);
        [self.hold.name(&name), hex(&name), hex(h)]
    }

    /// The chunk files on disk, in the hold and from before it.
    fn chunk_files(&self) -> Result<Vec<fs::DirEntry>> {
        let mut out = Vec::new();
        for top in ["hold", "chunks"] {
            let Ok(rd) = fs::read_dir(self.dir.join(top)) else { continue };
            for sub in rd {
                let sub = sub?;
                if !sub.file_type()?.is_dir() {
                    continue;
                }
                for f in fs::read_dir(sub.path())? {
                    out.push(f?);
                }
            }
        }
        Ok(out)
    }

    /// Empties the hold: every carried chunk and log that is neither ours nor ballast.
    pub fn clear_hold(&mut self) -> Result<usize> {
        let keep_ids: HashSet<StationId> = self.own().into_iter().chain(self.pinned.iter().copied()).collect();
        let keep_art: Vec<Hash> = keep_ids.iter().filter_map(|id| self.logs.get(id)).flat_map(|a| a.art_hashes()).collect();
        let keep: HashSet<String> = keep_ids.iter().flat_map(|id| self.chunks_of(id)).chain(keep_art).flat_map(|h| self.file_names(&h)).collect();
        let mut n = 0;
        for f in self.chunk_files()? {
            if !keep.contains(&f.file_name().to_string_lossy().to_string()) {
                fs::remove_file(f.path())?;
                n += 1;
            }
        }
        let keep_logs: HashSet<String> = keep_ids.iter().flat_map(|id| [self.log_path(id), self.old_log_path(id)]).filter_map(|p| p.file_name().map(|n| n.to_string_lossy().to_string())).collect();
        for f in fs::read_dir(self.dir.join("logs"))?.flatten() {
            if !keep_logs.contains(&f.file_name().to_string_lossy().to_string()) {
                let _ = fs::remove_file(f.path());
            }
        }
        self.logs.retain(|id, _| keep_ids.contains(id));
        Ok(n)
    }

    /// Evicts carried chunks until under quota. Chunks no station names go first, then the
    /// oldest-touched chunk of whichever station holds the most here, so one station flooding
    /// the disk shrinks before any other loses a chunk. Its own stations and pinned ones stay.
    /// Bytes of the ballast share already on this disk.
    pub fn ballast_bytes(&self) -> u64 {
        self.ballast.iter().filter_map(|h| fs::metadata(self.chunk_path(h)).ok()).map(|m| m.len()).sum()
    }

    pub fn enforce_quota(&self) -> Result<u64> {
        let own_art = self.own().iter().filter_map(|id| self.logs.get(id)).flat_map(|a| a.art_hashes()).collect::<Vec<_>>();
        let keep: HashSet<String> = self.own().iter().flat_map(|id| self.chunks_of(id)).chain(own_art).chain(self.ballast.iter().copied()).flat_map(|h| self.file_names(&h)).collect();
        let mut whose: HashMap<String, StationId> = HashMap::new();
        for id in self.logs.keys() {
            for h in self.chunks_of(id) {
                for n in self.file_names(&h) {
                    whose.entry(n).or_insert(*id);
                }
            }
        }
        let mut total = 0u64;
        let mut orphans = Vec::new();
        let mut by: HashMap<StationId, (u64, Vec<(SystemTime, u64, PathBuf)>)> = HashMap::new();
        for f in self.chunk_files()? {
            let m = f.metadata()?;
            total += m.len();
            let name = f.file_name().to_string_lossy().to_string();
            if keep.contains(&name) {
                continue;
            }
            let e = (m.modified().unwrap_or(SystemTime::UNIX_EPOCH), m.len(), f.path());
            match whose.get(&name) {
                Some(id) => {
                    let g = by.entry(*id).or_default();
                    g.0 += m.len();
                    g.1.push(e);
                }
                None => orphans.push(e),
            }
        }
        orphans.sort();
        for g in by.values_mut() {
            g.1.sort();
            g.1.reverse();
        }
        let mut freed = 0;
        for (_, len, path) in orphans {
            if total <= self.quota {
                return Ok(freed);
            }
            fs::remove_file(&path)?;
            total -= len;
            freed += len;
        }
        while total > self.quota {
            let Some(g) = by.values_mut().filter(|g| !g.1.is_empty()).max_by_key(|g| g.0) else { break };
            let (_, len, path) = g.1.pop().unwrap();
            g.0 -= len;
            fs::remove_file(&path)?;
            total -= len;
            freed += len;
        }
        Ok(freed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_chunk_from_before_sealing_never_answers_a_sealed_request() {
        let dir = std::env::temp_dir().join(format!("pyrate-store-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let s = Store::open(&dir, 1 << 20).unwrap();
        let plain = b"six seconds of old sound".to_vec();
        let h = *blake3::hash(&plain).as_bytes();
        let legacy = s.legacy_path(&h);
        fs::create_dir_all(legacy.parent().unwrap()).unwrap();
        fs::write(&legacy, &plain).unwrap();
        assert_eq!(s.get_sealed(&h), None, "asked for by its plain hash, it says nothing");
        assert!(!legacy.exists(), "and the plain file is gone");
        let sealed = s.get_sealed(&pyrate_core::seal::name(&h)).expect("it is held sealed now");
        assert_eq!(pyrate_core::seal::unseal(&h, &sealed), Some(plain));
        let _ = fs::remove_dir_all(&dir);
    }

    /// Every file under `dir`, as (name, bytes).
    fn files(dir: &Path) -> Vec<(String, Vec<u8>)> {
        let mut out = Vec::new();
        for e in fs::read_dir(dir).unwrap().flatten() {
            if e.path().is_dir() {
                out.extend(files(&e.path()));
            } else {
                out.push((e.file_name().to_string_lossy().to_string(), fs::read(e.path()).unwrap()));
            }
        }
        out
    }

    #[test]
    fn without_the_hold_key_the_disk_is_noise_even_to_someone_with_the_log() {
        let dir = std::env::temp_dir().join(format!("pyrate-hold-store-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let k = StationKey::generate().unwrap();
        let id = k.id();
        let plain = b"six seconds of a station's sound".to_vec();
        let h = *blake3::hash(&plain).as_bytes();
        {
            let mut s = Store::open_with(&dir, 1 << 20, Some([7; 32])).unwrap();
            s.add_key(StationKey::from_seed(k.seed())).unwrap();
            s.write(id, 1_000, Body::Colours { name: "Hidden".into(), about: "".into(), port: None, tip: None }).unwrap();
            s.put_chunk(&h, &plain).unwrap();
        }
        // Someone who holds the station's id, its log and the chunk's address, and knows the old
        // way of naming and sealing, still finds nothing on the disk to test or open.
        let named = [filed_as(&id), hex(&pyrate_core::seal::name(&h)), hex(&h), id.to_hex()];
        for (name, bytes) in files(&dir) {
            assert!(!named.iter().any(|n| name.contains(n.as_str())), "{name} gives itself away");
            assert!(pyrate_core::seal::unseal(&h, &bytes).is_none(), "{name} opens with the chunk's own key");
            assert!(read_sealed(&id, &old_log_key(&id), &bytes).is_empty(), "{name} opens with the id's key");
            assert!(!bytes.windows(32).any(|w| w == k.seed()), "{name} holds the station key in the clear");
        }
        // With another device's hold key nothing opens; with this one, everything does.
        let other = Store::open_with(&dir, 1 << 20, Some([8; 32])).unwrap();
        assert!(other.keys.is_empty() && other.get_chunk(&h).is_none());
        let mut s = Store::open_with(&dir, 1 << 20, Some([7; 32])).unwrap();
        assert!(s.keys.contains_key(&id));
        assert!(s.open_log(&id) && s.logs[&id].len() == 1);
        assert_eq!(s.get_chunk(&h), Some(plain));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_hold_from_before_the_hold_key_comes_across_and_leaves_nothing_behind() {
        let dir = std::env::temp_dir().join(format!("pyrate-hold-old-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        for d in ["logs", "chunks", "keys"] {
            fs::create_dir_all(dir.join(d)).unwrap();
        }
        let k = StationKey::generate().unwrap();
        let id = k.id();
        // The old layout: a key in the clear, a log under the id's key, a chunk sealed once.
        fs::write(dir.join("keys").join(id.to_hex()), k.seed()).unwrap();
        let mut a = Archive::new(id);
        a.write(&k, 1_000, Body::Colours { name: "Old".into(), about: "".into(), port: None, tip: None }).unwrap();
        let rec = seal_record(&old_log_key(&id), &a.entries()[0].to_bytes().unwrap()).unwrap();
        fs::write(dir.join("logs").join(format!("{}.sl", filed_as(&id))), rec).unwrap();
        let plain = b"an old chunk".to_vec();
        let h = *blake3::hash(&plain).as_bytes();
        let name = hex(&pyrate_core::seal::name(&h));
        fs::create_dir_all(dir.join("chunks").join(&name[..2])).unwrap();
        fs::write(dir.join("chunks").join(&name[..2]).join(&name), pyrate_core::seal::seal(&h, &plain)).unwrap();
        let mut s = Store::open_with(&dir, 1 << 20, Some([9; 32])).unwrap();
        assert!(s.keys.contains_key(&id), "the key comes across");
        assert_eq!(s.logs.get(&id).map(|a| a.len()), Some(1), "the log comes across");
        assert_eq!(s.get_chunk(&h), Some(plain), "the chunk comes across");
        assert!(s.open_log(&id));
        assert!(!dir.join("keys").join(id.to_hex()).exists() && !dir.join("logs").join(format!("{}.sl", filed_as(&id))).exists());
        assert!(!dir.join("chunks").join(&name[..2]).join(&name).exists(), "and the old files are gone");
        let _ = fs::remove_dir_all(&dir);
    }
}
