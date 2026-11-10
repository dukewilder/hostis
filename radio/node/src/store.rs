//! What a node holds: station logs and the chunks they name, on its own disk.
//! Its own stations are never evicted. Everything else it carries lives under a
//! quota and leaves oldest-touched first.
//!
//! Nothing on the disk says what it is. Chunks are sealed under keys drawn from their own
//! addresses, and logs under keys drawn from their stations' ids, each filed under a hash
//! no one can turn back into an id. A seized disk is unlabelled noise: only someone who
//! already knows a station's id can test whether it is here. The node itself opens a log
//! only when the station's card reaches it again from the network.

use std::{
    collections::{HashMap, HashSet},
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::SystemTime,
};

use anyhow::Result;
use pyrate_core::{Archive, Body, Hash, SignedEntry, StationId, StationKey};

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

fn log_cipher(id: &StationId) -> chacha20poly1305::ChaCha20Poly1305 {
    use chacha20poly1305::KeyInit;
    chacha20poly1305::ChaCha20Poly1305::new(&derive(b"pyrate/log-key/v1", id).into())
}

/// One sealed record: its length, a fresh nonce, and the entry under the station's log key.
fn seal_record(id: &StationId, plain: &[u8]) -> Result<Vec<u8>> {
    use chacha20poly1305::aead::Aead;
    let mut nonce = [0u8; 12];
    rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut nonce);
    let ct = log_cipher(id).encrypt(chacha20poly1305::Nonce::from_slice(&nonce), plain).map_err(|_| anyhow::anyhow!("seal"))?;
    let mut out = Vec::with_capacity(4 + 12 + ct.len());
    out.extend(((12 + ct.len()) as u32).to_be_bytes());
    out.extend(nonce);
    out.extend(ct);
    Ok(out)
}

fn read_sealed(id: &StationId, bytes: &[u8]) -> Archive {
    use chacha20poly1305::aead::Aead;
    let c = log_cipher(id);
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
    pub quota: u64,
    pub logs: HashMap<StationId, Archive>,
    pub keys: HashMap<StationId, StationKey>,
    /// Stations carried as ballast for the fleet: kept through quota sweeps.
    pub pinned: HashSet<StationId>,
}

impl Store {
    pub fn open(dir: &Path, quota: u64) -> Result<Self> {
        for d in ["logs", "chunks", "keys"] {
            fs::create_dir_all(dir.join(d))?;
        }
        let mut s = Self { dir: dir.to_path_buf(), quota, logs: HashMap::new(), keys: HashMap::new(), pinned: HashSet::new() };
        // keys/<station hex> holds the station's master seed, or a deputy's.
        for f in fs::read_dir(dir.join("keys"))? {
            let f = f?;
            let Ok(station) = StationId::from_hex(&f.file_name().to_string_lossy()) else { continue };
            let seed = fs::read(f.path())?;
            if let Ok(seed) = <[u8; 32]>::try_from(seed.as_slice()) {
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
        let p = self.dir.join("keys").join(id.to_hex());
        fs::write(&p, key.seed())?;
        self.keys.insert(id, key);
        self.logs.entry(id).or_insert_with(|| Archive::new(id));
        Ok(id)
    }

    fn log_path(&self, id: &StationId) -> PathBuf {
        self.dir.join("logs").join(format!("{}.sl", filed_as(id)))
    }

    /// Opens a station's sealed log, if this disk holds one and it is not open yet. The node
    /// calls this when the station's id reaches it, from its own keys or from the network.
    pub fn open_log(&mut self, id: &StationId) -> bool {
        if self.logs.contains_key(id) {
            return true;
        }
        let Ok(bytes) = fs::read(self.log_path(id)) else { return false };
        let a = read_sealed(id, &bytes);
        self.logs.insert(*id, a);
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
            file.write_all(&seal_record(&id, &bytes)?)?;
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

    /// Chunks are sealed on disk, after Freenet's content-hash keys: the file is named by a
    /// hash of the chunk's address and encrypted with a key drawn from the address itself.
    /// A seized device holds unlabelled noise. Nothing on it says what any file is; only
    /// someone who already holds a station's log can test for that station's chunks.
    fn sealed_name(h: &Hash) -> String {
        hex(&pyrate_core::seal::name(h))
    }

    fn chunk_path(&self, h: &Hash) -> PathBuf {
        let x = Self::sealed_name(h);
        self.dir.join("chunks").join(&x[..2]).join(x)
    }

    fn named_path(&self, name: &Hash) -> PathBuf {
        let x = hex(name);
        self.dir.join("chunks").join(&x[..2]).join(x)
    }

    /// A chunk's sealed bytes by the name it is filed under, as they lie on the disk. Nothing
    /// is opened: this is what a carrier hands on.
    pub fn get_sealed(&self, name: &Hash) -> Option<Vec<u8>> {
        let p = self.named_path(name);
        let b = fs::read(&p).ok()?;
        // A chunk from before sealing lies under its plain hash. Asked for by that hash as a
        // name, it would go out in the clear: it is sealed in place instead, and nothing is said.
        if blake3::hash(&b).as_bytes() == name {
            if self.put_chunk(name, &b).is_ok() {
                let _ = fs::remove_file(&p);
            }
            return None;
        }
        if let Ok(f) = fs::File::options().append(true).open(&p) {
            let _ = f.set_modified(SystemTime::now());
        }
        Some(b)
    }

    /// Keeps sealed bytes fetched for the chunk `h`, once they are checked against it.
    pub fn put_sealed(&self, h: &Hash, sealed: &[u8]) -> Result<()> {
        if pyrate_core::seal::unseal(h, sealed).is_none() {
            anyhow::bail!("sealed chunk does not match its hash");
        }
        let p = self.chunk_path(h);
        fs::create_dir_all(p.parent().unwrap())?;
        let tmp = p.with_extension("part");
        fs::write(&tmp, sealed)?;
        fs::rename(tmp, p)?;
        Ok(())
    }

    /// Where unsealed chunks lived before sealing. Read once, sealed, removed.
    fn legacy_path(&self, h: &Hash) -> PathBuf {
        let x = hex(h);
        self.dir.join("chunks").join(&x[..2]).join(x)
    }

    pub fn has_chunk(&self, h: &Hash) -> bool {
        self.chunk_path(h).exists() || self.legacy_path(h).exists()
    }

    pub fn put_chunk(&self, h: &Hash, bytes: &[u8]) -> Result<()> {
        if blake3::hash(bytes).as_bytes() != h {
            anyhow::bail!("chunk does not match its hash");
        }
        let sealed = pyrate_core::seal::seal(h, bytes);
        let p = self.chunk_path(h);
        fs::create_dir_all(p.parent().unwrap())?;
        let tmp = p.with_extension("part");
        fs::write(&tmp, sealed)?;
        fs::rename(tmp, p)?;
        Ok(())
    }

    pub fn get_chunk(&self, h: &Hash) -> Option<Vec<u8>> {
        let p = self.chunk_path(h);
        if let Ok(sealed) = fs::read(&p) {
            let bytes = pyrate_core::seal::unseal(h, &sealed)?;
            // Touch, so carrying keeps what is listened to.
            if let Ok(f) = fs::File::options().append(true).open(&p) {
                let _ = f.set_modified(SystemTime::now());
            }
            return Some(bytes);
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
        let mut bytes = Vec::new();
        for e in a.entries() {
            bytes.extend(seal_record(&id, &e.to_bytes()?)?);
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
        let _ = fs::remove_file(self.dir.join("logs").join(format!("{}.log", id.to_hex())));
    }

    pub fn remove_chunk(&self, h: &Hash) {
        let _ = fs::remove_file(self.chunk_path(h));
        let _ = fs::remove_file(self.legacy_path(h));
    }

    /// Empties the hold: every carried chunk and log that is neither ours nor ballast.
    pub fn clear_hold(&mut self) -> Result<usize> {
        let keep_ids: HashSet<StationId> = self.own().into_iter().chain(self.pinned.iter().copied()).collect();
        let keep: HashSet<String> = keep_ids.iter().flat_map(|id| self.chunks_of(id)).flat_map(|h| [Self::sealed_name(&h), hex(&h)]).collect();
        let mut n = 0;
        for sub in fs::read_dir(self.dir.join("chunks"))? {
            let sub = sub?;
            if !sub.file_type()?.is_dir() {
                continue;
            }
            for f in fs::read_dir(sub.path())? {
                let f = f?;
                if !keep.contains(&f.file_name().to_string_lossy().to_string()) {
                    fs::remove_file(f.path())?;
                    n += 1;
                }
            }
        }
        let keep_logs: HashSet<String> = keep_ids.iter().map(|id| format!("{}.sl", filed_as(id))).collect();
        for f in fs::read_dir(self.dir.join("logs"))?.flatten() {
            if !keep_logs.contains(&f.file_name().to_string_lossy().to_string()) {
                let _ = fs::remove_file(f.path());
            }
        }
        self.logs.retain(|id, _| keep_ids.contains(id));
        Ok(n)
    }

    /// Evicts carried chunks, oldest-touched first, until under quota.
    pub fn enforce_quota(&self) -> Result<u64> {
        let own_art = self.own().iter().filter_map(|id| self.logs.get(id)).flat_map(|a| a.art_hashes()).collect::<Vec<_>>();
        let keep: HashSet<String> = self.own().iter().chain(self.pinned.iter()).flat_map(|id| self.chunks_of(id)).chain(own_art).flat_map(|h| [Self::sealed_name(&h), hex(&h)]).collect();
        let mut files = Vec::new();
        let mut total = 0u64;
        for sub in fs::read_dir(self.dir.join("chunks"))? {
            let sub = sub?;
            if !sub.file_type()?.is_dir() {
                continue;
            }
            for f in fs::read_dir(sub.path())? {
                let f = f?;
                let m = f.metadata()?;
                total += m.len();
                files.push((m.modified().unwrap_or(SystemTime::UNIX_EPOCH), m.len(), f.path()));
            }
        }
        files.sort();
        let mut freed = 0;
        for (_, len, path) in files {
            if total <= self.quota {
                break;
            }
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            if keep.contains(&name) {
                continue;
            }
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
}
