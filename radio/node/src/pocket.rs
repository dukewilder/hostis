//! The node's side of pocket stations: keeping their keys, meeting on their own topics,
//! handing out their logs sealed, and never letting them near the public Log.
//! See pyrate_core::pocket.
//!
//! No rights reserved.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use futures_lite::StreamExt;
use iroh::EndpointId;
use iroh_gossip::{
    api::{Event, GossipSender},
    proto::TopicId,
};
use pyrate_core::{SignedEntry, StationId, pocket};
use serde_json::{Value, json};

use crate::net::{Announce, Gossip, Request, Response};
use crate::{Inner, hex, say};

pub(crate) struct Pocket {
    pub key: [u8; 32],
    hosts: Vec<EndpointId>,
    tx: Option<GossipSender>,
    task: Option<tokio::task::JoinHandle<()>>,
}

impl Drop for Pocket {
    fn drop(&mut self) {
        if let Some(t) = &self.task {
            t.abort();
        }
    }
}

impl Inner {
    pub(crate) fn pocket_key(&self, st: &StationId) -> Option<[u8; 32]> {
        self.pockets.lock().unwrap().get(st).map(|p| p.key)
    }

    /// The hosts a pocket's invitation named, to meet its members through.
    pub(crate) fn pocket_hosts(&self, st: &StationId) -> Vec<EndpointId> {
        self.pockets.lock().unwrap().get(st).map(|p| p.hosts.clone()).unwrap_or_default()
    }

    pub(crate) fn is_pocket(&self, st: &StationId) -> bool {
        self.pockets.lock().unwrap().contains_key(st)
    }

    /// Pockets this node knows, kept beside its keys: one line each, station and key.
    fn save_pockets(&self) {
        let lines: Vec<String> = self.pockets.lock().unwrap().iter().map(|(st, p)| format!("{} {} {}", st.to_hex(), hex(&p.key), p.hosts.iter().map(|h| h.to_string()).collect::<Vec<_>>().join(","))).collect();
        let _ = self.hold.write_note(&self.cfg.dir, "pockets", lines.join("\n").as_bytes());
    }

    /// Loads the pockets this node knows and meets each on its own topic.
    pub(crate) async fn open_pockets(self: &Arc<Self>) {
        let old = self.cfg.dir.join("keys").join("pockets");
        let text = self.hold.read_note(&self.cfg.dir, "pockets", Some(&old)).and_then(|b| String::from_utf8(b).ok()).unwrap_or_default();
        for line in text.lines() {
            let mut p = line.split_whitespace();
            let (Some(st), Some(k)) = (p.next().and_then(|s| StationId::from_hex(s).ok()), p.next().and_then(|k| crate::unhex(k).ok())) else { continue };
            let hosts: Vec<EndpointId> = p.next().unwrap_or("").split(',').filter_map(|h| h.parse().ok()).collect();
            let _ = self.pocket_open(st, k, hosts).await;
        }
    }

    pub(crate) async fn pocket_open(self: &Arc<Self>, st: StationId, key: [u8; 32], hosts: Vec<EndpointId>) -> Result<()> {
        if self.pockets.lock().unwrap().get(&st).is_some_and(|p| p.tx.is_some()) {
            return Ok(());
        }
        self.pockets.lock().unwrap().insert(st, Pocket { key, hosts: hosts.clone(), tx: None, task: None });
        self.save_pockets();
        let mut peers = hosts.clone();
        peers.extend(self.cfg.bootstrap.iter().copied());
        peers.retain(|p| *p != self.endpoint.id());
        let sub = self.gossip.subscribe(TopicId::from_bytes(pocket::topic(&key)), peers).await.map_err(|e| anyhow!("{e}"))?;
        let (tx, mut rx) = sub.split();
        let me = self.clone();
        let task = self.handle.spawn(async move {
            while let Some(ev) = rx.next().await {
                let Ok(Event::Received(m)) = ev else { continue };
                let Some(plain) = pocket::open(&key, &m.content) else { continue };
                let Ok(g) = postcard::from_bytes::<Gossip>(&plain) else { continue };
                // Only this pocket's own station: a member cannot slip another in.
                let ok = match &g {
                    Gossip::Station(a) => a.station == st,
                    Gossip::Carrier { station, .. } => *station == st,
                    // The members' hourly tokens, so a pocket counts who is aboard.
                    Gossip::Aboard { station, .. } => *station == st,
                    // Where the members can be reached, each in its own hand.
                    Gossip::Harbour { .. } => true,
                };
                if ok {
                    me.on_gossip(g);
                }
            }
        });
        if let Some(p) = self.pockets.lock().unwrap().get_mut(&st) {
            p.tx = Some(tx);
            p.task = Some(task);
        }
        Ok(())
    }

    /// Says something to a pocket's members only, sealed with its key.
    pub(crate) async fn pocket_say(&self, st: StationId, g: &Gossip) {
        let (tx, key) = {
            let p = self.pockets.lock().unwrap();
            let Some(p) = p.get(&st) else { return };
            (p.tx.clone(), p.key)
        };
        if let (Some(tx), Ok(bytes)) = (tx, postcard::to_stdvec(g)) {
            let _ = tx.broadcast(pocket::seal(&key, &bytes).into()).await;
        }
    }

    pub(crate) async fn pocket_card(&self, st: StationId, a: Announce) {
        self.on_gossip(Gossip::Station(a.clone()));
        self.pocket_say(st, &Gossip::Station(a)).await;
    }

    /// A pocket's log, sealed, for whoever asks by its tag: only members know the tag.
    pub(crate) fn pocket_entries(&self, tag: &[u8; 32], from: u64) -> Vec<Vec<u8>> {
        let found: Option<(StationId, [u8; 32])> = self.pockets.lock().unwrap().iter().find(|(_, p)| pocket::tag(&p.key) == *tag).map(|(st, p)| (*st, p.key));
        let Some((st, key)) = found else { return vec![] };
        let mut s = self.store.lock().unwrap();
        s.open_log(&st);
        s.logs.get(&st).map(|a| a.entries().iter().skip(from as usize).take(2000).filter_map(|e| e.to_bytes().ok()).map(|b| pocket::seal(&key, &b)).collect()).unwrap_or_default()
    }

    /// A pocket's log from the first member or host that answers, from `from` on.
    pub(crate) async fn pocket_fetch(&self, st: StationId, key: [u8; 32], from: u64) -> Vec<SignedEntry> {
        let tag = pocket::tag(&key);
        let mut asked: Vec<EndpointId> = self.pockets.lock().unwrap().get(&st).map(|p| p.hosts.clone()).unwrap_or_default();
        asked.extend(self.carriers(&st));
        asked.retain(|p| *p != self.endpoint.id());
        asked.dedup();
        for node in asked.into_iter().take(6) {
            let Ok(Response::Entries(list)) = self.ask(node, &Request::PocketLog { tag, from }).await else { continue };
            let entries: Vec<SignedEntry> = list.iter().filter_map(|b| pocket::open(&key, b)).filter_map(|b| SignedEntry::from_bytes(&b).ok()).filter(|e| e.entry.station == st).collect();
            if !entries.is_empty() {
                return entries;
            }
        }
        vec![]
    }

    /// Brings a pocket's log up to date from its members and hosts, never by its id.
    pub(crate) async fn pocket_sync(&self, st: StationId, key: [u8; 32]) -> Result<usize> {
        let tag = pocket::tag(&key);
        let mut asked: Vec<EndpointId> = self.pockets.lock().unwrap().get(&st).map(|p| p.hosts.clone()).unwrap_or_default();
        asked.extend(self.carriers(&st));
        asked.retain(|p| *p != self.endpoint.id());
        asked.dedup();
        let mut added = 0;
        for node in asked.into_iter().take(6) {
            let from = self.store.lock().unwrap().logs.get(&st).map(|a| a.len()).unwrap_or(0);
            let Ok(Response::Entries(list)) = self.ask(node, &Request::PocketLog { tag, from }).await else { continue };
            let entries: Vec<SignedEntry> = list.iter().filter_map(|b| pocket::open(&key, b)).filter_map(|b| SignedEntry::from_bytes(&b).ok()).filter(|e| e.entry.station == st).collect();
            if let Ok(n) = self.store.lock().unwrap().put_entries(st, entries) {
                added += n;
            }
        }
        Ok(added)
    }

    pub(crate) fn pocket_call(self: &Arc<Self>, cmd: &str, req: &Value) -> Result<Value> {
        match cmd {
            "pocket_invite" => {
                let st = StationId::from_hex(req["station"].as_str().context("station")?)?;
                let key = self.pocket_key(&st).context("not a pocket station")?;
                let relay = self.endpoint.addr().relay_urls().next().map(|u| u.to_string()).unwrap_or_default();
                Ok(json!({ "link": pocket::invitation(&st.0, &key, &format!("{}@{}", self.endpoint.id(), relay)) }))
            }
            "pocket_join" => {
                let (st, key, host) = pocket::read_invitation(req["link"].as_str().context("link")?).context("not an invitation")?;
                let st = StationId(st);
                let hosts: Vec<EndpointId> = host.split('@').next().and_then(|h| h.parse().ok()).into_iter().collect();
                let me = self.clone();
                self.handle.block_on(async move {
                    me.pocket_open(st, key, hosts).await?;
                    let _ = me.pocket_sync(st, key).await;
                    anyhow::Ok(())
                })?;
                say("joined a pocket station");
                Ok(json!({ "station": st.to_hex() }))
            }
            "pocket_leave" => {
                let st = StationId::from_hex(req["station"].as_str().context("station")?)?;
                self.pockets.lock().unwrap().remove(&st);
                self.save_pockets();
                self.log.lock().unwrap().remove(&st);
                let mut s = self.store.lock().unwrap();
                for h in s.chunks_of(&st) {
                    s.remove_chunk(&h);
                }
                s.forget_log(&st);
                Ok(json!({ "ok": true }))
            }
            _ => Err(anyhow!("unknown command {cmd}")),
        }
    }

    /// A new pocket station's key, made when its colours go up.
    pub(crate) fn pocket_new(self: &Arc<Self>, st: StationId) -> Result<()> {
        let mut key = [0u8; 32];
        rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut key);
        let me = self.clone();
        self.handle.block_on(async move { me.pocket_open(st, key, vec![]).await })
    }
}

pub(crate) type Pockets = HashMap<StationId, Pocket>;
