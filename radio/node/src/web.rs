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
/// Listeners one gangway serves at once, and stations it streams at once.
const MAX_LISTENERS: usize = 512;
const MAX_TOWERS: usize = 32;
/// The longest recording made into an episode, and how many are made at a time.
const MAX_EPISODE_MS: u64 = 6 * 3_600_000;
const EPISODE_MAKERS: usize = 2;
/// A tab is counted aboard once it has listened this long.
const COUNT_AFTER_MS: u64 = 30_000;

type Towers = Arc<Mutex<HashMap<StationId, broadcast::Sender<Bytes>>>>;

#[derive(Clone)]
struct Web {
    inner: Arc<Inner>,
    towers: Towers,
    /// The gangway carries only its own Lantern's stations unless told otherwise. A public
    /// page that streams any station makes its keeper the distributor of whatever is on air.
    every_station: bool,
    listeners: Arc<tokio::sync::Semaphore>,
    makers: Arc<tokio::sync::Semaphore>,
    /// Episodes being made, so two requests for one recording make it once.
    making: Arc<Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>>,
}

impl Web {
    /// A pocket station is never on the gangway: it is for its invited members only.
    fn allowed(&self, st: &StationId) -> bool {
        !self.inner.is_pocket(st) && (self.every_station || self.inner.store.lock().unwrap().keys.contains_key(st))
    }
}

pub async fn serve(inner: Arc<Inner>, addr: SocketAddr, every_station: bool) -> anyhow::Result<()> {
    let web = Web {
        inner,
        towers: Arc::new(Mutex::new(HashMap::new())),
        every_station,
        listeners: Arc::new(tokio::sync::Semaphore::new(MAX_LISTENERS)),
        makers: Arc::new(tokio::sync::Semaphore::new(EPISODE_MAKERS)),
        making: Arc::new(Mutex::new(HashMap::new())),
    };
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

/// Text made safe for XML: escaped, and without the control characters XML 1.0 forbids, any
/// one of which would make a podcast app refuse the whole feed.
fn xml(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_control() || matches!(c, '\t' | '\n' | '\r'))
        .collect::<String>()
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Where this page is reached from, for links inside the feed: the tunnel's name, not ours.
/// Only a plain host name and http or https are taken from the request.
fn base(h: &HeaderMap) -> String {
    let host = h.get("x-forwarded-host").or(h.get(header::HOST)).and_then(|v| v.to_str().ok()).unwrap_or("localhost");
    let host = if !host.is_empty() && host.len() <= 255 && host.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':' | '[' | ']')) { host } else { "localhost" };
    let local = host.starts_with("127.") || host.starts_with("localhost") || host.starts_with("[::1]");
    let proto = match h.get("x-forwarded-proto").and_then(|v| v.to_str().ok()) {
        Some("http") => "http",
        Some("https") => "https",
        _ if local => "http",
        _ => "https",
    };
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
    let base = xml(&base(&h));
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

/// One recording as an MP3 for podcast apps, encoded once and kept on disk. It is made a chunk
/// at a time, so a three-hour show costs no more memory than a song, and two requests for the
/// same recording make it once.
async fn episode(State(w): State<Web>, Path((id, track)): Path<(String, String)>, h: HeaderMap) -> Response {
    let Ok(st) = StationId::from_hex(&id) else { return StatusCode::NOT_FOUND.into_response() };
    let Ok(th) = crate::unhex(track.trim_end_matches(".mp3")) else { return StatusCode::NOT_FOUND.into_response() };
    if !w.allowed(&st) {
        return StatusCode::NOT_FOUND.into_response();
    }
    // The recording must be in this station's library now: a withdrawn one, or another
    // station's, is not served from the cache either.
    let Some(a) = w.inner.archive(&st) else { return StatusCode::NOT_FOUND.into_response() };
    let found = a.library().unwrap_or_default().into_iter().find(|(x, _)| *x == th).map(|(_, e)| e.entry.body().clone());
    let Some(Entry::Track { chunks, channels, .. }) = found else { return StatusCode::NOT_FOUND.into_response() };
    if chunks.iter().map(|c| c.duration_ms()).sum::<u64>() > MAX_EPISODE_MS || !(1..=2).contains(&channels) {
        return StatusCode::PAYLOAD_TOO_LARGE.into_response();
    }
    let dir = w.inner.cfg.dir.join("episodes");
    let key = format!("{}-{}", st.to_hex(), crate::hex(&th));
    let path = dir.join(format!("{key}.mp3"));
    if !path.exists() {
        let lock = w.making.lock().unwrap().entry(key.clone()).or_default().clone();
        let _making = lock.lock().await;
        if !path.exists() {
            let Ok(_permit) = w.makers.clone().acquire_owned().await else { return StatusCode::SERVICE_UNAVAILABLE.into_response() };
            let _ = std::fs::create_dir_all(&dir);
            let tmp = dir.join(format!("{key}.{}.part", now_ms()));
            let (inner, handle, tmp2) = (w.inner.clone(), tokio::runtime::Handle::current(), tmp.clone());
            let made = tokio::task::spawn_blocking(move || make_episode(&inner, &handle, st, &chunks, channels, &tmp2)).await.ok().flatten();
            if made.is_none() || std::fs::rename(&tmp, &path).is_err() {
                let _ = std::fs::remove_file(&tmp);
                return StatusCode::SERVICE_UNAVAILABLE.into_response();
            }
            // Keep the newest forty; the rest can be made again.
            if let Ok(rd) = std::fs::read_dir(&dir) {
                let mut files: Vec<_> = rd.filter_map(|f| f.ok()).filter(|f| f.path().extension().is_some_and(|e| e == "mp3")).filter_map(|f| Some((f.metadata().ok()?.modified().ok()?, f.path()))).collect();
                files.sort();
                let n = files.len().saturating_sub(40);
                for (_, f) in files.into_iter().take(n) {
                    let _ = std::fs::remove_file(f);
                }
            }
        }
        w.making.lock().unwrap().remove(&key);
    }
    serve_file(&path, &h).await
}

/// Fetches, decodes and encodes a recording chunk by chunk into `out`.
fn make_episode(inner: &Arc<Inner>, handle: &tokio::runtime::Handle, st: StationId, chunks: &[pyrate_core::Chunk], channels: u8, out: &std::path::Path) -> Option<()> {
    use std::io::Write;
    let mut lame = mp3lame_encoder::Builder::new()?;
    lame.set_num_channels(channels).ok()?;
    lame.set_sample_rate(48_000).ok()?;
    lame.set_brate(if channels == 1 { mp3lame_encoder::Bitrate::Kbps64 } else { mp3lame_encoder::Bitrate::Kbps128 }).ok()?;
    lame.set_quality(mp3lame_encoder::Quality::Good).ok()?;
    let mut lame = lame.build().ok()?;
    let mut dec = ChunkDecoder::new(channels).ok()?;
    let mut file = std::io::BufWriter::new(std::fs::File::create(out).ok()?);
    let mut buf = Vec::new();
    for c in chunks {
        let bytes = handle.block_on(inner.chunk(&st, &c.hash)).ok()?;
        let pcm = dec.decode(&bytes).unwrap_or_default();
        // A second at a time. The wrapper writes into the vector's spare room and tells LAME
        // nothing of its size when there is none, so LAME's worst case is reserved first:
        // without it, it writes past the end.
        for part in pcm.chunks(48_000 * channels as usize) {
            buf.clear();
            buf.reserve(part.len() * 5 / 4 + 7_200);
            if channels == 1 {
                lame.encode_to_vec(mp3lame_encoder::MonoPcm(part), &mut buf).ok()?;
            } else {
                lame.encode_to_vec(mp3lame_encoder::InterleavedPcm(part), &mut buf).ok()?;
            }
            file.write_all(&buf).ok()?;
        }
    }
    buf.clear();
    buf.reserve(7_200);
    lame.flush_to_vec::<mp3lame_encoder::FlushNoGap>(&mut buf).ok()?;
    file.write_all(&buf).ok()?;
    file.flush().ok()
}

/// A file on disk, in the pieces podcast apps ask for: a range, a suffix, or the whole,
/// read as it goes out rather than loaded whole.
async fn serve_file(path: &std::path::Path, h: &HeaderMap) -> Response {
    use tokio::io::{AsyncReadExt, AsyncSeekExt};
    let Ok(mut f) = tokio::fs::File::open(path).await else { return StatusCode::NOT_FOUND.into_response() };
    let Ok(total) = f.metadata().await.map(|m| m.len()) else { return StatusCode::NOT_FOUND.into_response() };
    let asked = h.get(header::RANGE).and_then(|v| v.to_str().ok()).and_then(|r| r.strip_prefix("bytes="));
    let (status, start, end) = match asked.map(|r| r.split_once('-')) {
        None => (StatusCode::OK, 0, total.saturating_sub(1)),
        Some(Some(("", n))) => match n.parse::<u64>() {
            Ok(n) if n > 0 && total > 0 => (StatusCode::PARTIAL_CONTENT, total.saturating_sub(n), total - 1),
            _ => return (StatusCode::RANGE_NOT_SATISFIABLE, [(header::CONTENT_RANGE, format!("bytes */{total}"))]).into_response(),
        },
        Some(Some((a, b))) => match (a.parse::<u64>(), if b.is_empty() { Ok(total.saturating_sub(1)) } else { b.parse::<u64>() }) {
            (Ok(s), Ok(e)) if s < total && s <= e => (StatusCode::PARTIAL_CONTENT, s, e.min(total - 1)),
            _ => return (StatusCode::RANGE_NOT_SATISFIABLE, [(header::CONTENT_RANGE, format!("bytes */{total}"))]).into_response(),
        },
        Some(None) => (StatusCode::OK, 0, total.saturating_sub(1)),
    };
    if f.seek(std::io::SeekFrom::Start(start)).await.is_err() {
        return StatusCode::NOT_FOUND.into_response();
    }
    let len = if total == 0 { 0 } else { end - start + 1 };
    let body = stream::unfold((f, len), |(mut f, left)| async move {
        if left == 0 {
            return None;
        }
        let mut b = vec![0u8; left.min(64 * 1024) as usize];
        match f.read(&mut b).await {
            Ok(0) | Err(_) => None,
            Ok(n) => {
                b.truncate(n);
                Some((Ok::<Bytes, std::io::Error>(Bytes::from(b)), (f, left - n as u64)))
            }
        }
    });
    let mut r = Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "audio/mpeg")
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CONTENT_LENGTH, len.to_string());
    if status == StatusCode::PARTIAL_CONTENT {
        r = r.header(header::CONTENT_RANGE, format!("bytes {start}-{end}/{total}"));
    }
    r.body(Body::from_stream(body)).unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

/// The browser player, served from PYRATE_PLAYER (a folder holding a copy of
/// hostis.black/pyrateradio/listen/). Any Lantern that serves it is one more door to the
/// network that no one can close by closing a domain.
async fn player(path: String) -> Response {
    let Some(dir) = std::env::var_os("PYRATE_PLAYER") else { return StatusCode::NOT_FOUND.into_response() };
    let Some(file) = player_path(std::path::Path::new(&dir), &path) else { return StatusCode::NOT_FOUND.into_response() };
    let Ok(bytes) = tokio::fs::read(&file).await else { return StatusCode::NOT_FOUND.into_response() };
    let kind = match file.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "wasm" => "application/wasm",
        "json" => "application/json",
        "png" => "image/png",
        "css" => "text/css",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    };
    (
        [
            (header::CONTENT_TYPE, kind),
            (header::CACHE_CONTROL, "max-age=600"),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            (header::REFERRER_POLICY, "no-referrer"),
            (header::CONTENT_SECURITY_POLICY, "frame-ancestors 'self'"),
        ],
        bytes,
    )
        .into_response()
}

/// The file a player path names, inside the player folder and nowhere else. Every segment must
/// be a plain name: none empty (a doubled slash would make the rest absolute), none starting
/// with a dot, none holding a backslash, a colon or a NUL. Whatever is found must still lie in
/// the folder once links are followed.
fn player_path(dir: &std::path::Path, path: &str) -> Option<std::path::PathBuf> {
    let path = if path.is_empty() || path.ends_with('/') { format!("{path}index.html") } else { path.to_string() };
    let mut out = dir.to_path_buf();
    for seg in path.split('/') {
        if seg.is_empty() || seg.starts_with('.') || seg.contains(['\\', ':', '\0']) {
            return None;
        }
        out.push(seg);
    }
    let real = out.canonicalize().ok()?;
    real.starts_with(dir.canonicalize().ok()?).then_some(real)
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_player_serves_its_own_folder_and_nothing_else() {
        let dir = std::env::temp_dir().join(format!("pyrate-player-test-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("pkg")).unwrap();
        std::fs::write(dir.join("index.html"), "x").unwrap();
        std::fs::write(dir.join("pkg").join("a.js"), "x").unwrap();
        assert!(super::player_path(&dir, "").is_some());
        assert!(super::player_path(&dir, "pkg/a.js").is_some());
        for bad in ["/etc/passwd", "//etc/passwd", "../index.html", "pkg/../../x", ".git/config", "pkg\\a.js", "C:/x", "pkg//a.js"] {
            assert!(super::player_path(&dir, bad).is_none(), "{bad}");
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink("/etc", dir.join("etc")).unwrap();
            assert!(super::player_path(&dir, "etc/passwd").is_none(), "a link out of the folder");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
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
    // Only stations this page may show are searched: its own, unless it serves every station.
    let owner = {
        let s = w.inner.store.lock().unwrap();
        let own = s.own();
        let mut look: Box<dyn Iterator<Item = (&StationId, &Archive)>> = if w.every_station { Box::new(s.logs.iter()) } else { Box::new(own.iter().filter_map(|id| s.logs.get_key_value(id))) };
        look.find(|(_, a)| a.art_hashes().contains(&h)).map(|(id, _)| *id)
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
    if !w.allowed(&st) || w.inner.archive(&st).is_none() {
        return StatusCode::NOT_FOUND.into_response();
    }
    let Ok(permit) = w.listeners.clone().try_acquire_owned() else { return StatusCode::SERVICE_UNAVAILABLE.into_response() };
    let mut guest = [0u8; 32];
    let _ = getrandom_fill(&mut guest);
    let rx = {
        let mut towers = w.towers.lock().unwrap();
        match towers.get(&st) {
            Some(tx) => tx.subscribe(),
            None if towers.len() >= MAX_TOWERS => return StatusCode::SERVICE_UNAVAILABLE.into_response(),
            None => {
                let (tx, rx) = broadcast::channel(256);
                towers.insert(st, tx.clone());
                tokio::spawn(tower(w.inner.clone(), w.towers.clone(), st, tx));
                rx
            }
        }
    };
    // A tab is counted once it has listened half a minute, then once each hour it stays: a
    // flurry of reconnects is not a crowd.
    let state = (rx, guest, w.inner.clone(), st, now_ms(), None::<u64>, permit);
    let body = stream::unfold(state, |(mut rx, guest, inner, st, began, mut counted, permit)| async move {
        loop {
            match rx.recv().await {
                Ok(b) => {
                    let t = now_ms();
                    if t.saturating_sub(began) >= COUNT_AFTER_MS && counted.is_none_or(|c| c / 3_600_000 != t / 3_600_000) {
                        inner.count_guest(&st, &guest);
                        counted = Some(t);
                    }
                    return Some((Ok::<Bytes, Infallible>(b), (rx, guest, inner, st, began, counted, permit)));
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
    // Steps in a row that sent nothing: past a minute's worth, the tower comes down.
    let mut dry = 0u32;
    loop {
        if dry > 0 {
            tokio::time::sleep(Duration::from_millis(500)).await;
            if dry > 120 {
                break;
            }
        }
        dry += 1;
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
                dry = 0;
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
                // Off the air: the tower waits for the station to come back.
                dry = 0;
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
            dry = 0;
        }
    }
    towers.lock().unwrap().remove(&st);
    say(&format!("web: tower down for {}", st.call_sign()));
}
