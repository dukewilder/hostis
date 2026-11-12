//! Sound out of the device's speaker. A queue of 48 kHz stereo samples in,
//! the default output device out, resampled on the fly if the device insists.

use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

use anyhow::{Result, anyhow};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

#[derive(Default)]
pub struct Queue {
    pub samples: VecDeque<f32>,
    /// Samples played since tuning, for the playhead.
    pub played: u64,
    pub starved: bool,
    pub volume: f32,
}

pub type SharedQueue = Arc<Mutex<Queue>>;

pub struct Output {
    _stream: cpal::Stream,
}

// cpal streams are not Send on every platform; the node keeps this on one thread.
pub fn open(queue: SharedQueue) -> Result<Output> {
    let host = cpal::default_host();
    let device = host.default_output_device().ok_or_else(|| anyhow!("no audio output"))?;
    let supported = device.default_output_config()?;
    let rate = supported.sample_rate().0 as f64;
    let channels = supported.channels() as usize;
    let config: cpal::StreamConfig = supported.config();
    // Whatever samples the device wants, it gets: some speakers take only integers.
    let stream = match supported.sample_format() {
        cpal::SampleFormat::I16 => out_stream::<i16>(&device, &config, queue, channels, rate)?,
        cpal::SampleFormat::U16 => out_stream::<u16>(&device, &config, queue, channels, rate)?,
        cpal::SampleFormat::I32 => out_stream::<i32>(&device, &config, queue, channels, rate)?,
        _ => out_stream::<f32>(&device, &config, queue, channels, rate)?,
    };
    stream.play()?;
    Ok(Output { _stream: stream })
}

/// No speaker at all, for tests on machines without one (PYRATE_NULL_AUDIO=1): the queue is
/// drained at 48 kHz in real time, as a device would, so playout and its status behave as on a
/// phone. Never chosen on its own.
pub fn drain(queue: SharedQueue) -> ! {
    let start = std::time::Instant::now();
    let mut taken: u64 = 0;
    loop {
        std::thread::sleep(std::time::Duration::from_millis(20));
        let owed = (start.elapsed().as_secs_f64() * 48_000.0) as u64;
        let mut q = queue.lock().unwrap_or_else(|e| e.into_inner());
        while taken < owed {
            taken += 1;
            if q.samples.len() >= 2 {
                q.samples.pop_front();
                q.samples.pop_front();
                q.played += 1;
                q.starved = false;
            } else {
                q.starved = true;
            }
        }
    }
}

fn out_stream<T: cpal::SizedSample + cpal::FromSample<f32>>(device: &cpal::Device, config: &cpal::StreamConfig, queue: SharedQueue, channels: usize, rate: f64) -> Result<cpal::Stream> {
    let step = 48_000.0 / rate;
    let mut phase = 0.0f64;
    let mut prev = [0f32; 2];
    let mut cur = [0f32; 2];
    let err = |e| eprintln!("audio: {e}");
    Ok(device.build_output_stream(
        config,
        move |out: &mut [T], _| {
            // A queue left poisoned by a panic elsewhere still plays; the sound thread never
            // panics on it.
            let mut q = queue.lock().unwrap_or_else(|e| e.into_inner());
            let vol = q.volume;
            for frame in out.chunks_mut(channels) {
                // Linear interpolation between 48 kHz frames.
                while phase >= 1.0 {
                    phase -= 1.0;
                    prev = cur;
                    if q.samples.len() >= 2 {
                        cur = [q.samples.pop_front().unwrap_or(0.0), q.samples.pop_front().unwrap_or(0.0)];
                        q.played += 1;
                        q.starved = false;
                    } else {
                        cur = [0.0, 0.0];
                        q.starved = true;
                    }
                }
                let t = phase as f32;
                let l = (prev[0] + (cur[0] - prev[0]) * t) * vol;
                let r = (prev[1] + (cur[1] - prev[1]) * t) * vol;
                match channels {
                    1 => frame[0] = T::from_sample((l + r) * 0.5),
                    _ => {
                        frame[0] = T::from_sample(l);
                        frame[1] = T::from_sample(r);
                        for c in frame.iter_mut().skip(2) {
                            *c = T::EQUILIBRIUM;
                        }
                    }
                }
                phase += step;
            }
        },
        err,
        None,
    )?)
}

/// The microphone, as 48 kHz mono i16 pushed into a shared buffer.
pub struct Input {
    _stream: cpal::Stream,
}

pub fn open_input(buf: Arc<Mutex<Vec<i16>>>) -> Result<Input> {
    let host = cpal::default_host();
    let device = host.default_input_device().ok_or_else(|| anyhow!("no microphone"))?;
    let supported = device.default_input_config()?;
    let rate = supported.sample_rate().0 as f64;
    let channels = supported.channels() as usize;
    let config: cpal::StreamConfig = supported.config();
    if channels == 0 {
        return Err(anyhow!("the microphone reports no channels"));
    }
    let stream = match supported.sample_format() {
        cpal::SampleFormat::I16 => in_stream::<i16>(&device, &config, buf, channels, rate)?,
        cpal::SampleFormat::U16 => in_stream::<u16>(&device, &config, buf, channels, rate)?,
        cpal::SampleFormat::I32 => in_stream::<i32>(&device, &config, buf, channels, rate)?,
        _ => in_stream::<f32>(&device, &config, buf, channels, rate)?,
    };
    stream.play()?;
    Ok(Input { _stream: stream })
}

fn in_stream<T: cpal::SizedSample>(device: &cpal::Device, config: &cpal::StreamConfig, buf: Arc<Mutex<Vec<i16>>>, channels: usize, rate: f64) -> Result<cpal::Stream>
where
    f32: cpal::FromSample<T>,
{
    let step = rate / 48_000.0;
    let mut phase = 0.0f64;
    Ok(device.build_input_stream(
        config,
        move |data: &[T], _| {
            let mut out = buf.lock().unwrap_or_else(|e| e.into_inner());
            let frames = data.len() / channels;
            // Average the channels, then step through at 48 kHz.
            while (phase as usize) < frames {
                let i = phase as usize * channels;
                let s: f32 = data[i..i + channels].iter().map(|x| <f32 as cpal::FromSample<T>>::from_sample_(*x)).sum::<f32>() / channels as f32;
                out.push((s.clamp(-1.0, 1.0) * 32767.0) as i16);
                phase += step;
            }
            phase -= frames as f64;
        },
        |e| eprintln!("mic: {e}"),
        None,
    )?)
}

/// A recording from the microphone to a WAV file, for computers, where the app has no
/// recorder of its own: a segment, a bell, a hail. One at a time.
struct Rec {
    stop: Arc<std::sync::atomic::AtomicBool>,
    path: std::path::PathBuf,
    started: std::time::Instant,
    done: std::thread::JoinHandle<Result<()>>,
}
static REC: Mutex<Option<Rec>> = Mutex::new(None);

pub fn rec_start(path: std::path::PathBuf) -> Result<()> {
    let _ = rec_stop();
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let (s2, p2) = (stop.clone(), path.clone());
    let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<()>>();
    let done = std::thread::spawn(move || -> Result<()> {
        let buf: Arc<Mutex<Vec<i16>>> = Arc::new(Mutex::new(Vec::new()));
        let input = match open_input(buf.clone()) {
            Ok(i) => { let _ = ready_tx.send(Ok(())); i }
            Err(e) => { let _ = ready_tx.send(Err(anyhow!("{e}"))); return Err(e) }
        };
        let mut pcm: Vec<i16> = Vec::new();
        while !s2.load(std::sync::atomic::Ordering::Relaxed) && pcm.len() < 48_000 * 60 * 30 {
            std::thread::sleep(std::time::Duration::from_millis(100));
            pcm.append(&mut buf.lock().unwrap_or_else(|e| e.into_inner()));
        }
        drop(input);
        pcm.append(&mut buf.lock().unwrap_or_else(|e| e.into_inner()));
        write_wav(&p2, &pcm)
    });
    ready_rx.recv().map_err(|_| anyhow!("the recorder did not start"))??;
    *REC.lock().unwrap_or_else(|e| e.into_inner()) = Some(Rec { stop, path, started: std::time::Instant::now(), done });
    Ok(())
}

/// The file and how long it ran, once it is written.
pub fn rec_stop() -> Option<(std::path::PathBuf, u64)> {
    let r = REC.lock().unwrap_or_else(|e| e.into_inner()).take()?;
    r.stop.store(true, std::sync::atomic::Ordering::Relaxed);
    let ms = r.started.elapsed().as_millis() as u64;
    match r.done.join() {
        Ok(Ok(())) => Some((r.path, ms)),
        _ => None,
    }
}

fn write_wav(path: &std::path::Path, pcm: &[i16]) -> Result<()> {
    let data = (pcm.len() * 2) as u32;
    let mut b = Vec::with_capacity(44 + data as usize);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes()); // PCM
    b.extend_from_slice(&1u16.to_le_bytes()); // mono
    b.extend_from_slice(&48_000u32.to_le_bytes());
    b.extend_from_slice(&96_000u32.to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data.to_le_bytes());
    for s in pcm { b.extend_from_slice(&s.to_le_bytes()) }
    std::fs::write(path, b)?;
    Ok(())
}
