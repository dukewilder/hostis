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

/// Folds any channel layout into the profile's: mono averages, stereo keeps the first two.
fn remap(input: &[f32], in_ch: usize, out_ch: usize, out: &mut Vec<f32>) {
    for frame in input.chunks_exact(in_ch) {
        match (in_ch, out_ch) {
            (a, b) if a == b => out.extend_from_slice(frame),
            (_, 1) => out.push(frame.iter().sum::<f32>() / in_ch as f32),
            (1, 2) => out.extend_from_slice(&[frame[0], frame[0]]),
            (_, 2) => out.extend_from_slice(&frame[..2]),
            _ => unreachable!("profiles are mono or stereo"),
        }
    }
}

fn to_i16(s: f32) -> i16 {
    (s.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16
}

/// Streams the file through the decoder, the resampler and Opus, one block at a time,
/// so an hour of audio never sits in memory as raw samples.
pub fn ingest(path: &Path, profile: Profile) -> Result<Ingested> {
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
    let mut total_frames: u64 = 0;

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
        remap(&decoded, in_ch, out_ch, &mut mapped);
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
