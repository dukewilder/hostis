//! The programme: local time, tapes, hours, slots, watches and premieres.

use pyrate_core::clock::Clock;
use pyrate_core::{Archive, Body, Chunk, Fill, Mode, Programme, Slot, Span, StationKey, Zone};

const MIN: u64 = 60_000;

/// A recording of so many minutes, in six-second chunks.
fn track(title: &str, minutes: u64, seed: u8) -> Body {
    let chunk = Chunk { hash: [seed; 32], len: 10, frames: 300 };
    Body::Track { title: title.into(), by: "".into(), source: "".into(), channels: 1, bitrate: 24_000, chunks: vec![chunk; (minutes * 10) as usize] }
}

fn chicago() -> Zone {
    Zone { name: "Test/Chicago".into(), offsets: vec![(0, -360)] }
}

/// A station with three 20-minute tracks, written well before the week under test.
fn station(k: &StationKey) -> (Archive, Vec<[u8; 32]>, u64) {
    let mut a = Archive::new(k.id());
    let t0 = 1_790_000_000_000u64;
    a.write(k, t0, Body::Colours { name: "P".into(), about: "".into(), port: None, tip: None }).unwrap();
    let mut hashes = Vec::new();
    for i in 0..3u8 {
        let e = a.write(k, t0 + 1 + i as u64, track(&format!("t{i}"), 20, i)).unwrap();
        hashes.push(e.hash().unwrap());
    }
    (a, hashes, t0 + 10)
}

/// The UTC moment of a local day (0 Monday) and minute in the week holding `near`.
fn at(z: &Zone, near: u64, day: i64, minute: i64) -> u64 {
    let origin = Zone::week_origin(z.local_min(near));
    z.utc_ms(origin + day * 1440 + minute)
}

const NEAR: u64 = 1_794_500_000_000;

#[test]
fn a_daily_tape_starts_from_the_top_at_its_hour_every_day() {
    let k = StationKey::generate().unwrap();
    let (mut a, h, t) = station(&k);
    let z = chicago();
    a.write(&k, t, Body::Programme(Programme { zone: z.clone(), mode: Mode::Daily { start_min: 18 * 60 }, ..Default::default() })).unwrap();
    let c = Clock::new(&a).unwrap();
    let tue18 = at(&z, NEAR, 1, 18 * 60);
    let np = c.now(tue18).unwrap();
    assert_eq!((np.track, np.offset_ms), (h[0], 0), "the top of the tape at six");
    let np = c.now(tue18 + 25 * MIN).unwrap();
    assert_eq!((np.track, np.offset_ms), (h[1], 5 * MIN));
    let np = c.now(tue18 + 65 * MIN).unwrap();
    assert_eq!((np.track, np.offset_ms), (h[0], 5 * MIN), "a short tape goes round again");
    let np = c.now(tue18 + (23 * 60 + 59) * MIN).unwrap();
    assert_eq!((np.track, np.offset_ms), (h[2], 19 * MIN));
    let np = c.now(tue18 + 24 * 60 * MIN).unwrap();
    assert_eq!((np.track, np.offset_ms), (h[0], 0), "and from the top again the next day");
}

#[test]
fn six_hours_a_day_and_off_the_air_the_rest() {
    let k = StationKey::generate().unwrap();
    let (mut a, h, t) = station(&k);
    let z = chicago();
    a.write(&k, t, Body::Programme(Programme { zone: z.clone(), mode: Mode::Daily { start_min: 18 * 60 }, hours: vec![Span::every_day(18 * 60, 360)], ..Default::default() })).unwrap();
    let c = Clock::new(&a).unwrap();
    assert!(c.now(at(&z, NEAR, 2, 12 * 60)).is_none(), "off the air at noon");
    assert_eq!(c.now(at(&z, NEAR, 2, 18 * 60)).unwrap().track, h[0]);
    assert!(c.now(at(&z, NEAR, 2, 23 * 60 + 59)).is_some(), "still on at 23:59");
    assert!(c.now(at(&z, NEAR, 3, 0)).is_none(), "off at midnight");
    let bill = c.upcoming(at(&z, NEAR, 2, 12 * 60), at(&z, NEAR, 3, 12 * 60));
    assert_eq!(bill.first().unwrap().0, at(&z, NEAR, 2, 18 * 60), "the watch bill starts when it comes on");
    assert!(bill.iter().all(|b| b.0 < at(&z, NEAR, 3, 0)), "and ends when it goes off");
}

#[test]
fn a_live_slot_plays_its_fill_and_says_its_name() {
    let k = StationKey::generate().unwrap();
    let (mut a, h, t) = station(&k);
    let z = chicago();
    let slot = Slot { title: "Evening Watch".into(), about: "".into(), span: Span::every_day(18 * 60, 60), fill: Fill::Tracks { tracks: vec![h[2]], shuffle: false }, live: true };
    a.write(&k, t, Body::Programme(Programme { zone: z.clone(), slots: vec![slot], ..Default::default() })).unwrap();
    let c = Clock::new(&a).unwrap();
    let np = c.now(at(&z, NEAR, 4, 18 * 60 + 10)).unwrap();
    assert_eq!((np.track, np.slot.as_deref()), (h[2], Some("Evening Watch")));
    assert_eq!(c.now(at(&z, NEAR, 4, 19 * 60 + 10)).unwrap().slot, None, "after the slot the library plays");
    let l = c.listings(at(&z, NEAR, 4, 0), at(&z, NEAR, 5, 0));
    assert_eq!(l.len(), 1);
    assert!(l[0].live && l[0].kind == "slot" && l[0].start_ms == at(&z, NEAR, 4, 18 * 60));
}

#[test]
fn a_premiere_takes_the_air_and_hands_it_back() {
    let k = StationKey::generate().unwrap();
    let (mut a, h, t) = station(&k);
    let z = chicago();
    a.write(&k, t, Body::Programme(Programme { zone: z.clone(), mode: Mode::InOrder, ..Default::default() })).unwrap();
    let start = at(&z, NEAR, 2, 20 * 60);
    a.write(&k, t + 1, Body::Show { title: "Premiere".into(), about: "First play".into(), at_ms: start, tracks: vec![h[1]], live_min: 0 }).unwrap();
    let c = Clock::new(&a).unwrap();
    let np = c.now(start + 5 * MIN).unwrap();
    assert_eq!((np.track, np.offset_ms, np.slot.as_deref()), (h[1], 5 * MIN, Some("Premiere")));
    assert_eq!(c.now(start + 25 * MIN).unwrap().slot, None, "then the programme carries on");
    let l = c.listings(start - 60 * MIN, start + 60 * MIN);
    assert!(l.iter().any(|x| x.kind == "show" && x.title == "Premiere" && x.end_ms == start + 20 * MIN));
}

#[test]
fn a_called_off_show_never_airs() {
    let k = StationKey::generate().unwrap();
    let (mut a, h, t) = station(&k);
    let start = NEAR;
    let show = a.write(&k, t, Body::Show { title: "Cancelled".into(), about: "".into(), at_ms: start, tracks: vec![h[1]], live_min: 0 }).unwrap().hash().unwrap();
    a.write(&k, t + 1, Body::Unshow { show }).unwrap();
    let c = Clock::new(&a).unwrap();
    assert_eq!(c.now(start + MIN).unwrap().slot, None);
}

#[test]
fn a_watch_plays_what_its_hand_added() {
    let k = StationKey::generate().unwrap();
    let hand = StationKey::generate().unwrap();
    let (mut a, h, t) = station(&k);
    a.write(&k, t, Body::Deputy { key: hand.id().0, label: "Ana".into() }).unwrap();
    let theirs = a.write(&hand, t + 1, Body::Deputed { by: hand.id().0, body: Box::new(track("ana", 20, 9)) }).unwrap().hash().unwrap();
    let z = chicago();
    let slot = Slot { title: "Ana's watch".into(), about: "".into(), span: Span { days: 0b0000_0001, start_min: 4 * 60, len_min: 4 * 60 }, fill: Fill::Hand { key: hand.id().0 }, live: false };
    a.write(&k, t + 2, Body::Programme(Programme { zone: z.clone(), slots: vec![slot], ..Default::default() })).unwrap();
    let c = Clock::new(&a).unwrap();
    for m in [0, 30, 90, 200] {
        assert_eq!(c.now(at(&z, NEAR, 0, 4 * 60 + m)).unwrap().track, theirs, "Monday's watch is Ana's");
    }
    let tuesday = c.now(at(&z, NEAR, 1, 4 * 60 + 30)).unwrap().track;
    assert!(h.contains(&tuesday) || tuesday == theirs, "Tuesday is the whole library's");
}

#[test]
fn newest_first_for_a_podcast() {
    let k = StationKey::generate().unwrap();
    let (mut a, h, t) = station(&k);
    let z = chicago();
    a.write(&k, t, Body::Programme(Programme { zone: z.clone(), mode: Mode::NewestFirst, ..Default::default() })).unwrap();
    let c = Clock::new(&a).unwrap();
    let mon = at(&z, NEAR, 0, 0);
    assert_eq!(c.now(mon + MIN).unwrap().track, h[2]);
    assert_eq!(c.now(mon + 21 * MIN).unwrap().track, h[1]);
}

#[test]
fn local_time_holds_across_daylight_saving() {
    let k = StationKey::generate().unwrap();
    let (mut a, h, t) = station(&k);
    // Standard time until the change, then an hour ahead.
    let change = NEAR + 3 * 86_400_000;
    let z = Zone { name: "Test/DST".into(), offsets: vec![(0, -360), (change, -300)] };
    a.write(&k, t, Body::Programme(Programme { zone: z.clone(), mode: Mode::Daily { start_min: 18 * 60 }, ..Default::default() })).unwrap();
    let c = Clock::new(&a).unwrap();
    let before = z.utc_ms(z.local_min(NEAR).div_euclid(1440) * 1440 + 18 * 60);
    let after = z.utc_ms(z.local_min(change + 2 * 86_400_000).div_euclid(1440) * 1440 + 18 * 60);
    assert_eq!((before / MIN) % 1440, 0, "six in the evening is midnight UTC in standard time");
    assert_eq!((after / MIN) % 1440, 23 * 60, "and eleven UTC in summer time");
    for t in [before, after] {
        let np = c.now(t).unwrap();
        assert_eq!((np.track, np.offset_ms), (h[0], 0), "the tape starts at six local either way");
    }
}

#[test]
fn the_later_of_rota_and_programme_rules() {
    let k = StationKey::generate().unwrap();
    let (mut a, _h, t) = station(&k);
    a.write(&k, t, Body::Programme(Programme { mode: Mode::InOrder, ..Default::default() })).unwrap();
    assert!(a.programme().is_some());
    a.write(&k, t + 1, Body::Rota(Default::default())).unwrap();
    assert!(a.programme().is_none(), "a later rota takes back over");
}

#[test]
fn notices_come_down_when_taken_down_or_out_of_date() {
    let k = StationKey::generate().unwrap();
    let (mut a, _h, t) = station(&k);
    let n1 = a.write(&k, t, Body::Notice { text: "Back Friday".into(), until_ms: 0 }).unwrap().hash().unwrap();
    a.write(&k, t + 1, Body::Notice { text: "Tonight only".into(), until_ms: t + 100 }).unwrap();
    assert_eq!(a.notices(t + 50).len(), 2);
    assert_eq!(a.notices(t + 200).len(), 1, "an out-of-date notice is gone");
    a.write(&k, t + 2, Body::Unnotice { notice: n1 }).unwrap();
    assert!(a.notices(t + 200).is_empty());
}

#[test]
fn the_page_says_when_an_off_air_station_comes_back_and_what_is_on_the_bill() {
    let k = StationKey::generate().unwrap();
    let (mut a, h, t) = station(&k);
    let z = chicago();
    let hours = vec![Span { days: 0x7f, start_min: 18 * 60, len_min: 6 * 60 }];
    let slot = Slot { title: "Night Call".into(), about: "Live".into(), span: Span { days: 0x7f, start_min: 20 * 60, len_min: 60 }, fill: Fill::Library, live: true };
    a.write(&k, t, Body::Programme(Programme { zone: z.clone(), mode: Mode::Shuffle, hours, slots: vec![slot], ..Default::default() })).unwrap();
    let show = a.write(&k, t + 1, Body::Show { title: "The Premiere".into(), about: "".into(), at_ms: at(&z, NEAR, 2, 12 * 60), tracks: vec![h[2]], live_min: 0 }).unwrap().hash().unwrap();
    a.write(&k, t + 2, Body::Notice { text: "Hello".into(), until_ms: 0 }).unwrap();

    // Tuesday at ten in the morning: off the air until six.
    let tue10 = at(&z, NEAR, 1, 10 * 60);
    let p = pyrate_core::page::page_of(&a, tue10, 7).unwrap();
    assert_eq!(p["on_air"], false);
    assert_eq!(p["back_at"].as_u64(), Some(at(&z, NEAR, 1, 18 * 60)), "back at six");
    assert_eq!(p["notices"][0]["text"], "Hello");
    assert_eq!(p["zone_name"], "Test/Chicago");
    let l = p["listings"].as_array().unwrap();
    let first_slot = l.iter().find(|x| x["kind"] == "slot").unwrap();
    assert_eq!((first_slot["title"].as_str(), first_slot["live"].as_bool()), (Some("Night Call"), Some(true)));
    assert_eq!(first_slot["start_ms"].as_u64(), Some(at(&z, NEAR, 1, 20 * 60)));
    let prem = l.iter().find(|x| x["kind"] == "show").unwrap();
    assert_eq!(prem["show"].as_str(), Some(pyrate_core::page::hex(&show).as_str()));
    assert_eq!(prem["tracks"][0]["title"], "t2");
    assert!(l.windows(2).all(|w| w[0]["start_ms"].as_u64() <= w[1]["start_ms"].as_u64()), "in order of time");

    // Tuesday at eight in the evening: on, in the live slot, which plays the library.
    let p = pyrate_core::page::page_of(&a, at(&z, NEAR, 1, 20 * 60 + 5), 1).unwrap();
    assert_eq!(p["now"]["slot"], "Night Call");
    assert!(p["back_at"].is_null());
    assert!(p["next"].as_array().unwrap().len() >= 2);

    // The programme goes out and comes back as the same JSON.
    let j = pyrate_core::page::programme_json(&a.programme().unwrap());
    assert_eq!(pyrate_core::page::programme_from(&j).unwrap(), a.programme().unwrap());
}

// ---- What a hostile or careless log cannot do ---------------------------------------

fn berlin() -> Zone {
    // Central European Time, summer time from 01:00 UTC on 28 March 2027.
    Zone { name: "Test/Berlin".into(), offsets: vec![(0, 60), (1_806_195_600_000, 120)] }
}

#[test]
fn the_hour_lost_in_spring_does_not_stop_the_clock_east_of_utc() {
    let k = StationKey::generate().unwrap();
    let (mut a, _h, t) = station(&k);
    let slot = Slot { title: "Night".into(), about: "".into(), span: Span::every_day(150, 120), fill: Fill::Library, live: false };
    a.write(&k, t, Body::Programme(Programme { zone: berlin(), slots: vec![slot], ..Default::default() })).unwrap();
    let c = Clock::new(&a).unwrap();
    // 03:10 summer time on the day of the change: 01:10 UTC.
    let t = 1_806_195_600_000 + 10 * MIN;
    assert!(c.now(t).is_some(), "plays on through the change");
    // 02:30 local did not exist that night: it begins when the clocks changed.
    let z = berlin();
    let lm = z.local_min(1_806_195_600_000);
    assert_eq!(z.utc_ms(lm - 30), 1_806_195_600_000, "a skipped minute begins at the change");
    assert_eq!(z.utc_ms(lm), 1_806_195_600_000);
}

#[test]
fn a_weekly_tape_signed_with_a_nonsense_minute_still_plays() {
    let k = StationKey::generate().unwrap();
    let (mut a, _h, t) = station(&k);
    a.write(&k, t, Body::Programme(Programme { mode: Mode::Weekly { day: 200, start_min: 65_535 }, ..Default::default() })).unwrap();
    let c = Clock::new(&a).unwrap();
    assert!(c.now(NEAR).is_some());
}

#[test]
fn recordings_of_a_moment_are_not_played_and_cannot_hang_the_clock() {
    let k = StationKey::generate().unwrap();
    let mut a = Archive::new(k.id());
    let t0 = 1_790_000_000_000u64;
    a.write(&k, t0, Body::Colours { name: "P".into(), about: "".into(), port: None, tip: None }).unwrap();
    let blip = Chunk { hash: [7; 32], len: 3, frames: 1 };
    a.write(&k, t0 + 1, Body::Track { title: "blip".into(), by: "".into(), source: "".into(), channels: 1, bitrate: 24_000, chunks: vec![blip] }).unwrap();
    let c = Clock::new(&a).unwrap();
    let started = std::time::Instant::now();
    // Late on a Sunday, the far end of the week's walk.
    let sunday = at(&Zone::utc(), NEAR, 6, 23 * 60);
    assert!(c.now(sunday).is_none(), "twenty milliseconds is not a recording");
    assert!(c.upcoming(sunday, sunday + 6 * 3_600_000).is_empty());
    assert!(started.elapsed() < std::time::Duration::from_secs(5), "and the clock stays quick ({:?})", started.elapsed());
}

#[test]
fn a_show_dated_at_the_end_of_time_breaks_nothing() {
    let k = StationKey::generate().unwrap();
    let (mut a, h, t) = station(&k);
    a.write(&k, t, Body::Show { title: "Never".into(), about: "".into(), at_ms: u64::MAX - 1, tracks: vec![h[0]], live_min: 30 }).unwrap();
    let c = Clock::new(&a).unwrap();
    assert!(c.now(NEAR).is_some());
    let _ = c.listings(NEAR, NEAR + 7 * 86_400_000);
}

/// A recording of so many seconds, in one chunk.
fn short(title: &str, ms: u64, seed: u8) -> Body {
    let chunk = Chunk { hash: [seed; 32], len: 10, frames: (ms / 20) as u16 };
    Body::Track { title: title.into(), by: "".into(), source: "".into(), channels: 1, bitrate: 24_000, chunks: vec![chunk] }
}

#[test]
fn a_station_of_short_jingles_plays_to_the_end_of_the_week() {
    let k = StationKey::from_seed([41; 32]);
    let mut a = Archive::new(k.id());
    let t0 = 1_790_000_000_000u64;
    a.write(&k, t0, short("jingle", 1_500, 1)).unwrap();
    let sunday_late = at(&Zone::utc(), NEAR, 6, 23 * 60);
    assert!(Clock::new(&a).unwrap().now(sunday_late).is_some(), "with no programme");
    a.write(&k, t0 + 5, Body::Programme(Programme::default())).unwrap();
    assert!(Clock::new(&a).unwrap().now(sunday_late).is_some(), "under the default programme");
}

#[test]
fn skipping_whole_rounds_changes_nothing() {
    let k = StationKey::from_seed([42; 32]);
    let (mut a, hashes, t0) = station(&k);
    for i in 0..9u8 {
        a.write(&k, t0 + 20 + i as u64, track(&format!("s{i}"), 3 + (i as u64 % 4), 10 + i)).unwrap();
    }
    // Something comes on the air mid-week and something leaves.
    let mid = at(&Zone::utc(), NEAR, 3, 600);
    let mut later = a.clone();
    later.write(&k, mid, track("late", 7, 60)).unwrap();
    later.write(&k, mid + 3 * 3_600_000, Body::Withdraw { track: hashes[1] }).unwrap();
    let modes = [None, Some(Mode::Shuffle), Some(Mode::InOrder), Some(Mode::NewestFirst), Some(Mode::Daily { start_min: 90 }), Some(Mode::Weekly { day: 2, start_min: 300 })];
    for mode in modes {
        let mut arch = later.clone();
        if let Some(m) = mode.clone() {
            arch.write(&k, mid + 4 * 3_600_000, Body::Programme(Programme { zone: chicago(), mode: m, ..Programme::default() })).unwrap();
        }
        let fast = Clock::new(&arch).unwrap();
        let slow = Clock::new(&arch).unwrap().stepping();
        let from = at(&Zone::utc(), NEAR, 0, 0);
        let mut t = from;
        while t < from + 8 * 86_400_000 {
            assert_eq!(fast.now(t), slow.now(t), "{mode:?} at {t}");
            t += 7 * MIN + 13_000;
        }
    }
}

/// Chicago's autumn: at 07:00 UTC on 1 November 2026 the clocks go back from 02:00 to 01:00.
fn chicago_autumn() -> Zone {
    Zone { name: "Test/Chicago".into(), offsets: vec![(0, -300), (1_793_516_400_000, -360)] }
}
const FALL_BACK: u64 = 1_793_516_400_000;

#[test]
fn the_hour_that_comes_twice_neither_rewinds_a_tape_nor_cuts_a_slot() {
    let k = StationKey::from_seed([43; 32]);
    let (mut a, h, t) = station(&k);
    let z = chicago_autumn();
    // A daily tape from 01:30, three 20-minute recordings: 01:30 came first at 06:30 UTC.
    a.write(&k, t, Body::Programme(Programme { zone: z.clone(), mode: Mode::Daily { start_min: 90 }, ..Default::default() })).unwrap();
    let c = Clock::new(&a).unwrap();
    let tape = FALL_BACK - 30 * MIN;
    for (when, started) in [(FALL_BACK - MIN, tape + 20 * MIN), (FALL_BACK + 5 * MIN, tape + 20 * MIN), (FALL_BACK + 20 * MIN, tape + 40 * MIN), (FALL_BACK + 40 * MIN, tape + 60 * MIN)] {
        assert_eq!(c.now(when).unwrap().started_at_ms, started, "the tape at {}", (when as i64 - FALL_BACK as i64) / 60_000);
    }
    // A slot from 01:30 for an hour runs an hour of real time, 06:30 to 07:30 UTC, and the
    // page says the same.
    let mut b = a.clone();
    let slot = Slot { title: "Night Watch".into(), about: "".into(), span: Span::every_day(90, 60), fill: Fill::Tracks { tracks: vec![h[2]], shuffle: false }, live: false };
    b.write(&k, t + 1, Body::Programme(Programme { zone: z.clone(), slots: vec![slot], ..Default::default() })).unwrap();
    let c = Clock::new(&b).unwrap();
    for m in [-20i64, 0, 10, 25] {
        assert_eq!(c.now((FALL_BACK as i64 + m * MIN as i64) as u64).unwrap().slot.as_deref(), Some("Night Watch"), "{m} minutes from the change");
    }
    assert_eq!(c.now(FALL_BACK + 35 * MIN).unwrap().slot, None, "the slot has had its hour");
    let l: Vec<_> = c.listings(FALL_BACK - 3 * 3_600_000, FALL_BACK + 3 * 3_600_000).into_iter().filter(|l| l.kind == "slot").collect();
    assert!(l.iter().any(|l| l.start_ms == FALL_BACK - 30 * MIN && l.end_ms == FALL_BACK + 30 * MIN), "{l:?}");
}

#[test]
fn the_watch_bill_cuts_where_the_air_changes_hands() {
    let k = StationKey::from_seed([44; 32]);
    let (mut a, h, t) = station(&k);
    let z = chicago();
    // In order, 20-minute recordings, and a ten-minute slot at 01:05 that plays the third.
    let slot = Slot { title: "Spot".into(), about: "".into(), span: Span::every_day(65, 10), fill: Fill::Tracks { tracks: vec![h[2]], shuffle: false }, live: false };
    a.write(&k, t, Body::Programme(Programme { zone: z.clone(), mode: Mode::InOrder, slots: vec![slot], hours: vec![Span::every_day(60, 120)], ..Default::default() })).unwrap();
    let c = Clock::new(&a).unwrap();
    let one = at(&z, NEAR, 2, 60);
    let bill = c.upcoming(one, one + 3 * 3_600_000);
    // Every piece is what the air says at its middle, and the pieces meet end to end.
    for (i, (start, track, len)) in bill.iter().enumerate() {
        assert_eq!(c.now(start + len / 2).map(|n| n.track), Some(*track), "piece {i} at {start}");
        if let Some(next) = bill.get(i + 1) {
            assert!(start + len <= next.0, "piece {i} overlaps the next");
        }
    }
    let starts: Vec<i64> = bill.iter().map(|b| (b.0 as i64 - one as i64) / MIN as i64).collect();
    assert!(starts.contains(&5) && starts.contains(&15), "the slot is listed and the cut recording resumes after it: {starts:?}");
    let slotted = bill.iter().find(|b| b.0 == one + 5 * MIN).unwrap();
    assert_eq!((slotted.1, slotted.2), (h[2], 10 * MIN));
    // Off the air at 03:00: nothing on the bill runs past it.
    let off = one + 2 * 3_600_000;
    assert!(bill.iter().all(|b| b.0 >= off || b.0 + b.2 <= off), "{bill:?}");
}
