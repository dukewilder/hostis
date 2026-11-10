//! A phone set a minute wrong would hear a minute off. Every fresh station card carries the
//! time its station signed it, and the stations together say how far our own clock has
//! strayed. See pyrate_core::drift: one vote a station, nothing moves until five agree, small
//! gaps are the network's own delay and large ones are not trusted.

use std::sync::Mutex;

use pyrate_core::drift::Drift;

static DRIFT: Mutex<Drift> = Mutex::new(Drift::new());

pub fn sample(station: [u8; 32], their_ms: u64, our_ms: u64) {
    DRIFT.lock().unwrap().sample(station, their_ms, our_ms);
}

pub fn offset_ms() -> i64 {
    DRIFT.lock().unwrap().offset()
}
