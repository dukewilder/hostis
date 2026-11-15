//! privacy_test <dir> <wav>
//! Privacy fails closed. A phone that cannot find two hops sends no request naming a station
//! straight to a carrier: no log, no chunk. With "ask carriers directly" turned on, it plays.
//! A stranger who dials a keeper's phone or a listener's straight learns nothing of what it
//! keeps or played. Run with PYRATE_LAN_ANY=1: the nodes find each other on the local network.
use std::{path::PathBuf, sync::Arc, time::Duration};

use iroh::{Endpoint, EndpointAddr, endpoint::presets};
use pyrate_core::{Body, StationId};
use pyrate_node::{
    Config, Node,
    net::{ALPN, Request, Response},
};
use serde_json::json;

/// One request from a fresh identity no one has met, dialled straight: the answer, if any.
fn probe(addr: EndpointAddr, req: Request) -> Option<Response> {
    let rt = tokio::runtime::Runtime::new().ok()?;
    rt.block_on(async {
        let ep = Endpoint::builder(presets::Minimal)
            .crypto_provider(Arc::new(rustls::crypto::aws_lc_rs::default_provider()))
            .relay_mode(iroh::RelayMode::Disabled)
            .bind()
            .await
            .ok()?;
        let c = tokio::time::timeout(Duration::from_secs(10), ep.connect(addr, ALPN)).await.ok()?.ok()?;
        let (mut s, mut r) = c.open_bi().await.ok()?;
        s.write_all(&postcard::to_stdvec(&req).ok()?).await.ok()?;
        s.finish().ok()?;
        let b = tokio::time::timeout(Duration::from_secs(10), r.read_to_end(64 << 20)).await.ok()?.ok()?;
        postcard::from_bytes(&b).ok()
    })
}

/// Whether a probe's answer gave anything away.
fn told(r: &Option<Response>) -> bool {
    match r {
        Some(Response::Entries(e)) => !e.is_empty(),
        Some(Response::Chunk(c)) => c.is_some(),
        Some(Response::Sealed(c)) => c.is_some(),
        Some(Response::Chatter(c)) => !c.is_empty(),
        _ => false,
    }
}

/// The first track chunk a node holds of a station.
fn first_chunk(n: &Node, st: &StationId) -> Option<[u8; 32]> {
    n.archive(st)?.entries().iter().find_map(|e| match e.entry.body() {
        Body::Track { chunks, .. } => chunks.first().map(|c| c.hash),
        _ => None,
    })
}

fn check(c: bool, what: &str) {
    println!("{} {what}", if c { "PASS" } else { "FAIL" });
}

fn playing(n: &Node) -> bool {
    n.call(&json!({ "cmd": "status" }))["now"]["state"] == "playing"
}

fn main() -> anyhow::Result<()> {
    // A phone dates its entries to the quarter hour and its cargo waits for it; the test network
    // waits twenty seconds instead.
    if std::env::var_os("PYRATE_QUARTER_MS").is_none() {
        // SAFETY: set before any node starts, while this process has one thread.
        unsafe { std::env::set_var("PYRATE_QUARTER_MS", "20000") };
    }
    let a: Vec<String> = std::env::args().collect();
    let work = PathBuf::from(&a[1]);
    let cfg = |d: &str, private: bool| Config { dir: work.join(d), quota: 1 << 30, lantern: false, bootstrap: vec![], public_dns: false, private, ballast: 0, carry_all: false, release_keys: vec![] };
    let k = Node::start(cfg("keeper", false))?;
    k.call(&json!({ "cmd": "lan", "value": true }));
    let raised = k.call(&json!({ "cmd": "raise", "name": "Watched", "about": "a test" }));
    let id = raised["id"].as_str().unwrap().to_string();
    let words = raised["words"].clone();
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
    if !ok {
        eprintln!("status: {}", l.call(&json!({ "cmd": "status" })));
    }
    check(ok, "with the switch on, it plays");
    // A Lantern makes two hops possible: a new phone, switch off, plays through them. The Lantern
    // is the phone's door, as HOSTIS's are for every copy: a radio met minutes ago is never a
    // first hop (the crew's rule against a crowd of new radios), and a door is one at once.
    let r = Node::start(Config { carry_all: true, lantern: true, ..cfg("relay", false) })?;
    r.call(&json!({ "cmd": "lan", "value": true }));
    // A guard is a radio known under one id for a week: the keeper's phone has known the other
    // keeper's that long, so it has a first hop, and the Lantern, the second node, takes the stem.
    b.age_peers(&[*k.id().as_bytes()], 8 * 86_400_000);
    let l2 = Node::start(Config { bootstrap: vec![r.addr().id], ..cfg("listener2", true) })?;
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

    // A stranger dials the keeper's phone and asks for its own station: a chunk in the clear,
    // the same chunk sealed, and the log. Each answer is the one a node with nothing gives.
    let own = first_chunk(&b, &bid);
    check(own.is_some(), "the keeper's phone holds its own station's sound");
    if let Some(h) = own {
        check(!told(&probe(b.addr(), Request::Chunk { hash: h })), "a keeper's phone hands a stranger no chunk of its own station");
        check(!told(&probe(b.addr(), Request::SealedChunk { name: pyrate_core::seal::name(&h) })), "nor the same chunk sealed");
    }
    check(!told(&probe(b.addr(), Request::Log { station: bid, from: 0 })), "nor its own station's log");
    // The listener played Watched: a stranger asks it for that log, and for a chunk of it that
    // no ballast share put there.
    let wid = StationId::from_hex(&id)?;
    let played = first_chunk(&l, &wid);
    check(l.archive(&wid).is_some(), "the listener holds the log of the station it played");
    check(!told(&probe(l.addr(), Request::Log { station: wid, from: 0 })), "a listener's phone hands a stranger no log of a station it played");
    if let Some(h) = played {
        check(!told(&probe(l.addr(), Request::SealedChunk { name: pyrate_core::seal::name(&h) })), "nor a sealed chunk outside its share");
        check(!told(&probe(l.addr(), Request::Chunk { hash: h })), "nor that chunk in the clear");
    }
    check(!told(&probe(l.addr(), Request::Chatter { station: wid })), "nor the lines of a room it sits in");
    // The same probe of a Lantern, which answers for what it carries, shows the probe works.
    check(told(&probe(r.addr(), Request::Log { station: wid, from: 0 })), "a Lantern answers the same probe for a station it carries");

    // A door names the address it was given. A radio in the open opens one at a test address;
    // a radio running dark (through Tor) opens none and names nothing, even when told to.
    let door_at = |n: &Node, port: u16| {
        n.call(&json!({ "cmd": "door", "on": true, "app": true, "addr": format!("203.0.113.7:{port}") }));
        for _ in 0..20 {
            std::thread::sleep(Duration::from_millis(250));
            if n.call(&json!({ "cmd": "door" }))["open"] == true {
                return true;
            }
        }
        false
    };
    let open = Node::start(cfg("open", true))?;
    check(door_at(&open, 41001), "a radio in the open opens a door at the address it is given");
    // SAFETY: set before the dark node starts; every other node here has already read it.
    unsafe { std::env::set_var("PYRATE_DARK", "1") };
    let dark = Node::start(cfg("dark", true))?;
    check(!door_at(&dark, 41002), "a radio running dark is never a door");
    let own = dark.call(&json!({ "cmd": "doors" }))["doors"].as_array().map(|d| d.len()).unwrap_or(0);
    check(own == 0, "a radio running dark names no door of its own");
    let why = dark.call(&json!({ "cmd": "door" }))["why"].clone();
    check(why == "This radio runs through Tor, so it is never a door.", &format!("and its door says why it is shut ({why})"));
    check(l.call(&json!({ "cmd": "door" }))["why"] == "The door is switched off.", "a radio never asked to be a door says its door is switched off");

    // number0's relays and address server stand in only when the owner turns them on.
    check(l.call(&json!({ "cmd": "number0" }))["value"] == false, "number0 stands in for nobody by default");
    check(l.call(&json!({ "cmd": "number0", "value": true }))["value"] == true, "the owner can turn number0 on");
    check(l.call(&json!({ "cmd": "number0", "value": false }))["value"] == false, "and off again");

    // An update check that only peeks fetches nothing and says whether a newer release is out.
    check(l.call(&json!({ "cmd": "update", "peek": true })) == json!({ "newer": false }), "a peek at the releases says there is nothing newer, downloading nothing");

    // A room's voice is new each week unless the owner keeps it there.
    let weekly = l.call(&json!({ "cmd": "chat_voice", "station": id }));
    let kept = l.call(&json!({ "cmd": "chat_voice", "station": id, "keep": true }));
    check(weekly["kept"] == false && kept["kept"] == true && weekly["voice"] != kept["voice"], &format!("a room's voice is the week's own until the owner keeps one ({} then {})", weekly["voice"], kept["voice"]));
    l.call(&json!({ "cmd": "chat_voice", "station": id, "keep": false }));

    // A co-host's name, typed "for your own record", stays on this device: the log signs none.
    let watched = StationId::from_hex(&id)?;
    let hand = pyrate_core::StationKey::generate()?.id();
    let r = k.call(&json!({ "cmd": "hand_appoint", "station": id, "words": words, "code": hand.to_hex(), "label": "Jane Doe" }));
    let signed = k.archive(&watched).and_then(|a| a.entries().iter().find_map(|e| match e.entry.body() {
        Body::Deputy { key, label } if *key == hand.0 => Some(label.clone()),
        _ => None,
    }));
    check(signed.as_deref() == Some(""), &format!("a co-host goes into the log with no name signed ({r} {signed:?})"));
    let hands = k.call(&json!({ "cmd": "programme", "station": id }))["hands"].clone();
    check(hands.as_array().is_some_and(|h| h.iter().any(|x| x["label"] == "Jane Doe")), &format!("and the keeper's device still shows the name it was given ({hands})"));

    // An upload with the disguise on goes the talk way, whatever it was marked; a cover beside a
    // recording in its folder never goes out with it.
    let folder = work.join("upload");
    std::fs::create_dir_all(&folder)?;
    std::fs::copy(&a[2], folder.join("song.wav"))?;
    std::fs::copy(concat!(env!("CARGO_MANIFEST_DIR"), "/../stations/shanties/station.jpg"), folder.join("cover.jpg"))?;
    k.call(&json!({ "cmd": "add_track", "station": id, "path": folder.join("song.wav"), "title": "Disguised", "by": "Test", "disguise": true }));
    let mut track = None;
    for _ in 0..90 {
        std::thread::sleep(Duration::from_secs(1));
        track = k.archive(&watched).and_then(|a| {
            a.entries().iter().find_map(|e| match e.entry.body() {
                Body::Track { title, channels, bitrate, .. } if title == "Disguised" => Some((e.hash().ok()?, *channels, *bitrate)),
                _ => None,
            })
        });
        if track.is_some() {
            break;
        }
    }
    let talk = pyrate_core::audio::Profile::Talk;
    check(track.is_some_and(|t| t.1 == talk.channels() && t.2 == talk.bitrate()), &format!("a disguised upload goes the talk way, hum filter and shift ({track:?})"));
    std::thread::sleep(Duration::from_secs(2));
    let art = track.and_then(|t| k.archive(&watched).and_then(|a| a.track_art(&t.0)));
    check(track.is_some() && art.is_none(), "and the cover beside it in its folder stays off the air");
    Ok(())
}
