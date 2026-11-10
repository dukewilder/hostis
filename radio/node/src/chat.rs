//! The node's side of chatter: joining a station's topic, hearing and checking lines,
//! speaking, and holding the last few lines in memory for whoever joins next. Nothing here
//! is written to disk. See pyrate_core::chat for the lines themselves.
//!
//! Joining is the listener's choice. It tells the nodes in the room that this one is there,
//! which listening alone never does.
//!
//! No rights reserved.

use std::collections::VecDeque;
use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use futures_lite::StreamExt;
use iroh::EndpointId;
use iroh_gossip::{
    api::{Event, GossipSender},
    proto::TopicId,
};
use pyrate_core::chat::{self, Say, Signed};
use pyrate_core::{StationId, StationKey, pocket};
use serde_json::{Value, json};

use crate::{Inner, describe_track, hex, now_ms, say, unhex};

/// How many kept hails a Lantern holds before the oldest go.
const KEPT_HAILS: usize = 500;

impl Inner {
    fn keep_hail(&self, s: &Signed) {
        keep_hail_in(&self.hold, &self.cfg.dir, s);
    }
}

fn keep_hail_in(hold: &crate::hold::Hold, dir: &std::path::Path, s: &Signed) {
    static ONE: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _one = ONE.lock().unwrap_or_else(|e| e.into_inner());
    let Ok(bytes) = postcard::to_allocvec(s) else { return };
    let id = hex(&chat::id(s));
    let mut kept: Vec<String> = hold.read_note(dir, "hails", None).and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
    if kept.contains(&id) || hold.write_note(dir, &format!("hail {id}"), &bytes).is_err() {
        return;
    }
    kept.push(id);
    while kept.len() > KEPT_HAILS {
        let old = kept.remove(0);
        hold.remove_note(dir, &format!("hail {old}"));
    }
    if let Ok(b) = serde_json::to_vec(&kept) {
        let _ = hold.write_note(dir, "hails", &b);
    }
}

#[cfg(test)]
mod kept_hails {
    use pyrate_core::StationKey;
    use pyrate_core::chat;

    #[test]
    fn a_kept_hail_is_noise_on_disk_and_opens_for_the_keeper() {
        let dir = std::env::temp_dir().join(format!("pyrate-hails-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let key = StationKey::generate().unwrap();
        let st = key.id();
        let mut store = crate::store::Store::open_with(&dir, u64::MAX, None).unwrap();
        store.add_key(StationKey::from_seed(key.seed())).unwrap();
        drop(store);
        let voice = b"OggS a few seconds of a voice saying hello there";
        let say = chat::hail(&[st.0], voice, "audio/ogg").unwrap();
        let listener = StationKey::generate().unwrap();
        let line = chat::speak(&listener, &st, "Anne", 1_794_000_000_000, say);
        let hold = crate::hold::Hold::for_dir(&dir, None).unwrap();
        super::keep_hail_in(&hold, &dir, &line);
        // Nothing under the node's folder holds the voice or the sender's name in the clear.
        let mut stack = vec![dir.clone()];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).unwrap().flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                    continue;
                }
                let b = std::fs::read(&p).unwrap();
                assert!(!b.windows(5).any(|w| w == b"hello"), "{p:?}");
                assert!(!b.windows(4).any(|w| w == b"Anne"), "{p:?}");
                assert!(!p.to_string_lossy().contains("Anne"));
            }
        }
        let out = dir.join("opened");
        assert_eq!(super::open_hails(&dir, &out).unwrap(), 1);
        let files: Vec<_> = std::fs::read_dir(&out).unwrap().flatten().collect();
        assert_eq!(files.len(), 1);
        assert_eq!(std::fs::read(files[0].path()).unwrap(), voice);
        assert!(files[0].file_name().to_string_lossy().contains("Anne"));
        // Opened once, then forgotten.
        assert_eq!(super::open_hails(&dir, &out).unwrap(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// Opens the hails a keeper's Lantern kept into `out`, one sound file each, named by
/// station, time and sender, and forgets them. The count opened.
pub fn open_hails(dir: &std::path::Path, out: &std::path::Path) -> Result<usize> {
    let hold = crate::hold::Hold::for_dir(dir, None)?;
    let store = crate::store::Store::open_with(dir, u64::MAX, None)?;
    let kept: Vec<String> = hold.read_note(dir, "hails", None).and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
    std::fs::create_dir_all(out)?;
    let mut n = 0;
    for id in &kept {
        let name = format!("hail {id}");
        let Some(s) = hold.read_note(dir, &name, None).and_then(|b| postcard::from_bytes::<Signed>(&b).ok()) else { continue };
        let st = StationId(s.line.station);
        let Some((audio, mime)) = store.keys.get(&st).and_then(|k| chat::open_hail(&s.line.say, &k.seed())) else { continue };
        let ext = if mime.contains("webm") { "webm" } else if mime.contains("ogg") { "ogg" } else { "m4a" };
        let who: String = chat::shown_name(&s.line.name).chars().map(|c| if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') { ' ' } else { c }).take(40).collect();
        let who = who.trim().trim_start_matches('.').to_string();
        let who = if who.is_empty() { StationId(s.line.from).call_sign() } else { who };
        std::fs::write(out.join(format!("{} {} {}.{ext}", st.call_sign(), s.line.at_ms / 1000, who)), audio)?;
        hold.remove_note(dir, &name);
        n += 1;
    }
    for id in &kept {
        hold.remove_note(dir, &format!("hail {id}"));
    }
    hold.remove_note(dir, "hails");
    Ok(n)
}

/// Lines a node holds per room, in memory.
const KEEP: usize = 200;

pub(crate) struct Room {
    tx: GossipSender,
    lines: VecDeque<Signed>,
    seen: chat::Seen,
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
        (chat::voice(&self.seed, st), false)
    }

    /// Takes a line into a room if it is good and new. True if it was.
    fn hear(&self, st: StationId, s: Signed) -> bool {
        if !chat::check(&s, &st, now_ms()) {
            return false;
        }
        // A hail is held only by the keeper it is sealed to; everyone else lets it pass.
        if matches!(s.line.say, Say::Hail { .. }) {
            let mine: Vec<[u8; 32]> = self.store.lock().unwrap().keys.get(&st).map(|k| vec![k.id().0]).unwrap_or_default();
            if !chat::hail_for(&s.line.say, &mine) {
                return false;
            }
            // A keeper's own Lantern may be asked (PYRATE_HAILS) to keep hails for a keeper who
            // is away. Each is kept as it came, still sealed to the station, and sealed again
            // under the node's hold key, so the disk shows neither the voice nor who sent it;
            // `lantern --open-hails <folder>` opens them for the keeper. Otherwise a hail lives
            // only in memory, as everything here does.
            if std::env::var_os("PYRATE_HAILS").is_some() {
                self.keep_hail(&s);
            }
        }
        let mut rooms = self.chats.lock().unwrap();
        let Some(r) = rooms.get_mut(&st) else { return false };
        if !r.seen.insert(chat::id(&s), s.line.at_ms, now_ms()) {
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
        // A pocket's room is its members' alone: a topic drawn from its key, met through its
        // hosts, every line sealed with the key.
        let key = self.pocket_key(&st);
        let mut peers: Vec<EndpointId> = if key.is_some() { self.pocket_hosts(&st) } else { self.cfg.bootstrap.clone() };
        if let Some(l) = self.log.lock().unwrap().get(&st) {
            peers.extend(l.carriers.keys().filter_map(|n| EndpointId::from_bytes(n).ok()));
        }
        peers.retain(|p| *p != self.endpoint.id());
        peers.sort();
        peers.dedup();
        peers.truncate(12);
        let topic = key.map(|k| pocket::chat_topic(&k)).unwrap_or_else(|| chat::topic(&st));
        let sub = self.gossip.subscribe(TopicId::from_bytes(topic), peers.clone()).await.map_err(|e| anyhow!("{e}"))?;
        let (tx, mut rx) = sub.split();
        let me = self.clone();
        let task = self.handle.spawn(async move {
            while let Some(ev) = rx.next().await {
                match ev {
                    Ok(Event::Received(m)) => {
                        let bytes = match &key {
                            Some(k) => pocket::open(k, &m.content),
                            None => Some(m.content.to_vec()),
                        };
                        if let Some(s) = bytes.and_then(|b| postcard::from_bytes::<Signed>(&b).ok()) {
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
        self.chats.lock().unwrap().insert(st, Room { tx, lines: VecDeque::new(), seen: chat::Seen::default(), task, neighbours: 0 });
        // What was said before we came: a carrier that sat in the room holds the last lines.
        let me = self.clone();
        self.handle.spawn(async move {
            let ask = match &key {
                Some(k) => crate::net::Request::PocketChatter { tag: pocket::tag(k) },
                None => crate::net::Request::Chatter { station: st },
            };
            for p in peers.into_iter().take(3) {
                if let Ok(crate::net::Response::Chatter(lines)) = me.ask(p, &ask).await {
                    let mut got = 0;
                    for b in lines.iter().take(KEEP) {
                        let b = match &key {
                            Some(k) => match pocket::open(k, b) { Some(b) => b, None => continue },
                            None => b.clone(),
                        };
                        if let Ok(s) = postcard::from_bytes::<Signed>(&b) {
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
        // A pocket's lines go only to whoever knows its tag, and sealed.
        if self.is_pocket(st) {
            return vec![];
        }
        let rooms = self.chats.lock().unwrap();
        let Some(r) = rooms.get(st) else { return vec![] };
        let open: Vec<&Signed> = r.lines.iter().filter(|s| !matches!(s.line.say, Say::Hail { .. })).collect();
        open[open.len().saturating_sub(60)..].iter().filter_map(|s| postcard::to_allocvec(s).ok()).collect()
    }

    /// The last lines of a pocket's room, sealed, for a member who asks by its tag.
    pub(crate) fn pocket_chatter(&self, tag: &[u8; 32]) -> Vec<Vec<u8>> {
        let found = self.pockets.lock().unwrap().iter().find(|(_, p)| pocket::tag(&p.key) == *tag).map(|(st, p)| (*st, p.key));
        let Some((st, key)) = found else { return vec![] };
        let rooms = self.chats.lock().unwrap();
        let Some(r) = rooms.get(&st) else { return vec![] };
        let open: Vec<&Signed> = r.lines.iter().filter(|s| !matches!(s.line.say, Say::Hail { .. })).collect();
        open[open.len().saturating_sub(60)..].iter().filter_map(|s| postcard::to_allocvec(s).ok()).map(|b| pocket::seal(&key, &b)).collect()
    }

    fn line_json(&self, st: &StationId, s: &Signed, me: &[u8; 32]) -> Value {
        let keeper = s.line.from == st.0 || self.store.lock().unwrap().logs.get(st).is_some_and(|a| a.deputies().contains(&s.line.from));
        let (kind, text, track, title) = match &s.line.say {
            Say::Text(t) => ("text", chat::shown(t), None, None),
            Say::Cheer => ("cheer", String::new(), None, None),
            Say::Request { track, note } => {
                let title = self.store.lock().unwrap().logs.get(st).map(|a| describe_track(a, track).0).filter(|t| !t.is_empty());
                ("request", chat::shown(note), Some(hex(track)), title)
            }
            Say::Hail { .. } => ("hail", String::new(), None, None),
        };
        json!({
            "id": hex(&chat::id(s)), "from": hex(&s.line.from), "voice": StationId(s.line.from).call_sign(),
            "name": chat::shown_name(&s.line.name), "at_ms": s.line.at_ms, "kind": kind, "text": text, "track": track, "title": title.map(crate::shown),
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
            // Plays a hail sealed to this keeper: unsealed into a file the app plays and removes.
            "chat_hail_open" => {
                let id = unhex(req["id"].as_str().context("id")?)?;
                let line = self.chats.lock().unwrap().get(&st).and_then(|r| r.lines.iter().find(|s| chat::id(s) == id).cloned()).context("that hail has gone")?;
                let seed = self.store.lock().unwrap().keys.get(&st).map(|k| k.seed()).context("only the keeper opens a hail")?;
                let (audio, mime) = chat::open_hail(&line.line.say, &seed).context("this hail is not for this phone")?;
                let ext = if mime.contains("webm") { "webm" } else if mime.contains("ogg") { "ogg" } else { "m4a" };
                let path = self.cfg.dir.join(format!("hail.{ext}"));
                std::fs::write(&path, audio)?;
                Ok(json!({ "path": path, "mime": mime }))
            }
            "chat_send" => {
                let say = match req["kind"].as_str().unwrap_or("text") {
                    "cheer" => Say::Cheer,
                    "hail" => {
                        let audio = std::fs::read(req["path"].as_str().context("path")?)?;
                        let mut to = vec![st.0];
                        if let Some(a) = self.store.lock().unwrap().logs.get(&st) {
                            to.extend(a.deputies());
                        }
                        chat::hail(&to, &audio, req["mime"].as_str().unwrap_or("audio/mp4")).context("a hail is twenty seconds at most")?
                    }
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
                let mut bytes = postcard::to_allocvec(&s)?;
                if let Some(k) = self.pocket_key(&st) {
                    bytes = pocket::seal(&k, &bytes);
                }
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
        // A Lantern that carries only its own stations sits only in their rooms.
        if !self.cfg.carry_all {
            for st in want {
                if let Err(e) = self.chat_join(st).await {
                    say(&format!("chatter: {e}"));
                }
            }
            return;
        }
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
