//! Pyrate Radio transport spike.
//!
//! `pyrate-spike raise` holds a station open and prints its id.
//! `pyrate-spike tune <id> [seconds]` connects and receives a 64 kbit/s stream,
//! reporting path (direct or relay), round trip, gaps and late frames.
//!
//! No rights reserved.

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow};
use iroh::{
    Endpoint, EndpointId,
    endpoint::{Connection, presets},
    protocol::{AcceptError, ProtocolHandler, Router},
    tls::CaTlsConfig,
};

const ALPN: &[u8] = b"pyrate/spike/0";
/// One Opus-sized frame every 125 ms is 8 KB/s, 64 kbit/s.
const FRAME_BYTES: usize = 1000;
const FRAME_EVERY: Duration = Duration::from_millis(125);
/// A frame later than this would have dropped out of a 15 s live buffer.
const LATE: Duration = Duration::from_secs(15);

fn now_us() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_micros() as u64
}

async fn bind() -> Result<Endpoint> {
    let mut b = Endpoint::builder(presets::N0).alpns(vec![ALPN.to_vec()]);
    if std::env::var_os("PYRATE_SYSTEM_ROOTS").is_some() {
        b = b.ca_roots_config(CaTlsConfig::system());
    }
    if std::env::var_os("HTTPS_PROXY").is_some() || std::env::var_os("https_proxy").is_some() {
        b = b.proxy_from_env();
    }
    let ep = b.bind().await.map_err(|e| anyhow!("bind: {e}"))?;
    Ok(ep)
}

fn path_line(conn: &Connection) -> String {
    match conn.paths().iter().find(|p| p.is_selected()) {
        Some(p) => {
            let kind = if p.is_relay() { "relay" } else { "direct" };
            let rtt = format!("{} ms", p.rtt().as_millis());
            format!("{kind} via {:?}, rtt {rtt}", p.remote_addr())
        }
        None => "no path selected".into(),
    }
}

#[derive(Debug, Clone)]
struct Station;

impl ProtocolHandler for Station {
    async fn accept(&self, conn: Connection) -> Result<(), AcceptError> {
        println!("aboard: {}", conn.remote_id().fmt_short());
        let mut send = conn.open_uni().await?;
        let mut seq: u64 = 0;
        let mut tick = tokio::time::interval(FRAME_EVERY);
        let mut frame = vec![0u8; FRAME_BYTES];
        loop {
            tick.tick().await;
            frame[..8].copy_from_slice(&seq.to_be_bytes());
            frame[8..16].copy_from_slice(&now_us().to_be_bytes());
            if send.write_all(&frame).await.is_err() {
                break;
            }
            seq += 1;
        }
        println!("ashore: {} after {seq} frames", conn.remote_id().fmt_short());
        Ok(())
    }
}

async fn raise() -> Result<()> {
    let ep = bind().await?;
    let router = Router::builder(ep).accept(ALPN, Station).spawn();
    router.endpoint().online().await;
    println!("ON AIR");
    println!("station id: {}", router.endpoint().id());
    println!("tune with:  pyrate-spike tune {}", router.endpoint().id());
    tokio::signal::ctrl_c().await?;
    router.shutdown().await.ok();
    Ok(())
}

async fn tune(id: EndpointId, secs: u64) -> Result<()> {
    let ep = bind().await?;
    let t0 = Instant::now();
    let conn = ep.connect(id, ALPN).await.map_err(|e| anyhow!("connect: {e}"))?;
    let connect_ms = t0.elapsed().as_millis();
    let mut recv = conn.accept_uni().await.context("stream")?;
    let mut frame = vec![0u8; FRAME_BYTES];
    recv.read_exact(&mut frame).await.context("first frame")?;
    let first_sound_ms = t0.elapsed().as_millis();
    println!("connected in {connect_ms} ms, first frame at {first_sound_ms} ms");
    println!("path: {}", path_line(&conn));

    // Clocks on two machines disagree, so delay is measured against the best seen.
    let mut min_delay = i64::MAX;
    let (mut frames, mut gaps, mut late, mut max_jitter) = (1u64, 0u64, 0u64, 0i64);
    let mut last_seq = u64::from_be_bytes(frame[..8].try_into()?);
    let mut last_report = Instant::now();
    let mut relay_secs = 0u64;
    let mut direct_secs = 0u64;
    let end = Instant::now() + Duration::from_secs(secs);
    while Instant::now() < end {
        recv.read_exact(&mut frame).await.context("frame")?;
        frames += 1;
        let seq = u64::from_be_bytes(frame[..8].try_into()?);
        let sent = u64::from_be_bytes(frame[8..16].try_into()?) as i64;
        if seq != last_seq + 1 {
            gaps += 1;
        }
        last_seq = seq;
        let delay = now_us() as i64 - sent;
        min_delay = min_delay.min(delay);
        let jitter = delay - min_delay;
        max_jitter = max_jitter.max(jitter);
        if jitter > LATE.as_micros() as i64 {
            late += 1;
        }
        if last_report.elapsed() >= Duration::from_secs(5) {
            last_report = Instant::now();
            let line = path_line(&conn);
            if line.starts_with("relay") { relay_secs += 5 } else { direct_secs += 5 }
            println!(
                "{:>4}s  {frames} frames  gaps {gaps}  late {late}  worst jitter {} ms  {line}",
                t0.elapsed().as_secs(),
                max_jitter / 1000
            );
        }
    }
    let expected = secs * 1000 / FRAME_EVERY.as_millis() as u64;
    println!("---");
    println!("connect          {connect_ms} ms");
    println!("first frame      {first_sound_ms} ms");
    println!("frames           {frames} of about {expected}");
    println!("gaps             {gaps}");
    println!("late past 15 s   {late}");
    println!("worst jitter     {} ms", max_jitter / 1000);
    println!("time on direct   {direct_secs} s");
    println!("time on relay    {relay_secs} s");
    println!("verdict          {}", if gaps == 0 && late == 0 { "HOLDS" } else { "FAILS" });
    conn.close(0u32.into(), b"strike");
    ep.close().await;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("raise") => raise().await,
        Some("tune") => {
            let id: EndpointId = args.get(2).context("station id")?.parse().map_err(|e| anyhow!("id: {e}"))?;
            let secs = args.get(3).map(|s| s.parse()).transpose()?.unwrap_or(120);
            tune(id, secs).await
        }
        _ => {
            eprintln!("usage: pyrate-spike raise | pyrate-spike tune <station id> [seconds]");
            Ok(())
        }
    }
}
