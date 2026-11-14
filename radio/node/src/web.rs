//! The gangway: any browser, any phone, no app. A Lantern serves the Log as JSON,
//! each station as an endless MP3 stream like any internet radio, and a page to
//! play them. One encoder per station, however many tabs are listening. Each station's
//! recordings also go out as a podcast feed, so any podcast app can follow it. Given a copy of
//! the open player (`--player`), it serves that too, at /listen/.
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
const PAGE_JS: &str = include_str!("web/listen.js");
/// The listener page loads only its own script, pictures and sound from this Lantern; it runs
/// nothing inline and sends no referrer.
const PAGE_CSP: &str = "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; media-src 'self'; connect-src 'self'; font-src 'self' data:; object-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'self'";
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
    views: Arc<Views>,
    /// The gangway carries only its own Lantern's stations unless told otherwise. A public
    /// page that streams any station makes its keeper the distributor of whatever is on air.
    every_station: bool,
    listeners: Arc<tokio::sync::Semaphore>,
    makers: Arc<tokio::sync::Semaphore>,
    /// Episodes being made, so two requests for one recording make it once.
    making: Arc<Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>>,
}

/// One recording, as the web pages need it.
struct TrackView {
    /// Where its entry is in the log.
    at: usize,
    title: String,
    by: String,
    channels: u8,
    duration_ms: u64,
    added_ms: u64,
    art: Option<[u8; 32]>,
    bell: bool,
    in_library: bool,
}

/// What the web pages need of one station's log, worked out once each time the log grows and
/// shared by every request after: no request copies a log or hashes its entries.
struct View {
    len: u64,
    clock: Option<Mutex<Clock>>,
    /// The last entry's time, when it is live sound.
    live_at: Option<u64>,
    /// The newest live chunks, oldest first.
    live: Vec<(u64, pyrate_core::Chunk, u8)>,
    tracks: HashMap<[u8; 32], TrackView>,
    /// The library in the order it was built, oldest first.
    library: Vec<[u8; 32]>,
    name: String,
    about: String,
    cover: Option<[u8; 32]>,
    /// The feed as last made, and the address it was made for.
    feed: Mutex<Option<(String, String)>>,
    /// What is playing, as last worked out, and when: good for a second.
    playing: Mutex<Option<(u64, serde_json::Value)>>,
}

impl View {
    fn build(a: &Archive) -> Self {
        let mut tracks = HashMap::new();
        let mut live = Vec::new();
        for (i, e) in a.entries().iter().enumerate() {
            match e.entry.body() {
                Entry::Track { title, by, channels, chunks, source, .. } => {
                    let Ok(h) = e.hash() else { continue };
                    let duration_ms = chunks.iter().map(|c| c.duration_ms()).sum();
                    tracks.insert(h, TrackView { at: i, title: title.clone(), by: by.clone(), channels: *channels, duration_ms, added_ms: e.entry.at_ms, art: None, bell: source.starts_with("bell "), in_library: false });
                }
                Entry::TrackArt { track, art } => {
                    if let Some(t) = tracks.get_mut(track) {
                        t.art = Some(art.hash);
                    }
                }
                Entry::Live { chunk, channels, .. } => {
                    live.push((e.entry.seq, *chunk, *channels));
                    if live.len() > 64 {
                        live.drain(..32);
                    }
                }
                _ => {}
            }
        }
        let library: Vec<[u8; 32]> = a.library().unwrap_or_default().into_iter().map(|(h, _)| h).collect();
        for h in &library {
            if let Some(t) = tracks.get_mut(h) {
                t.in_library = true;
            }
        }
        let live_at = a.entries().last().filter(|l| matches!(l.entry.body(), Entry::Live { .. })).map(|l| l.entry.at_ms);
        let (name, about) = match a.colours() {
            Some(Entry::Colours { name, about, .. }) => (name.clone(), about.clone()),
            _ => (a.station().call_sign(), String::new()),
        };
        let cover = a.art().and_then(|(c, _, _)| c).map(|c| c.hash);
        Self { len: a.len(), clock: Clock::new(a).ok().map(Mutex::new), live_at, live, tracks, library, name, about, cover, feed: Mutex::new(None), playing: Mutex::new(None) }
    }

    fn live_tail(&self, now: u64) -> Option<&[(u64, pyrate_core::Chunk, u8)]> {
        self.live_at.filter(|l| now.saturating_sub(*l) <= 25_000).map(|_| self.live.as_slice())
    }
}

/// Each station's view, kept until its log grows.
#[derive(Default)]
struct Views {
    map: Mutex<HashMap<StationId, Arc<View>>>,
    /// One view is worked out at a time: a crowd asking at once waits for the one in hand.
    building: tokio::sync::Mutex<()>,
}

impl Views {
    async fn get(&self, inner: &Arc<Inner>, st: &StationId) -> Option<Arc<View>> {
        let len = inner.store.lock().unwrap().logs.get(st).map(|a| a.len())?;
        let cached = |m: &HashMap<StationId, Arc<View>>| m.get(st).filter(|v| v.len == len).cloned();
        if let Some(v) = cached(&self.map.lock().unwrap()) {
            return Some(v);
        }
        let _b = self.building.lock().await;
        let len = inner.store.lock().unwrap().logs.get(st).map(|a| a.len())?;
        if let Some(v) = self.map.lock().unwrap().get(st).filter(|v| v.len == len).cloned() {
            return Some(v);
        }
        let a = inner.archive(st)?;
        let v = Arc::new(tokio::task::spawn_blocking(move || View::build(&a)).await.ok()?);
        let mut m = self.map.lock().unwrap();
        if m.len() >= 128 {
            m.clear();
        }
        m.insert(*st, v.clone());
        Some(v)
    }
}

/// A recording's chunks, read from the log in place.
fn chunks_at(inner: &Inner, st: &StationId, at: usize) -> Option<(Vec<pyrate_core::Chunk>, u8)> {
    let s = inner.store.lock().unwrap();
    match s.logs.get(st)?.entries().get(at)?.entry.body() {
        Entry::Track { chunks, channels, .. } => Some((chunks.clone(), *channels)),
        _ => None,
    }
}

/// One chunk of a recording, read from the log in place.
fn chunk_at(inner: &Inner, st: &StationId, at: usize, n: usize) -> Option<(pyrate_core::Chunk, u8)> {
    let s = inner.store.lock().unwrap();
    match s.logs.get(st)?.entries().get(at)?.entry.body() {
        Entry::Track { chunks, channels, .. } => Some((*chunks.get(n)?, *channels)),
        _ => None,
    }
}

impl Web {
    /// A pocket station is never on the gangway: it is for its invited members only.
    fn allowed(&self, st: &StationId) -> bool {
        !self.inner.is_pocket(st) && (self.every_station || self.inner.store.lock().unwrap().keys.contains_key(st))
    }
}

/// The node, for the player's page: it hands the player the doors it knows.
static NODE: std::sync::OnceLock<std::sync::Weak<Inner>> = std::sync::OnceLock::new();

/// The doors this node knows, as signed notes in hex, freshest first.
fn doors_hex(n: usize) -> Vec<String> {
    let Some(inner) = NODE.get().and_then(|w| w.upgrade()) else { return vec![] };
    let mut list = inner.notes.lock().unwrap().doors(crate::now_ms(), n);
    let own = inner.door.lock().unwrap().clone();
    if let Some(g) = own.and_then(|d| d.note(&inner.secret, crate::now_ms())).and_then(|g| postcard::to_stdvec(&g).ok()) {
        list.retain(|x| *x != g);
        list.insert(0, g);
        list.truncate(n);
    }
    list.iter().map(|b| crate::hex(b)).collect()
}

pub async fn serve(inner: Arc<Inner>, addr: SocketAddr, every_station: bool) -> anyhow::Result<()> {
    let _ = NODE.set(Arc::downgrade(&inner));
    let web = Web {
        inner,
        towers: Arc::new(Mutex::new(HashMap::new())),
        views: Arc::new(Views::default()),
        every_station,
        listeners: Arc::new(tokio::sync::Semaphore::new(MAX_LISTENERS)),
        makers: Arc::new(tokio::sync::Semaphore::new(EPISODE_MAKERS)),
        making: Arc::new(Mutex::new(HashMap::new())),
    };
    let app = Router::new()
        .route("/", get(|| async { ([(header::CONTENT_SECURITY_POLICY, PAGE_CSP), (header::REFERRER_POLICY, "no-referrer"), (header::X_CONTENT_TYPE_OPTIONS, "nosniff")], Html(PAGE)) }))
        .route("/listen.js", get(|| async { ([(header::CONTENT_TYPE, "text/javascript; charset=utf-8"), (header::X_CONTENT_TYPE_OPTIONS, "nosniff"), (header::CACHE_CONTROL, "max-age=600")], PAGE_JS) }))
        .route("/log.json", get(log))
        // The doors this node knows, one signed note (hex) a line: what a build bakes into the
        // player at deploy time.
        .route("/doors.txt", get(|| async { ([(header::CONTENT_TYPE, "text/plain; charset=utf-8"), (header::CACHE_CONTROL, "no-store")], doors_hex(16).join("\n")) }))
        .route("/now/{id}", get(now))
        .route("/listen", get(|| async { axum::response::Redirect::permanent("/listen/") }))
        .route("/listen/", get(|| async { player(String::new()).await }))
        // One path for both: a station's id is its endless MP3, any other name a file of the
        // open player. None of the player's files is named like a station.
        .route("/listen/{*path}", get(listen_or_player))
        .route("/art/{hash}", get(art))
        .route("/brand/{name}", get(brand))
        .route("/feed/{id}", get(feed))
        // The player's old address, for links made before it moved to /listen/.
        .route("/player", get(|| async { axum::response::Redirect::permanent("/listen/") }))
        .route("/player/", get(|| async { axum::response::Redirect::permanent("/listen/") }))
        .route("/player/{*path}", get(|Path(p): Path<String>| async move { player_moved(&p) }))
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

async fn now(State(w): State<Web>, Path(id): Path<String>) -> Response {
    let Ok(st) = StationId::from_hex(&id) else { return StatusCode::NOT_FOUND.into_response() };
    if !w.allowed(&st) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let Some(v) = w.views.get(&w.inner, &st).await else { return StatusCode::NOT_FOUND.into_response() };
    let t = now_ms();
    if v.live_tail(t).is_some() {
        return axum::Json(json!({ "live": true, "title": "ON AIR" })).into_response();
    }
    // Worked out at most once a second per station, however many ask.
    if let Some((at, body)) = v.playing.lock().unwrap().as_ref() {
        if t.saturating_sub(*at) < 1_000 {
            return ([(header::CACHE_CONTROL, "no-store")], axum::Json(body.clone())).into_response();
        }
    }
    let np = v.clock.as_ref().and_then(|c| c.lock().unwrap().now(t));
    let body = match np.and_then(|np| v.tracks.get(&np.track).map(|tr| (np, tr))) {
        Some((np, tr)) => {
            let pic = tr.art.map(|h| crate::hex(&h));
            json!({ "live": false, "title": crate::shown(&tr.title), "by": crate::shown(&tr.by), "art": pic, "position_ms": np.offset_ms, "duration_ms": np.duration_ms })
        }
        None => json!({ "live": false, "title": "", "by": "" }),
    };
    *v.playing.lock().unwrap() = Some((t, body.clone()));
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
/// `PYRATE_PUBLIC_URL` settles it outright. Otherwise the request's own Host, and the proxy's
/// forwarded headers only when `PYRATE_TRUST_PROXY=1` says a proxy in front sets them, so a
/// stranger reaching the Lantern directly cannot put a host of their choosing into its feeds.
/// Only a plain host name and http or https are taken from the request.
fn base(h: &HeaderMap) -> String {
    if let Ok(u) = std::env::var("PYRATE_PUBLIC_URL") {
        let u = u.trim().trim_end_matches('/');
        if (u.starts_with("https://") || u.starts_with("http://")) && u.len() <= 255 && !u.contains(['"', '<', '>', ' ']) {
            return u.to_string();
        }
    }
    let trust = std::env::var("PYRATE_TRUST_PROXY").is_ok_and(|v| v == "1");
    let forwarded = |k: &str| if trust { h.get(k) } else { None };
    let host = forwarded("x-forwarded-host").or(h.get(header::HOST)).and_then(|v| v.to_str().ok()).unwrap_or("localhost");
    let host = if !host.is_empty() && host.len() <= 255 && host.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':' | '[' | ']')) { host } else { "localhost" };
    let local = host.starts_with("127.") || host.starts_with("localhost") || host.starts_with("[::1]");
    let proto = match forwarded("x-forwarded-proto").and_then(|v| v.to_str().ok()) {
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
    let Some(v) = w.views.get(&w.inner, &st).await else { return StatusCode::NOT_FOUND.into_response() };
    let base = xml(&base(&h));
    if let Some((b, body)) = v.feed.lock().unwrap().as_ref() {
        if *b == base {
            return ([(header::CONTENT_TYPE, "application/rss+xml; charset=utf-8"), (header::CACHE_CONTROL, "max-age=300")], body.clone()).into_response();
        }
    }
    let link = format!("{}#{}", pyrate_core::open_player(), st.to_hex());
    let cover = v.cover.map(|c| format!("{base}/art/{}.jpg", crate::hex(&c)));
    let mut items = String::new();
    for h in v.library.iter().rev() {
        let Some(tr) = v.tracks.get(h) else { continue };
        // The bell is the station's ident, not an episode.
        if tr.bell {
            continue;
        }
        if items.len() > 1 << 20 || items.matches("<item>").count() >= 100 {
            break;
        }
        let ms = tr.duration_ms;
        let kbps = if tr.channels == 1 { 64 } else { 128 };
        let art = tr.art.map(|r| format!("<itunes:image href=\"{base}/art/{}.jpg\"/>", crate::hex(&r))).unwrap_or_default();
        items.push_str(&format!(
            "<item><title>{}</title><itunes:author>{}</itunes:author><guid isPermaLink=\"false\">{}</guid><pubDate>{}</pubDate><enclosure url=\"{base}/episode/{}/{}.mp3\" length=\"{}\" type=\"audio/mpeg\"/><itunes:duration>{}</itunes:duration>{art}</item>\n",
            xml(&tr.title), xml(&tr.by), crate::hex(h), rfc822(tr.added_ms), st.to_hex(), crate::hex(h), ms * kbps / 8, ms / 1000,
        ));
    }
    let (name, about) = (&v.name, &v.about);
    let image = cover.map(|c| format!("<itunes:image href=\"{c}\"/><image><url>{c}</url><title>{}</title><link>{}</link></image>", xml(name), xml(&link))).unwrap_or_default();
    let body = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<rss version=\"2.0\" xmlns:itunes=\"http://www.itunes.com/dtds/podcast-1.0.dtd\"><channel><title>{}</title><link>{}</link><description>{}</description><itunes:author>{}</itunes:author>{image}<generator>Pyrate Radio</generator>\n{items}</channel></rss>\n",
        xml(name), xml(&link), xml(if about.is_empty() { "A station on Pyrate Radio" } else { about }), xml(name),
    );
    *v.feed.lock().unwrap() = Some((base, body.clone()));
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
    let Ok(th) = <[u8; 32]>::try_from(th.as_slice()) else { return StatusCode::NOT_FOUND.into_response() };
    let Some(v) = w.views.get(&w.inner, &st).await else { return StatusCode::NOT_FOUND.into_response() };
    let Some(tr) = v.tracks.get(&th).filter(|t| t.in_library) else { return StatusCode::NOT_FOUND.into_response() };
    if tr.duration_ms > MAX_EPISODE_MS || !(1..=2).contains(&tr.channels) {
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
            let Some((chunks, channels)) = chunks_at(&w.inner, &st, tr.at) else { return StatusCode::NOT_FOUND.into_response() };
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
        let pcm = dec.decode_as(&bytes, c.frames);
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

/// The folder the open player is served from: `--player` on the Lantern, or PYRATE_PLAYER.
static PLAYER: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();

/// Serve the open player from this folder (browser/www/dist-open, or the image's copy).
pub fn set_player(dir: std::path::PathBuf) {
    let _ = PLAYER.set(dir);
}

fn player_dir() -> Option<std::path::PathBuf> {
    PLAYER.get().cloned().or_else(|| std::env::var_os("PYRATE_PLAYER").map(Into::into)).filter(|d| d.join("index.html").is_file())
}

/// Whether this gangway has the open player to serve.
pub fn has_player() -> bool {
    player_dir().is_some()
}

async fn listen_or_player(w: State<Web>, Path(path): Path<String>) -> Response {
    let id = path.trim_end_matches(".mp3");
    if id.len() == 64 && id.bytes().all(|b| b.is_ascii_hexdigit()) {
        return listen(w, Path(path)).await;
    }
    player(path).await
}

fn player_moved(path: &str) -> Response {
    // Only plain names go into the new address: nothing that could point it anywhere else.
    let plain = path.split('/').all(|seg| !seg.is_empty() && !seg.starts_with('.') && seg.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-')));
    if !plain {
        return StatusCode::NOT_FOUND.into_response();
    }
    axum::response::Redirect::permanent(&format!("/listen/{path}")).into_response()
}

/// The open player, the browser radio that plays every station and raises new ones. Any
/// Lantern that serves it is one more copy no one can close by closing a domain. It finds the
/// network by the doors and relays built into it, so it plays the same from any address.
async fn player(path: String) -> Response {
    let Some(dir) = player_dir() else { return StatusCode::NOT_FOUND.into_response() };
    let Some(file) = player_path(&dir, &path) else { return StatusCode::NOT_FOUND.into_response() };
    let Ok(bytes) = tokio::fs::read(&file).await else { return StatusCode::NOT_FOUND.into_response() };
    let ext = file.extension().and_then(|e| e.to_str()).unwrap_or("");
    let kind = player_type(ext);
    // The page is handed the doors this node knows now, beside those built into it. The player
    // takes them as one source among several (never a guard on this node's word alone), and a
    // signed copy's check of its own files leaves this line out.
    let bytes = if ext == "html" && path.trim_end_matches('/').rsplit('/').next().is_none_or(|f| f.is_empty() || f == "index.html") {
        let d = doors_hex(8);
        match std::str::from_utf8(&bytes) {
            Ok(t) if !d.is_empty() && t.contains("<meta charset=\"utf-8\">") => t.replacen("<meta charset=\"utf-8\">", &format!("<meta charset=\"utf-8\">\n<meta name=\"pyrate-doors\" content=\"{}\">", d.join(",")), 1).into_bytes(),
            _ => bytes,
        }
    } else {
        bytes
    };
    (
        [
            (header::CONTENT_TYPE, kind),
            // The page is looked at afresh each visit; everything it loads is named by version.
            (header::CACHE_CONTROL, if ext == "html" || ext == "webmanifest" { "no-cache" } else { "max-age=600" }),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            (header::REFERRER_POLICY, "no-referrer"),
            (header::CONTENT_SECURITY_POLICY, "frame-ancestors 'self'"),
        ],
        bytes,
    )
        .into_response()
}

fn player_type(ext: &str) -> &'static str {
    match ext {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "wasm" => "application/wasm",
        "json" => "application/json",
        "webmanifest" => "application/manifest+json",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "css" => "text/css; charset=utf-8",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
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
// The HOSTIS skull, the icon every HOSTIS page wears. The two older marks stay for pages cached before it.
const SKULL: &[u8] = include_bytes!("web/skull.png");

async fn brand(Path(name): Path<String>) -> Response {
    let body = match name.as_str() {
        "wordmark.png" => WORDMARK,
        "mark.png" => MARK,
        "skull.png" => SKULL,
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
    if !w.allowed(&st) || !w.inner.store.lock().unwrap().logs.contains_key(&st) {
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
                tokio::spawn(tower(w.inner.clone(), w.towers.clone(), w.views.clone(), st, tx));
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
async fn tower(inner: Arc<Inner>, towers: Towers, views: Arc<Views>, st: StationId, tx: broadcast::Sender<Bytes>) {
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
        let Some(v) = views.get(&inner, &st).await else {
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
        let (pcm, ch) = if let Some(tail) = v.live_tail(t) {
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
            (d.decode_as(&bytes, chunk.frames), ch)
        } else {
            if live_seq.take().is_some() {
                playhead = None;
                decoder = None;
            }
            let Some(clock) = v.clock.as_ref() else { break };
            let at = playhead.unwrap_or(t);
            let np = clock.lock().unwrap().now(at);
            let Some(np) = np else {
                // Off the air: the tower waits for the station to come back.
                dry = 0;
                tokio::time::sleep(Duration::from_secs(2)).await;
                continue;
            };
            let Some(tr) = v.tracks.get(&np.track) else { break };
            let Some((chunk, channels)) = chunk_at(&inner, &st, tr.at, np.chunk) else { continue };
            let channels = &channels;
            let Ok(bytes) = inner.chunk(&st, &chunk.hash).await else {
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            };
            if track != Some(np.track) || decoder.is_none() {
                decoder = ChunkDecoder::new(*channels).ok().map(|d| (*channels, d));
                track = Some(np.track);
            }
            let Some((_, d)) = decoder.as_mut() else { continue };
            let pcm = d.decode_as(&bytes, chunk.frames);
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
