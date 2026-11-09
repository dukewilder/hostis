//! A phone set a minute wrong would hear a minute off. Every fresh station card carries
//! the time its station signed it; the median gap between those times and ours is how far
//! our own clock has strayed. Corrections under two seconds are ignored, over five minutes
//! distrusted.

use std::sync::Mutex;

static SAMPLES: Mutex<Vec<i64>> = Mutex::new(Vec::new());

pub fn sample(their_ms: u64, our_ms: u64) {
    let mut s = SAMPLES.lock().unwrap();
    s.push(their_ms as i64 - our_ms as i64);
    if s.len() > 101 {
        s.remove(0);
    }
}

pub fn offset_ms() -> i64 {
    let s = SAMPLES.lock().unwrap();
    if s.len() < 7 {
        return 0;
    }
    let mut v = s.clone();
    v.sort();
    let m = v[v.len() / 2];
    if m.abs() < 2_000 || m.abs() > 300_000 { 0 } else { m }
}
