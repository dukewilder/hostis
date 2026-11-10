//! The gangway: any browser, any phone, no app. A Lantern serves the Log as JSON,
//! each station as an endless MP3 stream like any internet radio, and a page to
//! play them. One encoder per station, however many tabs are listening. Each station's
//! recordings also go out as a podcast feed, so any podcast app can follow it.
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
    http::{HeaderMap, StatusCode, header},
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
    /// The gangway carries only its own Lantern's stations unless told otherwise. A public
    /// page that streams any station makes its keeper the distributor of whatever is on air.
    every_station: bool,
}

impl Web {
    fn allowed(&self, st: &StationId) -> bool {
        self.every_station || self.inner.store.lock().unwrap().keys.contains_key(st)
    }
}

pub async fn serve(inner: Arc<Inner>, addr: SocketAddr, every_station: bool) -> anyhow::Result<()> {
    let web = Web { inner, towers: Arc::new(Mutex::new(HashMap::new())), every_station };
    let app = Router::new()
        .route("/", get(|| async { Html(PAGE) }))
        .route("/log.json", get(log))
        .route("/now/{id}", get(now))
        .route("/listen/{id}", get(listen))
        .route("/art/{hash}", get(art))
        .route("/brand/{name}", get(brand))
        .route("/feed/{id}", get(feed))
        .route("/player/", get(|| async { player(String::from("index.html")).await }))
        .route("/player/{*path}", get(|Path(p): Path<String>| async move { player(p).await }))
        .route("/episode/{id}/{track}", get(episode))
        .with_state(web);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    say(&format!("web listener on http://{addr}"));
    axum::serve(listener, app).await?;
    Ok(())
}

async fn log(State(w): State<Web>) -> impl IntoResponse {
    let all = w.inner.stations();
    let list: Vec<serde_json::Value> = all.as_array().into_iter().flatten()
        .filter(|s| s["id"].as_str().and_then(|i| StationId::from_hex(i).ok()).is_some_and(|i| w.allowed(&i)))
        .cloned().collect();
    ([(header::CACHE_CONTROL, "no-store"), (header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")], axum::Json(list))
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
    if !w.allowed(&st) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let Some(a) = w.inner.archive(&st) else { return StatusCode::NOT_FOUND.into_response() };
    let t = now_ms();
    if live_tail(&a, t).is_some() {
        return axum::Json(json!({ "live": true, "title": "ON AIR" })).into_response();
    }
    let np = Clock::new(&a).ok().and_then(|c| c.now(t));
    let body = match np {
        Some(np) => {
            let (title, by) = describe(&a, &np.track);
            let pic = a.track_art(&np.track).map(|r| crate::hex(&r.hash));
            json!({ "live": false, "title": title, "by": by, "art": pic, "position_ms": np.offset_ms, "duration_ms": np.duration_ms })
        }
        None => json!({ "live": false, "title": "", "by": "" }),
    };
    ([(header::CACHE_CONTROL, "no-store")], axum::Json(body)).into_response()
}

fn xml(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Where this page is reached from, for links inside the feed: the tunnel's name, not ours.
fn base(h: &HeaderMap) -> String {
    let host = h.get("x-forwarded-host").or(h.get(header::HOST)).and_then(|v| v.to_str().ok()).unwrap_or("localhost");
    let local = host.starts_with("127.") || host.starts_with("localhost") || host.starts_with("[::1]");
    let proto = h.get("x-forwarded-proto").and_then(|v| v.to_str().ok()).unwrap_or(if local { "http" } else { "https" });
    format!("{proto}://{host}")
}

fn rfc822(ms: u64) -> String {
    const DAYS: [&str; 7] = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"];
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let days = (ms / 86_400_000) as i64;
    let secs = ms / 1000 % 86_400;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{}, {d:02} {} {y} {:02}:{:02}:{:02} GMT", DAYS[days.rem_euclid(7) as usize], MONTHS[(m - 1) as usize], secs / 3600, secs / 60 % 60, secs % 60)
}

/// A station's recordings as a podcast feed, newest first. Any podcast app can follow it.
async fn feed(State(w): State<Web>, Path(id): Path<String>, h: HeaderMap) -> Response {
    let Ok(st) = StationId::from_hex(id.trim_end_matches(".xml")) else { return StatusCode::NOT_FOUND.into_response() };
    if !w.allowed(&st) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let Some(a) = w.inner.archive(&st) else { return StatusCode::NOT_FOUND.into_response() };
    let (name, about) = match a.colours() {
        Some(Entry::Colours { name, about, .. }) => (name.clone(), about.clone()),
        _ => (st.call_sign(), String::new()),
    };
    let base = base(&h);
    let link = format!("https://hostis.black/pyrateradio/listen/#{}", st.to_hex());
    let cover = a.art().and_then(|(c, _, _)| c).map(|c| format!("{base}/art/{}.jpg", crate::hex(&c.hash)));
    let lib = a.library().unwrap_or_default();
    let mut items = String::new();
    for (h, e) in lib.iter().rev().take(100) {
        let Entry::Track { title, by, chunks, channels, .. } = e.entry.body() else { continue };
        let ms: u64 = chunks.iter().map(|c| c.duration_ms()).sum();
        // The bell is the station's ident, not an episode.
        if matches!(e.entry.body(), Entry::Track { source, .. } if source.starts_with("bell ")) {
            continue;
        }
        let kbps = if *channels == 1 { 64 } else { 128 };
        let art = a.track_art(h).map(|r| format!("<itunes:image href=\"{base}/art/{}.jpg\"/>", crate::hex(&r.hash))).unwrap_or_default();
        items.push_str(&format!(
            "<item><title>{}</title><itunes:author>{}</itunes:author><guid isPermaLink=\"false\">{}</guid><pubDate>{}</pubDate><enclosure url=\"{base}/episode/{}/{}.mp3\" length=\"{}\" type=\"audio/mpeg\"/><itunes:duration>{}</itunes:duration>{art}</item>\n",
            xml(title), xml(by), crate::hex(h), rfc822(e.entry.at_ms), st.to_hex(), crate::hex(h), ms * kbps / 8, ms / 1000,
        ));
    }
    let image = cover.map(|c| format!("<itunes:image href=\"{c}\"/><image><url>{c}</url><title>{}</title><link>{}</link></image>", xml(&name), xml(&link))).unwrap_or_default();
    let body = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<rss version=\"2.0\" xmlns:itunes=\"http://www.itunes.com/dtds/podcast-1.0.dtd\"><channel><title>{}</title><link>{}</link><description>{}</description><itunes:author>{}</itunes:author>{image}<generator>Pyrate Radio</generator>\n{items}</channel></rss>\n",
        xml(&name), xml(&link), xml(if about.is_empty() { "A station on Pyrate Radio" } else { &about }), xml(&name),
    );
    ([(header::CONTENT_TYPE, "application/rss+xml; charset=utf-8"), (header::CACHE_CONTROL, "max-age=300")], body).into_response()
}

/// One recording as an MP3 for podcast apps, encoded once and kept on disk.
async fn episode(State(w): State<Web>, Path((id, track)): Path<(String, String)>, h: HeaderMap) -> Response {
    let Ok(st) = StationId::from_hex(&id) else { return StatusCode::NOT_FOUND.into_response() };
    let Ok(th) = crate::unhex(track.trim_end_matches(".mp3")) else { return StatusCode::NOT_FOUND.into_response() };
    if !w.allowed(&st) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let dir = w.inner.cfg.dir.join("episodes");
    let path = dir.join(format!("{}.mp3", crate::hex(&th)));
    if !path.exists() {
        let Some(a) = w.inner.archive(&st) else { return StatusCode::NOT_FOUND.into_response() };
        let found = a.library().unwrap_or_default().into_iter().find(|(x, _)| *x == th);
        let Some((_, e)) = found else { return StatusCode::NOT_FOUND.into_response() };
        let Entry::Track { chunks, channels, .. } = e.entry.body().clone() else { return StatusCode::NOT_FOUND.into_response() };
        let mut pcm = Vec::new();
        let Ok(mut dec) = ChunkDecoder::new(channels) else { return StatusCode::INTERNAL_SERVER_ERROR.into_response() };
        for c in &chunks {
            let Ok(bytes) = w.inner.chunk(&st, &c.hash).await else { return StatusCode::SERVICE_UNAVAILABLE.into_response() };
            pcm.extend(dec.decode(&bytes).unwrap_or_default());
        }
        let mp3 = tokio::task::spawn_blocking(move || -> Option<Vec<u8>> {
            let mut lame = mp3lame_encoder::Builder::new()?;
            lame.set_num_channels(channels.clamp(1, 2)).ok()?;
            lame.set_sample_rate(48_000).ok()?;
            lame.set_brate(if channels == 1 { mp3lame_encoder::Bitrate::Kbps64 } else { mp3lame_encoder::Bitrate::Kbps128 }).ok()?;
            lame.set_quality(mp3lame_encoder::Quality::Good).ok()?;
            let mut lame = lame.build().ok()?;
            let mut out = Vec::with_capacity(pcm.len() / 3);
            for part in pcm.chunks(48_000 * channels as usize) {
                if channels == 1 {
                    lame.encode_to_vec(mp3lame_encoder::MonoPcm(part), &mut out).ok()?;
                } else {
                    lame.encode_to_vec(mp3lame_encoder::InterleavedPcm(part), &mut out).ok()?;
                }
            }
            lame.flush_to_vec::<mp3lame_encoder::FlushNoGap>(&mut out).ok()?;
            Some(out)
        })
        .await
        .ok()
        .flatten();
        let Some(mp3) = mp3 else { return StatusCode::INTERNAL_SERVER_ERROR.into_response() };
        let _ = std::fs::create_dir_all(&dir);
        let tmp = path.with_extension("part");
        if std::fs::write(&tmp, &mp3).and_then(|_| std::fs::rename(&tmp, &path)).is_err() {
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
        // Keep the newest forty; the rest can be made again.
        if let Ok(rd) = std::fs::read_dir(&dir) {
            let mut files: Vec<_> = rd.filter_map(|f| f.ok()).filter_map(|f| Some((f.metadata().ok()?.modified().ok()?, f.path()))).collect();
            files.sort();
            let n = files.len().saturating_sub(40);
            for (_, f) in files.into_iter().take(n) {
                let _ = std::fs::remove_file(f);
            }
        }
    }
    let Ok(bytes) = std::fs::read(&path) else { return StatusCode::NOT_FOUND.into_response() };
    let total = bytes.len();
    // Podcast apps ask for pieces.
    let range = h.get(header::RANGE).and_then(|v| v.to_str().ok()).and_then(|r| r.strip_prefix("bytes=")).and_then(|r| {
        let (a, b) = r.split_once('-')?;
        let start: usize = a.parse().ok()?;
        let end: usize = if b.is_empty() { total.saturating_sub(1) } else { b.parse::<usize>().ok()?.min(total.saturating_sub(1)) };
        (start <= end && end < total).then_some((start, end))
    });
    match range {
        Some((s0, e0)) => (
            StatusCode::PARTIAL_CONTENT,
            [(header::CONTENT_TYPE, "audio/mpeg".to_string()), (header::ACCEPT_RANGES, "bytes".into()), (header::CONTENT_RANGE, format!("bytes {s0}-{e0}/{total}"))],
            bytes[s0..=e0].to_vec(),
        )
            .into_response(),
        None => ([(header::CONTENT_TYPE, "audio/mpeg"), (header::ACCEPT_RANGES, "bytes")], bytes).into_response(),
    }
}

/// The browser player, served from PYRATE_PLAYER (a folder holding a copy of
/// hostis.black/pyrateradio/listen/). Any Lantern that serves it is one more door to the
/// network that no one can close by closing a domain.
async fn player(path: String) -> Response {
    let Some(dir) = std::env::var_os("PYRATE_PLAYER") else { return StatusCode::NOT_FOUND.into_response() };
    let path = if path.is_empty() || path.ends_with('/') { format!("{path}index.html") } else { path };
    if path.split('/').any(|p| p == ".." || p.starts_with('.')) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let Ok(bytes) = tokio::fs::read(std::path::Path::new(&dir).join(&path)).await else { return StatusCode::NOT_FOUND.into_response() };
    let kind = match path.rsplit('.').next().unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "wasm" => "application/wasm",
        "json" => "application/json",
        "png" => "image/png",
        "css" => "text/css",
        _ => "application/octet-stream",
    };
    ([(header::CONTENT_TYPE, kind), (header::CACHE_CONTROL, "max-age=600")], bytes).into_response()
}

const WORDMARK: &[u8] = include_bytes!("web/wordmark.png");
const MARK: &[u8] = include_bytes!("web/mark.png");

async fn brand(Path(name): Path<String>) -> Response {
    let body = match name.as_str() {
        "wordmark.png" => WORDMARK,
        "mark.png" => MARK,
        _ => return StatusCode::NOT_FOUND.into_response(),
    };
    ([(header::CONTENT_TYPE, "image/png"), (header::CACHE_CONTROL, "public, max-age=86400")], body).into_response()
}

/// A picture, only when it belongs to a station this page may show.
async fn art(State(w): State<Web>, Path(hash): Path<String>) -> Response {
    let Ok(h) = crate::unhex(hash.trim_end_matches(".jpg")) else { return StatusCode::NOT_FOUND.into_response() };
    let owner = {
        let s = w.inner.store.lock().unwrap();
        s.logs.iter().find(|(_, a)| a.art_hashes().contains(&h)).map(|(id, _)| *id)
    };
    if !owner.is_some_and(|st| w.allowed(&st)) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let bytes = w.inner.store.lock().unwrap().get_chunk(&h).or_else(|| w.inner.art_mem.lock().unwrap().get(&h));
    match bytes {
        Some(b) => ([(header::CONTENT_TYPE, "image/jpeg"), (header::CACHE_CONTROL, "public, max-age=31536000, immutable")], b).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn listen(State(w): State<Web>, Path(id): Path<String>) -> Response {
    let id = id.trim_end_matches(".mp3");
    let Ok(st) = StationId::from_hex(id) else { return StatusCode::NOT_FOUND.into_response() };
    if !w.allowed(&st) {
        return StatusCode::NOT_FOUND.into_response();
    }
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
