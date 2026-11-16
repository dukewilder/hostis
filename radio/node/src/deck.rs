//! The deck of an app node: the station it plays, served to the crew while it plays, as a
//! listener's tab serves its own. The deck has a key made for it alone and no endpoint at all.
//! It pulls the asks waiting for it from its way in, as a tab behind a door does, along an
//! ordinary three-hop route (this node's guard, a middle, the way in), and its note goes up in
//! the same pulls for the way in to pass on. So no relay sees the deck's key beside this
//! device's address, no gossip neighbour hears the note from this node, and the way in, the one
//! radio that may ask the deck anything, sees a middle and never this device. The lead names a
//! cookie, never a node. Only with "Carry the radio while you listen" on, on a connection that
//! may carry others' traffic (on a phone, Wi-Fi), never while the node runs dark, never for its
//! own station or a pocket's. A phone passes on no more than it listens to; a computer six times.
use std::{
    collections::HashSet,
    sync::{Arc, atomic::Ordering::Relaxed},
    time::Duration,
};

use anyhow::{Result, anyhow};
use iroh::EndpointId;
use pyrate_core::StationId;

use crate::{
    Inner,
    hop::{Ask, Hop},
    net::{Gossip, Request, Response},
    now_ms, say,
};

/// Its own listening counted as at least 96 kbit/s, a minute of it.
const BUDGET_FLOOR: u64 = 720 * 1024;
/// A page of a log along a route.
const PAGE: usize = 256 * 1024;

pub(crate) struct Deck {
    pub(crate) station: StationId,
    seed: [u8; 32],
    pub(crate) id: [u8; 32],
    /// What the deck's lead names in place of a node: the way in hands the asks for it over in
    /// the deck's pulls.
    cookie: [u8; 32],
    pub(crate) intro: Option<[u8; 32]>,
    lead: Vec<u8>,
    /// The deck's note, to go up in its next pull.
    note: Vec<u8>,
    pulling: bool,
    said: u64,
    /// The sealed names of the station's chunks this node holds: the deck serves only those.
    names: HashSet<[u8; 32]>,
    names_at: u64,
    /// Bytes passed on this minute, and the minute.
    minute: (u64, u64),
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
        drop(old);
        let Some(station) = want else { return };
        if self.deck.lock().unwrap().is_none() {
            let seed: [u8; 32] = rand::random();
            let id = pyrate_core::StationKey::from_seed(seed).id().0;
            let cookie: [u8; 32] = rand::random();
            *self.deck.lock().unwrap() = Some(Deck { station, seed, id, cookie, intro: None, lead: vec![], note: vec![], pulling: false, said: 0, names: HashSet::new(), names_at: 0, minute: (0, 0) });
        }
        let now = now_ms();
        // The way in: a radio of the crew heard from lately, never this node or its guards.
        let (intro, said, cookie, id, seed) = {
            let d = self.deck.lock().unwrap();
            let Some(d) = d.as_ref() else { return };
            (d.intro, d.said, d.cookie, d.id, d.seed)
        };
        let fresh = intro.is_some_and(|x| self.crew_radios.lock().unwrap().get(&x).is_some_and(|(_, t)| now.saturating_sub(*t) < crate::hop::CREW_FRESH_MS));
        let intro = if fresh { intro } else { self.middle(&[]).map(|m| *m.as_bytes()) };
        let Some(intro) = intro else { return };
        if fresh && now.saturating_sub(said) < 120_000 {
            self.deck_names(now);
            return;
        }
        let lead = if fresh { self.deck.lock().unwrap().as_ref().map(|d| d.lead.clone()).unwrap_or_default() } else { pull_lead(&intro, &cookie).unwrap_or_default() };
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
        // The note goes up in the deck's next pull, and its way in passes it on: from this
        // node's own gossip endpoint, every neighbour would know this node plays the station.
        let note = postcard::to_stdvec(&Gossip::deck(&seed, station, head, intro, lead, now)).unwrap_or_default();
        let start = {
            let mut d = self.deck.lock().unwrap();
            match d.as_mut() {
                Some(d) if d.id == id => {
                    d.note = note;
                    !std::mem::replace(&mut d.pulling, true)
                }
                _ => false,
            }
        };
        if start {
            let me = self.clone();
            tokio::spawn(async move { me.deck_pull_loop(id).await });
        }
    }

    /// The deck's pulls, one after another while it lasts: each brings back the asks waiting at
    /// its way in, and each ask is answered in a pull of its own. Six pulls in a row that fail
    /// let the way in go, and the next tick draws another.
    async fn deck_pull_loop(self: Arc<Self>, deck: [u8; 32]) {
        let mut fails = 0u64;
        let mut last_intro: Option<[u8; 32]> = None;
        loop {
            let job = {
                let mut d = self.deck.lock().unwrap();
                match d.as_mut() {
                    Some(d) if d.id == deck => d.intro.map(|i| (i, d.cookie, std::mem::take(&mut d.note))),
                    _ => None,
                }
            };
            let Some((intro, cookie, note)) = job.filter(|_| self.deck_wanted().is_some()) else { break };
            // A new way in starts with a clean slate.
            if last_intro.replace(intro).is_some_and(|l| l != intro) {
                fails = 0;
            }
            match self.deck_pull(deck, intro, cookie, note.clone(), vec![]).await {
                Ok(()) => fails = 0,
                Err(e) => {
                    if let Some(d) = self.deck.lock().unwrap().as_mut().filter(|d| d.id == deck && d.note.is_empty()) {
                        d.note = note;
                    }
                    // No route to be had is no fault of the way in: wait for one, saying why at
                    // most once a minute.
                    if e.to_string().starts_with("no ") {
                        static SAID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
                        let now = now_ms();
                        if now.saturating_sub(SAID.load(std::sync::atomic::Ordering::Relaxed)) > 60_000 {
                            SAID.store(now, std::sync::atomic::Ordering::Relaxed);
                            say(&format!("deck: waiting for a route ({e})"));
                        }
                        tokio::time::sleep(Duration::from_secs(3)).await;
                        continue;
                    }
                    // Six failed pulls in a row, a middle that did not answer passed over each
                    // time, and the way in is the likelier fault.
                    fails += 1;
                    if fails >= 6 {
                        say(&format!("deck: six pulls failed ({e}); another way in next"));
                        if let Some(d) = self.deck.lock().unwrap().as_mut().filter(|d| d.id == deck) {
                            d.intro = None;
                            d.said = 0;
                        }
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(500 * fails)).await;
                }
            }
        }
        if let Some(d) = self.deck.lock().unwrap().as_mut().filter(|d| d.id == deck) {
            d.pulling = false;
        }
    }

    /// One pull, sealed to the way in and passed on by a middle after this node's guard. What it
    /// brings back is answered, each ask in a pull of its own.
    fn deck_pull(self: &Arc<Self>, deck: [u8; 32], intro: [u8; 32], cookie: [u8; 32], note: Vec<u8>, answers: Vec<(u64, Vec<u8>)>) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + '_>> {
        Box::pin(async move {
            let nodes = self.known_nodes_pub();
            let guard = self.fetch_guard(&nodes).ok_or_else(|| anyhow!("no guard"))?;
            if *guard.as_bytes() == intro {
                return Err(anyhow!("no way in apart from the guard"));
            }
            let middle = self.middle(&[*guard.as_bytes(), intro]).ok_or_else(|| anyhow!("no middle"))?;
            let ask = postcard::to_stdvec(&Ask::Pull { cookie, note, answers })?;
            let (hop, ciphers) = crate::hop::three_hops(middle.as_bytes(), &intro, &ask, None)?;
            let raw = tokio::time::timeout(Duration::from_secs(12), self.hop(guard, &hop)).await;
            let items = match &raw {
                Ok(Ok(raw)) => crate::hop::open_layers(&ciphers, raw).and_then(|p| postcard::from_bytes::<Vec<(u64, Vec<u8>)>>(&p).ok()),
                _ => None,
            };
            let Some(items) = items else {
                // No answer at all is most often a middle that has gone: the next pull draws
                // another. An empty one came back through the middle, so the way in is the
                // likelier fault, and the loop counts it against the way in.
                if !matches!(raw, Ok(Ok(_))) {
                    self.failed.lock().unwrap().insert(*middle.as_bytes(), now_ms());
                }
                return Err(match raw {
                    Err(_) => anyhow!("slow pull"),
                    Ok(Err(e)) => e,
                    Ok(Ok(_)) => anyhow!("the pull came back empty"),
                });
            };
            for (id, blob) in items.into_iter().take(16) {
                let me = self.clone();
                tokio::spawn(async move {
                    let out = me.deck_answer(deck, intro, &blob).await;
                    if !out.is_empty() {
                        let _ = me.deck_pull(deck, intro, cookie, vec![], vec![(id, out)]).await;
                    }
                });
            }
            Ok(())
        })
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
    /// log through the lead, and says which paths a connection of this radio's own to the deck
    /// runs over (none: the deck takes its asks in its own pulls).
    pub(crate) async fn deck_probe(self: &Arc<Self>, station: StationId) -> Option<(usize, Vec<bool>)> {
        let me = *self.endpoint.id().as_bytes();
        let note = self.log.lock().unwrap().get(&station)?.decks.iter().find(|(_, n)| n.intro == me).map(|(d, n)| (*d, n.clone()))?;
        let lead = pyrate_core::route::open_lead(&self.secret, &me, &note.1.lead)?;
        if lead.relay != pyrate_core::route::LEAD_PULL {
            return None;
        }
        let (eph, ct, c) = pyrate_core::route::seal_to(&note.0, &postcard::to_stdvec(&Ask::Log { station, from: 0 }).ok()?)?;
        let raw = self.pull_hand(lead.to, postcard::to_stdvec(&Hop::Sealed { eph, ct }).ok()?).await;
        let n = match postcard::from_bytes::<Response>(&pyrate_core::route::open_answer(&c, &raw)?).ok()? {
            Response::Entries(l) => l.len(),
            _ => 0,
        };
        let paths = EndpointId::from_bytes(&lead.to).ok().map(|to| self.paths_to(&to)).unwrap_or_default();
        Some((n, paths))
    }

    /// For the deck test: names `intro` as the deck's way in, said at the next tick.
    pub(crate) fn deck_set_way_in(&self, intro: [u8; 32]) -> bool {
        let mut d = self.deck.lock().unwrap();
        let Some(d) = d.as_mut() else { return false };
        let Some(lead) = pull_lead(&intro, &d.cookie) else { return false };
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

/// A deck's lead for its way in: the cookie it pulls under, never a node.
fn pull_lead(intro: &[u8; 32], cookie: &[u8; 32]) -> Option<Vec<u8>> {
    pyrate_core::route::make_lead(intro, &pyrate_core::route::Lead { to: *cookie, relay: pyrate_core::route::LEAD_PULL.into() })
}
