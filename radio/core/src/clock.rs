//! The clock. Every node works out what a station is playing from the station's
//! log and the time, and every node gets the same answer, to the millisecond.
//! No server says what is on. The log and the hour say it.
//!
//! The week is the unit. At 00:00 UTC each Monday the clock starts the week's
//! running order from the top, so working out "now" never walks back further
//! than seven days. Within the week, tracks play back to back. The library is
//! dealt in rounds: every track once, in an order seeded by the station, the
//! week and the round, before any track plays twice. A track added mid-week
//! joins the next round. Blocks put chosen tracks at chosen hours, and an
//! ident plays on the bell.

use serde::{Deserialize, Serialize};

use crate::{Archive, Body, Hash, Result};

const WEEK_MS: u64 = 7 * 24 * 3600 * 1000;
/// 1970-01-01 was a Thursday. Monday 1970-01-05 00:00 UTC.
const FIRST_MONDAY_MS: u64 = 4 * 24 * 3600 * 1000;
const DAY_MIN: u16 = 24 * 60;

/// A span of the day, in UTC minutes, given to a chosen set of tracks.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Block {
    pub start_min: u16,
    /// Exclusive. A block may run past midnight (start 22:00, end 02:00).
    pub end_min: u16,
    /// Track entry hashes. Empty means the whole library.
    pub tracks: Vec<Hash>,
    pub shuffle: bool,
}

impl Block {
    fn covers(&self, min: u16) -> bool {
        if self.start_min <= self.end_min {
            (self.start_min..self.end_min).contains(&min)
        } else {
            min >= self.start_min || min < self.end_min
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rota {
    /// Checked in order; the first block covering the minute wins. Outside every
    /// block the whole library plays, shuffled.
    pub blocks: Vec<Block>,
    /// A track played on the bell.
    pub ident: Option<Hash>,
    pub ident_every_min: u16,
}

impl Default for Rota {
    fn default() -> Self {
        Self { blocks: Vec::new(), ident: None, ident_every_min: 30 }
    }
}

/// What a station is playing at a given moment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NowPlaying {
    pub track: Hash,
    pub started_at_ms: u64,
    pub offset_ms: u64,
    pub duration_ms: u64,
    /// Which chunk to fetch first, and how far into it to begin.
    pub chunk: usize,
    pub chunk_offset_ms: u64,
}

#[derive(Clone, Debug)]
struct Track {
    hash: Hash,
    added_ms: u64,
    withdrawn_ms: Option<u64>,
    duration_ms: u64,
    chunk_ms: Vec<u64>,
}

impl Track {
    fn on_air(&self, t: u64) -> bool {
        self.added_ms <= t && self.withdrawn_ms.is_none_or(|w| t < w) && self.duration_ms > 0
    }
}

/// splitmix64, seeded from BLAKE3. Small, fast, the same on every machine.
struct Dice(u64);

impl Dice {
    fn new(parts: &[&[u8]]) -> Self {
        let mut h = blake3::Hasher::new();
        h.update(b"pyrate/clock/v1");
        for p in parts {
            h.update(&(p.len() as u32).to_le_bytes());
            h.update(p);
        }
        Self(u64::from_le_bytes(h.finalize().as_bytes()[..8].try_into().unwrap()))
    }

    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    fn shuffle<T>(&mut self, v: &mut [T]) {
        for i in (1..v.len()).rev() {
            let j = (self.next() % (i as u64 + 1)) as usize;
            v.swap(i, j);
        }
    }
}

/// A deck per block: the current round's order and how far through it the clock is.
struct Deck {
    round: u64,
    order: Vec<usize>,
    at: usize,
}

pub struct Clock {
    station: [u8; 32],
    rota: Rota,
    tracks: Vec<Track>,
}

impl Clock {
    pub fn new(archive: &Archive) -> Result<Self> {
        let mut tracks: Vec<Track> = Vec::new();
        for e in archive.entries() {
            match e.entry.body() {
                Body::Track { chunks, .. } => {
                    let chunk_ms: Vec<u64> = chunks.iter().map(|c| c.duration_ms()).collect();
                    tracks.push(Track {
                        hash: e.hash()?,
                        added_ms: e.entry.at_ms,
                        withdrawn_ms: None,
                        duration_ms: chunk_ms.iter().sum(),
                        chunk_ms,
                    });
                }
                Body::Withdraw { track } => {
                    if let Some(t) = tracks.iter_mut().find(|t| &t.hash == track) {
                        t.withdrawn_ms.get_or_insert(e.entry.at_ms);
                    }
                }
                _ => {}
            }
        }
        Ok(Self { station: archive.station().0, rota: archive.rota(), tracks })
    }

    fn block_at(&self, t: u64) -> Option<usize> {
        let min = ((t / 60_000) % DAY_MIN as u64) as u16;
        self.rota.blocks.iter().position(|b| b.covers(min))
    }

    /// Deals the next track for a block, starting a new round when the deck runs out.
    fn deal(&self, deck: &mut Deck, block: Option<usize>, week: u64, t: u64) -> Option<usize> {
        for _ in 0..2 {
            while deck.at < deck.order.len() {
                let i = deck.order[deck.at];
                deck.at += 1;
                if self.tracks[i].on_air(t) {
                    return Some(i);
                }
            }
            // A fresh round: everything on air now that the block allows.
            let (wanted, shuffle) = match block {
                Some(b) => (&self.rota.blocks[b].tracks, self.rota.blocks[b].shuffle),
                None => (&Vec::new(), true),
            };
            let pool = |all: bool| -> Vec<usize> {
                (0..self.tracks.len())
                    .filter(|&i| self.tracks[i].on_air(t))
                    .filter(|&i| all || wanted.contains(&self.tracks[i].hash))
                    .filter(|&i| Some(self.tracks[i].hash) != self.rota.ident)
                    .collect()
            };
            let mut order = pool(wanted.is_empty());
            if order.is_empty() && !wanted.is_empty() {
                // A block whose tracks are all gone falls back to the whole library.
                order = pool(true);
            }
            if order.is_empty() {
                return None;
            }
            deck.round += 1;
            if shuffle {
                let b = block.map(|b| b as u64).unwrap_or(u64::MAX);
                Dice::new(&[&self.station, &week.to_le_bytes(), &b.to_le_bytes(), &deck.round.to_le_bytes()])
                    .shuffle(&mut order);
            }
            deck.order = order;
            deck.at = 0;
        }
        None
    }

    /// What is on at `t`, in Unix milliseconds. None is dead air: an empty library.
    /// What plays between two moments: each track's start, its entry hash and its length.
    /// The watch bill, worked out on the device; no server is asked.
    pub fn upcoming(&self, from: u64, until: u64) -> Vec<(u64, Hash, u64)> {
        let mut out = Vec::new();
        let mut t = from;
        while t < until && out.len() < 2000 {
            // A quiet minute (nothing yet added, or a gap between blocks): step past it.
            let Some(np) = self.now(t) else {
                t = (t / 60_000 + 1) * 60_000;
                continue;
            };
            let start = t - np.offset_ms;
            out.push((start, np.track, np.duration_ms));
            t = start + np.duration_ms.max(1000) + 1;
        }
        out
    }

    pub fn now(&self, t: u64) -> Option<NowPlaying> {
        if t < FIRST_MONDAY_MS {
            return None;
        }
        let week = (t - FIRST_MONDAY_MS) / WEEK_MS;
        let mut cursor = FIRST_MONDAY_MS + week * WEEK_MS;
        let mut decks: Vec<Option<Deck>> = (0..=self.rota.blocks.len()).map(|_| None).collect();
        let mut last_ident: Option<u64> = None;
        let ident = self.rota.ident.and_then(|h| self.tracks.iter().position(|tr| tr.hash == h));
        let bell = self.rota.ident_every_min.max(1) as u64 * 60_000;

        loop {
            // An empty stretch: skip ahead a minute and look again.
            let pick = if let Some(i) = ident.filter(|&i| {
                self.tracks[i].on_air(cursor) && last_ident.is_none_or(|l| cursor - l >= bell)
            }) {
                last_ident = Some(cursor);
                Some(i)
            } else {
                let block = self.block_at(cursor);
                let slot = block.map(|b| b + 1).unwrap_or(0);
                let deck = decks[slot].get_or_insert_with(|| Deck { round: 0, order: Vec::new(), at: 0 });
                self.deal(deck, block, week, cursor)
            };
            let Some(i) = pick else {
                cursor = (cursor / 60_000 + 1) * 60_000;
                if cursor > t {
                    return None;
                }
                continue;
            };
            let tr = &self.tracks[i];
            if cursor + tr.duration_ms > t {
                let offset = t - cursor;
                let mut chunk = 0;
                let mut left = offset;
                while chunk < tr.chunk_ms.len() && left >= tr.chunk_ms[chunk] {
                    left -= tr.chunk_ms[chunk];
                    chunk += 1;
                }
                return Some(NowPlaying {
                    track: tr.hash,
                    started_at_ms: cursor,
                    offset_ms: offset,
                    duration_ms: tr.duration_ms,
                    chunk,
                    chunk_offset_ms: left,
                });
            }
            cursor += tr.duration_ms;
        }
    }
}
