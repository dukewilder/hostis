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
    let step = 48_000.0 / rate;
    let mut phase = 0.0f64;
    let mut prev = [0f32; 2];
    let mut cur = [0f32; 2];

    let err = |e| eprintln!("audio: {e}");
    let stream = device.build_output_stream(
        &config,
        move |out: &mut [f32], _| {
            let mut q = queue.lock().unwrap();
            let vol = q.volume;
            for frame in out.chunks_mut(channels) {
                // Linear interpolation between 48 kHz frames.
                while phase >= 1.0 {
                    phase -= 1.0;
                    prev = cur;
                    if q.samples.len() >= 2 {
                        cur = [q.samples.pop_front().unwrap(), q.samples.pop_front().unwrap()];
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
                    1 => frame[0] = (l + r) * 0.5,
                    _ => {
                        frame[0] = l;
                        frame[1] = r;
                        for c in frame.iter_mut().skip(2) {
                            *c = 0.0;
                        }
                    }
                }
                phase += step;
            }
        },
        err,
        None,
    )?;
    stream.play()?;
    Ok(Output { _stream: stream })
}
