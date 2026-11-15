//! A door asks the home router for a port: UPnP-IGD first, then NAT-PMP, then PCP.
//!
//! Each asks the router on the local network for a UDP port on its outside address that comes
//! through to the door's own. The port is held for as long as the router grants it (an hour by
//! UPnP-IGD, whatever NAT-PMP or PCP answer, a minute on some VPNs), renewed at half that while
//! the door is open and the router is the same one, and given back when the door closes. If none
//! answers, the radio is no door: an address nobody can reach is announced nowhere.
//!
//! No rights reserved.

use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, Ordering},
    },
    time::Duration,
};

use tokio::net::UdpSocket;

use crate::say;

/// What is asked for: an hour. NAT-PMP and PCP may grant less, and what they grant is kept.
const LEASE: u32 = 3600;

/// How the last mapping was made: 0 none, 1 UPnP-IGD, 2 NAT-PMP, 3 PCP. For the status.
pub static HOW: AtomicU32 = AtomicU32::new(0);

/// How a mapping was made, with what it takes to renew it and to give it back.
#[derive(Clone)]
enum Via {
    Igd { gw: igd_next::aio::Gateway<igd_next::aio::tokio::Tokio>, local: SocketAddr, ext_port: u16 },
    Pmp { gw: Ipv4Addr, me: IpAddr, ext_port: u16 },
    Pcp { gw: Ipv4Addr, me: Ipv4Addr, nonce: [u8; 12], ext: SocketAddr },
}

impl Via {
    fn router(&self) -> IpAddr {
        match self {
            Via::Igd { gw, .. } => gw.addr.ip(),
            Via::Pmp { gw, .. } | Via::Pcp { gw, .. } => IpAddr::V4(*gw),
        }
    }
}

/// A router's answer: the outside address, and for how long it holds.
struct Granted {
    outside: SocketAddr,
    lifetime: u32,
    via: Via,
}

/// When to ask again for a port granted for `lifetime` seconds, which is at half of it.
pub fn renew_after(lifetime: u32) -> Duration {
    Duration::from_secs((lifetime / 2).max(2) as u64)
}

/// A UDP port held open on the home router for the door, renewed until it is closed or the
/// network changes under it.
pub struct Mapping {
    pub outside: SocketAddr,
    port: u16,
    via: Arc<Mutex<Option<Via>>>,
    lost: Arc<AtomicBool>,
    renew: tokio::task::AbortHandle,
}

impl Mapping {
    /// Whether the port has lapsed, because the router stopped renewing it or this device is on
    /// another network now.
    pub fn lost(&self) -> bool {
        self.lost.load(Ordering::Relaxed)
    }

    /// Stops renewing and asks the router to drop the port, if it is still the router that
    /// gave it.
    pub async fn close(&self) {
        self.renew.abort();
        let via = self.via.lock().unwrap().take();
        let Some(via) = via else { return };
        if !same_network(&via) {
            return;
        }
        let _ = tokio::time::timeout(Duration::from_secs(4), give_back(&via, self.port)).await;
        say("door: the router's port given back");
    }
}

impl Drop for Mapping {
    fn drop(&mut self) {
        self.renew.abort();
    }
}

/// Maps `port` (UDP) on the router, and keeps it mapped until the handle is closed.
pub async fn map_udp(port: u16) -> Option<Mapping> {
    let route = gateway();
    let g = map_once(port).await?;
    let outside = g.outside;
    let via = Arc::new(Mutex::new(Some(g.via.clone())));
    let lost = Arc::new(AtomicBool::new(false));
    let (v2, l2) = (via.clone(), lost.clone());
    let renew = tokio::spawn(async move {
        let mut lifetime = g.lifetime;
        let mut left = Duration::from_secs(lifetime as u64);
        let mut wait = renew_after(lifetime);
        loop {
            tokio::time::sleep(wait).await;
            left = left.saturating_sub(wait);
            let Some(via) = v2.lock().unwrap().clone() else { return };
            // Another network: its router is never asked about this one's port.
            if gateway() != route || !same_network(&via) {
                l2.store(true, Ordering::Relaxed);
                return;
            }
            match again(&via, port).await {
                Some(n) if n.outside == outside => {
                    lifetime = n.lifetime;
                    left = Duration::from_secs(lifetime as u64);
                    wait = renew_after(lifetime);
                    *v2.lock().unwrap() = Some(n.via);
                }
                // A router that gives another address now: the door's address is gone.
                Some(_) => {
                    l2.store(true, Ordering::Relaxed);
                    return;
                }
                // No answer: once more in a few seconds, while the port still holds.
                None if left > Duration::from_secs(6) => wait = Duration::from_secs(5),
                None => {
                    l2.store(true, Ordering::Relaxed);
                    return;
                }
            }
        }
    })
    .abort_handle();
    Some(Mapping { outside, port, via, lost, renew })
}

/// Whether this device still reaches the router that gave a mapping from the same address.
fn same_network(via: &Via) -> bool {
    let router = via.router();
    match via {
        Via::Igd { local, .. } => local_ip_toward(router) == Some(local.ip()),
        Via::Pcp { me, .. } => local_ip_toward(router) == Some(IpAddr::V4(*me)),
        Via::Pmp { me, .. } => local_ip_toward(router) == Some(*me),
    }
}

async fn map_once(port: u16) -> Option<Granted> {
    if let Some(g) = igd(port).await {
        HOW.store(1, Ordering::Relaxed);
        return Some(g);
    }
    let gw = gateway()?;
    if let Some(g) = nat_pmp(gw, port, port).await {
        HOW.store(2, Ordering::Relaxed);
        return Some(g);
    }
    if let Some(g) = pcp(gw, port, None, rand::random(), LEASE).await {
        HOW.store(3, Ordering::Relaxed);
        return Some(g);
    }
    HOW.store(0, Ordering::Relaxed);
    None
}

/// The same mapping asked for again, from the router that gave it.
async fn again(via: &Via, port: u16) -> Option<Granted> {
    match via {
        Via::Igd { gw, local, ext_port } => {
            use igd_next::PortMappingProtocol;
            tokio::time::timeout(Duration::from_secs(3), gw.add_port(PortMappingProtocol::UDP, *ext_port, *local, LEASE, "")).await.ok()?.ok()?;
            let ext = tokio::time::timeout(Duration::from_secs(3), gw.get_external_ip()).await.ok()?.ok()?;
            Some(Granted { outside: SocketAddr::new(ext, *ext_port), lifetime: LEASE, via: via.clone() })
        }
        Via::Pmp { gw, ext_port, .. } => nat_pmp(*gw, port, *ext_port).await,
        Via::Pcp { gw, nonce, ext, .. } => pcp(*gw, port, Some(*ext), *nonce, LEASE).await,
    }
}

/// Asks the router to drop the mapping: DeletePortMapping, or NAT-PMP and PCP with no lifetime.
async fn give_back(via: &Via, port: u16) {
    match via {
        Via::Igd { gw, ext_port, .. } => {
            let _ = gw.remove_port(igd_next::PortMappingProtocol::UDP, *ext_port).await;
        }
        Via::Pmp { gw, .. } => {
            let _ = ask(*gw, 5351, &pmp_map(port, 0, 0), 16).await;
        }
        Via::Pcp { gw, me, nonce, ext } => {
            let _ = ask(*gw, 5351, &pcp_map(*me, port, Some(*ext), nonce, 0), 60).await;
        }
    }
}

/// This machine's address on the way to `to`, as the system would route a packet there.
fn local_ip_toward(to: IpAddr) -> Option<IpAddr> {
    let s = std::net::UdpSocket::bind(SocketAddr::from(([0, 0, 0, 0], 0))).ok()?;
    s.connect(SocketAddr::new(to, 9)).ok()?;
    Some(s.local_addr().ok()?.ip())
}

/// The default gateway: from the routing table where the system shows it, else the first
/// address of this machine's /24, which is where most home routers sit.
pub(crate) fn gateway() -> Option<Ipv4Addr> {
    if let Ok(t) = std::fs::read_to_string("/proc/net/route") {
        for line in t.lines().skip(1) {
            let f: Vec<&str> = line.split_whitespace().collect();
            if f.len() > 2 && f[1] == "00000000" {
                if let Ok(g) = u32::from_str_radix(f[2], 16) {
                    return Some(Ipv4Addr::from(g.to_le_bytes()));
                }
            }
        }
    }
    match local_ip_toward(IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1)))? {
        IpAddr::V4(v) if v.is_private() => {
            let o = v.octets();
            Some(Ipv4Addr::new(o[0], o[1], o[2], 1))
        }
        _ => None,
    }
}

fn public_ok(ip: IpAddr) -> bool {
    crate::net::relay_public(&format!("http://{}/", SocketAddr::new(ip, 1)))
}

async fn igd(port: u16) -> Option<Granted> {
    use igd_next::{PortMappingProtocol, SearchOptions};
    let opts = SearchOptions { timeout: Some(Duration::from_secs(3)), ..Default::default() };
    let gw = igd_next::aio::tokio::search_gateway(opts).await.ok()?;
    let local = SocketAddr::new(local_ip_toward(gw.addr.ip())?, port);
    let ext = tokio::time::timeout(Duration::from_secs(3), gw.get_external_ip()).await.ok()?.ok()?;
    // No description: a router lists its mappings to anyone on the network, and some report
    // them to the provider.
    let r = tokio::time::timeout(Duration::from_secs(3), gw.add_port(PortMappingProtocol::UDP, port, local, LEASE, "")).await.ok()?;
    let ext_port = match r {
        Ok(()) => port,
        // The same outside port is taken: any free one will do.
        Err(_) => tokio::time::timeout(Duration::from_secs(3), gw.add_any_port(PortMappingProtocol::UDP, local, LEASE, "")).await.ok()?.ok()?,
    };
    let a = SocketAddr::new(ext, ext_port);
    say(&format!("door: UPnP-IGD mapped {a}"));
    let via = Via::Igd { gw, local, ext_port };
    if !public_ok(ext) {
        give_back(&via, port).await;
        return None;
    }
    Some(Granted { outside: a, lifetime: LEASE, via })
}

async fn ask(gw: Ipv4Addr, port: u16, req: &[u8], want: usize) -> Option<Vec<u8>> {
    let s = UdpSocket::bind(SocketAddr::from(([0, 0, 0, 0], 0))).await.ok()?;
    s.connect(SocketAddr::new(IpAddr::V4(gw), port)).await.ok()?;
    let mut buf = [0u8; 1100];
    // RFC 6886's schedule, cut short: 250 ms, then twice as long, three tries.
    for i in 0..3 {
        s.send(req).await.ok()?;
        if let Ok(Ok(n)) = tokio::time::timeout(Duration::from_millis(250 << i), s.recv(&mut buf)).await {
            if n >= want {
                return Some(buf[..n].to_vec());
            }
        }
    }
    None
}

/// The lifetime a NAT-PMP mapping answer grants, in seconds (RFC 6886 3.3).
fn pmp_lifetime(r: &[u8]) -> u32 {
    u32::from_be_bytes([r[12], r[13], r[14], r[15]])
}

/// The lifetime a PCP answer grants, in seconds (RFC 6887 7.2).
fn pcp_lifetime(r: &[u8]) -> u32 {
    u32::from_be_bytes([r[4], r[5], r[6], r[7]])
}

/// A NAT-PMP UDP mapping request (RFC 6886 3.3): the inside port, the outside port wished for,
/// and the lifetime asked (0 with an outside port of 0 gives the mapping back).
fn pmp_map(port: u16, ext: u16, lifetime: u32) -> Vec<u8> {
    let mut req = vec![0, 1, 0, 0];
    req.extend_from_slice(&port.to_be_bytes());
    req.extend_from_slice(&ext.to_be_bytes());
    req.extend_from_slice(&lifetime.to_be_bytes());
    req
}

/// NAT-PMP (RFC 6886): the outside address, then a UDP mapping, `ext` the outside port wished for.
async fn nat_pmp(gw: Ipv4Addr, port: u16, ext: u16) -> Option<Granted> {
    let r = ask(gw, 5351, &[0, 0], 12).await?;
    if r[0] != 0 || r[1] != 128 || r[2..4] != [0, 0] {
        return None;
    }
    let ip = Ipv4Addr::new(r[8], r[9], r[10], r[11]);
    let r = ask(gw, 5351, &pmp_map(port, ext, LEASE), 16).await?;
    if r[0] != 0 || r[1] != 129 || r[2..4] != [0, 0] {
        return None;
    }
    let ext_port = u16::from_be_bytes([r[10], r[11]]);
    let lifetime = pmp_lifetime(&r);
    let a = SocketAddr::new(IpAddr::V4(ip), ext_port);
    let via = Via::Pmp { gw, me: local_ip_toward(IpAddr::V4(gw))?, ext_port };
    if lifetime == 0 || !public_ok(a.ip()) {
        give_back(&via, port).await;
        return None;
    }
    say(&format!("door: NAT-PMP mapped {a} for {lifetime} s"));
    Some(Granted { outside: a, lifetime, via })
}

/// A PCP MAP request for UDP (RFC 6887 11.1): from `me`, for `port`, the outside address
/// wished for, and `lifetime` seconds (0 gives the mapping back).
fn pcp_map(me: Ipv4Addr, port: u16, wish: Option<SocketAddr>, nonce: &[u8; 12], lifetime: u32) -> Vec<u8> {
    let mapped = |v: Ipv4Addr| {
        let mut b = [0u8; 16];
        b[10] = 0xff;
        b[11] = 0xff;
        b[12..].copy_from_slice(&v.octets());
        b
    };
    let (wish_port, wish_ip) = match wish {
        Some(SocketAddr::V4(w)) => (w.port(), *w.ip()),
        _ => (port, Ipv4Addr::UNSPECIFIED),
    };
    let mut req = vec![2, 1, 0, 0];
    req.extend_from_slice(&lifetime.to_be_bytes());
    req.extend_from_slice(&mapped(me));
    req.extend_from_slice(nonce);
    req.extend_from_slice(&[17, 0, 0, 0]);
    req.extend_from_slice(&port.to_be_bytes());
    req.extend_from_slice(&wish_port.to_be_bytes());
    req.extend_from_slice(&mapped(wish_ip));
    req
}

/// PCP (RFC 6887): one MAP request for UDP.
async fn pcp(gw: Ipv4Addr, port: u16, wish: Option<SocketAddr>, nonce: [u8; 12], lifetime: u32) -> Option<Granted> {
    let IpAddr::V4(me) = local_ip_toward(IpAddr::V4(gw))? else { return None };
    let r = ask(gw, 5351, &pcp_map(me, port, wish, &nonce, lifetime), 60).await?;
    if r[0] != 2 || r[1] != 0x81 || r[3] != 0 || r[24..36] != nonce {
        return None;
    }
    let ext_port = u16::from_be_bytes([r[42], r[43]]);
    let ip = Ipv4Addr::new(r[56], r[57], r[58], r[59]);
    let granted = pcp_lifetime(&r);
    let a = SocketAddr::new(IpAddr::V4(ip), ext_port);
    let via = Via::Pcp { gw, me, nonce, ext: a };
    if granted == 0 || !public_ok(a.ip()) {
        give_back(&via, port).await;
        return None;
    }
    say(&format!("door: PCP mapped {a} for {granted} s"));
    Some(Granted { outside: a, lifetime: granted, via })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_port_is_asked_for_again_at_half_its_lifetime() {
        // A VPN that grants a minute is asked again every thirty seconds.
        assert_eq!(renew_after(60), Duration::from_secs(30));
        assert_eq!(renew_after(3600), Duration::from_secs(1800));
        assert_eq!(renew_after(7), Duration::from_secs(3));
        assert_eq!(renew_after(1), Duration::from_secs(2), "never a busy loop");
    }

    #[test]
    fn a_closed_door_gives_its_port_back() {
        // NAT-PMP: the same inside port, no outside port, no lifetime.
        let give = pmp_map(41_000, 0, 0);
        assert_eq!(give, [0, 1, 0, 0, 0xa0, 0x28, 0, 0, 0, 0, 0, 0]);
        assert_eq!(&pmp_map(41_000, 41_000, LEASE)[8..], &LEASE.to_be_bytes());
        // PCP: the mapping's own nonce and outside address, with no lifetime.
        let nonce = [9u8; 12];
        let ext: SocketAddr = "198.51.100.20:41000".parse().unwrap();
        let back = pcp_map("192.168.1.20".parse().unwrap(), 41_000, Some(ext), &nonce, 0);
        assert_eq!(back.len(), 60);
        assert_eq!(&back[4..8], &[0, 0, 0, 0], "no lifetime");
        assert_eq!(&back[24..36], &nonce, "the nonce it was made under");
        assert_eq!(&back[42..44], &41_000u16.to_be_bytes());
        assert_eq!(&back[56..60], &[198, 51, 100, 20]);
    }

    #[test]
    fn the_granted_lifetime_is_read_from_the_answer() {
        // NAT-PMP: version, op 129, result 0, epoch, internal 41000, external 41000, 60 s.
        let mut pmp = vec![0, 129, 0, 0, 0, 0, 0, 9];
        pmp.extend_from_slice(&41000u16.to_be_bytes());
        pmp.extend_from_slice(&41000u16.to_be_bytes());
        pmp.extend_from_slice(&60u32.to_be_bytes());
        assert_eq!(pmp_lifetime(&pmp), 60);
        // PCP: version 2, MAP answer, result 0, lifetime 120.
        let mut p = vec![2, 0x81, 0, 0];
        p.extend_from_slice(&120u32.to_be_bytes());
        p.resize(60, 0);
        assert_eq!(pcp_lifetime(&p), 120);
    }
}
