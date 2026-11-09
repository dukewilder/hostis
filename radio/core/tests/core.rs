use pyrate_core::{
    Archive, Body, ChunkEncoder, Error, Profile, SignedEntry, StationId, StationKey,
    audio::{ChunkDecoder, FRAMES_PER_CHUNK, SAMPLE_RATE},
    decode_chunk, ingest::ingest,
};

fn sine(rate: u32, secs: f32, channels: usize, hz: f32) -> Vec<i16> {
    let n = (rate as f32 * secs) as usize;
    let mut v = Vec::with_capacity(n * channels);
    for i in 0..n {
        let s = ((i as f32 / rate as f32) * hz * std::f32::consts::TAU).sin() * 0.5;
        for _ in 0..channels {
            v.push((s * i16::MAX as f32) as i16);
        }
    }
    v
}

fn rms(v: &[i16]) -> f64 {
    (v.iter().map(|s| (*s as f64).powi(2)).sum::<f64>() / v.len() as f64).sqrt()
}

fn write_wav(path: &std::path::Path, rate: u32, channels: u16, pcm: &[i16]) {
    let data_len = (pcm.len() * 2) as u32;
    let mut b = Vec::new();
    b.extend(b"RIFF");
    b.extend((36 + data_len).to_le_bytes());
    b.extend(b"WAVEfmt ");
    b.extend(16u32.to_le_bytes());
    b.extend(1u16.to_le_bytes());
    b.extend(channels.to_le_bytes());
    b.extend(rate.to_le_bytes());
    b.extend((rate * channels as u32 * 2).to_le_bytes());
    b.extend((channels * 2).to_le_bytes());
    b.extend(16u16.to_le_bytes());
    b.extend(b"data");
    b.extend(data_len.to_le_bytes());
    for s in pcm {
        b.extend(s.to_le_bytes());
    }
    std::fs::write(path, b).unwrap();
}

#[test]
fn phrase_round_trip() {
    let k = StationKey::generate().unwrap();
    let phrase = k.phrase();
    assert_eq!(phrase.split(' ').count(), 24);
    let back = StationKey::from_phrase(&format!("  {}  ", phrase.to_uppercase())).unwrap();
    assert_eq!(back.id(), k.id());
    assert!(StationKey::from_phrase("abandon abandon").is_err());
}

#[test]
fn call_signs() {
    let k = StationKey::from_seed([7; 32]);
    let cs = k.id().call_sign();
    assert_eq!(cs, k.id().call_sign());
    assert_eq!(cs.chars().count(), 11);
    assert_eq!(cs.chars().nth(6), Some('·'));
    assert_ne!(cs, StationKey::from_seed([8; 32]).id().call_sign());
    assert_eq!(StationId::from_hex(&k.id().to_hex()).unwrap(), k.id());
    println!("call sign for seed 7: {cs}");
}

fn colours() -> Body {
    Body::Colours { name: "HOSTIS".into(), about: "The Articles, read aloud.".into(), port: None, tip: None }
}

#[test]
fn archive_accepts_a_true_chain() {
    let k = StationKey::generate().unwrap();
    let mut a = Archive::new(k.id());
    a.write(&k, 1000, colours()).unwrap();
    a.write(&k, 2000, Body::Live { chunk: pyrate_core::Chunk::of(b"x", 1), channels: 1, bitrate: 24_000 }).unwrap();
    assert_eq!(a.len(), 2);

    // A carrier rebuilds the log from the wire and gets the same head.
    let mut b = Archive::new(k.id());
    for e in a.entries() {
        b.append(SignedEntry::from_bytes(&e.to_bytes().unwrap()).unwrap()).unwrap();
    }
    assert_eq!(a.head(), b.head());
}

#[test]
fn archive_refuses_forgery_and_reordering() {
    let k = StationKey::generate().unwrap();
    let thief = StationKey::generate().unwrap();
    let mut a = Archive::new(k.id());
    a.write(&k, 1000, colours()).unwrap();

    // Signed by the wrong key.
    let mut forged = SignedEntry::sign(&thief, pyrate_core::Entry {
        station: k.id(), seq: 1, prev: a.head(), at_ms: 2000, body: Body::Strike,
    }).unwrap();
    assert!(matches!(a.append(forged.clone()), Err(Error::Signature)));

    // A true entry altered after signing.
    let mut good = SignedEntry::sign(&k, pyrate_core::Entry {
        station: k.id(), seq: 1, prev: a.head(), at_ms: 2000, body: colours(),
    }).unwrap();
    let mut tampered = good.clone();
    tampered.entry.at_ms = 2001;
    assert!(matches!(a.append(tampered), Err(Error::Signature)));

    // Wrong sequence, broken link, time running backwards.
    good.entry.seq = 5;
    assert!(matches!(a.append(good.clone()), Err(Error::Sequence { .. })));
    forged = SignedEntry::sign(&k, pyrate_core::Entry {
        station: k.id(), seq: 1, prev: [9; 32], at_ms: 2000, body: colours(),
    }).unwrap();
    assert!(matches!(a.append(forged), Err(Error::Chain)));
    assert!(matches!(a.write(&k, 500, colours()), Err(Error::Time)));
}

#[test]
fn nothing_after_strike() {
    let k = StationKey::generate().unwrap();
    let mut a = Archive::new(k.id());
    a.write(&k, 1, colours()).unwrap();
    a.write(&k, 2, Body::Strike).unwrap();
    assert!(a.is_struck());
    assert!(matches!(a.write(&k, 3, colours()), Err(Error::Struck)));
}

#[test]
fn library_skips_withdrawn_tracks() {
    let k = StationKey::generate().unwrap();
    let mut a = Archive::new(k.id());
    let track = |t: &str| Body::Track { title: t.into(), by: "".into(), source: "".into(), channels: 2, bitrate: 64_000, chunks: vec![] };
    a.write(&k, 1, track("one")).unwrap();
    let two = a.write(&k, 2, track("two")).unwrap().hash().unwrap();
    a.write(&k, 3, Body::Withdraw { track: two }).unwrap();
    let lib = a.library().unwrap();
    assert_eq!(lib.len(), 1);
}

#[test]
fn opus_round_trip() {
    let pcm = sine(SAMPLE_RATE, 13.0, 2, 440.0);
    let mut enc = ChunkEncoder::new(Profile::Music).unwrap();
    let mut chunks = enc.push(&pcm).unwrap();
    chunks.extend(enc.finish().unwrap());
    assert_eq!(chunks.len(), 3, "13 s is two full chunks and a short one");
    assert_eq!(chunks[0].0.frames, FRAMES_PER_CHUNK);
    assert_eq!(chunks[2].0.frames, 50);
    let kbps = chunks[0].0.len as f64 * 8.0 / 6.0 / 1000.0;
    assert!((50.0..80.0).contains(&kbps), "about 64 kbit/s, got {kbps}");

    let mut dec = ChunkDecoder::new(2).unwrap();
    let mut out = Vec::new();
    for (c, bytes) in &chunks {
        c.check(bytes).unwrap();
        out.extend(dec.decode(bytes).unwrap());
    }
    assert_eq!(out.len(), 650 * 960 * 2);
    let ratio = rms(&out[48_000..]) / rms(&pcm[48_000..]);
    assert!((0.8..1.2).contains(&ratio), "level survives, ratio {ratio}");

    // A listener tuning in at the second chunk decodes it alone.
    let alone = decode_chunk(&chunks[1].1, 2).unwrap();
    assert_eq!(alone.len(), 300 * 960 * 2);
    assert!(pyrate_core::Chunk::of(b"tampered", 1).check(&chunks[0].1).is_err());
}

#[test]
fn ingest_resamples_and_downmixes() {
    let dir = std::env::temp_dir().join(format!("pyrate-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let wav = dir.join("tone.wav");
    write_wav(&wav, 44_100, 2, &sine(44_100, 7.5, 2, 220.0));

    let got = ingest(&wav, Profile::Talk).unwrap();
    assert_eq!(got.channels, 1);
    assert!((7_400..=7_520).contains(&got.duration_ms), "duration {}", got.duration_ms);
    assert_eq!(got.chunks.len(), 2);
    let pcm = decode_chunk(&got.chunks[0].1, 1).unwrap();
    let r = rms(&pcm[9600..]);
    assert!(r > 8000.0, "tone survives the trip, rms {r}");

    let music = ingest(&wav, Profile::Music).unwrap();
    assert_eq!(music.channels, 2);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn deputies_broadcast_but_cannot_strike() {
    let master = StationKey::generate().unwrap();
    let phone = StationKey::generate().unwrap();
    let mut a = Archive::new(master.id());
    a.write(&master, 1, colours()).unwrap();
    a.write(&master, 2, Body::Deputy { key: phone.id().0, label: "phone".into() }).unwrap();
    let deputed = |seq: u64, prev, body: Body| SignedEntry::sign(&phone, pyrate_core::Entry {
        station: master.id(), seq, prev, at_ms: 3 + seq, body: Body::Deputed { by: phone.id().0, body: Box::new(body) },
    }).unwrap();
    a.append(deputed(2, a.head(), Body::Live { chunk: pyrate_core::Chunk::of(b"x", 1), channels: 1, bitrate: 24_000 })).unwrap();
    assert!(matches!(a.entries()[2].entry.body(), Body::Live { .. }));
    // A deputy cannot strike, appoint or revoke.
    assert!(a.append(deputed(3, a.head(), Body::Strike)).is_err());
    // The master revokes the seized phone; its next entry is refused.
    a.write(&master, 10, Body::Revoke { key: phone.id().0 }).unwrap();
    assert!(a.append(deputed(4, a.head(), colours())).is_err());
    // An unknown key claiming to be a deputy is refused.
    let thief = StationKey::generate().unwrap();
    let forged = SignedEntry::sign(&thief, pyrate_core::Entry {
        station: master.id(), seq: 4, prev: a.head(), at_ms: 20, body: Body::Deputed { by: thief.id().0, body: Box::new(colours()) },
    }).unwrap();
    assert!(a.append(forged).is_err());
}
