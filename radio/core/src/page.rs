//! A station's page: its programme, shows, notices and what plays next, and the JSON the
//! app writes them with. Hashes and keys travel as hex; modes and fills by name. Shared by
//! the phone's node and the browser, so both show the same page from the same log.
//!
//! No rights reserved.

use serde_json::{Value, json};

use crate::{Archive, Body, Hash};
use crate::programme::{Fill, Mode, Programme, Slot, Span, Zone};

/// A malformed programme or page request, in words fit for the keeper.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct Bad(pub String);

type Result<T> = std::result::Result<T, Bad>;

fn bad<T>(s: impl Into<String>) -> Result<T> {
    Err(Bad(s.into()))
}

pub fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

pub fn unhex(s: &str) -> Result<[u8; 32]> {
    crate::unhex32(s).ok_or_else(|| Bad("not a 64-character hex string".into()))
}

/// A track's title and who it is by, from its entry hash. Empty if the log has no such track.
pub fn describe_track(a: &Archive, track: &Hash) -> (String, String) {
    match a.track_entry(track).map(|e| e.entry.body()) {
        Some(Body::Track { title, by, .. }) => (title.clone(), by.clone()),
        _ => Default::default(),
    }
}

fn span_json(s: &Span) -> Value {
    json!({ "days": s.days, "start_min": s.start_min, "len_min": s.len_min })
}

fn span_from(v: &Value) -> Result<Span> {
    let Some(len) = v["len_min"].as_u64() else { return bad("a span needs a length") };
    let len = len.clamp(1, 7 * 1440) as u16;
    // Masked before it is narrowed, and refused if it names no day: a span that never runs
    // would leave a station dark with no word why.
    let days = (v["days"].as_u64().unwrap_or(0x7f) & 0x7f) as u8;
    if days == 0 {
        return bad("pick at least one day");
    }
    Ok(Span { days, start_min: (v["start_min"].as_u64().unwrap_or(0) % 1440) as u16, len_min: len })
}

pub fn programme_json(p: &Programme) -> Value {
    let mode = match &p.mode {
        Mode::Shuffle => json!({ "kind": "shuffle" }),
        Mode::InOrder => json!({ "kind": "in_order" }),
        Mode::NewestFirst => json!({ "kind": "newest_first" }),
        Mode::Daily { start_min } => json!({ "kind": "daily", "start_min": start_min }),
        Mode::Weekly { day, start_min } => json!({ "kind": "weekly", "day": day, "start_min": start_min }),
    };
    let slots: Vec<Value> = p
        .slots
        .iter()
        .map(|s| {
            let fill = match &s.fill {
                Fill::Library => json!({ "kind": "library" }),
                Fill::Tracks { tracks, shuffle } => json!({ "kind": "tracks", "tracks": tracks.iter().map(|h| hex(h)).collect::<Vec<_>>(), "shuffle": shuffle }),
                Fill::Hand { key } => json!({ "kind": "hand", "key": hex(key) }),
                Fill::Newest { count } => json!({ "kind": "newest", "count": count }),
            };
            json!({ "title": s.title, "about": s.about, "span": span_json(&s.span), "fill": fill, "live": s.live })
        })
        .collect();
    json!({
        "zone_name": p.zone.name, "zone": p.zone.offsets.iter().map(|(t, o)| json!([t, o])).collect::<Vec<_>>(),
        "mode": mode, "hours": p.hours.iter().map(span_json).collect::<Vec<_>>(), "slots": slots,
        "ident": p.ident.map(|h| hex(&h)), "ident_every_min": p.ident_every_min,
    })
}

pub fn programme_from(v: &Value) -> Result<Programme> {
    let mut offsets: Vec<(u64, i16)> = v["zone"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|x| Some((x.get(0)?.as_u64()?, x.get(1)?.as_i64()?.clamp(-14 * 60, 14 * 60) as i16)))
        .take(400)
        .collect();
    offsets.sort_by_key(|o| o.0);
    let m = &v["mode"];
    let mode = match m["kind"].as_str().unwrap_or("shuffle") {
        "shuffle" => Mode::Shuffle,
        "in_order" => Mode::InOrder,
        "newest_first" => Mode::NewestFirst,
        "daily" => Mode::Daily { start_min: (m["start_min"].as_u64().unwrap_or(0) % 1440) as u16 },
        "weekly" => Mode::Weekly { day: (m["day"].as_u64().unwrap_or(0) % 7) as u8, start_min: (m["start_min"].as_u64().unwrap_or(0) % 1440) as u16 },
        k => return bad(format!("unknown mode {k}")),
    };
    let hours = v["hours"].as_array().into_iter().flatten().take(14).map(span_from).collect::<Result<Vec<_>>>()?;
    let mut slots = Vec::new();
    for s in v["slots"].as_array().into_iter().flatten().take(64) {
        let f = &s["fill"];
        let fill = match f["kind"].as_str().unwrap_or("library") {
            "library" => Fill::Library,
            "tracks" => Fill::Tracks {
                tracks: f["tracks"].as_array().into_iter().flatten().filter_map(|h| h.as_str().and_then(|h| unhex(h).ok())).take(500).collect(),
                shuffle: f["shuffle"].as_bool().unwrap_or(false),
            },
            "hand" => Fill::Hand { key: unhex(f["key"].as_str().unwrap_or(""))? },
            "newest" => Fill::Newest { count: f["count"].as_u64().unwrap_or(1).clamp(1, 500) as u16 },
            k => return bad(format!("unknown fill {k}")),
        };
        slots.push(Slot {
            title: s["title"].as_str().unwrap_or("").chars().take(80).collect(),
            about: s["about"].as_str().unwrap_or("").chars().take(400).collect(),
            span: span_from(&s["span"])?,
            fill,
            live: s["live"].as_bool().unwrap_or(false),
        });
    }
    Ok(Programme {
        zone: Zone { name: v["zone_name"].as_str().unwrap_or("").chars().take(64).collect(), offsets },
        mode,
        hours,
        slots,
        ident: v["ident"].as_str().and_then(|h| unhex(h).ok()),
        ident_every_min: v["ident_every_min"].as_u64().unwrap_or(30).clamp(1, 1440) as u16,
    })
}

/// Everything a station's page shows, worked out from its log alone.
pub fn page_of(a: &Archive, now: u64, days: u64) -> Result<Value> {
    let clock = crate::clock::Clock::new(a).map_err(|e| Bad(e.to_string()))?;
    // Each track's words and each show's list, found once for the whole page rather than by
    // hashing the whole log again for every line of it.
    let names: std::collections::HashMap<Hash, (String, String)> = a
        .entries()
        .iter()
        .filter_map(|e| match e.entry.body() {
            Body::Track { title, by, .. } => Some((e.hash().ok()?, (title.clone(), by.clone()))),
            _ => None,
        })
        .collect();
    let named = |t: &Hash| names.get(t).cloned().unwrap_or_default();
    let shows: std::collections::HashMap<Hash, &Body> = a.shows().into_iter().collect();
    let until = now.saturating_add(days.min(60).saturating_mul(86_400_000));
    let listings: Vec<Value> = clock
        .listings(now.saturating_sub(3 * 3_600_000), until)
        .into_iter()
        .filter(|l| l.end_ms > now)
        .take(200)
        .map(|l| {
            let tracks: Vec<Value> = match l.show.and_then(|h| shows.get(&h).copied()) {
                Some(Body::Show { tracks, .. }) => tracks.iter().map(|t| json!({ "track": hex(t), "title": named(t).0 })).collect(),
                _ => vec![],
            };
            json!({ "start_ms": l.start_ms, "end_ms": l.end_ms, "title": l.title, "about": l.about, "kind": l.kind, "live": l.live, "show": l.show.map(|h| hex(&h)), "tracks": tracks })
        })
        .collect();
    let next: Vec<Value> = clock
        .upcoming(now, now + 6 * 3_600_000)
        .into_iter()
        .take(8)
        .map(|(start, track, dur)| {
            let (title, by) = named(&track);
            json!({ "start_ms": start, "duration_ms": dur, "title": title, "by": by, "track": hex(&track) })
        })
        .collect();
    let notices: Vec<Value> = a.notices(now).into_iter().take(10).map(|(h, at, text)| json!({ "id": hex(&h), "at_ms": at, "text": text })).collect();
    let on = clock.now(now);
    let programme = a.programme();
    let (name, about, port, tip) = match a.colours() {
        Some(Body::Colours { name, about, port, tip }) => (name.clone(), about.clone(), port.clone(), tip.clone()),
        _ => Default::default(),
    };
    let tracks = a.library().map(|l| l.len()).unwrap_or(0);
    // When an off-air station comes back.
    let back = if on.is_none() { clock.next_on(now, now + 7 * 86_400_000).map(|b| b.0) } else { None };
    Ok(json!({
        "id": a.station().to_hex(), "name": name, "about": about, "port": port, "tip": tip,
        "since": a.entries().first().map(|e| e.entry.at_ms),
        "tracks": tracks,
        "on_air": on.is_some(), "back_at": back,
        "now": on.as_ref().map(|np| { let (t, b) = named(&np.track); json!({ "title": t, "by": b, "slot": np.slot, "position_ms": np.offset_ms, "duration_ms": np.duration_ms }) }),
        "programme": programme.as_ref().map(programme_json), "zone_name": programme.as_ref().map(|p| p.zone.name.clone()),
        // The last change the station's time-zone table knows; past it the local time keeps its last offset.
        "zone_until": programme.as_ref().and_then(|p| p.zone.offsets.last().map(|o| o.0)).filter(|u| *u > 0),
        "listings": listings, "next": next, "notices": notices,
        "explicit": a.explicit(), "details": a.details().map(|(k, m)| json!({ "kinds": k, "mature": m })),
    }))
}

