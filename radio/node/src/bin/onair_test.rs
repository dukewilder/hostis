//! Two nodes in one process: one goes ON AIR from a file, the other tunes in.
//! onair_test <bootstrap id> <wav>
use std::{path::PathBuf, time::Duration};
use pyrate_node::{Config, Node};
use serde_json::json;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let boot = vec![args[1].parse()?];
    let cfg = |d: &str| Config { dir: PathBuf::from(d), quota: 1 << 30, lantern: false, bootstrap: boot.clone(), public_dns: false, private: true, ballast: 0, release_keys: vec![] };
    let s = Node::start(cfg("/tmp/oa-station"))?;
    let l = Node::start(cfg("/tmp/oa-listener"))?;
    let r = s.call(&json!({ "cmd": "raise", "name": "Night Watch Live", "about": "test" }));
    let id = r["id"].as_str().unwrap().to_string();
    println!("raised {r}");
    println!("{}", s.call(&json!({ "cmd": "go_live", "station": id, "file": args[2] })));
    std::thread::sleep(Duration::from_secs(15));
    println!("listener tunes: {}", l.call(&json!({ "cmd": "tune", "station": id })));
    for _ in 0..8 {
        std::thread::sleep(Duration::from_secs(5));
        println!("listener now: {}  station live: {}  listener sees: {}", l.call(&json!({ "cmd": "status" }))["now"], s.call(&json!({ "cmd": "status" }))["live"], l.call(&json!({ "cmd": "stations" })).as_array().map(|a| a.iter().map(|x| format!("{} carriers {}", x["name"], x["carriers"])).collect::<Vec<_>>().join("; ")).unwrap_or_default());
    }
    Ok(())
}
