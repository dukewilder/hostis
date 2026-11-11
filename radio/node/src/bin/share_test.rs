//! share_test <dir> <wav>
//! Ballast by shares: four phones on a charger, finding a station by their local beacons,
//! each hold part of its library rather than all of it, and the parts sit where a listener
//! working out the scores would look. Run with PYRATE_SHARE_COPIES=2 PYRATE_LAN_ANY=1.
use std::{path::PathBuf, time::Duration};

use pyrate_core::StationId;
use pyrate_node::{Config, Node, BALLAST_AUTO};
use serde_json::json;

fn check(c: bool, what: &str) {
    println!("{} {what}", if c { "PASS" } else { "FAIL" });
}

fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let work = PathBuf::from(&a[1]);
    let cfg = |d: &str, private: bool| Config { dir: work.join(d), quota: 1 << 30, lantern: false, bootstrap: vec![], public_dns: false, private, ballast: if private { BALLAST_AUTO } else { 0 }, carry_all: false, release_keys: vec![] };
    let s = Node::start(cfg("station", false))?;
    let id = s.call(&json!({ "cmd": "raise", "name": "Shares", "about": "a test" }))["id"].as_str().unwrap().to_string();
    let sid = StationId::from_hex(&id)?;
    s.call(&json!({ "cmd": "add_track", "station": id, "path": a[2], "title": "Long tone", "by": "Test" }));
    for _ in 0..120 {
        std::thread::sleep(Duration::from_secs(1));
        if s.call(&json!({ "cmd": "library", "station": id })).as_array().is_some_and(|l| !l.is_empty()) {
            break;
        }
    }
    let total = s.chunks_named(&sid).len();
    s.call(&json!({ "cmd": "lan", "value": true }));
    let phones: Vec<Node> = (0..4).map(|i| Node::start(cfg(&format!("phone-{i}"), true)).unwrap()).collect();
    for p in &phones {
        p.call(&json!({ "cmd": "lan", "value": true }));
        p.call(&json!({ "cmd": "ballast_ok", "value": true, "unmetered": true }));
    }
    let auto = phones[0].call(&json!({ "cmd": "ballast_size", "gb": 0 }));
    check(auto["auto"].as_u64().is_some_and(|b| b > 0), &format!("with no size named the ballast follows the free space ({auto})"));
    // The phones hear one another as they start to carry, and their shares shrink to fit.
    let mut held = vec![];
    for _ in 0..40 {
        std::thread::sleep(Duration::from_secs(10));
        held = phones.iter().map(|p| p.ballast_held()).collect::<Vec<_>>();
        if held.iter().all(|h| *h > 0 && *h < total) {
            break;
        }
    }
    println!("{total} chunks; ballast held {held:?}");
    check(held.iter().all(|h| *h > 0 && *h < total), "every phone holds a part of the station, none the whole");
    let sum: usize = held.iter().sum();
    let mean = sum as f64 / total as f64;
    check((0.8..2.6).contains(&mean), &format!("about two copies of each chunk across the phones ({mean:.2})"));
    let r = phones[1].call(&json!({ "cmd": "ballast_ok", "value": true, "unmetered": true }));
    check(r["carried_bytes"].as_u64().is_some_and(|b| b > 0), &format!("the papers say how much it carries ({})", r["carried_bytes"]));
    Ok(())
}
