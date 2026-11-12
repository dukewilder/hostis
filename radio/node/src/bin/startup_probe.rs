//! startup_probe <dir> <wav>
//! How long a listener waits, step by step: from opening the radio to the first station on the
//! dial, from pressing play to sound, from one station to the next, and how stop behaves. Run
//! with PYRATE_LAN_ANY=1 and PYRATE_NULL_AUDIO=1. A keeper with two stations and two Lanterns
//! make a small network; a phone that never asks a carrier directly is timed on it, then again
//! from the same folder, as a phone opened a second time.
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

use pyrate_node::{Config, Node};
use serde_json::{Value, json};

fn state(n: &Node) -> (String, String) {
    let s = n.call(&json!({ "cmd": "status" }));
    let now = &s["now"];
    (now["state"].as_str().unwrap_or("").to_string(), now["title"].as_str().unwrap_or("").to_string())
}

/// Polls every 100 ms until `done`, printing each change of state; returns the seconds taken.
fn watch(n: &Node, what: &str, limit: u64, done: impl Fn(&Node) -> bool) -> Option<f64> {
    let t = Instant::now();
    let mut last = String::new();
    while t.elapsed() < Duration::from_secs(limit) {
        let (st, title) = state(n);
        let line = format!("{st} {title}");
        if line != last {
            println!("     {:>6.1} s  {what}: {line}", t.elapsed().as_secs_f64());
            last = line;
        }
        if done(n) {
            return Some(t.elapsed().as_secs_f64());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    None
}

fn stations(n: &Node) -> Vec<Value> {
    n.call(&json!({ "cmd": "stations" })).as_array().cloned().unwrap_or_default()
}

fn playing(n: &Node) -> bool {
    n.call(&json!({ "cmd": "status" }))["now"]["state"] == "playing"
}

fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let work = PathBuf::from(&a[1]);
    let cfg = |d: &str, private: bool| Config { dir: work.join(d), quota: 1 << 30, lantern: false, bootstrap: vec![], public_dns: false, private, ballast: 0, carry_all: false, release_keys: vec![] };
    let lan = |n: &Node| {
        n.call(&json!({ "cmd": "lan", "value": true }));
    };
    let k = Node::start(cfg("keeper", false))?;
    lan(&k);
    let mut ids = vec![];
    for name in ["First Light", "Second Watch"] {
        let id = k.call(&json!({ "cmd": "raise", "name": name, "about": "a probe" }))["id"].as_str().unwrap().to_string();
        k.call(&json!({ "cmd": "add_track", "station": id, "path": a[2], "title": format!("{name} tone"), "by": "Probe" }));
        ids.push(id);
    }
    let r1 = Node::start(Config { carry_all: true, lantern: true, ..cfg("lantern1", false) })?;
    lan(&r1);
    let r2 = Node::start(Config { carry_all: true, lantern: true, ..cfg("lantern2", false) })?;
    lan(&r2);
    // The Lanterns take both stations aboard before any phone arrives.
    let t = Instant::now();
    while t.elapsed() < Duration::from_secs(120) && (stations(&r1).len() < 2 || stations(&r2).len() < 2) {
        std::thread::sleep(Duration::from_millis(500));
    }
    println!("network ready in {:.1} s ({} and {} stations on the Lanterns)", t.elapsed().as_secs_f64(), stations(&r1).len(), stations(&r2).len());
    std::thread::sleep(Duration::from_secs(20));

    for run in ["first open", "second open"] {
        println!("== {run}");
        let t0 = Instant::now();
        let p = Node::start(cfg("phone", true))?;
        lan(&p);
        let started = t0.elapsed().as_secs_f64();
        let mut first = None;
        while t0.elapsed() < Duration::from_secs(120) {
            if let Some(s) = stations(&p).into_iter().find(|s| ids.contains(&s["id"].as_str().unwrap_or("").to_string())) {
                first = Some(s);
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let dial = t0.elapsed().as_secs_f64();
        println!("node up {started:.2} s; first station on the dial {dial:.1} s");
        let Some(s) = first else { println!("FAIL no station reached the phone"); continue };
        let id = s["id"].as_str().unwrap().to_string();
        p.call(&json!({ "cmd": "tune", "station": id }));
        let sound = watch(&p, "tuned", 120, playing);
        println!("press play to sound: {sound:?} s (total from open {:.1} s)", t0.elapsed().as_secs_f64());
        // To the other station.
        let other = ids.iter().find(|x| **x != id).unwrap().clone();
        p.call(&json!({ "cmd": "tune", "station": other }));
        let switch = watch(&p, "switched", 120, |n| playing(n) && n.call(&json!({ "cmd": "status" }))["now"]["station"] == other.as_str());
        println!("station to station: {switch:?} s");
        // Stop pressed while the next station is still tuning in: nothing it was fetching may
        // reach the speaker afterwards.
        p.call(&json!({ "cmd": "tune", "station": id }));
        std::thread::sleep(Duration::from_millis(60));
        p.call(&json!({ "cmd": "stop" }));
        std::thread::sleep(Duration::from_secs(6));
        let heard = p.call(&json!({ "cmd": "status" }))["heard_ms"].as_u64().unwrap_or(0);
        println!("{} stop while tuning in: {heard} ms of sound after it", if heard == 0 { "ok  " } else { "FAIL" });
        p.call(&json!({ "cmd": "tune", "station": other }));
        watch(&p, "back on", 60, playing);
        // Stop, and whether it stays stopped.
        p.call(&json!({ "cmd": "stop" }));
        std::thread::sleep(Duration::from_secs(5));
        let st = p.call(&json!({ "cmd": "status" }));
        println!("after stop: tuned {} now {}", st["tuned"], st["now"]);
        drop(p);
        std::thread::sleep(Duration::from_secs(3));
    }
    Ok(())
}
