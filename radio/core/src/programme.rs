//! The programme: when a station is on the air and what fills each hour, in the station's
//! own local time.
//!
//! A station keeps playing by itself for as long as anyone carries it, so the programme is
//! written once and holds for decades. Its time zone travels as a table of offsets the
//! keeper's device works out ahead, daylight saving included, and every node reads the
//! same table, so "six in the evening" stays six in the evening in the keeper's town.
//!
//! No rights reserved.

use serde::{Deserialize, Serialize};

use crate::Hash;

pub const DAY_MIN: i64 = 1440;
pub const WEEK_MIN: i64 = 7 * DAY_MIN;

/// A station's time zone, as offsets from UTC.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Zone {
    /// For people: "America/Chicago". Empty is UTC.
    pub name: String,
    /// Minutes east of UTC, each from a moment on, oldest first.
    pub offsets: Vec<(u64, i16)>,
}

impl Zone {
    pub fn utc() -> Self {
        Self::default()
    }

    pub fn offset_min(&self, t_ms: u64) -> i64 {
        let mut off = self.offsets.first().map(|o| o.1).unwrap_or(0);
        for (from, o) in &self.offsets {
            if *from <= t_ms {
                off = *o;
            } else {
                break;
            }
        }
        off as i64
    }

    /// Local minutes since 1970-01-01 00:00 in this zone.
    pub fn local_min(&self, t_ms: u64) -> i64 {
        (t_ms / 60_000) as i64 + self.offset_min(t_ms)
    }

    /// The moment a local minute begins.
    pub fn utc_ms(&self, local_min: i64) -> u64 {
        let naive = (local_min.max(0) as u64) * 60_000;
        let guess = (local_min - self.offset_min(naive)).max(0) as u64 * 60_000;
        let off = self.offset_min(guess);
        (local_min - off).max(0) as u64 * 60_000
    }

    /// The local minute at which the week holding `lm` began: Monday 00:00.
    pub fn week_origin(lm: i64) -> i64 {
        // 1970-01-01 was a Thursday, day 3 of a Monday week.
        let dow = (lm.div_euclid(DAY_MIN) + 3).rem_euclid(7);
        lm - dow * DAY_MIN - lm.rem_euclid(DAY_MIN)
    }
}

/// A stretch of the week that repeats: days, a local starting time and a length.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    /// Bit 0 is Monday, bit 6 is Sunday. 0x7f is every day.
    pub days: u8,
    /// Local minute of the day it starts.
    pub start_min: u16,
    /// Minutes it runs. It may run past midnight.
    pub len_min: u16,
}

impl Span {
    pub fn every_day(start_min: u16, len_min: u16) -> Self {
        Self { days: 0x7f, start_min, len_min }
    }

    /// If a run covers local minute `lm`, the local minute that run began.
    pub fn run_start(&self, lm: i64) -> Option<i64> {
        if self.len_min == 0 {
            return None;
        }
        let origin = Zone::week_origin(lm);
        let mut best: Option<i64> = None;
        for back in [0i64, 1] {
            for d in 0..7i64 {
                if self.days & (1 << d) == 0 {
                    continue;
                }
                let s = origin - back * WEEK_MIN + d * DAY_MIN + self.start_min as i64;
                if s <= lm && lm < s + self.len_min as i64 && best.is_none_or(|b| s > b) {
                    best = Some(s);
                }
            }
        }
        best
    }

    /// Runs that begin in [from, until), as local minutes.
    pub fn starts_between(&self, from: i64, until: i64) -> Vec<i64> {
        let mut out = Vec::new();
        let mut origin = Zone::week_origin(from) - WEEK_MIN;
        while origin < until {
            for d in 0..7i64 {
                if self.days & (1 << d) == 0 {
                    continue;
                }
                let s = origin + d * DAY_MIN + self.start_min as i64;
                if s + self.len_min as i64 > from && s < until {
                    out.push(s);
                }
            }
            origin += WEEK_MIN;
        }
        out.sort();
        out.dedup();
        out
    }
}

/// How the library fills the hours no slot has taken.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mode {
    /// Every track once a round, in a dealt order, before any plays twice.
    Shuffle,
    /// The library in the order it was added, round and round, from the top each Monday.
    InOrder,
    /// Newest first, round and round, from the top each Monday. For podcasts.
    NewestFirst,
    /// In order, from the top every day at this local minute. A day's tape.
    Daily { start_min: u16 },
    /// In order, from the top every week at this local day (0 Monday) and minute.
    Weekly { day: u8, start_min: u16 },
}

/// What plays in a slot.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Fill {
    /// The whole library, shuffled for each run.
    Library,
    /// Chosen tracks, in this order or shuffled.
    Tracks { tracks: Vec<Hash>, shuffle: bool },
    /// What one hand added: a watch kept by that hand.
    Hand { key: [u8; 32] },
    /// The newest so many tracks, newest first.
    Newest { count: u16 },
}

/// A regular slot: a show, a watch, or the hour the keeper goes live.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Slot {
    pub title: String,
    pub about: String,
    pub span: Span,
    pub fill: Fill,
    /// The keeper means to be live in this slot. When they are not, the fill plays.
    pub live: bool,
}

/// When the station is on the air and what fills each hour. The latest one rules.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Programme {
    pub zone: Zone,
    pub mode: Mode,
    /// When the station is on the air. Empty: always. Slots are on the air whatever this says.
    pub hours: Vec<Span>,
    /// Checked in order; the first covering a moment has it.
    pub slots: Vec<Slot>,
    /// A recording played on the bell.
    pub ident: Option<Hash>,
    pub ident_every_min: u16,
}

impl Default for Programme {
    fn default() -> Self {
        Self { zone: Zone::utc(), mode: Mode::Shuffle, hours: Vec::new(), slots: Vec::new(), ident: None, ident_every_min: 30 }
    }
}

impl Programme {
    /// Whether the station is on the air at local minute `lm`, by its hours or a slot.
    pub fn on_air(&self, lm: i64) -> bool {
        self.hours.is_empty() || self.hours.iter().any(|h| h.run_start(lm).is_some()) || self.slots.iter().any(|s| s.span.run_start(lm).is_some())
    }
}
