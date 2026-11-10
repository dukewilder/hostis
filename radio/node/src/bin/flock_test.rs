//! flock_test: two nodes on one machine; one hosts a flock, the other joins by its link.
//! Prints how far apart their clocks are and the round trip. Run on a private network with
//! PYRATE_RELAYS set and --bootstrap a Lantern there.

use std::time::Duration;

use pyrate_node::{Config, Node};

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let boot: Vec<iroh::EndpointId> = args.get(1).into_iter().filter_map(|b| b.parse().ok()).collect();
    let mk = |d: &str| Node::start(Config { dir: std::env::temp_dir().join(d), quota: 100_000_000, lantern: false, carry_all: false, bootstrap: boot.clone(), public_dns: false, private: false, ballast: 0, release_keys: vec![] });
    let host = mk("flock-host")?;
    let guest = mk("flock-guest")?;
    std::thread::sleep(Duration::from_secs(4));
    let link = host.call(&serde_json::json!({ "cmd": "flock_host" }))["link"].as_str().unwrap_or_default().to_string();
    println!("link {link}");
    println!("join {}", guest.call(&serde_json::json!({ "cmd": "flock_join", "link": link })));
    for _ in 0..6 {
        std::thread::sleep(Duration::from_secs(5));
        println!("guest {}", guest.call(&serde_json::json!({ "cmd": "flock_status" })));
    }
    guest.call(&serde_json::json!({ "cmd": "flock_trim", "ms": 120 }));
    println!("trimmed {}", guest.call(&serde_json::json!({ "cmd": "flock_status" })));
    host.call(&serde_json::json!({ "cmd": "flock_leave" }));
    std::thread::sleep(Duration::from_secs(25));
    println!("after the host ends it {}", guest.call(&serde_json::json!({ "cmd": "flock_status" })));
    Ok(())
}
