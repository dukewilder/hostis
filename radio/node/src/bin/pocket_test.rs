//! pocket_test: a keeper raises a pocket station and invites one listener; an outsider on
//! the same network never sees it, never hears of it, and never carries it, while the member
//! listens and the keeper goes on the air. Run with PYRATE_RELAYS set and a Lantern to start from.
//! pocket_test <lantern id> <audio file>

use std::time::Duration;

use pyrate_node::{Config, Node};

fn check(ok: bool, what: &str) {
    println!("{} {what}", if ok { "PASS" } else { "FAIL" });
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let boot: Vec<iroh::EndpointId> = args.get(1).into_iter().filter_map(|b| b.parse().ok()).collect();
    let audio = args.get(2).cloned().unwrap_or_default();
    // Phones as they ship: private routes and the stem.
    let mk = |d: &str| {
        let dir = std::env::temp_dir().join(d);
        let _ = std::fs::remove_dir_all(&dir);
        Node::start(Config { dir, quota: 200_000_000, lantern: false, carry_all: false, bootstrap: boot.clone(), public_dns: false, private: true, ballast: 0, release_keys: vec![] })
    };
    let keeper = mk("pocket-keeper")?;
    let member = mk("pocket-member")?;
    let outsider = mk("pocket-outsider")?;
    // Every phone here relays hops and stems, as on Wi-Fi.
    for n in [&keeper, &member, &outsider] {
        n.call(&serde_json::json!({ "cmd": "ballast_ok", "value": true, "unmetered": true }));
    }
    let r = keeper.call(&serde_json::json!({ "cmd": "raise", "name": "The Back Room", "about": "Invitation only", "pocket": true }));
    let st = r["id"].as_str().unwrap_or_default().to_string();
    let sid = pyrate_core::StationId::from_hex(&st)?;
    println!("raised {st}");
    let job = keeper.call(&serde_json::json!({ "cmd": "add_track", "station": st, "path": audio, "title": "Secret Tone", "by": "", "talk": false }));
    println!("add {job}");
    std::thread::sleep(Duration::from_secs(8));
    let link = keeper.call(&serde_json::json!({ "cmd": "pocket_invite", "station": st }))["link"].as_str().unwrap_or_default().to_string();
    println!("invitation {}…", &link[..90.min(link.len())]);
    // A browser can come aboard too: the invitation goes to a file and the test waits for it.
    if let Ok(path) = std::env::var("PYRATE_BROWSER_INVITE") {
        keeper.call(&serde_json::json!({ "cmd": "chat_join", "station": st }));
        std::fs::write(&path, &link)?;
        let wait: u64 = std::env::var("PYRATE_BROWSER_WAIT").ok().and_then(|w| w.parse().ok()).unwrap_or(90);
        std::thread::sleep(Duration::from_secs(wait));
        let lines = keeper.call(&serde_json::json!({ "cmd": "chat_read", "station": st }));
        check(lines["lines"].as_array().into_iter().flatten().any(|l| l["text"] == "Ahoy from the browser"), &format!("the keeper hears the browser in the pocket's room ({})", lines["lines"]));
    }
    println!("join {}", member.call(&serde_json::json!({ "cmd": "pocket_join", "link": link })));
    std::thread::sleep(Duration::from_secs(5));
    println!("tune {}", member.call(&serde_json::json!({ "cmd": "tune", "station": st })));
    std::thread::sleep(Duration::from_secs(70));
    let find = |n: &Node| n.call(&serde_json::json!({ "cmd": "stations" })).as_array().into_iter().flatten().find(|s| s["id"] == st.as_str()).cloned();
    let seen = find(&member);
    check(seen.as_ref().is_some_and(|s| s["pocket"] == true), &format!("the member sees the pocket ({:?})", seen.map(|s| s["name"].clone())));
    let lib = member.call(&serde_json::json!({ "cmd": "library", "station": st }));
    check(lib.as_array().is_some_and(|l| !l.is_empty()), &format!("the member holds its library ({lib})"));
    let now = member.call(&serde_json::json!({ "cmd": "now_for", "stations": [st] }));
    check(now[&st]["title"].is_string(), &format!("the member works out what is on ({now})"));
    // The pocket's room: sealed on its own topic, heard by the keeper, never by the outsider.
    for n in [&member, &keeper, &outsider] {
        n.call(&serde_json::json!({ "cmd": "chat_join", "station": st }));
    }
    std::thread::sleep(Duration::from_secs(8));
    println!("say {}", member.call(&serde_json::json!({ "cmd": "chat_send", "station": st, "text": "The back room speaks", "name": "Gull" })));
    let said = |n: &Node| n.call(&serde_json::json!({ "cmd": "chat_read", "station": st }))["lines"].as_array().into_iter().flatten().any(|l| l["text"] == "The back room speaks");
    let mut heard = false;
    for _ in 0..10 {
        std::thread::sleep(Duration::from_secs(2));
        if said(&keeper) { heard = true; break; }
    }
    check(heard, "the keeper hears the member in the pocket's room");
    check(!said(&outsider), "the outsider, joining by the station's id, hears nothing");
    // The keeper goes on the air: on a phone this would go down the stem.
    println!("live {}", keeper.call(&serde_json::json!({ "cmd": "go_live", "station": st, "file": audio })));
    std::thread::sleep(Duration::from_secs(30));
    keeper.call(&serde_json::json!({ "cmd": "end_live" }));
    std::thread::sleep(Duration::from_secs(10));
    check(find(&outsider).is_none(), "the outsider's Log never shows the pocket");
    check(!outsider.heard_of(&sid), "the outsider never hears of it: no card, carrier, token or log");
    check(outsider.call(&serde_json::json!({ "cmd": "library", "station": st }))["error"].is_string(), "and holds none of its library");
    Ok(())
}
