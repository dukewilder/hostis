//! KEY-002, KEY-003, ARC-009: who may write what into a station's log, and what a log refuses
//! however well it is signed.
use pyrate_core::{Archive, Body, Entry, SignedEntry, StationKey};

fn entry(a: &Archive, at_ms: u64, body: Body) -> Entry {
    Entry { station: a.station(), seq: a.len(), prev: a.head(), at_ms, body }
}

/// A body written by `by` for the station: plain if `by` is the master, wrapped as a deputy's
/// entry otherwise, the way the apps write them.
fn as_written(master: &StationKey, by: &StationKey, body: Body) -> Body {
    if by.id() == master.id() { body } else { Body::Deputed { by: by.id().0, body: Box::new(body) } }
}

#[test]
fn each_kind_of_entry_takes_only_the_keys_allowed_it() {
    let master = StationKey::generate().unwrap();
    let deputy = StationKey::generate().unwrap();
    let revoked = StationKey::generate().unwrap();
    let stranger = StationKey::generate().unwrap();
    let ordinary = || {
        vec![
            Body::Colours { name: "n".into(), about: String::new(), port: None, tip: None },
            Body::Track { title: "t".into(), by: "b".into(), source: "s".into(), channels: 2, bitrate: 64_000, chunks: vec![] },
            Body::Notice { text: "x".into(), until_ms: 0 },
            Body::Details { kinds: vec![], mature: false },
            Body::Rating { explicit: true },
        ]
    };
    let master_only = || vec![Body::Deputy { key: [9; 32], label: "x".into() }, Body::Revoke { key: [9; 32] }, Body::Strike];
    for (who, key) in [("master", &master), ("deputy", &deputy), ("revoked deputy", &revoked), ("stranger", &stranger)] {
        for (kind, bodies) in [("ordinary", ordinary()), ("master-only", master_only())] {
            for body in bodies {
                let mut a = Archive::new(master.id());
                a.write(&master, 1, Body::Deputy { key: deputy.id().0, label: "d".into() }).unwrap();
                a.write(&master, 2, Body::Deputy { key: revoked.id().0, label: "r".into() }).unwrap();
                a.write(&master, 3, Body::Revoke { key: revoked.id().0 }).unwrap();
                let wrapped = as_written(&master, key, body.clone());
                let e = SignedEntry::sign(key, entry(&a, 4, wrapped)).unwrap();
                let took = a.append(e).is_ok();
                let allowed = match (who, kind) {
                    ("master", _) => true,
                    ("deputy", "ordinary") => true,
                    _ => false,
                };
                assert_eq!(took, allowed, "{who} writing {kind} {body:?}");
            }
        }
    }
}

#[test]
fn a_signed_entry_cannot_be_moved_replayed_forked_or_backdated() {
    let k = StationKey::generate().unwrap();
    let other = StationKey::generate().unwrap();
    let mut a = Archive::new(k.id());
    a.write(&k, 10, Body::Notice { text: "one".into(), until_ms: 0 }).unwrap();
    let first = a.entries()[0].clone();
    a.write(&k, 20, Body::Notice { text: "two".into(), until_ms: 0 }).unwrap();
    // Replayed: the same entry again.
    assert!(a.append(first.clone()).is_err(), "replayed entry");
    // Another station's, signed by its own key.
    let mut b = Archive::new(other.id());
    let theirs = b.write(&other, 30, Body::Notice { text: "x".into(), until_ms: 0 }).unwrap().clone();
    assert!(a.append(theirs).is_err(), "another station's entry");
    // The right sequence number but pointing at an older head: a fork.
    let fork = SignedEntry::sign(&k, Entry { station: k.id(), seq: a.len(), prev: first.hash().unwrap(), at_ms: 40, body: Body::Strike }).unwrap();
    assert!(a.append(fork).is_err(), "forked entry");
    // A gap in the sequence.
    let gap = SignedEntry::sign(&k, Entry { station: k.id(), seq: a.len() + 1, prev: a.head(), at_ms: 40, body: Body::Strike }).unwrap();
    assert!(a.append(gap).is_err(), "skipped sequence");
    // Dated before the entry it follows.
    let back = SignedEntry::sign(&k, entry(&a, 5, Body::Notice { text: "late".into(), until_ms: 0 })).unwrap();
    assert!(a.append(back).is_err(), "backdated entry");
    // Signed by another key for this station.
    let forged = SignedEntry::sign(&other, entry(&a, 50, Body::Notice { text: "forged".into(), until_ms: 0 })).unwrap();
    assert!(a.append(forged).is_err(), "entry signed by a stranger");
    // A deputy's entry claiming the master's name in its wrapper.
    let deputy = StationKey::generate().unwrap();
    let lie = SignedEntry::sign(&deputy, entry(&a, 60, Body::Deputed { by: k.id().0, body: Box::new(Body::Notice { text: "lie".into(), until_ms: 0 }) })).unwrap();
    assert!(a.append(lie).is_err(), "deputy entry naming the master as its writer");
    assert_eq!(a.len(), 2);
    // After a strike nothing more is taken.
    a.write(&k, 70, Body::Strike).unwrap();
    assert!(a.write(&k, 80, Body::Notice { text: "after".into(), until_ms: 0 }).is_err(), "entry after a strike");
}
