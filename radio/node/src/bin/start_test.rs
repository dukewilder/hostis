//! start_test keeper <dir> <wav> | start_test listen <dir> <station>
//! How fast a radio opens: the keeper raises a station on the local network and stays up;
//! each listen run starts a phone's node, tunes the station and prints the seconds to a full
//! dial, a title and sound. A second run on the same folder is the warm open.
use std::{path::PathBuf, time::{Duration, Instant}};

use pyrate_node::{Config, Node};
use serde_json::json;

fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let dir = PathBuf::from(&a[2]);
    let private = a[1] == "listen";
    let n = Node::start(Config { dir, quota: 1 << 30, lantern: false, bootstrap: vec![], public_dns: false, private, ballast: 0, carry_all: false, release_keys: vec![] })?;
    n.call(&json!({ "cmd": "lan", "value": true }));
    if a[1] == "keeper" {
        let id = n.call(&json!({ "cmd": "raise", "name": "Quick", "about": "a test" }))["id"].as_str().unwrap().to_string();
        n.call(&json!({ "cmd": "add_track", "station": id, "path": a[3], "title": "Tone", "by": "Test" }));
        println!("{id}");
        loop {
            std::thread::sleep(Duration::from_secs(60));
        }
    }
    if std::env::var_os("SYNC_DEBUG").is_some() {
        let id = pyrate_core::StationId::from_hex(&a[3])?;
        let t = Instant::now();
        let r = n.rt.block_on(n.inner.sync_log_direct(id));
        println!("direct sync {r:?} in {:?}", t.elapsed());
        let t = Instant::now();
        let r = n.rt.block_on(async { tokio::time::timeout(Duration::from_secs(30), n.inner.sync_log(id)).await });
        println!("archive here: {:?}", n.archive(&id).map(|a| a.len()));
        println!("sync {r:?} in {:?}", t.elapsed());
        return Ok(());
    }
    let t = Instant::now();
    n.call(&json!({ "cmd": "tune", "station": a[3] }));
    let (mut dial, mut title, mut sound) = (None, None, None);
    while t.elapsed() < Duration::from_secs(25) && sound.is_none() {
        std::thread::sleep(Duration::from_millis(100));
        if dial.is_none() && n.call(&json!({ "cmd": "stations" })).as_array().is_some_and(|l| !l.is_empty()) {
            dial = Some(t.elapsed().as_secs_f64());
        }
        let s = n.call(&json!({ "cmd": "status" }));
        let now = &s["now"];
        if title.is_none() && now["title"].as_str().is_some_and(|x| !x.is_empty()) {
            title = Some(t.elapsed().as_secs_f64());
        }
        if now["state"] == "playing" {
            sound = Some(t.elapsed().as_secs_f64());
        }
    }
    println!("dial {dial:?} s, title {title:?} s, sound {sound:?} s");
    // Long enough for the Log to be written down, as it would be in any real listen.
    std::thread::sleep(Duration::from_secs(15));
    if sound.is_none() {
        println!("{}", n.call(&json!({ "cmd": "status" })));
        println!("{}", n.call(&json!({ "cmd": "stations" })));
    }
    Ok(())
}
