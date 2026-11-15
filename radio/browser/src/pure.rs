//! The tab's rules that need no network, kept apart so they are tested on any machine with
//! `rustc --edition 2021 --test browser/src/pure.rs -o /tmp/pure && /tmp/pure`.

pub const QUARTER_MS: u64 = 15 * 60_000;

/// When a keeper's entry other than the live air is stamped, as the node stamps it: the next
/// quarter hour after now, and after the entry before it. The stamp then says nothing of the
/// minute it was signed, and the clock reads it only as "from when".
pub fn quarter_stamp(now: u64, prev: Option<u64>) -> u64 {
    ((now / QUARTER_MS + 1) * QUARTER_MS).max(prev.map(|p| p + 1).unwrap_or(0))
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
