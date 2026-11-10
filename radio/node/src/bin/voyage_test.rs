//! The whole voyage on one machine: a Lantern, a broadcaster, a listener who becomes a hand.
//! voyage_test <work dir> <wav> <picture>
use std::{path::PathBuf, time::Duration};
use pyrate_core::{Body, StationKey};

use pyrate_node::{Config, Node};
use serde::Serialize;
use serde_json::json;

fn files(d: &PathBuf) -> usize {
    fn walk(p: &std::path::Path) -> usize {
        std::fs::read_dir(p).map(|r| r.flatten().map(|e| if e.path().is_dir() { walk(&e.path()) } else { 1 }).sum()).unwrap_or(0)
    }
    walk(&d.join("chunks"))
}

fn check(ok: bool, what: &str) {
    println!("{} {what}", if ok { "PASS" } else { "FAIL" });
}

/// A log file in the format used before bodies were framed, for a station whose key is here.
fn old_log(dir: &PathBuf) -> StationKey {
    #[derive(Serialize)]
    struct OldEntry { station: pyrate_core::StationId, seq: u64, prev: [u8; 32], at_ms: u64, body: Body }
    #[derive(Serialize)]
    struct OldSigned { entry: OldEntry, sig: (u64, u64, u64, u64, u64, u64, u64, u64) }
    let k = StationKey::generate().unwrap();
    std::fs::create_dir_all(dir.join("logs")).unwrap();
    std::fs::create_dir_all(dir.join("keys")).unwrap();
    std::fs::write(dir.join("keys").join(k.id().to_hex()), k.seed()).unwrap();
    let mut f = Vec::new();
    let mut hashes = Vec::new();
    let chunk = pyrate_core::Chunk { hash: [9; 32], len: 10, frames: 300 };
    let mut add = |at: u64, body: Body, f: &mut Vec<u8>, hashes: &mut Vec<[u8; 32]>| {
        let rec = OldSigned { entry: OldEntry { station: k.id(), seq: hashes.len() as u64, prev: [0; 32], at_ms: at, body }, sig: (0, 0, 0, 0, 0, 0, 0, 0) };
        let b = postcard::to_stdvec(&rec).unwrap();
        hashes.push(*blake3::hash(&b).as_bytes());
        f.extend((b.len() as u32).to_be_bytes());
        f.extend(b);
    };
    add(1_700_000_000_000, Body::Colours { name: "The Old Port".into(), about: "from before".into(), port: None, tip: None }, &mut f, &mut hashes);
    add(1_700_000_001_000, Body::Track { title: "Kept".into(), by: "".into(), source: "".into(), channels: 1, bitrate: 24_000, chunks: vec![chunk; 3] }, &mut f, &mut hashes);
    add(1_700_000_002_000, Body::Track { title: "Gone".into(), by: "".into(), source: "".into(), channels: 1, bitrate: 24_000, chunks: vec![chunk; 3] }, &mut f, &mut hashes);
    let gone = hashes[2];
    add(1_700_000_003_000, Body::Withdraw { track: gone }, &mut f, &mut hashes);
    std::fs::write(dir.join("logs").join(format!("{}.log", k.id().to_hex())), f).unwrap();
    k
}

fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let work = PathBuf::from(&a[1]);
    let cfg = |d: &str, lantern: bool, boot: Vec<iroh::EndpointId>| Config {
        dir: work.join(d), quota: 1 << 30, lantern, bootstrap: boot, public_dns: false, private: !lantern, ballast: 0, carry_all: true, release_keys: vec![],
    };
    // The Lantern starts with an old-format station of its own.
    let old = old_log(&work.join("lantern"));
    let lantern = Node::start(cfg("lantern", true, vec![]))?;
    let st = lantern.call(&json!({ "cmd": "status" }));
    check(st["mine"]["name"] == "The Old Port", &format!("an old-format log is carried into the new format ({})", st["mine"]["name"]));
    check(work.join("lantern/logs").join(format!("{}.legacy", old.id().to_hex())).exists(), "the old file is set aside, not deleted");
    let lib = lantern.call(&json!({ "cmd": "library", "station": old.id().to_hex() }));
    let titles: Vec<String> = lib.as_array().into_iter().flatten().filter_map(|t| t["title"].as_str().map(String::from)).collect();
    check(titles == vec!["Kept".to_string()], &format!("the old library and its withdrawals come across ({titles:?})"));
    check(lantern.archive(&old.id()).is_some_and(|a| a.entries().first().is_some_and(|e| e.entry.at_ms == 1_700_000_000_000)), "at the times they were first written");
    let boot = vec![lantern.id()];
    let s = Node::start(cfg("station", false, boot.clone()))?;
    let l = Node::start(cfg("listener", false, boot.clone()))?;

    let r = s.call(&json!({ "cmd": "raise", "name": "Voyage", "about": "a test" }));
    let id = r["id"].as_str().unwrap().to_string();
    let words = r["words"].as_str().unwrap().to_string();
    println!("details: {}", s.call(&json!({ "cmd": "details_set", "station": id, "kinds": ["talk", "Readings"], "mature": true })));
    println!("papers: {}", s.call(&json!({ "cmd": "colours_set", "station": id, "port": "Austin", "tip": "dylan@example.com" })));
    println!("art: {}", s.call(&json!({ "cmd": "art_set", "station": id, "kind": "cover", "path": a[3] })));
    println!("track: {}", s.call(&json!({ "cmd": "add_track", "station": id, "path": a[2], "title": "Tone", "by": "Test" })));
    std::thread::sleep(Duration::from_secs(6));
    let lib = s.call(&json!({ "cmd": "library", "station": id }));
    let track = lib[0]["hash"].as_str().unwrap_or("").to_string();
    println!("rota: {}", s.call(&json!({ "cmd": "rota_set", "station": id, "blocks": [{ "start_min": 0, "end_min": 1440, "tracks": [track], "shuffle": false }], "ident": null, "ident_every_min": 30 })));

    let mut seen = json!(null);
    for _ in 0..40 {
        std::thread::sleep(Duration::from_secs(3));
        let list = l.call(&json!({ "cmd": "stations" }));
        if let Some(x) = list.as_array().and_then(|v| v.iter().find(|x| x["id"] == id.as_str())) {
            if x["tip"].is_string() && x["cover"].is_string() {
                seen = x.clone();
                break;
            }
        }
    }
    check(seen["kinds"] == json!(["talk", "readings"]), &format!("kinds travel on the card ({})", seen["kinds"]));
    check(seen["mature"] == true && seen["port"] == "Austin" && seen["tip"] == "dylan@example.com", "mature, port and tip travel on the card");
    check(seen["cover"].is_string() && seen["accent"].is_string(), "cover and accent travel on the card");

    let sid = pyrate_core::StationId::from_hex(&id)?;
    for i in 0..10 {
        let (sl, ll, la) = (s.archive(&sid).map(|a| a.len()), l.archive(&sid).map(|a| a.len()), lantern.archive(&sid).map(|a| a.len()));
        println!("log lengths {i}: station {sl:?} listener {ll:?} lantern {la:?}");
        if sl == ll { break; }
        std::thread::sleep(Duration::from_secs(4));
    }
    let bill = l.call(&json!({ "cmd": "schedule", "station": id, "hours": 2 }));
    println!("bill: {}", bill.to_string().chars().take(300).collect::<String>());
    if let Some(a) = l.archive(&sid) {
        println!("listener log now {} entries: {:?}", a.len(), a.entries().iter().map(|e| format!("{:?}", e.entry.body()).chars().take(30).collect::<String>()).collect::<Vec<_>>());
    }
    check(bill.as_array().is_some_and(|b| b.len() >= 2 && b[0]["title"].is_string()), &format!("the listener works out the watch bill ({} items)", bill.as_array().map(|b| b.len()).unwrap_or(0)));
    // A track added this minute first plays at the next whole minute.
    let wait = 61_000 - (pyrate_node::now_ms() % 60_000);
    std::thread::sleep(Duration::from_millis(wait + 1_500));
    let now = l.call(&json!({ "cmd": "now_for", "stations": [id] }));
    check(now[&id]["title"].is_string(), &format!("what is on now, beside the station ({})", now[&id]));

    // A hand: the listener asks for a code; the captain appoints it with the 24 words.
    let code = l.call(&json!({ "cmd": "hand_request" }))["code"].as_str().unwrap().to_string();
    let ap = s.call(&json!({ "cmd": "hand_appoint", "station": id, "words": words, "code": code, "label": "Second voice" }));
    check(ap["ok"] == true, &format!("the captain appoints a hand ({ap})"));
    let bad = s.call(&json!({ "cmd": "hand_appoint", "station": id, "words": "abandon abandon", "code": code }));
    check(bad["error"].is_string(), "wrong words appoint nobody");
    check(l.call(&json!({ "cmd": "hand_join", "station": id }))["ok"] == true, "the hand joins");
    let mut hand_ok = false;
    for _ in 0..20 {
        std::thread::sleep(Duration::from_secs(3));
        let w = l.call(&json!({ "cmd": "details_set", "station": id, "kinds": ["talk"], "mature": true }));
        if w["ok"] == true {
            let arch = l.archive(&pyrate_core::StationId::from_hex(&id)?).unwrap();
            hand_ok = arch.entries().last().is_some_and(|e| matches!(&e.entry.body, Body::Deputed { .. }));
            if hand_ok { break; }
        }
    }
    check(hand_ok, "the hand writes to the station's log as a deputy");

    // The Lantern carries every station by default.
    let mut carried = 0;
    for _ in 0..20 {
        std::thread::sleep(Duration::from_secs(3));
        carried = files(&work.join("lantern"));
        if carried >= 2 { break; }
    }
    check(carried >= 2, &format!("the Lantern carries the station and its art ({carried} files)"));

    // A listener's own switches: an explicit station is not carried while they are hidden.
    // (The first listener is now a hand, and a hand carries its own station whatever it says.)
    let third = Node::start(cfg("third", false, boot.clone()))?;
    s.call(&json!({ "cmd": "details_set", "station": id, "kinds": ["talk"], "mature": true, "explicit": true }));
    let mut heard = false;
    for _ in 0..40 {
        std::thread::sleep(Duration::from_secs(3));
        if third.call(&json!({ "cmd": "stations", "marooned": true })).as_array().is_some_and(|v| !v.iter().any(|x| x["id"] == id.as_str())) && third.heard_of(&sid) && !third.will_carry(&sid) {
            heard = true;
            break;
        }
    }
    check(heard, "an explicit station stays off a listener's Log, and is not carried, while explicit is hidden");
    third.call(&json!({ "cmd": "pref", "key": "show_explicit", "value": true }));
    check(third.will_carry(&sid), "and is carried once the listener shows explicit stations");

    // A maroon list, shared and taken aboard.
    l.call(&json!({ "cmd": "maroon", "station": id, "value": true }));
    let list = l.call(&json!({ "cmd": "maroon_export" }));
    check(list["text"].as_str().is_some_and(|t| t.contains(&id)), "a maroon list exports");
    let fresh = Node::start(cfg("fresh", false, boot.clone()))?;
    check(fresh.call(&json!({ "cmd": "maroon_import", "text": list["text"] }))["added"] == 1, "and another phone takes it aboard");
    check(fresh.call(&json!({ "cmd": "tune", "station": id }))["error"].is_string(), "a marooned station will not tune");

    // The Lantern's own maroon.txt takes the station off it.
    std::fs::write(work.join("lantern").join("maroon.txt"), format!("{id}  # test\n"))?;
    let mut gone = false;
    for _ in 0..30 {
        std::thread::sleep(Duration::from_secs(3));
        if files(&work.join("lantern")) == 0 { gone = true; break; }
    }
    check(gone, "maroon.txt empties the Lantern of the station, art included");

    // Striking takes the master, and the phone holds only a deputy: the 24 words do it.
    let bad = s.call(&json!({ "cmd": "strike", "station": id, "phrase": "abandon abandon" }));
    check(bad["error"].is_string() && !s.archive(&sid).is_some_and(|a| a.is_struck()), "wrong words strike nothing");
    let r = s.call(&json!({ "cmd": "strike", "station": id, "phrase": words }));
    check(r["ok"] == true && s.archive(&sid).is_some_and(|a| a.is_struck()), &format!("the captain strikes colours with the 24 words ({r})"));
    println!("done");
    Ok(())
}
