//! A door asks the home router for a port: UPnP-IGD first, then NAT-PMP, then PCP.
//!
//! Each asks the router on the local network for a UDP port on its outside address that comes
//! through to the door's own, for an hour at a time, renewed while the door is open. If none
//! answers, the radio is no door: an address nobody can reach is announced nowhere.
//!
//! No rights reserved.

use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::atomic::{AtomicU32, Ordering},
    time::Duration,
};

use tokio::net::UdpSocket;

use crate::say;

const LEASE: u32 = 3600;

/// How the last mapping was made: 0 none, 1 UPnP-IGD, 2 NAT-PMP, 3 PCP. For the status.
pub static HOW: AtomicU32 = AtomicU32::new(0);

/// Maps `port` (UDP) on the router, and keeps it mapped: the outside address, or None.
pub async fn map_udp(port: u16) -> Option<SocketAddr> {
    let got = map_once(port).await;
    if got.is_some() {
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs((LEASE / 2) as u64)).await;
                let _ = map_once(port).await;
            }
        });
    }
    got
}

async fn map_once(port: u16) -> Option<SocketAddr> {
    if let Some(a) = igd(port).await {
        HOW.store(1, Ordering::Relaxed);
        return Some(a);
    }
    let gw = gateway()?;
    if let Some(a) = nat_pmp(gw, port).await {
        HOW.store(2, Ordering::Relaxed);
        return Some(a);
    }
    if let Some(a) = pcp(gw, port).await {
        HOW.store(3, Ordering::Relaxed);
        return Some(a);
    }
    HOW.store(0, Ordering::Relaxed);
    None
}

/// This machine's address on the way to `to`, as the system would route a packet there.
fn local_ip_toward(to: IpAddr) -> Option<IpAddr> {
    let s = std::net::UdpSocket::bind(SocketAddr::from(([0, 0, 0, 0], 0))).ok()?;
    s.connect(SocketAddr::new(to, 9)).ok()?;
    Some(s.local_addr().ok()?.ip())
}

/// The default gateway: from the routing table where the system shows it, else the first
/// address of this machine's /24, which is where most home routers sit.
fn gateway() -> Option<Ipv4Addr> {
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

async fn igd(port: u16) -> Option<SocketAddr> {
    use igd_next::{PortMappingProtocol, SearchOptions};
    let opts = SearchOptions { timeout: Some(Duration::from_secs(3)), ..Default::default() };
    let gw = igd_next::aio::tokio::search_gateway(opts).await.ok()?;
    let local = local_ip_toward(gw.addr.ip())?;
    let ext = tokio::time::timeout(Duration::from_secs(3), gw.get_external_ip()).await.ok()?.ok()?;
    let r = tokio::time::timeout(Duration::from_secs(3), gw.add_port(PortMappingProtocol::UDP, port, SocketAddr::new(local, port), LEASE, "Pyrate Radio door")).await.ok()?;
    let ext_port = match r {
        Ok(()) => port,
        // The same outside port is taken: any free one will do.
        Err(_) => tokio::time::timeout(Duration::from_secs(3), gw.add_any_port(PortMappingProtocol::UDP, SocketAddr::new(local, port), LEASE, "Pyrate Radio door")).await.ok()?.ok()?,
    };
    let a = SocketAddr::new(ext, ext_port);
    say(&format!("door: UPnP-IGD mapped {a}"));
    public_ok(ext).then_some(a)
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

/// NAT-PMP (RFC 6886): the outside address, then a UDP mapping.
async fn nat_pmp(gw: Ipv4Addr, port: u16) -> Option<SocketAddr> {
    let r = ask(gw, 5351, &[0, 0], 12).await?;
    if r[0] != 0 || r[1] != 128 || r[2..4] != [0, 0] {
        return None;
    }
    let ext = Ipv4Addr::new(r[8], r[9], r[10], r[11]);
    let mut req = vec![0, 1, 0, 0];
    req.extend_from_slice(&port.to_be_bytes());
    req.extend_from_slice(&port.to_be_bytes());
    req.extend_from_slice(&LEASE.to_be_bytes());
    let r = ask(gw, 5351, &req, 16).await?;
    if r[0] != 0 || r[1] != 129 || r[2..4] != [0, 0] {
        return None;
    }
    let a = SocketAddr::new(IpAddr::V4(ext), u16::from_be_bytes([r[10], r[11]]));
    say(&format!("door: NAT-PMP mapped {a}"));
    public_ok(a.ip()).then_some(a)
}

/// PCP (RFC 6887): one MAP request for UDP.
async fn pcp(gw: Ipv4Addr, port: u16) -> Option<SocketAddr> {
    let IpAddr::V4(me) = local_ip_toward(IpAddr::V4(gw))? else { return None };
    let mapped = |v: Ipv4Addr| {
        let mut b = [0u8; 16];
        b[10] = 0xff;
        b[11] = 0xff;
        b[12..].copy_from_slice(&v.octets());
        b
    };
    let nonce: [u8; 12] = rand::random();
    let mut req = vec![2, 1, 0, 0];
    req.extend_from_slice(&LEASE.to_be_bytes());
    req.extend_from_slice(&mapped(me));
    req.extend_from_slice(&nonce);
    req.extend_from_slice(&[17, 0, 0, 0]);
    req.extend_from_slice(&port.to_be_bytes());
    req.extend_from_slice(&port.to_be_bytes());
    req.extend_from_slice(&mapped(Ipv4Addr::UNSPECIFIED));
    let r = ask(gw, 5351, &req, 60).await?;
    if r[0] != 2 || r[1] != 0x81 || r[3] != 0 || r[24..36] != nonce {
        return None;
    }
    let ext_port = u16::from_be_bytes([r[42], r[43]]);
    let ip = Ipv4Addr::new(r[56], r[57], r[58], r[59]);
    let a = SocketAddr::new(IpAddr::V4(ip), ext_port);
    say(&format!("door: PCP mapped {a}"));
    public_ok(a.ip()).then_some(a)
}
