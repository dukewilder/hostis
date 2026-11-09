//! The gangway: any browser, any phone, no app. A Lantern serves the Log as JSON,
//! each station as an endless MP3 stream like any internet radio, and a page to
//! play them. One encoder per station, however many tabs are listening.
//!
//! No rights reserved.

use std::{
    collections::HashMap,
    convert::Infallible,
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::Duration,
};

use axum::{
    Router,
    body::{Body, Bytes},
    extract::{Path, State},
    http::{StatusCode, header},
    response::{Html, IntoResponse, Response},
    routing::get,
};
use futures_lite::stream;
use pyrate_core::{Archive, Body as Entry, StationId, audio::ChunkDecoder, clock::Clock};
use serde_json::json;
use tokio::sync::broadcast;

use crate::{Inner, now_ms, say};

const PAGE: &str = include_str!("web/listen.html");
/// How far ahead of the hour the encoder runs. Browsers buffer the rest.
const LEAD_MS: u64 = 4_000;

type Towers = Arc<Mutex<HashMap<StationId, broadcast::Sender<Bytes>>>>;

#[derive(Clone)]
struct Web {
    inner: Arc<Inner>,
    towers: Towers,
}

pub async fn serve(inner: Arc<Inner>, addr: SocketAddr) -> anyhow::Result<()> {
    let web = Web { inner, towers: Arc::new(Mutex::new(HashMap::new())) };
    let app = Router::new()
        .route("/", get(|| async { Html(PAGE) }))
        .route("/log.json", get(log))
        .route("/now/{id}", get(now))
        .route("/listen/{id}", get(listen))
        .with_state(web);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    say(&format!("web listener on http://{addr}"));
    axum::serve(listener, app).await?;
    Ok(())
}

async fn log(State(w): State<Web>) -> impl IntoResponse {
    ([(header::CACHE_CONTROL, "no-store"), (header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")], axum::Json(w.inner.stations()))
}

fn describe(a: &Archive, track: &[u8; 32]) -> (String, String) {
    for e in a.entries() {
        if let Entry::Track { title, by, .. } = e.entry.body() {
            if e.hash().ok().as_ref() == Some(track) {
                return (title.clone(), by.clone());
            }
        }
    }
    (String::new(), String::new())
}

fn live_tail(a: &Archive, now: u64) -> Option<Vec<(u64, pyrate_core::Chunk, u8)>> {
    let last = a.entries().last()?;
    if !matches!(last.entry.body(), Entry::Live { .. }) || now.saturating_sub(last.entry.at_ms) > 25_000 {
        return None;
    }
    Some(a.entries().iter().filter_map(|e| match e.entry.body() {
        Entry::Live { chunk, channels, .. } => Some((e.entry.seq, *chunk, *channels)),
        _ => None,
    }).collect())
}

async fn now(State(w): State<Web>, Path(id): Path<String>) -> Response {
    let Ok(st) = StationId::from_hex(&id) else { return StatusCode::NOT_FOUND.into_response() };
    let Some(a) = w.inner.archive(&st) else { return StatusCode::NOT_FOUND.into_response() };
    let t = now_ms();
    if live_tail(&a, t).is_some() {
        return axum::Json(json!({ "live": true, "title": "ON AIR" })).into_response();
    }
    let np = Clock::new(&a).ok().and_then(|c| c.now(t));
    let body = match np {
        Some(np) => {
            let (title, by) = describe(&a, &np.track);
            json!({ "live": false, "title": title, "by": by, "position_ms": np.offset_ms, "duration_ms": np.duration_ms })
        }
        None => json!({ "live": false, "title": "", "by": "" }),
    };
    ([(header::CACHE_CONTROL, "no-store")], axum::Json(body)).into_response()
}

async fn listen(State(w): State<Web>, Path(id): Path<String>) -> Response {
    let id = id.trim_end_matches(".mp3");
    let Ok(st) = StationId::from_hex(id) else { return StatusCode::NOT_FOUND.into_response() };
    let mut guest = [0u8; 32];
    if getrandom_fill(&mut guest).is_ok() {
        w.inner.count_guest(&st, &guest);
    }
    let rx = {
        let mut towers = w.towers.lock().unwrap();
        match towers.get(&st) {
            Some(tx) => tx.subscribe(),
            None => {
                let (tx, rx) = broadcast::channel(256);
                towers.insert(st, tx.clone());
                tokio::spawn(tower(w.inner.clone(), w.towers.clone(), st, tx));
                rx
            }
        }
    };
    let body = stream::unfold((rx, guest, w.inner.clone(), st, now_ms()), |(mut rx, guest, inner, st, mut counted)| async move {
        loop {
            match rx.recv().await {
                Ok(b) => {
                    // Still aboard: count this tab again each hour it stays.
                    if now_ms() / 3_600_000 != counted / 3_600_000 {
                        inner.count_guest(&st, &guest);
                        counted = now_ms();
                    }
                    return Some((Ok::<Bytes, Infallible>(b), (rx, guest, inner, st, counted)));
                }
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => return None,
            }
        }
    });
    Response::builder()
        .header(header::CONTENT_TYPE, "audio/mpeg")
        .header(header::CACHE_CONTROL, "no-store")
        .header("icy-name", "Pyrate Radio")
        .body(Body::from_stream(body))
        .unwrap()
}

fn getrandom_fill(b: &mut [u8; 32]) -> anyhow::Result<()> {
    let k = pyrate_core::StationKey::generate()?;
    b.copy_from_slice(&k.seed());
    Ok(())
}

/// One per station with listeners: follows the clock (or the live broadcast),
/// decodes, encodes to MP3 at the pace of the hour, and hands the bytes to every tab.
async fn tower(inner: Arc<Inner>, towers: Towers, st: StationId, tx: broadcast::Sender<Bytes>) {
    say(&format!("web: tower up for {}", st.call_sign()));
    let mut lame = match mp3lame_encoder::Builder::new().and_then(|mut b| {
        b.set_num_channels(2).ok()?;
        b.set_sample_rate(48_000).ok()?;
        b.set_brate(mp3lame_encoder::Bitrate::Kbps128).ok()?;
        b.set_quality(mp3lame_encoder::Quality::Good).ok()?;
        b.build().ok()
    }) {
        Some(l) => l,
        None => return,
    };
    let mut playhead: Option<u64> = None;
    let mut track: Option<[u8; 32]> = None;
    let mut decoder: Option<(u8, ChunkDecoder)> = None;
    let mut live_seq: Option<u64> = None;
    let mut idle_since: Option<u64> = None;
    let mut last_sync = 0;
    loop {
        let t = now_ms();
        if tx.receiver_count() == 0 {
            let since = *idle_since.get_or_insert(t);
            if t - since > 30_000 {
                break;
            }
        } else {
            idle_since = None;
        }
        if t - last_sync > if live_seq.is_some() { 2_500 } else { 15_000 } {
            last_sync = t;
            let _ = inner.sync_log(st).await;
        }
        let Some(a) = inner.archive(&st) else {
            tokio::time::sleep(Duration::from_secs(1)).await;
            continue;
        };
        // Ahead enough? Rest.
        if let Some(ph) = playhead {
            if ph > t + LEAD_MS {
                tokio::time::sleep(Duration::from_millis(250)).await;
                continue;
            }
            if ph + 8_000 < t && live_seq.is_none() {
                playhead = None; // fell behind the hour: rejoin it
            }
        }
        let (pcm, ch) = if let Some(tail) = live_tail(&a, t) {
            let next = match live_seq {
                Some(s) => tail.iter().find(|(q, _, _)| *q > s).cloned(),
                None => tail.iter().rev().nth(1).or(tail.last()).cloned(),
            };
            let Some((seq, chunk, ch)) = next else {
                tokio::time::sleep(Duration::from_millis(500)).await;
                continue;
            };
            let Ok(bytes) = inner.chunk(&st, &chunk.hash).await else { continue };
            if live_seq.is_none() || decoder.as_ref().is_none_or(|(c, _)| *c != ch) {
                decoder = ChunkDecoder::new(ch).ok().map(|d| (ch, d));
            }
            live_seq = Some(seq);
            track = None;
            playhead = Some(playhead.unwrap_or(t).max(t) + chunk.duration_ms());
            let Some((_, d)) = decoder.as_mut() else { continue };
            (d.decode(&bytes).unwrap_or_default(), ch)
        } else {
            if live_seq.take().is_some() {
                playhead = None;
                decoder = None;
            }
            let Ok(clock) = Clock::new(&a) else { break };
            let at = playhead.unwrap_or(t);
            let Some(np) = clock.now(at) else {
                tokio::time::sleep(Duration::from_secs(2)).await;
                continue;
            };
            let entry = a.entries().iter().find(|e| e.hash().ok() == Some(np.track));
            let Some(Entry::Track { chunks, channels, .. }) = entry.map(|e| e.entry.body()) else { break };
            let Some(chunk) = chunks.get(np.chunk).copied() else { continue };
            let Ok(bytes) = inner.chunk(&st, &chunk.hash).await else {
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            };
            if track != Some(np.track) || decoder.is_none() {
                decoder = ChunkDecoder::new(*channels).ok().map(|d| (*channels, d));
                track = Some(np.track);
            }
            let Some((_, d)) = decoder.as_mut() else { continue };
            let pcm = d.decode(&bytes).unwrap_or_default();
            let skip = (np.chunk_offset_ms * 48) as usize * *channels as usize;
            playhead = Some(at - np.chunk_offset_ms + chunk.duration_ms());
            (pcm.get(skip..).unwrap_or(&[]).to_vec(), *channels)
        };
        // To stereo, then MP3.
        let stereo: Vec<i16> = if ch == 2 { pcm } else { pcm.iter().flat_map(|s| [*s, *s]).collect() };
        let mut out = Vec::with_capacity(stereo.len() / 2 + 7200);
        if lame.encode_to_vec(mp3lame_encoder::InterleavedPcm(&stereo), &mut out).is_ok() && !out.is_empty() {
            let _ = tx.send(Bytes::from(out));
        }
    }
    towers.lock().unwrap().remove(&st);
    say(&format!("web: tower down for {}", st.call_sign()));
}
