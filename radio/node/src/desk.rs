//! The live desk: a show made on this device. A microphone, a deck of music, six pads and a
//! second input, mixed at 48 kHz, held under full scale by a limiter that looks ahead, and
//! handed to the encoder as it is made, as the browser's desk does it (desk-worklet.js).
//!
//! The mixing has a thread of its own, paced by the clock, so a busy app never leaves a gap in
//! the air; the encoder has another, so signing a chunk into the log never holds up the sound.
//! The deck reads each track a few seconds ahead of the playhead, so an hour-long set is never
//! held in memory. Nothing of it reaches the speaker unless the keeper asks to hear the mix.
//!
//! A show is talk (mono, 24 kbps) while only a voice is on it. The deck, a pad or a second
//! input makes it music (stereo, 64 kbps): from the start if one is already playing, or from
//! the moment one is first used, by a fresh encoder at the edge of a frame. No rights reserved.

use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, Ordering::Relaxed},
        mpsc,
    },
    time::{Duration, Instant},
};

use anyhow::{Context, Result, anyhow, bail};
use pyrate_core::{Body, Chunk, Profile, StationId, voice::Voice};
use rubato::{
    Async, FixedAsync, Indexing, Resampler, SincInterpolationParameters, SincInterpolationType,
    WindowFunction, audioadapter_buffers::direct::InterleavedSlice,
};
use serde_json::{Value, json};
use symphonia::core::{
    codecs::audio::{AudioDecoder, AudioDecoderOptions},
    errors::Error as SymErr,
    formats::{FormatOptions, FormatReader, TrackType, probe::Hint},
    io::MediaSourceStream,
    meta::MetadataOptions,
};

use crate::{Inner, now_ms, say, shown};

const RATE: f64 = 48_000.0;
/// How far ahead of the playhead the deck reads, in frames.
const AHEAD: usize = 3 * 48_000;
/// The longest sound a pad takes, with a little grace for a file that says thirty seconds.
const PAD_MAX: usize = 48_000 * 61 / 2;
/// A device's cushion: enough to ride out the bursts it delivers in, little enough not to lag.
const CUSHION: usize = 3_840;
const BAND: f64 = 1_440.0;
/// More than this queued means the thread stalled or the device ran fast for a long while.
const FLOOD: usize = 19_200;
/// The limiter looks 5 ms ahead.
const LA: usize = 240;

fn db(x: f64) -> f32 {
    10f64.powf(x / 20.0) as f32
}

/// A one-pole glide's step for a time constant in seconds.
fn coef(tau: f64) -> f32 {
    (1.0 - (-1.0 / (tau * RATE)).exp()) as f32
}

// ---- Reading files ------------------------------------------------------------------------

/// A file read as it plays: decoded by symphonia a packet at a time, folded to one or two
/// channels and brought to 48 kHz.
pub(crate) struct Reader {
    format: Box<dyn FormatReader>,
    dec: Box<dyn AudioDecoder>,
    track: u32,
    ch: usize,
    rs: Option<Async<f32>>,
    started: bool,
    delay: usize,
    raw: Vec<f32>,
    mapped: Vec<f32>,
    out: Vec<f32>,
    /// The file's length as its header says, if it does.
    pub(crate) len_ms: Option<u64>,
    done: bool,
}

impl Reader {
    pub(crate) fn open(path: &Path, ch: usize) -> Result<Self> {
        let file = std::fs::File::open(path).context("that file cannot be opened")?;
        let mss = MediaSourceStream::new(Box::new(file), Default::default());
        let mut hint = Hint::new();
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            hint.with_extension(ext);
        }
        let format = symphonia::default::get_probe()
            .probe(&hint, mss, FormatOptions::default(), MetadataOptions::default())
            .map_err(|e| anyhow!("{e}"))?;
        let (track, params, frames) = {
            let t = format.default_track(TrackType::Audio).ok_or_else(|| anyhow!("no sound in that file"))?;
            let p = t.codec_params.as_ref().and_then(|p| p.audio()).ok_or_else(|| anyhow!("no sound in that file"))?.clone();
            (t.id, p, t.num_frames)
        };
        let len_ms = match (frames, params.sample_rate) {
            (Some(n), Some(r)) if r > 0 => Some(n * 1000 / r as u64),
            _ => None,
        };
        let dec = symphonia::default::get_codecs().make_audio_decoder(&params, &AudioDecoderOptions::default()).map_err(|e| anyhow!("{e}"))?;
        Ok(Self { format, dec, track, ch: ch.clamp(1, 2), rs: None, started: false, delay: 0, raw: Vec::new(), mapped: Vec::new(), out: Vec::new(), len_ms, done: false })
    }

    /// More sound, appended to `to` (interleaved, 48 kHz). False once the file is done.
    pub(crate) fn more(&mut self, to: &mut Vec<f32>) -> Result<bool> {
        if self.done {
            return Ok(false);
        }
        loop {
            let packet = match self.format.next_packet() {
                Ok(Some(p)) => p,
                Ok(None) => break,
                Err(SymErr::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(anyhow!("{e}")),
            };
            if packet.track_id != self.track {
                continue;
            }
            let buf = match self.dec.decode(&packet) {
                Ok(b) => b,
                Err(SymErr::DecodeError(_)) => continue,
                Err(e) => return Err(anyhow!("{e}")),
            };
            let n = buf.spec().channels().count();
            if n == 0 {
                continue;
            }
            let rate = buf.spec().rate();
            self.raw.clear();
            self.raw.resize(buf.samples_interleaved(), 0.0);
            buf.copy_to_slice_interleaved(&mut self.raw);
            if !self.started {
                self.started = true;
                if rate != 48_000 {
                    let params = SincInterpolationParameters::new(128, WindowFunction::Blackman2)
                        .oversampling_factor(256)
                        .interpolation(SincInterpolationType::Quadratic);
                    let r = Async::<f32>::new_sinc(RATE / rate as f64, 1.1, &params, 1024, self.ch, FixedAsync::Input).map_err(|e| anyhow!("{e}"))?;
                    self.delay = r.output_delay();
                    self.rs = Some(r);
                }
            }
            for f in self.raw.chunks_exact(n) {
                let (l, r) = if n == 1 { (f[0], f[0]) } else { (f[0], f[1]) };
                if self.ch == 1 {
                    self.mapped.push((l + r) * 0.5);
                } else {
                    self.mapped.push(l);
                    self.mapped.push(r);
                }
            }
            self.drain(false, to)?;
            return Ok(true);
        }
        self.drain(true, to)?;
        self.done = true;
        Ok(false)
    }

    /// Runs what is mapped through the resampler, or straight on at 48 kHz.
    fn drain(&mut self, last: bool, to: &mut Vec<f32>) -> Result<()> {
        let ch = self.ch;
        let Some(r) = self.rs.as_mut() else {
            to.append(&mut self.mapped);
            return Ok(());
        };
        loop {
            let have = self.mapped.len() / ch;
            let need = r.input_frames_next();
            if have < need && !(last && have > 0) {
                break;
            }
            let take = have.min(need);
            if take < need {
                // The resampler reads a full block; the tail is padded and marked partial.
                self.mapped.resize(need * ch, 0.0);
            }
            let have = self.mapped.len() / ch;
            let cap = r.output_frames_next();
            self.out.clear();
            self.out.resize(cap * ch, 0.0);
            let input = InterleavedSlice::new(&self.mapped[..], ch, have).map_err(|e| anyhow!("{e}"))?;
            let mut output = InterleavedSlice::new_mut(&mut self.out[..], ch, cap).map_err(|e| anyhow!("{e}"))?;
            let mut idx = Indexing::new();
            if take < need {
                idx.partial_len = Some(take);
            }
            let (used, made) = r.process_into_buffer(&input, &mut output, Some(&idx)).map_err(|e| anyhow!("{e}"))?;
            let used = if take < need { have } else { used };
            self.mapped.drain(..used * ch);
            let skip = self.delay.min(made);
            self.delay -= skip;
            to.extend_from_slice(&self.out[skip * ch..made * ch]);
            if take < need {
                break;
            }
        }
        Ok(())
    }
}

/// A pad's sound, whole: stereo 16-bit at 48 kHz, thirty seconds at most.
fn read_pad(path: &Path) -> Result<Vec<i16>> {
    let mut rd = Reader::open(path, 2).map_err(|_| anyhow!("That sound would not load."))?;
    let mut pcm = Vec::new();
    loop {
        let more = rd.more(&mut pcm).map_err(|_| anyhow!("That sound would not load."))?;
        if pcm.len() > PAD_MAX * 2 {
            bail!("Pads take up to thirty seconds.");
        }
        if !more {
            break;
        }
    }
    if pcm.is_empty() {
        bail!("That sound would not load.");
    }
    Ok(pcm.iter().map(|s| (s.clamp(-1.0, 1.0) * 32767.0) as i16).collect())
}

/// One deck track read ahead of the playhead on a thread of its own.
#[derive(Default)]
struct Ring {
    pcm: VecDeque<f32>,
    eof: bool,
    bad: bool,
}

struct Stream {
    id: u64,
    ring: Arc<Mutex<Ring>>,
    stop: Arc<AtomicBool>,
}

impl Stream {
    /// Reads a track from `from` frames in: a paused track picks up where it stopped.
    fn read(id: u64, path: PathBuf, from: u64) -> Self {
        let ring = Arc::new(Mutex::new(Ring::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let (r2, s2) = (ring.clone(), stop.clone());
        let _ = std::thread::Builder::new().name("deck".into()).spawn(move || {
            let mut rd = match Reader::open(&path, 2) {
                Ok(r) => r,
                Err(_) => {
                    let mut r = r2.lock().unwrap();
                    r.bad = true;
                    r.eof = true;
                    return;
                }
            };
            let (mut buf, mut made, mut skip) = (Vec::new(), 0usize, from as usize * 2);
            while !s2.load(Relaxed) {
                if skip == 0 && r2.lock().unwrap().pcm.len() >= AHEAD * 2 {
                    std::thread::sleep(Duration::from_millis(25));
                    continue;
                }
                buf.clear();
                let more = rd.more(&mut buf);
                made += buf.len();
                let k = skip.min(buf.len());
                skip -= k;
                let mut r = r2.lock().unwrap();
                r.pcm.extend(buf[k..].iter());
                match more {
                    Ok(true) => {}
                    // A file that breaks partway plays as far as it goes.
                    Ok(false) | Err(_) => {
                        r.bad = made == 0;
                        r.eof = true;
                        return;
                    }
                }
            }
        });
        Self { id, ring, stop }
    }
}

impl Drop for Stream {
    fn drop(&mut self) {
        self.stop.store(true, Relaxed);
    }
}

// ---- The desk's state, as the app sets it ------------------------------------------------

struct Item {
    id: u64,
    path: PathBuf,
    title: String,
    by: String,
    len_ms: Option<u64>,
    bad: bool,
}

struct Pad {
    name: String,
    pcm: Arc<Vec<i16>>,
    at: Option<usize>,
    /// Fading out, after a second tap.
    stop: bool,
    fade: f32,
}

pub(crate) struct Shared {
    running: bool,
    checking: bool,
    hear: bool,
    /// How far the deck drops while the microphone is open; 1 leaves it up.
    duck: f32,
    each: bool,
    /// The last thing worth saying, and when: shown for a few seconds.
    note: Option<(String, Instant)>,
    disguise: f64,
    mic_open: bool,
    mic_gain: f32,
    mic_dev: Option<String>,
    /// For tests: a file standing in for the microphone, played round and round.
    mic_file: Option<PathBuf>,
    mic_on: bool,
    mic_err: Option<String>,
    /// Bumped to ask for another try at a device that would not open.
    mic_tries: u64,
    mic_seen: u64,
    two_dev: Option<String>,
    /// For tests, as `mic_file`.
    two_file: Option<PathBuf>,
    two_open: bool,
    two_gain: f32,
    two_on: bool,
    two_err: Option<String>,
    two_tries: u64,
    list: Vec<Item>,
    at: Option<usize>,
    playing: bool,
    deck_gain: f32,
    /// Frames played of the track at hand, and which track and cue they belong to.
    pos: u64,
    pos_key: Option<(u64, u64)>,
    /// Bumped to start the track at hand from its beginning.
    cue: u64,
    /// Bumped whenever the list changes, so the app fetches it again only then.
    v: u64,
    next_id: u64,
    pads: [Option<Pad>; 6],
    pads_gain: f32,
    /// go_live's recording, standing in for the microphone; the show ends with it.
    file: Option<PathBuf>,
    music: bool,
    aired: u64,
}

impl Shared {
    fn new() -> Self {
        Self {
            running: false, checking: false, hear: false, duck: 0.25, each: false, note: None, disguise: 0.0,
            mic_open: true, mic_gain: 1.0, mic_dev: None, mic_file: None, mic_on: false, mic_err: None, mic_tries: 0, mic_seen: 0,
            two_dev: None, two_file: None, two_open: true, two_gain: 1.0, two_on: false, two_err: None, two_tries: 0,
            list: Vec::new(), at: None, playing: false, deck_gain: 1.0, pos: 0, pos_key: None, cue: 0, v: 0, next_id: 1,
            pads: Default::default(), pads_gain: 1.0, file: None, music: false, aired: 0,
        }
    }

    fn pads_playing(&self) -> bool {
        self.pads.iter().flatten().any(|p| p.at.is_some())
    }

    /// Whether anything beyond a voice is in play: the deck, a pad, or an open second input.
    fn in_play(&self) -> bool {
        self.playing || self.pads_playing() || (self.two_on && self.two_open)
    }

    /// The next track that will play after the one at hand.
    fn after(&self) -> Option<usize> {
        let from = self.at.map(|i| i + 1).unwrap_or(0);
        (from..self.list.len()).find(|&i| !self.list[i].bad)
    }

    /// On to the next track, playing it if `go`; past the last, nothing is cued.
    fn advance(&mut self, go: bool) {
        self.at = self.after();
        self.pos = 0;
        self.playing = go && self.at.is_some();
    }
}

#[derive(Default)]
struct Meters {
    mic: AtomicU32,
    two: AtomicU32,
    deck: AtomicU32,
    pads: AtomicU32,
    l: AtomicU32,
    r: AtomicU32,
}

fn level(a: &AtomicU32) -> f32 {
    f32::from_bits(a.load(Relaxed))
}

/// A meter's needle: up at once to a peak, back down at 24 dB a second.
fn mark(a: &AtomicU32, peak: f32, fall: f32) {
    a.store(peak.max(level(a) * fall).to_bits(), Relaxed);
}

fn r3(x: f32) -> f64 {
    (x as f64 * 1000.0).round() / 1000.0
}

pub(crate) struct Desk {
    pub(crate) sh: Mutex<Shared>,
    meters: Meters,
}

impl Desk {
    pub(crate) fn new() -> Self {
        Self { sh: Mutex::new(Shared::new()), meters: Meters::default() }
    }

    /// What the app shows: the inputs and the air with their levels, the deck and the pads,
    /// and the show. Null while the desk is idle and nothing is on the air, so the radio's own
    /// status stays light; the deck's list comes only when asked for.
    pub(crate) fn state(&self, inner: &Inner, list: bool, always: bool) -> Value {
        let on = *inner.live.lock().unwrap();
        let sh = self.sh.lock().unwrap();
        if on.is_none() && !always && !sh.running && sh.list.is_empty() && sh.pads.iter().all(|p| p.is_none()) {
            return Value::Null;
        }
        let m = &self.meters;
        let it = sh.at.and_then(|i| sh.list.get(i));
        let note = sh.note.as_ref().filter(|(_, at)| at.elapsed() < Duration::from_secs(8)).map(|(t, _)| t.as_str());
        let mut v = json!({
            "music": sh.music, "checking": sh.checking, "hear": sh.hear, "duck": r3(sh.duck), "each": sh.each,
            "note": note, "disguise": sh.disguise != 0.0, "aired": sh.aired,
            "mic": { "open": sh.mic_open, "on": sh.mic_on, "gain": r3(sh.mic_gain), "dev": sh.mic_dev, "err": sh.mic_err, "level": r3(level(&m.mic)) },
            "two": { "dev": sh.two_dev, "on": sh.two_on, "open": sh.two_open, "gain": r3(sh.two_gain), "err": sh.two_err, "level": r3(level(&m.two)) },
            "deck": {
                "n": sh.list.len(), "v": sh.v, "at": sh.at, "playing": sh.playing, "each": sh.each,
                "title": it.map(|x| x.title.as_str()), "by": it.map(|x| x.by.as_str()),
                "pos_ms": sh.pos / 48, "len_ms": it.and_then(|x| x.len_ms), "gain": r3(sh.deck_gain), "level": r3(level(&m.deck)),
            },
            "pads": sh.pads.iter().map(|p| p.as_ref().map(|p| json!({
                "name": p.name, "playing": p.at.is_some() && !p.stop,
                "pos": p.at.map(|a| r3(a as f32 / (p.pcm.len() / 2).max(1) as f32)).unwrap_or(0.0),
                "len_ms": p.pcm.len() as u64 / 96,
            }))).collect::<Vec<_>>(),
            "pads_gain": r3(sh.pads_gain), "pads_level": r3(level(&m.pads)),
            "air": [r3(level(&m.l)), r3(level(&m.r))],
        });
        if let Some((st, t)) = on {
            v["station"] = json!(st.to_hex());
            v["since_ms"] = json!(now_ms().saturating_sub(t));
            if let Some(n) = inner.log.lock().unwrap().get(&st).map(|l| l.aboard(now_ms())) {
                v["aboard"] = json!(n);
            }
        }
        if list {
            v["list"] = json!(sh.list.iter().map(|x| json!({ "title": x.title, "by": x.by, "len_ms": x.len_ms, "bad": x.bad })).collect::<Vec<_>>());
        }
        v
    }

    /// Readies a show: go_live's recording if it gave one, the disguise as set, and another try
    /// at the microphone unless it is already open. The try's number, for `mic_answer`.
    pub(crate) fn ready(&self, file: Option<PathBuf>, semitones: f64) -> Option<u64> {
        let mut sh = self.sh.lock().unwrap();
        let again = file.is_some() || sh.file.is_some() || !sh.mic_on;
        sh.file = file;
        sh.disguise = semitones;
        sh.note = None;
        if !again {
            return None;
        }
        sh.mic_tries += 1;
        Some(sh.mic_tries)
    }

    /// Waits for the desk's thread to try the microphone: Ok once it is open, or what went wrong.
    pub(crate) fn mic_answer(&self, tries: u64, within: Duration) -> Result<(), String> {
        let t = Instant::now();
        while t.elapsed() < within {
            {
                let sh = self.sh.lock().unwrap();
                if sh.mic_seen >= tries {
                    return if sh.mic_on { Ok(()) } else { Err(sh.mic_err.clone().unwrap_or_else(|| "no microphone".into())) };
                }
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        Err("the microphone did not answer".into())
    }

    /// Whether a show could go out without a voice: music on the deck, a pad, a second input.
    pub(crate) fn has_music(&self) -> bool {
        let sh = self.sh.lock().unwrap();
        !sh.list.is_empty() || sh.pads.iter().any(|p| p.is_some()) || sh.two_dev.is_some() || sh.two_file.is_some()
    }

    pub(crate) fn say(&self, note: &str) {
        self.sh.lock().unwrap().note = Some((note.to_string(), Instant::now()));
    }
}

/// Starts the desk's thread if it is not running and something needs it: a show, levels being
/// checked, the deck or a pad playing, or the mix being heard.
pub(crate) fn kick(inner: &Arc<Inner>) {
    let mut sh = inner.desk.sh.lock().unwrap();
    if sh.running {
        return;
    }
    if !(sh.checking || sh.playing || sh.pads_playing() || sh.hear || inner.live.lock().unwrap().is_some()) {
        return;
    }
    sh.running = true;
    let i2 = inner.clone();
    if std::thread::Builder::new().name("desk".into()).spawn(move || run(i2)).is_err() {
        sh.running = false;
    }
}

fn gain(v: &Value, top: f32) -> Option<f32> {
    v.as_f64().map(|g| (g as f32).clamp(0.0, top))
}

/// A field that names something or, given as null or empty, nothing; None when it is absent.
fn named(req: &Value, k: &str) -> Option<Option<String>> {
    req.get(k).map(|v| v.as_str().filter(|s| !s.is_empty()).map(String::from))
}

/// The desk's commands, from the app.
pub(crate) fn call(inner: &Arc<Inner>, cmd: &str, req: &Value) -> Result<Value> {
    let desk = &inner.desk;
    // Gains are linear, up to ten decibels over level.
    const TOP: f32 = 3.17;
    match cmd {
        "desk" => {}
        "desk_devices" => {
            #[cfg(feature = "playback")]
            let names = crate::player::input_names();
            #[cfg(not(feature = "playback"))]
            let names: Vec<String> = Vec::new();
            return Ok(json!({ "inputs": names }));
        }
        "desk_set" => {
            let wants_check = req["check"].as_bool() == Some(true);
            let semis = if wants_check { Some(inner.disguise()) } else { None };
            let mut sh = desk.sh.lock().unwrap();
            if let Some(b) = req["mic_open"].as_bool() {
                sh.mic_open = b;
            }
            if let Some(g) = gain(&req["mic_gain"], TOP) {
                sh.mic_gain = g;
            }
            if let Some(d) = named(req, "mic_dev") {
                if d != sh.mic_dev {
                    sh.mic_dev = d;
                    sh.mic_tries += 1;
                }
            }
            // For tests, where there is no microphone: a file in its place, round and round.
            if let Some(p) = named(req, "mic_file") {
                sh.mic_file = p.map(PathBuf::from);
                sh.mic_tries += 1;
            }
            if let Some(s) = req["disguise"].as_f64() {
                sh.disguise = s.clamp(-12.0, 12.0);
            }
            if let Some(d) = named(req, "two_dev") {
                sh.two_dev = d;
                sh.two_tries += 1;
                // A second input opens the desk's ears, as checking levels does.
                if sh.two_dev.is_some() && inner.live.lock().unwrap().is_none() {
                    sh.checking = true;
                }
            }
            if let Some(p) = named(req, "two_file") {
                sh.two_file = p.map(PathBuf::from);
                sh.two_tries += 1;
            }
            if let Some(b) = req["two_open"].as_bool() {
                sh.two_open = b;
            }
            if let Some(g) = gain(&req["two_gain"], TOP) {
                sh.two_gain = g;
            }
            if let Some(g) = gain(&req["deck_gain"], TOP) {
                sh.deck_gain = g;
            }
            if let Some(g) = gain(&req["pads_gain"], TOP) {
                sh.pads_gain = g;
            }
            if let Some(d) = gain(&req["duck"], 1.0) {
                sh.duck = d;
            }
            if let Some(b) = req["each"].as_bool() {
                sh.each = b;
            }
            if let Some(b) = req["hear"].as_bool() {
                sh.hear = b;
            }
            if let Some(b) = req["check"].as_bool() {
                if b && inner.live.lock().unwrap().is_none() {
                    sh.checking = true;
                    sh.mic_tries += 1;
                    sh.note = None;
                    if let Some(s) = semis {
                        sh.disguise = s;
                    }
                } else if !b {
                    sh.checking = false;
                }
            }
        }
        "deck_add" => {
            let files: Vec<Value> = match req["files"].as_array() {
                Some(a) => a.clone(),
                None => vec![req.clone()],
            };
            let mut probe = Vec::new();
            let mut sh = desk.sh.lock().unwrap();
            for f in files.iter().take(500) {
                let Some(p) = f["path"].as_str() else { continue };
                let path = PathBuf::from(p);
                let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                let title = f["title"].as_str().filter(|t| !t.trim().is_empty()).map(String::from).unwrap_or(stem);
                let id = sh.next_id;
                sh.next_id += 1;
                sh.list.push(Item { id, path: path.clone(), title: shown(title.trim()), by: shown(f["by"].as_str().unwrap_or("").trim()), len_ms: None, bad: false });
                probe.push((id, path));
            }
            sh.v += 1;
            drop(sh);
            // Each track's length, from its header, on a thread of its own: an hour's MP3 is
            // not read through to learn it.
            let i2 = inner.clone();
            std::thread::spawn(move || {
                for (id, path) in probe {
                    let got = Reader::open(&path, 2).map(|r| r.len_ms);
                    let mut sh = i2.desk.sh.lock().unwrap();
                    if let Some(x) = sh.list.iter_mut().find(|x| x.id == id) {
                        match got {
                            Ok(l) => x.len_ms = l,
                            Err(_) => x.bad = true,
                        }
                    }
                    sh.v += 1;
                }
            });
        }
        "deck_play" => {
            let mut sh = desk.sh.lock().unwrap();
            if let Some(i) = req["at"].as_u64().map(|i| i as usize) {
                if sh.list.get(i).is_none_or(|x| x.bad) {
                    bail!("That track will not play.");
                }
                sh.at = Some(i);
                sh.cue += 1;
                sh.pos = 0;
            } else if sh.at.is_none() {
                sh.at = sh.after();
                sh.pos = 0;
            }
            sh.playing = sh.at.is_some();
        }
        "deck_pause" => desk.sh.lock().unwrap().playing = false,
        "deck_next" => {
            let mut sh = desk.sh.lock().unwrap();
            let go = sh.playing;
            sh.advance(go);
        }
        "deck_remove" => {
            let mut sh = desk.sh.lock().unwrap();
            let i = req["at"].as_u64().context("at")? as usize;
            if i >= sh.list.len() {
                bail!("no such track");
            }
            let gone = sh.list.remove(i);
            match sh.at {
                // The track at hand goes: the one after it waits, cued.
                Some(a) if a == i => {
                    sh.at = (i..sh.list.len()).find(|&k| !sh.list[k].bad);
                    sh.playing = false;
                    sh.pos = 0;
                    sh.cue += 1;
                }
                Some(a) if a > i => sh.at = Some(a - 1),
                _ => {}
            }
            sh.v += 1;
            drop(sh);
            return Ok(json!({ "ok": true, "path": gone.path.to_string_lossy() }));
        }
        "pad_load" => {
            let slot = req["slot"].as_u64().context("slot")? as usize;
            let path = PathBuf::from(req["path"].as_str().context("path")?);
            if slot >= 6 {
                bail!("there are six pads");
            }
            let pcm = read_pad(&path)?;
            let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            let name = req["name"].as_str().filter(|t| !t.trim().is_empty()).map(String::from).unwrap_or(stem);
            desk.sh.lock().unwrap().pads[slot] = Some(Pad { name: shown(name.trim()), pcm: Arc::new(pcm), at: None, stop: false, fade: 1.0 });
        }
        "pad_fire" => {
            let slot = req["slot"].as_u64().context("slot")? as usize;
            let mut sh = desk.sh.lock().unwrap();
            let Some(p) = sh.pads.get_mut(slot).and_then(|p| p.as_mut()) else { bail!("that pad is empty") };
            // A tap fires it; a tap while it plays stops it, quickly but without a click.
            if p.at.is_some() && !p.stop {
                p.stop = true;
            } else {
                p.at = Some(0);
                p.stop = false;
                p.fade = 1.0;
            }
        }
        "pad_clear" => {
            let slot = req["slot"].as_u64().context("slot")? as usize;
            if let Some(p) = desk.sh.lock().unwrap().pads.get_mut(slot) {
                *p = None;
            }
        }
        other => bail!("unknown command {other}"),
    }
    // Only what sounds wakes the desk's thread: a look at the desk never does.
    if matches!(cmd, "desk_set" | "deck_play" | "pad_fire") {
        kick(inner);
    }
    Ok(desk.state(inner, req["list"].as_bool().unwrap_or(cmd == "desk"), true))
}

// ---- The inputs ------------------------------------------------------------------------------

#[derive(Clone, PartialEq)]
enum Want {
    File(PathBuf, bool),
    Dev(Option<String>),
}

enum Source {
    Off,
    /// A file standing in for a device: a test's tone, or go_live's recording in real time.
    File { rd: Reader, path: PathBuf, buf: VecDeque<f32>, looped: bool, done: bool },
    #[cfg(feature = "playback")]
    Device { _input: crate::player::Input, buf: Arc<Mutex<Vec<i16>>>, fifo: VecDeque<f32>, primed: bool, fill: f64, nth: u32, heard: Instant },
}

impl Source {
    fn open(w: &Want, ch: usize) -> (Self, Option<String>) {
        match w {
            Want::File(p, looped) => match Reader::open(p, ch) {
                Ok(rd) => (Source::File { rd, path: p.clone(), buf: VecDeque::new(), looped: *looped, done: false }, None),
                Err(e) => (Source::Off, Some(format!("That file would not play ({e}).")))
            },
            #[cfg(feature = "playback")]
            Want::Dev(name) => {
                let buf = Arc::new(Mutex::new(Vec::new()));
                match crate::player::open_input_from(name.as_deref(), ch, buf.clone()) {
                    Ok(i) => (Source::Device { _input: i, buf, fifo: VecDeque::new(), primed: false, fill: CUSHION as f64, nth: 0, heard: Instant::now() }, None),
                    Err(e) => (Source::Off, Some(e.to_string())),
                }
            }
            #[cfg(not(feature = "playback"))]
            Want::Dev(_) => (Source::Off, Some("this build has no microphone".into())),
        }
    }

    fn on(&self) -> bool {
        !matches!(self, Source::Off)
    }

    /// `n` frames, `ch` wide, into `out`. False once the source has gone: a recording that does
    /// not loop has run out, or a device has said nothing for two seconds.
    fn take(&mut self, n: usize, ch: usize, out: &mut Vec<f32>) -> bool {
        out.clear();
        let want = n * ch;
        let alive = match self {
            Source::Off => true,
            Source::File { rd, path, buf, looped, done } => {
                let mut rounds = 0;
                while buf.len() < want && !*done {
                    let mut v = Vec::new();
                    match rd.more(&mut v) {
                        Ok(true) => buf.extend(v),
                        Ok(false) => {
                            buf.extend(v);
                            // Round again from the top; a file with no sound in it stops here.
                            rounds += 1;
                            match (*looped && rounds < 3).then(|| Reader::open(path, ch)) {
                                Some(Ok(r)) => *rd = r,
                                _ => *done = true,
                            }
                        }
                        Err(_) => *done = true,
                    }
                }
                let k = want.min(buf.len());
                out.extend(buf.drain(..k));
                !(*done && buf.is_empty())
            }
            #[cfg(feature = "playback")]
            Source::Device { buf, fifo, primed, fill, nth, heard, .. } => {
                {
                    let mut b = buf.lock().unwrap_or_else(|e| e.into_inner());
                    if !b.is_empty() {
                        *heard = Instant::now();
                    }
                    fifo.extend(b.drain(..).map(|s| s as f32 / 32768.0));
                }
                let frames = fifo.len() / ch;
                if frames > FLOOD {
                    fifo.drain(..(frames - CUSHION) * ch);
                }
                let frames = fifo.len() / ch;
                *fill = *fill * 0.9 + frames as f64 * 0.1;
                if !*primed && frames >= CUSHION {
                    *primed = true;
                }
                if *primed && frames < n {
                    // Ran dry: what there is, then quiet while the cushion builds again.
                    out.extend(fifo.drain(..));
                    *primed = false;
                } else if *primed {
                    // A device's clock runs a hair fast or slow against this one: one frame in
                    // five hundred is dropped or doubled to hold the cushion, too few to hear.
                    let fast = *fill > CUSHION as f64 + BAND;
                    let slow = *fill < CUSHION as f64 - BAND;
                    for _ in 0..n {
                        *nth += 1;
                        if *nth >= 500 {
                            *nth = 0;
                            if fast && fifo.len() >= 2 * ch {
                                fifo.drain(..ch);
                            } else if slow {
                                out.extend(fifo.iter().take(ch).copied().collect::<Vec<_>>());
                                continue;
                            }
                        }
                        for _ in 0..ch {
                            out.push(fifo.pop_front().unwrap_or(0.0));
                        }
                    }
                }
                heard.elapsed() < Duration::from_secs(2)
            }
        };
        out.resize(want, 0.0);
        alive
    }
}

// ---- The air -------------------------------------------------------------------------------

/// The ceiling stands three decibels under full scale, four while a show is talk: Opus rings
/// past a sharp edge by as much as two at music's rate and more at talk's. The gain each
/// sample needs is held at its lowest for 5 ms and averaged over 5 ms, and the sound is
/// delayed to match, so the gain is down before a peak arrives, never after.
struct Limiter {
    dl: [[f32; LA]; 2],
    dp: usize,
    q: VecDeque<(f32, u64)>,
    t: u64,
    r: f32,
    boxv: [f32; LA],
    bp: usize,
    sum: f64,
    rel: f32,
    ceil: f32,
}

impl Limiter {
    fn new() -> Self {
        Self { dl: [[0.0; LA]; 2], dp: 0, q: VecDeque::with_capacity(LA + 1), t: 0, r: 1.0, boxv: [1.0; LA], bp: 0, sum: LA as f64, rel: coef(0.15), ceil: db(-3.0) }
    }

    fn run(&mut self, l: f32, r: f32) -> (f32, f32) {
        let pk = l.abs().max(r.abs());
        let need = if pk > self.ceil { self.ceil / pk } else { 1.0 };
        let t = self.t;
        self.t += 1;
        if self.q.front().is_some_and(|&(_, ti)| ti + LA as u64 <= t) {
            self.q.pop_front();
        }
        while self.q.back().is_some_and(|&(v, _)| v >= need) {
            self.q.pop_back();
        }
        self.q.push_back((need, t));
        let h = self.q.front().map(|x| x.0).unwrap_or(1.0);
        self.r = if h < self.r { h } else { self.r + (h - self.r) * self.rel };
        self.sum += (self.r - self.boxv[self.bp]) as f64;
        self.boxv[self.bp] = self.r;
        self.bp = (self.bp + 1) % LA;
        let g = ((self.sum / LA as f64) as f32).min(1.0);
        self.dl[0][self.dp] = l;
        self.dl[1][self.dp] = r;
        self.dp = (self.dp + 1) % LA;
        let c = self.ceil;
        ((self.dl[0][self.dp] * g).clamp(-c, c), (self.dl[1][self.dp] * g).clamp(-c, c))
    }
}

enum Air {
    Pcm(Vec<i16>),
    Music,
    End,
}

struct Show {
    station: StationId,
    started: u64,
    tx: mpsc::Sender<Air>,
    music: bool,
}

/// Past this, a decoded frame is made again, quieter: a little under full scale, for decoders
/// that round a hair differently from this one.
const CODEC_CEIL: f32 = 0.93;
/// After a frame has been brought down, the level climbs back at 3 dB a second.
const RISE: f32 = 1.006_93;

struct Held {
    input: Vec<i16>,
    g0: f32,
    g1: f32,
    packet: Vec<u8>,
    enc: Vec<u64>,
    dec: Vec<u64>,
}

/// Opus as the listeners will hear it. A limiter ahead of the encoder keeps the air three
/// decibels down, but Opus keeps each band's energy and not its phase, so a few strong tones,
/// or a mix already squared off at the top, can come out of a listener's decoder three
/// decibels over what went in: past full scale. So each frame is encoded and at once decoded
/// again, and one that would come out too loud is made again, quieter, together with the
/// frame before it, whose last few milliseconds the encoder's lookahead carries into this one.
/// libopus keeps each state in one block that may be copied as it is, so going back a frame
/// is a copy. A frame waits for the next before it leaves: 20 ms more delay, and no clip.
struct Coder {
    ch: usize,
    enc: Vec<u64>,
    dec: Vec<u64>,
    held: Option<Held>,
    pkt: Vec<u8>,
    out: Vec<f32>,
    scaled: Vec<i16>,
    current: Vec<u8>,
    frames: u16,
}

impl Coder {
    /// As the recordings' encoder (pyrate_core::ChunkEncoder) sets it up: the same profile,
    /// constant bitrate, so a chunk's size says nothing of what plays.
    fn new(profile: Profile) -> Result<Self> {
        use opusic_sys as ox;
        let ch = profile.channels() as i32;
        let app = if profile == Profile::Music { ox::OPUS_APPLICATION_AUDIO } else { ox::OPUS_APPLICATION_VOIP };
        // SAFETY: each state is a block of the size libopus asks for, which it initialises
        // before anything else touches it.
        unsafe {
            let (es, ds) = (ox::opus_encoder_get_size(ch), ox::opus_decoder_get_size(ch));
            anyhow::ensure!(es > 0 && ds > 0, "opus would not say its size");
            let mut enc = vec![0u64; (es as usize).div_ceil(8)];
            let mut dec = vec![0u64; (ds as usize).div_ceil(8)];
            let e: *mut ox::OpusEncoder = enc.as_mut_ptr().cast();
            anyhow::ensure!(ox::opus_encoder_init(e, 48_000, ch, app) == ox::OPUS_OK, "the encoder would not start");
            anyhow::ensure!(ox::opus_decoder_init(dec.as_mut_ptr().cast(), 48_000, ch) == ox::OPUS_OK, "the decoder would not start");
            anyhow::ensure!(ox::opus_encoder_ctl(e, ox::OPUS_SET_BITRATE_REQUEST, profile.bitrate() as i32) == ox::OPUS_OK, "bitrate");
            anyhow::ensure!(ox::opus_encoder_ctl(e, ox::OPUS_SET_VBR_REQUEST, 0i32) == ox::OPUS_OK, "constant bitrate");
            Ok(Self { ch: ch as usize, enc, dec, held: None, pkt: vec![0; 4000], out: vec![0.0; 960 * ch as usize], scaled: Vec::new(), current: Vec::with_capacity(64 * 1024), frames: 0 })
        }
    }

    /// One frame, its gain running from `g0` to `g1`, encoded and decoded again: the packet and
    /// the loudest sample a listener would hear of it.
    fn code(&mut self, input: &[i16], g0: f32, g1: f32) -> (Vec<u8>, f32) {
        use opusic_sys as ox;
        let n = input.len() / self.ch;
        self.scaled.clear();
        for (i, f) in input.chunks_exact(self.ch).enumerate() {
            let g = g0 + (g1 - g0) * (i + 1) as f32 / n as f32;
            self.scaled.extend(f.iter().map(|s| (*s as f32 * g).round() as i16));
        }
        // SAFETY: the states were initialised in `new`, and every buffer is as long as the
        // length passed with it. A failed encode leaves an empty packet, which decoders conceal.
        let len = unsafe { ox::opus_encode(self.enc.as_mut_ptr().cast(), self.scaled.as_ptr(), n as i32, self.pkt.as_mut_ptr(), self.pkt.len() as i32) }.max(0) as usize;
        let packet = self.pkt[..len].to_vec();
        let got = unsafe { ox::opus_decode_float(self.dec.as_mut_ptr().cast(), packet.as_ptr(), len as i32, self.out.as_mut_ptr(), n as i32, 0) };
        let peak = self.out[..(got.max(0) as usize * self.ch).min(self.out.len())].iter().fold(0f32, |m, x| m.max(x.abs()));
        (packet, peak)
    }

    /// One 20 ms frame in (interleaved, the coder's channels); finished chunks out.
    fn frame(&mut self, input: Vec<i16>, out: &mut Vec<(Chunk, Vec<u8>)>) {
        let held = self.held.take();
        let start = held.as_ref().map(|h| h.g1).unwrap_or(1.0);
        let end = (start * RISE).min(1.0);
        let mut before = (self.enc.clone(), self.dec.clone());
        let (mut packet, mut peak) = self.code(&input, start, end);
        let (mut s, mut tries, mut again) = (1.0f32, 0, None);
        while peak > CODEC_CEIL && tries < 4 {
            tries += 1;
            s *= CODEC_CEIL / peak * 0.97;
            let mut worst = 0f32;
            match &held {
                Some(h) => {
                    self.enc.copy_from_slice(&h.enc);
                    self.dec.copy_from_slice(&h.dec);
                    let (p, pk) = self.code(&h.input, h.g0, start * s);
                    worst = pk;
                    again = Some(p);
                    before = (self.enc.clone(), self.dec.clone());
                }
                None => {
                    self.enc.copy_from_slice(&before.0);
                    self.dec.copy_from_slice(&before.1);
                }
            }
            let (p, pk) = self.code(&input, start * s, end * s);
            packet = p;
            peak = pk.max(worst);
        }
        if let Some(h) = held {
            self.add(again.unwrap_or(h.packet), out);
        }
        self.held = Some(Held { input, g0: start * s, g1: end * s, packet, enc: before.0, dec: before.1 });
    }

    fn add(&mut self, p: Vec<u8>, out: &mut Vec<(Chunk, Vec<u8>)>) {
        self.current.extend_from_slice(&(p.len() as u16).to_be_bytes());
        self.current.extend_from_slice(&p);
        self.frames += 1;
        if self.frames == pyrate_core::audio::FRAMES_PER_CHUNK {
            let b = std::mem::take(&mut self.current);
            out.push((Chunk::of(&b, self.frames), b));
            self.frames = 0;
        }
    }

    /// The frame still waiting, then what there is of the last chunk.
    fn finish(mut self, out: &mut Vec<(Chunk, Vec<u8>)>) {
        if let Some(h) = self.held.take() {
            self.add(h.packet, out);
        }
        if self.frames > 0 {
            let b = std::mem::take(&mut self.current);
            out.push((Chunk::of(&b, self.frames), b));
        }
    }
}

/// The encoder's thread: the air as it is made, in 20 ms frames, into six-second chunks, each
/// signed into the log the moment it is done.
fn encode(inner: Arc<Inner>, station: StationId, music: bool, rx: mpsc::Receiver<Air>) {
    let profile = |m: bool| if m { Profile::Music } else { Profile::Talk };
    let mut enc = match Coder::new(profile(music)) {
        Ok(c) => c,
        Err(e) => {
            say(&format!("the encoder would not start: {e}"));
            return;
        }
    };
    let mut ch = if music { 2 } else { 1 };
    // Stereo, interleaved: whatever is left over between frames carries across a change of
    // encoder, so a switch to music loses no sound.
    let mut pend: Vec<i16> = Vec::new();
    let mut done: Vec<(Chunk, Vec<u8>)> = Vec::new();
    let mut sent = false;
    let put = |done: &mut Vec<(Chunk, Vec<u8>)>, ch: usize, sent: &mut bool| {
        for (c, b) in done.drain(..) {
            air_chunk(&inner, station, c, b, ch as u8);
            *sent = true;
        }
    };
    let feed = |enc: &mut Coder, pend: &mut Vec<i16>, ch: usize, done: &mut Vec<(Chunk, Vec<u8>)>| {
        let whole = pend.len() / 1920 * 1920;
        for f in pend[..whole].chunks_exact(1920) {
            let frame = if ch == 1 { f.chunks_exact(2).map(|s| ((s[0] as i32 + s[1] as i32) >> 1) as i16).collect() } else { f.to_vec() };
            enc.frame(frame, done);
        }
        pend.drain(..whole);
    };
    loop {
        match rx.recv().unwrap_or(Air::End) {
            Air::Pcm(p) => {
                pend.extend_from_slice(&p);
                feed(&mut enc, &mut pend, ch, &mut done);
                put(&mut done, ch, &mut sent);
            }
            Air::Music => {
                if ch == 1 {
                    // Music joins a talk show: the chunk so far leaves as it is, and the next
                    // starts in stereo.
                    let Ok(next) = Coder::new(Profile::Music) else { continue };
                    std::mem::replace(&mut enc, next).finish(&mut done);
                    put(&mut done, 1, &mut sent);
                    ch = 2;
                }
            }
            Air::End => {
                // The last part-frame is made up with quiet, what is left of the chunk goes,
                // then the end of the broadcast, so listeners go back to the schedule at once.
                if !pend.is_empty() {
                    pend.resize(1920, 0);
                    feed(&mut enc, &mut pend, ch, &mut done);
                }
                enc.finish(&mut done);
                put(&mut done, ch, &mut sent);
                if sent {
                    let _ = inner.store.lock().unwrap().write(station, now_ms() + 1, Body::LiveEnd);
                    sail(&inner, station);
                }
                say(&format!("off the air {}", station.call_sign()));
                return;
            }
        }
    }
}

fn air_chunk(inner: &Arc<Inner>, station: StationId, c: Chunk, bytes: Vec<u8>, ch: u8) {
    let bitrate = if ch == 2 { Profile::Music.bitrate() } else { Profile::Talk.bitrate() };
    {
        let mut s = inner.store.lock().unwrap();
        if s.put_chunk(&c.hash, &bytes).is_ok() {
            let _ = s.write(station, now_ms(), Body::Live { chunk: c, channels: ch, bitrate });
        }
    }
    sail(inner, station);
}

/// A private node's chunk goes down the stem the moment it is made. A pocket station never
/// does: its members fetch from the keeper, who tells them it is on the air on its own topic.
fn sail(inner: &Arc<Inner>, station: StationId) {
    if inner.cfg.private && !inner.is_pocket(&station) {
        let m2 = inner.clone();
        inner.handle.spawn(async move { m2.stem_push(station, None).await });
    }
}

// ---- The mixer ---------------------------------------------------------------------------

struct Mixer {
    inner: Arc<Inner>,
    show: Option<Show>,
    mic: Source,
    mic_want: Option<(Want, u64)>,
    voice: Voice,
    semis: f64,
    two: Source,
    two_want: Option<(Want, u64)>,
    hum: [Voice; 2],
    cur: Option<(u64, Stream)>,
    next: Option<Stream>,
    lim: Limiter,
    g_mic: f32,
    g_two: f32,
    g_deck: f32,
    g_duck: f32,
    g_pads: f32,
    idle: usize,
    #[cfg(feature = "playback")]
    mon: Option<(crate::player::Output, crate::player::SharedQueue)>,
    a: Vec<f32>,
    b: Vec<f32>,
    d: Vec<f32>,
    p: Vec<f32>,
    v16: Vec<i16>,
}

fn run(inner: Arc<Inner>) {
    say("the desk is up");
    let mut m = Mixer {
        inner,
        show: None,
        mic: Source::Off,
        mic_want: None,
        voice: Voice::new(true, 0.0),
        semis: 0.0,
        two: Source::Off,
        two_want: None,
        hum: [Voice::new(true, 0.0), Voice::new(true, 0.0)],
        cur: None,
        next: None,
        lim: Limiter::new(),
        g_mic: 0.0,
        g_two: 0.0,
        g_deck: 0.0,
        g_duck: 1.0,
        g_pads: 0.0,
        idle: 0,
        #[cfg(feature = "playback")]
        mon: None,
        a: Vec::new(),
        b: Vec::new(),
        d: Vec::new(),
        p: Vec::new(),
        v16: Vec::new(),
    };
    // Paced by the clock: each turn makes as much of the air as time has called for.
    let t0 = Instant::now();
    let mut made: u64 = 0;
    loop {
        std::thread::sleep(Duration::from_millis(10));
        let due = (t0.elapsed().as_secs_f64() * RATE) as u64;
        let mut n = due.saturating_sub(made) as usize;
        // Half a second behind means the thread was starved or the device slept: on from now,
        // rather than a burst to catch up with.
        if n > 24_000 {
            made = due - 960;
            n = 960;
        }
        if n == 0 {
            continue;
        }
        made += n as u64;
        if !m.block(n) {
            break;
        }
    }
    let d = &m.inner.desk.meters;
    for a in [&d.mic, &d.two, &d.deck, &d.pads, &d.l, &d.r] {
        a.store(0f32.to_bits(), Relaxed);
    }
    say("the desk rests");
}

impl Mixer {
    /// The show, as the node has it: begun or ended since the last turn.
    fn follow(&mut self, live: Option<(StationId, u64)>) {
        let same = matches!((&self.show, live), (Some(s), Some((st, t))) if s.station == st && s.started == t);
        if same {
            return;
        }
        if let Some(s) = self.show.take() {
            let _ = s.tx.send(Air::End);
            // Every input lets go of its device at once and the deck and pads stop: the
            // microphone's light goes out with the broadcast.
            let mut sh = self.inner.desk.sh.lock().unwrap();
            sh.checking = false;
            sh.playing = false;
            sh.file = None;
            for p in sh.pads.iter_mut().flatten() {
                if p.at.is_some() {
                    p.stop = true;
                }
            }
        }
        if let Some((station, started)) = live {
            let (tx, rx) = mpsc::channel();
            let mut sh = self.inner.desk.sh.lock().unwrap();
            let music = sh.in_play();
            sh.music = music;
            sh.aired = 0;
            sh.checking = false;
            drop(sh);
            let i2 = self.inner.clone();
            let _ = std::thread::Builder::new().name("encoder".into()).spawn(move || encode(i2, station, music, rx));
            self.lim.ceil = db(if music { -3.0 } else { -4.0 });
            say(&format!("ON AIR {}", station.call_sign()));
            self.show = Some(Show { station, started, tx, music });
        }
    }

    /// Opens and closes the inputs as the desk asks: only while on the air or checking levels.
    fn devices(&mut self) {
        let inner = self.inner.clone();
        let (mic, two, semis) = {
            let sh = inner.desk.sh.lock().unwrap();
            let up = self.show.is_some() || sh.checking;
            let mic = if !up {
                None
            } else if let Some(f) = &sh.file {
                Some((Want::File(f.clone(), false), sh.mic_tries))
            } else if let Some(f) = &sh.mic_file {
                Some((Want::File(f.clone(), true), sh.mic_tries))
            } else {
                Some((Want::Dev(sh.mic_dev.clone()), sh.mic_tries))
            };
            let two = if !up {
                None
            } else if let Some(f) = &sh.two_file {
                Some((Want::File(f.clone(), true), sh.two_tries))
            } else {
                sh.two_dev.clone().map(|d| (Want::Dev(Some(d)), sh.two_tries))
            };
            (mic, two, sh.disguise)
        };
        if mic != self.mic_want {
            // The old one lets go first: some devices take one stream at a time.
            self.mic = Source::Off;
            let (src, err) = match &mic {
                Some((w, _)) => Source::open(w, 1),
                None => (Source::Off, None),
            };
            let mut sh = inner.desk.sh.lock().unwrap();
            sh.mic_on = src.on();
            sh.mic_err = err;
            if let Some((_, tries)) = &mic {
                sh.mic_seen = *tries;
            }
            self.mic = src;
            self.mic_want = mic;
            self.voice = Voice::new(true, semis);
            self.semis = semis;
        }
        if semis != self.semis {
            self.voice = Voice::new(true, semis);
            self.semis = semis;
        }
        if two != self.two_want {
            self.two = Source::Off;
            let (src, err) = match &two {
                Some((w, _)) => Source::open(w, 2),
                None => (Source::Off, None),
            };
            let mut sh = inner.desk.sh.lock().unwrap();
            sh.two_on = src.on();
            sh.two_err = err;
            self.two = src;
            self.two_want = two;
            self.hum = [Voice::new(true, 0.0), Voice::new(true, 0.0)];
        }
    }

    /// One turn: `n` frames of the air.
    fn block(&mut self, n: usize) -> bool {
        let inner = self.inner.clone();
        let live = *inner.live.lock().unwrap();
        self.follow(live);
        self.devices();

        // The voice: the hum out and the disguise on, as the microphone gives it.
        let mut mic = std::mem::take(&mut self.a);
        let mic_alive = self.mic.take(n, 1, &mut mic);
        if self.mic.on() {
            self.v16.clear();
            self.v16.extend(mic.iter().map(|s| (s.clamp(-1.0, 1.0) * 32767.0) as i16));
            self.voice.run(&mut self.v16);
            for (o, s) in mic.iter_mut().zip(&self.v16) {
                *o = *s as f32 / 32768.0;
            }
        }
        // The second input loses its hum and nothing else.
        let mut two = std::mem::take(&mut self.b);
        let two_alive = self.two.take(n, 2, &mut two);
        if self.two.on() {
            for c in 0..2 {
                self.v16.clear();
                self.v16.extend(two.iter().skip(c).step_by(2).map(|s| (s.clamp(-1.0, 1.0) * 32767.0) as i16));
                self.hum[c].run(&mut self.v16);
                for (k, s) in self.v16.iter().enumerate() {
                    two[2 * k + c] = *s as f32 / 32768.0;
                }
            }
        }

        let mut deck = std::mem::take(&mut self.d);
        deck.clear();
        deck.resize(n * 2, 0.0);
        let mut pads = std::mem::take(&mut self.p);
        pads.clear();
        pads.resize(n * 2, 0.0);

        let mut sh = inner.desk.sh.lock().unwrap();
        // An input that went away says so and closes.
        if !mic_alive && self.mic.on() {
            if sh.file.is_some() {
                // go_live's recording has played out: so has the show.
                let mut l = inner.live.lock().unwrap();
                if l.is_some_and(|x| Some(x) == live) {
                    *l = None;
                }
            } else {
                sh.note = Some(("The microphone went away.".into(), Instant::now()));
                sh.mic_err = Some("The microphone went away.".into());
            }
            sh.mic_on = false;
            self.mic = Source::Off;
        }
        if !two_alive && self.two.on() {
            sh.note = Some(("The second input went away.".into(), Instant::now()));
            sh.two_on = false;
            self.two = Source::Off;
        }

        // The deck: the track at hand, read ahead, and the one after it once this one is read,
        // so a track that ends gives way to the next within the same turn.
        let mut filled = 0usize;
        for _ in 0..3 {
            let want = sh.at.and_then(|i| sh.list.get(i)).map(|x| (x.id, x.path.clone()));
            let key = want.as_ref().map(|(id, _)| (*id, sh.cue));
            if self.cur.as_ref().map(|(cue, s)| (s.id, *cue)) != key {
                self.cur = None;
                if let Some((id, path)) = want {
                    // The same track and cue as the position belongs to, after the desk has
                    // rested: on from where it was. Anything else starts at the top.
                    let from = if sh.pos_key == key { sh.pos } else { 0 };
                    let s = match self.next.take() {
                        Some(nx) if nx.id == id && from == 0 => nx,
                        _ => Stream::read(id, path, from),
                    };
                    self.cur = Some((sh.cue, s));
                    sh.pos = from;
                    sh.pos_key = key;
                }
            }
            if !sh.playing || filled == n {
                break;
            }
            let Some((_, s)) = &self.cur else { break };
            let (got, read, ended, bad) = {
                let mut r = s.ring.lock().unwrap();
                let k = ((n - filled) * 2).min(r.pcm.len());
                for (o, v) in deck[filled * 2..].iter_mut().zip(r.pcm.drain(..k)) {
                    *o = v;
                }
                (k / 2, r.eof, r.eof && r.pcm.is_empty(), r.bad)
            };
            filled += got;
            sh.pos += got as u64;
            if bad {
                if let Some(x) = sh.at.and_then(|i| sh.list.get_mut(i)) {
                    x.bad = true;
                }
                sh.note = Some(("That track would not play.".into(), Instant::now()));
                sh.v += 1;
                sh.advance(true);
                continue;
            }
            if ended {
                let pos = sh.pos;
                if let Some(x) = sh.at.and_then(|i| sh.list.get_mut(i)) {
                    if x.len_ms.is_none() {
                        x.len_ms = Some(pos / 48);
                    }
                }
                sh.v += 1;
                let go = !sh.each;
                sh.advance(go);
                continue;
            }
            if read && !sh.each {
                // Read to its end: the next starts reading now, so it follows without a gap.
                match (sh.after().and_then(|i| sh.list.get(i)), &self.next) {
                    (Some(x), Some(nx)) if nx.id == x.id => {}
                    (Some(x), _) => self.next = Some(Stream::read(x.id, x.path.clone(), 0)),
                    (None, _) => self.next = None,
                }
            }
            break;
        }

        // The pads, each from where it is.
        const FADE: f32 = 0.998_96;
        for p in sh.pads.iter_mut().flatten() {
            let Some(mut at) = p.at else { continue };
            let mut on = true;
            for i in 0..n {
                if (at + 1) * 2 > p.pcm.len() {
                    on = false;
                    break;
                }
                if p.stop {
                    p.fade *= FADE;
                    if p.fade < 1e-3 {
                        on = false;
                        break;
                    }
                }
                pads[2 * i] += p.pcm[2 * at] as f32 / 32768.0 * p.fade;
                pads[2 * i + 1] += p.pcm[2 * at + 1] as f32 / 32768.0 * p.fade;
                at += 1;
            }
            p.at = if on { Some(at) } else { None };
        }

        // Music joins a talk show the moment something beyond a voice is in play.
        if let Some(s) = self.show.as_mut() {
            if !s.music && sh.in_play() {
                s.music = true;
                sh.music = true;
                let _ = s.tx.send(Air::Music);
                self.lim.ceil = db(-3.0);
            }
        }

        // The mix. Every gain glides, so no change clicks; the deck dips under the open mic
        // over about a fifth of a second and comes back up more slowly.
        let t_mic = if sh.mic_open { sh.mic_gain } else { 0.0 };
        let t_two = if sh.two_open { sh.two_gain } else { 0.0 };
        let t_duck = if sh.mic_open && self.mic.on() { sh.duck } else { 1.0 };
        let (t_deck, t_pads, mic_gain, two_gain) = (sh.deck_gain, sh.pads_gain, sh.mic_gain, sh.two_gain);
        let (k_gate, k_down, k_up) = (coef(0.015), coef(0.07), coef(0.15));
        let k_duck = if t_duck < self.g_duck { k_down } else { k_up };
        let mut air = Vec::with_capacity(n * 2);
        let (mut pk_mic, mut pk_two, mut pk_deck, mut pk_pads, mut pk_l, mut pk_r) = (0f32, 0f32, 0f32, 0f32, 0f32, 0f32);
        for i in 0..n {
            self.g_mic += (t_mic - self.g_mic) * k_gate;
            self.g_two += (t_two - self.g_two) * k_gate;
            self.g_deck += (t_deck - self.g_deck) * k_gate;
            self.g_pads += (t_pads - self.g_pads) * k_gate;
            self.g_duck += (t_duck - self.g_duck) * k_duck;
            let v = mic[i];
            let (tl, tr) = (two[2 * i], two[2 * i + 1]);
            let (dl, dr) = (deck[2 * i] * self.g_deck, deck[2 * i + 1] * self.g_deck);
            let (pl, pr) = (pads[2 * i] * self.g_pads, pads[2 * i + 1] * self.g_pads);
            // Each meter reads after its fader and before its gate, so a closed mic still shows.
            pk_mic = pk_mic.max((v * mic_gain).abs());
            pk_two = pk_two.max((tl * two_gain).abs().max((tr * two_gain).abs()));
            pk_deck = pk_deck.max(dl.abs().max(dr.abs()));
            pk_pads = pk_pads.max(pl.abs().max(pr.abs()));
            let m = v * self.g_mic;
            let l = m + tl * self.g_two + dl * self.g_duck + pl;
            let r = m + tr * self.g_two + dr * self.g_duck + pr;
            let (l, r) = self.lim.run(l, r);
            pk_l = pk_l.max(l.abs());
            pk_r = pk_r.max(r.abs());
            air.push(l);
            air.push(r);
        }
        let fall = (-(24.0 / 20.0) * std::f64::consts::LN_10 * n as f64 / RATE).exp() as f32;
        let mt = &inner.desk.meters;
        mark(&mt.mic, if self.mic.on() { pk_mic } else { 0.0 }, fall);
        mark(&mt.two, if self.two.on() { pk_two } else { 0.0 }, fall);
        mark(&mt.deck, pk_deck, fall);
        mark(&mt.pads, pk_pads, fall);
        mark(&mt.l, pk_l, fall);
        mark(&mt.r, pk_r, fall);
        if self.show.is_some() {
            sh.aired += n as u64;
        }
        let hear = sh.hear;
        // Nothing to do for two seconds and the desk rests, under the same lock that wakes it.
        let busy = self.show.is_some() || live.is_some() || sh.checking || sh.playing || sh.pads_playing() || sh.hear;
        self.idle = if busy { 0 } else { self.idle + n };
        let rest = self.idle > 2 * 48_000;
        if rest {
            sh.running = false;
            sh.mic_on = false;
            sh.two_on = false;
        }
        drop(sh);

        if let Some(s) = &self.show {
            let pcm: Vec<i16> = air.iter().map(|x| (x * 32767.0).round() as i16).collect();
            let _ = s.tx.send(Air::Pcm(pcm));
        }
        self.monitor(hear, &air);
        self.a = mic;
        self.b = two;
        self.d = deck;
        self.p = pads;
        !rest
    }

    /// Hearing the mix is the one way any of the desk reaches this device's speaker, and only
    /// when asked: on open speakers the microphone would take it in again.
    #[cfg(feature = "playback")]
    fn monitor(&mut self, hear: bool, air: &[f32]) {
        if !hear {
            self.mon = None;
            return;
        }
        if std::env::var_os("PYRATE_NULL_AUDIO").is_some_and(|v| v != "0") {
            return;
        }
        if self.mon.is_none() {
            let q: crate::player::SharedQueue = Arc::new(Mutex::new(crate::player::Queue { volume: 1.0, ..Default::default() }));
            match crate::player::open(q.clone()) {
                Ok(o) => self.mon = Some((o, q)),
                Err(e) => {
                    self.inner.desk.say(&format!("The mix cannot be heard here ({e})."));
                    self.inner.desk.sh.lock().unwrap().hear = false;
                    return;
                }
            }
        }
        if let Some((_, q)) = &self.mon {
            let mut q = q.lock().unwrap_or_else(|e| e.into_inner());
            q.samples.extend(air.iter().copied());
            // A fifth of a second at most stands between the desk and the ear.
            let over = q.samples.len().saturating_sub(9_600 * 2);
            q.samples.drain(..over);
        }
    }

    #[cfg(not(feature = "playback"))]
    fn monitor(&mut self, _hear: bool, _air: &[f32]) {}
}
