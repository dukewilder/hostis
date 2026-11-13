//! Two nodes in one process: one goes ON AIR from a file, the other tunes in and hears it
//! live; then the broadcast ends and the listener leaves the live air within seconds.
//! onair_test <bootstrap id> <wav>. Run with PYRATE_LAN_ANY=1 and PYRATE_NULL_AUDIO=1.
use std::{path::PathBuf, time::{Duration, Instant}};
use pyrate_node::{Config, Node};
use serde_json::json;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let boot = vec![args[1].parse()?];
    // The keeper relays for others, as a computer on a home line does; the listener is a phone
    // that only ever asks through two hops.
    let cfg = |d: &str, private: bool| {
        let _ = std::fs::remove_dir_all(d);
        Config { dir: PathBuf::from(d), quota: 1 << 30, lantern: false, bootstrap: boot.clone(), public_dns: false, private, ballast: 0, carry_all: false, release_keys: vec![] }
    };
    let s = Node::start(cfg("/tmp/oa-keeper", false))?;
    let l = Node::start(cfg("/tmp/oa-listener", true))?;
    let r = s.call(&json!({ "cmd": "raise", "name": "Night Watch Live", "about": "test" }));
    let id = r["id"].as_str().unwrap().to_string();
    println!("{}", s.call(&json!({ "cmd": "go_live", "station": id, "file": args[2] })));
    // The keeper tuned to its own station while on the air hears nothing of it.
    s.call(&json!({ "cmd": "tune", "station": id }));
    // First the new station has to reach the listener at all.
    let t = Instant::now();
    let sees = |n: &Node| n.call(&json!({ "cmd": "stations" })).as_array().is_some_and(|a| a.iter().any(|x| x["id"] == id.as_str()));
    while !sees(&l) && t.elapsed() < Duration::from_secs(180) { std::thread::sleep(Duration::from_millis(500)) }
    println!("{} the listener sees the new station ({:.1} s after it went live)", if sees(&l) { "PASS" } else { "FAIL" }, t.elapsed().as_secs_f64());
    l.call(&json!({ "cmd": "tune", "station": id }));
    let state = |n: &Node| n.call(&json!({ "cmd": "status" }))["now"]["state"].as_str().unwrap_or("").to_string();
    let t = Instant::now();
    while state(&l) != "live" && t.elapsed() < Duration::from_secs(90) {
        std::thread::sleep(Duration::from_millis(250));
        if std::env::var("OA_DEBUG").is_ok() && t.elapsed().as_millis() % 10_000 < 250 {
            let seen = |n: &Node| n.call(&json!({ "cmd": "stations" })).as_array().map(|a| a.iter().filter(|x| x["id"] == id.as_str()).map(|x| format!("carriers {} head {} live {}", x["carriers"], x["head"], x["live"])).collect::<Vec<_>>().join(",")).unwrap_or_default();
            println!("  {:.0} s: listener {} sees [{}]; keeper sees [{}]", t.elapsed().as_secs_f64(), l.call(&json!({ "cmd": "status" }))["now"]["state"], seen(&l), seen(&s));
        }
    }
    let heard = state(&l) == "live";
    println!("{} the listener hears the station live ({:.1} s)", if heard { "PASS" } else { "FAIL" }, t.elapsed().as_secs_f64());
    let own = state(&s);
    println!("{} the keeper on the air is not played its own station (state '{own}')", if own != "live" { "PASS" } else { "FAIL" });
    std::thread::sleep(Duration::from_secs(6));
    s.call(&json!({ "cmd": "end_live" }));
    let t = Instant::now();
    while state(&l) == "live" && t.elapsed() < Duration::from_secs(60) { std::thread::sleep(Duration::from_millis(250)) }
    let left = t.elapsed().as_secs_f64();
    println!("{} the listener leaves the live air after the end ({left:.1} s)", if left < 20.0 { "PASS" } else { "FAIL" });
    Ok(())
}
