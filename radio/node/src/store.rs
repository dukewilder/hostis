//! What a node holds: station logs and the chunks they name, on its own disk.
//! Its own stations are never evicted. Everything else it carries lives under a
//! quota and leaves oldest-touched first.

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
        for f in fs::read_dir(dir.join("logs"))? {
            let f = f?;
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
            s.logs.insert(id, a);
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
        self.dir.join("logs").join(format!("{}.log", id.to_hex()))
    }

    /// Appends whatever entries extend the log, in order. Returns how many were new.
    pub fn put_entries(&mut self, id: StationId, entries: Vec<SignedEntry>) -> Result<usize> {
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
            file.write_all(&(bytes.len() as u32).to_be_bytes())?;
            file.write_all(&bytes)?;
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
        let key = self.keys.get(&id).ok_or_else(|| anyhow::anyhow!("not our station"))?;
        let a = self.logs.entry(id).or_insert_with(|| Archive::new(id));
        // A deputy wraps what it writes, so carriers can check the deputy's signature.
        let body = if key.id() == id { body } else { Body::Deputed { by: key.id().0, body: Box::new(body) } };
        let e = pyrate_core::Entry { station: id, seq: a.len(), prev: a.head(), at_ms, body };
        let signed = SignedEntry::sign(key, e)?;
        self.put_entries(id, vec![signed.clone()])?;
        Ok(signed)
    }

    /// A struck station's log and chunks go. The strike entry itself is kept,
    /// so the station is not fetched again.
    fn drop_station(&mut self, id: &StationId) -> Result<()> {
        let hashes = self.chunks_of(id);
        for h in hashes {
            let _ = fs::remove_file(self.chunk_path(&h));
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
                _ => {}
            }
        }
        out
    }

    fn chunk_path(&self, h: &Hash) -> PathBuf {
        let x = hex(h);
        self.dir.join("chunks").join(&x[..2]).join(x)
    }

    pub fn has_chunk(&self, h: &Hash) -> bool {
        self.chunk_path(h).exists()
    }

    pub fn put_chunk(&self, h: &Hash, bytes: &[u8]) -> Result<()> {
        if blake3::hash(bytes).as_bytes() != h {
            anyhow::bail!("chunk does not match its hash");
        }
        let p = self.chunk_path(h);
        fs::create_dir_all(p.parent().unwrap())?;
        let tmp = p.with_extension("part");
        fs::write(&tmp, bytes)?;
        fs::rename(tmp, p)?;
        Ok(())
    }

    pub fn get_chunk(&self, h: &Hash) -> Option<Vec<u8>> {
        let p = self.chunk_path(h);
        let bytes = fs::read(&p).ok()?;
        // Touch, so carrying keeps what is listened to.
        if let Ok(f) = fs::File::options().append(true).open(&p) {
            let _ = f.set_modified(SystemTime::now());
        }
        Some(bytes)
    }

    /// Evicts carried chunks, oldest-touched first, until under quota.
    pub fn enforce_quota(&self) -> Result<u64> {
        let keep: HashSet<String> = self.own().iter().chain(self.pinned.iter()).flat_map(|id| self.chunks_of(id)).map(|h| hex(&h)).collect();
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
