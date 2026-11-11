//! Drop a file in, get chunks out. MP3, FLAC, WAV, Ogg Vorbis, AAC and the rest
//! symphonia reads, at any sample rate, mono or stereo or more.

use std::{fs::File, path::Path};

use rubato::{
    Async, FixedAsync, Indexing, Resampler, SincInterpolationParameters, SincInterpolationType,
    WindowFunction, audioadapter_buffers::direct::InterleavedSlice,
};
use symphonia::core::{
    codecs::audio::AudioDecoderOptions,
    errors::Error as SymErr,
    formats::{FormatOptions, TrackType, probe::Hint},
    io::MediaSourceStream,
    meta::MetadataOptions,
};

use crate::{
    Chunk, Error, Result,
    audio::{ChunkEncoder, Profile, SAMPLE_RATE},
};

pub struct Ingested {
    pub chunks: Vec<(Chunk, Vec<u8>)>,
    pub duration_ms: u64,
    pub channels: u8,
    pub bitrate: u32,
    /// The picture embedded in the file, front cover first, as it was found.
    pub picture: Option<Vec<u8>>,
}

/// How much of each channel goes left and right when more than two are folded to stereo,
/// by the channels' places (the usual downmix: centre and surrounds at -3 dB, the LFE left
/// out, so a film's dialogue stays). Channels with no stated place: the first two as left
/// and right, as before.
fn fold_weights(ch: &symphonia::core::audio::Channels, n: usize) -> Vec<(f32, f32)> {
    const H: f32 = std::f32::consts::FRAC_1_SQRT_2;
    let mut w: Vec<(f32, f32)> = match ch {
        symphonia::core::audio::Channels::Positioned(p) => (0..64u32)
            .filter(|b| p.bits() & (1u64 << b) != 0)
            .map(|b| match b {
                0 => (1.0, 0.0),
                1 => (0.0, 1.0),
                2 | 13 => (H, H),
                3 => (0.0, 0.0),
                4 | 6 | 9 | 12 | 15 => (H, 0.0),
                5 | 7 | 10 | 14 | 17 => (0.0, H),
                _ => (0.5, 0.5),
            })
            .collect(),
        _ => (0..n).map(|i| match i {
            0 => (1.0, 0.0),
            1 => (0.0, 1.0),
            _ => (0.0, 0.0),
        }).collect(),
    };
    w.resize(n, (0.0, 0.0));
    // Scaled so the fold cannot pass full scale; the loudness pass brings it back up.
    let most = w.iter().map(|x| x.0).sum::<f32>().max(w.iter().map(|x| x.1).sum::<f32>()).max(1.0);
    w.iter().map(|(l, r)| (l / most, r / most)).collect()
}

/// Folds any channel layout into the profile's: one or two channels as they are, more by
/// their places, and mono as the middle of the stereo fold.
fn remap(input: &[f32], in_ch: usize, weights: &[(f32, f32)], out_ch: usize, out: &mut Vec<f32>) {
    if in_ch == 0 {
        return;
    }
    for frame in input.chunks_exact(in_ch) {
        let (l, r) = match in_ch {
            1 => (frame[0], frame[0]),
            2 => (frame[0], frame[1]),
            _ => frame.iter().zip(weights).fold((0.0, 0.0), |(l, r), (x, (a, b))| (l + x * a, r + x * b)),
        };
        match (in_ch, out_ch) {
            (a, b) if a == b => out.extend_from_slice(frame),
            (_, 1) => out.push((l + r) * 0.5),
            _ => out.extend_from_slice(&[l, r]),
        }
    }
}

fn to_i16(s: f32) -> i16 {
    (s.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16
}

/// Streams the file through the decoder, the resampler and Opus, one block at a time,
/// so an hour of audio never sits in memory as raw samples.
/// Where a recording's sound begins and ends, and the gain that brings it to the house level.
struct Level {
    gain: f32,
    start: u64,
    end: u64,
}

/// Every station plays at one loudness: -16 LUFS, the level phones and earbuds are built
/// around. Gain is capped so quiet hiss is not raised into noise, and so no peak passes -1 dBFS.
const TARGET_LUFS: f64 = -16.0;
const MAX_GAIN_DB: f64 = 18.0;
const CEILING_DB: f64 = -1.0;
/// Below -50 dBFS counts as dead air, trimmed from the head and tail.
const SILENCE: f32 = 0.003_16;

fn measure(path: &Path, out_ch: usize) -> Result<Level> {
    let file = File::open(path)?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let mut format = symphonia::default::get_probe()
        .probe(&hint, mss, FormatOptions::default(), MetadataOptions::default())
        .map_err(|e| Error::Decode(e.to_string()))?;
    let track = format.default_track(TrackType::Audio).ok_or(Error::Decode("no audio track".into()))?;
    let track_id = track.id;
    let params = track.codec_params.as_ref().and_then(|p| p.audio()).ok_or(Error::Decode("no audio parameters".into()))?.clone();
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(&params, &AudioDecoderOptions::default())
        .map_err(|e| Error::Decode(e.to_string()))?;
    let mut meter: Option<ebur128::EbuR128> = None;
    let (mut decoded, mut mapped) = (Vec::new(), Vec::new());
    let (mut at, mut first, mut last) = (0u64, None::<u64>, 0u64);
    loop {
        let packet = match format.next_packet() {
            Ok(Some(p)) => p,
            Ok(None) => break,
            Err(SymErr::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(Error::Decode(e.to_string())),
        };
        if packet.track_id != track_id {
            continue;
        }
        let Ok(buf) = decoder.decode(&packet) else { continue };
        let in_ch = buf.spec().channels().count();
        if in_ch == 0 {
            continue;
        }
        let weights = fold_weights(buf.spec().channels(), in_ch);
        let rate = buf.spec().rate();
        decoded.clear();
        decoded.resize(buf.samples_interleaved(), 0.0);
        buf.copy_to_slice_interleaved(&mut decoded);
        mapped.clear();
        remap(&decoded, in_ch, &weights, out_ch, &mut mapped);
        let m = match &mut meter {
            Some(m) => m,
            None => meter.insert(ebur128::EbuR128::new(out_ch as u32, rate, ebur128::Mode::I | ebur128::Mode::SAMPLE_PEAK).map_err(|e| Error::Decode(format!("{e:?}")))?),
        };
        let _ = m.add_frames_f32(&mapped);
        for (i, f) in mapped.chunks_exact(out_ch).enumerate() {
            if f.iter().any(|s| s.abs() > SILENCE) {
                first.get_or_insert(at + i as u64);
                last = at + i as u64 + 1;
            }
        }
        at += (mapped.len() / out_ch) as u64;
    }
    let Some(m) = meter else { return Ok(Level { gain: 1.0, start: 0, end: u64::MAX }) };
    // A file of nothing but silence is not a recording.
    if first.is_none() && at > 0 {
        return Err(Error::Decode("the recording is silent".into()));
    }
    let lufs = m.loudness_global().unwrap_or(f64::NEG_INFINITY);
    let peak = (0..out_ch as u32).filter_map(|c| m.sample_peak(c).ok()).fold(0.0f64, f64::max);
    let mut gain_db = if lufs.is_finite() { (TARGET_LUFS - lufs).min(MAX_GAIN_DB) } else { 0.0 };
    if peak > 0.0 {
        gain_db = gain_db.min(CEILING_DB - 20.0 * peak.log10());
    }
    let start = first.unwrap_or(0);
    // A breath of tail is kept so endings are not clipped.
    let end = if first.is_some() { last + 14_400 } else { at };
    Ok(Level { gain: 10f32.powf(gain_db as f32 / 20.0), start, end })
}

pub fn ingest(path: &Path, profile: Profile) -> Result<Ingested> {
    ingest_voiced(path, profile, 0.0)
}

/// As [`ingest`]; talk also loses its mains hum (see [`crate::voice`]) and, if asked, has the
/// voice shifted by `semitones`. Music goes through as it is.
pub fn ingest_voiced(path: &Path, profile: Profile, semitones: f64) -> Result<Ingested> {
    let level = measure(path, profile.channels() as usize)?;
    let file = File::open(path)?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let mut format = symphonia::default::get_probe()
        .probe(&hint, mss, FormatOptions::default(), MetadataOptions::default())
        .map_err(|e| Error::Decode(e.to_string()))?;
    let picture = {
        let mut m = format.metadata();
        m.skip_to_latest();
        m.current().and_then(|rev| {
            let all: Vec<_> = rev.media.visuals.iter().chain(rev.per_track.iter().flat_map(|p| p.metadata.visuals.iter())).collect();
            all.iter()
                .find(|v| v.usage == Some(symphonia::core::meta::StandardVisualKey::FrontCover))
                .or(all.first())
                .map(|v| v.data.to_vec())
        })
    };
    let track = format.default_track(TrackType::Audio).ok_or(Error::Decode("no audio track".into()))?;
    let track_id = track.id;
    let params = track
        .codec_params
        .as_ref()
        .and_then(|p| p.audio())
        .ok_or(Error::Decode("no audio parameters".into()))?
        .clone();
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(&params, &AudioDecoderOptions::default())
        .map_err(|e| Error::Decode(e.to_string()))?;

    let out_ch = profile.channels() as usize;
    let mut encoder = ChunkEncoder::new(profile)?;
    let mut chunks = Vec::new();
    let mut resampler: Option<Async<f32>> = None;
    let mut in_rate = 0u32;
    let mut delay_left = 0usize;

    let mut decoded: Vec<f32> = Vec::new();
    let mut mapped: Vec<f32> = Vec::new();
    let mut resampled: Vec<f32> = Vec::new();
    let mut pcm: Vec<i16> = Vec::new();
    let mut voice = crate::voice::Voice::new(profile == Profile::Talk, if profile == Profile::Talk { semitones } else { 0.0 });
    let mut total_frames: u64 = 0;
    let mut src_at: u64 = 0;

    // Runs `mapped` through the resampler (or straight on at 48 kHz) and into Opus.
    let mut drain = |mapped: &mut Vec<f32>,
                     resampler: &mut Option<Async<f32>>,
                     delay_left: &mut usize,
                     last: bool,
                     encoder: &mut ChunkEncoder,
                     chunks: &mut Vec<(Chunk, Vec<u8>)>,
                     total_frames: &mut u64|
     -> Result<()> {
        pcm.clear();
        match resampler {
            None => {
                pcm.extend(mapped.iter().map(|s| to_i16(*s)));
                mapped.clear();
            }
            Some(r) => loop {
                let have = mapped.len() / out_ch;
                let need = r.input_frames_next();
                if have < need && !(last && have > 0) {
                    break;
                }
                let take = have.min(need);
                if take < need {
                    // The resampler reads a full block; the tail is padded and marked partial.
                    mapped.resize(need * out_ch, 0.0);
                }
                let have = mapped.len() / out_ch;
                let out_cap = r.output_frames_next();
                resampled.clear();
                resampled.resize(out_cap * out_ch, 0.0);
                let input = InterleavedSlice::new(&mapped[..], out_ch, have)
                    .map_err(|e| Error::Resample(e.to_string()))?;
                let mut output = InterleavedSlice::new_mut(&mut resampled[..], out_ch, out_cap)
                    .map_err(|e| Error::Resample(e.to_string()))?;
                let mut idx = Indexing::new();
                if take < need {
                    idx.partial_len = Some(take);
                }
                let (used, made) = r
                    .process_into_buffer(&input, &mut output, Some(&idx))
                    .map_err(|e| Error::Resample(e.to_string()))?;
                let used = if take < need { have } else { used };
                mapped.drain(..used * out_ch);
                let skip = (*delay_left).min(made);
                *delay_left -= skip;
                pcm.extend(resampled[skip * out_ch..made * out_ch].iter().map(|s| to_i16(*s)));
                if take < need {
                    break;
                }
            },
        }
        *total_frames += (pcm.len() / out_ch) as u64;
        voice.run(&mut pcm);
        chunks.extend(encoder.push(&pcm)?);
        Ok(())
    };

    loop {
        let packet = match format.next_packet() {
            Ok(Some(p)) => p,
            Ok(None) => break,
            Err(SymErr::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(Error::Decode(e.to_string())),
        };
        if packet.track_id != track_id {
            continue;
        }
        let buf = match decoder.decode(&packet) {
            Ok(b) => b,
            Err(SymErr::DecodeError(_)) => continue,
            Err(e) => return Err(Error::Decode(e.to_string())),
        };
        let spec = buf.spec().clone();
        let in_ch = spec.channels().count();
        if in_ch == 0 {
            continue;
        }
        let weights = fold_weights(spec.channels(), in_ch);
        let rate = spec.rate();
        if in_rate == 0 {
            in_rate = rate;
            if rate != SAMPLE_RATE {
                let params = SincInterpolationParameters::new(128, WindowFunction::Blackman2)
                    .oversampling_factor(256)
                    .interpolation(SincInterpolationType::Quadratic);
                let r = Async::<f32>::new_sinc(
                    SAMPLE_RATE as f64 / rate as f64,
                    1.1,
                    &params,
                    1024,
                    out_ch,
                    FixedAsync::Input,
                )
                .map_err(|e| Error::Resample(e.to_string()))?;
                delay_left = r.output_delay();
                resampler = Some(r);
            }
        }
        decoded.clear();
        decoded.resize(buf.samples_interleaved(), 0.0);
        buf.copy_to_slice_interleaved(&mut decoded);
        // Dead air off the head and tail, and every recording brought to the house level.
        let frames = (decoded.len() / in_ch) as u64;
        let from = level.start.saturating_sub(src_at).min(frames) as usize;
        let to = level.end.saturating_sub(src_at).min(frames) as usize;
        src_at += frames;
        if from >= to {
            continue;
        }
        let kept = &mut decoded[from * in_ch..to * in_ch];
        for s in kept.iter_mut() {
            *s = (*s * level.gain).clamp(-1.0, 1.0);
        }
        remap(kept, in_ch, &weights, out_ch, &mut mapped);
        drain(&mut mapped, &mut resampler, &mut delay_left, false, &mut encoder, &mut chunks, &mut total_frames)?;
    }
    drain(&mut mapped, &mut resampler, &mut delay_left, true, &mut encoder, &mut chunks, &mut total_frames)?;
    if let Some(c) = encoder.finish()? {
        chunks.push(c);
    }
    if total_frames == 0 {
        return Err(Error::Decode("no audio in file".into()));
    }
    Ok(Ingested {
        chunks,
        duration_ms: total_frames * 1000 / SAMPLE_RATE as u64,
        channels: profile.channels(),
        bitrate: profile.bitrate(),
        picture,
    })
}
