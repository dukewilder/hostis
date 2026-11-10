//! Tape it: the last minutes of what is playing, as a plain Ogg Opus file any player opens.
//! The chunks are written as they came, with no second encode.
//!
//! No rights reserved.

use crate::audio::{one_frame, packets};

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0u32;
    for &b in data {
        crc ^= (b as u32) << 24;
        for _ in 0..8 {
            crc = if crc & 0x8000_0000 != 0 { (crc << 1) ^ 0x04c1_1db7 } else { crc << 1 };
        }
    }
    crc
}

struct Pages {
    out: Vec<u8>,
    serial: u32,
    seq: u32,
}

impl Pages {
    fn page(&mut self, packets: &[&[u8]], granule: u64, flags: u8) {
        let mut lacing = Vec::new();
        for p in packets {
            let mut n = p.len();
            while n >= 255 {
                lacing.push(255u8);
                n -= 255;
            }
            lacing.push(n as u8);
        }
        let mut page = Vec::with_capacity(27 + lacing.len() + packets.iter().map(|p| p.len()).sum::<usize>());
        page.extend_from_slice(b"OggS");
        page.push(0);
        page.push(flags);
        page.extend_from_slice(&granule.to_le_bytes());
        page.extend_from_slice(&self.serial.to_le_bytes());
        page.extend_from_slice(&self.seq.to_le_bytes());
        page.extend_from_slice(&[0, 0, 0, 0]);
        page.push(lacing.len() as u8);
        page.extend_from_slice(&lacing);
        for p in packets {
            page.extend_from_slice(p);
        }
        let c = crc32(&page);
        page[22..26].copy_from_slice(&c.to_le_bytes());
        self.out.extend_from_slice(&page);
        self.seq += 1;
    }
}

fn tags(title: &str, artist: &str) -> Vec<u8> {
    let vendor = b"Pyrate Radio";
    let comments = [format!("TITLE={title}"), format!("ARTIST={artist}"), "COMMENT=Taped off Pyrate Radio".to_string()];
    let mut t = b"OpusTags".to_vec();
    t.extend_from_slice(&(vendor.len() as u32).to_le_bytes());
    t.extend_from_slice(vendor);
    t.extend_from_slice(&(comments.len() as u32).to_le_bytes());
    for c in &comments {
        t.extend_from_slice(&(c.len() as u32).to_le_bytes());
        t.extend_from_slice(c.as_bytes());
    }
    t
}

/// Writes chunks (as stored on the network) into an Ogg Opus file. A chunk that will not
/// read, a packet that is not one 20 ms frame, one or two channels anything else: each is an
/// error said plainly, never a file that will not play.
pub fn ogg_opus(chunks: &[Vec<u8>], channels: u8, title: &str, artist: &str) -> Result<Vec<u8>, &'static str> {
    if !(1..=2).contains(&channels) {
        return Err("a tape is mono or stereo");
    }
    let mut all: Vec<&[u8]> = Vec::new();
    for c in chunks {
        let ps = packets(c).map_err(|_| "a chunk of the tape would not read")?;
        if ps.iter().any(|p| !one_frame(p)) {
            return Err("a chunk of the tape is not plain Opus frames");
        }
        all.extend(ps);
    }
    if all.is_empty() {
        return Err("nothing to tape yet");
    }
    let mut p = Pages { out: Vec::new(), serial: 0x5079_7261, seq: 0 };
    let mut head = b"OpusHead".to_vec();
    head.push(1);
    head.push(channels);
    head.extend_from_slice(&312u16.to_le_bytes());
    head.extend_from_slice(&48_000u32.to_le_bytes());
    head.extend_from_slice(&0i16.to_le_bytes());
    head.push(0);
    p.page(&[&head], 0, 0x02);
    let t = tags(title, artist);
    p.page(&[&t], 0, 0);
    // A page holds at most 255 lacing segments: packets go in until the next would overflow.
    let segs = |q: &[u8]| q.len() / 255 + 1;
    let mut granule = 312u64;
    let mut at = 0;
    while at < all.len() {
        let mut end = at;
        let mut used = 0;
        while end < all.len() && end - at < 50 && used + segs(all[end]) <= 255 {
            used += segs(all[end]);
            end += 1;
        }
        granule += 960 * (end - at) as u64;
        p.page(&all[at..end], granule, if end == all.len() { 0x04 } else { 0 });
        at = end;
    }
    Ok(p.out)
}
