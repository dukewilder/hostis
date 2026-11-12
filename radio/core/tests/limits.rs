//! ARC-002: every kind of entry has limits a signature cannot lift.
use pyrate_core::{Archive, Body, SignedEntry, StationKey, archive::{MAX_ENTRY_BYTES, MAX_LONG, MAX_SHORT}};

fn try_body(body: Body) -> bool {
    let k = StationKey::generate().unwrap();
    let mut a = Archive::new(k.id());
    a.write(&k, 1, body).is_ok()
}

#[test]
fn text_fields_are_bounded_at_the_limit_and_one_past_it() {
    let s = |n| "x".repeat(n);
    assert!(try_body(Body::Colours { name: s(MAX_SHORT), about: s(MAX_LONG), port: None, tip: None }));
    assert!(!try_body(Body::Colours { name: s(MAX_SHORT + 1), about: String::new(), port: None, tip: None }));
    assert!(!try_body(Body::Colours { name: String::new(), about: s(MAX_LONG + 1), port: None, tip: None }));
    assert!(!try_body(Body::Colours { name: String::new(), about: String::new(), port: Some(s(MAX_SHORT + 1)), tip: None }));
    assert!(!try_body(Body::Colours { name: String::new(), about: String::new(), port: None, tip: Some(s(MAX_SHORT + 1)) }));
    let track = |title: String, source: String| Body::Track { title, by: "b".into(), source, channels: 2, bitrate: 64_000, chunks: vec![] };
    assert!(try_body(track(s(MAX_SHORT), s(MAX_LONG))));
    assert!(!try_body(track(s(MAX_SHORT + 1), String::new())));
    assert!(!try_body(track(String::new(), s(MAX_LONG + 1))));
    assert!(!try_body(Body::Notice { text: s(MAX_LONG + 1), until_ms: 0 }));
    assert!(!try_body(Body::Deputy { key: [1; 32], label: s(MAX_SHORT + 1) }));
    assert!(!try_body(Body::Details { kinds: vec!["k".into(); 65], mature: false }));
    assert!(!try_body(Body::Show { title: String::new(), about: String::new(), at_ms: 0, tracks: vec![[0; 32]; 10_001], live_min: 0 }));
}

#[test]
fn no_entry_past_a_mebibyte_is_decoded_or_taken() {
    // The largest legal recording fits; a bigger encoding is refused before it is decoded.
    let k = StationKey::generate().unwrap();
    let mut a = Archive::new(k.id());
    let chunks = (0..20_000u32).map(|i| pyrate_core::Chunk { hash: *blake3::hash(&i.to_le_bytes()).as_bytes(), len: 50_000, frames: 300 }).collect();
    let e = a.write(&k, 1, Body::Track { title: "t".repeat(MAX_SHORT), by: "b".repeat(MAX_SHORT), source: "s".repeat(MAX_LONG), channels: 2, bitrate: 64_000, chunks }).unwrap();
    let n = e.to_bytes().unwrap().len();
    println!("largest legal recording: {n} bytes");
    assert!(n <= MAX_ENTRY_BYTES);
    assert_eq!(a.bytes(), n as u64);
    let mut big = vec![0u8; MAX_ENTRY_BYTES + 1];
    big[0] = 1;
    assert!(matches!(SignedEntry::from_bytes(&big), Err(pyrate_core::Error::TooBig)));
}
