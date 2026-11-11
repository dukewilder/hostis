//! What a microphone gives away, taken out before it is encoded.
//!
//! **Mains hum.** Every recording made near the grid carries its hum, at 50 or 60 Hz and their
//! harmonics, and the grid's frequency wanders in a pattern that power companies log second by
//! second. Matched against those logs, a recording's hum says when it was made and on which grid
//! (electrical network frequency analysis). A comb of narrow notches at both grids' harmonics
//! takes it out; a voice, being far wider than the notches, loses nothing a listener can hear.
//!
//! **The voice itself.** A keeper may ask for the voice to go out shifted: pitch and formants
//! together, a few semitones down or up, so the timbre a voiceprint is drawn from is not the
//! keeper's. A two-tap delay line whose taps sweep and cross-fade (the oldest pitch shifter
//! there is) does it with no lookahead, so it can run on the live air.
//!
//! Both work on 48 kHz mono 16-bit samples, the Talk profile's own. No rights reserved.

use std::f64::consts::PI;

const RATE: f64 = 48_000.0;

#[derive(Clone, Copy, Default)]
struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    x1: f64,
    x2: f64,
    y1: f64,
    y2: f64,
}

impl Biquad {
    fn notch(f: f64, q: f64) -> Self {
        let w = 2.0 * PI * f / RATE;
        let alpha = w.sin() / (2.0 * q);
        let a0 = 1.0 + alpha;
        Self { b0: 1.0 / a0, b1: -2.0 * w.cos() / a0, b2: 1.0 / a0, a1: -2.0 * w.cos() / a0, a2: (1.0 - alpha) / a0, ..Default::default() }
    }

    fn run(&mut self, x: f64) -> f64 {
        let y = self.b0 * x + self.b1 * self.x1 + self.b2 * self.x2 - self.a1 * self.y1 - self.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

/// The harmonics of 50 and 60 Hz up to 360 Hz, where a grid's hum carries most of its trace.
const HUM: [f64; 12] = [50.0, 60.0, 100.0, 120.0, 150.0, 180.0, 200.0, 240.0, 250.0, 300.0, 350.0, 360.0];

/// Grain of the pitch shifter, in samples: 40 ms, short enough not to echo, long enough not
/// to buzz.
const GRAIN: f64 = 1_920.0;

/// A microphone's samples on their way to the encoder.
pub struct Voice {
    notches: Vec<Biquad>,
    ratio: f64,
    line: Vec<f64>,
    at: usize,
    phase: f64,
}

impl Voice {
    /// `dehum` takes out mains hum; `semitones` shifts the voice (0 leaves it as it is).
    pub fn new(dehum: bool, semitones: f64) -> Self {
        let notches = if dehum { HUM.iter().map(|f| Biquad::notch(*f, 30.0)).collect() } else { Vec::new() };
        Self { notches, ratio: 2f64.powf(semitones.clamp(-12.0, 12.0) / 12.0), line: vec![0.0; 4 * GRAIN as usize], at: 0, phase: 0.0 }
    }

    /// Whether it changes anything at all.
    pub fn active(&self) -> bool {
        !self.notches.is_empty() || (self.ratio - 1.0).abs() > 1e-6
    }

    fn tap(&self, delay: f64) -> f64 {
        let n = self.line.len();
        let pos = self.at as f64 - delay;
        let pos = if pos < 0.0 { pos + n as f64 } else { pos };
        let i = pos.floor() as usize % n;
        let j = (i + 1) % n;
        let f = pos - pos.floor();
        self.line[i] * (1.0 - f) + self.line[j] * f
    }

    fn one(&mut self, s: f64) -> f64 {
        let mut x = s;
        for b in &mut self.notches {
            x = b.run(x);
        }
        if (self.ratio - 1.0).abs() <= 1e-6 {
            return x;
        }
        let n = self.line.len();
        self.line[self.at] = x;
        // Two taps half a grain apart; each one's delay sweeps through the grain at the rate
        // that plays the past back faster (higher) or slower (lower), and fades in and out so
        // the jump at the end of each sweep is never heard. Their windows sum to one.
        let p2 = (self.phase + 0.5) % 1.0;
        let w1 = (PI * self.phase).sin().powi(2);
        let w2 = (PI * p2).sin().powi(2);
        let y = self.tap(1.0 + self.phase * GRAIN) * w1 + self.tap(1.0 + p2 * GRAIN) * w2;
        self.phase = (self.phase + (1.0 - self.ratio) / GRAIN).rem_euclid(1.0);
        self.at = (self.at + 1) % n;
        y
    }

    /// Runs samples through, in place.
    pub fn run(&mut self, pcm: &mut [i16]) {
        if !self.active() {
            return;
        }
        for s in pcm.iter_mut() {
            *s = self.one(*s as f64).round().clamp(-32768.0, 32767.0) as i16;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(f: f64, n: usize) -> Vec<i16> {
        (0..n).map(|i| ((2.0 * PI * f * i as f64 / RATE).sin() * 10_000.0) as i16).collect()
    }

    fn rms(p: &[i16]) -> f64 {
        (p.iter().map(|s| (*s as f64).powi(2)).sum::<f64>() / p.len() as f64).sqrt()
    }

    /// Strongest of a few probe frequencies, by correlation.
    fn loudest(p: &[i16], probes: &[f64]) -> f64 {
        *probes
            .iter()
            .max_by(|a, b| {
                let c = |f: f64| {
                    let (mut s, mut c) = (0.0, 0.0);
                    for (i, x) in p.iter().enumerate() {
                        let t = 2.0 * PI * f * i as f64 / RATE;
                        s += *x as f64 * t.sin();
                        c += *x as f64 * t.cos();
                    }
                    s * s + c * c
                };
                c(**a).partial_cmp(&c(**b)).unwrap()
            })
            .unwrap()
    }

    #[test]
    fn hum_goes_and_a_voice_stays() {
        let n = 48_000;
        let mut hum = tone(60.0, n);
        let mut voice = tone(440.0, n);
        let mut v = Voice::new(true, 0.0);
        v.run(&mut hum);
        let mut v = Voice::new(true, 0.0);
        v.run(&mut voice);
        assert!(rms(&hum[24_000..]) < 300.0, "60 Hz hum all but gone: {}", rms(&hum[24_000..]));
        assert!(rms(&voice[24_000..]) > 6_500.0, "440 Hz untouched: {}", rms(&voice[24_000..]));
        let mut hum50 = tone(150.0, n);
        Voice::new(true, 0.0).run(&mut hum50);
        assert!(rms(&hum50[24_000..]) < 300.0, "and the European grid's third harmonic");
    }

    #[test]
    fn a_voice_moves_where_it_is_asked() {
        let n = 48_000;
        let mut down = tone(400.0, n);
        Voice::new(false, -4.0).run(&mut down);
        let probes: Vec<f64> = (250..=450).step_by(5).map(|f| f as f64).collect();
        let got = loudest(&down[12_000..], &probes);
        let want = 400.0 * 2f64.powf(-4.0 / 12.0);
        assert!((got - want).abs() < 12.0, "four semitones down: wanted {want:.0} Hz, got {got} Hz");
        assert!(rms(&down[12_000..]) > 5_000.0, "at about the same loudness: {}", rms(&down[12_000..]));
        let mut same = tone(400.0, 4_800);
        let before = same.clone();
        Voice::new(false, 0.0).run(&mut same);
        assert_eq!(same, before, "asked for nothing, it changes nothing");
    }
}
