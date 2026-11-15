//! The tab's rules that need no network, kept apart so they are tested on any machine with
//! `rustc --edition 2021 --test browser/src/pure.rs -o /tmp/pure && /tmp/pure`.

pub const QUARTER_MS: u64 = 15 * 60_000;

/// When a keeper's entry other than the live air is stamped, as the node stamps it: the next
/// quarter hour after now, and after the entry before it. The stamp then says nothing of the
/// minute it was signed, and the clock reads it only as "from when".
pub fn quarter_stamp(now: u64, prev: Option<u64>) -> u64 {
    ((now / QUARTER_MS + 1) * QUARTER_MS).max(prev.map(|p| p + 1).unwrap_or(0))
}

/// How many of a log's first entries may leave the tab at `now`. A keeper's entry is stamped at
/// the next quarter hour and waits in the tab until then, so the stamp matches when it leaves.
/// Everything up to the last entry whose stamp has come, or that is the live air (which goes at
/// once, and takes the entries before it, as the chain needs them), may go. `entry(i)` gives
/// the stamp and whether the entry is live.
pub fn release_cut(len: usize, entry: impl Fn(usize) -> (u64, bool), now: u64) -> usize {
    (0..len).rev().find(|&i| {
        let (at, live) = entry(i);
        live || at <= now
    }).map(|i| i + 1).unwrap_or(0)
}

/// Which chunk of a track plays `offset` ms into it, given each chunk's length: its index and
/// where it starts within the track. None past the end.
pub fn chunk_at(durations: &[u64], offset: u64) -> Option<(usize, u64)> {
    let mut start = 0;
    for (i, d) in durations.iter().enumerate() {
        if offset < start + d {
            return Some((i, start));
        }
        start += d;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_wait_for_their_stamp_and_live_air_goes_at_once() {
        let q = QUARTER_MS;
        // (stamp, live): two sent long ago, two stamped for the coming quarter.
        let log = [(q, false), (2 * q, false), (11 * q, false), (11 * q + 1, false)];
        let cut = |now| release_cut(log.len(), |i| log[i], now);
        assert_eq!(cut(10 * q + 5), 2, "the two waiting stamps stay in the tab");
        assert_eq!(cut(11 * q - 1), 2);
        assert_eq!(cut(11 * q), 3, "an entry goes the moment its stamp comes");
        assert_eq!(cut(11 * q + 1), 4);
        // A show starting inside the wait takes the entries before it along.
        let live = [(q, false), (11 * q, false), (11 * q + 1, true)];
        assert_eq!(release_cut(live.len(), |i| live[i], 10 * q), 3);
        assert_eq!(release_cut(0, |_| (0, false), 0), 0);
    }

    #[test]
    fn a_held_track_finds_its_chunk() {
        let d = [6000, 6000, 3000];
        assert_eq!(chunk_at(&d, 0), Some((0, 0)));
        assert_eq!(chunk_at(&d, 5999), Some((0, 0)));
        assert_eq!(chunk_at(&d, 6000), Some((1, 6000)));
        assert_eq!(chunk_at(&d, 14_999), Some((2, 12_000)));
        assert_eq!(chunk_at(&d, 15_000), None);
    }

    #[test]
    fn stamps_fall_on_the_next_quarter_hour_and_after_the_last() {
        let q = QUARTER_MS;
        assert_eq!(quarter_stamp(10 * q + 1, None), 11 * q);
        assert_eq!(quarter_stamp(10 * q, None), 11 * q, "a stamp on the quarter goes to the next");
        assert_eq!(quarter_stamp(10 * q + 5, Some(3 * q)), 11 * q);
        // A batch in one quarter: each after the one before, in order.
        assert_eq!(quarter_stamp(10 * q + 9, Some(11 * q)), 11 * q + 1);
        assert_eq!(quarter_stamp(10 * q + 9, Some(11 * q + 1)), 11 * q + 2);
    }
}
