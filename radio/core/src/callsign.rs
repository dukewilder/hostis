//! Call signs. Six letters and four hex digits drawn from the key, e.g. `KAROMI·52E1`: a
//! name to say aloud, not a proof. It carries about 35 bits, so a patient forger can grind
//! out a key with the same call sign in hours; the key itself is the station, and the
//! station's page shows it. Every call sign stays as it was drawn (the vowels lean slightly
//! to A), since changing the draw would rename every station on the air.

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
