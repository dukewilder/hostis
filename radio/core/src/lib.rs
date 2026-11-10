//! pyrate-core: the parts of Pyrate Radio every build shares.
//!
//! A station is a key. Everything it airs is an entry in its own signed,
//! hash-linked, append-only log. Audio travels as six-second Opus chunks,
//! addressed by their BLAKE3 hash, so any node can carry them and any node
//! can prove where they came from.
//!
//! No rights reserved.

pub mod archive;
pub mod art;
pub mod audio;
pub mod callsign;
pub mod chat;
pub mod clock;
#[cfg(feature = "codec")]
pub mod ingest;
pub mod keys;
pub mod page;
pub mod pocket;
pub mod programme;
pub mod seal;
pub mod tape;

pub use archive::{ArtRef, Archive, Body, Chunk, Entry, SignedEntry, release_message, release_trusted};
pub use clock::{NowPlaying, Rota};
pub use programme::{Fill, Mode, Programme, Slot, Span, Zone};
pub use audio::Profile;
#[cfg(feature = "codec")]
pub use audio::{ChunkEncoder, decode_chunk};
pub use keys::{StationId, StationKey};

/// A BLAKE3 hash. The same hash iroh-blobs uses, so chunks are blobs as they stand.
pub type Hash = [u8; 32];

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("bad word phrase: {0}")]
    Phrase(String),
    #[error("bad key")]
    Key,
    #[error("signature does not verify")]
    Signature,
    #[error("entry is for another station")]
    WrongStation,
    #[error("expected entry {expected}, got {got}")]
    Sequence { expected: u64, got: u64 },
    #[error("entry does not follow the last one")]
    Chain,
    #[error("entry is dated before the one it follows")]
    Time,
    #[error("the station has struck its colours")]
    Struck,
    #[error("chunk does not match its hash")]
    ChunkHash,
    #[error("malformed chunk")]
    Chunk,
    #[error("encoding: {0}")]
    Encoding(#[from] postcard::Error),
    #[cfg(feature = "codec")]
    #[error("opus: {0}")]
    Opus(#[from] opus::Error),
    #[error("cannot read audio: {0}")]
    Decode(String),
    #[error("resampling: {0}")]
    Resample(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
