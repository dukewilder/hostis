//! Introduced doors and duty by need: the rules with no network in them.
//!
//! A listener's app door says only that it is there (a presence note with no address). A tab
//! already on the network asks a middle for a way in, through its own route; the middle draws a
//! door from the presence notes it holds, weighted towards light load, and hands the tab a
//! ticket for that door alone, so the tab never chooses which door it reaches. The tab's
//! WebRTC offer then goes to the door sealed, through the same middle, and the door answers
//! with its own. Doors take a few introductions an hour and middles a few a minute from any
//! one hop before them, so listing the doors costs many slow requests, each by a fresh route.
//!
//! Each listener's node takes the door job and the hop job only while the network needs them:
//! it opens when the radios doing that job are busy or too few, closes when they have been idle
//! for half an hour and there are enough of them, and never holds a job longer than six hours
//! before resting at least as long. Every threshold is jittered, so nodes do not move together.
//!
//! No rights reserved.

use std::collections::{HashMap, VecDeque};

use serde::{Deserialize, Serialize};

/// Introductions one door takes in an hour; the rest are refused.
pub const DOOR_INTROS_HOUR: usize = 12;
/// Introductions one door takes in an hour from any one middle.
pub const DOOR_INTROS_MIDDLE: usize = 4;
/// Draws and introductions a middle passes on in a minute for any one hop before it.
pub const MIDDLE_PER_MINUTE: usize = 4;
/// How long a middle's ticket for a door holds.
pub const TICKET_MS: u64 = 60_000;

/// One door a middle may draw: its node, the tabs it serves now and the most it takes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Door {
    pub node: [u8; 32],
    pub load: u16,
    pub limit: u16,
}

/// Draws the door for one introduction from what the middle holds: never one of `not` (the
/// middle itself and the hop before it), never one with no room, and the lighter a door's load
/// the likelier it is drawn (its free room, squared). `r` is the caller's randomness. Nothing
/// the tab sends goes into the draw.
pub fn draw(pool: &[Door], not: &[[u8; 32]], r: u64) -> Option<[u8; 32]> {
    let weight = |d: &Door| {
        let room = d.limit.saturating_sub(d.load) as u64;
        room * room
    };
    let open: Vec<(&Door, u64)> = pool.iter().filter(|d| d.limit > 0 && d.load < d.limit && !not.contains(&d.node)).map(|d| (d, weight(d))).collect();
    let total: u64 = open.iter().map(|x| x.1).sum();
    if total == 0 {
        return None;
    }
    let mut at = r % total;
    for (d, w) in open {
        if at < w {
            return Some(d.node);
        }
        at -= w;
    }
    None
}

/// At most `max` in any `span_ms`.
#[derive(Clone, Debug)]
pub struct Window {
    span_ms: u64,
    max: usize,
    times: VecDeque<u64>,
}

impl Window {
    pub fn new(span_ms: u64, max: usize) -> Self {
        Window { span_ms, max, times: VecDeque::new() }
    }

    /// Whether one more is let through at `now`, counting it if so.
    pub fn take(&mut self, now: u64) -> bool {
        while self.times.front().is_some_and(|t| now.saturating_sub(*t) >= self.span_ms) {
            self.times.pop_front();
        }
        if self.times.len() >= self.max {
            return false;
        }
        self.times.push_back(now);
        true
    }

    pub fn used(&self, now: u64) -> usize {
        self.times.iter().filter(|t| now.saturating_sub(**t) < self.span_ms).count()
    }
}

/// A window for each radio that asks, at most `cap` remembered (the quietest go first).
#[derive(Clone, Debug)]
pub struct PerRadio {
    span_ms: u64,
    max: usize,
    cap: usize,
    by: HashMap<[u8; 32], Window>,
}

impl PerRadio {
    pub fn new(span_ms: u64, max: usize, cap: usize) -> Self {
        PerRadio { span_ms, max, cap, by: HashMap::new() }
    }

    pub fn take(&mut self, from: &[u8; 32], now: u64) -> bool {
        if !self.by.contains_key(from) && self.by.len() >= self.cap {
            self.by.retain(|_, w| w.used(now) > 0);
            if self.by.len() >= self.cap {
                return false;
            }
        }
        self.by.entry(*from).or_insert_with(|| Window::new(self.span_ms, self.max)).take(now)
    }
}

/// A middle's tickets: each names the door it drew and the hop that asked, and goes once.
#[derive(Default, Debug)]
pub struct Tickets {
    by: HashMap<[u8; 16], ([u8; 32], [u8; 32], u64)>,
}

impl Tickets {
    /// A ticket for `door`, drawn for the hop `from`.
    pub fn issue(&mut self, ticket: [u8; 16], door: [u8; 32], from: [u8; 32], now: u64) {
        self.by.retain(|_, (_, _, t)| now.saturating_sub(*t) < TICKET_MS);
        if self.by.len() < 4_096 {
            self.by.insert(ticket, (door, from, now));
        }
    }

    /// The door a ticket was drawn for, if the same hop brings it back in time. Once only: a
    /// made-up ticket, another hop's or a spent one goes nowhere.
    pub fn take(&mut self, ticket: &[u8; 16], from: &[u8; 32], now: u64) -> Option<[u8; 32]> {
        let (door, by, at) = self.by.remove(ticket)?;
        (by == *from && now.saturating_sub(at) < TICKET_MS).then_some(door)
    }
}

/// The tab's half of a link to a door: its ICE name and password, its certificate's
/// fingerprint and its candidates as the browser wrote them. Sealed to the door.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Offer {
    pub ufrag: String,
    pub pwd: String,
    pub fp: [u8; 32],
    pub cands: Vec<String>,
}

/// The door's half, sealed back to the tab: its own ICE name and password, its certificate's
/// fingerprint, the addresses the tab may try (only those the network already sees it at), and
/// when this spell of door duty began.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Answer {
    pub ufrag: String,
    pub pwd: String,
    pub fp: [u8; 32],
    pub cands: Vec<String>,
    pub since: u64,
}

fn fine(s: &str, min: usize) -> bool {
    s.len() >= min && s.len() <= 256 && s.bytes().all(|c| c.is_ascii_graphic())
}

impl Offer {
    /// Within what a browser writes: names and passwords of plain characters, at most twelve
    /// candidates of a line each.
    pub fn sound(&self) -> bool {
        fine(&self.ufrag, 4) && fine(&self.pwd, 22) && self.cands.len() <= 12 && self.cands.iter().all(|c| c.len() <= 300 && c.is_ascii())
    }
}

impl Answer {
    pub fn sound(&self) -> bool {
        fine(&self.ufrag, 4) && fine(&self.pwd, 22) && !self.cands.is_empty() && self.cands.len() <= 6 && self.cands.iter().all(|c| c.len() <= 64 && c.parse::<std::net::SocketAddr>().is_ok())
    }
}

/// What a node sees of the radios doing one job: how many (not counting itself), their
/// average load as a share of what each takes, and how many the network needs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Seen {
    pub known: usize,
    pub load: f64,
    pub target: usize,
}

/// How many radios one job needs for `tabs` counted on the network: one for each `per`, and
/// never fewer than `floor`.
pub fn target(tabs: u64, per: u64, floor: usize) -> usize {
    (tabs.div_ceil(per.max(1)) as usize).max(floor).min(4_096)
}

/// Above this share of their load the radios doing a job are busy, and another opens.
pub const OPEN_LOAD: f64 = 0.5;
/// Under this share for `QUIET_MS`, with enough others, one closes.
pub const CLOSE_LOAD: f64 = 0.1;
pub const QUIET_MS: u64 = 30 * 60_000;
/// The longest spell of one job; the rest after it is at least as long.
pub const SPELL_MS: u64 = 6 * 3_600_000;

/// What a tick decided.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Change {
    None,
    Open,
    /// Closed; `rest` when the spell ran its six hours and a rest begins.
    Close { rest: bool },
}

/// One job's state on one node.
#[derive(Clone, Debug)]
pub struct Duty {
    pub on: bool,
    pub since: u64,
    pub rest_until: u64,
    quiet_since: Option<u64>,
    /// This spell's jitter on the open load, close load, quiet time, target and spell length.
    jit: [f64; 5],
    pub spells: u32,
}

/// A factor between 0.8 and 1.2 from eight bits of `r`.
fn wobble(r: u64, i: u32) -> f64 {
    0.8 + ((r >> (8 * i)) & 0xff) as f64 / 255.0 * 0.4
}

impl Duty {
    pub fn new(r: u64) -> Self {
        Duty { on: false, since: 0, rest_until: 0, quiet_since: None, jit: Self::draw(r), spells: 0 }
    }

    fn draw(r: u64) -> [f64; 5] {
        [wobble(r, 0), wobble(r, 1), wobble(r, 2), wobble(r, 3), wobble(r, 4)]
    }

    /// The thresholds this spell goes by: open load, close load, quiet time, target, spell.
    pub fn thresholds(&self, target: usize) -> (f64, f64, u64, usize, u64) {
        let j = self.jit;
        (
            (OPEN_LOAD * j[0]).min(0.9),
            CLOSE_LOAD * j[1],
            (QUIET_MS as f64 * j[2]) as u64,
            ((target as f64 * j[3]).round() as usize).max(1),
            // Never past six hours: the jitter only shortens a spell.
            (SPELL_MS as f64 * j[4].min(1.0)) as u64,
        )
    }

    /// Decides at `now`: `allowed` is everything the owner and the device allow (a computer, or
    /// a phone on Wi-Fi while charging, not through Tor, carrying on and the switch not off),
    /// `seen` the radios doing this job, and `r` fresh randomness for the next spell's jitter.
    pub fn tick(&mut self, now: u64, allowed: bool, seen: &Seen, r: u64) -> Change {
        let (open_at, close_at, quiet, target, spell) = self.thresholds(seen.target);
        if !allowed {
            self.quiet_since = None;
            if self.on {
                self.on = false;
                return Change::Close { rest: false };
            }
            return Change::None;
        }
        if self.on {
            let held = now.saturating_sub(self.since);
            if held >= spell {
                // At least as long as the spell, and a little more by the jitter.
                self.on = false;
                self.quiet_since = None;
                self.rest_until = now + (held as f64 * (1.0 + 0.25 * (wobble(r, 5) - 0.8) / 0.4)) as u64;
                self.spells += 1;
                self.jit = Self::draw(r);
                return Change::Close { rest: true };
            }
            if seen.load < close_at && seen.known >= target {
                let q = *self.quiet_since.get_or_insert(now);
                if now.saturating_sub(q) >= quiet {
                    self.on = false;
                    self.quiet_since = None;
                    self.spells += 1;
                    self.jit = Self::draw(r);
                    return Change::Close { rest: false };
                }
            } else {
                self.quiet_since = None;
            }
            return Change::None;
        }
        if now < self.rest_until {
            return Change::None;
        }
        if seen.load > open_at || seen.known < target {
            self.on = true;
            self.since = now;
            self.quiet_since = None;
            return Change::Open;
        }
        Change::None
    }

    /// Resting after a full spell.
    pub fn resting(&self, now: u64) -> bool {
        !self.on && now < self.rest_until
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn door(n: u8, load: u16, limit: u16) -> Door {
        Door { node: [n; 32], load, limit }
    }

    #[test]
    fn a_middle_draws_light_doors_and_never_itself_or_the_hop_before_it() {
        let pool = [door(1, 0, 64), door(2, 60, 64), door(3, 64, 64), door(4, 0, 64), door(5, 0, 64)];
        let mut by = HashMap::new();
        for r in 0..20_000u64 {
            let d = draw(&pool, &[[4; 32], [5; 32]], r.wrapping_mul(0x9E37_79B9_7F4A_7C15)).unwrap();
            *by.entry(d[0]).or_insert(0) += 1;
        }
        assert!(!by.contains_key(&4) && !by.contains_key(&5), "never the middle itself or the hop before it");
        assert!(!by.contains_key(&3), "never a full door");
        assert!(by[&1] > 100 * by.get(&2).copied().unwrap_or(1).max(1) / 2, "the light door far more often than the busy one: {by:?}");
        assert_eq!(draw(&[door(3, 64, 64)], &[], 7), None);
        assert_eq!(draw(&[door(1, 0, 0)], &[], 7), None, "a radio taking no tabs is no door");
    }

    #[test]
    fn a_ticket_goes_only_to_the_door_the_middle_drew() {
        let mut t = Tickets::default();
        let (guard, other, drawn) = ([1u8; 32], [2u8; 32], [9u8; 32]);
        t.issue([7; 16], drawn, guard, 1_000);
        // The tab cannot name a door: a ticket it made up goes nowhere.
        assert_eq!(t.take(&[8; 16], &guard, 1_500), None);
        // Another hop bringing the ticket goes nowhere, and the ticket is spent.
        assert_eq!(t.take(&[7; 16], &other, 1_500), None);
        assert_eq!(t.take(&[7; 16], &guard, 1_500), None);
        t.issue([6; 16], drawn, guard, 1_000);
        assert_eq!(t.take(&[6; 16], &guard, 1_000 + TICKET_MS), None, "a minute and it lapses");
        t.issue([5; 16], drawn, guard, 1_000);
        assert_eq!(t.take(&[5; 16], &guard, 2_000), Some(drawn), "the drawn door, once");
        assert_eq!(t.take(&[5; 16], &guard, 2_000), None);
    }

    #[test]
    fn rate_limits_hold() {
        // A door takes twelve introductions an hour.
        let mut w = Window::new(3_600_000, DOOR_INTROS_HOUR);
        assert!((0..12).all(|i| w.take(i * 1_000)));
        assert!(!w.take(20_000), "the thirteenth in the hour is refused");
        assert!(w.take(3_600_000 + 1), "an hour on, one more");
        // A middle passes a few a minute for each hop before it, the others unaffected.
        let mut p = PerRadio::new(60_000, MIDDLE_PER_MINUTE, 3);
        assert!((0..4).all(|_| p.take(&[1; 32], 0)));
        assert!(!p.take(&[1; 32], 10), "a fifth from one hop in the minute is refused");
        assert!(p.take(&[2; 32], 10), "another hop has its own");
        // A crowd of new hops cannot push the busy ones out.
        assert!(p.take(&[3; 32], 20));
        assert!(!p.take(&[4; 32], 30), "past the cap, no room while all are busy");
        assert!(p.take(&[1; 32], 60_000), "a minute on, again");
    }

    #[test]
    fn an_offer_and_an_answer_hold_only_plain_fields() {
        let o = Offer { ufrag: "abcd".into(), pwd: "p".repeat(22), fp: [1; 32], cands: vec!["candidate:1 1 udp 1 198.51.100.1 4000 typ srflx".into()] };
        assert!(o.sound());
        assert!(!Offer { pwd: "short".into(), ..o.clone() }.sound());
        assert!(!Offer { cands: vec!["x".into(); 13], ..o.clone() }.sound());
        let a = Answer { ufrag: "abcd".into(), pwd: "p".repeat(22), fp: [1; 32], cands: vec!["198.51.100.1:4000".into()], since: 1 };
        assert!(a.sound());
        assert!(!Answer { cands: vec![], ..a.clone() }.sound(), "an answer with nowhere to try is none");
        assert!(!Answer { cands: vec!["door.example:4000".into()], ..a.clone() }.sound(), "addresses only, never names to look up");
    }

    fn seen(known: usize, load: f64, target: usize) -> Seen {
        Seen { known, load, target }
    }

    #[test]
    fn duty_opens_under_load_and_closes_when_idle_with_jitter() {
        // Too few doors known: it opens at once.
        let mut d = Duty::new(0x1234_5678_9abc_def0);
        assert_eq!(d.tick(0, true, &seen(0, 0.0, 3), 1), Change::Open);
        // Not allowed: it closes at once and stays shut.
        assert_eq!(d.tick(1, false, &seen(0, 0.0, 3), 1), Change::Close { rest: false });
        assert_eq!(d.tick(2, false, &seen(0, 0.0, 3), 1), Change::None);
        // Enough doors and idle: it stays shut; busy ones and it opens.
        let mut d = Duty::new(42);
        assert_eq!(d.tick(0, true, &seen(10, 0.05, 3), 1), Change::None);
        assert_eq!(d.tick(60_000, true, &seen(10, 0.95, 3), 1), Change::Open);
        // Idle for half an hour (jittered) with enough others, and it closes.
        let (_, _, quiet, _, _) = d.thresholds(3);
        assert_eq!(d.tick(120_000, true, &seen(10, 0.0, 3), 1), Change::None);
        assert_eq!(d.tick(120_000 + quiet - 1, true, &seen(10, 0.0, 3), 1), Change::None);
        assert_eq!(d.tick(120_000 + quiet, true, &seen(10, 0.0, 3), 1), Change::Close { rest: false });
        // Idle but too few others: it stays open.
        let mut d = Duty::new(42);
        assert_eq!(d.tick(0, true, &seen(0, 0.0, 3), 1), Change::Open);
        assert_eq!(d.tick(2 * QUIET_MS, true, &seen(1, 0.0, 3), 1), Change::None);
        // Six hours at most, then a rest at least as long, and a new spell only after it.
        let mut d = Duty::new(7);
        assert_eq!(d.tick(0, true, &seen(0, 0.9, 3), 1), Change::Open);
        let (_, _, _, _, spell) = d.thresholds(3);
        assert!(spell <= SPELL_MS && spell >= SPELL_MS * 8 / 10);
        assert_eq!(d.tick(spell, true, &seen(0, 0.9, 3), 0xffff_ffff_ffff), Change::Close { rest: true });
        assert!(d.rest_until >= 2 * spell, "the rest is at least as long as the spell");
        assert!(d.resting(spell + 1));
        assert_eq!(d.tick(d.rest_until - 1, true, &seen(0, 0.9, 3), 1), Change::None);
        assert_eq!(d.tick(d.rest_until, true, &seen(0, 0.9, 3), 1), Change::Open);
        // Jitter: a hundred nodes draw a spread of thresholds, so they do not move together.
        let mut opens: Vec<u64> = (0..100u64).map(|i| (Duty::new(i.wrapping_mul(0x9E37_79B9_7F4A_7C15)).thresholds(3).0 * 1000.0) as u64).collect();
        opens.sort();
        opens.dedup();
        assert!(opens.len() > 30, "open thresholds spread: {}", opens.len());
        let quiets: std::collections::HashSet<u64> = (0..100u64).map(|i| Duty::new(i.wrapping_mul(0x9E37_79B9_7F4A_7C15)).thresholds(3).2 / 1000).collect();
        assert!(quiets.len() > 30);
        // A node at 0.55 opens or not by its own jitter: some do and some do not.
        let at = (0..100u64).filter(|i| Duty::new(i.wrapping_mul(0x9E37_79B9_7F4A_7C15)).tick(0, true, &seen(10, 0.55, 3), 1) == Change::Open).count();
        assert!(at > 5 && at < 95, "{at} of 100 opened at 0.55");
    }

    #[test]
    fn a_target_grows_with_the_tabs() {
        assert_eq!(target(0, 16, 3), 3);
        assert_eq!(target(100, 16, 3), 7);
        assert_eq!(target(17, 16, 1), 2);
    }
}
