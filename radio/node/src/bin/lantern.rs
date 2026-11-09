//! Lantern: a headless node that runs all day on a spare machine.
//! It carries every station it hears of, relays for anyone, and can raise
//! stations of its own from folders of audio.
//!
//! lantern --dir <data> [--quota-gb 20] [--bootstrap <id>,<id>] [--public-dns]
//!         [--seed "Name|About|folder|music"]...

use std::{collections::HashSet, path::PathBuf, time::Duration};

use pyrate_core::{Body, Profile};
use pyrate_node::{Config, Node, now_ms};

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let get = |flag: &str| args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).cloned();
    let dir = PathBuf::from(get("--dir").unwrap_or_else(|| "lantern-data".into()));
    let quota = get("--quota-gb").and_then(|q| q.parse::<u64>().ok()).unwrap_or(20) * 1_000_000_000;
    let bootstrap = get("--bootstrap")
        .map(|b| b.split(',').filter_map(|s| s.trim().parse().ok()).collect())
        .unwrap_or_default();
    let seeds: Vec<String> = args.windows(2).filter(|w| w[0] == "--seed").map(|w| w[1].clone()).collect();
    std::fs::create_dir_all(&dir)?;

    let node = Node::start(Config { dir: dir.clone(), quota, lantern: true, bootstrap, public_dns: args.iter().any(|a| a == "--public-dns") })?;
    println!("LANTERN LIT");
    // --http 127.0.0.1:8080 serves the web listener; --relay 127.0.0.1:3340 runs a relay. Both
    // bind to loopback by default: a tunnel puts them on the public internet, not this machine.
    if let Some(addr) = get("--http").and_then(|a| a.parse().ok()) {
        let inner = node.inner.clone();
        node.rt.spawn(async move {
            if let Err(e) = pyrate_node::web::serve(inner, addr).await {
                println!("web listener: {e}");
            }
        });
    }
    if let Some(addr) = get("--relay").and_then(|a| a.parse::<std::net::SocketAddr>().ok()) {
        node.rt.spawn(async move {
            let mut cfg = iroh_relay::server::ServerConfig::default();
            cfg.relay = Some(iroh_relay::server::RelayConfig::new(addr));
            match iroh_relay::server::Server::spawn(cfg).await {
                Ok(server) => {
                    println!("relay on http://{addr}");
                    std::mem::forget(server);
                }
                Err(e) => println!("relay: {e}"),
            }
        });
    }
    println!("node id: {}", node.id());

    // --withdraw "words": takes matching recordings off our own stations' clocks.
    let withdraw: Vec<String> = args.windows(2).filter(|w| w[0] == "--withdraw").map(|w| w[1].to_lowercase()).collect();
    if !withdraw.is_empty() {
        let mine: Vec<String> = node.call(&serde_json::json!({ "cmd": "status" }))
            .get("mine").and_then(|m| m.get("id")).and_then(|v| v.as_str()).map(String::from).into_iter().collect();
        let _ = mine;
        for name in seeds.iter().filter_map(|s| s.split('|').next()) {
            let Some(id) = node.inner_store_find(name) else { continue };
            let lib = node.call(&serde_json::json!({ "cmd": "library", "station": id.to_hex() }));
            for t in lib.as_array().into_iter().flatten() {
                let title = format!("{} {}", t["by"].as_str().unwrap_or(""), t["title"].as_str().unwrap_or("")).to_lowercase();
                if withdraw.iter().any(|w| title.contains(w)) {
                    node.call(&serde_json::json!({ "cmd": "withdraw", "station": id.to_hex(), "track": t["hash"] }));
                    println!("withdrew {title}");
                }
            }
        }
    }
    seed_all(&node, &seeds)?;
    for line in node.call(&serde_json::json!({ "cmd": "status" }))["mine"].as_object().into_iter() {
        let _ = line;
    }
    let mut n = 0u64;
    loop {
        std::thread::sleep(Duration::from_secs(60));
        n += 1;
        // New files in a seed folder go on the clock within ten minutes.
        if n % 10 == 0 {
            if let Err(e) = seed_all(&node, &seeds) {
                println!("seeding: {e}");
            }
        }
        let st = node.call(&serde_json::json!({ "cmd": "stations" }));
        let n = st.as_array().map(|a| a.len()).unwrap_or(0);
        let line: Vec<String> = st.as_array().into_iter().flatten().take(8)
            .map(|s| format!("{} ({} aboard)", s["name"].as_str().unwrap_or("?"), s["aboard"])).collect();
        println!("{} · {n} stations in the Log · {}", now_ms() / 1000, line.join(", "));
        let _ = Body::Strike; // keep the import honest across features
    }
}

fn seed_all(node: &Node, seeds: &[String]) -> anyhow::Result<()> {
    for seed in seeds {
        let parts: Vec<&str> = seed.split('|').collect();
        let [name, about, folder, kind] = parts[..] else {
            eprintln!("seed must be Name|About|folder|music or talk");
            continue;
        };
        let profile = if kind == "talk" { Profile::Talk } else { Profile::Music };
        let existing = {
            let s = node.inner_store_find(name);
            s
        };
        let id = match existing {
            Some(id) => id,
            None => node.raise(name, about, None)?,
        };

        let have: HashSet<String> = node.titles(&id).into_iter().collect();
        let mut files: Vec<PathBuf> = std::fs::read_dir(folder)?
            .filter_map(|f| f.ok().map(|f| f.path()))
            .filter(|p| p.extension().is_some_and(|e| ["mp3", "ogg", "flac", "wav", "m4a", "opus", "oga"].contains(&e.to_string_lossy().to_lowercase().as_str())))
            .collect();
        files.sort();
        for f in files {
            let stem = f.file_stem().unwrap().to_string_lossy().to_string();
            let (by, title) = match stem.split_once(" - ") {
                Some((b, t)) => (b.trim().to_string(), t.trim().to_string()),
                None => (String::new(), stem.clone()),
            };
            if have.contains(&title) {
                continue;
            }
            let source = std::fs::read_to_string(f.with_extension("source.txt")).unwrap_or_default();
            match node.add_track(id, &f, &title, &by, source.trim(), profile) {
                Ok(_) => println!("  set {title}"),
                Err(e) => println!("  could not set {title}: {e}"),
            }
        }
    }
    Ok(())
}
