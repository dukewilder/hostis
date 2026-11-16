//! Duty by need: which jobs this listener's device takes, decided by the device itself.
//!
//! The app says what the device may do (a computer, or a phone on Wi-Fi while it charges, not
//! through Tor, carrying on and the owner's door switch not off). Within that, every few minutes
//! the node looks at the radios already doing each job and takes it only while the network needs
//! it (see `pyrate_core::doorway::Duty`): the door job from the load of the doors it knows and how
//! many the tabs the network counts would want, the hop job (middle and way in) from the load of
//! the middles it knows. A spell of either lasts at most six hours, then the device rests at
//! least as long; a door takes a new certificate and port for each spell. Lanterns and servers
//! hold both jobs always. Ballast follows the fleet's size, as before.
//!
//! No rights reserved.

use std::{
    net::SocketAddr,
    sync::{Arc, atomic::Ordering},
    time::Duration,
};

use pyrate_core::doorway::{Change, Duty, Seen, target};
use serde_json::{Value, json};

use crate::{Inner, door, net::Gossip, now_ms, say};

/// Tabs one door is wanted for, and the fewest doors the network keeps; the same for middles.
const TABS_PER_DOOR: u64 = 16;
const DOORS_FLOOR: usize = 3;
const TABS_PER_MIDDLE: u64 = 8;
const MIDDLES_FLOOR: usize = 6;
/// Hops a middle takes in a minute: a listener's device, and a Lantern.
const HOP_LIMIT: u16 = 600;
const HOP_LIMIT_LANTERN: u16 = 6_000;

/// What the app allows and what this device is doing.
pub(crate) struct Duties {
    /// The app's word that this device may be a door, where it reaches it, and whether links
    /// may name it.
    pub door_allowed: bool,
    pub door_public: Option<SocketAddr>,
    pub door_links: bool,
    pub door: Duty,
    pub hops: Duty,
    /// The last tick, for the status.
    pub ticked: u64,
}

impl Duties {
    pub fn new() -> Self {
        let mut hops = Duty::new(rand::random());
        // A node starts on the hop job, as it would open it anyway with nothing known yet; it
        // lets it go once it has been idle with enough others.
        hops.on = true;
        hops.since = now_ms();
        Duties { door_allowed: false, door_public: None, door_links: false, door: Duty::new(rand::random()), hops, ticked: 0 }
    }
}

/// How often duty is decided: every three minutes or so, sooner on a test network.
fn every() -> Duration {
    let base = std::env::var("PYRATE_DUTY_MS").ok().and_then(|v| v.parse().ok()).unwrap_or(180_000u64);
    Duration::from_millis(base * 8 / 10 + rand::random::<u64>() % (base * 4 / 10).max(1))
}

impl Inner {
    /// This radio's presence: as a door taking introductions while one is open, and as a middle
    /// while it holds the hop job. None while running dark, or doing neither. No address is in it.
    pub(crate) fn presence_note(&self) -> Option<Gossip> {
        if self.dark {
            return None;
        }
        let door = self.door.lock().unwrap().clone().filter(|d| d.intro && !d.is_closed());
        let hop = self.hop_duty.load(Ordering::Relaxed) && self.crew.load(Ordering::Relaxed) && self.relay_ok.load(Ordering::Relaxed);
        if door.is_none() && !hop {
            return None;
        }
        let now = now_ms();
        let (since, load, limit, map) = door.map(|d| (d.since.min(now), d.clients().min(door::MAX_SESSIONS) as u16, door::MAX_SESSIONS as u16, d.map)).unwrap_or((0, 0, 0, false));
        let (hops, hop_limit) = if hop { (self.hops_lately(now), if self.cfg.lantern { HOP_LIMIT_LANTERN } else { HOP_LIMIT }) } else { (0, 0) };
        let relay = self.endpoint.addr().relay_urls().next().map(|u| u.to_string()).filter(|r| crate::net::relay_named(r)).unwrap_or_default();
        Some(Gossip::presence(&self.secret, since, load, limit, map, hops.min(hop_limit), hop_limit, &relay, now))
    }

    /// One more hop passed on as a middle or a way in.
    pub(crate) fn hop_counted(&self) {
        let m = now_ms() / 60_000;
        let mut h = self.hops_minute.lock().unwrap();
        if h.0 != m {
            *h = (m, 0, if h.0 + 1 == m { h.1 } else { 0 });
        }
        h.1 = h.1.saturating_add(1);
    }

    /// Hops passed on in the last full minute, or this one if it is busier.
    fn hops_lately(&self, now: u64) -> u16 {
        let m = now / 60_000;
        let h = self.hops_minute.lock().unwrap();
        match m.saturating_sub(h.0) {
            0 => h.1.max(h.2),
            1 => h.1,
            _ => 0,
        }
    }

    /// The listeners the network counts, over every station this node knows.
    fn tabs_counted(&self, now: u64) -> u64 {
        self.log.lock().unwrap().values().map(|l| l.aboard(now)).sum()
    }

    /// What this node sees of the doors and of the middles, itself left out.
    fn duty_seen(&self, now: u64) -> (Seen, Seen) {
        let me = *self.endpoint.id().as_bytes();
        let tabs = self.tabs_counted(now);
        let notes = self.notes.lock().unwrap();
        let pres: Vec<_> = notes.presence(now).into_iter().filter(|p| p.node != me).collect();
        let doors: Vec<_> = pres.iter().filter(|p| p.limit > 0).collect();
        // Listed doors that say nothing of their load count towards how many there are.
        let listed = notes.doors_listed(now).into_iter().filter(|(n, _)| *n != me && !doors.iter().any(|p| p.node == *n)).count();
        let share = |v: &[f64]| if v.is_empty() { 0.0 } else { v.iter().sum::<f64>() / v.len() as f64 };
        let door_load: Vec<f64> = doors.iter().map(|p| p.load as f64 / p.limit as f64).collect();
        let mids: Vec<f64> = pres.iter().filter(|p| p.hop_limit > 0).map(|p| p.hops as f64 / p.hop_limit as f64).collect();
        (
            Seen { known: doors.len() + listed, load: share(&door_load), target: target(tabs, TABS_PER_DOOR, DOORS_FLOOR) },
            Seen { known: mids.len(), load: share(&mids), target: target(tabs, TABS_PER_MIDDLE, MIDDLES_FLOOR) },
        )
    }

    /// Decides both jobs now.
    pub(crate) async fn duty_tick(self: &Arc<Self>) {
        let now = now_ms();
        if self.cfg.lantern {
            // A Lantern is extra help by its keeper's choice: it holds the hop job always, and its
            // door is opened and closed by its keeper alone.
            self.hop_duty.store(true, Ordering::Relaxed);
            return;
        }
        let (doors, mids) = self.duty_seen(now);
        let (door_change, door_on, public, links, hop_on) = {
            let mut d = self.duty.lock().unwrap();
            d.ticked = now;
            let allowed = d.door_allowed && !self.dark && self.crew.load(Ordering::Relaxed);
            let c = d.door.tick(now, allowed, &doors, rand::random());
            // On mobile data a device carries nobody's traffic, but that is the crew note's own
            // gate (`relay_ok`), which can change at any moment: the job itself stays.
            let hop_allowed = self.crew.load(Ordering::Relaxed) && !self.dark;
            match d.hops.tick(now, hop_allowed, &mids, rand::random()) {
                Change::Open => say("duty: passing on routes, as the network needs more middles"),
                Change::Close { rest: true } => say("duty: six hours of passing on routes, resting now"),
                Change::Close { .. } if !hop_allowed => say("duty: the crew switch is off, so no routes are passed on"),
                Change::Close { .. } => say("duty: enough middles, this one stops"),
                Change::None => {}
            }
            (c, d.door.on, d.door_public, d.door_links, d.hops.on)
        };
        self.hop_duty.store(hop_on, Ordering::Relaxed);
        match door_change {
            Change::Open => {
                say(&format!("duty: a door, as {} are known of {} wanted at {:.0}% load", doors.known, doors.target, doors.load * 100.0));
                // A new spell, a new certificate and port.
                self.hold.remove_note(&self.cfg.dir, "door");
                let _ = self.door_open(public, true, links).await;
            }
            Change::Close { rest } => {
                let allowed = self.duty.lock().unwrap().door_allowed && !self.dark && self.crew.load(Ordering::Relaxed);
                say(if rest { "duty: the door was open six hours, resting now" } else if allowed { "duty: enough doors, this one closes" } else { "duty: the door is not allowed now, so it closes" });
                self.door_close();
                self.hold.remove_note(&self.cfg.dir, "door");
                if allowed {
                    self.door_why(if rest { door::WHY_REST } else { door::WHY_IDLE });
                }
            }
            Change::None => {
                let open = self.door.lock().unwrap().is_some();
                if door_on && !open && !self.door_opening.load(Ordering::SeqCst) && !self.dark {
                    // On duty, but the door did not open (no address yet, the port was taken):
                    // tried again at each tick.
                    let _ = self.door_open(public, true, links).await;
                } else if !door_on && !open && self.duty.lock().unwrap().door_allowed && !self.dark {
                    let resting = self.duty.lock().unwrap().door.resting(now);
                    self.door_why(if resting { door::WHY_REST } else { door::WHY_WAITING });
                }
            }
        }
    }

    /// The app's word on the door: whether this device may be one, where (a test network's
    /// address), and whether links may name it. Decided at once.
    pub(crate) fn door_allow(self: &Arc<Self>, allowed: bool, public: Option<SocketAddr>, links: bool) {
        {
            let mut d = self.duty.lock().unwrap();
            let same = d.door_allowed == allowed && d.door_public == public && d.door_links == links;
            d.door_allowed = allowed;
            d.door_public = public;
            d.door_links = links;
            if same {
                return;
            }
        }
        let i = self.clone();
        self.handle.spawn(async move { i.duty_tick().await });
    }

    /// Every few minutes, and the presence of a middle with no door open every two.
    pub(crate) fn duty_loop(self: &Arc<Self>) {
        let w = Arc::downgrade(self);
        self.handle.spawn(async move {
            tokio::time::sleep(Duration::from_secs(3)).await;
            loop {
                let Some(i) = w.upgrade() else { return };
                i.duty_tick().await;
                drop(i);
                tokio::time::sleep(every()).await;
            }
        });
        let w = Arc::downgrade(self);
        self.handle.spawn(async move {
            tokio::time::sleep(Duration::from_secs(10)).await;
            loop {
                let Some(i) = w.upgrade() else { return };
                // An open door says its presence itself, as often as its note.
                if i.door.lock().unwrap().is_none() {
                    if let Some(g) = i.presence_note() {
                        if let Ok(raw) = postcard::to_stdvec(&g) {
                            i.notes.lock().unwrap().keep(&g, &raw, now_ms());
                        }
                        i.broadcast(&g).await;
                    }
                }
                drop(i);
                tokio::time::sleep(Duration::from_secs(120)).await;
            }
        });
    }

    /// The jobs this device is doing now, for the app's one line.
    pub(crate) fn jobs(&self) -> Value {
        let now = now_ms();
        let d = self.door.lock().unwrap().clone();
        let duty = self.duty.lock().unwrap();
        let door_on = d.is_some();
        let hops = self.hop_duty.load(Ordering::Relaxed) && self.crew.load(Ordering::Relaxed) && self.relay_ok.load(Ordering::Relaxed) && !self.dark;
        let ballast = self.ballast_ok.load(Ordering::Relaxed) && !self.store.lock().unwrap().pinned.is_empty();
        let mut parts = Vec::new();
        if door_on {
            parts.push("a door for browsers");
        }
        if hops {
            parts.push("a middle for others' routes");
        }
        if ballast {
            parts.push("a keeper of ballast");
        }
        let line = match parts.len() {
            0 => "This device is only listening now.".to_string(),
            1 => format!("This device is {} now.", parts[0]),
            n => format!("This device is {} and {} now.", parts[..n - 1].join(", "), parts[n - 1]),
        };
        json!({
            "door": door_on, "listed": d.as_ref().is_some_and(|d| d.listed()), "introduced": d.as_ref().map(|d| d.introduced.load(Ordering::Relaxed)).unwrap_or(0),
            "browsers": d.as_ref().map(|d| d.clients()).unwrap_or(0),
            "hops": hops, "ballast": ballast,
            "door_allowed": duty.door_allowed, "door_resting": duty.door.resting(now), "hops_resting": duty.hops.resting(now),
            "door_since": if duty.door.on { duty.door.since } else { 0 }, "hops_since": if duty.hops.on { duty.hops.since } else { 0 },
            "ticked": duty.ticked, "middle_draws": self.introductions.load(Ordering::Relaxed),
            "line": line,
        })
    }
}
