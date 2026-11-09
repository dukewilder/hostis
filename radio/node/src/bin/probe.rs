//! probe --dir <data> --bootstrap <id> [seconds]
//! Joins the Log, lists stations, then for each: syncs its log, works out what is
//! on now, fetches that chunk from a carrier and decodes it. The whole path, no speaker.

use std::{path::PathBuf, time::Duration};

use pyrate_core::{Body, audio::ChunkDecoder, clock::Clock};
use pyrate_node::{Config, Node, now_ms};

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let get = |flag: &str| args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).cloned();
    let dir = PathBuf::from(get("--dir").unwrap_or_else(|| "probe-data".into()));
    let bootstrap = get("--bootstrap").map(|b| b.split(',').filter_map(|s| s.trim().parse().ok()).collect()).unwrap_or_default();
    let wait: u64 = get("--wait").and_then(|w| w.parse().ok()).unwrap_or(45);
    let node = Node::start(Config { dir, quota: 1_000_000_000, lantern: false, bootstrap, public_dns: args.iter().any(|a| a == "--public-dns"), private: args.iter().any(|a| a == "--private"), ballast: 0 })?;
    println!("probe node {}", node.id());
    let t0 = std::time::Instant::now();
    let mut stations = serde_json::Value::Null;
    while t0.elapsed() < Duration::from_secs(wait) {
        std::thread::sleep(Duration::from_secs(2));
        stations = node.call(&serde_json::json!({ "cmd": "stations" }));
        if stations.as_array().is_some_and(|a| !a.is_empty()) {
            break;
        }
    }
    println!("Log after {:.1} s: {}", t0.elapsed().as_secs_f64(), stations);
    for s in stations.as_array().into_iter().flatten() {
        let id = pyrate_core::StationId::from_hex(s["id"].as_str().unwrap())?;
        let inner = node.inner.clone();
        let t = std::time::Instant::now();
        node.rt.block_on(async {
            inner.sync_log(id).await?;
            let archive = node.archive(&id).ok_or_else(|| anyhow::anyhow!("no log"))?;
            let clock = Clock::new(&archive)?;
            let Some(np) = clock.now(now_ms()) else {
                println!("{}: dead air", s["name"]);
                return anyhow::Ok(());
            };
            let e = archive.entries().iter().find(|e| e.hash().ok() == Some(np.track)).unwrap();
            let Body::Track { title, chunks, channels, .. } = e.entry.body() else { unreachable!() };
            let bytes = inner.chunk(&id, &chunks[np.chunk].hash).await?;
            let pcm = ChunkDecoder::new(*channels)?.decode(&bytes)?;
            println!(
                "{} [{}]: {} entries; on now \"{}\" at {:.1} s; chunk {} fetched and decoded ({} samples) in {:.2} s",
                s["name"].as_str().unwrap_or("?"), id.call_sign(), archive.len(), title,
                np.offset_ms as f64 / 1000.0, np.chunk, pcm.len(), t.elapsed().as_secs_f64()
            );
            anyhow::Ok(())
        })?;
    }
    Ok(())
}
