//! The node's side of chatter: joining a station's topic, hearing and checking lines,
//! speaking, and holding the last few lines in memory for whoever joins next. Nothing here
//! is written to disk. See pyrate_core::chat for the lines themselves.
//!
//! Joining is the listener's choice. It tells the nodes in the room that this one is there,
//! which listening alone never does.
//!
//! No rights reserved.

use std::collections::{HashSet, VecDeque};
use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use futures_lite::StreamExt;
use iroh::EndpointId;
use iroh_gossip::{
    api::{Event, GossipSender},
    proto::TopicId,
};
use pyrate_core::chat::{self, Say, Signed};
use pyrate_core::{StationId, StationKey};
use serde_json::{Value, json};

use crate::{Inner, describe_track, hex, now_ms, say, unhex};

/// Lines a node holds per room, in memory.
const KEEP: usize = 200;

pub(crate) struct Room {
    tx: GossipSender,
    lines: VecDeque<Signed>,
    seen: HashSet<[u8; 32]>,
    task: tokio::task::JoinHandle<()>,
    neighbours: usize,
}

impl Drop for Room {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl Inner {
    /// The key this node speaks with under a station's colours: the station's own key when
    /// this node keeps it, otherwise the listener's voice for that station.
    fn speaker(&self, st: &StationId) -> (StationKey, bool) {
        if let Some(k) = self.store.lock().unwrap().keys.get(st) {
            return (StationKey::from_seed(k.seed()), true);
        }
        (chat::voice(&self.secret, st), false)
    }

    /// Takes a line into a room if it is good and new. True if it was.
    fn hear(&self, st: StationId, s: Signed) -> bool {
        if !chat::check(&s, &st, now_ms()) {
            return false;
        }
        let mut rooms = self.chats.lock().unwrap();
        let Some(r) = rooms.get_mut(&st) else { return false };
        if !r.seen.insert(chat::id(&s)) {
            return false;
        }
        r.lines.push_back(s);
        while r.lines.len() > KEEP {
            r.lines.pop_front();
        }
        true
    }

    /// Joins a station's chatter, if not already in it. Peers to meet: the station's
    /// carriers and this node's own bootstrap, Lanterns among them.
    pub(crate) async fn chat_join(self: &Arc<Self>, st: StationId) -> Result<()> {
        if self.chats.lock().unwrap().contains_key(&st) {
            return Ok(());
        }
        let mut peers: Vec<EndpointId> = self.cfg.bootstrap.clone();
        if let Some(l) = self.log.lock().unwrap().get(&st) {
            peers.extend(l.carriers.keys().filter_map(|n| EndpointId::from_bytes(n).ok()));
        }
        peers.retain(|p| *p != self.endpoint.id());
        peers.sort();
        peers.dedup();
        peers.truncate(12);
        let sub = self.gossip.subscribe(TopicId::from_bytes(chat::topic(&st)), peers.clone()).await.map_err(|e| anyhow!("{e}"))?;
        let (tx, mut rx) = sub.split();
        let me = self.clone();
        let task = self.handle.spawn(async move {
            while let Some(ev) = rx.next().await {
                match ev {
                    Ok(Event::Received(m)) => {
                        if let Ok(s) = postcard::from_bytes::<Signed>(&m.content) {
                            me.hear(st, s);
                        }
                    }
                    Ok(Event::NeighborUp(_)) => {
                        if let Some(r) = me.chats.lock().unwrap().get_mut(&st) {
                            r.neighbours += 1;
                        }
                    }
                    Ok(Event::NeighborDown(_)) => {
                        if let Some(r) = me.chats.lock().unwrap().get_mut(&st) {
                            r.neighbours = r.neighbours.saturating_sub(1);
                        }
                    }
                    _ => {}
                }
            }
        });
        self.chats.lock().unwrap().insert(st, Room { tx, lines: VecDeque::new(), seen: HashSet::new(), task, neighbours: 0 });
        // What was said before we came: a carrier that sat in the room holds the last lines.
        let me = self.clone();
        self.handle.spawn(async move {
            for p in peers.into_iter().take(3) {
                if let Ok(crate::net::Response::Chatter(lines)) = me.ask(p, &crate::net::Request::Chatter { station: st }).await {
                    let mut got = 0;
                    for b in lines.iter().take(KEEP) {
                        if let Ok(s) = postcard::from_bytes::<Signed>(b) {
                            got += me.hear(st, s) as usize;
                        }
                    }
                    if got > 0 {
                        // Older lines first, as they were said.
                        if let Some(r) = me.chats.lock().unwrap().get_mut(&st) {
                            r.lines.make_contiguous().sort_by_key(|s| s.line.at_ms);
                        }
                        break;
                    }
                }
            }
        });
        Ok(())
    }

    /// The last lines a room holds, for a node that has just come in.
    pub(crate) fn chatter_for(&self, st: &StationId) -> Vec<Vec<u8>> {
        let rooms = self.chats.lock().unwrap();
        let Some(r) = rooms.get(st) else { return vec![] };
        r.lines.iter().rev().take(60).rev().filter_map(|s| postcard::to_allocvec(s).ok()).collect()
    }

    fn line_json(&self, st: &StationId, s: &Signed, me: &[u8; 32]) -> Value {
        let keeper = s.line.from == st.0 || self.store.lock().unwrap().logs.get(st).is_some_and(|a| a.deputies().contains(&s.line.from));
        let (kind, text, track, title) = match &s.line.say {
            Say::Text(t) => ("text", t.clone(), None, None),
            Say::Cheer => ("cheer", String::new(), None, None),
            Say::Request { track, note } => {
                let title = self.store.lock().unwrap().logs.get(st).map(|a| describe_track(a, track).0).filter(|t| !t.is_empty());
                ("request", note.clone(), Some(hex(track)), title)
            }
        };
        json!({
            "id": hex(&chat::id(s)), "from": hex(&s.line.from), "voice": StationId(s.line.from).call_sign(),
            "name": s.line.name, "at_ms": s.line.at_ms, "kind": kind, "text": text, "track": track, "title": title,
            "keeper": keeper, "mine": s.line.from == *me,
        })
    }

    pub(crate) fn chat_call(self: &Arc<Self>, cmd: &str, st: StationId, req: &Value) -> Result<Value> {
        match cmd {
            "chat_join" => {
                let me = self.clone();
                self.handle.block_on(async move { me.chat_join(st).await })?;
                Ok(json!({ "ok": true }))
            }
            "chat_leave" => {
                self.chats.lock().unwrap().remove(&st);
                Ok(json!({ "ok": true }))
            }
            "chat_read" => {
                let (key, keeper) = self.speaker(&st);
                let me = key.id().0;
                let rooms = self.chats.lock().unwrap();
                let Some(r) = rooms.get(&st) else { return Ok(json!({ "joined": false, "lines": [] })) };
                let lines: Vec<Signed> = r.lines.iter().cloned().collect();
                let neighbours = r.neighbours;
                drop(rooms);
                Ok(json!({
                    "joined": true, "neighbours": neighbours, "keeper": keeper,
                    "voice": StationId(me).call_sign(),
                    "lines": lines.iter().map(|s| self.line_json(&st, s, &me)).collect::<Vec<_>>(),
                }))
            }
            "chat_send" => {
                let say = match req["kind"].as_str().unwrap_or("text") {
                    "cheer" => Say::Cheer,
                    "request" => Say::Request { track: unhex(req["track"].as_str().context("track")?)?, note: req["text"].as_str().unwrap_or("").into() },
                    _ => {
                        let t = req["text"].as_str().unwrap_or("").trim().to_string();
                        if t.is_empty() {
                            anyhow::bail!("nothing to say");
                        }
                        Say::Text(t)
                    }
                };
                let (key, _) = self.speaker(&st);
                let name = req["name"].as_str().unwrap_or("").to_string();
                let s = chat::speak(&key, &st, &name, now_ms(), say);
                let bytes = postcard::to_allocvec(&s)?;
                let tx = self.chats.lock().unwrap().get(&st).map(|r| r.tx.clone()).context("join the chatter first")?;
                self.hear(st, s);
                self.handle.block_on(async move { tx.broadcast(bytes.into()).await }).map_err(|e| anyhow!("{e}"))?;
                Ok(json!({ "ok": true }))
            }
            _ => Err(anyhow!("unknown command {cmd}")),
        }
    }

    /// A Lantern sits in the rooms of its own stations and the busiest in the Log, so that
    /// listeners who come in find someone there and the last lines to read.
    pub(crate) async fn keep_rooms(self: &Arc<Self>) {
        let mut want: Vec<StationId> = self.store.lock().unwrap().keys.keys().copied().collect();
        let now = now_ms();
        let mut busy: Vec<(u64, StationId)> = self.log.lock().unwrap().iter().filter(|(_, l)| l.fresh(now)).map(|(id, l)| (l.aboard(now), *id)).collect();
        busy.sort_by(|a, b| b.0.cmp(&a.0));
        want.extend(busy.into_iter().take(24).map(|(_, id)| id));
        want.dedup();
        for st in want.into_iter().take(40) {
            if let Err(e) = self.chat_join(st).await {
                say(&format!("chatter: {e}"));
            }
        }
    }
}
