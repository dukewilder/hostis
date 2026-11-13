//! The deck of an app node: the station it plays, served to the crew while it plays, as a
//! listener's tab serves its own. The deck has a key made for it alone and an endpoint of its
//! own with no direct paths at all: it reaches the network only through its relay, and nobody
//! can reach it any other way. So its way in, the one radio that may ask it anything, sees a
//! relay and never this device's address. The node's own id is never named: the lead names the
//! deck's key. Only with "Carry the radio while you listen" on, on a connection that may carry
//! others' traffic (on a phone, Wi-Fi), never while the node runs dark, never for its own
//! station or a pocket's. A phone passes on no more than it listens to; a computer six times.
use std::{
    collections::HashSet,
    sync::{Arc, Weak, atomic::Ordering::Relaxed},
    time::Duration,
};

use iroh::{
    Endpoint, EndpointId,
    endpoint::{Connection, presets},
    protocol::{AcceptError, ProtocolHandler, Router},
};
use pyrate_core::StationId;

use crate::{
    Inner,
    hop::{Ask, Hop},
    net::{ALPN, Gossip, Request, Response},
    now_ms,
};

/// Its own listening counted as at least 96 kbit/s, a minute of it.
const BUDGET_FLOOR: u64 = 720 * 1024;
/// A page of a log along a route.
const PAGE: usize = 256 * 1024;

pub(crate) struct Deck {
    pub(crate) station: StationId,
    seed: [u8; 32],
    pub(crate) id: [u8; 32],
    ep: Endpoint,
    _router: Router,
    pub(crate) intro: Option<[u8; 32]>,
    lead: Vec<u8>,
    relay: String,
    said: u64,
    /// The sealed names of the station's chunks this node holds: the deck serves only those.
    names: HashSet<[u8; 32]>,
    names_at: u64,
    /// Bytes passed on this minute, and the minute.
    minute: (u64, u64),
}

#[derive(Clone)]
struct DeckProto {
    inner: Weak<Inner>,
    id: [u8; 32],
}

impl std::fmt::Debug for DeckProto {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("DeckProto")
    }
}

impl ProtocolHandler for DeckProto {
    async fn accept(&self, conn: Connection) -> Result<(), AcceptError> {
        let from = *conn.remote_id().as_bytes();
        // Two asks at a time on one connection at most: the way in has no reason for more.
        let mut n = 0;
        while let Ok((mut send, mut recv)) = conn.accept_bi().await {
            n += 1;
            if n > 4_096 {
                break;
            }
            let Ok(Ok(bytes)) = tokio::time::timeout(Duration::from_secs(10), recv.read_to_end(1 << 20)).await else { continue };
            let Some(inner) = self.inner.upgrade() else { break };
            let out = match postcard::from_bytes::<Request>(&bytes) {
                Ok(Request::Hop(b)) => inner.deck_answer(self.id, from, &b).await,
                _ => vec![],
            };
            let _ = send.write_all(&postcard::to_stdvec(&Response::Hop(out)).unwrap_or_default()).await;
            let _ = send.finish();
        }
        Ok(())
    }
}

impl Inner {
    /// Whether this node has a deck to keep now, and for which station.
    fn deck_wanted(&self) -> Option<StationId> {
        if !self.crew.load(Relaxed) || !self.relay_ok.load(Relaxed) || self.dark {
            return None;
        }
        // While tuned, through a stall too: a deck that closed at every gap in the sound would
        // come back under a new key each time, and every listener's note of it would go stale.
        let st = self.tuned.lock().unwrap().as_ref().map(|t| t.station)?;
        // A node built without playback plays nothing, and keeps no deck.
        #[cfg(not(feature = "playback"))]
        return None;
        #[cfg(feature = "playback")]
        if self.np.lock().unwrap()["station"].as_str() != Some(st.to_hex().as_str()) {
            return None;
        }
        if self.pockets.lock().unwrap().contains_key(&st) || self.store.lock().unwrap().own().contains(&st) {
            return None;
        }
        Some(st)
    }

    /// Every five seconds: opens the deck of what this node plays, closes it when it stops or
    /// the switch goes off, finds it a way in and says so every two minutes.
    pub(crate) async fn deck_tick(self: &Arc<Self>) {
        let want = self.deck_wanted();
        let old = {
            let mut d = self.deck.lock().unwrap();
            if d.as_ref().is_some_and(|d| Some(d.station) != want) { d.take() } else { None }
        };
        if let Some(old) = old {
            old.ep.close().await;
        }
        let Some(station) = want else { return };
        if self.deck.lock().unwrap().is_none() {
            let Some(relay) = self.endpoint.addr().relay_urls().next().cloned() else { return };
            let seed: [u8; 32] = rand::random();
            let id = pyrate_core::StationKey::from_seed(seed).id().0;
            // No IP transport at all: the deck is reached through its relay and nothing else,
            // and it reaches nobody any other way, so no path ever shows its address.
            let mut b = Endpoint::builder(presets::Minimal)
                .crypto_provider(Arc::new(rustls::crypto::aws_lc_rs::default_provider()))
                .secret_key(iroh::SecretKey::from_bytes(&seed))
                .relay_mode(iroh::RelayMode::Custom(iroh::RelayMap::from_iter([relay.clone()])))
                .clear_ip_transports();
            // Through the same proxy as the node's own relay connections, where there is one.
            if std::env::var_os("HTTPS_PROXY").is_some() && !crate::net::LOCAL_RELAYS_OK.load(Relaxed) {
                b = b.proxy_from_env().ca_tls_config(iroh::tls::CaTlsConfig::system());
            }
            let ep = match b.bind().await {
                Ok(ep) => ep,
                Err(_) => return,
            };
            let router = Router::builder(ep.clone()).accept(ALPN, DeckProto { inner: Arc::downgrade(self), id }).spawn();
            *self.deck.lock().unwrap() = Some(Deck { station, seed, id, ep, _router: router, intro: None, lead: vec![], relay: relay.to_string(), said: 0, names: HashSet::new(), names_at: 0, minute: (0, 0) });
        }
        let now = now_ms();
        // The way in: a radio of the crew heard from lately, never this node or its guards.
        let (intro, said, relay, id, seed) = {
            let d = self.deck.lock().unwrap();
            let Some(d) = d.as_ref() else { return };
            (d.intro, d.said, d.relay.clone(), d.id, d.seed)
        };
        let fresh = intro.is_some_and(|x| self.crew_radios.lock().unwrap().get(&x).is_some_and(|(_, t)| now.saturating_sub(*t) < crate::hop::CREW_FRESH_MS));
        let intro = if fresh { intro } else { self.middle(&[]).map(|m| *m.as_bytes()) };
        let Some(intro) = intro else { return };
        if fresh && now.saturating_sub(said) < 120_000 {
            self.deck_names(now);
            return;
        }
        let lead = if fresh { self.deck.lock().unwrap().as_ref().map(|d| d.lead.clone()).unwrap_or_default() } else { pyrate_core::route::make_lead(&intro, &pyrate_core::route::Lead { to: id, relay }).unwrap_or_default() };
        if lead.is_empty() {
            return;
        }
        if let Some(d) = self.deck.lock().unwrap().as_mut() {
            d.intro = Some(intro);
            d.lead = lead.clone();
            d.said = now;
        }
        self.deck_names(now);
        let head = self.store.lock().unwrap().logs.get(&station).map(|a| a.len()).unwrap_or(0);
        self.broadcast(&Gossip::deck(&seed, station, head, intro, lead, now)).await;
    }

    /// The sealed names of what this node holds of the deck's station, every half minute.
    fn deck_names(&self, now: u64) {
        let st = {
            let d = self.deck.lock().unwrap();
            match d.as_ref() {
                Some(d) if now.saturating_sub(d.names_at) >= 30_000 => d.station,
                _ => return,
            }
        };
        let names: HashSet<[u8; 32]> = self.store.lock().unwrap().chunks_of(&st).iter().map(pyrate_core::seal::name).collect();
        if let Some(d) = self.deck.lock().unwrap().as_mut() {
            d.names = names;
            d.names_at = now;
        }
    }

    /// Takes `n` bytes from this minute's budget for the deck, if they fit.
    fn deck_spend(&self, n: usize) -> bool {
        let factor = if self.cfg.private { 1 } else { 6 };
        let minute = now_ms() / 60_000;
        let mut d = self.deck.lock().unwrap();
        let Some(d) = d.as_mut() else { return false };
        if d.minute.0 != minute {
            d.minute = (minute, 0);
        }
        if d.minute.1 + n as u64 > factor * BUDGET_FLOOR {
            return false;
        }
        d.minute.1 += n as u64;
        true
    }

    /// An ask that reached the deck: answered only with the switch on, only when it came from
    /// the deck's own way in, and only within the budget. Pages of the station's log, and the
    /// chunks of it this node holds; nothing else, so a guess at what else it holds goes unanswered.
    async fn deck_answer(&self, deck: [u8; 32], from: [u8; 32], bytes: &[u8]) -> Vec<u8> {
        let (seed, station, intro) = {
            let d = self.deck.lock().unwrap();
            match d.as_ref() {
                Some(d) if d.id == deck => (d.seed, d.station, d.intro),
                _ => return vec![],
            }
        };
        if !pyrate_core::route::deck_answers(self.crew.load(Relaxed) && self.relay_ok.load(Relaxed), &from, intro.as_ref()) {
            return vec![];
        }
        let Ok(Hop::Sealed { eph, ct }) = postcard::from_bytes::<Hop>(bytes) else { return vec![] };
        let Some((plain, c)) = pyrate_core::route::open_sealed(&seed, &deck, &eph, &ct) else { return vec![] };
        let resp = match postcard::from_bytes::<Ask>(&plain) {
            Ok(Ask::Log { station: st, from }) if st == station => match self.serve(Request::Log { station, from }) {
                Response::Entries(mut list) => {
                    let mut sum = 0;
                    let keep = list.iter().take_while(|b| {
                        sum += b.len();
                        sum <= PAGE || sum == b.len()
                    }).count();
                    list.truncate(keep);
                    Response::Entries(list)
                }
                _ => Response::Entries(vec![]),
            },
            Ok(Ask::Sealed { station: st, name }) if st == station => {
                let held = self.deck.lock().unwrap().as_ref().is_some_and(|d| d.names.contains(&name));
                Response::Sealed(if held { self.store.lock().unwrap().get_sealed(&name) } else { None })
            }
            Ok(Ask::Log { .. }) => Response::Entries(vec![]),
            _ => Response::Sealed(None),
        };
        let out = postcard::to_stdvec(&resp).unwrap_or_default();
        if !self.deck_spend(out.len()) {
            return vec![];
        }
        if matches!(resp, Response::Entries(ref l) if !l.is_empty()) || matches!(resp, Response::Sealed(Some(_))) {
            self.served_bytes.fetch_add(out.len() as u64, Relaxed);
        }
        pyrate_core::route::seal_answer(&c, &out)
    }

    /// For the deck test: as the way in of a deck of `station`, asks it for the first page of its
    /// log through the lead, and says which paths the connection to it runs over.
    pub(crate) async fn deck_probe(self: &Arc<Self>, station: StationId) -> Option<(usize, Vec<bool>)> {
        let me = *self.endpoint.id().as_bytes();
        let note = self.log.lock().unwrap().get(&station)?.decks.iter().find(|(_, n)| n.intro == me).map(|(d, n)| (*d, n.clone()))?;
        let lead = pyrate_core::route::open_lead(&self.secret, &me, &note.1.lead)?;
        let to = EndpointId::from_bytes(&lead.to).ok()?;
        let url: iroh::RelayUrl = lead.relay.parse().ok()?;
        self.book.add_endpoint_info(iroh::EndpointAddr { id: to, addrs: [iroh::TransportAddr::Relay(url)].into_iter().collect() });
        let (eph, ct, c) = pyrate_core::route::seal_to(&note.0, &postcard::to_stdvec(&Ask::Log { station, from: 0 }).ok()?)?;
        let raw = match self.ask(to, &Request::Hop(postcard::to_stdvec(&Hop::Sealed { eph, ct }).ok()?)).await.ok()? {
            Response::Hop(b) => b,
            _ => return None,
        };
        let n = match postcard::from_bytes::<Response>(&pyrate_core::route::open_answer(&c, &raw)?).ok()? {
            Response::Entries(l) => l.len(),
            _ => 0,
        };
        // A path chosen later than the first answer would show here too: give it a moment.
        tokio::time::sleep(Duration::from_secs(5)).await;
        let conn = self.conns.lock().unwrap().get(&to).cloned()?;
        let paths: Vec<bool> = conn.paths().iter().map(|p| p.is_relay()).collect();
        Some((n, paths))
    }

    /// For the deck test: names `intro` as the deck's way in, said at the next tick.
    pub(crate) fn deck_set_way_in(&self, intro: [u8; 32]) -> bool {
        let mut d = self.deck.lock().unwrap();
        let Some(d) = d.as_mut() else { return false };
        let Some(lead) = pyrate_core::route::make_lead(&intro, &pyrate_core::route::Lead { to: d.id, relay: d.relay.clone() }) else { return false };
        d.intro = Some(intro);
        d.lead = lead;
        d.said = 0;
        self.crew_radios.lock().unwrap().entry(intro).or_insert((String::new(), now_ms()));
        true
    }

    /// For the deck test: which paths this node's connection to `peer` runs over (true for a relay).
    pub(crate) fn paths_to(&self, peer: &EndpointId) -> Vec<bool> {
        self.conns.lock().unwrap().get(peer).map(|c| c.paths().iter().map(|p| p.is_relay()).collect()).unwrap_or_default()
    }
}
