//! Stations kept to their link are spoken of under tags.
//!
//! Whatever a node says of such a station on the open Log (its card, that it carries it, its
//! listeners' tokens, a deck) goes sealed under a key drawn from the station's id and the hour,
//! filed under a tag drawn the same way (`net::Gossip::Tagged`). A node holding the id, from
//! the link, works out the tags of the stations it knows and opens what is filed under them;
//! every other node passes the notes on and keeps a few for newcomers, unread. So a Lantern or a
//! phone that carries such a station names itself to that station's listeners and to nobody
//! else, and nobody can list which stations it carries.
//!
//! A listed station's id is public, so a tag would hide nothing: its carriers sign plain notes,
//! as before. A Lantern names itself for a listed station it carries blind only where the
//! ballast score would have put it anyway (`may_name_carrier`), so its note says nothing anyone
//! could not work out from the two ids.
//!
//! No rights reserved.

use std::collections::HashMap;

use pyrate_core::StationId;

use crate::{
    Inner, net,
    net::{Gossip, tag_hours, tag_of},
    now_ms,
};

/// The tags of the stations a node knows, for the hours around now.
#[derive(Default)]
pub(crate) struct TagTable {
    built: u64,
    known: usize,
    map: HashMap<[u8; 16], (StationId, u64)>,
}

impl Inner {
    /// Whether a station keeps itself off the public dial, by its card or by its own log.
    pub(crate) fn link_only(&self, st: &StationId) -> bool {
        if let Some(a) = self.log.lock().unwrap().get(st).and_then(|l| l.announce.as_ref()) {
            return a.link_only;
        }
        self.store.lock().unwrap().logs.get(st).is_some_and(|a| a.link_only())
    }

    /// What goes out on the open Log for a note: as it is, sealed under its station's tag when
    /// that station is kept to its link, or nothing (a plain carrier note for such a station).
    pub(crate) fn outgoing(&self, g: &Gossip) -> Option<Gossip> {
        let Some(st) = g.about() else { return Some(g.clone()) };
        if !self.link_only(&st) {
            return Some(g.clone());
        }
        Gossip::tagged(g, now_ms())
    }

    /// The stations this node can work out tags for: those in its Log, those whose logs it
    /// holds and its own.
    fn tag_stations(&self) -> Vec<StationId> {
        let mut v: Vec<StationId> = self.log.lock().unwrap().keys().copied().collect();
        {
            let s = self.store.lock().unwrap();
            v.extend(s.logs.keys().copied());
            v.extend(s.keys.keys().copied());
        }
        if let Some(t) = self.tuned.lock().unwrap().as_ref() {
            v.push(t.station);
        }
        v.sort_unstable();
        v.dedup();
        v
    }

    /// The station and hour a tag was drawn from, if it is one of a station this node knows.
    fn tag_owner(&self, tag: &[u8; 16]) -> Option<(StationId, u64)> {
        let now = now_ms();
        {
            let t = self.tags.lock().unwrap();
            if let Some(hit) = t.map.get(tag) {
                return Some(*hit);
            }
            // A miss is believed while the table is fresh: rebuilt each minute, or sooner when
            // this node has come to know more stations.
            if now.saturating_sub(t.built) < 60_000 && now / net::TAG_SPAN_MS == t.built / net::TAG_SPAN_MS && t.known == self.log.lock().unwrap().len() {
                return None;
            }
        }
        let stations = self.tag_stations();
        let known = self.log.lock().unwrap().len();
        let mut map = HashMap::with_capacity(stations.len() * 3);
        for st in stations {
            for h in tag_hours(now) {
                map.insert(tag_of(&st, h), (st, h));
            }
        }
        let hit = map.get(tag).copied();
        *self.tags.lock().unwrap() = TagTable { built: now, known, map };
        hit
    }

    /// A tagged note opened, if it is about a station this node knows.
    pub(crate) fn open_tagged(&self, g: &Gossip) -> Option<Gossip> {
        let Gossip::Tagged { tag, .. } = g else { return None };
        let (st, hour) = self.tag_owner(tag)?;
        g.open_tagged(&st, hour)
    }

    /// The tagged notes this node holds for one station, opened: for a station just named by a
    /// link, or asked of this node as an exit, so its card and carriers are known at once.
    pub(crate) fn open_tags_for(&self, st: &StationId) -> Vec<Gossip> {
        let now = now_ms();
        let hours = tag_hours(now);
        let tags: Vec<[u8; 16]> = hours.iter().map(|h| tag_of(st, *h)).collect();
        let raws = self.notes.lock().unwrap().tagged_for(&tags, now);
        raws.iter()
            .filter_map(|r| postcard::from_bytes::<Gossip>(r).ok())
            .filter_map(|g| hours.iter().find_map(|h| g.open_tagged(st, *h)))
            .collect()
    }

    /// Takes in what a station's tagged notes say: its card, then its carriers. True when the
    /// card shows entries this node lacks.
    pub(crate) fn take_tags_for(&self, st: &StationId) -> bool {
        let mut grew = false;
        let mut notes = self.open_tags_for(st);
        notes.sort_by_key(|g| !matches!(g, Gossip::Station(_)));
        let me = *self.endpoint.id().as_bytes();
        for g in notes {
            if let Some(g) = g.from_network(now_ms(), &me) {
                grew |= self.on_gossip(g).is_some();
            }
        }
        grew
    }

    /// Whether this node may name itself as a carrier of a station on the open Log. A phone
    /// keeps its own rules (its ballast alone); a Lantern names itself for any station it keeps
    /// openly and any kept to its link (whose note goes sealed), and for a listed station it
    /// carries blind only where the ballast score falls to it, or no other carrier is known, so
    /// a station a stem left with it is still found.
    pub(crate) fn may_name_carrier(&self, st: &StationId) -> bool {
        if !self.cfg.lantern || self.is_open(st) || self.link_only(st) {
            return true;
        }
        let me = *self.endpoint.id().as_bytes();
        let fleet = self.fleet();
        if net::share_score(&me, &st.0) < net::share_of(fleet.len() + 1) {
            return true;
        }
        let now = now_ms();
        !self.log.lock().unwrap().get(st).is_some_and(|l| l.carriers.iter().any(|(n, (_, seen))| *n != me && now.saturating_sub(*seen) < net::CARRIER_STALE_MS))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::{Card, pow_find};
    use pyrate_core::StationKey;

    fn card(link_only: bool) -> (StationKey, net::Announce) {
        let k = StationKey::generate().unwrap();
        let c = Card { station: k.id(), head: 3, name: "Kept Close".into(), about: "by its link".into(), port: None, live: false, at_ms: now_ms(), pow: pow_find(&k.id()), since: 1, proof: None, cover: None, accent: None, kinds: vec![], mature: false, tip: None, explicit: false, tracks: None, named: None, freq: None, link_only };
        let a = c.sign(&k);
        (k, a)
    }

    #[test]
    fn a_tagged_note_opens_only_with_its_station_and_hour_and_names_neither() {
        let (k, a) = card(true);
        let now = now_ms();
        let hour = now / net::TAG_SPAN_MS;
        let inner = Gossip::Station(a);
        let g = Gossip::tagged(&inner, now).expect("a card goes sealed");
        let raw = postcard::to_stdvec(&g).unwrap();
        for needle in [&k.id().0[..], b"Kept Close", b"by its link"] {
            assert!(!raw.windows(needle.len()).any(|w| w == needle), "nothing of the station shows");
        }
        assert!(matches!(g.open_tagged(&k.id(), hour), Some(Gossip::Station(x)) if x.station == k.id()));
        assert!(g.open_tagged(&k.id(), hour + 1).is_none(), "another hour's key does not open it");
        let other = StationKey::generate().unwrap().id();
        assert!(g.open_tagged(&other, hour).is_none(), "nor another station's");
        assert_ne!(tag_of(&k.id(), hour), tag_of(&k.id(), hour + 1), "the tag turns each hour");
        // A card and a carrier note are sealed to the same steps of size.
        let carried = Gossip::tagged(&Gossip::carried(&[3; 32], k.id(), 3, now), now).unwrap();
        let (Gossip::Tagged { ct: a, .. }, Gossip::Tagged { ct: b, .. }) = (&g, &carried) else { unreachable!() };
        assert_eq!(a.len() % 512, b.len() % 512);
        // A plain carrier note is never sealed: it is believed only from its own node.
        assert!(Gossip::tagged(&Gossip::Carrier { station: k.id(), node: [3; 32], head: 3 }, now).is_none());
    }

    #[test]
    fn a_tagged_note_that_lies_about_its_station_is_refused() {
        let (k, _) = card(true);
        let (k2, a2) = card(true);
        let now = now_ms();
        let hour = now / net::TAG_SPAN_MS;
        // Sealed under k's tag, holding k2's card: whoever opens it with k's id refuses it.
        let g = Gossip::tagged(&Gossip::Station(a2), now).unwrap();
        let Gossip::Tagged { ct, .. } = g else { unreachable!() };
        let forged = Gossip::Tagged { tag: tag_of(&k.id(), hour), ct };
        assert!(forged.open_tagged(&k.id(), hour).is_none());
        assert!(forged.open_tagged(&k2.id(), hour).is_none(), "and its own station's tag is not the one it is filed under");
    }

    #[test]
    fn notes_keep_tagged_notes_by_tag_and_hand_on_the_freshest() {
        let mut n = net::Notes::default();
        let now = now_ms();
        let (k, a) = card(true);
        let g = Gossip::tagged(&Gossip::Station(a), now).unwrap();
        let raw = postcard::to_stdvec(&g).unwrap();
        n.keep(&g, &raw, now);
        n.keep(&g, &raw, now);
        let tags: Vec<[u8; 16]> = tag_hours(now).iter().map(|h| tag_of(&k.id(), *h)).collect();
        assert_eq!(n.tagged_for(&tags, now).len(), 1, "the same note once");
        assert_eq!(n.tagged_fresh(now, 10).len(), 1);
        assert!(n.tagged_for(&tags, now + 20 * 60_000).is_empty(), "and not past its time");
        for i in 0..20u8 {
            let junk = Gossip::Tagged { tag: tag_of(&k.id(), now / net::TAG_SPAN_MS), ct: vec![i; 64] };
            n.keep(&junk, &postcard::to_stdvec(&junk).unwrap(), now + u64::from(i));
        }
        assert!(n.tagged_for(&tags, now).len() <= 8, "a flood under one tag keeps eight");
    }
}
