//! Radios on the same Wi-Fi find each other with no internet at all.
//!
//! When the keeper turns it on, a node says on the local network, every fifteen seconds, that
//! it is here and where on that network it can be reached, and listens for others saying the
//! same. Any it hears go into its address book and into the gossip, so a room, a building or a
//! phone's hotspot keeps playing every station its phones carry when the wider internet is cut.
//! It is off by default: anyone else on the same network can hear that a radio is there. A
//! node running dark (through Tor) never does it, since it has no direct paths to offer.
//!
//! No rights reserved.

use std::{net::{Ipv4Addr, SocketAddr, UdpSocket}, sync::Arc, time::Duration};

use iroh::EndpointId;
use serde::{Deserialize, Serialize};

use crate::{Inner, say};

const PORT: u16 = 47823;
const MAGIC: &[u8] = b"pyrate/lan/v2";

/// A node's word that it is here: signed with its own key and dated, so no one on the network
/// can speak for another node or replay an old beacon.
#[derive(Serialize, Deserialize)]
struct Beacon {
    node: [u8; 32],
    addrs: Vec<SocketAddr>,
    at_ms: u64,
    sig: Vec<u8>,
}

impl Beacon {
    /// The node that signed this beacon, if it is the node it names and was sent within two
    /// minutes: no one can name another node or replay an old beacon. The addresses are then
    /// held to the one the beacon came from, so no one can point a node at an address it does
    /// not hold.
    fn signer(&self, now: u64) -> Option<EndpointId> {
        let id = EndpointId::from_bytes(&self.node).ok()?;
        let sig = <[u8; 64]>::try_from(self.sig.as_slice()).ok()?;
        (now.abs_diff(self.at_ms) <= 120_000 && id.verify(&Self::message(&self.node, &self.addrs, self.at_ms), &iroh::Signature::from_bytes(&sig)).is_ok()).then_some(id)
    }

    fn message(node: &[u8; 32], addrs: &[SocketAddr], at_ms: u64) -> Vec<u8> {
        let mut m = MAGIC.to_vec();
        m.extend_from_slice(&postcard::to_stdvec(&(node, addrs, at_ms)).unwrap_or_default());
        m
    }
}

/// Beacons taken lately, by address and node: one every ten seconds from each node, eight nodes
/// from any one address, a few hundred in all.
fn fresh_sender(seen: &mut std::collections::HashMap<(std::net::IpAddr, [u8; 32]), u64>, from: std::net::IpAddr, node: [u8; 32], now: u64) -> bool {
    seen.retain(|_, t| now.saturating_sub(*t) < 60_000);
    let key = (from, node);
    if let Some(t) = seen.get(&key) {
        if now.saturating_sub(*t) < 10_000 {
            return false;
        }
    } else if seen.len() >= 256 || seen.keys().filter(|(ip, _)| *ip == from).count() >= 8 {
        return false;
    }
    seen.insert(key, now);
    true
}

/// An address only the local network can reach: the only kind a beacon carries.
fn local(a: &SocketAddr) -> bool {
    // PYRATE_LAN_ANY: any address, for a test network on addresses no LAN would use.
    if std::env::var_os("PYRATE_LAN_ANY").is_some() {
        return true;
    }
    match a {
        SocketAddr::V4(v) => v.ip().is_private() || v.ip().is_link_local(),
        SocketAddr::V6(v) => (v.ip().segments()[0] & 0xffc0) == 0xfe80 || (v.ip().segments()[0] & 0xfe00) == 0xfc00,
    }
}

impl Inner {
    pub(crate) fn lan_on(&self) -> bool {
        !self.dark && self.hold.read_note(&self.cfg.dir, "lan", None).is_some_and(|b| b == b"1")
    }

    /// Starts the beacon and the listener, once.
    pub(crate) fn lan_start(self: &Arc<Self>) {
        if !self.lan_on() || self.lan_running.swap(true, std::sync::atomic::Ordering::Relaxed) {
            return;
        }
        let rx = {
            let s = socket2::Socket::new(socket2::Domain::IPV4, socket2::Type::DGRAM, Some(socket2::Protocol::UDP)).ok();
            s.and_then(|s| {
                let _ = s.set_reuse_address(true);
                #[cfg(unix)]
                let _ = s.set_reuse_port(true);
                s.set_broadcast(true).ok()?;
                s.bind(&SocketAddr::from((Ipv4Addr::UNSPECIFIED, PORT)).into()).ok()?;
                let u: UdpSocket = s.into();
                u.set_nonblocking(true).ok()?;
                tokio::net::UdpSocket::from_std(u).ok()
            })
        };
        let me = self.clone();
        tokio::spawn(async move {
            let Some(sock) = rx else {
                say("nearby radios: the local port is taken, so this node only listens for the internet");
                return;
            };
            say("listening for radios on this network");
            let mut buf = vec![0u8; 2048];
            let mut seen = std::collections::HashMap::new();
            let mut tick = tokio::time::interval(Duration::from_secs(15));
            loop {
                if !me.lan_on() {
                    me.lan_running.store(false, std::sync::atomic::Ordering::Relaxed);
                    return;
                }
                tokio::select! {
                    _ = tick.tick() => {
                        let addrs: Vec<SocketAddr> = me.endpoint.addr().ip_addrs().copied().filter(local).take(8).collect();
                        if addrs.is_empty() {
                            continue;
                        }
                        let (node, at_ms) = (*me.endpoint.id().as_bytes(), crate::now_ms());
                        let sig = me.endpoint.secret_key().sign(&Beacon::message(&node, &addrs, at_ms)).to_bytes().to_vec();
                        let b = Beacon { node, addrs, at_ms, sig };
                        if let Ok(body) = postcard::to_stdvec(&b) {
                            let msg = [MAGIC, &body].concat();
                            let _ = sock.send_to(&msg, (Ipv4Addr::BROADCAST, PORT)).await;
                        }
                    }
                    got = sock.recv_from(&mut buf) => {
                        let Ok((n, from)) = got else { continue };
                        let Some(body) = buf[..n].strip_prefix(MAGIC) else { continue };
                        let Ok(b) = postcard::from_bytes::<Beacon>(body) else { continue };
                        let now = crate::now_ms();
                        let Some(id) = b.signer(now) else { continue };
                        if id == me.endpoint.id() {
                            continue;
                        }
                        if !fresh_sender(&mut seen, from.ip(), b.node, now) {
                            continue;
                        }
                        let addrs: Vec<SocketAddr> = b.addrs.into_iter().filter(|a| a.ip() == from.ip()).filter(local).take(8).collect();
                        if addrs.is_empty() {
                            continue;
                        }
                        // A node met here is like any other: whatever it says of a station is
                        // checked against that station's key before anything is believed.
                        me.book.add_endpoint_info(iroh::EndpointAddr { id, addrs: addrs.into_iter().map(iroh::TransportAddr::Ip).collect() });
                        let tx = me.gossip_tx.lock().unwrap().clone();
                        if let Some(tx) = tx {
                            let _ = tx.join_peers(vec![id]).await;
                        }
                    }
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn connections_offer_the_post_quantum_exchange_first() {
        let p = rustls::crypto::aws_lc_rs::default_provider();
        assert_eq!(p.kx_groups[0].name(), rustls::NamedGroup::X25519MLKEM768);
        assert!(p.kx_groups.iter().any(|g| g.name() == rustls::NamedGroup::X25519), "and X25519 for peers without it");
    }

    #[test]
    fn a_beacon_carries_only_local_addresses() {
        assert!(super::local(&"192.168.1.9:4000".parse().unwrap()));
        assert!(super::local(&"[fe80::1]:4000".parse().unwrap()));
        assert!(!super::local(&"8.8.8.8:4000".parse().unwrap()));
    }
}

#[cfg(test)]
mod beacon_tests {
    use super::*;

    fn beacon(k: &iroh::SecretKey, addrs: Vec<SocketAddr>, at_ms: u64) -> Beacon {
        let node = *k.public().as_bytes();
        let sig = k.sign(&Beacon::message(&node, &addrs, at_ms)).to_bytes().to_vec();
        Beacon { node, addrs, at_ms, sig }
    }

    #[test]
    fn beacons_are_signed_dated_and_rate_limited() {
        let k = iroh::SecretKey::generate();
        let a: SocketAddr = "192.168.1.5:4000".parse().unwrap();
        let now = 10_000_000;
        assert_eq!(beacon(&k, vec![a], now).signer(now), Some(k.public()));
        // Too old, or naming another node, or with its addresses changed: refused.
        assert!(beacon(&k, vec![a], now - 200_000).signer(now).is_none());
        let other = iroh::SecretKey::generate();
        let mut forged = beacon(&k, vec![a], now);
        forged.node = *other.public().as_bytes();
        assert!(forged.signer(now).is_none());
        let mut moved = beacon(&k, vec![a], now);
        moved.addrs = vec!["192.168.1.9:4000".parse().unwrap()];
        assert!(moved.signer(now).is_none());
        let mut seen = std::collections::HashMap::new();
        let ip = a.ip();
        assert!(fresh_sender(&mut seen, ip, [1; 32], now));
        assert!(!fresh_sender(&mut seen, ip, [1; 32], now + 5_000));
        assert!(fresh_sender(&mut seen, ip, [2; 32], now + 5_000), "another node at the same address");
        assert!(fresh_sender(&mut seen, ip, [1; 32], now + 11_000));
        for i in 3..20u8 {
            fresh_sender(&mut seen, ip, [i; 32], now + 12_000);
        }
        assert_eq!(seen.len(), 8, "no more than eight nodes from one address");
    }
}
