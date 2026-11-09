//! Station art on a node.
//!
//! Art is never ballast: no phone carries a picture for the fleet. A picture lands on the
//! device only for a station the owner keeps, is tuned to, or broadcasts, and then it is
//! sealed like any recording. Everything seen while browsing lives in memory and is gone
//! when the app closes. Lanterns carry art for the stations they carry.
//!
//! No rights reserved.

use std::{collections::VecDeque, path::Path, sync::Arc};

use anyhow::{Result, anyhow};
use pyrate_core::{ArtRef, Body, Hash, StationId, art};

use crate::{Inner, now_ms, say};

const SHELF_BYTES: usize = 24 << 20;

/// Art held in memory: newest kept, oldest dropped past 24 MB.
#[derive(Default)]
pub struct Shelf {
    items: VecDeque<(Hash, Vec<u8>)>,
    bytes: usize,
    pub(crate) asked: std::collections::HashSet<Hash>,
}

impl Shelf {
    pub(crate) fn get(&self, h: &Hash) -> Option<Vec<u8>> {
        self.items.iter().find(|(k, _)| k == h).map(|(_, v)| v.clone())
    }

    pub(crate) fn put(&mut self, h: Hash, b: Vec<u8>) {
        if self.get(&h).is_some() {
            return;
        }
        self.bytes += b.len();
        self.items.push_back((h, b));
        while self.bytes > SHELF_BYTES {
            let Some((_, old)) = self.items.pop_front() else { break };
            self.bytes -= old.len();
        }
    }
}

/// A cover picture in the same folder as a recording, as music libraries keep them.
pub fn folder_cover(track: &Path) -> Option<Vec<u8>> {
    let dir = track.parent()?;
    for name in ["cover", "folder", "front", "album", "Cover", "Folder", "Front", "Album"] {
        for ext in ["jpg", "jpeg", "png", "webp"] {
            if let Ok(b) = std::fs::read(dir.join(format!("{name}.{ext}"))) {
                return Some(b);
            }
        }
    }
    None
}

impl Inner {
    /// Whether this device may write a station's art to its hold.
    fn art_kept_here(&self, st: &StationId) -> bool {
        if (self.cfg.lantern && self.cfg.carry_all) || self.is_own(st) {
            return true;
        }
        if self.tuned.lock().unwrap().as_ref().is_some_and(|t| &t.station == st) {
            return true;
        }
        self.chart().0.contains(&st.to_hex())
    }

    /// The bytes of a picture, from the hold, from memory, or from the fleet.
    pub async fn art_bytes(self: &Arc<Self>, st: StationId, h: Hash) -> Result<Vec<u8>> {
        if self.refused().contains(&st) {
            return Err(anyhow!("marooned"));
        }
        if let Some(b) = self.store.lock().unwrap().get_chunk(&h) {
            return Ok(b);
        }
        if let Some(b) = self.art_mem.lock().unwrap().get(&h) {
            return Ok(b);
        }
        let keep = self.art_kept_here(&st);
        let b = self.fetch(&st, &h, keep).await?;
        if b.len() > art::MAX_BYTES {
            return Err(anyhow!("too large to be art"));
        }
        if !keep {
            self.art_mem.lock().unwrap().put(h, b.clone());
        }
        Ok(b)
    }

    /// Starts fetching a picture if it is not already here or on its way.
    pub fn art_want(self: &Arc<Self>, st: StationId, h: Hash) {
        if self.art_ready(&h) || !self.art_mem.lock().unwrap().asked.insert(h) {
            return;
        }
        let me = self.clone();
        self.handle.spawn(async move {
            if let Err(e) = me.art_bytes(st, h).await {
                say(&format!("art not found yet: {e}"));
            }
            me.art_mem.lock().unwrap().asked.remove(&h);
        });
    }

    fn art_ready(&self, h: &Hash) -> bool {
        self.art_mem.lock().unwrap().get(h).is_some() || self.store.lock().unwrap().has_chunk(h)
    }

    /// A picture decoded to RGBA for the app, if it is here.
    pub fn art_pixels(&self, h: &Hash) -> Option<(u32, u32, Vec<u8>)> {
        let b = self.art_mem.lock().unwrap().get(h).or_else(|| self.store.lock().unwrap().get_chunk(h))?;
        art::decode(&b).ok()
    }

    /// Raises new art for one of our stations: a cover or a banner, from any picture.
    pub fn set_art(&self, st: StationId, kind: &str, raw: &[u8]) -> Result<()> {
        let k = if kind == "banner" { art::Kind::Banner } else { art::Kind::Cover };
        let p = art::prepare(raw, k).map_err(|e| anyhow!(e))?;
        let hash = *blake3::hash(&p.jpeg).as_bytes();
        let mut s = self.store.lock().unwrap();
        let (mut cover, mut banner, mut accent) = s.logs.get(&st).and_then(|a| a.art()).unwrap_or((None, None, [0xED, 0xE4, 0xCE]));
        s.put_chunk(&hash, &p.jpeg)?;
        let r = Some(ArtRef { hash, w: p.w, h: p.h });
        if kind == "banner" {
            banner = r;
        } else {
            cover = r;
            accent = p.accent;
        }
        s.write(st, now_ms(), Body::Art { cover, banner, accent })?;
        Ok(())
    }

    /// Takes art down. The station's colour falls back to bone when the cover goes.
    pub fn clear_art(&self, st: StationId, kind: &str) -> Result<()> {
        let mut s = self.store.lock().unwrap();
        let Some((mut cover, mut banner, mut accent)) = s.logs.get(&st).and_then(|a| a.art()) else { return Ok(()) };
        if kind == "banner" {
            banner = None;
        } else {
            cover = None;
            accent = [0xED, 0xE4, 0xCE];
        }
        s.write(st, now_ms(), Body::Art { cover, banner, accent })?;
        Ok(())
    }
}
