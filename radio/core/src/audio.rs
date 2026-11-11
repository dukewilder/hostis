//! Opus in, Opus out. 48 kHz, 20 ms frames, 300 frames to a six-second chunk.
//!
//! A chunk's bytes are its frames, each preceded by its length as a big-endian u16.
//! Nothing else: the entry that names the chunk carries channels and bitrate.

#[cfg(feature = "codec")]
use opus::{Application, Bitrate, Channels};

#[cfg(feature = "codec")]
use crate::Chunk;
use crate::{Error, Result};

pub const SAMPLE_RATE: u32 = 48_000;
/// Samples per channel in one 20 ms frame.
pub const FRAME: usize = 960;
pub const FRAMES_PER_CHUNK: u16 = 300;

/// How a station sounds. Chosen once per track or broadcast.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Profile {
    /// Stereo, 64 kbit/s.
    Music,
    /// Mono, 24 kbit/s.
    Talk,
}

impl Profile {
    pub fn channels(self) -> u8 {
        match self {
            Profile::Music => 2,
            Profile::Talk => 1,
        }
    }

    pub fn bitrate(self) -> u32 {
        match self {
            Profile::Music => 64_000,
            Profile::Talk => 24_000,
        }
    }
}

#[cfg(feature = "codec")]
fn opus_channels(n: u8) -> Result<Channels> {
    match n {
        1 => Ok(Channels::Mono),
        2 => Ok(Channels::Stereo),
        _ => Err(Error::Chunk),
    }
}

#[cfg(feature = "codec")]
/// Takes interleaved 48 kHz PCM, gives back finished chunks.
pub struct ChunkEncoder {
    enc: opus::Encoder,
    channels: usize,
    pending: Vec<i16>,
    current: Vec<u8>,
    frames: u16,
    packet: Vec<u8>,
}

#[cfg(feature = "codec")]
impl ChunkEncoder {
    pub fn new(profile: Profile) -> Result<Self> {
        let app = match profile {
            Profile::Music => Application::Audio,
            Profile::Talk => Application::Voip,
        };
        let mut enc = opus::Encoder::new(SAMPLE_RATE, opus_channels(profile.channels())?, app)?;
        enc.set_bitrate(Bitrate::Bits(profile.bitrate() as i32))?;
        // Constant bitrate: every chunk of a profile is the same size, so its size on the wire
        // says nothing of what plays, and a listener's traffic cannot be matched to a recording.
        enc.set_vbr(false)?;
        Ok(Self {
            enc,
            channels: profile.channels() as usize,
            pending: Vec::new(),
            current: Vec::with_capacity(64 * 1024),
            frames: 0,
            packet: vec![0u8; 4000],
        })
    }

    fn encode_frame(&mut self, pcm: &[i16]) -> Result<Option<(Chunk, Vec<u8>)>> {
        let n = self.enc.encode(pcm, &mut self.packet)?;
        self.current.extend_from_slice(&(n as u16).to_be_bytes());
        self.current.extend_from_slice(&self.packet[..n]);
        self.frames += 1;
        if self.frames == FRAMES_PER_CHUNK {
            return Ok(Some(self.take()));
        }
        Ok(None)
    }

    fn take(&mut self) -> (Chunk, Vec<u8>) {
        let bytes = std::mem::take(&mut self.current);
        let chunk = Chunk::of(&bytes, self.frames);
        self.frames = 0;
        (chunk, bytes)
    }

    /// Feeds interleaved samples. Returns every chunk completed by them.
    pub fn push(&mut self, pcm: &[i16]) -> Result<Vec<(Chunk, Vec<u8>)>> {
        self.pending.extend_from_slice(pcm);
        let step = FRAME * self.channels;
        let mut out = Vec::new();
        let mut at = 0;
        while self.pending.len() - at >= step {
            let frame: Vec<i16> = self.pending[at..at + step].to_vec();
            at += step;
            if let Some(c) = self.encode_frame(&frame)? {
                out.push(c);
            }
        }
        self.pending.drain(..at);
        Ok(out)
    }

    /// Pads the last frame with silence and returns the final, shorter chunk.
    pub fn finish(mut self) -> Result<Option<(Chunk, Vec<u8>)>> {
        if !self.pending.is_empty() {
            let mut frame = std::mem::take(&mut self.pending);
            frame.resize(FRAME * self.channels, 0);
            if let Some(c) = self.encode_frame(&frame)? {
                return Ok(Some(c));
            }
        }
        if self.frames == 0 {
            return Ok(None);
        }
        Ok(Some(self.take()))
    }
}

/// Splits a chunk into its Opus packets.
pub fn packets(bytes: &[u8]) -> Result<Vec<&[u8]>> {
    let mut out = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        if at + 2 > bytes.len() {
            return Err(Error::Chunk);
        }
        let n = u16::from_be_bytes([bytes[at], bytes[at + 1]]) as usize;
        at += 2;
        if at + n > bytes.len() {
            return Err(Error::Chunk);
        }
        out.push(&bytes[at..at + n]);
        at += n;
    }
    Ok(out)
}

/// Whether an Opus packet is one 20 ms frame, by its table-of-contents byte (RFC 6716): the
/// only packet a chunk's timing counts.
pub fn one_frame(p: &[u8]) -> bool {
    let Some(&toc) = p.first() else { return false };
    let (cfg, code) = (toc >> 3, toc & 3);
    // Frame lengths in tenths of a millisecond.
    let tenths: u32 = if cfg < 12 { [100, 200, 400, 600][(cfg & 3) as usize] } else if cfg < 16 { [100, 200][(cfg & 1) as usize] } else { [25, 50, 100, 200][(cfg & 3) as usize] };
    let frames: u32 = match code {
        0 => 1,
        1 | 2 => 2,
        _ => p.get(1).map(|b| (b & 0x3f) as u32).unwrap_or(0),
    };
    frames * tenths == 200 && p.len() <= 1275 * 2
}

#[cfg(feature = "codec")]
/// Keeps decoder state across chunks, so a stream plays without seams.
pub struct ChunkDecoder {
    dec: opus::Decoder,
    channels: usize,
}

#[cfg(feature = "codec")]
impl ChunkDecoder {
    pub fn new(channels: u8) -> Result<Self> {
        Ok(Self { dec: opus::Decoder::new(SAMPLE_RATE, opus_channels(channels)?)?, channels: channels as usize })
    }

    /// Interleaved 48 kHz PCM for one chunk. Every packet is one 20 ms frame, as the clock
    /// counts them: one that is empty, or that holds more or less than a frame (a chunk
    /// from somewhere other than this encoder), stands as 20 ms of Opus's own concealment,
    /// so a chunk always plays for as long as its station says and no longer.
    pub fn decode(&mut self, bytes: &[u8]) -> Result<Vec<i16>> {
        let mut out = Vec::new();
        let mut buf = vec![0i16; FRAME * self.channels];
        for p in packets(bytes)?.into_iter().take(crate::archive::MAX_FRAMES as usize) {
            let one = !p.is_empty() && opus::packet::get_nb_samples(p, SAMPLE_RATE).ok() == Some(FRAME);
            let n = self.dec.decode(if one { p } else { &[] }, &mut buf, false)?;
            out.extend_from_slice(&buf[..n.min(FRAME) * self.channels]);
        }
        Ok(out)
    }

    /// As [`decode`](Self::decode), for a chunk said to hold `frames` frames: a chunk whose
    /// packets do not number its frames is refused, so its sound cannot outrun its timing.
    pub fn decode_frames(&mut self, bytes: &[u8], frames: u16) -> Result<Vec<i16>> {
        if packets(bytes)?.len() != frames as usize {
            return Err(Error::Chunk);
        }
        self.decode(bytes)
    }

    /// A chunk's sound, always as long as its frames say: one that will not decode, or whose
    /// packets do not number its frames, is concealed for its length instead.
    pub fn decode_as(&mut self, bytes: &[u8], frames: u16) -> Vec<i16> {
        // Never more than a chunk can hold, whatever a chunk claims.
        let frames = frames.min(crate::archive::MAX_FRAMES);
        self.decode_frames(bytes, frames).or_else(|_| self.conceal(frames)).unwrap_or_default()
    }

    /// Fills a lost chunk with Opus's own concealment, so a gap is a soft dip.
    pub fn conceal(&mut self, frames: u16) -> Result<Vec<i16>> {
        let frames = frames.min(crate::archive::MAX_FRAMES);
        let mut out = Vec::new();
        let mut buf = vec![0i16; FRAME * self.channels];
        for _ in 0..frames {
            let n = self.dec.decode(&[], &mut buf, false)?;
            out.extend_from_slice(&buf[..n * self.channels]);
        }
        Ok(out)
    }
}

#[cfg(feature = "codec")]
/// Decodes one chunk on its own, as a listener tuning in mid-stream does.
pub fn decode_chunk(bytes: &[u8], channels: u8) -> Result<Vec<i16>> {
    ChunkDecoder::new(channels)?.decode(bytes)
}
