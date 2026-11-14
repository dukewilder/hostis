//! Doors: a listener's app as a way in for browsers, with no relay and no TLS.
//!
//! A page cannot take connections, so a browser reaches the network only through something
//! that can. A door is a listener's app (on a computer, or a phone on Wi-Fi while charging)
//! that takes a browser's WebRTC data channel straight, WebRTC-direct style: the browser knows
//! the door's address and certificate fingerprint from a signed note, writes the door's half of
//! the SDP itself, and no signalling server is asked anything. The ICE username and password are
//! the same random string, which the browser chooses and the door reads off the first STUN
//! request (as in libp2p's webrtc-direct). The DTLS certificate is the door's own, kept from one
//! run to the next, so its fingerprint in the note stays true.
//!
//! Over the channel run the same requests and answers a tab sends over iroh. A door answers a
//! hop as a first hop does (it is the tab's guard), hands over the cards it knows, passes on the
//! gossip it hears, and carries forwards from other radios to the tabs behind it, as a relay
//! would. It answers nothing else: what a tab asks of the network goes through routes the door
//! cannot read. The door sees the tab's address, as a guard does.
//!
//! The same socket answers plain STUN binding requests, so tabs can learn their own address
//! from the network's doors instead of anyone else's server.
//!
//! No rights reserved.

use std::{
    collections::{HashMap, VecDeque},
    net::{IpAddr, SocketAddr},
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

/// Browsers at once on one door: a computer's share, and a phone's.
const MAX_SESSIONS: usize = 64;
/// A data channel message: the same parts the tabs' own links use, each with its five-byte head
/// inside the 16 KiB the channel's SDP allows.
const PART: usize = 16 * 1024 - 5;
/// The most one request or answer may be.
const MESSAGE_MAX: usize = 4 << 20;
/// Requests from one browser waiting on an answer at once.
const ASKS_PER_TAB: usize = 8;
/// Frame ids the door starts carry this bit; the tab's own start at one.
const FROM_DOOR: u32 = 0x8000_0000;

enum Cmd {
    /// An answer (or a push, under id 0) to the browser at `to`.
    Send { to: SocketAddr, id: u32, bytes: Vec<u8> },
    /// A request to the tab `tab`, from another radio's forward.
    Ask { tab: [u8; 32], bytes: Vec<u8>, reply: oneshot::Sender<Vec<u8>> },
    /// Gossip to every tab behind this door.
    Push(Vec<u8>),
    Close,
}

struct Session {
    rtc: Rtc,
    ch: Option<ChannelId>,
    peer: Option<[u8; 32]>,
    parts: HashMap<u32, Vec<u8>>,
    out: VecDeque<Vec<u8>>,
    waiting: HashMap<u32, oneshot::Sender<Vec<u8>>>,
    next: u32,
    asks: Arc<std::sync::atomic::AtomicUsize>,
    born: Instant,
    wake: Instant,
}

pub struct Door {
    pub fp: [u8; 32],
    pub port: u16,
    pub since: u64,
    /// Where browsers reach it: a mapped port on the home router, or an address the keeper gave.
    pub public: Mutex<Option<SocketAddr>>,
    pub app: bool,
    /// Its owner lets shared links name it.
    pub link: AtomicBool,
    cmd: mpsc::UnboundedSender<Cmd>,
    clients: Mutex<HashMap<[u8; 32], SocketAddr>>,
    pub sessions: AtomicU64,
    pub opened: AtomicU64,
    pub served: AtomicU64,
    pub stun: AtomicU64,
    closed: AtomicBool,
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
}

/// The door's certificate and port, kept in the node's sealed notes from one run to the next.
fn identity(inner: &Inner) -> Option<(DtlsCert, u16, u64)> {
    if let Some(b) = inner.hold.read_note(&inner.cfg.dir, "door", None) {
        if let Ok((certificate, private_key, port, since)) = postcard::from_bytes::<(Vec<u8>, Vec<u8>, u16, u64)>(&b) {
            return Some((DtlsCert { certificate, private_key }, port, since));
        }
    }
    let p = str0m::crypto::from_feature_flags();
    let cert = p.dtls_provider.generate_certificate()?;
    let port = 20_000 + (rand::random::<u16>() % 40_000);
    let since = now_ms();
    let _ = inner.hold.write_note(&inner.cfg.dir, "door", &postcard::to_stdvec(&(&cert.certificate, &cert.private_key, port, since)).ok()?);
    Some((cert, port, since))
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
    /// Opens this node's door on its kept port. `public` is where browsers reach it: given by
    /// the keeper or the test network, or else asked of the home router (see `portmap`). With
    /// no public address the door stays shut: a door nobody can reach is announced nowhere.
    pub async fn door_open(self: &Arc<Self>, public: Option<SocketAddr>, app: bool, link: bool) -> anyhow::Result<Arc<Door>> {
        if let Some(d) = self.door.lock().unwrap().clone() {
            return Ok(d);
        }
        let (cert, kept_port, since) = identity(self).ok_or_else(|| anyhow::anyhow!("no certificate"))?;
        let port = std::env::var("PYRATE_DOOR_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(kept_port);
        let sock = UdpSocket::bind(SocketAddr::from(([0, 0, 0, 0], port))).await?;
        let port = sock.local_addr()?.port();
        let public = match public {
            Some(p) => Some(p),
            None => crate::portmap::map_udp(port).await,
        };
        let Some(public) = public else {
            say("door: no port could be mapped on the router, so this radio is not a door");
            anyhow::bail!("unreachable");
        };
        let (tx, rx) = mpsc::unbounded_channel();
        let door = Arc::new(Door {
            fp: sha256(&cert.certificate),
            port,
            since,
            public: Mutex::new(Some(public)),
            app,
            link: AtomicBool::new(link),
            cmd: tx,
            clients: Mutex::new(HashMap::new()),
            sessions: AtomicU64::new(0),
            opened: AtomicU64::new(0),
            served: AtomicU64::new(0),
            stun: AtomicU64::new(0),
            closed: AtomicBool::new(false),
        });
        *self.door.lock().unwrap() = Some(door.clone());
        say(&format!("door open at {public} (udp {port})"));
        let (w, d2) = (Arc::downgrade(self), door.clone());
        tokio::spawn(async move { run(w, d2, sock, cert, public, rx).await });
        // Its note, now and every two minutes while it is open.
        let (i2, d3) = (self.clone(), door.clone());
        tokio::spawn(async move {
            // First once the Log has neighbours to hear it, then every two minutes.
            tokio::time::sleep(Duration::from_secs(8)).await;
            while !d3.closed.load(Ordering::Relaxed) {
                if let Some(g) = d3.note(&i2.secret, now_ms()) {
                    if let Ok(raw) = postcard::to_stdvec(&g) {
                        i2.notes.lock().unwrap().keep(&g, &raw, now_ms());
                    }
                    i2.broadcast(&g).await;
                }
                let n = d3.served.load(Ordering::Relaxed);
                tokio::time::sleep(Duration::from_secs(if n == 0 && now_ms().saturating_sub(d3.since) < 120_000 { 20 } else { 120 })).await;
            }
        });
        Ok(door)
    }

    pub fn door_close(&self) {
        if let Some(d) = self.door.lock().unwrap().take() {
            d.close();
            say("door closed");
        }
    }

    /// A request from a browser on this door: a hop as its first hop, or the cards.
    async fn door_request(self: &Arc<Self>, bytes: Vec<u8>) -> Vec<u8> {
        let resp = match postcard::from_bytes::<Request>(&bytes) {
            Ok(Request::Hop(b)) => {
                let Ok(_hop) = (if self.cfg.lantern { &crate::HOPPING_LANTERN } else { &crate::HOPPING }).try_acquire() else { return vec![] };
                Response::Hop(self.on_hop(&b).await)
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
    fn door_ctl(self: &Arc<Self>, door: &Door, from: SocketAddr, sess: &mut Session, bytes: &[u8]) {
        let Ok(ctl) = postcard::from_bytes::<DoorCtl>(bytes) else { return };
        let me = *self.endpoint.id().as_bytes();
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
                door.clients.lock().unwrap().insert(node, from);
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

async fn run(inner: Weak<Inner>, door: Arc<Door>, sock: UdpSocket, cert: DtlsCert, public: SocketAddr, mut rx: mpsc::UnboundedReceiver<Cmd>) {
    let mut sessions: HashMap<SocketAddr, Session> = HashMap::new();
    let mut buf = vec![0u8; 2048];
    // Every session's packets are given as arriving here, the address its one candidate names.
    let local = SocketAddr::new(public.ip(), public.port());
    let mut opened_recent: VecDeque<Instant> = VecDeque::new();
    loop {
        let now = Instant::now();
        let wake = sessions.values().map(|s| s.wake).min().unwrap_or(now + Duration::from_secs(1)).max(now + Duration::from_millis(1));
        let mut touched: Vec<SocketAddr> = Vec::new();
        tokio::select! {
            r = sock.recv_from(&mut buf) => {
                let Ok((n, from)) = r else { continue };
                let pkt = &buf[..n];
                let Some(inner) = inner.upgrade() else { return };
                if let Some(s) = sessions.get_mut(&from) {
                    if let Ok(rcv) = Receive::new(Protocol::Udp, from, local, pkt) {
                        if let Err(e) = s.rtc.handle_input(Input::Receive(Instant::now(), rcv)) {
                            debug(&format!("input from {from}: {e}"));
                        }
                        touched.push(from);
                    }
                } else if let Some(out) = stun_answer(pkt, from) {
                    door.stun.fetch_add(1, Ordering::Relaxed);
                    let _ = sock.try_send_to(&out, from);
                } else if let Some((ufrag, theirs)) = direct_ufrag(pkt) {
                    // A new browser. At most a few a second, and a door's share at once.
                    opened_recent.retain(|t| t.elapsed() < Duration::from_secs(1));
                    if sessions.len() >= MAX_SESSIONS || opened_recent.len() >= 8 || !inner.crew.load(Ordering::Relaxed) {
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
                    let Ok(c) = Candidate::host(local, "udp") else { continue };
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
                    let mut s = Session { rtc, ch: None, peer: None, parts: HashMap::new(), out: VecDeque::new(), waiting: HashMap::new(), next: 1, asks: Default::default(), born: Instant::now(), wake: Instant::now() };
                    if let Ok(rcv) = Receive::new(Protocol::Udp, from, local, pkt) {
                        let _ = s.rtc.handle_input(Input::Receive(Instant::now(), rcv));
                    }
                    sessions.insert(from, s);
                    door.opened.fetch_add(1, Ordering::Relaxed);
                    touched.push(from);
                }
            }
            c = rx.recv() => {
                match c {
                    None | Some(Cmd::Close) => break,
                    Some(Cmd::Send { to, id, bytes }) => {
                        if let Some(s) = sessions.get_mut(&to) {
                            s.out.extend(frame(id, &bytes));
                            touched.push(to);
                        }
                    }
                    Some(Cmd::Ask { tab, bytes, reply }) => {
                        let to = door.clients.lock().unwrap().get(&tab).copied();
                        if let Some(s) = to.and_then(|to| sessions.get_mut(&to).map(|s| (to, s))) {
                            let (to, s) = s;
                            let id = FROM_DOOR | s.next;
                            s.next = s.next.wrapping_add(1) & !FROM_DOOR;
                            if s.waiting.len() < 32 {
                                s.waiting.insert(id, reply);
                                s.out.extend(frame(id, &bytes));
                                touched.push(to);
                            }
                        }
                    }
                    Some(Cmd::Push(g)) => {
                        let Ok(b) = postcard::to_stdvec(&DoorCtl::Gossip(g)) else { continue };
                        for (a, s) in sessions.iter_mut() {
                            if s.peer.is_some() && s.out.len() < 256 {
                                s.out.extend(frame(0, &b));
                                touched.push(*a);
                            }
                        }
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
        for a in touched {
            let Some(s) = sessions.get_mut(&a) else { continue };
            drive(&inner, &door, &sock, a, s);
        }
        // Gone: closed, or never opened a channel within fifteen seconds.
        let before = sessions.len();
        sessions.retain(|a, s| {
            let keep = s.rtc.is_alive() && (s.ch.is_some() || s.born.elapsed() < Duration::from_secs(15));
            if !keep {
                if let Some(p) = s.peer {
                    let mut c = door.clients.lock().unwrap();
                    if c.get(&p) == Some(a) {
                        c.remove(&p);
                    }
                }
            }
            keep
        });
        if sessions.len() != before {
            door.sessions.store(sessions.len() as u64, Ordering::Relaxed);
        }
    }
    door.clients.lock().unwrap().clear();
}

fn drive(inner: &Arc<Inner>, door: &Arc<Door>, sock: &UdpSocket, at: SocketAddr, s: &mut Session) {
    // What waits to go out goes, as far as the channel takes it.
    if let Some(ch) = s.ch {
        while let Some(f) = s.out.front() {
            let Some(mut c) = s.rtc.channel(ch) else { break };
            match c.write(true, f) {
                Ok(true) => {
                    s.out.pop_front();
                }
                _ => break,
            }
        }
    }
    loop {
        match s.rtc.poll_output() {
            Ok(Output::Timeout(t)) => {
                s.wake = t;
                break;
            }
            Ok(Output::Transmit(t)) => {
                let _ = sock.try_send_to(&t.contents, t.destination);
            }
            Ok(Output::Event(e)) => match { debug(&format!("event {at}: {e:?}")); e } {
                Event::ChannelOpen(id, _) => {
                    s.ch = Some(id);
                    door.sessions.store(door.sessions.load(Ordering::Relaxed) + 1, Ordering::Relaxed);
                    if let Some(g) = door.note(&inner.secret, now_ms()) {
                        if let Ok(b) = postcard::to_stdvec(&g).and_then(|n| postcard::to_stdvec(&DoorCtl::Note(n))) {
                            s.out.extend(frame(0, &b));
                        }
                    }
                    if let Some(ch) = s.ch {
                        while let Some(f) = s.out.front() {
                            let Some(mut c) = s.rtc.channel(ch) else { break };
                            if !matches!(c.write(true, f), Ok(true)) {
                                break;
                            }
                            s.out.pop_front();
                        }
                    }
                }
                Event::ChannelData(d) => {
                    if d.data.len() < 5 {
                        continue;
                    }
                    let id = u32::from_be_bytes(d.data[..4].try_into().unwrap());
                    let last = d.data[4] == 1;
                    let total: usize = s.parts.values().map(|p| p.len()).sum();
                    if total + d.data.len() > MESSAGE_MAX || (!s.parts.contains_key(&id) && s.parts.len() >= 2 * ASKS_PER_TAB) {
                        s.rtc.disconnect();
                        break;
                    }
                    s.parts.entry(id).or_default().extend_from_slice(&d.data[5..]);
                    if !last {
                        continue;
                    }
                    let msg = s.parts.remove(&id).unwrap_or_default();
                    if id == 0 {
                        inner.door_ctl(door, at, s, &msg);
                    } else if id & FROM_DOOR != 0 {
                        if let Some(w) = s.waiting.remove(&id) {
                            let ans = match postcard::from_bytes::<Response>(&msg) {
                                Ok(Response::Hop(b)) => b,
                                _ => vec![],
                            };
                            let _ = w.send(ans);
                        }
                    } else {
                        // A request: answered in its own task, at most eight at a time per tab.
                        if s.asks.load(Ordering::Relaxed) >= ASKS_PER_TAB {
                            s.out.extend(frame(id, &postcard::to_stdvec(&Response::Hop(vec![])).unwrap_or_default()));
                            continue;
                        }
                        s.asks.fetch_add(1, Ordering::Relaxed);
                        let (i2, d2, asks) = (inner.clone(), door.clone(), s.asks.clone());
                        tokio::spawn(async move {
                            let ans = i2.door_request(msg).await;
                            d2.served.fetch_add(1, Ordering::Relaxed);
                            asks.fetch_sub(1, Ordering::Relaxed);
                            let _ = d2.cmd.send(Cmd::Send { to: at, id, bytes: ans });
                        });
                    }
                }
                Event::ChannelBufferedAmountLow(_) => {
                    if let Some(ch) = s.ch {
                        while let Some(f) = s.out.front() {
                            let Some(mut c) = s.rtc.channel(ch) else { break };
                            if !matches!(c.write(true, f), Ok(true)) {
                                break;
                            }
                            s.out.pop_front();
                        }
                    }
                }
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
}

fn debug(m: &str) {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if *ON.get_or_init(|| std::env::var("PYRATE_DOOR_DEBUG").is_ok()) {
        eprintln!("door: {m}");
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
