//! crew_deck_test <bootstrap id> <station id>
//! A phone's deck reached through its way in on the test network: the phone (on Wi-Fi) plays the
//! station and opens a deck of it; the way in asks the deck for its log through the lead, and the
//! connection it holds to the deck must run over the relay alone, never a direct path. Turning
//! "Carry the radio while you listen" off closes the deck. Run with PYRATE_RELAYS set to the test
//! relay, PYRATE_LAN_ANY=1 and PYRATE_NULL_AUDIO=1, after net_up.sh.
use std::{path::PathBuf, time::{Duration, Instant}};

use pyrate_core::StationId;
use pyrate_node::{Config, Node};
use serde_json::json;

fn check(ok: bool, what: &str) -> bool {
    println!("{} {what}", if ok { "PASS" } else { "FAIL" });
    ok
}

fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let boot: iroh::EndpointId = a[1].parse()?;
    let st = StationId::from_hex(&a[2])?;
    let cfg = |d: &str, private: bool| {
        let _ = std::fs::remove_dir_all(d);
        Config { dir: PathBuf::from(d), quota: 1 << 30, lantern: false, bootstrap: vec![boot], public_dns: false, private, ballast: 0, carry_all: false, release_keys: vec![] }
    };
    let way = Node::start(cfg("/tmp/cd-way", false))?;
    let phone = Node::start(cfg("/tmp/cd-phone", true))?;
    // A phone on Wi-Fi, not charging: it may carry others' traffic, and keeps no ballast.
    phone.call(&json!({ "cmd": "ballast_ok", "value": false, "unmetered": true }));
    let mut ok = true;
    let t = Instant::now();
    let sees = |n: &Node| n.call(&json!({ "cmd": "stations" })).as_array().is_some_and(|l| l.iter().any(|x| x["id"] == a[2].as_str()));
    while !(sees(&phone) && sees(&way)) && t.elapsed() < Duration::from_secs(120) {
        std::thread::sleep(Duration::from_millis(500));
    }
    phone.call(&json!({ "cmd": "tune", "station": a[2] }));
    let playing = |n: &Node| n.call(&json!({ "cmd": "status" }))["now"]["state"] == "playing";
    let t = Instant::now();
    while !playing(&phone) && t.elapsed() < Duration::from_secs(90) {
        std::thread::sleep(Duration::from_millis(500));
    }
    ok &= check(playing(&phone), &format!("the phone plays the station ({:.0} s)", t.elapsed().as_secs_f64()));
    let t = Instant::now();
    while phone.deck_now().is_none_or(|d| d.1.is_none()) && t.elapsed() < Duration::from_secs(60) {
        std::thread::sleep(Duration::from_millis(500));
    }
    let Some((deck, _)) = phone.deck_now() else {
        check(false, "the phone opens a deck of what it plays");
        // What stood in the way: the phone's relay, and whether an endpoint with no direct
        // paths binds here at all.
        let relay = phone.addr().relay_urls().next().cloned();
        println!("INFO the phone's relay: {relay:?}; status {}", phone.call(&json!({ "cmd": "status" }))["now"]);
        if let Some(r) = relay {
            let rt = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
            let b = rt.block_on(
                iroh::Endpoint::builder(iroh::endpoint::presets::Minimal)
                    .crypto_provider(std::sync::Arc::new(rustls::crypto::aws_lc_rs::default_provider()))
                    .relay_mode(iroh::RelayMode::Custom(iroh::RelayMap::from_iter([r])))
                    .clear_ip_transports()
                    .bind(),
            );
            println!("INFO a relay-only endpoint binds: {:?}", b.as_ref().map(|e| e.id()).map_err(|e| e.to_string()));
        }
        std::process::exit(1);
    };
    ok &= check(deck != *phone.id().as_bytes(), "the deck has a key of its own, never the phone's node id");
    // The test's way in: this node, so the test can see what a way in sees.
    ok &= check(phone.deck_set_way_in(*way.id().as_bytes()), "the deck names the test's radio as its way in");
    let t = Instant::now();
    let mut got = None;
    while got.is_none() && t.elapsed() < Duration::from_secs(90) {
        std::thread::sleep(Duration::from_secs(3));
        got = way.deck_probe(st);
    }
    let (entries, paths) = got.unwrap_or((0, vec![]));
    ok &= check(entries > 0, &format!("the way in reads the deck's log through the lead ({entries} entries, {:.0} s)", t.elapsed().as_secs_f64()));
    ok &= check(!paths.is_empty() && paths.iter().all(|r| *r), &format!("the way in's connection to the deck runs over the relay alone ({} paths, {} direct)", paths.len(), paths.iter().filter(|r| !**r).count()));
    // Against that: the same radio's connection to the Lantern on this machine runs direct.
    let lantern = way.paths_to(&boot);
    println!("INFO the way in's connection to the Lantern: {} paths, {} direct", lantern.len(), lantern.iter().filter(|r| !**r).count());
    phone.call(&json!({ "cmd": "crew", "on": false }));
    let t = Instant::now();
    while phone.deck_now().is_some() && t.elapsed() < Duration::from_secs(20) {
        std::thread::sleep(Duration::from_millis(500));
    }
    ok &= check(phone.deck_now().is_none(), "with the switch off the deck closes");
    std::process::exit(if ok { 0 } else { 1 })
}
