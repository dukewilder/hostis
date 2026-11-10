//! The station's log. Every act of a station (raising its colours, adding a track,
//! airing a live chunk, striking) is an entry, signed by the station and linked to
//! the entry before it by hash. Anyone carrying the log can check every link.

use serde::{Deserialize, Serialize};

use crate::{Error, Hash, Result, StationId, StationKey};

const DOMAIN: &[u8] = b"pyrate/entry/v2\0";

/// Six seconds of Opus, addressed by the BLAKE3 hash of its bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Chunk {
    pub hash: Hash,
    pub len: u32,
    /// 20 ms Opus frames in the chunk. 300 in a full one.
    pub frames: u16,
}

impl Chunk {
    pub fn of(bytes: &[u8], frames: u16) -> Self {
        Self { hash: *blake3::hash(bytes).as_bytes(), len: bytes.len() as u32, frames }
    }

    pub fn check(&self, bytes: &[u8]) -> Result<()> {
        if bytes.len() as u32 != self.len || blake3::hash(bytes).as_bytes() != &self.hash {
            return Err(Error::ChunkHash);
        }
        Ok(())
    }

    pub fn duration_ms(&self) -> u64 {
        self.frames as u64 * 20
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Body {
    /// The station's colours. The latest one in the log is the one flown.
    Colours {
        name: String,
        about: String,
        /// A declared city, for Local. Never the listener's.
        port: Option<String>,
        /// A Monero or Bitcoin address for tips, paid by the listener's own wallet.
        tip: Option<String>,
    },
    /// A recording added to the station's library, for the clock to play.
    Track {
        title: String,
        by: String,
        /// Where it came from and why it is free to air, in the station's words.
        source: String,
        channels: u8,
        bitrate: u32,
        chunks: Vec<Chunk>,
    },
    /// A recording taken out of the library. Its chunks may still be carried until dropped.
    Withdraw { track: Hash },
    /// Six seconds of a live broadcast.
    Live { chunk: Chunk, channels: u8, bitrate: u32 },
    /// The end of the station. Honest nodes drop its archive.
    Strike,
    /// The station's standing schedule. The latest one rules.
    Rota(crate::clock::Rota),
    /// Master only: lets another key broadcast for the station. A phone holds a deputy;
    /// the 24 words of the master stay off it.
    Deputy { key: [u8; 32], label: String },
    /// Master only: a deputy's key stops counting, from this entry on.
    Revoke { key: [u8; 32] },
    /// An entry written by a deputy. The signature on it is the deputy's.
    Deputed {
        by: [u8; 32],
        #[serde(with = "framed_box")]
        body: Box<Body>,
    },
    /// A new version of the app, carried like a recording. Phones trust it only when two of
    /// the three release keys have signed it, whoever wrote the entry.
    Release { version: String, code: u32, sha256: [u8; 32], size: u64, chunks: Vec<Chunk>, notes: String, sigs: Vec<(u8, Vec<u8>)> },
    /// The station's art and the colour its pages are drawn in. The latest one rules.
    Art { cover: Option<ArtRef>, banner: Option<ArtRef>, accent: [u8; 3] },
    /// The picture that shows while a track plays: usually its album cover.
    TrackArt { track: Hash, art: ArtRef },
    /// What kind of station this is, for the Log's filters, and whether it calls itself mature.
    /// The station's own word: nobody else can set it. The latest one rules.
    Details { kinds: Vec<String>, mature: bool },
    /// Whether the station calls itself explicit: sexual or graphic material, a step past
    /// mature. The station's own word. Listeners see explicit stations only if they choose to.
    Rating { explicit: bool },
    /// An entry from a later version of the app. Kept and passed on exactly as it came, so
    /// old and new nodes share one log; this version does nothing with it.
    ///
    /// The rule that keeps this working: a variant, once released, never changes. New
    /// information goes in a new variant.
    #[serde(skip)]
    Unknown(Vec<u8>),
}

/// Bodies travel inside a length prefix, so a node can step over a variant it does not know.
mod framed {
    use super::Body;
    use serde::{Deserializer, Serializer};

    pub fn encode(b: &Body) -> Vec<u8> {
        match b {
            Body::Unknown(raw) => raw.clone(),
            b => postcard::to_stdvec(b).unwrap_or_default(),
        }
    }

    pub fn decode(raw: Vec<u8>) -> Body {
        match postcard::take_from_bytes::<Body>(&raw) {
            Ok((b, rest)) if rest.is_empty() => b,
            _ => Body::Unknown(raw),
        }
    }

    pub fn serialize<S: Serializer>(b: &Body, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_bytes(&encode(b))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Body, D::Error> {
        Ok(decode(serde_bytes_vec(d)?))
    }

    fn serde_bytes_vec<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        struct V;
        impl<'de> serde::de::Visitor<'de> for V {
            type Value = Vec<u8>;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("bytes")
            }
            fn visit_bytes<E: serde::de::Error>(self, v: &[u8]) -> Result<Vec<u8>, E> {
                Ok(v.to_vec())
            }
            fn visit_byte_buf<E: serde::de::Error>(self, v: Vec<u8>) -> Result<Vec<u8>, E> {
                Ok(v)
            }
        }
        d.deserialize_bytes(V)
    }
}

mod framed_box {
    use super::Body;
    use serde::{Deserializer, Serializer};

    pub fn serialize<S: Serializer>(b: &Box<Body>, s: S) -> Result<S::Ok, S::Error> {
        super::framed::serialize(b, s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Box<Body>, D::Error> {
        super::framed::deserialize(d).map(Box::new)
    }
}

/// A picture named by its fingerprint. Only plain JPEGs of the sizes in `art` count.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtRef {
    pub hash: Hash,
    pub w: u16,
    pub h: u16,
}

/// What each release key signs.
pub fn release_message(code: u32, sha256: &[u8; 32]) -> Vec<u8> {
    let mut m = b"pyrate/release/v1\0".to_vec();
    m.extend_from_slice(&code.to_le_bytes());
    m.extend_from_slice(sha256);
    m
}

/// True when at least two different keys of `keys` have signed this release.
pub fn release_trusted(keys: &[[u8; 32]], code: u32, sha256: &[u8; 32], sigs: &[(u8, Vec<u8>)]) -> bool {
    let msg = release_message(code, sha256);
    let mut good = std::collections::HashSet::new();
    for (i, sig) in sigs {
        let (Some(k), Ok(sig)) = (keys.get(*i as usize), <[u8; 64]>::try_from(sig.as_slice())) else { continue };
        if StationId(*k).verify(&msg, &sig).is_ok() {
            good.insert(*i);
        }
    }
    good.len() >= 2
}

impl Body {
    /// What the entry says, whoever signed it.
    pub fn inner(&self) -> &Body {
        match self {
            Body::Deputed { body, .. } => body.inner(),
            b => b,
        }
    }

    /// Acts only the master key may sign. An entry this version cannot read is never
    /// taken as one of them.
    pub fn master_only(&self) -> bool {
        matches!(self, Body::Deputy { .. } | Body::Revoke { .. } | Body::Strike | Body::Deputed { .. })
    }
}



#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub station: StationId,
    pub seq: u64,
    /// Hash of the previous signed entry. Zero for the first.
    pub prev: Hash,
    /// Milliseconds since the Unix epoch, by the station's clock.
    pub at_ms: u64,
    #[serde(with = "framed")]
    pub body: Body,
}

impl Entry {
    /// What the entry says, whoever signed it.
    pub fn body(&self) -> &Body {
        self.body.inner()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedEntry {
    pub entry: Entry,
    #[serde(with = "sig_bytes")]
    pub sig: [u8; 64],
}

impl SignedEntry {
    fn message(entry: &Entry) -> Result<Vec<u8>> {
        let mut m = DOMAIN.to_vec();
        m.extend(postcard::to_stdvec(entry)?);
        Ok(m)
    }

    pub fn sign(key: &StationKey, entry: Entry) -> Result<Self> {
        let sig = key.sign(&Self::message(&entry)?);
        Ok(Self { entry, sig })
    }

    /// Checks the signature against the key the entry claims: the station's, or a deputy's.
    pub fn verify(&self) -> Result<()> {
        let signer = match &self.entry.body {
            Body::Deputed { by, .. } => StationId(*by),
            _ => self.entry.station,
        };
        signer.verify(&Self::message(&self.entry)?, &self.sig)
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        Ok(postcard::to_stdvec(self)?)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        Ok(postcard::from_bytes(bytes)?)
    }

    /// The hash the next entry points back to. Also the entry's address as a blob.
    pub fn hash(&self) -> Result<Hash> {
        Ok(*blake3::hash(&self.to_bytes()?).as_bytes())
    }
}

/// One station's log, checked link by link as it grows.
#[derive(Clone, Debug)]
pub struct Archive {
    station: StationId,
    entries: Vec<SignedEntry>,
    head: Hash,
    deputies: std::collections::HashSet<[u8; 32]>,
}

impl Archive {
    pub fn new(station: StationId) -> Self {
        Self { station, entries: Vec::new(), head: [0; 32], deputies: Default::default() }
    }

    pub fn station(&self) -> StationId {
        self.station
    }

    pub fn entries(&self) -> &[SignedEntry] {
        &self.entries
    }

    pub fn len(&self) -> u64 {
        self.entries.len() as u64
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn head(&self) -> Hash {
        self.head
    }

    pub fn is_struck(&self) -> bool {
        matches!(self.entries.last(), Some(e) if e.entry.body == Body::Strike)
    }

    /// Deputy keys currently allowed to write for the station.
    pub fn deputies(&self) -> Vec<[u8; 32]> {
        self.deputies.iter().copied().collect()
    }

    /// Checks an entry against the log and appends it. Nothing invalid gets in.
    pub fn append(&mut self, signed: SignedEntry) -> Result<()> {
        let e = &signed.entry;
        if e.station != self.station {
            return Err(Error::WrongStation);
        }
        if self.is_struck() {
            return Err(Error::Struck);
        }
        if e.seq != self.len() {
            return Err(Error::Sequence { expected: self.len(), got: e.seq });
        }
        if e.prev != self.head {
            return Err(Error::Chain);
        }
        if let Some(last) = self.entries.last() {
            if e.at_ms < last.entry.at_ms {
                return Err(Error::Time);
            }
        }
        if let Body::Deputed { by, body } = &e.body {
            if !self.deputies.contains(by) || body.master_only() {
                return Err(Error::Signature);
            }
        }
        signed.verify()?;
        match &e.body {
            Body::Deputy { key, .. } => {
                self.deputies.insert(*key);
            }
            Body::Revoke { key } => {
                self.deputies.remove(key);
            }
            _ => {}
        }
        self.head = signed.hash()?;
        self.entries.push(signed);
        Ok(())
    }

    /// Signs and appends a new entry. Only the holder of the key can do this.
    pub fn write(&mut self, key: &StationKey, at_ms: u64, body: Body) -> Result<&SignedEntry> {
        let entry = Entry { station: self.station, seq: self.len(), prev: self.head, at_ms, body };
        let signed = SignedEntry::sign(key, entry)?;
        self.append(signed)?;
        Ok(self.entries.last().expect("just appended"))
    }

    /// The colours currently flown, if any.
    pub fn colours(&self) -> Option<&Body> {
        self.entries.iter().rev().map(|e| e.entry.body()).find(|b| matches!(b, Body::Colours { .. }))
    }

    /// The rota in force, or the default: the whole library, shuffled, all day.
    pub fn rota(&self) -> crate::clock::Rota {
        self.entries.iter().rev().find_map(|e| match e.entry.body() {
            Body::Rota(r) => Some(r.clone()),
            _ => None,
        }).unwrap_or_default()
    }

    /// Whether the station calls itself explicit. The latest word rules.
    pub fn explicit(&self) -> bool {
        self.entries.iter().rev().find_map(|e| match e.entry.body() {
            Body::Rating { explicit } => Some(*explicit),
            _ => None,
        }).unwrap_or(false)
    }

    /// The station's own word on what it is, if it has said.
    pub fn details(&self) -> Option<(Vec<String>, bool)> {
        self.entries.iter().rev().find_map(|e| match e.entry.body() {
            Body::Details { kinds, mature } => Some((kinds.clone(), *mature)),
            _ => None,
        })
    }

    /// The station's latest art, if it has raised any.
    pub fn art(&self) -> Option<(Option<ArtRef>, Option<ArtRef>, [u8; 3])> {
        self.entries.iter().rev().find_map(|e| match e.entry.body() {
            Body::Art { cover, banner, accent } => Some((*cover, *banner, *accent)),
            _ => None,
        })
    }

    /// The art for one track, by the track's entry hash.
    pub fn track_art(&self, track: &Hash) -> Option<ArtRef> {
        self.entries.iter().rev().find_map(|e| match e.entry.body() {
            Body::TrackArt { track: t, art } if t == track => Some(*art),
            _ => None,
        })
    }

    /// Every picture this station's log names.
    pub fn art_hashes(&self) -> Vec<Hash> {
        let mut out = Vec::new();
        for e in &self.entries {
            match e.entry.body() {
                Body::Art { cover, banner, .. } => out.extend(cover.iter().chain(banner.iter()).map(|a| a.hash)),
                Body::TrackArt { art, .. } => out.push(art.hash),
                _ => {}
            }
        }
        out
    }

    /// Tracks in the library, oldest first, minus any withdrawn. Each with its entry hash.
    pub fn library(&self) -> Result<Vec<(Hash, &SignedEntry)>> {
        let mut withdrawn = std::collections::HashSet::new();
        for e in &self.entries {
            if let Body::Withdraw { track } = e.entry.body() {
                withdrawn.insert(*track);
            }
        }
        let mut out = Vec::new();
        for e in &self.entries {
            if matches!(e.entry.body(), Body::Track { .. }) {
                let h = e.hash()?;
                if !withdrawn.contains(&h) {
                    out.push((h, e));
                }
            }
        }
        Ok(out)
    }
}

mod sig_bytes {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(sig: &[u8; 64], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_bytes(sig)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<[u8; 64], D::Error> {
        let v: &[u8] = Deserialize::deserialize(d)?;
        v.try_into().map_err(|_| serde::de::Error::invalid_length(v.len(), &"64 bytes"))
    }
}

/// Logs written before bodies were framed (until 9 November 2026). Read only to carry a
/// station's colours and art into a fresh log; nothing else in them survives the change.
pub mod legacy {
    use super::*;

    #[derive(Deserialize)]
    struct OldEntry {
        #[allow(dead_code)]
        station: StationId,
        #[allow(dead_code)]
        seq: u64,
        #[allow(dead_code)]
        prev: Hash,
        at_ms: u64,
        body: Body,
    }

    #[derive(Deserialize)]
    struct OldSigned {
        entry: OldEntry,
    }

    /// Every entry of an old log this version can read, in order: when it was written, what it
    /// said, and the hash it was known by. Entries written by a deputy are left out.
    pub fn entries(bytes: &[u8]) -> Vec<(u64, Body, Hash)> {
        let mut out = Vec::new();
        let mut at = 0;
        while at + 4 <= bytes.len() {
            let n = u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
            at += 4;
            if at + n > bytes.len() {
                break;
            }
            let rec = &bytes[at..at + n];
            if let Ok(e) = postcard::from_bytes::<OldSigned>(rec) {
                out.push((e.entry.at_ms, e.entry.body, *blake3::hash(rec).as_bytes()));
            }
            at += n;
        }
        out
    }

    /// The last colours and art found in an old log file's length-prefixed records.
    pub fn colours_and_art(bytes: &[u8]) -> (Option<Body>, Option<Body>) {
        let (mut colours, mut art) = (None, None);
        let mut at = 0;
        while at + 4 <= bytes.len() {
            let n = u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
            at += 4;
            if at + n > bytes.len() {
                break;
            }
            if let Ok(e) = postcard::from_bytes::<OldSigned>(&bytes[at..at + n]) {
                match e.entry.body {
                    b @ Body::Colours { .. } => colours = Some(b),
                    b @ Body::Art { .. } => art = Some(b),
                    _ => {}
                }
            }
            at += n;
        }
        (colours, art)
    }
}
