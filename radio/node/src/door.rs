//! Doors: a listener's app as a way in for browsers, with no relay and no TLS.
//!
//! A page cannot take connections, so a browser reaches the network only through something
//! that can. A door is a listener's app (on a computer, or a phone on Wi-Fi while charging)
//! that takes a browser's WebRTC data channel. There are two ways in.
//!
//! A listed door (a Lantern or a server, kept as a door by its keeper, or a listener's app whose
//! mapped address is a VPN's and never home's) gossips a signed note with its address and
//! certificate fingerprint, and tabs dial it WebRTC-direct style: the browser writes the door's
//! half of the SDP itself, and no signalling server is asked anything. The ICE username and
//! password are the same random string, which the browser chooses and the door reads off the
//! first STUN request (as in libp2p's webrtc-direct).
//!
//! An introduced door (every other listener's app) gossips only that it is there, with no
//! address (`Gossip::Presence`). A tab already on the network asks a middle for a way in through
//! its own route; the middle draws a door and passes it the tab's offer sealed, and the door
//! answers the same way with its own ICE name and password, its fingerprint and the addresses
//! the network already sees it at. The door runs full ICE from its side, so it punches through
//! its NAT with no router mapping, and at most twelve tabs an hour are introduced to it.
//!
//! The DTLS certificate is the door's own. A kept door keeps it from one run to the next while
//! this node's id and its address on the network stay the same, so its fingerprint in the note
//! stays true; a new id or a new address gives a new certificate, port and first day, so no door
//! outlives a network change. A listener's app door takes a new one each spell of door duty.
//!
//! A door names only the address the network already sees this device at. A port the router
//! maps is checked against the address the relays see this device come from; where the two
//! differ (a VPN, usually) or the network has not said, the port is let go.
//!
//! Over the channel run the same requests and answers a tab sends over iroh. A door answers a
//! hop as a first hop does (the tab's guard, or the relay of its hops), hands over the cards it
//! knows, passes on the gossip it hears, and carries forwards from other radios to the tabs
//! behind it, as a relay would. It answers nothing else: what a tab asks of the network goes
//! through routes the door cannot read. The door sees the tab's address, as a guard does.
//!
//! The same socket answers plain STUN binding requests, so tabs can learn their own address
//! from the network's doors instead of anyone else's server.
//!
//! No rights reserved.

use std::{
    collections::{HashMap, HashSet, VecDeque},
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::{Arc, Mutex, Weak, atomic::{AtomicBool, AtomicU64, Ordering}},
    time::{Duration, Instant},
};

use iroh::EndpointId;
use str0m::{
    Candidate, Event, IceConnectionState, IceCreds, Input, Output, Rtc, RtcConfig,
    channel::{ChannelConfig, ChannelId},
    config::DtlsCert,
    net::{Protocol, Receive},
};
use tokio::{net::UdpSocket, sync::{mpsc, oneshot}};

use crate::{Inner, net::{DoorCtl, Gossip, Request, Response}, now_ms, say};
use pyrate_core::doorway::{Answer, Offer, PerRadio, Window};

/// Browsers at once on one door: a computer's share, and a phone's.
pub(crate) const MAX_SESSIONS: usize = 64;
/// Introduced tabs whose links have not opened yet, at once.
const INTRO_PENDING: usize = 16;
/// Addresses one introduced tab's checks may come from or go to.
const INTRO_ADDRS: usize = 12;
/// A data channel message: the same parts the tabs' own links use, each with its five-byte head
/// inside the 16 KiB the channel's SDP allows.
const PART: usize = 16 * 1024 - 5;
/// The most one request or answer may be.
const MESSAGE_MAX: usize = 4 << 20;
/// Requests from one browser waiting on an answer at once.
const ASKS_PER_TAB: usize = 8;
/// Frame ids the door starts carry this bit; the tab's own start at one.
const FROM_DOOR: u32 = 0x8000_0000;
/// What may wait to go out to one browser before the door lets it go.
const OUT_MAX: usize = 4 << 20;
/// Browsers at once from one address.
const SESSIONS_PER_IP: usize = 4;
/// A browser's cards, at most once in this long.
const CARDS_EVERY: Duration = Duration::from_secs(10);
/// Plain STUN answers to one address in ten seconds.
const STUN_PER_IP: u32 = 5;

/// Why a door is shut, in words the app can show.
pub const WHY_OFF: &str = "The door is switched off.";
pub const WHY_DARK: &str = "This radio runs through Tor, so it is never a door.";
pub const WHY_ASKING: &str = "The router is being asked for a port.";
pub const WHY_NO_PORT: &str = "The router would not open a port, so no door.";
pub const WHY_NOT_SEEN: &str = "The router's address is not the one the network sees, so no door.";
pub const WHY_MOVED: &str = "This device's address on the network changed, so the door closed.";
pub const WHY_LOST: &str = "The router stopped holding the port, so the door closed.";
pub const WHY_NO_SOCKET: &str = "The door's port could not be opened on this device.";
pub const WHY_IDLE: &str = "Enough other doors are open and quiet, so this one is shut for now.";
pub const WHY_REST: &str = "This door was open six hours, so it is resting before it opens again.";
pub const WHY_WAITING: &str = "Enough doors are open, so this one opens only when they get busy.";

/// The frame id when `f` is the last part of a message, the part whose going frees its slot.
fn last_part(f: &[u8]) -> Option<u32> {
    (f.len() >= 5 && f[4] == 1).then(|| u32::from_be_bytes([f[0], f[1], f[2], f[3]]))
}

/// Whether a plain STUN request from `ip` is answered now: five in any ten seconds at most, so
/// the port is no one's reflector. `seen` remembers who asked lately.
fn stun_ok(seen: &mut HashMap<IpAddr, (Instant, u32)>, ip: IpAddr, now: Instant) -> bool {
    if seen.len() > 4096 {
        seen.retain(|_, (t, _)| now.duration_since(*t) < Duration::from_secs(10));
    }
    let e = seen.entry(ip).or_insert((now, 0));
    if now.duration_since(e.0) >= Duration::from_secs(10) {
        *e = (now, 0);
    }
    if e.1 >= STUN_PER_IP {
        return false;
    }
    e.1 += 1;
    true
}

/// Whether a new browser is let in: room on the door, room for one more from its address, and
/// not too many new ones this second.
fn admit(total: usize, same_ip: usize, this_second: usize) -> bool {
    total < MAX_SESSIONS && same_ip < SESSIONS_PER_IP && this_second < 8
}

/// Whether a tab's request is answered now: only once the tab has said who it is, with a slot
/// free, an id not already waiting, and the cards at most once in ten seconds.
fn may_ask(said_hello: bool, asks: usize, waiting: bool, cards: bool, cards_at: Option<Instant>, now: Instant) -> bool {
    said_hello && asks < ASKS_PER_TAB && !waiting && !(cards && cards_at.is_some_and(|t| now.duration_since(t) < CARDS_EVERY))
}

/// A mapped port is a door only at the address the network already sees this device at. An
/// address the network has not given is never taken as the same.
pub fn address_rule(mapped: IpAddr, observed: Option<Ipv4Addr>) -> Result<(), &'static str> {
    match observed {
        Some(o) if mapped == IpAddr::V4(o) => Ok(()),
        _ => Err(WHY_NOT_SEEN),
    }
}

/// A listener's app door is listed only in the VPN case: the router that mapped its port is
/// reached through a tunnel, and the address it gave is none this device holds as home's.
pub fn listable(tunnel: bool, outside: IpAddr, home: &[IpAddr]) -> bool {
    tunnel && !home.contains(&outside)
}

/// The addresses an introduced door gives a tab to try: the one the keeper gave, the router's
/// mapped port, and the address the network sees with the door's own port (most routers keep
/// it). On a test network with none of these, loopback. Never an address the network has not
/// shown this device at.
pub fn app_candidates(given: Option<SocketAddr>, mapped: Option<SocketAddr>, seen: Option<Ipv4Addr>, port: u16, test: bool) -> Vec<SocketAddr> {
    let mut v = Vec::new();
    for a in [given, mapped, seen.map(|o| SocketAddr::new(IpAddr::V4(o), port))].into_iter().flatten() {
        if !v.contains(&a) {
            v.push(a);
        }
    }
    if v.is_empty() && test {
        v.push(SocketAddr::from(([127, 0, 0, 1], port)));
    }
    v
}

enum Cmd {
    /// An answer (or a push, under id 0) to the browser on session `to`.
    Send { to: u64, id: u32, bytes: Vec<u8> },
    /// A tab introduced by a middle: its offer, and where the door's half goes.
    Intro { offer: Offer, reply: oneshot::Sender<Option<Answer>> },
    /// A request to the tab `tab`, from another radio's forward.
    Ask { tab: [u8; 32], bytes: Vec<u8>, reply: oneshot::Sender<Vec<u8>> },
    /// Gossip to every tab behind this door.
    Push(Vec<u8>),
    Close,
}

struct Session {
    rtc: Rtc,
    /// The address the browser is at, once known: given for a WebRTC-direct tab, learnt from
    /// its first check for an introduced one.
    at: Option<SocketAddr>,
    /// The address its packets are given as arriving at: its one local candidate.
    local: SocketAddr,
    /// An introduced tab's link: the door's own ICE name for it, and the addresses its checks
    /// have come from or gone to.
    ufrag: Option<String>,
    addrs: Vec<SocketAddr>,
    ch: Option<ChannelId>,
    peer: Option<[u8; 32]>,
    parts: HashMap<u32, Vec<u8>>,
    out: VecDeque<Vec<u8>>,
    waiting: HashMap<u32, oneshot::Sender<Vec<u8>>>,
    next: u32,
    /// Requests whose answers have not yet gone: a slot is free only once its answer has.
    asks: usize,
    counted: HashSet<u32>,
    /// Bytes waiting in `out`.
    out_bytes: usize,
    cards_at: Option<Instant>,
    born: Instant,
    wake: Instant,
}

impl Session {
    /// Queues frames for the browser. False, and the session is let go, once more would wait
    /// than the door holds for one browser.
    fn queue(&mut self, frames: Vec<Vec<u8>>) -> bool {
        let n: usize = frames.iter().map(|f| f.len()).sum();
        if self.out_bytes + n > OUT_MAX {
            self.rtc.disconnect();
            return false;
        }
        self.out_bytes += n;
        self.out.extend(frames);
        true
    }

    /// Writes what waits as far as the channel takes it. An answer's slot frees as its last
    /// part goes.
    fn flush(&mut self) {
        let Some(ch) = self.ch else { return };
        while let Some(f) = self.out.front() {
            let Some(mut c) = self.rtc.channel(ch) else { break };
            if !matches!(c.write(true, f), Ok(true)) {
                break;
            }
            let f = self.out.pop_front().unwrap_or_default();
            self.out_bytes = self.out_bytes.saturating_sub(f.len());
            if let Some(id) = last_part(&f) {
                if self.counted.remove(&id) {
                    self.asks = self.asks.saturating_sub(1);
                }
            }
        }
    }
}

pub struct Door {
    pub fp: [u8; 32],
    pub port: u16,
    pub since: u64,
    /// Where browsers reach it from a note: a mapped port on the home router, or an address the
    /// keeper gave. None for an introduced door, whose address is in no note.
    pub public: Mutex<Option<SocketAddr>>,
    pub app: bool,
    /// Whether it takes introductions, and the addresses it gives an introduced tab to try:
    /// only those the network already sees this device at (or the keeper gave).
    pub intro: bool,
    pub cands: Vec<SocketAddr>,
    /// Whether the router gave it a port.
    pub map: bool,
    /// Introductions taken this hour, in all and from each middle.
    intros: Mutex<Window>,
    intros_by: Mutex<PerRadio>,
    pub introduced: AtomicU64,
    /// Its owner lets shared links name it.
    pub link: AtomicBool,
    cmd: mpsc::UnboundedSender<Cmd>,
    /// The tabs behind this door, by node id, and the session each is on.
    clients: Mutex<HashMap<[u8; 32], u64>>,
    pub sessions: AtomicU64,
    pub opened: AtomicU64,
    pub served: AtomicU64,
    pub stun: AtomicU64,
    closed: AtomicBool,
    /// The router's port, given back when the door closes. None where the address was given.
    mapping: Mutex<Option<crate::portmap::Mapping>>,
}

impl Door {
    /// The tabs behind this door, by node id.
    pub fn has_client(&self, tab: &[u8; 32]) -> bool {
        self.clients.lock().unwrap().contains_key(tab)
    }

    pub fn clients(&self) -> usize {
        self.clients.lock().unwrap().len()
    }

    /// A request to a tab behind this door, and its answer (empty if none within eight seconds).
    pub async fn ask(&self, tab: [u8; 32], req: &Request) -> Vec<u8> {
        let Ok(bytes) = postcard::to_stdvec(req) else { return vec![] };
        let (tx, rx) = oneshot::channel();
        if self.cmd.send(Cmd::Ask { tab, bytes, reply: tx }).is_err() {
            return vec![];
        }
        tokio::time::timeout(Duration::from_secs(8), rx).await.ok().and_then(|r| r.ok()).unwrap_or_default()
    }

    pub fn push(&self, gossip: &[u8]) {
        if gossip.len() <= 64 * 1024 && self.clients() > 0 {
            let _ = self.cmd.send(Cmd::Push(gossip.to_vec()));
        }
    }

    pub fn close(&self) {
        self.closed.store(true, Ordering::Relaxed);
        let _ = self.cmd.send(Cmd::Close);
    }

    /// This door's signed note, if it has an address to give.
    pub fn note(&self, secret: &[u8; 32], at_ms: u64) -> Option<Gossip> {
        let a = (*self.public.lock().unwrap())?;
        Some(Gossip::door(secret, &a.to_string(), self.fp, self.since, self.app, self.link.load(Ordering::Relaxed), at_ms))
    }

    /// Whether its address is in a note: a listed door.
    pub fn listed(&self) -> bool {
        self.public.lock().unwrap().is_some()
    }

    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Relaxed)
    }

    /// Whether one more introduction is taken now, from the middle `from`: twelve an hour in
    /// all, four from any one middle.
    fn intro_ok(&self, from: &[u8; 32], now: u64) -> bool {
        // Counted against the middle first, so one middle's flood spends only its own four.
        self.intro && self.intros_by.lock().unwrap().take(from, now) && self.intros.lock().unwrap().take(now)
    }
}

/// The door's certificate, port and first day, kept in the node's sealed notes from one run to
/// the next while this node's id and the address the network sees it at stay the same. Either
/// changing gives fresh ones.
fn identity(inner: &Inner, ip: IpAddr) -> Option<(DtlsCert, u16, u64)> {
    let me = *inner.endpoint.id().as_bytes();
    if let Some(kept) = inner.hold.read_note(&inner.cfg.dir, "door", None).and_then(|b| kept_identity(&b, &me, ip)) {
        return Some(kept);
    }
    let p = str0m::crypto::from_feature_flags();
    let cert = p.dtls_provider.generate_certificate()?;
    let port = 20_000 + (rand::random::<u16>() % 40_000);
    let since = now_ms();
    let _ = inner.hold.write_note(&inner.cfg.dir, "door", &identity_note(&cert, port, since, &me, ip)?);
    Some((cert, port, since))
}

fn identity_note(cert: &DtlsCert, port: u16, since: u64, me: &[u8; 32], ip: IpAddr) -> Option<Vec<u8>> {
    postcard::to_stdvec(&(&cert.certificate, &cert.private_key, port, since, me, ip.to_string())).ok()
}

/// A kept identity, if it was made for this node id at this address. One from before ids and
/// addresses were kept beside it is never taken.
fn kept_identity(b: &[u8], me: &[u8; 32], ip: IpAddr) -> Option<(DtlsCert, u16, u64)> {
    let (certificate, private_key, port, since, id, at) = postcard::from_bytes::<(Vec<u8>, Vec<u8>, u16, u64, [u8; 32], String)>(b).ok()?;
    (id == *me && at == ip.to_string()).then_some((DtlsCert { certificate, private_key }, port, since))
}

fn sha256(b: &[u8]) -> [u8; 32] {
    use sha2::Digest;
    sha2::Sha256::digest(b).into()
}

/// A plain STUN binding request (no username): answered with the address it came from.
fn stun_answer(pkt: &[u8], from: SocketAddr) -> Option<Vec<u8>> {
    if pkt.len() < 20 || pkt[0] != 0 || pkt[1] != 1 || pkt[4..8] != [0x21, 0x12, 0xA4, 0x42] {
        return None;
    }
    let len = u16::from_be_bytes([pkt[2], pkt[3]]) as usize;
    if pkt.len() < 20 + len {
        return None;
    }
    // Any USERNAME means an ICE check, which is a session's business.
    let mut at = 20;
    while at + 4 <= 20 + len {
        let t = u16::from_be_bytes([pkt[at], pkt[at + 1]]);
        let l = u16::from_be_bytes([pkt[at + 2], pkt[at + 3]]) as usize;
        if t == 0x0006 {
            return None;
        }
        at += 4 + l.div_ceil(4) * 4;
    }
    let mut attr = Vec::new();
    let port = from.port() ^ 0x2112;
    match from.ip() {
        IpAddr::V4(v) => {
            attr.extend_from_slice(&[0, 0x01]);
            attr.extend_from_slice(&port.to_be_bytes());
            let x = u32::from_be_bytes(v.octets()) ^ 0x2112_A442;
            attr.extend_from_slice(&x.to_be_bytes());
        }
        IpAddr::V6(v) => {
            attr.extend_from_slice(&[0, 0x02]);
            attr.extend_from_slice(&port.to_be_bytes());
            let mut key = [0u8; 16];
            key[..4].copy_from_slice(&[0x21, 0x12, 0xA4, 0x42]);
            key[4..].copy_from_slice(&pkt[8..20]);
            for (o, k) in v.octets().iter().zip(key) {
                attr.push(o ^ k);
            }
        }
    }
    let mut out = vec![0x01, 0x01];
    out.extend_from_slice(&((4 + attr.len()) as u16).to_be_bytes());
    out.extend_from_slice(&pkt[4..20]);
    out.extend_from_slice(&0x0020u16.to_be_bytes());
    out.extend_from_slice(&(attr.len() as u16).to_be_bytes());
    out.extend_from_slice(&attr);
    Some(out)
}

/// A WebRTC-direct browser's first check: the ICE name it gave this door (also the door's
/// password), and its own name. The browser's own half of the session goes unchanged.
fn direct_ufrag(pkt: &[u8]) -> Option<(String, String)> {
    let m = str0m::ice::StunMessage::parse(pkt).ok()?;
    if !m.is_binding_request() {
        return None;
    }
    let (a, b) = m.split_username()?;
    let fine = |x: &str| x.len() >= 4 && x.len() <= 256 && x.bytes().all(|c| c.is_ascii_graphic());
    (a.starts_with("pyrate+door+v1/") && a.len() >= 22 && fine(a) && fine(b)).then(|| (a.to_string(), b.to_string()))
}

fn frame(id: u32, bytes: &[u8]) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut at = 0;
    loop {
        let end = (at + PART).min(bytes.len());
        let mut f = Vec::with_capacity(5 + end - at);
        f.extend_from_slice(&id.to_be_bytes());
        f.push((end == bytes.len()) as u8);
        f.extend_from_slice(&bytes[at..end]);
        out.push(f);
        if end == bytes.len() {
            break;
        }
        at = end;
    }
    out
}

impl Inner {
    /// Opens this node's door. `public` is where browsers reach it, where the keeper or the test
    /// network gives it; otherwise the home router is asked for a port (see `portmap`), and the
    /// door opens only if the router's outside address is the one the network sees this device
    /// at. With no address to give, the door stays shut and says why: a door nobody can reach
    /// is announced nowhere.
    pub async fn door_open(self: &Arc<Self>, public: Option<SocketAddr>, app: bool, link: bool) -> anyhow::Result<Arc<Door>> {
        // A radio running dark (through Tor) is never a door: a door's note carries the address
        // the router gives it, and that is the address Tor is there to hide. Nothing is bound,
        // no router is asked, and nothing is announced.
        if self.dark {
            self.door_why(WHY_DARK);
            anyhow::bail!("running dark, so this radio is not a door");
        }
        if let Some(d) = self.door.lock().unwrap().clone() {
            return Ok(d);
        }
        if self.door_opening.swap(true, Ordering::SeqCst) {
            anyhow::bail!("already opening");
        }
        let r = self.door_open_once(public, app, link).await;
        self.door_opening.store(false, Ordering::SeqCst);
        if let Err(e) = &r {
            say(&format!("door: {e}"));
        }
        r
    }

    async fn door_open_once(self: &Arc<Self>, public: Option<SocketAddr>, app: bool, link: bool) -> anyhow::Result<Arc<Door>> {
        if app {
            return self.door_open_app(public, link).await;
        }
        let asked = self.door_asked.load(Ordering::SeqCst);
        // Where the network sees this device, before any router is asked.
        let seen = match public {
            Some(p) => p.ip(),
            None => match self.observed_v4() {
                Some(o) => IpAddr::V4(o),
                None => {
                    self.door_why(WHY_NOT_SEEN);
                    anyhow::bail!("the network has not said where it sees this device");
                }
            },
        };
        // A router that gave the wrong address is not asked again on the same network for a while.
        let here = (crate::portmap::gateway(), seen);
        let refused = *self.door_refused.lock().unwrap();
        if public.is_none() && refused.is_some_and(|(at, w)| w == here && now_ms().saturating_sub(at) < 30 * 60_000) {
            self.door_why(WHY_NOT_SEEN);
            anyhow::bail!("refused on this network lately");
        }
        let (cert, kept_port, since) = identity(self, seen).ok_or_else(|| anyhow::anyhow!("no certificate"))?;
        let port = std::env::var("PYRATE_DOOR_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(kept_port);
        let Ok(sock) = UdpSocket::bind(SocketAddr::from(([0, 0, 0, 0], port))).await else {
            self.door_why(WHY_NO_SOCKET);
            anyhow::bail!("the door's port is taken");
        };
        let port = sock.local_addr()?.port();
        let (public, mapping) = match public {
            Some(p) => (p, None),
            None => {
                self.door_why(WHY_ASKING);
                let Some(m) = crate::portmap::map_udp(port).await else {
                    self.door_why(WHY_NO_PORT);
                    anyhow::bail!("no port could be mapped on the router, so this radio is not a door");
                };
                // Switched off while the router was asked: the port goes back, and no door.
                if self.door_asked.load(Ordering::SeqCst) != asked || self.dark {
                    m.close().await;
                    anyhow::bail!("switched off while the router was asked");
                }
                let rule = address_rule(m.outside.ip(), self.observed_v4()).and_then(|_| if m.outside.ip() == seen { Ok(()) } else { Err(WHY_MOVED) });
                if let Err(why) = rule {
                    m.close().await;
                    *self.door_refused.lock().unwrap() = Some((now_ms(), here));
                    self.door_why(why);
                    anyhow::bail!("the router's address {} is not where the network sees this device", m.outside.ip());
                }
                (m.outside, Some(m))
            }
        };
        let watch = mapping.is_some().then_some(seen);
        Ok(self.door_start(cert, port, since, sock, Some(public), false, link, vec![public], mapping, watch))
    }

    /// A listener's app door. It opens on the address the keeper gave (a test network's), or
    /// asks the router for a port, or takes none: an introduced door punches through its NAT
    /// itself. It is listed (a note with its address) only where the router that mapped its port
    /// is a VPN's, reached through a tunnel, and the address is none this device holds as home's:
    /// otherwise its address is in no note, and only tabs a middle introduces learn it. The
    /// addresses it gives them are those the network already sees this device at.
    async fn door_open_app(self: &Arc<Self>, public: Option<SocketAddr>, link: bool) -> anyhow::Result<Arc<Door>> {
        let asked = self.door_asked.load(Ordering::SeqCst);
        let seen = self.observed_v4();
        let test = crate::net::LOCAL_RELAYS_OK.load(Ordering::Relaxed);
        let ip = public.map(|p| p.ip()).or(seen.map(IpAddr::V4)).unwrap_or(IpAddr::V4(Ipv4Addr::UNSPECIFIED));
        let (cert, kept_port, since) = identity(self, ip).ok_or_else(|| anyhow::anyhow!("no certificate"))?;
        let port = std::env::var("PYRATE_DOOR_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(kept_port);
        let Ok(sock) = UdpSocket::bind(SocketAddr::from(([0, 0, 0, 0], port))).await else {
            self.door_why(WHY_NO_SOCKET);
            anyhow::bail!("the door's port is taken");
        };
        let port = sock.local_addr()?.port();
        let (mut mapping, mut listed) = (None, None);
        if public.is_none() && !test {
            self.door_why(WHY_ASKING);
            if let Some(m) = crate::portmap::map_udp(port).await {
                if self.door_asked.load(Ordering::SeqCst) != asked || self.dark {
                    m.close().await;
                    anyhow::bail!("switched off while the router was asked");
                }
                match address_rule(m.outside.ip(), self.observed_v4()) {
                    Ok(()) => {
                        if listable(crate::portmap::via_tunnel(), m.outside.ip(), &crate::portmap::home_addresses()) {
                            listed = Some(m.outside);
                        }
                        mapping = Some(m);
                    }
                    // The router's address is not the one the network sees: the port goes back.
                    Err(_) => m.close().await,
                }
            }
        }
        if self.door_asked.load(Ordering::SeqCst) != asked || self.dark {
            if let Some(m) = mapping {
                m.close().await;
            }
            anyhow::bail!("switched off while the door was opening");
        }
        let cands = app_candidates(public, mapping.as_ref().map(|m| m.outside), seen, port, test);
        if cands.is_empty() {
            if let Some(m) = mapping {
                m.close().await;
            }
            self.door_why(WHY_NOT_SEEN);
            anyhow::bail!("the network has not said where it sees this device");
        }
        Ok(self.door_start(cert, port, since, sock, listed, true, link, cands, mapping, seen.map(IpAddr::V4)))
    }

    /// The door, open: its loop, its notes, and the watch on its address.
    #[allow(clippy::too_many_arguments)]
    fn door_start(
        self: &Arc<Self>, cert: DtlsCert, port: u16, since: u64, sock: UdpSocket, public: Option<SocketAddr>, app: bool, link: bool, cands: Vec<SocketAddr>,
        mapping: Option<crate::portmap::Mapping>, watch: Option<IpAddr>,
    ) -> Arc<Door> {
        let map = mapping.is_some();
        let (tx, rx) = mpsc::unbounded_channel();
        let door = Arc::new(Door {
            fp: sha256(&cert.certificate),
            port,
            since,
            public: Mutex::new(public),
            app,
            intro: true,
            cands: cands.clone(),
            map,
            intros: Mutex::new(Window::new(3_600_000, pyrate_core::doorway::DOOR_INTROS_HOUR)),
            intros_by: Mutex::new(PerRadio::new(3_600_000, pyrate_core::doorway::DOOR_INTROS_MIDDLE, 256)),
            introduced: AtomicU64::new(0),
            link: AtomicBool::new(link),
            cmd: tx,
            clients: Mutex::new(HashMap::new()),
            sessions: AtomicU64::new(0),
            opened: AtomicU64::new(0),
            served: AtomicU64::new(0),
            stun: AtomicU64::new(0),
            closed: AtomicBool::new(false),
            mapping: Mutex::new(mapping),
        });
        *self.door.lock().unwrap() = Some(door.clone());
        *self.door_why.lock().unwrap() = None;
        match public {
            Some(p) => say(&format!("door open at {p} (udp {port})")),
            None => say(&format!("door open for introduced tabs only (udp {port}, {} addresses to give)", cands.len())),
        }
        let (w, d2) = (Arc::downgrade(self), door.clone());
        tokio::spawn(async move { run(w, d2, sock, cert, cands, rx).await });
        // Its note where it is listed, and its presence, now and every two minutes while it is open.
        let (i2, d3) = (self.clone(), door.clone());
        tokio::spawn(async move {
            // First once the Log has neighbours to hear it, then every two minutes.
            tokio::time::sleep(Duration::from_secs(8)).await;
            while !d3.closed.load(Ordering::Relaxed) {
                for g in [d3.note(&i2.secret, now_ms()), i2.presence_note()].into_iter().flatten() {
                    if let Ok(raw) = postcard::to_stdvec(&g) {
                        i2.notes.lock().unwrap().keep(&g, &raw, now_ms());
                    }
                    i2.broadcast(&g).await;
                }
                let n = d3.served.load(Ordering::Relaxed) + d3.introduced.load(Ordering::Relaxed);
                tokio::time::sleep(Duration::from_secs(if n == 0 && now_ms().saturating_sub(d3.since) < 120_000 { 20 } else { 120 })).await;
            }
        });
        // A door lasts only while the network sees this device where it did, and a mapped one
        // while the router holds the port. Either changing closes it; the next open draws a new
        // identity.
        if let Some(seen) = watch {
            let (w, d4) = (Arc::downgrade(self), door.clone());
            tokio::spawn(async move {
                loop {
                    tokio::time::sleep(Duration::from_secs(15)).await;
                    let Some(inner) = w.upgrade() else { return };
                    if d4.closed.load(Ordering::Relaxed) {
                        return;
                    }
                    let lost = d4.mapping.lock().unwrap().as_ref().is_some_and(|m| m.lost());
                    let moved = inner.observed_v4().is_some_and(|o| IpAddr::V4(o) != seen);
                    if lost || moved {
                        if d4.app {
                            inner.hold.remove_note(&inner.cfg.dir, "door");
                        }
                        inner.door_shut(&d4, if moved { WHY_MOVED } else { WHY_LOST }).await;
                        return;
                    }
                }
            });
        }
        door
    }

    /// An introduction, from the middle `from`: the tab's offer sealed to this node. Answered with
    /// this door's half sealed back, or nothing when the door is shut, has taken its share this
    /// hour, or the offer does not open.
    pub(crate) async fn on_introduce(self: &Arc<Self>, from: &[u8; 32], blob: &[u8]) -> Vec<u8> {
        let Some(door) = self.door.lock().unwrap().clone() else { return vec![] };
        if self.dark || !self.crew.load(Ordering::Relaxed) || door.is_closed() {
            return vec![];
        }
        let Ok(crate::hop::Hop::Sealed { eph, ct }) = postcard::from_bytes::<crate::hop::Hop>(blob) else { return vec![] };
        let me = *self.endpoint.id().as_bytes();
        let Some((plain, c)) = pyrate_core::route::open_sealed(&self.secret, &me, &eph, &ct) else { return vec![] };
        let Ok(offer) = postcard::from_bytes::<Offer>(&plain) else { return vec![] };
        if !offer.sound() || !door.intro_ok(from, now_ms()) {
            debug(&format!("introduction from {} refused", crate::hex(&from[..4])));
            return vec![];
        }
        let (tx, rx) = oneshot::channel();
        if door.cmd.send(Cmd::Intro { offer, reply: tx }).is_err() {
            return vec![];
        }
        let Ok(Ok(Some(answer))) = tokio::time::timeout(Duration::from_secs(3), rx).await else { return vec![] };
        door.introduced.fetch_add(1, Ordering::Relaxed);
        say(&format!("door: a tab introduced by {}", crate::hex(&from[..4])));
        pyrate_core::route::seal_answer(&c, &postcard::to_stdvec(&answer).unwrap_or_default())
    }

    /// The public IPv4 address the relays see this device come from, if the network has said.
    pub(crate) fn observed_v4(&self) -> Option<Ipv4Addr> {
        use iroh::Watcher;
        let r = self.endpoint.net_report().get()?;
        r.global_v4.map(|a| *a.ip())
    }

    pub(crate) fn door_why(&self, why: &str) {
        *self.door_why.lock().unwrap() = Some(why.to_string());
    }

    /// Closes the door at the owner's word. No new browsers come in, and the router's port goes back.
    pub fn door_close(&self) {
        self.door_asked.fetch_add(1, Ordering::SeqCst);
        self.door_why(WHY_OFF);
        if let Some(d) = self.door.lock().unwrap().take() {
            d.close();
            let m = d.mapping.lock().unwrap().take();
            if let Some(m) = m {
                self.handle.spawn(async move { m.close().await });
            }
            say("door closed");
        }
    }

    /// Closes the door and waits for the router's port to be given back: for shutting down.
    pub(crate) async fn door_close_now(&self) {
        self.door_asked.fetch_add(1, Ordering::SeqCst);
        self.door_why(WHY_OFF);
        let d = self.door.lock().unwrap().take();
        if let Some(d) = d {
            d.close();
            let m = d.mapping.lock().unwrap().take();
            if let Some(m) = m {
                m.close().await;
            }
        }
    }

    /// Closes this door, if it is still the open one, saying why.
    async fn door_shut(&self, door: &Arc<Door>, why: &str) {
        let mine = {
            let mut d = self.door.lock().unwrap();
            if d.as_ref().is_some_and(|o| Arc::ptr_eq(o, door)) { d.take() } else { None }
        };
        if let Some(d) = mine {
            d.close();
            let m = d.mapping.lock().unwrap().take();
            if let Some(m) = m {
                m.close().await;
            }
            self.door_why(why);
            say(&format!("door closed: {why}"));
        }
    }

    /// A request from the tab `from` on this door: a hop as its first hop (or the relay of its
    /// hops), or the cards.
    async fn door_request(self: &Arc<Self>, bytes: Vec<u8>, from: [u8; 32]) -> Vec<u8> {
        let resp = match postcard::from_bytes::<Request>(&bytes) {
            Ok(Request::Hop(b)) => {
                let Ok(_hop) = (if self.cfg.lantern { &crate::HOPPING_LANTERN } else { &crate::HOPPING }).try_acquire() else { return vec![] };
                Response::Hop(self.on_hop(&b, &from).await)
            }
            Ok(Request::Cards) => {
                let mut c = self.cards_answer().as_ref().clone();
                c.extend(self.notes.lock().unwrap().doors(now_ms(), 16));
                Response::Cards(c)
            }
            _ => Response::Hop(vec![]),
        };
        postcard::to_stdvec(&resp).unwrap_or_default()
    }

    /// A tab's control frame: its hello, or its own crew note to pass on.
    fn door_ctl(self: &Arc<Self>, door: &Door, id: u64, sess: &mut Session, bytes: &[u8]) {
        let Ok(ctl) = postcard::from_bytes::<DoorCtl>(bytes) else { return };
        let me = *self.endpoint.id().as_bytes();
        let from = sess.at.map(|a| a.to_string()).unwrap_or_default();
        match ctl {
            DoorCtl::Hello { node, at_ms, sig } => {
                let Ok(sig) = <[u8; 64]>::try_from(sig.as_slice()) else { return };
                if at_ms.abs_diff(now_ms()) > 5 * 60_000 || node == me || sess.peer.is_some() {
                    return;
                }
                if pyrate_core::StationId(node).verify(&crate::net::door_hello_message(&me, &node, at_ms), &sig).is_err() {
                    debug(&format!("hello from {from}: the signature does not hold"));
                    return;
                }
                debug(&format!("hello from {from}: {}", crate::hex(&node[..4])));
                sess.peer = Some(node);
                door.clients.lock().unwrap().insert(node, id);
            }
            DoorCtl::Gossip(raw) => {
                // Only the tab's own crew note: what it plays never goes this way.
                let Some(peer) = sess.peer else { return };
                let Ok(g) = postcard::from_bytes::<Gossip>(&raw) else { return };
                debug(&format!("gossip from {from}: {:?}", g.crew_checked(now_ms()).map(|(n, r)| (n == peer, r))));
                if g.crew_checked(now_ms()).is_some_and(|(n, _)| n == peer) && raw.len() <= 1024 {
                    self.notes.lock().unwrap().keep(&g, &raw, now_ms());
                    let _ = self.on_gossip(g.clone());
                    let i = self.clone();
                    tokio::spawn(async move { i.broadcast(&g).await });
                    door.push(&raw);
                }
            }
            DoorCtl::Note(_) => {}
        }
    }
}

/// The two ICE names of a binding request: the receiver's, then the sender's.
fn ice_names(pkt: &[u8]) -> Option<(String, String)> {
    let m = str0m::ice::StunMessage::parse(pkt).ok()?;
    if !m.is_binding_request() {
        return None;
    }
    let (a, b) = m.split_username()?;
    Some((a.to_string(), b.to_string()))
}

/// One of an introduced tab's candidates, as the browser wrote it: a UDP address only, never a
/// name to look up (a browser's own `.local` names are passed over, and found from its first
/// check instead), and on the open internet only a public address, so no offer can turn the
/// door's checks on the house it stands in.
fn remote_candidate(line: &str) -> Option<Candidate> {
    let l = line.trim().trim_start_matches("a=").strip_prefix("candidate:")?;
    let t: Vec<&str> = l.split_whitespace().collect();
    if t.len() < 8 || !t[2].eq_ignore_ascii_case("udp") || t[6] != "typ" {
        return None;
    }
    let a = SocketAddr::new(t[4].parse::<IpAddr>().ok()?, t[5].parse().ok()?);
    let test = crate::net::LOCAL_RELAYS_OK.load(Ordering::Relaxed);
    if a.port() == 0 || !(test || crate::net::relay_public(&format!("http://{a}/"))) {
        return None;
    }
    Candidate::host(a, "udp").ok()
}

/// A session for a tab a middle introduced: full ICE from the door's side (controlled, with
/// checks of its own to the tab's candidates, so the door's NAT opens towards the tab), the
/// tab's certificate held to its offer, and the same data channel a WebRTC-direct tab gets.
/// Returns the session, and the door's ICE name and password for it.
fn intro_session(cert: &DtlsCert, offer: &Offer, local: SocketAddr) -> Option<(Session, String, String)> {
    let ufrag = format!("pyi{}", crate::hex(&rand::random::<[u8; 6]>()));
    let pwd = crate::hex(&rand::random::<[u8; 16]>());
    let mut rtc = RtcConfig::new().set_ice_lite(false).set_dtls_cert(cert.clone()).set_local_ice_credentials(IceCreds { ufrag: ufrag.clone(), pass: pwd.clone() }).build(Instant::now());
    rtc.add_local_candidate(Candidate::host(local, "udp").ok()?);
    for c in offer.cands.iter().filter_map(|c| remote_candidate(c)).take(8) {
        rtc.add_remote_candidate(c);
    }
    {
        let mut api = rtc.direct_api();
        api.set_ice_controlling(false);
        api.set_remote_ice_credentials(IceCreds { ufrag: offer.ufrag.clone(), pass: offer.pwd.clone() });
        api.set_remote_fingerprint(str0m::config::Fingerprint { hash_func: "sha-256".into(), bytes: offer.fp.to_vec() });
        api.start_dtls(false).ok()?;
        api.start_sctp(false);
        api.create_data_channel(ChannelConfig { label: "pyrate".into(), ordered: true, negotiated: Some(0), ..Default::default() });
    }
    let s = Session::new(rtc, None, local, Some(ufrag.clone()));
    Some((s, ufrag, pwd))
}

impl Session {
    fn new(rtc: Rtc, at: Option<SocketAddr>, local: SocketAddr, ufrag: Option<String>) -> Self {
        Session {
            rtc, at, local, ufrag, addrs: at.into_iter().collect(), ch: None, peer: None, parts: HashMap::new(), out: VecDeque::new(), waiting: HashMap::new(), next: 1,
            asks: 0, counted: HashSet::new(), out_bytes: 0, cards_at: None, born: Instant::now(), wake: Instant::now(),
        }
    }
}

async fn run(inner: Weak<Inner>, door: Arc<Door>, sock: UdpSocket, cert: DtlsCert, cands: Vec<SocketAddr>, mut rx: mpsc::UnboundedReceiver<Cmd>) {
    let mut sessions: HashMap<u64, Session> = HashMap::new();
    // Which session each address's packets go to, and introduced sessions by the door's ICE name.
    let mut by_addr: HashMap<SocketAddr, u64> = HashMap::new();
    let mut by_name: HashMap<String, u64> = HashMap::new();
    let mut next_id: u64 = 1;
    let mut buf = vec![0u8; 2048];
    // A WebRTC-direct session's packets are given as arriving at the address its note names, an
    // introduced one's at the first address the door gives.
    let listed = *door.public.lock().unwrap();
    let local_intro = cands.first().copied().unwrap_or(SocketAddr::from(([127, 0, 0, 1], door.port)));
    let cand_names: Vec<String> = cands.iter().map(|a| a.to_string()).collect();
    let mut opened_recent: VecDeque<Instant> = VecDeque::new();
    // Plain STUN answers lately, by address: no one gets the port as a reflector.
    let mut stun_from: HashMap<IpAddr, (Instant, u32)> = HashMap::new();
    loop {
        let now = Instant::now();
        let wake = sessions.values().map(|s| s.wake).min().unwrap_or(now + Duration::from_secs(1)).max(now + Duration::from_millis(1));
        let mut touched: Vec<u64> = Vec::new();
        tokio::select! {
            r = sock.recv_from(&mut buf) => {
                let Ok((n, from)) = r else { continue };
                let pkt = &buf[..n];
                let Some(inner) = inner.upgrade() else { return };
                // An introduced tab's first check from an address not seen yet: by the door's own
                // ICE name for it.
                let known = by_addr.get(&from).copied().or_else(|| {
                    let (mine, _) = ice_names(pkt)?;
                    let id = *by_name.get(&mine)?;
                    let s = sessions.get_mut(&id)?;
                    if s.addrs.len() >= INTRO_ADDRS {
                        return None;
                    }
                    s.addrs.push(from);
                    s.at.get_or_insert(from);
                    by_addr.insert(from, id);
                    Some(id)
                });
                if let Some(id) = known {
                    if let Some(s) = sessions.get_mut(&id) {
                        if let Ok(rcv) = Receive::new(Protocol::Udp, from, s.local, pkt) {
                            if let Err(e) = s.rtc.handle_input(Input::Receive(Instant::now(), rcv)) {
                                debug(&format!("input from {from}: {e}"));
                            }
                            touched.push(id);
                        }
                    }
                } else if let Some(out) = stun_answer(pkt, from) {
                    if !stun_ok(&mut stun_from, from.ip(), Instant::now()) {
                        continue;
                    }
                    door.stun.fetch_add(1, Ordering::Relaxed);
                    let _ = sock.try_send_to(&out, from);
                } else if let (Some(public), Some((ufrag, theirs))) = (listed, direct_ufrag(pkt)) {
                    // A new browser dialling the listed address. At most a few a second, a few
                    // from any one address, and a door's share at once.
                    opened_recent.retain(|t| t.elapsed() < Duration::from_secs(1));
                    let same_ip = sessions.values().filter(|s| s.at.is_some_and(|a| a.ip() == from.ip())).count();
                    if !admit(sessions.len(), same_ip, opened_recent.len()) || !inner.crew.load(Ordering::Relaxed) {
                        continue;
                    }
                    opened_recent.push_back(Instant::now());
                    let creds = IceCreds { ufrag: ufrag.clone(), pass: ufrag.clone() };
                    // A lite door never checks the browser, so the browser's password is never needed.
                    let remote = IceCreds { ufrag: theirs, pass: ufrag.clone() };
                    let mut rtc = RtcConfig::new()
                        .set_ice_lite(true)
                        .set_dtls_cert(cert.clone())
                        .set_fingerprint_verification(false)
                        .set_local_ice_credentials(creds.clone())
                        .build(Instant::now());
                    let Ok(c) = Candidate::host(public, "udp") else { continue };
                    rtc.add_local_candidate(c);
                    {
                        let mut api = rtc.direct_api();
                        api.set_ice_controlling(false);
                        api.set_remote_ice_credentials(remote);
                        // The browser's certificate is its own business: a tab proves who it is by
                        // its hello, not its certificate. str0m wants a fingerprint all the same.
                        api.set_remote_fingerprint(str0m::config::Fingerprint { hash_func: "sha-256".into(), bytes: vec![0; 32] });
                        if api.start_dtls(false).is_err() {
                            continue;
                        }
                        api.start_sctp(false);
                        api.create_data_channel(ChannelConfig { label: "pyrate".into(), ordered: true, negotiated: Some(0), ..Default::default() });
                    }
                    let mut s = Session::new(rtc, Some(from), public, None);
                    if let Ok(rcv) = Receive::new(Protocol::Udp, from, public, pkt) {
                        let _ = s.rtc.handle_input(Input::Receive(Instant::now(), rcv));
                    }
                    let id = next_id;
                    next_id += 1;
                    sessions.insert(id, s);
                    by_addr.insert(from, id);
                    door.opened.fetch_add(1, Ordering::Relaxed);
                    touched.push(id);
                }
            }
            c = rx.recv() => {
                match c {
                    None | Some(Cmd::Close) => break,
                    Some(Cmd::Send { to, id, bytes }) => {
                        if let Some(s) = sessions.get_mut(&to) {
                            s.queue(frame(id, &bytes));
                            touched.push(to);
                        }
                    }
                    Some(Cmd::Ask { tab, bytes, reply }) => {
                        let to = door.clients.lock().unwrap().get(&tab).copied();
                        if let Some((to, s)) = to.and_then(|to| sessions.get_mut(&to).map(|s| (to, s))) {
                            let id = FROM_DOOR | s.next;
                            s.next = s.next.wrapping_add(1) & !FROM_DOOR;
                            if s.waiting.len() < 32 && s.queue(frame(id, &bytes)) {
                                s.waiting.insert(id, reply);
                                touched.push(to);
                            }
                        }
                    }
                    Some(Cmd::Push(g)) => {
                        let Ok(b) = postcard::to_stdvec(&DoorCtl::Gossip(g)) else { continue };
                        for (a, s) in sessions.iter_mut() {
                            if s.peer.is_some() && s.out.len() < 256 && s.queue(frame(0, &b)) {
                                touched.push(*a);
                            }
                        }
                    }
                    Some(Cmd::Intro { offer, reply }) => {
                        let waiting = sessions.values().filter(|s| s.ufrag.is_some() && s.ch.is_none()).count();
                        let crew = inner.upgrade().is_some_and(|i| i.crew.load(Ordering::Relaxed));
                        let made = (sessions.len() < MAX_SESSIONS && waiting < INTRO_PENDING && crew).then(|| intro_session(&cert, &offer, local_intro)).flatten();
                        let Some((s, ufrag, pwd)) = made else {
                            let _ = reply.send(None);
                            continue;
                        };
                        let id = next_id;
                        next_id += 1;
                        by_name.insert(ufrag.clone(), id);
                        sessions.insert(id, s);
                        door.opened.fetch_add(1, Ordering::Relaxed);
                        let _ = reply.send(Some(Answer { ufrag, pwd, fp: door.fp, cands: cand_names.clone(), since: door.since }));
                        touched.push(id);
                    }
                }
            }
            _ = tokio::time::sleep_until(wake.into()) => {
                let now = Instant::now();
                for (a, s) in sessions.iter_mut() {
                    if s.wake <= now {
                        let _ = s.rtc.handle_input(Input::Timeout(now));
                        touched.push(*a);
                    }
                }
            }
        }
        let Some(inner) = inner.upgrade() else { return };
        touched.sort();
        touched.dedup();
        for id in touched {
            let Some(s) = sessions.get_mut(&id) else { continue };
            let sent = drive(&inner, &door, &sock, id, s);
            // An introduced tab's checks went out to its candidates: their answers come back to it.
            if s.ufrag.is_some() {
                for a in sent {
                    if s.addrs.len() < INTRO_ADDRS && !by_addr.contains_key(&a) {
                        s.addrs.push(a);
                        by_addr.insert(a, id);
                    }
                }
            }
        }
        // Gone: closed, or never opened a channel within fifteen seconds (twenty for a tab that
        // was introduced, whose checks start from both ends).
        let before = sessions.len();
        sessions.retain(|id, s| {
            let grace = Duration::from_secs(if s.ufrag.is_some() { 20 } else { 15 });
            let keep = s.rtc.is_alive() && (s.ch.is_some() || s.born.elapsed() < grace);
            if !keep {
                if let Some(p) = s.peer {
                    let mut c = door.clients.lock().unwrap();
                    if c.get(&p) == Some(id) {
                        c.remove(&p);
                    }
                }
                for a in &s.addrs {
                    if by_addr.get(a) == Some(id) {
                        by_addr.remove(a);
                    }
                }
                if let Some(u) = &s.ufrag {
                    by_name.remove(u);
                }
            }
            keep
        });
        if sessions.len() != before {
            door.sessions.store(sessions.values().filter(|s| s.ch.is_some()).count() as u64, Ordering::Relaxed);
        }
    }
    door.clients.lock().unwrap().clear();
}

/// Runs a session's output. Returns where it sent packets, so an introduced tab's answers to the
/// door's own checks find their way back to it.
fn drive(inner: &Arc<Inner>, door: &Arc<Door>, sock: &UdpSocket, id: u64, s: &mut Session) -> Vec<SocketAddr> {
    let mut sent = Vec::new();
    // What waits to go out goes, as far as the channel takes it.
    s.flush();
    let at = s.at.map(|a| a.to_string()).unwrap_or_else(|| "an introduced tab".into());
    loop {
        match s.rtc.poll_output() {
            Ok(Output::Timeout(t)) => {
                s.wake = t;
                break;
            }
            Ok(Output::Transmit(t)) => {
                let _ = sock.try_send_to(&t.contents, t.destination);
                if !sent.contains(&t.destination) {
                    sent.push(t.destination);
                }
            }
            Ok(Output::Event(e)) => match { debug(&format!("event {at}: {e:?}")); e } {
                Event::ChannelOpen(cid, _) => {
                    s.ch = Some(cid);
                    door.sessions.fetch_add(1, Ordering::Relaxed);
                    // Its note first: the listed door's own, or for an introduced tab the door's
                    // presence, which names the radio the middle drew and no address.
                    let note = door.note(&inner.secret, now_ms()).filter(|_| s.ufrag.is_none()).or_else(|| inner.presence_note());
                    if let Some(g) = note {
                        if let Ok(b) = postcard::to_stdvec(&g).and_then(|n| postcard::to_stdvec(&DoorCtl::Note(n))) {
                            s.queue(frame(0, &b));
                        }
                    }
                    s.flush();
                }
                Event::ChannelData(d) => {
                    if d.data.len() < 5 {
                        continue;
                    }
                    let fid = u32::from_be_bytes(d.data[..4].try_into().unwrap());
                    let last = d.data[4] == 1;
                    let total: usize = s.parts.values().map(|p| p.len()).sum();
                    if total + d.data.len() > MESSAGE_MAX || (!s.parts.contains_key(&fid) && s.parts.len() >= 2 * ASKS_PER_TAB) {
                        s.rtc.disconnect();
                        break;
                    }
                    s.parts.entry(fid).or_default().extend_from_slice(&d.data[5..]);
                    if !last {
                        continue;
                    }
                    let msg = s.parts.remove(&fid).unwrap_or_default();
                    if fid == 0 {
                        inner.door_ctl(door, id, s, &msg);
                    } else if fid & FROM_DOOR != 0 {
                        if let Some(w) = s.waiting.remove(&fid) {
                            let ans = match postcard::from_bytes::<Response>(&msg) {
                                Ok(Response::Hop(b)) => b,
                                _ => vec![],
                            };
                            let _ = w.send(ans);
                        }
                    } else {
                        // A request: only from a tab that has said who it is, answered in its
                        // own task, at most eight waiting at a time per tab (a slot frees once
                        // its answer has gone), and the cards at most once in ten seconds.
                        let cards = matches!(postcard::from_bytes::<Request>(&msg), Ok(Request::Cards));
                        if !may_ask(s.peer.is_some(), s.asks, s.counted.contains(&fid), cards, s.cards_at, Instant::now()) {
                            if !s.queue(frame(fid, &postcard::to_stdvec(&Response::Hop(vec![])).unwrap_or_default())) {
                                break;
                            }
                            continue;
                        }
                        if cards {
                            s.cards_at = Some(Instant::now());
                        }
                        s.asks += 1;
                        s.counted.insert(fid);
                        let (i2, d2, peer) = (inner.clone(), door.clone(), s.peer.unwrap_or_default());
                        tokio::spawn(async move {
                            let ans = i2.door_request(msg, peer).await;
                            d2.served.fetch_add(1, Ordering::Relaxed);
                            let _ = d2.cmd.send(Cmd::Send { to: id, id: fid, bytes: ans });
                        });
                    }
                }
                Event::ChannelBufferedAmountLow(_) => s.flush(),
                Event::ChannelClose(_) => s.rtc.disconnect(),
                Event::IceConnectionStateChange(IceConnectionState::Disconnected) => s.rtc.disconnect(),
                _ => {}
            },
            Err(e) => {
                debug(&format!("poll {at}: {e}"));
                s.rtc.disconnect();
                break;
            }
        }
    }
    // An answer queued while the channel was busy is tried again soon.
    if !s.out.is_empty() && s.ch.is_some() {
        s.wake = s.wake.min(Instant::now() + Duration::from_millis(20));
    }
    sent
}

fn debug(m: &str) {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if *ON.get_or_init(|| std::env::var("PYRATE_DOOR_DEBUG").is_ok()) {
        crate::say(&format!("door: {m}"));
    }
}

/// A tab's id, for callers outside.
pub fn tab_id(id: &EndpointId) -> [u8; 32] {
    *id.as_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_door_holds_each_browser_to_its_share() {
        let now = Instant::now();
        // STUN: five answers to one address in ten seconds, others unaffected, then five more.
        let mut seen = HashMap::new();
        let (a, b): (IpAddr, IpAddr) = ("198.51.100.1".parse().unwrap(), "198.51.100.2".parse().unwrap());
        assert!((0..5).all(|_| stun_ok(&mut seen, a, now)));
        assert!(!stun_ok(&mut seen, a, now), "the sixth in ten seconds goes unanswered");
        assert!(stun_ok(&mut seen, b, now), "another address has its own five");
        assert!(stun_ok(&mut seen, a, now + Duration::from_secs(10)), "and ten seconds on, again");
        // Four browsers from one address, sixty-four in all, eight new in a second.
        assert!(admit(10, 3, 0) && !admit(10, 4, 0), "a fifth from one address waits");
        assert!(!admit(MAX_SESSIONS, 0, 0) && !admit(10, 0, 8));
        // A request only after the hello, with a slot free, and the cards once in ten seconds.
        assert!(!may_ask(false, 0, false, false, None, now), "nothing before the hello");
        assert!(may_ask(true, 0, false, false, None, now));
        assert!(!may_ask(true, ASKS_PER_TAB, false, false, None, now), "eight waiting at most");
        assert!(!may_ask(true, 0, true, false, None, now), "an id already waiting is not taken twice");
        assert!(!may_ask(true, 0, false, true, Some(now), now + Duration::from_secs(9)), "the cards once in ten seconds");
        assert!(may_ask(true, 0, false, true, Some(now), now + CARDS_EVERY));
        // A slot frees as the last part of its answer goes.
        let parts = frame(7, &vec![1u8; PART * 2 + 10]);
        assert_eq!(parts.iter().map(|f| last_part(f)).collect::<Vec<_>>(), vec![None, None, Some(7)]);
    }

    #[test]
    fn a_browser_that_never_reads_is_let_go() {
        let rtc = RtcConfig::new().build(Instant::now());
        let mut s = Session::new(rtc, None, "127.0.0.1:1".parse().unwrap(), None);
        let mb = vec![0u8; 1 << 20];
        assert!((0..4).all(|i| s.queue(frame(i, &mb[..(1 << 20) - 4096]))), "four MiB may wait");
        assert!(!s.queue(frame(9, &mb)), "past that the browser is let go");
        assert!(!s.rtc.is_alive());
    }

    #[test]
    fn a_door_opens_only_where_the_network_sees_this_device() {
        let seen: Ipv4Addr = "198.51.100.20".parse().unwrap();
        assert_eq!(address_rule(IpAddr::V4(seen), Some(seen)), Ok(()), "the router's address is the one seen: a door");
        assert_eq!(address_rule("203.0.113.9".parse().unwrap(), Some(seen)), Err(WHY_NOT_SEEN), "another address (a VPN, say): no door");
        assert_eq!(address_rule(IpAddr::V4(seen), None), Err(WHY_NOT_SEEN), "the network has not said: no door");
    }

    #[test]
    fn an_app_door_is_listed_only_behind_a_vpn_and_gives_only_the_addresses_seen() {
        let seen: Ipv4Addr = "45.87.214.21".parse().unwrap();
        let home: IpAddr = "45.87.214.30".parse().unwrap();
        // The VPN case: mapped through a tunnel, at an address that is not home's.
        assert!(listable(true, IpAddr::V4(seen), &[home]));
        assert!(!listable(false, IpAddr::V4(seen), &[home]), "the home router's own port: never listed");
        assert!(!listable(true, home, &[home]), "a tunnel that comes out at home: never listed");
        // An introduced door gives the router's port and the address the network sees, and on a
        // test network loopback when it has nothing else; never anything when the network has
        // said nothing and none was given.
        let mapped: SocketAddr = "45.87.214.21:41000".parse().unwrap();
        assert_eq!(app_candidates(None, Some(mapped), Some(seen), 40000, false), vec![mapped, "45.87.214.21:40000".parse().unwrap()]);
        assert_eq!(app_candidates(None, None, Some(seen), 40000, false), vec!["45.87.214.21:40000".parse::<SocketAddr>().unwrap()]);
        assert!(app_candidates(None, None, None, 40000, false).is_empty(), "the network has not said: no door");
        assert_eq!(app_candidates(None, None, None, 40000, true), vec!["127.0.0.1:40000".parse::<SocketAddr>().unwrap()]);
        // A tab's own candidates: a public UDP address only, never a name or a house's address.
        assert!(remote_candidate("candidate:1 1 udp 2130706431 45.87.214.22 50000 typ srflx raddr 0.0.0.0 rport 0 generation 0").is_some());
        for bad in ["candidate:1 1 udp 2130706431 3f1c-a1.local 50000 typ host", "candidate:1 1 tcp 2130706431 45.87.214.22 50000 typ host", "candidate:1 1 udp 1 192.168.1.4 50000 typ host", "candidate:1 1 udp 1 45.87.214.22 0 typ host", "nonsense"] {
            assert!(remote_candidate(bad).is_none(), "{bad}");
        }
    }

    /// Runs a stand-in tab and an introduced door against each other in memory, packets passed
    /// by hand, until the door's channel opens or `steps` run out. Returns whether it opened,
    /// and the ICE names on the tab's first check.
    fn meet(door: &mut Session, tab: &mut Rtc, door_at: SocketAddr, tab_at: SocketAddr, steps: usize) -> (bool, Option<(String, String)>) {
        let mut now = Instant::now();
        let mut first = None;
        for _ in 0..steps {
            let mut to_door = Vec::new();
            let mut to_tab = Vec::new();
            loop {
                match tab.poll_output() {
                    Ok(Output::Transmit(t)) => to_door.push(t.contents.to_vec()),
                    Ok(Output::Timeout(_)) => break,
                    Ok(Output::Event(_)) => {}
                    Err(_) => return (false, first),
                }
            }
            loop {
                match door.rtc.poll_output() {
                    Ok(Output::Transmit(t)) => to_tab.push((t.destination, t.contents.to_vec())),
                    Ok(Output::Timeout(_)) => break,
                    Ok(Output::Event(Event::ChannelOpen(..))) => return (true, first),
                    Ok(Output::Event(_)) => {}
                    Err(_) => return (false, first),
                }
            }
            for p in to_door {
                if first.is_none() {
                    first = ice_names(&p);
                }
                let _ = door.rtc.handle_input(Input::Receive(now, Receive::new(Protocol::Udp, tab_at, door_at, &p).unwrap()));
            }
            for (to, p) in to_tab {
                assert_eq!(to, tab_at, "the door's checks go to the tab's own candidate");
                let _ = tab.handle_input(Input::Receive(now, Receive::new(Protocol::Udp, door_at, tab_at, &p).unwrap()));
            }
            now += Duration::from_millis(20);
            let _ = tab.handle_input(Input::Timeout(now));
            let _ = door.rtc.handle_input(Input::Timeout(now));
        }
        (false, first)
    }

    #[test]
    fn an_introduced_tab_and_its_door_meet_by_full_ice() {
        let p = str0m::crypto::from_feature_flags();
        let door_cert = p.dtls_provider.generate_certificate().unwrap();
        let tab_cert = p.dtls_provider.generate_certificate().unwrap();
        let (door_at, tab_at): (SocketAddr, SocketAddr) = ("45.87.214.21:47000".parse().unwrap(), "45.87.214.22:47001".parse().unwrap());
        let tab_creds = IceCreds { ufrag: "tabu".into(), pass: "t".repeat(24) };
        let offer = Offer { ufrag: tab_creds.ufrag.clone(), pwd: tab_creds.pass.clone(), fp: sha256(&tab_cert.certificate), cands: vec![format!("candidate:1 1 udp 2130706431 {} {} typ host", tab_at.ip(), tab_at.port())] };
        assert!(offer.sound());
        let (mut door, ufrag, pwd) = intro_session(&door_cert, &offer, door_at).unwrap();
        assert!(ufrag.starts_with("pyi") && pwd.len() >= 22);
        // The tab, as a browser would be: the offerer, controlling, the DTLS client.
        let tab_with = |fp: [u8; 32]| {
            let mut tab = RtcConfig::new().set_ice_lite(false).set_dtls_cert(tab_cert.clone()).set_local_ice_credentials(tab_creds.clone()).build(Instant::now());
            tab.add_local_candidate(Candidate::host(tab_at, "udp").unwrap());
            tab.add_remote_candidate(Candidate::host(door_at, "udp").unwrap());
            {
                let mut api = tab.direct_api();
                api.set_ice_controlling(true);
                api.set_remote_ice_credentials(IceCreds { ufrag: ufrag.clone(), pass: pwd.clone() });
                api.set_remote_fingerprint(str0m::config::Fingerprint { hash_func: "sha-256".into(), bytes: fp.to_vec() });
                api.start_dtls(true).unwrap();
                api.start_sctp(true);
                api.create_data_channel(ChannelConfig { label: "pyrate".into(), ordered: true, negotiated: Some(0), ..Default::default() });
            }
            tab
        };
        let mut tab = tab_with(sha256(&door_cert.certificate));
        let (opened, names) = meet(&mut door, &mut tab, door_at, tab_at, 400);
        assert!(opened, "the channel opens with no relay and no mapping");
        assert_eq!(names.map(|n| n.0), Some(ufrag.clone()), "the tab's first check names the door's ICE name for it, which binds its address");
        // A tab whose certificate is not the one its offer named never gets a channel.
        let (mut door2, ufrag2, pwd2) = intro_session(&door_cert, &Offer { fp: [7; 32], ..offer.clone() }, door_at).unwrap();
        let mut tab2 = tab_with(sha256(&door_cert.certificate));
        tab2.direct_api().set_remote_ice_credentials(IceCreds { ufrag: ufrag2, pass: pwd2 });
        assert!(!meet(&mut door2, &mut tab2, door_at, tab_at, 400).0, "another certificate: no channel");
    }

    #[test]
    fn a_door_identity_never_survives_a_new_address_or_a_new_id() {
        let p = str0m::crypto::from_feature_flags();
        let cert = p.dtls_provider.generate_certificate().unwrap();
        let (me, other) = ([1u8; 32], [2u8; 32]);
        let home: IpAddr = "198.51.100.20".parse().unwrap();
        let note = identity_note(&cert, 41_000, 1_000, &me, home).unwrap();
        let kept = kept_identity(&note, &me, home).unwrap();
        assert_eq!((kept.1, kept.2), (41_000, 1_000), "the same id at the same address keeps its door");
        assert!(kept_identity(&note, &me, "203.0.113.9".parse().unwrap()).is_none(), "a new address: a new certificate, port and first day");
        assert!(kept_identity(&note, &other, home).is_none(), "a new id: the same");
        // One kept by an older version, with neither id nor address beside it, is never taken.
        let old = postcard::to_stdvec(&(&cert.certificate, &cert.private_key, 41_000u16, 1_000u64)).unwrap();
        assert!(kept_identity(&old, &me, home).is_none());
    }

    #[test]
    fn a_plain_stun_request_is_answered_with_where_it_came_from() {
        let mut req = vec![0, 1, 0, 0, 0x21, 0x12, 0xA4, 0x42];
        req.extend_from_slice(&[7u8; 12]);
        let from: SocketAddr = "203.0.113.9:40000".parse().unwrap();
        let out = stun_answer(&req, from).unwrap();
        assert_eq!(&out[..2], &[1, 1]);
        assert_eq!(&out[8..20], &[7u8; 12]);
        let port = u16::from_be_bytes([out[26], out[27]]) ^ 0x2112;
        let ip = u32::from_be_bytes(out[28..32].try_into().unwrap()) ^ 0x2112_A442;
        assert_eq!(port, 40000);
        assert_eq!(std::net::Ipv4Addr::from(ip), "203.0.113.9".parse::<std::net::Ipv4Addr>().unwrap());
    }

    #[test]
    fn an_ice_check_is_left_to_its_session() {
        // A binding request with a USERNAME attribute is not a plain STUN request.
        let mut req = vec![0, 1, 0, 8, 0x21, 0x12, 0xA4, 0x42];
        req.extend_from_slice(&[1u8; 12]);
        req.extend_from_slice(&[0, 6, 0, 4, b'a', b'b', b':', b'c']);
        assert!(stun_answer(&req, "198.51.100.1:1".parse().unwrap()).is_none());
    }

    #[test]
    fn a_long_message_goes_in_parts_and_only_the_last_says_so() {
        let f = frame(9, &vec![1u8; PART * 2 + 3]);
        assert_eq!(f.len(), 3);
        assert!(f[..2].iter().all(|p| p[4] == 0));
        assert_eq!(f[2][4], 1);
        assert_eq!(frame(1, &[]).len(), 1);
    }
}
