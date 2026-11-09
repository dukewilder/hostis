//! Call signs. Six letters and four hex digits drawn from the key, e.g. `KAROMI·52E1`.
//! Nobody chooses one, so nobody can squat one, and a forged one is a different key.

use crate::StationId;

const CONSONANTS: &[u8; 16] = b"BDFGHJKLMNPRSTVZ";
const VOWELS: &[u8; 5] = b"AEIOU";

pub fn call_sign(id: &StationId) -> String {
    let mut h = blake3::Hasher::new();
    h.update(b"pyrate/callsign/v1");
    h.update(&id.0);
    let h = h.finalize();
    let b = h.as_bytes();
    let mut s = String::with_capacity(12);
    for i in 0..3 {
        s.push(CONSONANTS[(b[i] & 0x0f) as usize] as char);
        s.push(VOWELS[(b[i] >> 4) as usize % 5] as char);
    }
    s.push('·');
    s.push_str(&format!("{:02X}{:02X}", b[3], b[4]));
    s
}
