//! privacy_test <dir> <wav>
//! Privacy fails closed. A phone that cannot find two hops sends no request naming a station
//! straight to a carrier: no log, no chunk. With "ask carriers directly" turned on, it plays.
//! Run with PYRATE_LAN_ANY=1: the nodes find each other on the local network.
use std::{path::PathBuf, time::Duration};

use pyrate_node::{Config, Node};
use serde_json::json;

fn check(c: bool, what: &str) {
    println!("{} {what}", if c { "PASS" } else { "FAIL" });
}

fn playing(n: &Node) -> bool {
    n.call(&json!({ "cmd": "status" }))["now"]["state"] == "playing"
}

fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let work = PathBuf::from(&a[1]);
    let cfg = |d: &str, private: bool| Config { dir: work.join(d), quota: 1 << 30, lantern: false, bootstrap: vec![], public_dns: false, private, ballast: 0, carry_all: false, release_keys: vec![] };
    let k = Node::start(cfg("keeper", false))?;
    k.call(&json!({ "cmd": "lan", "value": true }));
    let id = k.call(&json!({ "cmd": "raise", "name": "Watched", "about": "a test" }))["id"].as_str().unwrap().to_string();
    k.call(&json!({ "cmd": "add_track", "station": id, "path": a[2], "title": "Tone", "by": "Test" }));
    let l = Node::start(cfg("listener", true))?;
    l.call(&json!({ "cmd": "lan", "value": true }));
    let strict = l.call(&json!({ "cmd": "direct_ok" }));
    check(strict["value"] == false, "a phone asks carriers directly only when its owner says so");
    // A broadcasting phone that never connects directly, with one other node in reach.
    let b = Node::start(cfg("broadcaster", true))?;
    b.call(&json!({ "cmd": "lan", "value": true }));
    let bid = b.call(&json!({ "cmd": "raise", "name": "Held Back", "about": "a test" }))["id"].as_str().unwrap().to_string();
    b.call(&json!({ "cmd": "add_track", "station": bid, "path": a[2], "title": "Tone", "by": "Test" }));
    let bid = pyrate_core::StationId::from_hex(&bid)?;
    // Wait for the listener to hear of the station, then tune it with only one other node known.
    for _ in 0..60 {
        std::thread::sleep(Duration::from_secs(1));
        if l.call(&json!({ "cmd": "stations" })).as_array().is_some_and(|v| !v.is_empty()) {
            break;
        }
    }
    let before = k.served_direct();
    l.call(&json!({ "cmd": "tune", "station": id }));
    std::thread::sleep(Duration::from_secs(25));
    check(!playing(&l), "with one node known and no two-hop route, nothing plays");
    check(k.served_direct() == before, &format!("and the carrier was asked for nothing directly ({} requests)", k.served_direct() - before));
    check(!k.heard_of(&bid), "a broadcasting phone with one node in reach hands that node nothing of its station");
    l.call(&json!({ "cmd": "direct_ok", "value": true }));
    let mut ok = false;
    for _ in 0..40 {
        std::thread::sleep(Duration::from_secs(1));
        if playing(&l) {
            ok = true;
            break;
        }
    }
    check(ok, "with the switch on, it plays");
    // A Lantern makes two hops possible: a new phone, switch off, plays through them.
    let r = Node::start(Config { carry_all: true, lantern: true, ..cfg("relay", false) })?;
    r.call(&json!({ "cmd": "lan", "value": true }));
    let l2 = Node::start(cfg("listener2", true))?;
    l2.call(&json!({ "cmd": "lan", "value": true }));
    for _ in 0..60 {
        std::thread::sleep(Duration::from_secs(1));
        if l2.call(&json!({ "cmd": "stations" })).as_array().is_some_and(|v| !v.is_empty()) {
            break;
        }
    }
    l2.call(&json!({ "cmd": "tune", "station": id }));
    let mut ok2 = false;
    for _ in 0..60 {
        std::thread::sleep(Duration::from_secs(1));
        if playing(&l2) {
            ok2 = true;
            break;
        }
    }
    check(ok2, "with a Lantern beside the keeper, a phone that never asks directly plays through two hops");
    let mut out = false;
    for _ in 0..120 {
        std::thread::sleep(Duration::from_secs(1));
        if r.heard_of(&bid) || k.heard_of(&bid) {
            out = true;
            break;
        }
    }
    check(out, "with a second node in reach, the broadcasting phone's station goes out two hops away");
    Ok(())
}
