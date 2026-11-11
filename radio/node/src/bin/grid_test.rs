//! grid_test <dir> <wav> [jpg]
//! Off the grid: a station packed into one file plays on a node that has never been online,
//! with every chunk checked; a damaged file is refused; and two nodes with nearby radios on
//! find each other on the local network with no bootstrap and no relay.
use std::{path::PathBuf, time::Duration};

use pyrate_core::StationId;
use pyrate_node::{Config, Node};
use serde_json::json;

fn check(c: bool, what: &str) {
    println!("{} {what}", if c { "PASS" } else { "FAIL" });
}

fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let work = PathBuf::from(&a[1]);
    let cfg = |d: &str| Config { dir: work.join(d), quota: 1 << 30, lantern: false, bootstrap: vec![], public_dns: false, private: true, ballast: 0, carry_all: false, release_keys: vec![] };
    let s = Node::start(cfg("station"))?;
    let r = s.call(&json!({ "cmd": "raise", "name": "Off Grid", "about": "a test" }));
    let id = r["id"].as_str().unwrap().to_string();
    let sid = StationId::from_hex(&id)?;
    if let Some(jpg) = a.get(3) {
        s.call(&json!({ "cmd": "art_set", "station": id, "kind": "cover", "path": jpg }));
    }
    s.call(&json!({ "cmd": "add_track", "station": id, "path": a[2], "title": "Tone", "by": "Test" }));
    for _ in 0..60 {
        std::thread::sleep(Duration::from_secs(1));
        if s.call(&json!({ "cmd": "library", "station": id })).as_array().is_some_and(|l| !l.is_empty()) {
            break;
        }
    }
    let pack = work.join("off-grid.pyrate");
    let e = s.call(&json!({ "cmd": "pack_export", "station": id, "path": pack.to_string_lossy() }));
    check(e["missing"] == 0 && e["chunks"].as_u64().unwrap_or(0) > 0, &format!("the station packs into one file ({e})"));

    let o = Node::start(cfg("offline"))?;
    let i = o.call(&json!({ "cmd": "pack_import", "path": pack.to_string_lossy() }));
    check(i["station"] == id.as_str(), &format!("a node that was never online opens it ({i})"));
    let (sa, oa) = (s.archive(&sid).map(|a| a.len()), o.archive(&sid).map(|a| a.len()));
    check(sa.is_some() && sa == oa, &format!("the whole log came across ({sa:?} and {oa:?})"));
    let lib = o.call(&json!({ "cmd": "library", "station": id }));
    check(lib.as_array().is_some_and(|l| l.len() == 1), &format!("its library reads the same ({})", lib.as_array().map(|l| l.len()).unwrap_or(0)));
    let hashes = s.chunks_named(&sid);
    let all = hashes.iter().all(|h| o.chunk_here(h));
    check(!hashes.is_empty() && all, &format!("every chunk is aboard and opens ({} chunks)", hashes.len()));

    let mut bad = std::fs::read(&pack)?;
    let first = s.chunks_named(&sid)[0];
    let at = bad.windows(32).rposition(|w| w == first).expect("the chunk is in the file");
    bad[at + 32 + 4 + 10] ^= 0xff;
    let badp = work.join("damaged.pyrate");
    std::fs::write(&badp, &bad)?;
    let b = Node::start(cfg("third"))?;
    let _ = b.call(&json!({ "cmd": "pack_import", "path": badp.to_string_lossy() }));
    let refused = !b.chunk_here(&first) && s.chunks_named(&sid).iter().filter(|h| **h != first).all(|h| b.chunk_here(h));
    check(refused, "a chunk damaged in the file is refused, and the rest are taken");
    let mut junk = b"pyrate/pack/v1\n".to_vec();
    junk.extend([7u8; 64]);
    std::fs::write(work.join("junk.pyrate"), &junk)?;
    check(b.call(&json!({ "cmd": "pack_import", "path": work.join("junk.pyrate").to_string_lossy() }))["error"].is_string(), "a file that is not a station is refused");

    // Nearby radios: two nodes with no bootstrap find each other by their beacons.
    let x = Node::start(cfg("lan-a"))?;
    let y = Node::start(cfg("lan-b"))?;
    x.call(&json!({ "cmd": "lan", "value": true }));
    y.call(&json!({ "cmd": "lan", "value": true }));
    let mut met = false;
    for _ in 0..30 {
        std::thread::sleep(Duration::from_secs(2));
        // With no bootstrap and no relay, only the beacon can have told x where y is.
        if x.diag(y.id()).contains("Ok(Ok(\"ok\"))") {
            met = true;
            break;
        }
    }
    check(met, "two radios on one network find each other with no bootstrap and no relay");
    Ok(())
}
