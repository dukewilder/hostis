//! Two nodes and a Lantern: a station raises art, a listener browsing sees it without
//! anything landing on its disk. art_test <bootstrap id> <picture> <wav with cover beside it>
use std::{path::PathBuf, time::Duration};
use pyrate_node::{Config, Node};
use serde_json::json;

fn files(d: &str) -> usize {
    walk(&PathBuf::from(d).join("hold")) + walk(&PathBuf::from(d).join("chunks"))
}
fn walk(p: &std::path::Path) -> usize {
    std::fs::read_dir(p).map(|r| r.flatten().map(|e| if e.path().is_dir() { walk(&e.path()) } else { 1 }).sum()).unwrap_or(0)
}

fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let boot = vec![a[1].parse()?];
    let cfg = |d: &str| Config { dir: PathBuf::from(d), quota: 1 << 30, lantern: false, bootstrap: boot.clone(), public_dns: false, private: true, ballast: 0, carry_all: false, release_keys: vec![] };
    let s = Node::start(cfg("/home/claude/arttest4/station"))?;
    let l = Node::start(cfg("/home/claude/arttest4/listener"))?;
    let id = s.call(&json!({ "cmd": "raise", "name": "Art Test", "about": "pictures" }))["id"].as_str().unwrap().to_string();
    println!("cover: {}", s.call(&json!({ "cmd": "art_set", "station": id, "kind": "cover", "path": a[2] })));
    println!("banner: {}", s.call(&json!({ "cmd": "art_set", "station": id, "kind": "banner", "path": a[2] })));
    println!("track: {}", s.call(&json!({ "cmd": "add_track", "station": id, "path": a[3], "title": "Tone" })));
    std::thread::sleep(Duration::from_secs(8));
    println!("station status: {}", s.call(&json!({ "cmd": "status" }))["mine"]);
    println!("jobs: {}", s.call(&json!({ "cmd": "status" }))["jobs"]);
    let arch = s.archive(&pyrate_core::StationId::from_hex(&id)?).unwrap();
    println!("track art entries: {}", arch.entries().iter().filter(|e| matches!(e.entry.body(), pyrate_core::Body::TrackArt { .. })).count());
    let before = files("/home/claude/arttest4/listener");
    for i in 0..30 {
        std::thread::sleep(Duration::from_secs(3));
        let list = l.call(&json!({ "cmd": "stations" }));
        if i % 5 == 0 { println!("{i}: listener lists {}", list.as_array().map(|v| v.len()).unwrap_or(0)); }
        let Some(st) = list.as_array().and_then(|v| v.iter().find(|x| x["id"] == id.as_str())).cloned() else { continue };
        let Some(cover) = st["cover"].as_str() else { println!("{i}: listed, no cover yet"); continue };
        l.call(&json!({ "cmd": "art_want", "station": id, "hash": cover }));
        if let Some((w, h, px)) = l.art_pixels(cover) {
            println!("listener sees cover {w}x{h} ({} bytes RGBA), accent {}", px.len(), st["accent"]);
            println!("cover on listener disk: {} (files before {before}, after {})", l.on_disk(cover), files("/home/claude/arttest4/listener"));
            return Ok(());
        }
    }
    anyhow::bail!("listener never saw the cover")
}
