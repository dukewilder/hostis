//! Updates and refusal lists, carried as stations.
//!
//! The Releases station is release key one; its log holds versions of the app, each signed by
//! two of the three release keys. The HOSTIS refusal list is release key two; its log holds
//! stations that nodes following it will not list, play or carry. Both spread like any station,
//! so neither needs a server, and neither can be taken down from one place.
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

    fn list_station(&self) -> Option<StationId> {
        self.cfg.release_keys.get(1).map(|k| StationId(*k))
    }

    pub(crate) fn special_ids(&self) -> Vec<StationId> {
        self.release_station().into_iter().chain(self.list_station()).collect()
    }

    pub(crate) fn special(&self, id: &StationId) -> bool {
        self.special_ids().contains(id)
    }

    pub fn following(&self) -> bool {
        !self.cfg.dir.join("unfollow-lists").exists()
    }

    pub fn set_following(&self, on: bool) -> Result<()> {
        let p = self.cfg.dir.join("unfollow-lists");
        if on {
            let _ = std::fs::remove_file(p);
        } else {
            std::fs::write(p, b"")?;
        }
        Ok(())
    }

    /// Stations refused by the lists this node follows.
    pub fn refused(&self) -> HashSet<StationId> {
        let mut out = HashSet::new();
        if !self.following() {
            return out;
        }
        let Some(list) = self.list_station() else { return out };
        let store = self.store.lock().unwrap();
        let Some(a) = store.logs.get(&list) else { return out };
        for e in a.entries() {
            match e.entry.body() {
                Body::Refuse { station, .. } => {
                    out.insert(StationId(*station));
                }
                Body::Unrefuse { station } => {
                    out.remove(&StationId(*station));
                }
                _ => {}
            }
        }
        out
    }

    /// Cheap check for the request path, which already holds the store lock.
    pub(crate) fn refused_quick(&self, _id: &StationId) -> bool {
        false
    }

    /// A refused station's recordings leave the hold and the ballast.
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
            for h in s.chunks_of(&id) {
                s.remove_chunk(&h);
            }
            s.pinned.remove(&id);
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

    /// What a Lantern holding the Releases or refusal-list deputy does on its own: publish what
    /// the operator drops in its folders.
    pub(crate) async fn lantern_duties(self: &Arc<Self>) {
        if let Some(st) = self.release_station().filter(|s| self.store.lock().unwrap().keys.contains_key(s)) {
            if let Err(e) = self.publish_releases(st) {
                say(&format!("releases: {e}"));
            }
        }
        if let Some(st) = self.list_station().filter(|s| self.store.lock().unwrap().keys.contains_key(s)) {
            if let Err(e) = self.publish_refusals(st) {
                say(&format!("refusals: {e}"));
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
                Some((i.parse().ok()?, (0..h.len() / 2).map(|j| u8::from_str_radix(&h[j * 2..j * 2 + 2], 16)).collect::<Result<Vec<u8>, _>>().ok()?))
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

    /// refusals.txt: one station id per line, a space, then the reason. Removing a line unrefuses.
    fn publish_refusals(&self, st: StationId) -> Result<()> {
        let Ok(text) = std::fs::read_to_string(self.cfg.dir.join("refusals.txt")) else { return Ok(()) };
        let wanted: Vec<(StationId, String)> = text.lines().filter_map(|l| {
            let (id, why) = l.trim().split_once(' ').unwrap_or((l.trim(), ""));
            Some((StationId::from_hex(id).ok()?, why.to_string()))
        }).collect();
        let now: HashSet<StationId> = {
            let follow = self.following();
            let r = if follow { self.refused() } else { HashSet::new() };
            r
        };
        let mut s = self.store.lock().unwrap();
        for (id, why) in &wanted {
            if !now.contains(id) {
                s.write(st, now_ms(), Body::Refuse { station: id.0, reason: why.clone() })?;
            }
        }
        let keep: HashSet<StationId> = wanted.iter().map(|(i, _)| *i).collect();
        for id in now.difference(&keep) {
            s.write(st, now_ms(), Body::Unrefuse { station: id.0 })?;
        }
        Ok(())
    }
}
