//! FUZ-002, in part: random station logs, built the way any station may sign them, and the
//! clock asked about them at random moments. Nothing a log can say may make the clock panic,
//! loop without end, or take long.
use pyrate_core::{
    Archive, Body, Chunk, StationKey,
    clock::{Block, Clock, Rota},
    programme::{Fill, Mode, Programme, Slot, Span, Zone},
};

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
}

fn pick_hash(r: &mut Rng, tracks: &[[u8; 32]]) -> [u8; 32] {
    if !tracks.is_empty() && r.below(4) != 0 { tracks[r.below(tracks.len() as u64) as usize] } else { [r.next() as u8; 32] }
}

fn random_log(r: &mut Rng) -> Archive {
    let k = StationKey::generate().unwrap();
    let mut a = Archive::new(k.id());
    let mut t = 1_700_000_000_000u64 + r.below(1_000_000_000);
    let mut tracks: Vec<[u8; 32]> = vec![];
    for _ in 0..r.below(40) {
        t += r.below(10_000_000);
        let body = match r.below(9) {
            0..=2 => {
                let chunks = (0..r.below(30)).map(|i| Chunk { hash: [i as u8; 32], len: 1 + r.below(256 * 1024) as u32, frames: 1 + r.below(300) as u16 }).collect();
                Body::Track { title: "t".into(), by: ["a", "b", ""][r.below(3) as usize].into(), source: if r.below(5) == 0 { "bell x".into() } else { "s".into() }, channels: 1 + r.below(2) as u8, bitrate: 64_000, chunks }
            }
            3 => Body::Withdraw { track: pick_hash(r, &tracks) },
            4 => Body::Rota(Rota {
                blocks: (0..r.below(5)).map(|_| Block { start_min: r.below(1500) as u16, end_min: r.below(1500) as u16, tracks: (0..r.below(4)).map(|_| pick_hash(r, &tracks)).collect(), shuffle: r.below(2) == 0 }).collect(),
                ident: (r.below(2) == 0).then(|| pick_hash(r, &tracks)),
                ident_every_min: r.below(200) as u16,
            }),
            5 => {
                let mode = match r.below(5) {
                    0 => Mode::Shuffle,
                    1 => Mode::InOrder,
                    2 => Mode::NewestFirst,
                    3 => Mode::Daily { start_min: r.below(3000) as u16 },
                    _ => Mode::Weekly { day: r.below(12) as u8, start_min: r.below(3000) as u16 },
                };
                let span = |r: &mut Rng| Span { days: r.below(256) as u8, start_min: r.below(3000) as u16, len_min: r.below(3000) as u16 };
                let slots = (0..r.below(5))
                    .map(|_| {
                        let fill = match r.below(4) {
                            0 => Fill::Library,
                            1 => Fill::Tracks { tracks: (0..r.below(4)).map(|_| pick_hash(r, &tracks)).collect(), shuffle: r.below(2) == 0 },
                            2 => Fill::Hand { key: [r.next() as u8; 32] },
                            _ => Fill::Newest { count: r.below(70_000) as u16 },
                        };
                        Slot { title: "s".into(), about: String::new(), span: span(r), fill, live: r.below(3) == 0 }
                    })
                    .collect();
                let offsets = (0..r.below(3)).map(|_| (r.below(u64::MAX >> 1), (r.below(3000) as i16) - 1500)).collect();
                Body::Programme(Programme { zone: Zone { name: String::new(), offsets }, mode, hours: (0..r.below(4)).map(|_| span(r)).collect(), slots, ident: (r.below(2) == 0).then(|| pick_hash(r, &tracks)), ident_every_min: r.below(500) as u16 })
            }
            6 => Body::Show { title: "show".into(), about: String::new(), at_ms: t.saturating_sub(r.below(100_000_000)) + r.below(200_000_000), tracks: (0..r.below(5)).map(|_| pick_hash(r, &tracks)).collect(), live_min: r.below(500) as u32 },
            7 => Body::Live { chunk: Chunk { hash: [9; 32], len: 100, frames: 1 + r.below(300) as u16 }, channels: 2, bitrate: 64_000 },
            _ => Body::Unshow { show: [r.next() as u8; 32] },
        };
        let is_track = matches!(body, Body::Track { .. });
        if let Ok(e) = a.write(&k, t, body) {
            if is_track {
                tracks.push(e.hash().unwrap());
            }
        }
    }
    a
}

#[test]
fn the_clock_survives_any_log() {
    let mut worst = 0u128;
    for seed in 1..=600u64 {
        let mut r = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let a = random_log(&mut r);
        if std::env::var_os("FUZZ_TRACE").is_some() {
            eprintln!("seed {seed}");
        }
        let start = std::time::Instant::now();
        let ok = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let Ok(c) = Clock::new(&a) else { return };
            let base = a.entries().last().map_or(1_700_000_000_000, |e| e.entry.at_ms);
            for i in 0..6u64 {
                let t = base.saturating_sub(50_000_000) + r.below(200_000_000) + i;
                let _ = c.now(t);
                let _ = c.upcoming(t, t + 86_400_000);
                let _ = c.next_on(t, t + 7 * 86_400_000);
                let _ = c.listings(t, t + 7 * 86_400_000);
            }
        }));
        assert!(ok.is_ok(), "seed {seed}: the clock panicked on a log a station could sign");
        let ms = start.elapsed().as_millis();
        if ms > 300 {
            eprintln!("seed {seed}: {ms} ms");
        }
        worst = worst.max(ms);
    }
    println!("slowest log: {worst} ms");
    assert!(worst < 1_000, "no log makes the clock take a second ({worst} ms)");
}

/// FUZZ_SEED=<n>: one log, each question timed, to find what stalls.
#[test]
fn one_seed() {
    let Some(seed) = std::env::var("FUZZ_SEED").ok().and_then(|s| s.parse::<u64>().ok()) else { return };
    let mut r = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    let a = random_log(&mut r);
    for e in a.entries() {
        let b = e.entry.body();
        let kind = format!("{b:?}");
        eprintln!("{} {}", e.entry.at_ms, &kind[..kind.len().min(220)]);
    }
    let t0 = std::time::Instant::now();
    let c = Clock::new(&a).unwrap();
    eprintln!("new {} ms", t0.elapsed().as_millis());
    let base = a.entries().last().map_or(1_700_000_000_000, |e| e.entry.at_ms);
    for i in 0..6u64 {
        let t = base.saturating_sub(50_000_000) + r.below(200_000_000) + i;
        let t0 = std::time::Instant::now();
        let _ = c.now(t);
        eprintln!("now {} ms", t0.elapsed().as_millis());
        let t0 = std::time::Instant::now();
        let _ = c.upcoming(t, t + 86_400_000);
        eprintln!("upcoming {} ms", t0.elapsed().as_millis());
        let t0 = std::time::Instant::now();
        let _ = c.next_on(t, t + 7 * 86_400_000);
        eprintln!("next_on {} ms", t0.elapsed().as_millis());
        let t0 = std::time::Instant::now();
        let _ = c.listings(t, t + 7 * 86_400_000);
        eprintln!("listings {} ms", t0.elapsed().as_millis());
    }
}

/// The worst a station can sign on purpose: one-second recordings, a bell every minute, and
/// each way of running a week. What it costs a listener to ask what is on, and what is coming.
#[test]
fn the_costliest_schedules_a_station_can_sign() {
    let k = StationKey::generate().unwrap();
    let modes = [Mode::Shuffle, Mode::InOrder, Mode::NewestFirst, Mode::Daily { start_min: 0 }, Mode::Weekly { day: 0, start_min: 0 }];
    let mut worst_now = 0u128;
    let mut worst_bill = 0u128;
    let mut worst_play = 0u128;
    for (m, mode) in modes.into_iter().enumerate() {
        for bell in [false, true] {
            for n in [1usize, 3, 50, 2000] {
                let mut a = Archive::new(k.id());
                let mut t = 1_700_000_000_000u64;
                let mut hashes = vec![];
                for i in 0..n {
                    t += 1;
                    let chunks = vec![Chunk { hash: [(i % 251) as u8; 32], len: 100, frames: 50 }];
                    let e = a.write(&k, t, Body::Track { title: format!("t{i}"), by: "a".into(), source: "s".into(), channels: 2, bitrate: 64_000, chunks }).unwrap();
                    hashes.push(e.hash().unwrap());
                }
                let ident = bell.then(|| hashes[0]);
                a.write(&k, t + 1, Body::Programme(Programme { zone: Zone { name: String::new(), offsets: vec![] }, mode: mode.clone(), hours: vec![], slots: vec![], ident, ident_every_min: 1 })).unwrap();
                let c = Clock::new(&a).unwrap();
                // Late in the week, when the most has to be walked.
                let week = 7 * 86_400_000u64;
                for at in [t + 3_600_000, t + 3 * 86_400_000, t + week - 60_000, t + 30 * 86_400_000] {
                    let s = std::time::Instant::now();
                    let _ = c.now(at);
                    let now_ms = s.elapsed().as_millis();
                    let s = std::time::Instant::now();
                    let _ = c.upcoming(at, at + 6 * 3_600_000);
                    let bill_ms = s.elapsed().as_millis();
                    // A minute of a player asking four times a second.
                    let s = std::time::Instant::now();
                    for i in 0..240 {
                        let _ = c.now(at + 60_000 + i * 250);
                    }
                    let play_ms = s.elapsed().as_millis();
                    worst_play = worst_play.max(play_ms);
                    if now_ms > 20 || bill_ms > 100 || play_ms > 200 {
                        eprintln!("mode {m} bell {bell} tracks {n} at +{} h: now {now_ms} ms, bill {bill_ms} ms, a minute of play {play_ms} ms", (at - t) / 3_600_000);
                    }
                    worst_now = worst_now.max(now_ms);
                    worst_bill = worst_bill.max(bill_ms);
                }
            }
        }
    }
    println!("worst now {worst_now} ms, worst bill {worst_bill} ms, worst minute of play {worst_play} ms");
    // Measured here: now 40 ms, bill 270 ms, a minute of play 2.5 s (4% of a core), all for
    // one-second recordings and a bell every minute. Before: bills of 22 s.
    assert!(worst_now < 250 && worst_bill < 1_500 && worst_play < 5_000, "a station can sign a schedule that costs its listeners too much");
}

/// What the clock passes over by arithmetic (whole rounds, empty minutes) it would have found
/// by walking: on random logs, the quick clock and the walking one agree at every moment.
#[test]
fn passing_over_changes_nothing_on_any_log() {
    for seed in 1..=250u64 {
        let mut r = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let a = random_log(&mut r);
        let (Ok(fast), Ok(slow)) = (Clock::new(&a), Clock::new(&a)) else { continue };
        let slow = slow.stepping();
        let base = a.entries().first().map_or(1_700_000_000_000, |e| e.entry.at_ms);
        for i in 0..12u64 {
            let t = base + r.below(30 * 86_400_000) + i;
            assert_eq!(fast.now(t), slow.now(t), "seed {seed} at {t}");
        }
    }
}

/// The same for a station with no programme: its rota of daily blocks.
#[test]
fn the_costliest_rotas_a_station_can_sign() {
    let k = StationKey::generate().unwrap();
    let mut worst_now = 0u128;
    let mut worst_bill = 0u128;
    for blocks in [0usize, 1, 5] {
        for bell in [false, true] {
            for n in [3usize, 2000] {
                let mut a = Archive::new(k.id());
                let mut t = 1_700_000_000_000u64;
                let mut hashes = vec![];
                for i in 0..n {
                    t += 1;
                    let chunks = vec![Chunk { hash: [(i % 251) as u8; 32], len: 100, frames: 50 }];
                    let e = a.write(&k, t, Body::Track { title: format!("t{i}"), by: "a".into(), source: "s".into(), channels: 2, bitrate: 64_000, chunks }).unwrap();
                    hashes.push(e.hash().unwrap());
                }
                let blocks: Vec<Block> = (0..blocks).map(|b| Block { start_min: (b * 240) as u16, end_min: (b * 240 + 120) as u16, tracks: vec![], shuffle: true }).collect();
                a.write(&k, t + 1, Body::Rota(Rota { blocks, ident: bell.then(|| hashes[0]), ident_every_min: 1 })).unwrap();
                let c = Clock::new(&a).unwrap();
                for at in [t + 3_600_000, t + 3 * 86_400_000, t + 7 * 86_400_000 - 60_000] {
                    let s = std::time::Instant::now();
                    let _ = c.now(at);
                    let now_ms = s.elapsed().as_millis();
                    let s = std::time::Instant::now();
                    let _ = c.upcoming(at, at + 6 * 3_600_000);
                    let bill_ms = s.elapsed().as_millis();
                    if now_ms > 20 || bill_ms > 100 {
                        eprintln!("rota blocks {} bell {bell} tracks {n} at +{} h: now {now_ms} ms, bill {bill_ms} ms", c_blocks(&a), (at - t) / 3_600_000);
                    }
                    worst_now = worst_now.max(now_ms);
                    worst_bill = worst_bill.max(bill_ms);
                }
            }
        }
    }
    println!("rota: worst now {worst_now} ms, worst bill {worst_bill} ms");
    assert!(worst_now < 250 && worst_bill < 1_500, "a rota can cost its listeners too much");
}

fn c_blocks(a: &Archive) -> usize {
    a.rota().blocks.len()
}

/// A rota block naming many recordings the station does not have, over a large library.
#[test]
fn a_rota_block_naming_thousands_of_recordings() {
    let k = StationKey::generate().unwrap();
    let mut a = Archive::new(k.id());
    let mut t = 1_700_000_000_000u64;
    for i in 0..2000usize {
        t += 1;
        let chunks = vec![Chunk { hash: [(i % 251) as u8; 32], len: 100, frames: 50 }];
        a.write(&k, t, Body::Track { title: format!("t{i}"), by: "a".into(), source: "s".into(), channels: 2, bitrate: 64_000, chunks }).unwrap();
    }
    let wanted: Vec<[u8; 32]> = (0..20_000u32).map(|i| { let mut h = [0u8; 32]; h[..4].copy_from_slice(&i.to_le_bytes()); h }).collect();
    a.write(&k, t + 1, Body::Rota(Rota { blocks: vec![Block { start_min: 0, end_min: 1439, tracks: wanted, shuffle: true }], ident: None, ident_every_min: 0 })).unwrap();
    let c = Clock::new(&a).unwrap();
    for at in [t + 3_600_000, t + 3 * 86_400_000] {
        let s = std::time::Instant::now();
        let _ = c.now(at);
        let now_ms = s.elapsed().as_millis();
        let s = std::time::Instant::now();
        let _ = c.upcoming(at, at + 3_600_000);
        let bill_ms = s.elapsed().as_millis();
        println!("rota of 20,000 names at +{} h: now {now_ms} ms, bill {bill_ms} ms", (at - t) / 3_600_000);
        // Before the block's names were matched once: now 4.7 s, a bill 82 s.
        assert!(now_ms < 250 && bill_ms < 1_500);
    }
}

/// The same agreement for stations with no programme, whose rota is walked its own way.
#[test]
fn passing_over_changes_nothing_on_any_rota() {
    let mut tried = 0;
    for seed in 1..=3000u64 {
        let mut r = Rng(seed.wrapping_mul(0x2545_F491_4F6C_DD1D) | 1);
        let a = random_log(&mut r);
        // Only rota stations: a log with a programme is walked as a programme.
        if a.programme().is_some() {
            continue;
        }
        let (Ok(fast), Ok(slow)) = (Clock::new(&a), Clock::new(&a)) else { continue };
        tried += 1;
        let slow = slow.stepping();
        let base = a.entries().first().map_or(1_700_000_000_000, |e| e.entry.at_ms);
        for i in 0..12u64 {
            let t = base + r.below(20 * 86_400_000) + i;
            assert_eq!(fast.now(t), slow.now(t), "seed {seed} at {t}");
        }
    }
    println!("{tried} rota logs");
    assert!(tried > 100);
}

/// A player asks again and again as a recording plays: the answers it is given from what the
/// clock remembers are the answers walking would give.
#[test]
fn remembered_answers_are_the_walked_answers() {
    for seed in 1..=1500u64 {
        let mut r = Rng(seed.wrapping_mul(0xD6E8_FEB8_6659_FD93) | 1);
        let a = random_log(&mut r);
        let (Ok(fast), Ok(slow)) = (Clock::new(&a), Clock::new(&a)) else { continue };
        let slow = slow.stepping();
        let base = a.entries().first().map_or(1_700_000_000_000, |e| e.entry.at_ms);
        for _ in 0..4 {
            let mut t = base + r.below(20 * 86_400_000);
            for _ in 0..40 {
                assert_eq!(fast.now(t), slow.now(t), "seed {seed} at {t}");
                t += [250, 1_000, 7_000, 61_000, 300_000][r.below(5) as usize];
            }
        }
    }
}

/// The programme and rota in force are found by index; they are the latest written, as a
/// search of the whole log from its end finds them.
#[test]
fn the_rota_and_programme_in_force_are_the_latest_written() {
    for seed in 1..=500u64 {
        let mut r = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let a = random_log(&mut r);
        let scan_p = a.entries().iter().rev().find_map(|e| match e.entry.body() {
            Body::Programme(p) => Some(Some(p.clone())),
            Body::Rota(_) => Some(None),
            _ => None,
        }).flatten();
        let scan_r = a.entries().iter().rev().find_map(|e| match e.entry.body() {
            Body::Rota(r) => Some(r.clone()),
            _ => None,
        }).unwrap_or_default();
        assert_eq!(a.programme(), scan_p, "seed {seed}");
        assert_eq!(a.rota(), scan_r, "seed {seed}");
    }
}
