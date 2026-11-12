//! The clock. Every node works out what a station is playing from the station's
//! log and the time, and every node gets the same answer, to the millisecond.
//! No server says what is on. The log and the hour say it.
//!
//! Two programmes exist. The older rota works in UTC: the week starts at 00:00 UTC each
//! Monday, the library is dealt in rounds, and blocks put chosen tracks at chosen hours.
//! The programme works in the station's own local time: a mode for the library (shuffled,
//! in order, newest first, a daily tape or a weekly tape), the hours it is on the air,
//! weekly slots for shows, watches and live hours, and an ident on the bell. One-off shows
//! override both. Whichever of rota and programme is later in the log rules.

use serde::{Deserialize, Serialize};

use crate::{Archive, Body, Hash, Result};
use crate::programme::{Fill, Mode, Programme, Zone};

const WEEK_MS: u64 = 7 * 24 * 3600 * 1000;
/// 1970-01-01 was a Thursday. Monday 1970-01-05 00:00 UTC.
const FIRST_MONDAY_MS: u64 = 4 * 24 * 3600 * 1000;
const DAY_MIN: u16 = 24 * 60;
/// A walk never takes more steps than this, however short the tracks.
const MAX_STEPS: usize = 1_300_000;
/// Steps the watch bill may spend in all, so a long window of short tracks stays quick.
const BILL_STEPS: usize = 4_000_000;
/// The most steps a bill of what is coming walks: a few hours of one-second tracks, a week of
/// anything real.
pub const UPCOMING_STEPS: usize = 20_000;
/// A recording shorter than this is not played: a second is the shortest thing worth airing,
/// and it keeps a week's walk within its steps whatever a station signs.
pub const MIN_TRACK_MS: u64 = 1_000;

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
    /// The show or slot on the air, if one is.
    pub slot: Option<String>,
}

/// A stretch of the schedule, for a station's page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Listing {
    pub start_ms: u64,
    pub end_ms: u64,
    pub title: String,
    pub about: String,
    /// "show", "slot" or "hours".
    pub kind: &'static str,
    pub live: bool,
    /// The show's entry hash, for a one-off show.
    pub show: Option<Hash>,
}

#[derive(Clone, Debug)]
struct Track {
    hash: Hash,
    added_ms: u64,
    withdrawn_ms: Option<u64>,
    duration_ms: u64,
    chunk_ms: Vec<u64>,
    /// The key that added it: the station's own, or a hand's.
    by: [u8; 32],
}

impl Track {
    fn on_air(&self, t: u64) -> bool {
        self.added_ms <= t && self.withdrawn_ms.is_none_or(|w| t < w) && self.duration_ms >= MIN_TRACK_MS
    }
}

#[derive(Clone, Debug)]
struct Show {
    hash: Hash,
    title: String,
    about: String,
    at_ms: u64,
    /// The moment its recordings are judged on the air: its start, or when it was put on the
    /// bill if that was later. A premiere set for this minute of a recording added seconds
    /// ago still airs it.
    as_of: u64,
    tracks: Vec<usize>,
    live_ms: u64,
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

/// Where a walk takes its tracks from.
enum Source {
    /// Rounds from a pool, each track once a round; shuffled with this seed.
    Rounds { pool: Vec<usize>, shuffle: Option<Vec<u8>>, round: u64, order: Vec<usize>, at: usize },
    /// A fixed order, round and round.
    Cycle { order: Vec<usize>, at: usize },
    /// A fixed order, once, each track judged on the air as of one moment.
    Once { order: Vec<usize>, at: usize, as_of: u64 },
}

impl Source {
    fn next(&mut self, tracks: &[Track], t: u64) -> Option<usize> {
        match self {
            Source::Once { order, at, as_of } => {
                while *at < order.len() {
                    let i = order[*at];
                    *at += 1;
                    if tracks[i].on_air(*as_of) {
                        return Some(i);
                    }
                }
                None
            }
            Source::Cycle { order, at } => {
                if order.is_empty() {
                    return None;
                }
                for _ in 0..order.len() * 2 + 1 {
                    if *at >= order.len() {
                        *at = 0;
                    }
                    let i = order[*at];
                    *at += 1;
                    if tracks[i].on_air(t) {
                        return Some(i);
                    }
                }
                None
            }
            Source::Rounds { pool, shuffle, round, order, at } => {
                for _ in 0..2 {
                    while *at < order.len() {
                        let i = order[*at];
                        *at += 1;
                        if tracks[i].on_air(t) {
                            return Some(i);
                        }
                    }
                    let mut fresh: Vec<usize> = pool.iter().copied().filter(|&i| tracks[i].on_air(t)).collect();
                    if fresh.is_empty() {
                        return None;
                    }
                    *round += 1;
                    if let Some(seed) = shuffle {
                        Dice::new(&[seed, &round.to_le_bytes()]).shuffle(&mut fresh);
                    }
                    *order = fresh;
                    *at = 0;
                }
                None
            }
        }
    }
}

pub struct Clock {
    station: [u8; 32],
    rota: Rota,
    programme: Option<Programme>,
    tracks: Vec<Track>,
    /// Each track's place in `tracks`, by its entry hash.
    index: std::collections::HashMap<Hash, usize>,
    shows: Vec<Show>,
    /// Steps walked since the counter was last set, for the watch bill's budget.
    steps: std::sync::atomic::AtomicUsize,
    /// Whether whole rounds are skipped by arithmetic (always, but for the tests that check
    /// skipping changes nothing).
    skip: std::sync::atomic::AtomicBool,
}

impl Clock {
    pub fn new(archive: &Archive) -> Result<Self> {
        let mut tracks: Vec<Track> = Vec::new();
        let mut billed: std::collections::HashMap<Hash, u64> = std::collections::HashMap::new();
        let station = archive.station().0;
        for e in archive.entries() {
            let by = match &e.entry.body {
                Body::Deputed { by, .. } => *by,
                _ => station,
            };
            match e.entry.body() {
                Body::Track { chunks, .. } => {
                    let chunk_ms: Vec<u64> = chunks.iter().map(|c| c.duration_ms()).collect();
                    tracks.push(Track {
                        hash: e.hash()?,
                        added_ms: e.entry.at_ms,
                        withdrawn_ms: None,
                        duration_ms: chunk_ms.iter().sum(),
                        chunk_ms,
                        by,
                    });
                }
                Body::Withdraw { track } => {
                    if let Some(t) = tracks.iter_mut().rev().find(|t| &t.hash == track) {
                        t.withdrawn_ms.get_or_insert(e.entry.at_ms);
                    }
                }
                Body::Show { .. } => {
                    billed.insert(e.hash()?, e.entry.at_ms);
                }
                _ => {}
            }
        }
        let index: std::collections::HashMap<Hash, usize> = tracks.iter().enumerate().map(|(i, t)| (t.hash, i)).collect();
        let shows = archive
            .shows()
            .into_iter()
            .filter_map(|(hash, b)| match b {
                Body::Show { title, about, at_ms, tracks: list, live_min } => Some(Show {
                    hash,
                    title: title.clone(),
                    about: about.clone(),
                    at_ms: *at_ms,
                    as_of: (*at_ms).max(billed.get(&hash).copied().unwrap_or(0)),
                    tracks: list.iter().filter_map(|h| index.get(h).copied()).collect(),
                    live_ms: *live_min as u64 * 60_000,
                }),
                _ => None,
            })
            .collect();
        let programme = archive.programme().map(|mut p| {
            // The station's word, made sane: whatever a log says, a day is a day and a week a week.
            for (_, o) in p.zone.offsets.iter_mut() {
                *o = (*o).clamp(-14 * 60, 14 * 60);
            }
            p.zone.offsets.sort_by_key(|o| o.0);
            p.zone.offsets.dedup_by_key(|o| o.0);
            for s in p.slots.iter_mut().map(|s| &mut s.span).chain(p.hours.iter_mut()) {
                s.start_min %= DAY_MIN;
                s.len_min = s.len_min.min(7 * DAY_MIN);
            }
            if let Mode::Daily { start_min } | Mode::Weekly { start_min, .. } = &mut p.mode {
                *start_min %= DAY_MIN;
            }
            if let Mode::Weekly { day, .. } = &mut p.mode {
                *day %= 7;
            }
            p
        });
        Ok(Self { station, rota: archive.rota(), programme, tracks, index, shows, steps: std::sync::atomic::AtomicUsize::new(0), skip: std::sync::atomic::AtomicBool::new(true) })
    }

    /// The same clock walking every track, for tests that compare.
    #[doc(hidden)]
    pub fn stepping(self) -> Self {
        self.skip.store(false, std::sync::atomic::Ordering::Relaxed);
        self
    }

    /// Whole rounds of `members` that fit between `cursor` and `t` before any of them comes
    /// on or goes off the air: how many, and how long one lasts. A round's length is the same
    /// whatever its order, so a week of short tracks is crossed in a few steps, not hundreds
    /// of thousands.
    fn whole_rounds(&self, members: &[usize], cursor: u64, t: u64) -> Option<(u64, u64)> {
        if !self.skip.load(std::sync::atomic::Ordering::Relaxed) || members.is_empty() {
            return None;
        }
        let mut change = u64::MAX;
        let mut d = 0u64;
        for &i in members {
            let tr = &self.tracks[i];
            if tr.added_ms > cursor {
                change = change.min(tr.added_ms);
            }
            if let Some(w) = tr.withdrawn_ms.filter(|w| *w > cursor) {
                change = change.min(w);
            }
            if tr.on_air(cursor) {
                d = d.saturating_add(tr.duration_ms);
            }
        }
        let limit = t.min(change);
        if d == 0 || cursor.saturating_add(d) > limit {
            return None;
        }
        Some(((limit - cursor) / d, d))
    }

    /// What plays between two moments: each piece's start, its entry hash and how long it
    /// is on. The watch bill, worked out on the device; no server is asked. A recording is
    /// cut wherever the air changes hands (a show or slot begins or ends, the hours end, a
    /// tape starts over, the week turns), and one that resumes after a cut is listed from
    /// where it resumes.
    pub fn upcoming(&self, from: u64, until: u64) -> Vec<(u64, Hash, u64)> {
        self.upcoming_n(from, until, 2000)
    }

    /// The next thing on the air from `from`, if anything is before `until`.
    pub fn next_on(&self, from: u64, until: u64) -> Option<(u64, Hash, u64)> {
        self.upcoming_n(from, until, 1).into_iter().next()
    }

    /// As [`upcoming`](Self::upcoming), stopping at `n` entries. However short a station's
    /// recordings, the walk takes at most `UPCOMING_STEPS` steps: a library of one-second
    /// tracks cannot make a week's bill cost seconds.
    pub fn upcoming_n(&self, from: u64, until: u64, n: usize) -> Vec<(u64, Hash, u64)> {
        let cuts = self.cuts(from, until);
        let mut out: Vec<(u64, Hash, u64)> = Vec::new();
        let mut last_started = 0u64;
        let mut t = from;
        let mut walked = 0usize;
        self.steps.store(0, std::sync::atomic::Ordering::Relaxed);
        while t < until && out.len() < n && walked < UPCOMING_STEPS && self.steps.load(std::sync::atomic::Ordering::Relaxed) < BILL_STEPS {
            walked += 1;
            let next_cut = |after: u64| cuts.iter().copied().find(|c| *c > after);
            // A quiet minute (nothing yet added, off the air, a gap between blocks): step past it.
            let Some(np) = self.now(t) else {
                let minute = (t / 60_000 + 1) * 60_000;
                t = next_cut(t).map_or(minute, |c| c.min(minute));
                continue;
            };
            let start = t - np.offset_ms;
            let natural = start + np.duration_ms.max(1000);
            let end = next_cut(t).filter(|c| *c < natural).unwrap_or(natural);
            match out.last_mut() {
                // The same playing carried on across a cut that changed nothing.
                Some(l) if l.1 == np.track && last_started == start && l.0 + l.2 == t => l.2 = end - l.0,
                _ => {
                    let begin = if out.is_empty() { start } else { t };
                    out.push((begin, np.track, end - begin));
                }
            }
            last_started = start;
            t = end;
        }
        out
    }

    /// Every moment between two at which the air may change hands, in order.
    fn cuts(&self, from: u64, until: u64) -> Vec<u64> {
        let mut c: Vec<u64> = self.listings(from, until).iter().flat_map(|l| [l.start_ms, l.end_ms]).collect();
        match &self.programme {
            Some(p) => {
                let (lf, lu) = (p.zone.local_min(from), p.zone.local_min(until));
                let mut w = Zone::week_origin(lf);
                while w <= lu {
                    c.push(p.zone.utc_ms(w));
                    if let Mode::Weekly { day, start_min } = p.mode {
                        c.push(p.zone.utc_ms(w + day as i64 * 1440 + start_min as i64));
                    }
                    w += 7 * 1440;
                }
                if let Mode::Daily { start_min } = p.mode {
                    let mut d = lf.div_euclid(1440) * 1440 + start_min as i64;
                    while d <= lu {
                        c.push(p.zone.utc_ms(d));
                        d += 1440;
                    }
                }
            }
            None => {
                if from >= FIRST_MONDAY_MS {
                    let mut w = FIRST_MONDAY_MS + (from - FIRST_MONDAY_MS) / WEEK_MS * WEEK_MS;
                    while w <= until {
                        c.push(w);
                        w += WEEK_MS;
                    }
                }
                let mut day = from / 86_400_000 * 86_400_000;
                while day <= until && !self.rota.blocks.is_empty() {
                    for b in &self.rota.blocks {
                        c.push(day + b.start_min as u64 * 60_000);
                        c.push(day + b.end_min as u64 * 60_000);
                    }
                    day += 86_400_000;
                }
            }
        }
        c.retain(|x| *x > from && *x < until);
        c.sort_unstable();
        c.dedup();
        c
    }

    /// What is on at `t`, in Unix milliseconds. None is dead air: an empty library, or the
    /// station off the air.
    pub fn now(&self, t: u64) -> Option<NowPlaying> {
        if t < FIRST_MONDAY_MS {
            return None;
        }
        if let Some(s) = self.show_at(t) {
            let mut src = Source::Once { order: s.tracks.clone(), at: 0, as_of: s.as_of };
            if let Some(mut np) = self.walk(s.at_ms, &mut src, t, None) {
                np.slot = Some(s.title.clone());
                return Some(np);
            }
            // Past its recordings: the live part, or nothing left. The programme carries on.
        }
        match &self.programme {
            None => self.rota_now(t),
            Some(p) => self.programme_now(p, t),
        }
    }

    fn show_len(&self, s: &Show) -> u64 {
        s.tracks.iter().filter(|&&i| self.tracks[i].on_air(s.as_of)).map(|&i| self.tracks[i].duration_ms).sum::<u64>().saturating_add(s.live_ms)
    }

    /// The show on the air at `t`: the latest to have started, if it has not ended.
    fn show_at(&self, t: u64) -> Option<&Show> {
        self.shows.iter().filter(|s| s.at_ms <= t && t < s.at_ms.saturating_add(self.show_len(s))).max_by_key(|s| s.at_ms)
    }

    fn ident_index(&self, ident: Option<Hash>) -> Option<usize> {
        ident.and_then(|h| self.index.get(&h).copied())
    }

    /// Plays tracks back to back from `start`, the ident on the bell, and says what is on at `t`.
    fn walk(&self, start: u64, src: &mut Source, t: u64, ident: Option<(usize, u64)>) -> Option<NowPlaying> {
        // A run that begins after `t` has nothing on at `t`.
        if start > t {
            return None;
        }
        let mut cursor = start;
        let mut last_ident: Option<u64> = None;
        for _ in 0..MAX_STEPS {
            self.steps.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            // At the start of a round, with no bell to keep, whole rounds are passed over at once.
            if ident.is_none() {
                match src {
                    Source::Cycle { order, at } if *at == 0 || *at >= order.len() => {
                        if let Some((k, d)) = self.whole_rounds(order, cursor, t) {
                            cursor += k * d;
                            *at = 0;
                        }
                    }
                    Source::Rounds { pool, round, order, at, .. } if *at >= order.len() => {
                        if let Some((k, d)) = self.whole_rounds(pool, cursor, t) {
                            cursor += k * d;
                            *round += k;
                        }
                    }
                    _ => {}
                }
            }
            let pick = match ident {
                Some((i, bell)) if self.tracks[i].on_air(cursor) && last_ident.is_none_or(|l| cursor - l >= bell) => {
                    last_ident = Some(cursor);
                    Some(i)
                }
                _ => src.next(&self.tracks, cursor),
            };
            let Some(i) = pick else {
                // Nothing on air yet: look again a minute on.
                cursor = (cursor / 60_000 + 1) * 60_000;
                if cursor > t {
                    return None;
                }
                if matches!(src, Source::Once { .. }) {
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
                    slot: None,
                });
            }
            cursor += tr.duration_ms;
        }
        None
    }

    fn library_order(&self) -> Vec<usize> {
        (0..self.tracks.len()).collect()
    }

    fn programme_now(&self, p: &Programme, t: u64) -> Option<NowPlaying> {
        let lm = p.zone.local_min(t);
        let ident = self.ident_index(p.ident).map(|i| (i, p.ident_every_min.max(1) as u64 * 60_000));
        let not_ident = |i: &usize| Some(self.tracks[*i].hash) != p.ident;
        for (n, slot) in p.slots.iter().enumerate() {
            let Some((start, run)) = p.zone.run_at(&slot.span, t) else { continue };
            let seed = |tag: &[u8]| -> Vec<u8> {
                let mut s = self.station.to_vec();
                s.extend_from_slice(tag);
                s.extend_from_slice(&(n as u64).to_le_bytes());
                s.extend_from_slice(&run.to_le_bytes());
                s
            };
            let mut src = match &slot.fill {
                Fill::Library => Source::Rounds { pool: self.library_order().into_iter().filter(not_ident).collect(), shuffle: Some(seed(b"slot")), round: 0, order: vec![], at: 0 },
                Fill::Tracks { tracks, shuffle } => {
                    let order: Vec<usize> = tracks.iter().filter_map(|h| self.index.get(h).copied()).collect();
                    if *shuffle {
                        Source::Rounds { pool: order, shuffle: Some(seed(b"slot")), round: 0, order: vec![], at: 0 }
                    } else {
                        Source::Cycle { order, at: 0 }
                    }
                }
                Fill::Hand { key } => Source::Rounds {
                    pool: self.library_order().into_iter().filter(|&i| &self.tracks[i].by == key).filter(not_ident).collect(),
                    shuffle: Some(seed(b"watch")),
                    round: 0,
                    order: vec![],
                    at: 0,
                },
                Fill::Newest { count } => {
                    let mut order: Vec<usize> = self.library_order().into_iter().filter(|&i| self.tracks[i].on_air(start)).filter(not_ident).collect();
                    order.reverse();
                    order.truncate((*count).max(1) as usize);
                    Source::Cycle { order, at: 0 }
                }
            };
            if let Some(mut np) = self.walk(start, &mut src, t, ident) {
                np.slot = Some(slot.title.clone());
                return Some(np);
            }
            // A slot with nothing to play falls back to the station's own mode.
            break;
        }
        if !p.on_air_at(t) {
            return None;
        }
        // Each tape's anchor, chosen in UTC: the latest of its local starts to have come.
        let week_origin = p.zone.anchor_at(Zone::week_origin(lm), 7 * 1440, t);
        let lib: Vec<usize> = self.library_order().into_iter().filter(not_ident).collect();
        let (anchor_lm, mut src) = match &p.mode {
            Mode::Shuffle => {
                let mut seed = self.station.to_vec();
                seed.extend_from_slice(b"mode");
                seed.extend_from_slice(&week_origin.to_le_bytes());
                (week_origin, Source::Rounds { pool: lib, shuffle: Some(seed), round: 0, order: vec![], at: 0 })
            }
            Mode::InOrder => (week_origin, Source::Cycle { order: lib, at: 0 }),
            Mode::NewestFirst => {
                let mut order = lib;
                order.reverse();
                (week_origin, Source::Cycle { order, at: 0 })
            }
            Mode::Daily { start_min } => {
                let s = *start_min as i64;
                let day_start = (lm - s).div_euclid(1440) * 1440 + s;
                (p.zone.anchor_at(day_start, 1440, t), Source::Cycle { order: lib, at: 0 })
            }
            Mode::Weekly { day, start_min } => {
                let s = Zone::week_origin(lm) + (*day as i64 % 7) * 1440 + *start_min as i64;
                (p.zone.anchor_at(s, 7 * 1440, t), Source::Cycle { order: lib, at: 0 })
            }
        };
        let start = p.zone.utc_ms(anchor_lm);
        // With set hours, a continuous mode's clock runs only while the station is on the air,
        // so a library in order is heard in order across the days and nothing plays unheard
        // in the hours it is off.
        if !p.hours.is_empty() && matches!(p.mode, Mode::Shuffle | Mode::InOrder | Mode::NewestFirst) {
            let heard = self.on_air_ms(p, start, t);
            let mut np = self.walk(start, &mut src, start + heard, ident)?;
            np.started_at_ms = t.saturating_sub(np.offset_ms);
            return Some(np);
        }
        self.walk(start, &mut src, t, ident)
    }

    /// How long the station's set hours have been on the air between two moments.
    fn on_air_ms(&self, p: &Programme, from: u64, to: u64) -> u64 {
        let (lf, lt) = (p.zone.local_min(from), p.zone.local_min(to));
        let mut runs: Vec<(u64, u64)> = Vec::new();
        for h in &p.hours {
            for s in h.starts_between(lf - h.len_min as i64 - 2 * 1440, lt + 1) {
                let u = p.zone.utc_ms(s);
                let (a, b) = (u.max(from), (u + h.len_min as u64 * 60_000).min(to));
                if a < b {
                    runs.push((a, b));
                }
            }
        }
        runs.sort_unstable();
        let (mut total, mut reach) = (0u64, from);
        for (a, b) in runs {
            let a = a.max(reach);
            if b > a {
                total += b - a;
                reach = b;
            }
        }
        total
    }

    /// The schedule between two moments, for a station's page: shows, slots and on-air hours.
    pub fn listings(&self, from: u64, until: u64) -> Vec<Listing> {
        let mut out = Vec::new();
        for s in &self.shows {
            // A show with nothing it can play and no live part is not promised on the page.
            let len = self.show_len(s);
            let end = s.at_ms.saturating_add(len);
            if len > 0 && end > from && s.at_ms < until {
                out.push(Listing {
                    start_ms: s.at_ms,
                    end_ms: end,
                    title: s.title.clone(),
                    about: s.about.clone(),
                    kind: "show",
                    live: s.live_ms > 0,
                    show: Some(s.hash),
                });
            }
        }
        if let Some(p) = &self.programme {
            let (lf, lu) = (p.zone.local_min(from), p.zone.local_min(until));
            for slot in &p.slots {
                for run in slot.span.starts_between(lf, lu) {
                    out.push(Listing {
                        start_ms: p.zone.utc_ms(run),
                        end_ms: p.zone.utc_ms(run) + slot.span.len_min as u64 * 60_000,
                        title: slot.title.clone(),
                        about: slot.about.clone(),
                        kind: "slot",
                        live: slot.live,
                        show: None,
                    });
                }
            }
            for h in &p.hours {
                for run in h.starts_between(lf, lu) {
                    out.push(Listing {
                        start_ms: p.zone.utc_ms(run),
                        end_ms: p.zone.utc_ms(run) + h.len_min as u64 * 60_000,
                        title: "On the air".into(),
                        about: String::new(),
                        kind: "hours",
                        live: false,
                        show: None,
                    });
                }
            }
        }
        out.sort_by_key(|l| (l.start_ms, l.kind));
        out
    }

    // ---- The rota, as it always was ---------------------------------------------------

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

    fn rota_now(&self, t: u64) -> Option<NowPlaying> {
        let week = (t - FIRST_MONDAY_MS) / WEEK_MS;
        let mut cursor = FIRST_MONDAY_MS + week * WEEK_MS;
        let mut decks: Vec<Option<Deck>> = (0..=self.rota.blocks.len()).map(|_| None).collect();
        let mut last_ident: Option<u64> = None;
        let ident = self.ident_index(self.rota.ident);
        let bell = self.rota.ident_every_min.max(1) as u64 * 60_000;

        let everything: Vec<usize> = (0..self.tracks.len()).collect();
        for _ in 0..MAX_STEPS {
            self.steps.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            // The whole library shuffled, no blocks and no bell: whole rounds at once.
            if ident.is_none() && self.rota.blocks.is_empty() {
                let deck = decks[0].get_or_insert_with(|| Deck { round: 0, order: Vec::new(), at: 0 });
                if deck.at >= deck.order.len() {
                    if let Some((k, d)) = self.whole_rounds(&everything, cursor, t) {
                        cursor += k * d;
                        deck.round += k;
                    }
                }
            }
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
                    slot: None,
                });
            }
            cursor += tr.duration_ms;
        }
        None
    }
}
