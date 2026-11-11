//! A station as one file: its signed log, its card and every chunk of its sound and art.
//!
//! A pack passes by SD card, USB stick, Bluetooth or a phone's share sheet when the network is
//! cut or watched, and the phone that opens it plays the station at once and serves it to every
//! node it meets. Nothing in a pack is taken on trust: each entry is checked against the
//! station's key, each chunk against the hash the signed log names, and the card against its
//! signature. A pack can be carried by anyone and altered by no one.
//!
//! No rights reserved.

use std::{collections::HashSet, io::{BufReader, BufWriter, Read, Write}, path::Path, sync::Arc, time::Duration};

use anyhow::{Result, anyhow, bail};
use pyrate_core::{Hash, SignedEntry, StationId};

use crate::{Inner, net::{Announce, Gossip}};

const MAGIC: &[u8] = b"pyrate/pack/v1\n";
/// The most any one piece of a pack may claim to be: larger than any entry or chunk.
const MAX_PIECE: u32 = 16 * 1024 * 1024;

fn put(w: &mut impl Write, b: &[u8]) -> Result<()> {
    w.write_all(&(b.len() as u32).to_be_bytes())?;
    w.write_all(b)?;
    Ok(())
}

fn take(r: &mut impl Read) -> Result<Option<Vec<u8>>> {
    let mut n = [0u8; 4];
    match r.read_exact(&mut n) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e.into()),
    }
    let n = u32::from_be_bytes(n);
    if n > MAX_PIECE {
        bail!("not a station file");
    }
    let mut b = vec![0u8; n as usize];
    r.read_exact(&mut b)?;
    Ok(Some(b))
}

impl Inner {
    /// Writes a station to `path`, fetching any chunk this node lacks. Returns how many
    /// entries and chunks went in, and how many chunks could not be found.
    pub(crate) async fn pack_export(self: &Arc<Self>, station: StationId, path: &Path) -> Result<(usize, usize, usize)> {
        let _ = tokio::time::timeout(Duration::from_secs(20), self.sync_log(station)).await;
        let (entries, hashes) = {
            let s = self.store.lock().unwrap();
            let a = s.logs.get(&station).filter(|a| a.len() > 0).ok_or_else(|| anyhow!("this station's log is not here yet"))?;
            let mut h = s.chunks_of(&station);
            h.extend(a.art_hashes());
            let mut seen = HashSet::new();
            h.retain(|x| seen.insert(*x));
            (a.entries().iter().filter_map(|e| e.to_bytes().ok()).collect::<Vec<_>>(), h)
        };
        let card = self.log.lock().unwrap().get(&station).and_then(|l| l.announce.clone());
        let tmp = path.with_extension("part");
        let mut w = BufWriter::new(std::fs::File::create(&tmp)?);
        w.write_all(MAGIC)?;
        w.write_all(&station.0)?;
        put(&mut w, &card.as_ref().and_then(|c| postcard::to_stdvec(c).ok()).unwrap_or_default())?;
        w.write_all(&(entries.len() as u32).to_be_bytes())?;
        for e in &entries {
            put(&mut w, e)?;
        }
        let (mut wrote, mut missing) = (0, 0);
        for h in hashes {
            let have = self.store.lock().unwrap().get_chunk(&h);
            let bytes = match have {
                Some(b) => Some(b),
                None => tokio::time::timeout(Duration::from_secs(20), self.chunk(&station, &h)).await.ok().and_then(|r| r.ok()),
            };
            match bytes {
                Some(b) => {
                    w.write_all(&h)?;
                    put(&mut w, &b)?;
                    wrote += 1;
                }
                None => missing += 1,
            }
        }
        w.flush()?;
        drop(w);
        std::fs::rename(&tmp, path)?;
        Ok((entries.len(), wrote, missing))
    }

    /// Opens a station file: the log, checked entry by entry; the chunks the log names, each
    /// checked against its hash; and the card, checked against its signature.
    pub(crate) fn pack_import(self: &Arc<Self>, path: &Path) -> Result<StationId> {
        let mut r = BufReader::new(std::fs::File::open(path)?);
        let mut magic = vec![0u8; MAGIC.len()];
        r.read_exact(&mut magic)?;
        if magic != MAGIC {
            bail!("not a station file");
        }
        let mut id = [0u8; 32];
        r.read_exact(&mut id)?;
        let station = StationId(id);
        let card = take(&mut r)?.unwrap_or_default();
        let mut n = [0u8; 4];
        r.read_exact(&mut n)?;
        let mut entries = Vec::new();
        for _ in 0..u32::from_be_bytes(n).min(1_000_000) {
            let b = take(&mut r)?.ok_or_else(|| anyhow!("the file ends early"))?;
            let e = SignedEntry::from_bytes(&b).map_err(|_| anyhow!("an entry in the file is damaged"))?;
            if e.entry.station != station {
                bail!("an entry in the file belongs to another station");
            }
            entries.push(e);
        }
        // Entries the station's key did not sign are refused here, as from any peer.
        self.store.lock().unwrap().put_entries(station, entries)?;
        let named: HashSet<Hash> = {
            let s = self.store.lock().unwrap();
            let mut h: HashSet<Hash> = s.chunks_of(&station).into_iter().collect();
            h.extend(s.logs.get(&station).map(|a| a.art_hashes()).unwrap_or_default());
            h
        };
        loop {
            let mut h = [0u8; 32];
            match r.read_exact(&mut h) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(e.into()),
            }
            let b = take(&mut r)?.ok_or_else(|| anyhow!("the file ends early"))?;
            if named.contains(&h) {
                // put_chunk refuses bytes that do not hash to the name the log gave them.
                let _ = self.store.lock().unwrap().put_chunk(&h, &b);
            }
        }
        if let Ok(a) = postcard::from_bytes::<Announce>(&card) {
            if a.station == station {
                self.on_gossip(Gossip::Station(a));
            }
        }
        Ok(station)
    }
}
