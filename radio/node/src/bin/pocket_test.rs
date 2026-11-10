//! pocket_test: a keeper raises a pocket station and invites one listener; an outsider on
//! the same network never sees it. Run with PYRATE_RELAYS set and --bootstrap a Lantern.

use std::time::Duration;

use pyrate_node::{Config, Node};

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let boot: Vec<iroh::EndpointId> = args.get(1).into_iter().filter_map(|b| b.parse().ok()).collect();
    let audio = args.get(2).cloned().unwrap_or_default();
    let mk = |d: &str| {
        let dir = std::env::temp_dir().join(d);
        let _ = std::fs::remove_dir_all(&dir);
        Node::start(Config { dir, quota: 200_000_000, lantern: false, carry_all: false, bootstrap: boot.clone(), public_dns: false, private: false, ballast: 0, release_keys: vec![] })
    };
    let keeper = mk("pocket-keeper")?;
    let member = mk("pocket-member")?;
    let outsider = mk("pocket-outsider")?;
    let r = keeper.call(&serde_json::json!({ "cmd": "raise", "name": "The Back Room", "about": "Invitation only", "pocket": true }));
    let st = r["id"].as_str().unwrap_or_default().to_string();
    println!("raised {st}");
    let job = keeper.call(&serde_json::json!({ "cmd": "add_track", "station": st, "path": audio, "title": "Secret Tone", "by": "", "talk": false }));
    println!("add {job}");
    std::thread::sleep(Duration::from_secs(8));
    let link = keeper.call(&serde_json::json!({ "cmd": "pocket_invite", "station": st }))["link"].as_str().unwrap_or_default().to_string();
    println!("invitation {}…", &link[..90.min(link.len())]);
    println!("join {}", member.call(&serde_json::json!({ "cmd": "pocket_join", "link": link })));
    std::thread::sleep(Duration::from_secs(5));
    println!("tune {}", member.call(&serde_json::json!({ "cmd": "tune", "station": st })));
    std::thread::sleep(Duration::from_secs(70));
    let find = |n: &Node| n.call(&serde_json::json!({ "cmd": "stations" })).as_array().into_iter().flatten().find(|s| s["id"] == st.as_str()).cloned();
    println!("member sees {:?}", find(&member).map(|s| (s["name"].clone(), s["pocket"].clone())));
    println!("member library {}", member.call(&serde_json::json!({ "cmd": "library", "station": st })));
    println!("member now {}", member.call(&serde_json::json!({ "cmd": "now_for", "stations": [st] })));
    println!("outsider sees {:?}", find(&outsider).map(|s| s["name"].clone()));
    println!("outsider library {}", outsider.call(&serde_json::json!({ "cmd": "library", "station": st })));
    Ok(())
}
