use pyrate_core::{
    Archive, Body, Chunk, StationKey,
    clock::{Block, Clock, Rota},
};

const MIN: u64 = 60_000;
// Wednesday 11 November 2026, 12:00 UTC.
const T: u64 = 1_794_398_400_000;

fn track(title: &str, minutes: u64) -> Body {
    let full = minutes * 10; // six-second chunks
    let chunks = (0..full).map(|i| Chunk::of(format!("{title}{i}").as_bytes(), 300)).collect();
    Body::Track { title: title.into(), by: "".into(), source: "".into(), channels: 2, bitrate: 64_000, chunks }
}

fn station(n: usize) -> (StationKey, Archive, Vec<[u8; 32]>) {
    let k = StationKey::from_seed([3; 32]);
    let mut a = Archive::new(k.id());
    let mut hashes = Vec::new();
    for i in 0..n {
        hashes.push(a.write(&k, 1_000 + i as u64, track(&format!("t{i}"), 3 + (i as u64 % 3))).unwrap().hash().unwrap());
    }
    (k, a, hashes)
}

#[test]
fn every_node_agrees() {
    let (_, a, _) = station(12);
    let one = Clock::new(&a).unwrap().now(T).unwrap();
    let two = Clock::new(&a.clone()).unwrap().now(T).unwrap();
    assert_eq!(one, two);
}

#[test]
fn time_moves_forward_through_a_track() {
    let (_, a, _) = station(12);
    let c = Clock::new(&a).unwrap();
    let now = c.now(T).unwrap();
    let later = c.now(T + 1_000).unwrap();
    if later.track == now.track {
        assert_eq!(later.started_at_ms, now.started_at_ms);
        assert_eq!(later.offset_ms, now.offset_ms + 1_000);
    }
    assert_eq!(now.chunk as u64, now.offset_ms / 6_000);
    assert_eq!(now.chunk_offset_ms, now.offset_ms % 6_000);
    // The next track starts the moment this one ends.
    let next = c.now(now.started_at_ms + now.duration_ms).unwrap();
    assert_eq!(next.offset_ms, 0);
}

#[test]
fn every_track_plays_before_any_repeats() {
    let (_, a, hashes) = station(10);
    let c = Clock::new(&a).unwrap();
    let mut seen = Vec::new();
    // Rounds start at Monday 00:00 UTC; mid-week a round is already under way.
    let mut t = T - 60 * 3_600_000;
    while seen.len() < 10 {
        let n = c.now(t).unwrap();
        assert!(!seen.contains(&n.track), "repeat before the round ended");
        seen.push(n.track);
        t = n.started_at_ms + n.duration_ms;
    }
    for h in hashes {
        assert!(seen.contains(&h));
    }
}

#[test]
fn empty_library_is_dead_air() {
    let k = StationKey::generate().unwrap();
    let a = Archive::new(k.id());
    assert!(Clock::new(&a).unwrap().now(T).is_none());
}

#[test]
fn withdrawn_tracks_stop_playing() {
    let (k, mut a, hashes) = station(3);
    for h in &hashes[..2] {
        a.write(&k, 5_000, Body::Withdraw { track: *h }).unwrap();
    }
    let c = Clock::new(&a).unwrap();
    for i in 0..50 {
        assert_eq!(c.now(T + i * 7 * MIN).unwrap().track, hashes[2]);
    }
}

#[test]
fn blocks_and_the_bell() {
    let (k, mut a, hashes) = station(8);
    let ident = a.write(&k, 6_000, track("ident", 1)).unwrap().hash().unwrap();
    a.write(&k, 7_000, Body::Rota(Rota {
        blocks: vec![Block { start_min: 12 * 60, end_min: 14 * 60, tracks: vec![hashes[0], hashes[1]], shuffle: false }],
        ident: Some(ident),
        ident_every_min: 30,
    })).unwrap();
    let c = Clock::new(&a).unwrap();

    // Inside the block (after any track carried over from before noon), only its two tracks or the ident.
    let mut t = T + 10 * MIN;
    let mut idents = 0;
    while t < T + 110 * MIN {
        let n = c.now(t).unwrap();
        if n.started_at_ms >= T {
            assert!(n.track == hashes[0] || n.track == hashes[1] || n.track == ident);
        }
        if n.track == ident {
            idents += 1;
        }
        t = n.started_at_ms + n.duration_ms;
    }
    assert!((3..=5).contains(&idents), "the bell rings about every half hour, rang {idents}");
}

#[test]
fn a_new_track_joins_the_next_round() {
    let (k, mut a, _) = station(5);
    let new = a.write(&k, T - 2 * 3_600_000, track("new", 4)).unwrap().hash().unwrap();
    let c = Clock::new(&a).unwrap();
    let mut t = T;
    let mut heard = false;
    for _ in 0..12 {
        let n = c.now(t).unwrap();
        heard |= n.track == new;
        t = n.started_at_ms + n.duration_ms;
    }
    assert!(heard);
}

#[test]
fn a_week_of_lookups_is_quick() {
    let (_, a, _) = station(200);
    let c = Clock::new(&a).unwrap();
    let start = std::time::Instant::now();
    // Sunday night: the longest walk from Monday.
    let sunday = T + 4 * 24 * 3_600_000 + 11 * 3_600_000;
    for i in 0..20 {
        c.now(sunday + i * 1_000).unwrap();
    }
    let each = start.elapsed() / 20;
    assert!(each.as_millis() < 50, "one lookup took {each:?}");
}
