//! The framed log format, loudness, the watch bill and the tape.
use pyrate_core::{Archive, Body, SignedEntry, StationKey, archive::legacy, audio::Profile, clock::Clock, ingest::ingest, tape::ogg_opus};
use serde::Serialize;

fn wav(path: &std::path::Path, secs_silence: f32, secs_tone: f32, amp: f32) {
    let rate = 48_000u32;
    let n_sil = (secs_silence * rate as f32) as usize;
    let n_tone = (secs_tone * rate as f32) as usize;
    let mut data = Vec::new();
    for i in 0..n_sil + n_tone {
        let v = if i < n_sil { 0.0 } else { (i as f32 * 440.0 * std::f32::consts::TAU / rate as f32).sin() * amp };
        data.extend(((v * 32767.0) as i16).to_le_bytes());
    }
    let mut f = b"RIFF".to_vec();
    f.extend((36 + data.len() as u32).to_le_bytes());
    f.extend(b"WAVEfmt ");
    f.extend(16u32.to_le_bytes());
    f.extend(1u16.to_le_bytes());
    f.extend(1u16.to_le_bytes());
    f.extend(rate.to_le_bytes());
    f.extend((rate * 2).to_le_bytes());
    f.extend(2u16.to_le_bytes());
    f.extend(16u16.to_le_bytes());
    f.extend(b"data");
    f.extend((data.len() as u32).to_le_bytes());
    f.extend(data);
    std::fs::write(path, f).unwrap();
}

#[test]
fn an_unknown_entry_is_kept_exactly_and_still_verifies() {
    let k = StationKey::generate().unwrap();
    let mut a = Archive::new(k.id());
    a.write(&k, 1_000, Body::Colours { name: "Test".into(), about: "".into(), port: None, tip: None }).unwrap();
    // A variant from some later version: index 200, with a payload this build cannot read.
    let future = Body::Unknown(vec![200, 1, 3, 3, 7]);
    a.write(&k, 2_000, future).unwrap();
    let e = a.entries().last().unwrap().clone();
    let bytes = e.to_bytes().unwrap();
    let back = SignedEntry::from_bytes(&bytes).unwrap();
    assert_eq!(back.entry.body, Body::Unknown(vec![200, 1, 3, 3, 7]));
    assert_eq!(back.to_bytes().unwrap(), bytes, "passed on byte for byte");
    back.verify().unwrap();
    let mut b = Archive::new(k.id());
    for e in a.entries() {
        b.append(SignedEntry::from_bytes(&e.to_bytes().unwrap()).unwrap()).unwrap();
    }
    assert_eq!(b.len(), 2);
    assert!(!Body::Unknown(vec![]).master_only());
}

#[test]
fn details_are_the_stations_own_word() {
    let k = StationKey::generate().unwrap();
    let mut a = Archive::new(k.id());
    a.write(&k, 1, Body::Details { kinds: vec!["talk".into()], mature: false }).unwrap();
    a.write(&k, 2, Body::Details { kinds: vec!["music".into()], mature: true }).unwrap();
    assert_eq!(a.details(), Some((vec!["music".to_string()], true)));
}

#[test]
fn old_logs_yield_their_colours() {
    #[derive(Serialize)]
    struct OldEntry {
        station: pyrate_core::StationId,
        seq: u64,
        prev: [u8; 32],
        at_ms: u64,
        body: Body,
    }
    #[derive(Serialize)]
    struct OldSigned {
        entry: OldEntry,
        sig: (u64, u64, u64, u64, u64, u64, u64, u64),
    }
    let k = StationKey::generate().unwrap();
    let rec = OldSigned {
        entry: OldEntry { station: k.id(), seq: 0, prev: [0; 32], at_ms: 5, body: Body::Colours { name: "Old Port".into(), about: "a".into(), port: None, tip: None } },
        sig: (0, 0, 0, 0, 0, 0, 0, 0),
    };
    let b = postcard::to_stdvec(&rec).unwrap();
    let mut file = (b.len() as u32).to_be_bytes().to_vec();
    file.extend(b);
    let (c, art) = legacy::colours_and_art(&file);
    assert!(matches!(c, Some(Body::Colours { name, .. }) if name == "Old Port"));
    assert!(art.is_none());
}

#[test]
fn quiet_recordings_come_up_and_dead_air_goes() {
    let dir = std::env::temp_dir().join(format!("pyrate-level-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("quiet.wav");
    wav(&p, 2.0, 4.0, 0.05);
    let got = ingest(&p, Profile::Talk).unwrap();
    assert!(got.duration_ms < 4_600, "two seconds of silence trimmed, got {} ms", got.duration_ms);
    let mut d = pyrate_core::audio::ChunkDecoder::new(1).unwrap();
    let pcm: Vec<i16> = got.chunks.iter().flat_map(|(_, b)| d.decode(b).unwrap()).collect();
    let peak = pcm.iter().map(|s| s.unsigned_abs()).max().unwrap() as f32 / 32767.0;
    assert!(peak > 0.15 && peak < 0.95, "a 0.05 sine is raised toward the house level, peak {peak}");
}

#[test]
fn the_watch_bill_runs_track_after_track() {
    let k = StationKey::generate().unwrap();
    let mut a = Archive::new(k.id());
    a.write(&k, 1, Body::Colours { name: "W".into(), about: "".into(), port: None, tip: None }).unwrap();
    for i in 0..3 {
        let chunk = pyrate_core::Chunk { hash: [i as u8; 32], len: 10, frames: 300 };
        a.write(&k, 2 + i, Body::Track { title: format!("t{i}"), by: "".into(), source: "".into(), channels: 1, bitrate: 24_000, chunks: vec![chunk; 10] }).unwrap();
    }
    let c = Clock::new(&a).unwrap();
    let t0 = 1_800_000_000_000u64;
    let bill = c.upcoming(t0, t0 + 3_600_000);
    assert!(bill.len() >= 50, "an hour of one-minute tracks");
    for w in bill.windows(2) {
        assert_eq!(w[0].0 + w[0].2, w[1].0, "each track starts as the last one ends");
    }
}

#[test]
fn a_tape_is_a_real_ogg_opus_file() {
    let mut enc = pyrate_core::ChunkEncoder::new(Profile::Talk).unwrap();
    let tone: Vec<i16> = (0..48_000 * 7).map(|i| ((i as f32 * 0.05).sin() * 8000.0) as i16).collect();
    let mut chunks: Vec<Vec<u8>> = enc.push(&tone).unwrap().into_iter().map(|(_, b)| b).collect();
    if let Some((_, b)) = enc.finish().unwrap() {
        chunks.push(b);
    }
    let ogg = ogg_opus(&chunks, 1, "Tone", "Test Station").unwrap();
    assert!(ogg_opus(&chunks, 3, "Tone", "Test Station").is_err(), "three channels is no tape");
    assert!(ogg_opus(&[], 1, "Tone", "Test Station").is_err(), "nothing is no tape");
    assert!(ogg_opus(&[vec![0, 9, 1]], 1, "Tone", "Test Station").is_err(), "a broken chunk is said, not dropped");
    assert!(ogg.starts_with(b"OggS"));
    let p = std::env::temp_dir().join(format!("pyrate-tape-{}.ogg", std::process::id()));
    std::fs::write(&p, &ogg).unwrap();
    if let Ok(out) = std::process::Command::new("ffprobe").args(["-v", "error", "-show_entries", "format=duration:stream=codec_name", "-of", "csv=p=0"]).arg(&p).output() {
        let s = String::from_utf8_lossy(&out.stdout);
        assert!(s.contains("opus"), "ffprobe reads it as Opus: {s}");
    }
}

#[test]
fn the_watch_bill_steps_over_quiet_minutes() {
    let k = StationKey::generate().unwrap();
    let mut a = Archive::new(k.id());
    let t0 = 1_800_000_030_000u64;
    a.write(&k, t0, Body::Colours { name: "Q".into(), about: "".into(), port: None, tip: None }).unwrap();
    let chunk = pyrate_core::Chunk { hash: [7; 32], len: 10, frames: 300 };
    a.write(&k, t0 + 1, Body::Track { title: "late".into(), by: "".into(), source: "".into(), channels: 1, bitrate: 24_000, chunks: vec![chunk; 10] }).unwrap();
    let bill = Clock::new(&a).unwrap().upcoming(t0 + 2, t0 + 600_000);
    assert!(!bill.is_empty(), "a track added mid-minute is on the bill from the next minute");
}

/// A 5.1 WAV (WAVE_FORMAT_EXTENSIBLE, mask 0x3F) with sound only in the centre channel, the
/// way a film keeps its dialogue.
fn surround_wav(path: &std::path::Path, centre: bool) {
    let secs = 3u32;
    let frames = 48_000 * secs;
    let mut data = Vec::with_capacity((frames * 12) as usize);
    for i in 0..frames {
        for ch in 0..6 {
            let v = if centre && ch == 2 { ((i as f32 * 0.06).sin() * 12_000.0) as i16 } else { 0 };
            data.extend_from_slice(&v.to_le_bytes());
        }
    }
    let mut w = Vec::new();
    w.extend_from_slice(b"RIFF");
    w.extend_from_slice(&(4 + 8 + 40 + 8 + data.len() as u32).to_le_bytes());
    w.extend_from_slice(b"WAVEfmt ");
    w.extend_from_slice(&40u32.to_le_bytes());
    w.extend_from_slice(&0xFFFEu16.to_le_bytes());
    w.extend_from_slice(&6u16.to_le_bytes());
    w.extend_from_slice(&48_000u32.to_le_bytes());
    w.extend_from_slice(&(48_000u32 * 12).to_le_bytes());
    w.extend_from_slice(&12u16.to_le_bytes());
    w.extend_from_slice(&16u16.to_le_bytes());
    w.extend_from_slice(&22u16.to_le_bytes());
    w.extend_from_slice(&16u16.to_le_bytes());
    w.extend_from_slice(&0x3Fu32.to_le_bytes());
    w.extend_from_slice(&[0x01, 0, 0, 0, 0, 0, 0x10, 0, 0x80, 0, 0, 0xAA, 0, 0x38, 0x9B, 0x71]);
    w.extend_from_slice(b"data");
    w.extend_from_slice(&(data.len() as u32).to_le_bytes());
    w.extend_from_slice(&data);
    std::fs::write(path, w).unwrap();
}

#[test]
fn a_films_centre_channel_survives_the_fold_and_silence_is_refused() {
    let dir = std::env::temp_dir();
    let p = dir.join(format!("pyrate-51-{}.wav", std::process::id()));
    surround_wav(&p, true);
    let got = ingest(&p, Profile::Music).expect("the dialogue comes through");
    let mut dec = pyrate_core::audio::ChunkDecoder::new(got.channels).unwrap();
    let pcm: Vec<i16> = got.chunks.iter().flat_map(|(_, b)| dec.decode(b).unwrap()).collect();
    let rms = (pcm.iter().map(|&s| (s as f64).powi(2)).sum::<f64>() / pcm.len().max(1) as f64).sqrt();
    assert!(rms > 1_000.0, "the centre is heard on both sides: rms {rms}");
    surround_wav(&p, false);
    assert!(ingest(&p, Profile::Music).is_err(), "a silent file is no recording");
    let _ = std::fs::remove_file(&p);
}
