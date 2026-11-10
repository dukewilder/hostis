//! Flock: phones in one room playing as one. One phone hosts and shows a code; the others
//! scan it, set their clocks to the host's to within a few milliseconds over direct pings,
//! and follow the host's station. Each phone may also be trimmed by hand, for a speaker that
//! runs late. Only those holding the code are answered, and nothing is kept.
//!
//! No rights reserved.

use std::sync::Arc;
use std::sync::atomic::Ordering::Relaxed;

use anyhow::{Context, Result, anyhow};
use iroh::EndpointId;
use pyrate_core::StationId;
use serde_json::{Value, json};

use crate::net::{Request, Response};
use crate::{Inner, hex, now_ms};

impl Inner {
    /// The hour as this phone plays it: the shared clock, set to the flock's host, trimmed.
    pub(crate) fn play_now(&self) -> u64 {
        (now_ms() as i64 + self.flock_offset.load(Relaxed) + self.flock_trim.load(Relaxed)).max(0) as u64
    }

    /// The host's answer to a phone holding the right code: its clock and its station.
    pub(crate) fn flock_answer(&self, code: &[u8; 8]) -> Response {
        if self.flock_host.lock().unwrap().as_ref() != Some(code) {
            return Response::Flock { now_ms: 0, station: None };
        }
        let station = self.tuned.lock().unwrap().as_ref().map(|t| t.station);
        Response::Flock { now_ms: now_ms() + self.flock_offset.load(Relaxed).max(0) as u64, station }
    }

    /// Eight pings to the host; the one that came back fastest says how far apart the clocks are.
    async fn flock_sync(self: &Arc<Self>, host: EndpointId, code: [u8; 8]) -> Result<Option<StationId>> {
        let mut best: Option<(u64, i64, Option<StationId>)> = None;
        for _ in 0..8 {
            let t0 = now_ms();
            let Ok(Response::Flock { now_ms: h, station }) = self.ask(host, &Request::Flock { code }).await else { continue };
            let t1 = now_ms();
            if h == 0 {
                anyhow::bail!("the host has stopped the flock");
            }
            let rtt = t1.saturating_sub(t0);
            let off = h as i64 + rtt as i64 / 2 - t1 as i64;
            if best.as_ref().is_none_or(|b| rtt < b.0) {
                best = Some((rtt, off, station));
            }
        }
        let (rtt, off, station) = best.context("the host did not answer")?;
        // Smoothly, so a late ping cannot make the music jump.
        let old = self.flock_offset.load(Relaxed);
        let next = if old == 0 { off } else { old + (off - old) / 4 };
        self.flock_offset.store(next, Relaxed);
        self.flock_rtt.store(rtt, Relaxed);
        Ok(station)
    }

    pub(crate) fn flock_call(self: &Arc<Self>, cmd: &str, req: &Value) -> Result<Value> {
        match cmd {
            // Hosting: a fresh code, and the link others scan to join.
            "flock_host" => {
                let mut code = [0u8; 8];
                rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut code);
                *self.flock_host.lock().unwrap() = Some(code);
                *self.flock_with.lock().unwrap() = None;
                self.flock_offset.store(0, Relaxed);
                Ok(json!({ "link": self.flock_link(&code) }))
            }
            "flock_join" => {
                let link = req["link"].as_str().context("link")?;
                let tail = link.split("flock=").nth(1).context("not a flock link")?;
                let (who, code) = tail.split_once('~').context("not a flock link")?;
                let host: EndpointId = who.split('@').next().unwrap_or("").parse().map_err(|_| anyhow!("not a flock link"))?;
                let code: [u8; 8] = unhex_n(code)?;
                *self.flock_host.lock().unwrap() = None;
                *self.flock_with.lock().unwrap() = Some((host, code));
                let me = self.clone();
                // Keep in step: a fresh look every twenty seconds, and follow the host's station.
                self.handle.spawn(async move {
                    while me.flock_with.lock().unwrap().is_some_and(|(h, c)| h == host && c == code) {
                        match me.flock_sync(host, code).await {
                            Ok(Some(st)) if me.tuned.lock().unwrap().as_ref().map(|t| t.station) != Some(st) => {
                                let _ = me.tune_to(st);
                            }
                            Err(e) if e.to_string().contains("stopped") => {
                                *me.flock_with.lock().unwrap() = None;
                                me.flock_offset.store(0, Relaxed);
                                break;
                            }
                            _ => {}
                        }
                        tokio::time::sleep(std::time::Duration::from_secs(20)).await;
                    }
                });
                Ok(json!({ "ok": true }))
            }
            "flock_leave" => {
                *self.flock_host.lock().unwrap() = None;
                *self.flock_with.lock().unwrap() = None;
                self.flock_offset.store(0, Relaxed);
                Ok(json!({ "ok": true }))
            }
            "flock_trim" => {
                self.flock_trim.store(req["ms"].as_i64().unwrap_or(0).clamp(-1000, 1000), Relaxed);
                Ok(json!({ "ok": true }))
            }
            "flock_status" => Ok(json!({
                "hosting": self.flock_host.lock().unwrap().is_some(),
                // The code stays readable for as long as the flock is hosted, so closing the
                // sheet never strands the room without it.
                "link": self.flock_host.lock().unwrap().map(|c| self.flock_link(&c)),
                "joined": self.flock_with.lock().unwrap().is_some(),
                "offset_ms": self.flock_offset.load(Relaxed),
                "rtt_ms": self.flock_rtt.load(Relaxed),
                "trim_ms": self.flock_trim.load(Relaxed),
            })),
            _ => Err(anyhow!("unknown command {cmd}")),
        }
    }
}

impl Inner {
    fn flock_link(&self, code: &[u8; 8]) -> String {
        let relay = self.endpoint.addr().relay_urls().next().map(|u| u.to_string()).unwrap_or_default();
        format!("{}#flock={}@{}~{}", pyrate_core::open_player(), self.endpoint.id(), relay, hex(code))
    }
}

fn unhex_n<const N: usize>(s: &str) -> Result<[u8; N]> {
    let mut out = [0u8; N];
    pyrate_core::unhex_into(s, &mut out).ok_or_else(|| anyhow!("bad code"))?;
    Ok(out)
}
