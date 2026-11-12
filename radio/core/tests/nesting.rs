//! KEY-008: an entry whose body is a deputy's entry wrapped inside another, many deep.
fn varint(mut n: usize, out: &mut Vec<u8>) {
    loop {
        let b = (n & 0x7f) as u8;
        n >>= 7;
        if n == 0 {
            out.push(b);
            return;
        }
        out.push(b | 0x80);
    }
}

fn nested(depth: usize) -> Vec<u8> {
    // Sizes from the inside out, then written from the outside in.
    let mut sizes = vec![1usize];
    for _ in 0..depth {
        let inner = *sizes.last().unwrap();
        let mut v = vec![];
        varint(inner, &mut v);
        sizes.push(1 + 32 + v.len() + inner);
    }
    let mut out = Vec::with_capacity(*sizes.last().unwrap());
    for k in (0..depth).rev() {
        out.push(8);
        out.extend([7u8; 32]);
        varint(sizes[k], &mut out);
    }
    out.push(4);
    out
}

#[test]
fn deeply_nested_deputed_bodies_do_not_crash() {
    // 200,000 deep is about 7 MB, inside the 16 MiB a peer may send.
    for depth in [1usize, 2, 10, 1_000, 30_000, 200_000] {
        let raw = nested(depth);
        let t = std::time::Instant::now();
        let b = match postcard::from_bytes::<pyrate_core::Body>(&raw) { Ok(b) => b, Err(e) => panic!("depth {depth}: {e:?}") };
        let ms = t.elapsed().as_millis();
        println!("depth {depth}: {} bytes, {ms} ms", raw.len());
        assert!(ms < 500, "decoding is not quadratic in depth");
        // However deep the bytes go, what is decoded stops after a few levels: past that the
        // body is kept as unread bytes.
        let mut levels = 0;
        let mut at = &b;
        while let pyrate_core::Body::Deputed { body, .. } = at {
            levels += 1;
            at = body;
        }
        assert!(levels <= 3, "depth {depth}: decoded {levels} levels");
        if depth > 2 {
            assert!(matches!(at, pyrate_core::Body::Unknown(_)));
        }
    }
}

#[test]
fn a_deputys_entry_survives_the_round_trip() {
    let master = pyrate_core::StationKey::generate().unwrap();
    let deputy = pyrate_core::StationKey::generate().unwrap();
    let mut a = pyrate_core::Archive::new(master.id());
    a.write(&master, 1, pyrate_core::Body::Deputy { key: deputy.id().0, label: "phone".into() }).unwrap();
    let body = pyrate_core::Body::Deputed { by: deputy.id().0, body: Box::new(pyrate_core::Body::Notice { text: "hello".into(), until_ms: 0 }) };
    let e = pyrate_core::Entry { station: master.id(), seq: a.len(), prev: a.head(), at_ms: 2, body };
    let bytes = pyrate_core::SignedEntry::sign(&deputy, e).unwrap().to_bytes().unwrap();
    let back = pyrate_core::SignedEntry::from_bytes(&bytes).unwrap();
    a.append(back).unwrap();
    assert!(matches!(a.entries()[1].entry.body.inner(), pyrate_core::Body::Notice { .. }));
}
