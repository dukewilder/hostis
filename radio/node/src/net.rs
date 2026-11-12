//! What passes between nodes.
//!
//! Requests go over `pyrate/0`: one request per stream, one answer back.
//! The Log goes over gossip, on one topic every node joins.

use pyrate_core::{Hash, StationId, StationKey};
use serde::{Deserialize, Serialize};

pub const ALPN: &[u8] = b"pyrate/1";
pub const MAX_FRAME: usize = 16 * 1024 * 1024;
/// Leading zero bits a station's proof of work must show. About a second on a phone, once.
pub const POW_BITS: u32 = 24;
/// Work behind each listener token: a second or two on a phone, once an hour.
pub const TOKEN_BITS: u32 = 20;

pub fn log_topic() -> [u8; 32] {
    *blake3::hash(b"pyrate/log/v2").as_bytes()
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Request {
    /// Signed entries from `from` onward, as postcard bytes.
    Log { station: StationId, from: u64 },
    Chunk { hash: Hash },
    /// A hop of a private route or a stem. See `hop.rs`.
    Hop(Vec<u8>),
    /// The last lines of a station's chatter that a node in the room still holds.
    Chatter { station: StationId },
    /// A chunk, sealed, by the name it is filed under. The carrier never learns its address
    /// and never opens it. See pyrate_core::seal.
    SealedChunk { name: Hash },
    /// A flock's host's clock and station, for a phone holding the flock's code.
    Flock { code: [u8; 8] },
    /// A pocket station's log, sealed, asked for by the tag its key draws: never by its id.
    PocketLog { tag: Hash, from: u64 },
    /// The last lines of a pocket's chatter, sealed, asked for by its tag. Answered with
    /// `Response::Chatter`. Older nodes do not know this request and answer nothing.
    PocketChatter { tag: Hash },
    /// The cards of the stations a node knows, so a newcomer's dial fills in one round trip
    /// instead of waiting on gossip, with what it needs to reach their carriers privately.
    /// Answered with `Response::Cards`; older nodes answer nothing.
    Cards,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Response {
    Entries(Vec<Vec<u8>>),
    Chunk(Option<Vec<u8>>),
    Hop(Vec<u8>),
    Chatter(Vec<Vec<u8>>),
    Sealed(Option<Vec<u8>>),
    Flock { now_ms: u64, station: Option<StationId> },
    /// Gossip as it would have come, each item the postcard bytes of a `Gossip`: signed station
    /// cards first, most listened first, then the carriers' own signed notes for them (which
    /// stations they carry, and the relay each can be reached through). Nothing in it is taken
    /// on the answerer's word: every card and note is checked against its own signature.
    Cards(Vec<Vec<u8>>),
}

/// The signed face of a station's card. Fields are only ever added at the end: an older
/// node reads the ones it knows, ignores the rest, and still checks the signature over every
/// byte, so new versions can say more without splitting the Log.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Card {
    pub station: StationId,
    pub head: u64,
    pub name: String,
    pub about: String,
    pub port: Option<String>,
    pub live: bool,
    pub at_ms: u64,
    pub pow: u64,
    /// When the station first went on the air, from its first entry.
    pub since: u64,
    /// When a deputy signs: the master-signed entry appointing it.
    pub proof: Option<Vec<u8>>,
    /// The station's cover and page colour.
    pub cover: Option<[u8; 32]>,
    pub accent: Option<[u8; 3]>,
    /// The station's own word on what it is: its kinds, and whether it calls itself mature.
    pub kinds: Vec<String>,
    pub mature: bool,
    /// Where tips go, paid from the listener's own wallet.
    pub tip: Option<String>,
    /// Whether the station calls itself explicit. Written after the fields above, so an
    /// older app reads the card as it always has; a card from before it reads as not explicit.
    #[serde(skip)]
    pub explicit: bool,
    /// How many recordings the station has to play, written after `explicit` the same way. A
    /// card from before it reads as None: not known. A listener's radio uses it to keep a
    /// station with nothing on it out of the way until it has something.
    #[serde(skip)]
    pub tracks: Option<u32>,
    /// Since when the station has worn its present name (see `Archive::named_since`), after
    /// `tracks`. Two stations with names that read alike are told apart by it.
    #[serde(skip)]
    pub named: Option<u64>,
}

const CARD_DOMAIN: &[u8] = b"pyrate/announce/v2\0";

/// The most a card may say: a name, a line about the station, a port, eight kinds, a tip
/// address and a deputy's appointment. A card past any of these is refused unread.
pub const CARD_NAME: usize = 64;
pub const CARD_ABOUT: usize = 600;
pub const CARD_PORT: usize = 120;
pub const CARD_KINDS: usize = 8;
pub const CARD_KIND: usize = 32;
pub const CARD_TIP: usize = 200;
pub const CARD_PROOF: usize = 1024;

fn clip(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

impl Card {
    pub fn sane(&self) -> bool {
        self.name.chars().count() <= CARD_NAME
            && self.about.chars().count() <= CARD_ABOUT
            && self.port.as_ref().is_none_or(|p| p.chars().count() <= CARD_PORT)
            && self.kinds.len() <= CARD_KINDS
            && self.kinds.iter().all(|k| k.chars().count() <= CARD_KIND)
            && self.tip.as_ref().is_none_or(|t| t.len() <= CARD_TIP)
            && self.proof.as_ref().is_none_or(|p| p.len() <= CARD_PROOF)
    }

    /// The card cut to what any node will take, for a station whose own words run long.
    pub fn clipped(mut self) -> Self {
        self.name = clip(&self.name, CARD_NAME);
        self.about = clip(&self.about, CARD_ABOUT);
        self.port = self.port.map(|p| clip(&p, CARD_PORT));
        self.kinds = self.kinds.into_iter().take(CARD_KINDS).map(|k| clip(&k, CARD_KIND)).collect();
        self.tip = self.tip.filter(|t| t.len() <= CARD_TIP);
        self
    }

    pub fn sign(self, key: &StationKey) -> Announce {
        let mut raw = postcard::to_stdvec(&self).unwrap_or_default();
        raw.extend(postcard::to_stdvec(&self.explicit).unwrap_or_default());
        if let Some(t) = self.tracks {
            raw.extend(postcard::to_stdvec(&t).unwrap_or_default());
            if let Some(n) = self.named {
                raw.extend(postcard::to_stdvec(&n).unwrap_or_default());
            }
        }
        let mut m = CARD_DOMAIN.to_vec();
        m.extend_from_slice(&raw);
        Announce { sig: key.sign(&m), card: self, raw }
    }
}

/// A station's card in the Log, signed by the station.
#[derive(Clone, Debug)]
pub struct Announce {
    pub card: Card,
    raw: Vec<u8>,
    pub sig: [u8; 64],
}

impl std::ops::Deref for Announce {
    type Target = Card;
    fn deref(&self) -> &Card {
        &self.card
    }
}

#[derive(Serialize, Deserialize)]
struct AnnounceWire {
    #[serde(with = "serde_bytes_vec")]
    card: Vec<u8>,
    #[serde(with = "serde_bytes64")]
    sig: [u8; 64],
}

mod serde_bytes_vec {
    use serde::{Deserialize, Deserializer, Serializer};
    pub fn serialize<S: Serializer>(v: &Vec<u8>, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_bytes(v)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        Vec::<u8>::deserialize(d)
    }
}

impl Serialize for Announce {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        AnnounceWire { card: self.raw.clone(), sig: self.sig }.serialize(s)
    }
}

/// The most bytes a card may take as it travels. Every field at its limit comes to about
/// 5 KB; a card past this is refused before it is read.
pub const CARD_WIRE: usize = 8 * 1024;
/// The most bytes a card may carry after the fields this version knows, for later versions'
/// fields. The signature covers them, so they travel; they are never a way to make a small
/// card large.
pub const CARD_EXTRA: usize = 512;

impl<'de> Deserialize<'de> for Announce {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let w = AnnounceWire::deserialize(d)?;
        if w.card.len() > CARD_WIRE {
            return Err(serde::de::Error::custom("card too large"));
        }
        // Trailing fields from later versions are ignored here and kept in `raw`, up to a limit.
        let (mut card, rest): (Card, &[u8]) = postcard::take_from_bytes(&w.card).map_err(serde::de::Error::custom)?;
        let rest = match postcard::take_from_bytes::<bool>(rest) {
            Ok((e, rest)) => {
                card.explicit = e;
                match postcard::take_from_bytes::<u32>(rest) {
                    Ok((t, rest)) => {
                        card.tracks = Some(t);
                        match postcard::take_from_bytes::<u64>(rest) {
                            Ok((n, rest)) => {
                                card.named = Some(n);
                                rest
                            }
                            Err(_) => rest,
                        }
                    }
                    Err(_) => rest,
                }
            }
            Err(_) => rest,
        };
        if rest.len() > CARD_EXTRA {
            return Err(serde::de::Error::custom("card carries too much after its fields"));
        }
        Ok(Announce { card, raw: w.card, sig: w.sig })
    }
}

impl Announce {
    fn message(&self) -> Vec<u8> {
        let mut m = CARD_DOMAIN.to_vec();
        m.extend_from_slice(&self.raw);
        m
    }

    pub fn verify(&self) -> bool {
        if self.raw.len() > CARD_WIRE || !self.card.sane() || !pow_ok(&self.station, self.pow) {
            return false;
        }
        let signer = match &self.proof {
            None => self.station,
            Some(bytes) => {
                let Ok(p) = pyrate_core::SignedEntry::from_bytes(bytes) else { return false };
                let pyrate_core::Body::Deputy { key, .. } = &p.entry.body else { return false };
                if p.entry.station != self.station || p.verify().is_err() {
                    return false;
                }
                StationId(*key)
            }
        };
        signer.verify(&self.message(), &self.sig).is_ok()
    }
}

/// Whether a card's signer is still the station's to trust, by this device's copy of the
/// station's log. A master's card always is. A deputy's is only once the log here reaches the
/// card's head and still names that deputy: until then nothing on the card that moves money,
/// the tip address, is shown.
pub fn deputy_current(a: &Announce, arch: Option<&pyrate_core::Archive>) -> bool {
    let Some(p) = &a.proof else { return true };
    let Some(arch) = arch else { return false };
    let Ok(e) = pyrate_core::SignedEntry::from_bytes(p) else { return false };
    let pyrate_core::Body::Deputy { key, .. } = &e.entry.body else { return false };
    arch.len() >= a.head && arch.deputies().contains(key)
}

fn pow_hash(station: &StationId, nonce: u64) -> u32 {
    let mut h = blake3::Hasher::new();
    h.update(b"pyrate/pow/v1");
    h.update(&station.0);
    h.update(&nonce.to_le_bytes());
    let b = h.finalize();
    u32::from_be_bytes(b.as_bytes()[..4].try_into().unwrap()).leading_zeros()
}

/// How much work a nonce shows for a station: the leading zero bits of its hash.
pub fn pow_bits(station: &StationId, nonce: u64) -> u32 {
    pow_hash(station, nonce)
}

/// A better nonce than `from` shows, searched over `span` nonces after `start`: what a
/// station does when the Log is full and a newcomer must outwork the weakest card in it.
pub fn pow_improve(station: &StationId, from: u64, start: u64, span: u64) -> Option<u64> {
    let have = pow_hash(station, from);
    (start..start.saturating_add(span)).find(|n| pow_hash(station, *n) > have)
}

pub fn pow_ok(station: &StationId, nonce: u64) -> bool {
    pow_hash(station, nonce) >= POW_BITS
}

/// The work a station does once, so raising ten thousand junk stations costs ten thousand times as much.
pub fn pow_find(station: &StationId) -> u64 {
    (0u64..).find(|n| pow_ok(station, *n)).unwrap()
}

fn token_work(station: &StationId, hour: u64, token: &[u8; 32], nonce: u64) -> u32 {
    let mut h = blake3::Hasher::new();
    h.update(b"pyrate/token/v1");
    h.update(&station.0);
    h.update(&hour.to_le_bytes());
    h.update(token);
    h.update(&nonce.to_le_bytes());
    u32::from_be_bytes(h.finalize().as_bytes()[..4].try_into().unwrap()).leading_zeros()
}

pub fn token_ok(station: &StationId, hour: u64, token: &[u8; 32], nonce: u64) -> bool {
    token_work(station, hour, token, nonce) >= TOKEN_BITS
}

pub fn token_find(station: &StationId, hour: u64, token: &[u8; 32]) -> u64 {
    (0u64..).find(|n| token_ok(station, hour, token, *n)).unwrap()
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Gossip {
    Station(Announce),
    /// A node saying it carries a station, and how far its copy of the log reaches.
    Carrier { station: StationId, node: [u8; 32], head: u64 },
    /// Listener tokens for one station in one hour, each with its proof of work. Every node
    /// checks them and counts for itself; nobody's sketch is taken on trust.
    Aboard { station: StationId, hour: u64, tokens: Vec<([u8; 32], u64)> },
    /// A carrier saying which relay it can be reached through, signed with its own node key, so
    /// nodes find each other inside the network without asking anyone's directory. Older nodes
    /// do not know this message and pass over it.
    Harbour { node: [u8; 32], relay: String, at_ms: u64, sig: Vec<u8> },
    /// A carrier note signed with the carrier's own node key, so nobody can say a node carries
    /// a station when it does not. Unsigned `Carrier` notes from the network are no longer
    /// believed.
    Carried { station: StationId, node: [u8; 32], head: u64, at_ms: u64, sig: Vec<u8> },
}

fn carried_message(station: &StationId, node: &[u8; 32], head: u64, at_ms: u64) -> Vec<u8> {
    let mut m = b"pyrate/carried/v1\0".to_vec();
    m.extend_from_slice(&station.0);
    m.extend_from_slice(node);
    m.extend_from_slice(&head.to_le_bytes());
    m.extend_from_slice(&at_ms.to_le_bytes());
    m
}

fn harbour_message(node: &[u8; 32], relay: &str, at_ms: u64) -> Vec<u8> {
    let mut m = b"pyrate/harbour/v1\0".to_vec();
    m.extend_from_slice(node);
    m.extend_from_slice(&at_ms.to_le_bytes());
    m.extend_from_slice(relay.as_bytes());
    m
}

impl Gossip {
    /// This node's relay, signed with the node's own secret.
    pub fn harbour(secret: &[u8; 32], relay: &str, at_ms: u64) -> Self {
        let key = StationKey::from_seed(*secret);
        let node = key.id().0;
        Gossip::Harbour { node, relay: relay.into(), at_ms, sig: key.sign(&harbour_message(&node, relay, at_ms)).to_vec() }
    }

    /// This node saying it carries a station, signed with the node's own secret.
    pub fn carried(secret: &[u8; 32], station: StationId, head: u64, at_ms: u64) -> Self {
        let key = StationKey::from_seed(*secret);
        let node = key.id().0;
        Gossip::Carried { station, node, head, at_ms, sig: key.sign(&carried_message(&station, &node, head, at_ms)).to_vec() }
    }

    /// A signed carrier note as the plain kind, if the node itself signed it within fifteen
    /// minutes of `now` (and not more than five ahead).
    pub fn carried_checked(&self, now: u64) -> Option<Gossip> {
        let Gossip::Carried { station, node, head, at_ms, sig } = self else { return None };
        if now.saturating_sub(*at_ms) > 15 * 60_000 || at_ms.saturating_sub(now) > 5 * 60_000 {
            return None;
        }
        let sig: [u8; 64] = sig.as_slice().try_into().ok()?;
        StationId(*node).verify(&carried_message(station, node, *head, *at_ms), &sig).ok()?;
        Some(Gossip::Carrier { station: *station, node: *node, head: *head })
    }

    /// What may be taken from the network: a signed carrier note opened, an unsigned one only
    /// from the neighbour it names (its own connection vouches for it; older nodes send no
    /// other kind), everything else as it came.
    pub fn from_network(self, now: u64, from: &[u8; 32]) -> Option<Gossip> {
        match self {
            g @ Gossip::Carrier { node, .. } => (node == *from).then_some(g),
            g @ Gossip::Carried { .. } => g.carried_checked(now),
            g => Some(g),
        }
    }

    /// For a harbour note: the node and its relay, if the node itself signed it near `now`.
    pub fn harbour_checked(&self, now: u64) -> Option<([u8; 32], String)> {
        let Gossip::Harbour { node, relay, at_ms, sig } = self else { return None };
        let sig: [u8; 64] = sig.as_slice().try_into().ok()?;
        if relay.len() > 256 || at_ms.abs_diff(now) > 15 * 60_000 {
            return None;
        }
        StationId(*node).verify(&harbour_message(node, relay, *at_ms), &sig).ok()?;
        relay_public(relay).then(|| (*node, relay.clone()))
    }
}

/// How many items a `Cards` answer may hold: six hundred cards and the notes beside them.
pub const CARDS_ITEMS: usize = 4_000;

/// The latest signed notes heard from carriers, as they came, for passing on with cards: per
/// node, the relay it named; per station and node, its carrier note. Each is kept only if its
/// own signature holds, and only the newest of each.
#[derive(Default)]
pub struct Notes {
    harbour: std::collections::HashMap<[u8; 32], (u64, Vec<u8>)>,
    carried: std::collections::HashMap<(StationId, [u8; 32]), (u64, Vec<u8>)>,
}

impl Notes {
    const HARBOURS: usize = 4_000;
    const CARRIED: usize = 20_000;
    /// Notes older than this are not handed on, so the one who takes them still believes them.
    const FRESH_MS: u64 = 14 * 60_000;

    pub fn keep(&mut self, g: &Gossip, raw: &[u8], now: u64) {
        if raw.len() > 1024 {
            return;
        }
        match g {
            Gossip::Harbour { node, at_ms, .. } if g.harbour_checked(now).is_some() => {
                if self.harbour.get(node).is_none_or(|(t, _)| t < at_ms) {
                    self.harbour.insert(*node, (*at_ms, raw.to_vec()));
                }
                if self.harbour.len() > Self::HARBOURS {
                    Self::trim(&mut self.harbour, now, Self::HARBOURS);
                }
            }
            Gossip::Carried { station, node, at_ms, .. } if g.carried_checked(now).is_some() => {
                let k = (*station, *node);
                if self.carried.get(&k).is_none_or(|(t, _)| t < at_ms) {
                    self.carried.insert(k, (*at_ms, raw.to_vec()));
                }
                if self.carried.len() > Self::CARRIED {
                    Self::trim(&mut self.carried, now, Self::CARRIED);
                }
            }
            _ => {}
        }
    }

    /// Stale notes go first, then the oldest, down to nine tenths of the cap.
    fn trim<K: Clone + Eq + std::hash::Hash>(m: &mut std::collections::HashMap<K, (u64, Vec<u8>)>, now: u64, cap: usize) {
        m.retain(|_, (t, _)| now.saturating_sub(*t) < Self::FRESH_MS);
        if m.len() > cap {
            let mut ages: Vec<(u64, K)> = m.iter().map(|(k, (t, _))| (*t, k.clone())).collect();
            ages.sort_by_key(|a| a.0);
            for (_, k) in ages.into_iter().take(m.len() - cap * 9 / 10) {
                m.remove(&k);
            }
        }
    }

    /// For these stations, in their order: the four freshest carrier notes of each, then the
    /// relay note of every carrier named. Nothing for any other station.
    pub fn for_stations(&self, stations: &[StationId], now: u64) -> Vec<Vec<u8>> {
        let fresh = |t: &u64| now.saturating_sub(*t) < Self::FRESH_MS && t.saturating_sub(now) < 5 * 60_000;
        let mut by: std::collections::HashMap<StationId, Vec<(u64, [u8; 32], &Vec<u8>)>> = Default::default();
        for ((st, node), (t, raw)) in &self.carried {
            if fresh(t) {
                by.entry(*st).or_default().push((*t, *node, raw));
            }
        }
        let mut out = Vec::new();
        let mut named = Vec::new();
        for st in stations {
            let Some(mut l) = by.remove(st) else { continue };
            l.sort_by(|a, b| b.0.cmp(&a.0));
            for (_, node, raw) in l.into_iter().take(4) {
                out.push(raw.clone());
                if !named.contains(&node) {
                    named.push(node);
                }
            }
        }
        for node in named {
            if let Some((t, raw)) = self.harbour.get(&node).filter(|(t, _)| fresh(t)) {
                let _ = t;
                out.push(raw.clone());
            }
        }
        out.truncate(CARDS_ITEMS);
        out
    }
}

/// Set on a node whose own relays are on loopback: a private test network, where the Harbour
/// notes name loopback relays too.
pub static LOCAL_RELAYS_OK: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Whether a relay a peer advertises may go in the address book: http or https, and a public
/// host. A signed note proves who sent it, not that the address is somewhere this node should
/// reach, so loopback, private, link-local and local-only names are refused.
pub fn relay_public(relay: &str) -> bool {
    let Some(rest) = relay.strip_prefix("https://").or_else(|| relay.strip_prefix("http://")) else { return false };
    let host_port = rest.split(['/', '?', '#']).next().unwrap_or("");
    if host_port.is_empty() || host_port.contains('@') {
        return false;
    }
    let host = if let Some(v6) = host_port.strip_prefix('[') { v6.split(']').next().unwrap_or("") } else { host_port.rsplit_once(':').map(|(h, _)| h).unwrap_or(host_port) };
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    let local_ok = LOCAL_RELAYS_OK.load(std::sync::atomic::Ordering::Relaxed);
    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        if ip.is_loopback() {
            return local_ok;
        }
        return match ip {
            std::net::IpAddr::V4(v) => !(v.is_private() || v.is_link_local() || v.is_unspecified() || v.is_broadcast() || v.is_multicast() || v.is_documentation() || v.octets()[0] == 0 || (v.octets()[0] == 100 && (64..128).contains(&v.octets()[1]))),
            std::net::IpAddr::V6(v) => {
                let seg = v.segments()[0];
                !(v.is_unspecified() || v.is_multicast() || (seg & 0xfe00) == 0xfc00 || (seg & 0xffc0) == 0xfe80 || v.to_ipv4_mapped().is_some())
            }
        };
    }
    if host == "localhost" || host.ends_with(".localhost") {
        return local_ok;
    }
    // A name is checked here but resolved later, and could then point anywhere. Over https the
    // relay must prove that name with a certificate before a byte of HTTP is sent, so a name
    // turned to a private address gets nowhere; plain http has no such proof, so it takes a
    // public address written out and no name.
    if relay.starts_with("http://") {
        return false;
    }
    !host.is_empty() && host.contains('.') && host.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-') && ![".local", ".internal", ".lan", ".home.arpa", ".home", ".corp"].iter().any(|t| host.ends_with(t))
}

#[cfg(test)]
mod relay_tests {
    use super::relay_public;

    #[test]
    fn only_public_relays_are_taken() {
        for ok in ["https://relay.hostis.black/", "http://45.87.214.21:42779/", "https://euw1-1.relay.iroh.network./"] {
            assert!(relay_public(ok), "{ok}");
        }
        for bad in ["http://127.0.0.1:3340", "http://localhost:3340/", "http://10.0.0.5/", "http://192.168.1.1:80/", "http://169.254.169.254/", "http://[::1]:3340/", "http://[fd00::1]/", "ftp://relay.example.org/", "http://user@relay.example.org/", "http://printer.local/", "http://100.64.0.1/", "http://0.0.0.0/", "http://relay.example.org/"] {
            assert!(!relay_public(bad), "{bad}");
        }
    }
}

/// Copies of every chunk the fleet aims to keep between its ballast holds.
pub const SHARE_COPIES: f64 = 8.0;

/// Where a sealed chunk falls for a node, from 0 to 1: the node holds it as ballast when this
/// is under its share. Anyone can work it out from the node's id and the chunk's sealed name,
/// so a listener asks the likely holders first, and nobody had to agree on anything.
pub fn share_score(node: &[u8; 32], name: &Hash) -> f64 {
    let mut h = blake3::Hasher::new_derive_key("pyrate/share/v1");
    h.update(node);
    h.update(name);
    let b = h.finalize();
    u64::from_le_bytes(b.as_bytes()[..8].try_into().unwrap()) as f64 / u64::MAX as f64
}

/// The share of the Log each node holds so the whole keeps about SHARE_COPIES copies:
/// everything while the fleet is small, less as it grows.
pub fn share_of(fleet: usize) -> f64 {
    // PYRATE_SHARE_COPIES: fewer copies, so a test fleet of five already splits the Log.
    let copies = std::env::var("PYRATE_SHARE_COPIES").ok().and_then(|v| v.parse().ok()).unwrap_or(SHARE_COPIES);
    (copies / fleet.max(1) as f64).min(1.0)
}

/// A node's ballast from the chunks on offer, (sealed name, length): those whose score falls
/// under the share, lowest first, up to the budget. A node with less room keeps the start of
/// the same list, so a listener asking by score still finds what it kept.
pub fn pick_shares(node: &[u8; 32], chunks: &[(Hash, u64)], share: f64, budget: u64) -> Vec<(Hash, u64)> {
    let mut v: Vec<(f64, Hash, u64)> = chunks.iter().map(|(n, l)| (share_score(node, n), *n, *l)).filter(|(s, _, _)| *s < share).collect();
    v.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut used = 0u64;
    v.into_iter()
        .take_while(|(_, _, l)| {
            used += l;
            used <= budget
        })
        .map(|(_, n, l)| (n, l))
        .collect()
}

/// Carriers a station's listing keeps: the freshest claims, the oldest giving way.
pub const MAX_CARRIERS: usize = 64;
/// How long a carrier that failed to answer is tried only after the others.
pub const FAILED_MS: u64 = 10 * 60_000;

/// Takes in a carrier's claim. Anyone can say they carry anything, so a claim never reaches
/// past the station's own card, and a full list decides who stays like this:
///
/// - A carrier this node trusts (one that has served it a station's entries or sound, or a
///   harbour) is never pushed out by one it does not.
/// - Otherwise a carrier that failed lately, or has gone quiet for ten minutes, gives way first.
/// - Failing that, a trusted newcomer takes the place of the stalest stranger, and an untrusted
///   one takes the place of a stranger chosen at random one time in four (`coin`).
///
/// So claims that arrive first and keep saying they are there cannot keep a real carrier out:
/// it gets in within a few of its claims, is asked, serves, and from then on is trusted. And a
/// flood that arrives later cannot push out the carriers that have served this node.
#[allow(clippy::too_many_arguments)]
pub fn note_carrier(
    carriers: &mut std::collections::HashMap<[u8; 32], (u64, u64)>,
    node: [u8; 32],
    head: u64,
    card_head: Option<u64>,
    now: u64,
    trusted: impl Fn(&[u8; 32]) -> bool,
    failed: impl Fn(&[u8; 32]) -> bool,
    coin: u64,
) {
    let head = card_head.map_or(head, |c| head.min(c));
    if carriers.contains_key(&node) || carriers.len() < MAX_CARRIERS {
        carriers.insert(node, (head, now));
        return;
    }
    let strangers: Vec<([u8; 32], u64)> = carriers.iter().filter(|(n, _)| !trusted(n)).map(|(n, (_, seen))| (*n, *seen)).collect();
    let spent = strangers.iter().filter(|(n, seen)| failed(n) || now.saturating_sub(*seen) > CARRIER_STALE_MS).min_by_key(|(_, seen)| *seen);
    let victim = match spent {
        Some((n, _)) => Some(*n),
        None if trusted(&node) => strangers.iter().min_by_key(|(_, seen)| *seen).map(|(n, _)| *n),
        None if !strangers.is_empty() && coin % 4 == 0 => {
            let mut s = strangers.clone();
            s.sort_unstable();
            Some(s[(coin / 4 % s.len() as u64) as usize].0)
        }
        None => carriers.iter().filter(|(_, (_, seen))| now.saturating_sub(*seen) > CARRIER_STALE_MS).min_by_key(|(_, (_, seen))| *seen).map(|(n, _)| *n),
    };
    if let Some(v) = victim {
        carriers.remove(&v);
        carriers.insert(node, (head, now));
    }
}

/// How long a carrier may go unheard before a newcomer may take its place on a full list.
pub const CARRIER_STALE_MS: u64 = 10 * 60_000;

/// Carriers in the order to ask them: any that failed lately last, those that have served this
/// node before the rest, then the furthest copy of the log (no claim counting past the card),
/// then the freshest word.
pub fn rank_carriers(carriers: &std::collections::HashMap<[u8; 32], (u64, u64)>, me: &[u8; 32], card_head: Option<u64>, failed: impl Fn(&[u8; 32]) -> bool, trusted: impl Fn(&[u8; 32]) -> bool) -> Vec<[u8; 32]> {
    let mut v: Vec<(bool, bool, u64, u64, [u8; 32])> = carriers
        .iter()
        .filter(|(n, _)| *n != me)
        .map(|(n, (h, s))| (failed(n), !trusted(n), card_head.map_or(*h, |c| (*h).min(c)), *s, *n))
        .collect();
    v.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)).then(b.2.cmp(&a.2)).then(b.3.cmp(&a.3)));
    v.into_iter().map(|x| x.4).collect()
}

/// HyperLogLog with 256 registers. 256 bytes on the wire, about 6.5 percent error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sketch(pub [u8; 256]);

impl Default for Sketch {
    fn default() -> Self {
        Self([0; 256])
    }
}

impl Sketch {
    pub fn add(&mut self, token: &[u8; 32]) {
        let t = blake3::derive_key("pyrate/sketch/v1", token);
        let i = t[0] as usize;
        let w = u64::from_le_bytes(t[1..9].try_into().unwrap());
        let rank = (w.leading_zeros() + 1).min(32) as u8;
        self.0[i] = self.0[i].max(rank);
    }

    pub fn merge(&mut self, other: &[u8]) {
        for (a, b) in self.0.iter_mut().zip(other) {
            *a = (*a).max(*b);
        }
    }

    pub fn estimate(&self) -> u64 {
        let m = 256f64;
        let alpha = 0.7213 / (1.0 + 1.079 / m);
        let sum: f64 = self.0.iter().map(|r| 2f64.powi(-(*r as i32))).sum();
        let raw = alpha * m * m / sum;
        let zeros = self.0.iter().filter(|r| **r == 0).count();
        if raw <= 2.5 * m && zeros > 0 {
            (m * (m / zeros as f64).ln()).round() as u64
        } else {
            raw.round() as u64
        }
    }
}

/// "About 40". Counts are approximate, and the app says so.
pub fn round_aboard(n: u64) -> u64 {
    match n {
        0..=10 => n,
        11..=100 => (n + 2) / 5 * 5,
        101..=1000 => (n + 5) / 10 * 10,
        _ => (n + 50) / 100 * 100,
    }
}

mod serde_bytes64 {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(sig: &[u8; 64], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_bytes(sig)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<[u8; 64], D::Error> {
        let v: Vec<u8> = Deserialize::deserialize(d)?;
        v.as_slice().try_into().map_err(|_| serde::de::Error::invalid_length(v.len(), &"64 bytes"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_pass_on_only_signed_fresh_notes_for_the_stations_shown() {
        let now = 50_000_000;
        let (a, b) = ([5u8; 32], [6u8; 32]);
        let (shown, hidden) = (StationId([9; 32]), StationId([8; 32]));
        let mut n = Notes::default();
        let raw = |g: &Gossip| postcard::to_stdvec(g).unwrap();
        for g in [Gossip::carried(&a, shown, 3, now), Gossip::carried(&b, hidden, 3, now), Gossip::harbour(&a, "https://relay.example.org/", now), Gossip::harbour(&b, "https://relay.example.net/", now)] {
            n.keep(&g, &raw(&g), now);
        }
        // A forged note, someone else's signature on another node's claim, is never kept.
        let Gossip::Carried { station, head, at_ms, sig, .. } = Gossip::carried(&a, shown, 4, now) else { unreachable!() };
        let forged = Gossip::Carried { station, node: StationKey::from_seed(b).id().0, head, at_ms, sig };
        n.keep(&forged, &raw(&forged), now);
        let out: Vec<Gossip> = n.for_stations(&[shown], now + 60_000).iter().map(|b| postcard::from_bytes(b).unwrap()).collect();
        let node_a = StationKey::from_seed(a).id().0;
        // The shown station's carrier and that carrier's relay, and nothing about the other.
        assert_eq!(out.len(), 2, "{out:?}");
        assert!(matches!(&out[0], Gossip::Carried { station, node, head: 3, .. } if *station == shown && *node == node_a));
        assert!(matches!(&out[1], Gossip::Harbour { node, .. } if *node == node_a));
        // Past fourteen minutes nothing is handed on.
        assert!(n.for_stations(&[shown], now + 15 * 60_000).is_empty());
    }

    #[test]
    fn a_carrier_note_is_believed_only_from_its_own_node() {
        let secret = [5u8; 32];
        let st = StationId([9; 32]);
        let g = Gossip::carried(&secret, st, 12, 1_000_000);
        let node = StationKey::from_seed(secret).id().0;
        let stranger = [1u8; 32];
        assert!(matches!(g.clone().from_network(1_000_000, &stranger), Some(Gossip::Carrier { node: n, head: 12, .. }) if n == node));
        let Gossip::Carried { station, head, at_ms, sig, .. } = g.clone() else { unreachable!() };
        let forged = Gossip::Carried { station, node: stranger, head, at_ms, sig: sig.clone() };
        assert!(forged.from_network(1_000_000, &stranger).is_none(), "someone else's signature on a claim about another node");
        let raised = Gossip::Carried { station, node, head: head + 1000, at_ms, sig };
        assert!(raised.from_network(1_000_000, &stranger).is_none(), "a head pushed up after signing");
        assert!(g.from_network(1_000_000 + 16 * 60_000, &stranger).is_none(), "an old note");
        // An unsigned note only from the neighbour it names.
        let plain = Gossip::Carrier { station: st, node, head: 3 };
        assert!(plain.clone().from_network(0, &stranger).is_none());
        assert!(plain.from_network(0, &node).is_some());
    }

    #[test]
    fn shares_split_the_log_and_shrink_with_the_fleet() {
        let chunks: Vec<(Hash, u64)> = (0..4000u32).map(|i| (*blake3::hash(&i.to_le_bytes()).as_bytes(), 1000)).collect();
        assert_eq!(share_of(3), 1.0);
        let fleet = 80usize;
        let p = share_of(fleet);
        let nodes: Vec<[u8; 32]> = (0..fleet as u32).map(|i| *blake3::hash(&(i + 9_000_000).to_le_bytes()).as_bytes()).collect();
        let mut copies = std::collections::HashMap::<Hash, usize>::new();
        for n in &nodes {
            let mine = pick_shares(n, &chunks, p, u64::MAX);
            // Each node holds about a tenth of the Log, not all of it.
            assert!(mine.len() > 250 && mine.len() < 550, "{}", mine.len());
            for (c, _) in mine {
                *copies.entry(c).or_default() += 1;
            }
        }
        let mean = copies.values().sum::<usize>() as f64 / chunks.len() as f64;
        assert!((7.0..9.0).contains(&mean), "{mean}");
        // Nearly every chunk is held somewhere (a Poisson eight leaves about 1 in 3000 bare).
        assert!(copies.len() >= chunks.len() - 10);
        // A smaller budget keeps the start of the same list.
        let full = pick_shares(&nodes[0], &chunks, p, u64::MAX);
        let half = pick_shares(&nodes[0], &chunks, p, (full.len() as u64 / 2) * 1000);
        assert_eq!(half[..], full[..half.len()]);
    }


    /// A stranger's bytes, random and mutated from real ones, into every decoder the wire
    /// reaches: none may panic, hang or allocate without bound. A small fuzzer that runs with
    /// the ordinary tests, so every build is checked.
    #[test]
    fn strangers_bytes_never_panic_a_decoder() {
        let mut x = 0x9e37_79b9_7f4a_7c15u64;
        let mut rnd = move || {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x
        };
        let k = StationKey::generate().unwrap();
        let card = Card {
            station: k.id(), head: 3, name: "HOSTIS".into(), about: "a line".into(), port: Some("Tortuga".into()),
            live: false, at_ms: 1, pow: pow_find(&k.id()), since: 1, proof: None, cover: None, accent: None,
            kinds: vec!["talk".into()], mature: false, tip: None, explicit: false, tracks: None, named: None,
        }
        .sign(&k);
        let entry = pyrate_core::SignedEntry::sign(&k, pyrate_core::Entry { station: k.id(), seq: 0, prev: [0; 32], at_ms: 1, body: pyrate_core::Body::Colours { name: "x".into(), about: "y".into(), port: None, tip: None } }).unwrap().to_bytes().unwrap();
        let seeds: Vec<Vec<u8>> = vec![
            postcard::to_stdvec(&Gossip::Station(card.clone())).unwrap(),
            postcard::to_stdvec(&card).unwrap(),
            entry.clone(),
            postcard::to_stdvec(&Request::Log { station: k.id(), from: 0 }).unwrap(),
        ];
        let mut dec = pyrate_core::audio::ChunkDecoder::new(2).unwrap();
        // PYRATE_FUZZ_ROUNDS runs it longer, for a deep pass.
        let rounds: u32 = std::env::var("PYRATE_FUZZ_ROUNDS").ok().and_then(|v| v.parse().ok()).unwrap_or(6_000);
        for i in 0..rounds {
            let mut b: Vec<u8> = if i % 2 == 0 {
                (0..(rnd() % 2048) as usize).map(|_| rnd() as u8).collect()
            } else {
                let mut b = seeds[(rnd() % seeds.len() as u64) as usize].clone();
                for _ in 0..1 + rnd() % 6 {
                    if b.is_empty() {
                        break;
                    }
                    let at = (rnd() % b.len() as u64) as usize;
                    match rnd() % 3 {
                        0 => b[at] = rnd() as u8,
                        1 => b.truncate(at),
                        _ => b.insert(at, 0xff),
                    }
                }
                b
            };
            let _ = postcard::from_bytes::<Gossip>(&b);
            if let Ok(a) = postcard::from_bytes::<Announce>(&b) {
                let _ = a.verify();
            }
            let _ = postcard::from_bytes::<Request>(&b);
            let _ = postcard::from_bytes::<Response>(&b);
            let _ = postcard::from_bytes::<crate::hop::Hop>(&b);
            let _ = postcard::from_bytes::<pyrate_core::chat::Signed>(&b);
            if let Ok(e) = pyrate_core::SignedEntry::from_bytes(&b) {
                let _ = e.verify();
            }
            let _ = pyrate_core::audio::packets(&b);
            let _ = dec.decode_as(&b, 300);
            if i % 50 == 0 {
                let _ = pyrate_core::art::decode(&b);
                b.splice(0..0, [0xff, 0xd8, 0xff, 0xe0]);
                let _ = pyrate_core::art::decode(&b);
            }
        }
    }

    #[test]
    fn a_harbour_note_is_the_node_s_own_word() {
        let secret = [5u8; 32];
        let g = Gossip::harbour(&secret, "https://relay.example/", 1_000_000);
        let (node, relay) = g.harbour_checked(1_000_000).unwrap();
        assert_eq!(node, StationKey::from_seed(secret).id().0);
        assert_eq!(relay, "https://relay.example/");
        assert!(g.harbour_checked(1_000_000 + 16 * 60_000).is_none(), "stale");
        let Gossip::Harbour { node, at_ms, sig, .. } = g else { unreachable!() };
        let forged = Gossip::Harbour { node, relay: "https://elsewhere.example/".into(), at_ms, sig };
        assert!(forged.harbour_checked(1_000_000).is_none(), "another relay under the same signature");
    }

    #[test]
    fn a_false_claim_jumps_no_queue_and_a_flood_grows_nothing() {
        let none = |_: &[u8; 32]| false;
        let mut c = std::collections::HashMap::new();
        note_carrier(&mut c, [1; 32], 10, Some(10), 1_000, none, none, 1);
        note_carrier(&mut c, [2; 32], u64::MAX, Some(10), 2_000, none, none, 1);
        let order = rank_carriers(&c, &[0; 32], Some(10), none, none);
        assert_eq!(order, vec![[2; 32], [1; 32]], "a claim past the card counts as the card, and the fresher word goes first");
        let order = rank_carriers(&c, &[0; 32], Some(10), |n| *n == [2; 32], none);
        assert_eq!(order[0], [1; 32], "a carrier that failed lately goes last");
        let order = rank_carriers(&c, &[0; 32], Some(10), none, |n| *n == [1; 32]);
        assert_eq!(order[0], [1; 32], "one that has served this node goes first");
        // A late flood of a thousand fresh identities: the carriers that served stay.
        let served = |n: &[u8; 32]| *n == [1; 32] || *n == [2; 32];
        for i in 0..1_000u32 {
            let mut n = [9u8; 32];
            n[..4].copy_from_slice(&i.to_le_bytes());
            note_carrier(&mut c, n, 10, Some(10), 3_000 + i as u64, served, none, i as u64);
        }
        assert_eq!(c.len(), MAX_CARRIERS);
        assert!(c.contains_key(&[1; 32]) && c.contains_key(&[2; 32]), "a flood of fresh identities pushes out no carrier that has served");
        // A carrier silent past ten minutes gives way; a trusted one always finds a place.
        let t = 3_000 + CARRIER_STALE_MS + 2_000;
        note_carrier(&mut c, [6; 32], 10, Some(10), t, |n| *n == [6; 32] || served(n), none, 1);
        assert!(c.contains_key(&[6; 32]) && c.len() == MAX_CARRIERS);
    }

    #[test]
    fn claims_that_came_first_cannot_keep_a_real_carrier_out() {
        // NET-001: 64 attacker claims fill the list and are refreshed every time; the real
        // carrier keeps claiming. It gets in within a few claims, whatever the coins.
        let none = |_: &[u8; 32]| false;
        let mut worst = 0;
        for seed in 0..200u64 {
            let mut c = std::collections::HashMap::new();
            let attackers: Vec<[u8; 32]> = (0..64u8).map(|i| [i + 100; 32]).collect();
            for a in &attackers {
                note_carrier(&mut c, *a, 10, Some(10), 0, none, none, 1);
            }
            let mut rng = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let mut claims = 0;
            for t in 1..1_000u64 {
                for a in &attackers {
                    rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                    note_carrier(&mut c, *a, 10, Some(10), t * 1_000, none, none, rng >> 33);
                }
                rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                note_carrier(&mut c, [1; 32], 10, Some(10), t * 1_000, none, none, rng >> 33);
                claims += 1;
                if c.contains_key(&[1; 32]) {
                    break;
                }
            }
            assert!(c.contains_key(&[1; 32]), "seed {seed}: never got in");
            worst = worst.max(claims);
        }
        assert!(worst <= 40, "the real carrier got in within {worst} claims at worst");
        // And one that fails is the first to go.
        let mut c = std::collections::HashMap::new();
        for i in 0..64u8 {
            note_carrier(&mut c, [i + 100; 32], 10, Some(10), 0, none, none, 1);
        }
        note_carrier(&mut c, [1; 32], 10, Some(10), 5, none, |n| *n == [100; 32], 1);
        assert!(c.contains_key(&[1; 32]) && !c.contains_key(&[100; 32]));
    }

    #[test]
    fn sketch_counts_roughly() {
        let mut s = Sketch::default();
        for i in 0u32..1000 {
            s.add(blake3::hash(&i.to_le_bytes()).as_bytes());
        }
        let e = s.estimate();
        assert!((850..1150).contains(&e), "estimate {e}");
        let mut small = Sketch::default();
        for i in 0u32..7 {
            small.add(blake3::hash(&i.to_le_bytes()).as_bytes());
        }
        assert_eq!(small.estimate(), 7);
    }

    #[test]
    fn announce_signs_and_proves() {
        let k = StationKey::generate().unwrap();
        let a = Card {
            station: k.id(), head: 3, name: "HOSTIS".into(), about: "".into(), port: None,
            live: false, at_ms: 1, pow: pow_find(&k.id()), since: 1, proof: None, cover: None, accent: None,
            kinds: vec!["talk".into()], mature: false, tip: None, explicit: false, tracks: None, named: None,
        }
        .sign(&k);
        assert!(a.verify());
        let wire = postcard::to_stdvec(&a).unwrap();
        let back: Announce = postcard::from_bytes(&wire).unwrap();
        assert!(back.verify());
        assert_eq!(back.kinds, vec!["talk".to_string()]);
        let mut forged = back.clone();
        forged.card.head = 4;
        forged.raw = postcard::to_stdvec(&forged.card).unwrap();
        assert!(!forged.verify());
    }

    #[test]
    fn explicit_rides_at_the_end_and_old_cards_read_as_not() {
        let k = StationKey::generate().unwrap();
        let mut card = Card {
            station: k.id(), head: 1, name: "X".into(), about: "".into(), port: None,
            live: false, at_ms: 1, pow: pow_find(&k.id()), since: 1, proof: None, cover: None, accent: None,
            kinds: vec![], mature: true, tip: None, explicit: true, tracks: None, named: None,
        };
        let back: Announce = postcard::from_bytes(&postcard::to_stdvec(&card.clone().sign(&k)).unwrap()).unwrap();
        assert!(back.verify() && back.explicit && back.mature);
        // A card signed before explicit existed: the same bytes, minus the last one.
        card.explicit = false;
        let raw = postcard::to_stdvec(&card).unwrap();
        let mut m = CARD_DOMAIN.to_vec();
        m.extend_from_slice(&raw);
        let wire = postcard::to_stdvec(&AnnounceWire { card: raw, sig: k.sign(&m) }).unwrap();
        let old: Announce = postcard::from_bytes(&wire).unwrap();
        assert!(old.verify() && !old.explicit);
    }

    #[test]
    fn tracks_ride_after_explicit_and_older_cards_read_as_unknown() {
        let k = StationKey::generate().unwrap();
        let card = Card {
            station: k.id(), head: 3, name: "Y".into(), about: "".into(), port: None,
            live: false, at_ms: 1, pow: pow_find(&k.id()), since: 1, proof: None, cover: None, accent: None,
            kinds: vec![], mature: false, tip: None, explicit: true, tracks: Some(42), named: Some(9),
        };
        let back: Announce = postcard::from_bytes(&postcard::to_stdvec(&card.clone().sign(&k)).unwrap()).unwrap();
        assert!(back.verify() && back.explicit && back.tracks == Some(42) && back.named == Some(9));
        // A 0.9.13 card: explicit and nothing after it.
        let older: Announce = postcard::from_bytes(&postcard::to_stdvec(&Card { tracks: None, named: None, ..card }.sign(&k)).unwrap()).unwrap();
        assert!(older.verify() && older.explicit && older.tracks.is_none() && older.named.is_none());
    }

    #[test]
    fn a_card_from_a_later_version_still_verifies() {
        // A later version appends a field to the card. This one reads what it knows.
        #[derive(Serialize)]
        struct Later<'a> {
            card: &'a Card,
            extra: u32,
        }
        let k = StationKey::generate().unwrap();
        let card = Card {
            station: k.id(), head: 1, name: "Later".into(), about: "".into(), port: None,
            live: false, at_ms: 1, pow: pow_find(&k.id()), since: 1, proof: None, cover: None, accent: None,
            kinds: vec![], mature: false, tip: None, explicit: false, tracks: None, named: None,
        };
        let mut raw = postcard::to_stdvec(&card).unwrap();
        raw.extend(postcard::to_stdvec(&Later { card: &card, extra: 9 }.extra).unwrap());
        let mut m = CARD_DOMAIN.to_vec();
        m.extend_from_slice(&raw);
        let wire = postcard::to_stdvec(&AnnounceWire { card: raw, sig: k.sign(&m) }).unwrap();
        let a: Announce = postcard::from_bytes(&wire).unwrap();
        assert!(a.verify());
        assert_eq!(a.name, "Later");
        assert_eq!(postcard::to_stdvec(&a).unwrap(), wire, "passed on byte for byte");
    }
}

#[cfg(test)]
mod card_limit_tests {
    use super::*;

    fn with_tail(tail: usize) -> Result<Announce, postcard::Error> {
        let k = pyrate_core::StationKey::generate().unwrap();
        let card = Card { station: k.id(), head: 1, name: "Tail".into(), about: String::new(), port: None, live: false, at_ms: 1, pow: pow_find(&k.id()), since: 1, proof: None, cover: None, accent: None, kinds: vec![], mature: false, tip: None, explicit: false, tracks: None, named: None };
        let mut raw = postcard::to_stdvec(&card).unwrap();
        raw.extend(postcard::to_stdvec(&false).unwrap());
        // The fields this version knows end with the recordings count and the name's age; the
        // tail comes after them.
        raw.extend(postcard::to_stdvec(&7u32).unwrap());
        raw.extend(postcard::to_stdvec(&5u64).unwrap());
        raw.extend(std::iter::repeat_n(0x5a, tail));
        let mut m = CARD_DOMAIN.to_vec();
        m.extend_from_slice(&raw);
        postcard::from_bytes(&postcard::to_stdvec(&AnnounceWire { card: raw, sig: k.sign(&m) }).unwrap())
    }

    #[test]
    fn signed_bytes_after_a_cards_fields_are_bounded() {
        let ok = with_tail(CARD_EXTRA).expect("a later version's fields, up to the limit, are read");
        assert!(ok.verify());
        assert!(with_tail(CARD_EXTRA + 1).is_err(), "one byte past the limit is refused unread");
        assert!(with_tail(CARD_WIRE).is_err(), "so is a card past its whole size");
    }

    #[test]
    fn long_cards_are_clipped_by_their_station_and_refused_from_anyone_else() {
        let k = pyrate_core::StationKey::generate().unwrap();
        let card = |name: String| Card { station: k.id(), head: 0, name, about: "a".repeat(5_000), port: Some("p".repeat(500)), live: false, at_ms: 1, pow: pow_find(&k.id()), since: 1, proof: None, cover: None, accent: None, kinds: (0..30).map(|i| format!("k{i}")).collect(), mature: false, tip: Some("t".repeat(300)), explicit: false, tracks: None, named: None };
        assert!(!card("N".repeat(500)).sign(&k).verify());
        let ok = card("Ñ".repeat(500)).clipped().sign(&k);
        assert!(ok.verify());
        assert_eq!(ok.name.chars().count(), CARD_NAME);
        assert!(ok.tip.is_none() && ok.kinds.len() == CARD_KINDS);
    }

    #[test]
    fn a_deputys_card_is_trusted_only_against_a_current_log() {
        let master = pyrate_core::StationKey::generate().unwrap();
        let deputy = pyrate_core::StationKey::generate().unwrap();
        let mut arch = pyrate_core::Archive::new(master.id());
        let proof = arch.write(&master, 1_000, pyrate_core::Body::Deputy { key: deputy.id().0, label: "x".into() }).unwrap().to_bytes().unwrap();
        let c = Card { station: master.id(), head: arch.len(), name: "S".into(), about: String::new(), port: None, live: false, at_ms: 1, pow: pow_find(&master.id()), since: 1, proof: Some(proof), cover: None, accent: None, kinds: vec![], mature: false, tip: Some("x".into()), explicit: false, tracks: None, named: None }.sign(&deputy);
        assert!(c.verify());
        assert!(!deputy_current(&c, None));
        assert!(deputy_current(&c, Some(&arch)));
        arch.write(&master, 2_000, pyrate_core::Body::Revoke { key: deputy.id().0 }).unwrap();
        assert!(!deputy_current(&c, Some(&arch)));
    }
}
