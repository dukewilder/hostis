//! Updates carried as a station, and each node's own choice of what it carries.
//!
//! The Releases station is release key one; its log holds versions of the app, each signed by
//! two of the three release keys. It spreads like any station, so it needs no server and
//! cannot be taken down from one place. There is no shared list of stations to refuse: each
//! node decides for itself, privately, and tells no one.
//!
//! No rights reserved.

use std::{collections::HashSet, path::PathBuf, sync::Arc};

use anyhow::{Result, anyhow};
use pyrate_core::{Body, StationId, release_trusted};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::{Inner, now_ms, say};

/// Keeps a sync call usable from the JSON door, which is not async.
pub trait AwaitIn: std::future::Future + Sized {
    fn await_in(self, rt: &tokio::runtime::Runtime) -> Self::Output {
        rt.block_on(self)
    }
}
impl<F: std::future::Future> AwaitIn for F {}

const MAX_APK: u64 = 200_000_000;

impl Inner {
    fn release_station(&self) -> Option<StationId> {
        self.cfg.release_keys.first().map(|k| StationId(*k))
    }

    pub(crate) fn special_ids(&self) -> Vec<StationId> {
        self.release_station().into_iter().collect()
    }

    pub(crate) fn special(&self, id: &StationId) -> bool {
        self.special_ids().contains(id)
    }

    /// Stations this node will not list, play or carry: the owner's marooned stations, plus
    /// any ids a Lantern operator puts in maroon.txt (one per line). Kept on this device only.
    pub fn refused(&self) -> HashSet<StationId> {
        {
            let c = self.refused_cache.lock().unwrap();
            if now_ms().saturating_sub(c.0) < 5_000 {
                return c.1.clone();
            }
        }
        let set = self.refused_fresh();
        *self.refused_cache.lock().unwrap() = (now_ms(), set.clone());
        set
    }

    /// Forgets the cached list, after the owner maroons or brings back a station.
    pub(crate) fn refused_changed(&self) {
        self.refused_cache.lock().unwrap().0 = 0;
    }

    fn refused_fresh(&self) -> HashSet<StationId> {
        let (_, marooned) = self.chart();
        let file = std::fs::read_to_string(self.cfg.dir.join("maroon.txt")).unwrap_or_default();
        marooned.iter().map(String::as_str).chain(file.lines().map(|l| l.split_whitespace().next().unwrap_or("")))
            .filter_map(|h| StationId::from_hex(h).ok()).collect()
    }

    /// Cheap check for the request path, which already holds the store lock.
    pub(crate) fn refused_quick(&self, id: &StationId) -> bool {
        self.refused_cache.lock().unwrap().1.contains(id)
    }

    /// A marooned station's recordings leave the hold and the ballast.
    pub(crate) fn drop_refused(&self) {
        let refused = self.refused();
        if refused.is_empty() {
            return;
        }
        let mut s = self.store.lock().unwrap();
        for id in refused {
            if s.keys.contains_key(&id) {
                continue;
            }
            let pics = s.logs.get(&id).map(|a| a.art_hashes()).unwrap_or_default();
            for h in s.chunks_of(&id).into_iter().chain(pics) {
                s.remove_chunk(&h);
            }
            s.pinned.remove(&id);
            s.forget_log(&id);
        }
    }

    /// The newest release this build can trust and install, fetched and checked.
    pub async fn check_update(self: &Arc<Self>, current: u32) -> Result<Value> {
        let st = self.release_station().ok_or_else(|| anyhow!("this build trusts no release keys yet"))?;
        let _ = self.sync_log(st).await;
        let best = {
            let s = self.store.lock().unwrap();
            let a = s.logs.get(&st).ok_or_else(|| anyhow!("no releases heard yet"))?;
            a.entries().iter().filter_map(|e| match e.entry.body() {
                Body::Release { version, code, sha256, size, chunks, notes, sigs }
                    if *code > current && *size <= MAX_APK && release_trusted(&self.cfg.release_keys, *code, sha256, sigs) =>
                    Some((version.clone(), *code, *sha256, chunks.clone(), notes.clone())),
                _ => None,
            }).max_by_key(|r| r.1)
        };
        let Some((version, code, sha, chunks, notes)) = best else { return Ok(json!({ "available": false })) };
        let dir = self.cfg.dir.join("updates");
        std::fs::create_dir_all(&dir)?;
        let path: PathBuf = dir.join(format!("pyrate-{code}.apk"));
        if !path.exists() {
            let mut apk = Vec::new();
            for c in &chunks {
                apk.extend(self.chunk(&st, &c.hash).await?);
            }
            if Sha256::digest(&apk).as_slice() != sha {
                return Err(anyhow!("the assembled release does not match its signed hash"));
            }
            std::fs::write(&path, apk)?;
            say(&format!("release {version} aboard and checked"));
        }
        Ok(json!({ "available": true, "version": version, "code": code, "notes": notes, "path": path.to_string_lossy() }))
    }

    /// What a Lantern holding the Releases deputy does on its own: publish what the operator
    /// drops in its releases folder.
    pub(crate) async fn lantern_duties(self: &Arc<Self>) {
        self.keep_rooms().await;
        if let Some(st) = self.release_station().filter(|s| self.store.lock().unwrap().keys.contains_key(s)) {
            if let Err(e) = self.publish_releases(st) {
                say(&format!("releases: {e}"));
            }
        }
    }

    /// releases/<code>-<version>.apk with releases/<code>-<version>.sigs beside it.
    fn publish_releases(&self, st: StationId) -> Result<()> {
        let dir = self.cfg.dir.join("releases");
        let Ok(rd) = std::fs::read_dir(&dir) else { return Ok(()) };
        let have: HashSet<u32> = {
            let s = self.store.lock().unwrap();
            s.logs.get(&st).map(|a| a.entries().iter().filter_map(|e| match e.entry.body() {
                Body::Release { code, .. } => Some(*code),
                _ => None,
            }).collect()).unwrap_or_default()
        };
        for f in rd.flatten() {
            let p = f.path();
            if p.extension().is_none_or(|e| e != "apk") {
                continue;
            }
            let stem = p.file_stem().unwrap().to_string_lossy().to_string();
            let Some((code, version)) = stem.split_once('-') else { continue };
            let Ok(code) = code.parse::<u32>() else { continue };
            if have.contains(&code) {
                continue;
            }
            let apk = std::fs::read(&p)?;
            let sha: [u8; 32] = Sha256::digest(&apk).into();
            let sigs: Vec<(u8, Vec<u8>)> = std::fs::read_to_string(p.with_extension("sigs")).unwrap_or_default().lines().filter_map(|l| {
                let (i, h) = l.trim().split_once(' ')?;
                let mut sig = vec![0u8; h.len() / 2];
                pyrate_core::unhex_into(h, &mut sig)?;
                Some((i.parse().ok()?, sig))
            }).collect();
            if !release_trusted(&self.cfg.release_keys, code, &sha, &sigs) {
                say(&format!("release {code} skipped: it needs two of the three release signatures"));
                continue;
            }
            let notes = std::fs::read_to_string(p.with_extension("txt")).unwrap_or_default();
            let mut chunks = Vec::new();
            let mut s = self.store.lock().unwrap();
            for piece in apk.chunks(1_000_000) {
                let c = pyrate_core::Chunk::of(piece, 0);
                s.put_chunk(&c.hash, piece)?;
                chunks.push(c);
            }
            s.write(st, now_ms(), Body::Release { version: version.into(), code, sha256: sha, size: apk.len() as u64, chunks, notes, sigs })?;
            say(&format!("published release {version} ({code})"));
        }
        Ok(())
    }
}
