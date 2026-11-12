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
