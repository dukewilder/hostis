# A: clock / programme / page / audio / ingest audit

Scope: core/src/{clock,programme,page,audio,tape,ingest,callsign,art}.rs, core/tests/*, node/src/{clockfix,player}.rs.
Throwaway tests (not in repo): /tmp/audit/tests/{a,b}.rs, run with CARGO_TARGET_DIR=/tmp/audit-target (debug build, so overflow checks are ON; a release build wraps instead of panicking).
No cross-platform divergence found in the shuffle path: splitmix64 + BLAKE3 seed, LE integer encodings, no floats, no HashMap, no usize in anything hashed. (art::accent is the one HashMap, see A17.)

### A1 One hostile station can move every listener's clock by up to five minutes
- Where: node/src/clockfix.rs:10-27, fed from node/src/lib.rs:547-549
- Severity: high
- What: `sample()` stores one gap per received card with no per-station limit, and the gate `now.saturating_sub(a.at_ms) < 60_000` lets any card dated in the future through (saturating_sub gives 0). The median of the last 101 gaps is then applied to `now_ms()` if it is between 2 s and 300 s.
- Failure: a station (or one key) announces ~7+ cards, each with a larger `at_ms` (each counts as "fresh") dated now+299 s. Median is +299 s, `offset_ms()` returns it, every node that hears the card hears every station 4m59s early, so the "agree to the millisecond" property is gone for that victim. Honest stations whose phone clocks are wrong skew it the same way, only unintentionally.
- Fix: reject cards with `a.at_ms > now + 2_000` (clock cannot be ahead of the moment it was received); keep at most one sample per station id (map station -> latest gap); require samples from >= 5 distinct stations before correcting; also subtract typical gossip delay by only using cards received directly from the signer. Code sketch: `static SAMPLES: Mutex<HashMap<[u8;32], i64>>` capped at 101 with oldest-eviction, median over values.
- Confidence: likely (read the call site; not run)

### A2 `t - cursor` underflow in `walk` when a run's computed start is after `t` (panic in debug, garbage in release)
- Where: core/src/clock.rs:351 (`let offset = t - cursor;`), start produced at :383 (`p.zone.utc_ms(run)`), :453 (`p.zone.utc_ms(anchor_lm)`), :446-449 (Weekly), programme.rs:50-55 (`utc_ms`)
- Severity: high
- What: `walk` assumes `start <= t`. Three ways to break it: (a) `Zone::utc_ms` for a local minute that does not exist (spring-forward gap) or exists twice (fall-back) maps differently by sign of offset. For zones east of UTC (Europe, Australia) a local minute in the gap maps to a UTC moment after the real transition, and in the repeated hour it maps to the second occurrence, so `run_start`/Daily/Weekly anchors can land after `t`. (b) `Mode::Weekly { start_min >= 1440 }` only subtracts one week, so the anchor stays in the future (start_min is unvalidated u16 in the signed log; page.rs clamps only the editing path). (c) a hostile or odd offset table (unsorted, jumps of 28 h; offsets are not clamped or sorted when read from the log).
- Failure (confirmed): Zone Berlin `[(0,60),(1806195600000,120)]` (2027-03-28 01:00 UTC), slot every day 02:30 len 120, `now(03:10 CEST)` panics "attempt to subtract with overflow" at clock.rs:351. Same on the autumn change for any slot/Daily/Weekly starting 02:00-02:59 local (first pass of the repeated hour: start maps to the second pass, in the future). Also confirmed: `Mode::Weekly{day:0,start_min:65535}` panics in `now()` on any date. In a release build this wraps: `offset_ms` ~1.8e19, `chunk == chunk_ms.len()`, so a consumer indexing `chunks[np.chunk]` panics or plays nothing.
- Fix: make `walk` total: `if start > t { return None }` at the top (or clamp `start = start.min(t)`), and use `t.saturating_sub(cursor)`. Make `utc_ms` pick one rule for both signs (earliest valid UTC whose local_min equals the request; if none, the first instant at or after the gap), e.g. `let a = local - off_before; let b = local - off_after;` and choose by checking `local_min(candidate) == local_min`. Reduce `start_min % 1440` and `day % 7` when decoding Mode/Span from the log (or in `Clock`), and clamp/sort/dedupe `Zone.offsets` on read.
- Confidence: confirmed (tested)

### A3 Rota walk has no step cap: a track of a few milliseconds hangs the lookup
- Where: core/src/clock.rs:555-604 (`rota_now`, bare `loop`), reached from `now()` at :308 for any station with no programme (the default)
- Severity: high
- What: `rota_now` walks from Monday 00:00 UTC track by track with no MAX_STEPS. `Clock::new` accepts any track with duration > 0, so a signed track of one chunk with `frames: 1` (20 ms) is legal. A late-week lookup walks ~30 million steps, and each `deal` round rebuilds a pool Vec and re-seeds BLAKE3.
- Failure (confirmed): one 20 ms track, `now(Sunday)` takes 3.9 s in a debug build; `upcoming()` calls `now()` up to ~360 times for a six-hour window when the walk dead-ends, and `page_of` calls `now`, `upcoming` (twice if off air). A hostile station's page freezes the phone's node for minutes. Honest stations with many sub-2-second clips are slow too.
- Fix: enforce a minimum duration at `Clock::new` (for example ignore tracks under 1 s, or treat anything shorter as 1 s for scheduling) and apply the same step cap as `walk`. Better, remove the O(week) replay: take the playlist period `P = sum of on-air durations` per round and jump `cursor` forward by whole rounds when the on-air set is unchanged since the last event (tracks added/withdrawn are the only change points).
- Confidence: confirmed (tested)

### A4 `walk` gives up after 400,000 steps, so short tracks mean dead air late in the week
- Where: core/src/clock.rs:22 (`MAX_STEPS`), :330-370
- Severity: medium
- What: the programme walk starts each Monday 00:00 local, so reaching Sunday evening needs 604,800 s / avg track length steps. At 1.5 s average the budget runs out about 80 minutes before the week ends; idle-minute steps in a gap also spend the budget.
- Failure (confirmed): one 1.5 s track, default Programme (Shuffle), `now(Sunday 23:00 UTC)` returns None (dead air) while Sunday 12:00 returns Some. `page_of` then reports the station off air and `back_at` points to the next lookup. A library of 3-second jingles has no slack at all across DST-length weeks (+1 h).
- Fix: same as A3: skip whole rounds arithmetically instead of stepping, or at minimum raise the cap to cover a full week at the minimum accepted duration (`7*86400*1000 / min_duration_ms`).
- Confidence: confirmed (tested)

### A5 Fall-back hour: Daily and Weekly tapes rewind, slots drop out
- Where: core/src/clock.rs:440-451 (`Mode::Daily`/`Weekly` anchors from `lm`), :381-383 (slot run via `run_start(lm)`); root cause `Zone::local_min` is not monotonic (programme.rs:45)
- Severity: medium
- What: at the autumn change `local_min` repeats the hour. Every decision keyed off `lm` is evaluated twice.
- Failure (confirmed for Daily): Chicago, 2026-11-01 07:00 UTC change, `Mode::Daily{start_min:90}` (01:30). Offset into the tape: 06:59 UTC -> 29 min; 07:00 UTC -> 1470 min (yesterday's tape, a day and 30 min in); stays on yesterday's tape until 07:30 UTC, then jumps to 60 min. Two audible jumps. By reading: a slot starting 01:30 local len 60 plays 06:30-07:00 UTC, drops to the station mode 07:00-07:30 UTC (lm 60-89 is before its start), then returns mid-run at 07:30 UTC; the page listing shows it uninterrupted (06:30-08:30 UTC).
- Fix: derive run starts from UTC, not from `lm` alone. For each candidate local run start `s`, compute its UTC start `u = utc_ms(s)` and select the latest run with `u <= t` and `t < utc_ms(s + len)`. Same for Daily/Weekly: choose the latest candidate anchor whose UTC start is <= t. That makes the repeated hour behave as continuation.
- Confidence: confirmed (Daily), likely (slot)

### A6 `upcoming()` (watch bill and the page's "next") ignores every cut
- Where: core/src/clock.rs:275-291
- Severity: medium
- What: it steps `t = start + np.duration_ms` (the scheduled end of the track). But playback is cut earlier at slot end, off-air hours, a show ending, the Monday reseed and each Daily/Weekly anchor, and then something else starts.
- Failure (confirmed): InOrder programme, 20-minute tracks, slot 01:05-01:15 playing another track. Bill: tracks at 60, 80, 100... (each "20 min"); the slot item never appears, and the track that resumes at 01:15 after the slot (offset 15 min into the 01:00 track) is listed as starting at 01:00 for 20 minutes. Same at the end of on-air hours: the last track is shown running to its natural end after the station is off.
- Fix: have `NowPlaying` carry `until_ms` = min(track end, end of slot/show/on-air span, next anchor) computed where the cut is decided in `programme_now`/`now`, and step `t = until_ms` in `upcoming`. Cheap alternative: also probe `now()` at every `Listing` boundary from `listings(from, until)` and merge.
- Confidence: confirmed (tested)

### A7 `at_ms + show_len` overflows for a signed Show with a huge `at_ms`
- Where: core/src/clock.rs:319 (`show_at`), :460 (`listings`)
- Severity: medium
- What: `Body::Show.at_ms` is any u64; `s.at_ms + self.show_len(s)` is unchecked. Debug builds and the test harness panic; release wraps so the show silently never airs (or lists with `end_ms < start_ms`).
- Failure: Show with `at_ms = u64::MAX - 1`, one 20 min track, then any `now()`, `listings()` or `page_of()` panics in debug. In release, `end_ms` is tiny, so the listing has end before start, and the `.filter(l.end_ms > now)` in page_of drops or keeps it arbitrarily.
- Fix: `s.at_ms.saturating_add(self.show_len(s))` in both places; reject shows whose `at_ms` is beyond, say, entry.at_ms + 10 years in `Clock::new`.
- Confidence: likely (read; same class as A2 which was tested)

### A8 A show premiering before its track was added never airs, and the listing still promises it
- Where: core/src/clock.rs:313-315 (`show_len` uses `on_air(s.at_ms)`), :167-171 via `Source::Once::next` (`on_air(cursor)`), :460 (`.max(60_000)`)
- Severity: medium
- What: a track is on air only from its entry's `at_ms`. A premiere scheduled for "this minute" (at_ms rounded down) of a track added a few seconds ago, or any premiere backdated before the track entry, has `on_air(at_ms) == false`, so `show_len` is 0 (plus live part) and `show_at` never selects it. `listings` still shows it for at least a minute (`show_len().max(60_000)`).
- Failure: add track at T+5 s, add Show{at_ms: T, tracks:[that]}: `now(T+30 s)` plays the programme, the page lists "Premiere". Separately `show_len` evaluates `on_air` once at `at_ms` while `walk` re-evaluates at each `cursor`, so a track withdrawn mid-show is skipped by the walk but still counted in `show_len`, leaving the last seconds of the window as "show on air, nothing to play" (falls through to programme mid-track).
- Fix: for shows, evaluate availability at `max(at_ms, show_entry.at_ms)` (store the entry time in `Show`), and use the same predicate in `show_len` and in the walk (pass a fixed `as_of` into `Source::Once`). Do not list a show whose `show_len == 0 && live_ms == 0`.
- Confidence: likely

### A9 Call signs carry ~35 bits, so a lookalike can be ground out for a target
- Where: core/src/callsign.rs:9-23
- Severity: medium
- What: three consonant/vowel pairs (4 + 2.3 bits each) plus 16 hex bits is about 2^35 states, taken from 5 bytes of the hash. The header comment says nobody can squat one and a forged one is a different key; a second-preimage of 2^35 key generations (~3.4e10) is hours on a GPU for ed25519.
- Failure: attacker generates keys until `call_sign(id) == "KAROMI·52E1"` and runs a lookalike station that shows the same call sign next to the real one.
- Fix: do not present it as an identity proof. Either lengthen it (use 8+ bytes: e.g. two more groups, or 64 bits of hex) or always show a fingerprint of the key alongside it where trust matters, and fix the comment. Also vowel index `(b>>4) % 5` over 16 values is slightly biased (A gets 4/16, others 3/16).
- Confidence: likely

### A10 Clock correction jumps from 0 to 2 s instead of easing in
- Where: node/src/clockfix.rs:26
- Severity: low
- What: `if m.abs() < 2_000 { 0 }` is a dead band with a step at its edge. A median hovering around 2,000 flips `now_ms()` by 2 s each time it crosses, so the playhead re-seeks repeatedly.
- Fix: apply the offset with hysteresis (turn on above 2.5 s, off below 1 s) and slew toward it.
- Confidence: likely

### A11 `ChunkDecoder::decode` trusts the chunk's packet count and sizes
- Where: core/src/audio.rs:166-174; archive.rs `Chunk::check` (:25) checks only length and hash
- Severity: low
- What: nothing ties `Chunk.frames` (what the clock uses for timing) to the packets inside. The decode buffer is 120 ms; a zero-length packet is passed to Opus as `&[]`, which is loss concealment for the whole 120 ms buffer (6 frames) rather than 20 ms. A hostile chunk can also hold packets of 60 or 120 ms.
- Failure: a station signs a chunk with `frames: 300` whose bytes decode to 3 s or 12 s of audio. The player queue and the clock disagree and the stream drifts for that station.
- Fix: in `decode`, reject empty packets (`n == 0`), use `opus::packet::get_nb_samples` to require 960 samples per packet, and require `packets.len() == frames` when the caller supplies it.
- Confidence: possible

### A12 Ogg writer can overflow the lacing byte and drops bad chunks silently
- Where: core/src/tape.rs:26-45, :83
- Severity: low
- What: `lacing.len() as u8` wraps when a page of 50 packets needs more than 255 segments (packets over ~1.3 KB average; legit 64 kbit/s frames are far smaller but chunk bytes come from other stations). `packets(c).unwrap_or_default()` drops a malformed chunk without saying so, and zero chunks yield a file with no EOS page. `channels` is written into OpusHead unvalidated (0 or >2 gives an invalid header).
- Failure: a tape of a hostile or corrupt stream produces an unplayable file with no error.
- Fix: cap each page by segment count (`while segs + needed > 255 { flush }`) not packet count; return `Result` and propagate chunk errors; validate `channels` in 1..=2.
- Confidence: possible

### A13 `unhex` accepts a leading `+`
- Where: core/src/page.rs:33
- Severity: low
- What: `u8::from_str_radix("+f", 16)` is `Ok(15)`, so `"+f..."` pairs parse and non-canonical hashes/keys are accepted from the page API.
- Fix: check `s.bytes().all(|b| b.is_ascii_hexdigit())` first.
- Confidence: confirmed by reading the std behaviour (not run)

### A14 `span_from` truncates `days` and accepts a span that never runs
- Where: core/src/page.rs:56
- Severity: low
- What: `as_u64().unwrap_or(0x7f) as u8 & 0x7f` truncates before masking: `days: 256` becomes 0, and `days: 0` is accepted. A non-empty `hours` list whose spans all have no days means `Programme::on_air` is false forever, so the station is dark except for slots, with no error.
- Fix: parse as u64, `if d & 0x7f == 0 { return bad("pick at least one day") }`, then mask.
- Confidence: likely

### A15 Hours plus a continuous mode silently skips what plays off the air
- Where: core/src/clock.rs:422-453
- Severity: low
- What: Shuffle, InOrder and NewestFirst walk continuously from Monday 00:00 local; off-air hours merely hide the stream. A keeper who sets InOrder (or NewestFirst "for a podcast") with six hours on air a day never has episodes heard in order: whatever is "now" in the hidden continuous walk is what the listener gets, and 18 hours a day of tracks are never heard. Only Daily/Weekly restart at the on-air time.
- Fix: when `hours` is non-empty, advance the mode clock only inside the on-air spans (anchor the walk at the start of each on-air run, like a slot), or document that hours require Daily/Weekly.
- Confidence: likely

### A16 Spans and tables read from the log are not validated, so `now` and `listings` can disagree and costs are unbounded
- Where: core/src/programme.rs:82-121 (`run_start`, `starts_between`), :32-42 (`offset_min`); core/src/clock.rs:264, :394, :201-205
- Severity: low
- What: only page.rs (the editing path) clamps `start_min`, `len_min`, `days`, offsets (+-14 h), 400 offsets, 64 slots, 500 tracks. A signed Programme in the log has none of that: `start_min` up to 65535 and `len_min` up to 65535 (45 days) make `run_start` (looks back one week only) and `starts_between` disagree; an unsorted offset table makes `offset_min` stop at the first later entry. Clock::new's `tracks.iter().position` per show hash and per `Fill::Tracks` hash inside every `now()` is O(shows x tracks x library).
- Failure: a station with 100k Show hashes and a 100k-track library costs ~1e10 comparisons in `Clock::new`.
- Fix: normalise once in `Clock::new` (clamp/sort offsets, `start_min %= 1440`, `len_min = len_min.min(WEEK_MIN)`, cap slot/track counts) and build a `HashMap<Hash, usize>` index (iteration order is not used, so determinism is safe).
- Confidence: likely

### A17 `art::accent` depends on HashMap iteration order for ties
- Where: core/src/art.rs:81, :98
- Severity: low
- What: `buckets.into_iter().max_by(total_cmp)` returns the last maximum, and `std` HashMap iteration order is random per process. Two buckets with equal weight (a flat two-colour cover) can give different accents run to run. The accent is stored in the signed `Body::Art`, so nodes do not disagree, but rebuilding is not reproducible.
- Fix: use `BTreeMap`, or `max_by` on `(weight, key)`.
- Confidence: possible

### A18 `art::prepare` ignores EXIF orientation
- Where: core/src/art.rs:49-54
- Severity: low
- What: the image is decoded and resized without applying the orientation tag, then re-encoded without EXIF, so a portrait phone photo becomes a permanently sideways cover.
- Fix: `let mut dec = r.into_decoder()?; let o = dec.orientation()?; let mut img = DynamicImage::from_decoder(dec)?; img.apply_orientation(o);`
- Confidence: likely

### A19 Ingest folds multichannel files badly
- Where: core/src/ingest.rs:33-43
- Severity: low
- What: `(_, 2) => frame[..2]` takes only front L/R of 5.1 and drops the centre channel (dialogue); `(_, 1)` averages all channels including LFE. Loudness is then measured on that fold, so a movie rip goes in quiet and thin. Also `chunks_exact(in_ch)` panics if a decoder ever reports 0 channels, and a file that is entirely below -50 dBFS is accepted at full length (only `total_frames == 0` is rejected).
- Fix: standard downmix (`L + 0.707 C + 0.707 Ls`, `R + 0.707 C + 0.707 Rs`, drop LFE) when `in_ch > 2`; guard `in_ch == 0`; reject all-silent input.
- Confidence: likely

### A20 Hard seams at the week boundary (and at every anchor)
- Where: core/src/clock.rs:425-453, :556-557
- Severity: low
- What: the rota restarts at Monday 00:00 UTC and the programme modes at Monday 00:00 local with a new shuffle seed, so the track playing at 23:59:xx Sunday is cut mid-track and a new first track starts. Slot ends and Daily/Weekly anchors cut the same way. The log comments describe the restart but the seam is audible weekly.
- Fix: optional: when the walk crosses the anchor, let the track that started before the anchor finish, then start the new week; keep the same seed rule so nodes still agree.
- Confidence: likely (design trade-off)

### A21 Offset table ends: time zone freezes after the last entry
- Where: core/src/programme.rs:32-42; page.rs:88-95 (`take(400)`)
- Severity: low
- What: after the last (from, offset) entry the zone stays at that offset forever. "Holds for decades" depends on the keeper's device generating enough rows; if the last row is a spring-forward the station runs an hour off all winter once the table runs out, with no warning in the page.
- Fix: have the page JSON expose `zone_valid_until` (last `from`) and warn the keeper; or store a rule (IANA name + rule set) instead of a table.
- Confidence: possible

### A22 Spring-forward gap and `utc_ms` are inconsistent between east and west zones
- Where: core/src/programme.rs:50-55
- Severity: low
- What: a nonexistent local minute (02:30 on the spring day) maps to 01:30 standard (an hour early) west of UTC but to 03:30 summer east of UTC; listings for those runs show start_ms in the wrong hour. Folded into the fix for A2; listed separately since a tape or slot placed in the gap also starts at the wrong real time even when no panic occurs.
- Fix: same as A2 (one rule: first instant at or after the gap).
- Confidence: likely

### A23 Output and input streams ignore the device's sample format
- Where: node/src/player.rs:41-43, :97-99
- Severity: low
- What: both streams are built with `f32` callbacks regardless of `supported.sample_format()`. On a backend whose default format is i16/u16 (some ALSA and Android paths) `build_*_stream::<f32>` returns an error and there is no sound or microphone. Also the output callback takes a std `Mutex` on the real-time thread and `unwrap()`s it, so a poisoned queue turns into an audio-thread panic.
- Fix: `match supported.sample_format() { F32 => build::<f32>, I16 => build::<i16>, U16 => ... }` (convert at the edge); use `try_lock` and output silence on contention.
- Confidence: possible

### A24 `page_of` days overflow and repeated full-log hashing
- Where: core/src/page.rs:39-47, :141, :146-150
- Severity: low
- What: `now + days * 86_400_000` overflows for a large `days` argument (debug panic, release wrap to a window in the past). `describe_track` re-hashes every log entry per call and is called per show track per listing, and `a.shows()` is rebuilt per listing, so a station with many shows and a large log makes `page_of` quadratic.
- Fix: `days.min(60)` with `saturating_mul`; build one `HashMap<Hash, (title, by)>` and one shows map per `page_of` call.
- Confidence: likely
