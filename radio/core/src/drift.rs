//! How far this device's clock has strayed from the network's.
//!
//! Every station card carries the moment its station signed it. The gap between that moment
//! and our own clock, taken across many stations, says how far we are out. One station gets
//! one vote however many cards it sends, nothing moves until several stations agree, and a
//! correction once made eases off only when the gap has closed, so a median hovering at the
//! edge cannot make the playhead jump back and forth.
//!
//! No rights reserved.

use std::collections::VecDeque;

/// Stations whose word is kept: the latest gap from each.
const KEEP: usize = 101;
/// Stations that must have spoken before any correction. Raising a station costs a few
/// seconds of work, so a handful of them proves little: nine, and most of them must agree.
const QUORUM: usize = 9;
/// A gap smaller than this is the network's own delay, not a wrong clock.
const ON_MS: i64 = 2_500;
/// Once correcting, keep at it until the gap is this small.
const OFF_MS: i64 = 1_000;
/// A gap larger than this is not trusted at all. A phone's own clock, kept by its network, is
/// rarely out by more.
const MAX_MS: i64 = 120_000;
/// How far a correction moves toward a new target, per minute of our clock: a crowd of new
/// stations saying one wrong time can only nudge the playhead while honest ones keep arriving.
const SLEW_PER_MIN: i64 = 10_000;

#[derive(Default, Debug)]
pub struct Drift {
    gaps: VecDeque<([u8; 32], i64)>,
    current: i64,
    at: u64,
}

impl Drift {
    pub const fn new() -> Self {
        Self { gaps: VecDeque::new(), current: 0, at: 0 }
    }

    /// One station's card: `theirs` the time it signed, `ours` our clock when it arrived.
    pub fn sample(&mut self, station: [u8; 32], theirs: u64, ours: u64) {
        let gap = theirs as i64 - ours as i64;
        self.gaps.retain(|(s, _)| *s != station);
        self.gaps.push_back((station, gap));
        while self.gaps.len() > KEEP {
            self.gaps.pop_front();
        }
        let target = self.work_out();
        // The correction eases toward each new target, whichever way it lies, by time passed
        // on our own clock: a flood of cards in one moment moves it no further than one card.
        if self.at != 0 && ours > self.at {
            let step = (ours - self.at) as i64 * SLEW_PER_MIN / 60_000;
            self.current += (target - self.current).clamp(-step, step);
        }
        if ours > self.at {
            self.at = ours;
        }
    }

    fn work_out(&self) -> i64 {
        if self.gaps.len() < QUORUM {
            return 0;
        }
        let mut v: Vec<i64> = self.gaps.iter().map(|(_, g)| *g).collect();
        v.sort_unstable();
        let m = v[v.len() / 2];
        // Two thirds of the stations heard must sit within a few seconds of the middle: a
        // clutch of new stations all saying one wrong time cannot carry a split crowd.
        let near = v.iter().filter(|g| (**g - m).abs() <= ON_MS).count();
        if near * 3 < v.len() * 2 {
            // No agreement: hold whatever correction stands.
            return self.current;
        }
        let threshold = if self.current == 0 { ON_MS } else { OFF_MS };
        if m.abs() < threshold || m.abs() > MAX_MS { 0 } else { m }
    }

    /// Milliseconds to add to our clock to reach the network's.
    pub fn offset(&self) -> i64 {
        self.current
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st(n: u8) -> [u8; 32] {
        [n; 32]
    }

    #[test]
    fn one_station_cannot_move_the_clock() {
        let mut d = Drift::default();
        for i in 0..100u64 {
            d.sample(st(1), 1_000_000 + 250_000 + i, 1_000_000);
        }
        assert_eq!(d.offset(), 0, "a hundred cards from one station are one vote");
        for n in 2..=5 {
            d.sample(st(n), 1_000_000, 1_000_000);
        }
        assert_eq!(d.offset(), 0, "four honest stations outvote it");
    }

    #[test]
    fn a_wrong_clock_is_corrected_once_stations_agree_and_eases_off_cleanly() {
        let mut d = Drift::default();
        // Every station says the network is `gap` ahead at our time `ours`, and one says it
        // again ten minutes later, by when the correction has had time to arrive.
        let all = |d: &mut Drift, gap: u64, ours: u64| {
            (1..=9).for_each(|n| d.sample(st(n), ours + gap, ours));
            d.sample(st(1), ours + 600_000 + gap, ours + 600_000);
        };
        for n in 1..=8 {
            d.sample(st(n), 1_060_000, 1_000_000);
        }
        d.sample(st(1), 1_660_000, 1_600_000);
        assert_eq!(d.offset(), 0, "eight stations are not yet a quorum");
        all(&mut d, 60_000, 2_000_000);
        assert_eq!(d.offset(), 60_000, "with nine, our clock is a minute slow");
        all(&mut d, 1_500, 3_000_000);
        assert_eq!(d.offset(), 1_500, "still correcting above the lower edge");
        all(&mut d, 500, 4_000_000);
        assert_eq!(d.offset(), 0, "and off once the gap has closed");
        all(&mut d, 2_000, 5_000_000);
        assert_eq!(d.offset(), 0, "two seconds does not turn it back on");
    }

    #[test]
    fn a_crowd_of_new_stations_can_only_nudge_the_clock() {
        let mut d = Drift::default();
        for n in 1..=12 {
            d.sample(st(n), 1_000_000, 1_000_000);
        }
        for n in 13..=40 {
            d.sample(st(n), 1_060_000 + 120_000, 1_060_000);
        }
        assert!(d.offset() <= 10_000, "a sudden crowd moves the clock ten seconds in a minute, not two minutes: {}", d.offset());
        for i in 0..1000 {
            d.sample(st(40), 1_060_000 + 120_000 + i, 1_060_000);
        }
        assert!(d.offset() <= 10_000, "and a flood of cards in one moment moves it no further");
    }

    #[test]
    fn a_clutch_of_new_stations_cannot_carry_a_split_crowd() {
        let mut d = Drift::default();
        for n in 1..=6 {
            d.sample(st(n), 1_000_000, 1_000_000);
        }
        for n in 7..=12 {
            d.sample(st(n), 1_100_000, 1_000_000);
        }
        assert_eq!(d.offset(), 0, "six honest and six hostile: no agreement, no move");
        d.sample(st(13), 1_400_000, 1_000_000);
        assert_eq!(d.offset(), 0);
    }
}
