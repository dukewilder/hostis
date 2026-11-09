//! `ingest <file> [music|talk] [out.wav]`
//! Runs a file through the whole chain (decode, resample, Opus, sign into a station's
//! log, verify, decode back) and reports what came out. Writes the result as WAV if asked.

use std::{path::PathBuf, time::Instant};

use pyrate_core::{Archive, Body, Profile, StationKey, audio::ChunkDecoder, ingest::ingest};

fn main() -> pyrate_core::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let path = PathBuf::from(args.get(1).expect("usage: ingest <file> [music|talk] [out.wav]"));
    let profile = match args.get(2).map(String::as_str) {
        Some("talk") => Profile::Talk,
        _ => Profile::Music,
    };
    let t = Instant::now();
    let got = ingest(&path, profile)?;
    let secs = t.elapsed().as_secs_f64();
    let bytes: u64 = got.chunks.iter().map(|(c, _)| c.len as u64).sum();
    let in_bytes = std::fs::metadata(&path)?.len();

    let key = StationKey::generate()?;
    let mut log = Archive::new(key.id());
    log.write(&key, 1, Body::Colours { name: "Test".into(), about: "".into(), port: None, tip: None })?;
    let title = path.file_stem().unwrap().to_string_lossy().to_string();
    log.write(&key, 2, Body::Track {
        title: title.clone(), by: "".into(), source: "test".into(),
        channels: got.channels, bitrate: got.bitrate,
        chunks: got.chunks.iter().map(|(c, _)| *c).collect(),
    })?;
    let mut check = Archive::new(key.id());
    for e in log.entries() {
        check.append(e.clone())?;
    }

    let mut dec = ChunkDecoder::new(got.channels)?;
    let mut pcm = Vec::new();
    for (c, b) in &got.chunks {
        c.check(b)?;
        pcm.extend(dec.decode(b)?);
    }

    println!("station      {}", key.id().call_sign());
    println!("track        {title}");
    println!("duration     {:.1} s", got.duration_ms as f64 / 1000.0);
    println!("chunks       {}", got.chunks.len());
    println!("size         {:.2} MB in, {:.2} MB out ({:.0} kbit/s)", in_bytes as f64 / 1e6, bytes as f64 / 1e6,
        bytes as f64 * 8.0 / (got.duration_ms as f64 / 1000.0) / 1000.0);
    println!("speed        {:.0}x real time", got.duration_ms as f64 / 1000.0 / secs);
    println!("log          {} entries, chain verified", check.len());

    if let Some(out) = args.get(3) {
        let ch = got.channels as u16;
        let data_len = (pcm.len() * 2) as u32;
        let mut b = Vec::with_capacity(44 + data_len as usize);
        b.extend(b"RIFF"); b.extend((36 + data_len).to_le_bytes()); b.extend(b"WAVEfmt ");
        b.extend(16u32.to_le_bytes()); b.extend(1u16.to_le_bytes()); b.extend(ch.to_le_bytes());
        b.extend(48_000u32.to_le_bytes()); b.extend((48_000 * ch as u32 * 2).to_le_bytes());
        b.extend((ch * 2).to_le_bytes()); b.extend(16u16.to_le_bytes());
        b.extend(b"data"); b.extend(data_len.to_le_bytes());
        for s in &pcm { b.extend(s.to_le_bytes()); }
        std::fs::write(out, b)?;
        println!("wrote        {out}");
    }
    Ok(())
}
