//! A phone raises a station and sets a recording; a second phone, a stranger, waits to see it
//! and to hear it. Both only ever ask through two hops, as phones do. Prints how long each
//! step took, so the time from raising to being heard by someone else can be watched.
//! raise_test <bootstrap id> <wav> [rounds]. Run with PYRATE_RELAYS, PYRATE_LAN_ANY=1 and
//! PYRATE_NULL_AUDIO=1 against the test network.
use std::{path::PathBuf, time::{Duration, Instant}};
use pyrate_node::{Config, Node};
use serde_json::json;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let boot = vec![args[1].parse()?];
    let rounds: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(1);
    let cfg = |d: &str| {
        let _ = std::fs::remove_dir_all(d);
        Config { dir: PathBuf::from(d), quota: 1 << 30, lantern: false, bootstrap: boot.clone(), public_dns: false, private: true, ballast: 0, carry_all: false, release_keys: vec![] }
    };
    let k = Node::start(cfg("/tmp/rt-keeper"))?;
    let l = Node::start(cfg("/tmp/rt-stranger"))?;
    std::thread::sleep(Duration::from_secs(8));
    let mut seen_s = Vec::new();
    let mut heard_s = Vec::new();
    for round in 0..rounds {
        let t0 = Instant::now();
        let r = k.call(&json!({ "cmd": "raise", "name": format!("Raised {round}"), "about": "test" }));
        let id = r["id"].as_str().unwrap().to_string();
        println!("round {round}: raised {} in {:.1} s", &id[..10], t0.elapsed().as_secs_f64());
        k.call(&json!({ "cmd": "add_track", "station": id, "path": args[2], "title": "Harbour Lights" }));
        let entry = |n: &Node| n.call(&json!({ "cmd": "stations" })).as_array().and_then(|a| a.iter().find(|x| x["id"] == id.as_str()).cloned());
        let mut set_at = None;
        while t0.elapsed() < Duration::from_secs(600) {
            if set_at.is_none() && { let m = k.call(&json!({ "cmd": "status" }))["mine"].clone(); m["id"] == id.as_str() && m["tracks"].as_u64().unwrap_or(0) > 0 } {
                set_at = Some(t0.elapsed().as_secs_f64());
                println!("  recording set at {:.1} s", set_at.unwrap());
            }
            if entry(&l).is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(500));
        }
        let seen = t0.elapsed().as_secs_f64();
        let e = entry(&l);
        println!("  {} the stranger sees it at {seen:.1} s (listed {})", if e.is_some() { "PASS" } else { "FAIL" }, e.as_ref().map(|e| e["listed"].to_string()).unwrap_or_default());
        seen_s.push(seen);
        l.call(&json!({ "cmd": "tune", "station": id }));
        let state = |n: &Node| n.call(&json!({ "cmd": "status" }))["now"]["state"].as_str().unwrap_or("").to_string();
        while state(&l) != "playing" && t0.elapsed() < Duration::from_secs(700) {
            std::thread::sleep(Duration::from_millis(250));
        }
        let heard = t0.elapsed().as_secs_f64();
        println!("  {} the stranger hears it at {heard:.1} s (state '{}')", if state(&l) == "playing" { "PASS" } else { "FAIL" }, state(&l));
        heard_s.push(heard);
        l.call(&json!({ "cmd": "stop" }));
        // Listed once the keeper's work climbs to it.
        let t1 = Instant::now();
        while !entry(&l).is_some_and(|e| e["listed"] == true) && t1.elapsed() < Duration::from_secs(900) {
            std::thread::sleep(Duration::from_secs(2));
        }
        println!("  listed for the stranger {:.0} s after it was heard", t1.elapsed().as_secs_f64());
    }
    println!("seen {seen_s:.0?} heard {heard_s:.0?}");
    Ok(())
}
