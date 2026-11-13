//! The live desk end to end: a keeper goes on the air from the desk, a listener tunes in, and
//! what the listener is sent is decoded as its player decodes it and measured, tone by tone.
//! There is no microphone here, so the microphone, the deck, a pad and the second input each
//! play a tone of their own from a file (the desk's test-only `mic_file` and `two_file`).
//! desk_test <bootstrap id> [work dir]. Run with PYRATE_LAN_ANY=1 and PYRATE_NULL_AUDIO=1.
use std::{
    f64::consts::PI,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use pyrate_core::{Body, StationId, audio::ChunkDecoder};
use pyrate_node::{Config, Node};
use serde_json::{Value, json};

const MIC_HZ: f64 = 700.0;
const DECK_HZ: f64 = 1500.0;
const PAD_HZ: f64 = 3200.0;
const TWO_HZ: f64 = 2400.0;
const R: f64 = 48_000.0;

/// A 16-bit WAV of `secs`, each channel's sample at time t from `f(t, channel)`.
fn wav(path: &Path, rate: u32, ch: u16, secs: f64, f: impl Fn(f64, u16) -> f64) -> anyhow::Result<()> {
    let frames = (secs * rate as f64) as u32;
    let data = frames * ch as u32 * 2;
    let mut b = Vec::with_capacity(44 + data as usize);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&ch.to_le_bytes());
    b.extend_from_slice(&rate.to_le_bytes());
    b.extend_from_slice(&(rate * ch as u32 * 2).to_le_bytes());
    b.extend_from_slice(&(ch * 2).to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data.to_le_bytes());
    for i in 0..frames {
        for c in 0..ch {
            let s = (f(i as f64 / rate as f64, c).clamp(-1.0, 1.0) * 32767.0) as i16;
            b.extend_from_slice(&s.to_le_bytes());
        }
    }
    std::fs::write(path, b)?;
    Ok(())
}

/// How strong a tone is in `x`, as its amplitude.
fn amp(x: &[f32], hz: f64) -> f64 {
    let (mut s, mut c) = (0.0, 0.0);
    for (i, v) in x.iter().enumerate() {
        let t = 2.0 * PI * hz * i as f64 / R;
        s += *v as f64 * t.sin();
        c += *v as f64 * t.cos();
    }
    2.0 * (s * s + c * c).sqrt() / x.len().max(1) as f64
}

fn verdict(ok: bool) -> &'static str {
    if ok { "PASS" } else { "FAIL" }
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let boot = vec![args.get(1).expect("desk_test <bootstrap id> [work dir]").parse()?];
    let work = PathBuf::from(args.get(2).cloned().unwrap_or_else(|| "/tmp/desk-test".into()));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work)?;
    let (mic, deck, pad, two) = (work.join("mic.wav"), work.join("deck.wav"), work.join("pad.wav"), work.join("two.wav"));
    // The microphone a voice's level of tone, round and round; the deck a minute at 44.1 kHz,
    // so it goes through the resampler; a pad of mono at 32 kHz; the second input on the left
    // alone. The voice and the deck together stay under the limiter's ceiling.
    wav(&mic, 48_000, 1, 2.0, |t, _| 0.3 * (2.0 * PI * MIC_HZ * t).sin())?;
    wav(&deck, 44_100, 2, 60.0, |t, _| 0.35 * (2.0 * PI * DECK_HZ * t).sin())?;
    wav(&pad, 32_000, 1, 2.5, |t, _| 0.5 * (2.0 * PI * PAD_HZ * t).sin())?;
    wav(&two, 48_000, 2, 2.0, |t, c| if c == 0 { 0.3 * (2.0 * PI * TWO_HZ * t).sin() } else { 0.0 })?;

    let cfg = |d: &str, private: bool| {
        let _ = std::fs::remove_dir_all(d);
        Config { dir: PathBuf::from(d), quota: 1 << 30, lantern: false, bootstrap: boot.clone(), public_dns: false, private, ballast: 0, carry_all: false, release_keys: vec![] }
    };
    let k = Node::start(cfg("/tmp/desk-keeper", false))?;
    let l = Node::start(cfg("/tmp/desk-listener", true))?;
    let r = k.call(&json!({ "cmd": "raise", "name": "Desk Watch", "about": "test" }));
    let id = r["id"].as_str().unwrap().to_string();
    let sid = StationId::from_hex(&id)?;
    let call = |v: Value| {
        let r = k.call(&v);
        if r.get("error").is_some() {
            println!("  {} -> {}", v, r);
        }
        r
    };

    // The desk is laid out before the show: the deck loaded, a pad, the microphone's stand-in.
    call(json!({ "cmd": "desk_set", "mic_file": mic.to_string_lossy() }));
    call(json!({ "cmd": "deck_add", "files": [{ "path": deck.to_string_lossy(), "title": "Harbour Tone", "by": "Test Band" }] }));
    let pl = call(json!({ "cmd": "pad_load", "slot": 0, "path": pad.to_string_lossy(), "name": "Sting" }));
    println!("{} a pad loads from a file ({} ms)", verdict(pl["pads"][0]["len_ms"].as_u64().is_some_and(|m| (2_400..2_600).contains(&m))), pl["pads"][0]["len_ms"]);
    let long = work.join("long.wav");
    wav(&long, 48_000, 1, 31.5, |_, _| 0.0)?;
    let refused = call(json!({ "cmd": "pad_load", "slot": 1, "path": long.to_string_lossy() }));
    println!("{} a pad refuses a sound over thirty seconds ({})", verdict(refused["error"].as_str().is_some_and(|e| e.contains("thirty"))), refused["error"]);
    let _ = std::fs::remove_file(&long);
    let gl = call(json!({ "cmd": "go_live", "station": id }));
    println!("{} the keeper goes live from the desk ({gl})", verdict(gl["ok"] == true));
    let d = call(json!({ "cmd": "desk" }));
    println!("{} a voice alone goes out as talk (music {})", verdict(d["music"] == false && d["mic"]["on"] == true), d["music"]);
    // The keeper tuned to its own station while on the air hears nothing of it.
    k.call(&json!({ "cmd": "tune", "station": id }));

    let t = Instant::now();
    let sees = |n: &Node| n.call(&json!({ "cmd": "stations" })).as_array().is_some_and(|a| a.iter().any(|x| x["id"] == id.as_str()));
    while !sees(&l) && t.elapsed() < Duration::from_secs(180) {
        std::thread::sleep(Duration::from_millis(500))
    }
    println!("{} the listener sees the new station ({:.1} s)", verdict(sees(&l)), t.elapsed().as_secs_f64());
    l.call(&json!({ "cmd": "tune", "station": id }));
    let state = |n: &Node| n.call(&json!({ "cmd": "status" }))["now"]["state"].as_str().unwrap_or("").to_string();
    let t = Instant::now();
    while state(&l) != "live" && t.elapsed() < Duration::from_secs(90) {
        std::thread::sleep(Duration::from_millis(250));
    }
    println!("{} the listener hears the station live ({:.1} s)", verdict(state(&l) == "live"), t.elapsed().as_secs_f64());
    let own = state(&k);
    println!("{} the keeper on the air is not played its own station (state '{own}')", verdict(own != "live"));

    // The show, in phases. Each mark is where the desk's air stood when the change was made.
    let aired = || k.call(&json!({ "cmd": "desk" }))["aired"].as_u64().unwrap_or(0);
    let mut marks: Vec<(&str, u64)> = Vec::new();
    let wait = |s: f64| std::thread::sleep(Duration::from_secs_f64(s));
    marks.push(("talk", aired()));
    wait(8.0);
    call(json!({ "cmd": "deck_play" }));
    marks.push(("deck under the mic", aired()));
    wait(10.0);
    let st = call(json!({ "cmd": "desk" }));
    println!("{} the desk says what the deck plays ({} at {} ms of {} ms)", verdict(st["deck"]["title"] == "Harbour Tone" && st["deck"]["pos_ms"].as_u64().is_some_and(|p| p > 8_000) && st["deck"]["len_ms"].as_u64() == Some(60_000)), st["deck"]["title"], st["deck"]["pos_ms"], st["deck"]["len_ms"]);
    call(json!({ "cmd": "desk_set", "mic_open": false }));
    marks.push(("mic closed", aired()));
    wait(8.0);
    call(json!({ "cmd": "desk_set", "mic_open": true, "duck": 1.0 }));
    marks.push(("no dip", aired()));
    wait(6.0);
    call(json!({ "cmd": "desk_set", "mic_open": false, "duck": 0.25 }));
    call(json!({ "cmd": "pad_fire", "slot": 0 }));
    marks.push(("pad", aired()));
    wait(3.5);
    call(json!({ "cmd": "desk_set", "two_file": two.to_string_lossy() }));
    marks.push(("second input", aired()));
    wait(5.0);
    call(json!({ "cmd": "desk_set", "two_file": null, "mic_open": true }));
    marks.push(("dips again", aired()));
    wait(3.0);
    call(json!({ "cmd": "desk_set", "mic_gain": 3.16, "deck_gain": 3.16, "pads_gain": 3.16 }));
    call(json!({ "cmd": "pad_fire", "slot": 0 }));
    marks.push(("everything hot", aired()));
    wait(3.0);
    let levels = call(json!({ "cmd": "desk" }));
    println!(
        "{} the desk reports levels (mic {}, deck {}, pads {}, air {})",
        verdict(levels["mic"]["level"].as_f64().is_some_and(|x| x > 0.5) && levels["deck"]["level"].as_f64().is_some_and(|x| x > 1.0) && levels["air"][0].as_f64().is_some_and(|x| x > 0.5 && x <= 0.71)),
        levels["mic"]["level"], levels["deck"]["level"], levels["pads_level"], levels["air"]
    );
    call(json!({ "cmd": "desk_set", "mic_gain": 1.0, "deck_gain": 1.0, "pads_gain": 1.0 }));
    call(json!({ "cmd": "deck_pause" }));
    marks.push(("tail", aired()));
    // Long enough for the listener, some fifteen seconds behind, to hear all of it.
    wait(16.0);
    k.call(&json!({ "cmd": "end_live" }));
    let t = Instant::now();
    while state(&l) == "live" && t.elapsed() < Duration::from_secs(60) {
        std::thread::sleep(Duration::from_millis(250))
    }
    let left = t.elapsed().as_secs_f64();
    println!("{} the listener leaves the live air after the end ({left:.1} s)", verdict(left < 12.0));
    std::thread::sleep(Duration::from_millis(500));
    let total = aired();
    marks.push(("end", total));

    // What the listener was sent, decoded as its player decodes it: a fresh decoder whenever
    // the channels change, and every chunk as long as its frames say.
    let _ = l.rt.block_on(async { tokio::time::timeout(Duration::from_secs(15), l.inner.sync_log(sid)).await });
    let a = l.archive(&sid).ok_or_else(|| anyhow::anyhow!("the listener has no log"))?;
    let lives: Vec<_> = a.entries().iter().filter_map(|e| match e.entry.body() { Body::Live { chunk, channels, .. } => Some((*chunk, *channels)), _ => None }).collect();
    let ended = matches!(a.entries().last().map(|e| e.entry.body()), Some(Body::LiveEnd));
    println!("{} the broadcast ends with its end signed into the log", verdict(ended));
    let mut pcm: Vec<f32> = Vec::new();
    let mut spans: Vec<(u64, u8, u16)> = Vec::new();
    let mut dec: Option<(u8, ChunkDecoder)> = None;
    let mut missing = 0;
    for (c, ch) in &lives {
        let Ok(bytes) = l.rt.block_on(async { tokio::time::timeout(Duration::from_secs(20), l.inner.chunk(&sid, &c.hash)).await }).unwrap_or_else(|_| Err(anyhow::anyhow!("timed out"))) else {
            missing += 1;
            pcm.extend(std::iter::repeat_n(0.0, c.frames as usize * 960 * 2));
            continue;
        };
        if dec.as_ref().is_none_or(|(x, _)| x != ch) {
            dec = Some((*ch, ChunkDecoder::new(*ch)?));
        }
        let Some((_, d)) = dec.as_mut() else { continue };
        spans.push(((pcm.len() / 2) as u64, *ch, c.frames));
        for f in d.decode_as(&bytes, c.frames).chunks(*ch as usize) {
            let l = f[0] as f32 / 32768.0;
            pcm.push(l);
            pcm.push(if *ch == 2 { f[1] as f32 / 32768.0 } else { l });
        }
    }
    let frames = (pcm.len() / 2) as u64;
    println!("{} every chunk reaches the listener ({} chunks, {} missing)", verdict(missing == 0 && !lives.is_empty()), lives.len(), missing);
    println!("{} not a sample of the air is lost ({} frames aired, {} sent, the last frame made up with quiet)", verdict(frames >= total && frames - total < 960), total, frames);

    let mono: Vec<f32> = pcm.chunks(2).map(|f| (f[0] + f[1]) * 0.5).collect();
    let left_ch: Vec<f32> = pcm.chunks(2).map(|f| f[0]).collect();
    let right_ch: Vec<f32> = pcm.chunks(2).map(|f| f[1]).collect();
    // A phase is measured from half a second after its change, so every glide has settled.
    let phase = |name: &str| -> (usize, usize) {
        let i = marks.iter().position(|(n, _)| *n == name).unwrap();
        let a = (marks[i].1 + 24_000).min(frames) as usize;
        let b = (marks[i + 1].1.saturating_sub(4_800)).min(frames) as usize;
        (a, b.max(a))
    };
    let tone = |name: &str, hz: f64| {
        let (a, b) = phase(name);
        amp(&mono[a..b], hz)
    };
    for (name, _) in &marks[..marks.len() - 1] {
        println!("  {name:20} mic {:.3}  deck {:.3}  pad {:.3}  second {:.3}", tone(name, MIC_HZ), tone(name, DECK_HZ), tone(name, PAD_HZ), tone(name, TWO_HZ));
    }
    let (m_talk, d_talk) = (tone("talk", MIC_HZ), tone("talk", DECK_HZ));
    println!("{} the open microphone is heard ({m_talk:.3}) and nothing else", verdict(m_talk > 0.15 && d_talk < 0.01));
    let (m_shut, d_full) = (tone("mic closed", MIC_HZ), tone("mic closed", DECK_HZ));
    println!("{} the closed microphone is not heard ({m_shut:.4})", verdict(m_shut < 0.01));
    let d_dip = tone("deck under the mic", DECK_HZ);
    println!("{} the deck is heard ({d_full:.3})", verdict(d_full > 0.25));
    println!("{} under the open mic the deck dips to about a quarter ({:.2} of its level)", verdict((0.18..0.35).contains(&(d_dip / d_full))), d_dip / d_full);
    // The dip glides both ways: about a fifth of a second down, a little longer back up. Each
    // window is placed from where the change was made, in 48 kHz frames.
    let at = |name: &str| marks.iter().find(|m| m.0 == name).map(|m| m.1 as usize).unwrap_or(0);
    let deck_in = |from: usize, ms: (usize, usize)| amp(&mono[(from + ms.0 * 48).min(mono.len())..(from + ms.1 * 48).min(mono.len())], DECK_HZ);
    let (up0, up1) = (deck_in(at("mic closed"), (0, 30)), deck_in(at("mic closed"), (600, 700)));
    println!("{} and comes back when the mic closes, gliding up ({:.2} at first, {:.2} by 0.6 s, {:.2} of the file's level after)", verdict(up0 < 0.6 * d_full && up1 > 0.9 * d_full && d_full > 0.9 * 0.35), up0 / d_full, up1 / d_full, d_full / 0.35);
    let (dn0, dn1, dn2) = (deck_in(at("dips again"), (0, 30)), deck_in(at("dips again"), (250, 300)), tone("dips again", DECK_HZ));
    println!("{} the dip glides down over about 200 ms ({:.2} at first, {:.2} by 0.25 s, {:.2} after)", verdict(dn0 > 0.6 * d_full && dn1 < 0.35 * d_full && (0.18..0.35).contains(&(dn2 / d_full))), dn0 / d_full, dn1 / d_full, dn2 / d_full);
    let d_none = tone("no dip", DECK_HZ);
    println!("{} with the dip switched off it stays up under the mic ({:.2})", verdict(d_none / d_full > 0.85 && tone("no dip", MIC_HZ) > 0.15), d_none / d_full);
    let p = tone("pad", PAD_HZ);
    println!("{} a pad's sound is heard ({p:.3})", verdict(p > 0.2));
    let (a2, b2) = phase("second input");
    let (tl, tr) = (amp(&left_ch[a2..b2], TWO_HZ), amp(&right_ch[a2..b2], TWO_HZ));
    println!("{} the second input is heard, on its own side ({tl:.3} left, {tr:.3} right)", verdict(tl > 0.15 && tr < tl * 0.3));
    // DESK_DUMP=<dir> keeps what the listener was sent, for a closer look.
    if let Some(d) = std::env::var_os("DESK_DUMP") {
        let d = PathBuf::from(d);
        let _ = std::fs::create_dir_all(&d);
        std::fs::write(d.join("air.f32"), pcm.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>())?;
        std::fs::write(d.join("marks.json"), json!(marks.iter().map(|(n, f)| json!([n, f])).collect::<Vec<_>>()).to_string())?;
        std::fs::write(d.join("spans.json"), json!(spans.iter().map(|(f, c, n)| json!([f, c, n])).collect::<Vec<_>>()).to_string())?;
    }
    let peak = pcm.iter().fold(0f32, |m, x| m.max(x.abs()));
    let (ah, bh) = phase("everything hot");
    let hot_peak = pcm[ah * 2..bh * 2].iter().fold(0f32, |m, x| m.max(x.abs()));
    let all_in = tone("everything hot", MIC_HZ) > 0.05 && tone("everything hot", DECK_HZ) > 0.05 && tone("everything hot", PAD_HZ) > 0.05;
    println!("{} pushed ten decibels over, the air stays under full scale (peak {:.3} there, {:.3} in the whole show)", verdict(peak < 0.98 && all_in), hot_peak, peak);

    // Talk until the deck starts, then music, once, at the change and without a gap.
    let deck_at = marks.iter().find(|m| m.0 == "deck under the mic").map(|m| m.1).unwrap_or(0);
    let first_music = spans.iter().find(|s| s.1 == 2).map(|s| s.0);
    let talk_before = spans.iter().take_while(|s| s.1 == 1).count();
    let back_to_talk = spans.iter().skip_while(|s| s.1 == 1).any(|s| s.1 == 1);
    let switch_ok = first_music.is_some_and(|f| f + 960 >= deck_at && f <= deck_at + 9_600) && talk_before > 0 && !back_to_talk;
    println!("{} the show turns to stereo music when the deck starts ({} talk chunks, music from frame {:?}, deck at {deck_at})", verdict(switch_ok), talk_before, first_music);
    // The voice runs on across the change. A fresh Opus encoder starts with its 6.5 ms of
    // lookahead, which is the most the change may cost: the longest quiet stretch, in 1 ms
    // steps, within a quarter of a second either side of it.
    let sw = first_music.unwrap_or(0) as usize;
    let talk_rms = m_talk / 2f64.sqrt();
    let (mut run, mut longest) = (0, 0);
    for w in (sw.saturating_sub(12_000)..sw + 12_000).step_by(48) {
        if w + 48 > mono.len() {
            break;
        }
        let rms = (mono[w..w + 48].iter().map(|x| (*x as f64).powi(2)).sum::<f64>() / 48.0).sqrt();
        run = if rms < 0.25 * talk_rms { run + 1 } else { 0 };
        longest = longest.max(run);
    }
    println!("{} the voice carries across the change (quiet for {longest} ms at most, the encoder's own restart)", verdict(longest <= 8 && first_music.is_some()));
    // Off the air, a paused track waits where it stopped, even once the desk has rested.
    call(json!({ "cmd": "deck_play", "at": 0 }));
    wait(2.0);
    call(json!({ "cmd": "deck_pause" }));
    wait(3.5);
    let paused = call(json!({ "cmd": "desk" }))["deck"]["pos_ms"].as_u64().unwrap_or(0);
    call(json!({ "cmd": "deck_play" }));
    wait(1.5);
    let on = call(json!({ "cmd": "desk" }));
    let resumed = on["deck"]["pos_ms"].as_u64().unwrap_or(0);
    println!(
        "{} a paused track picks up where it stopped after the desk rests ({paused} ms, then {resumed} ms, level {})",
        verdict(paused >= 1_500 && resumed > paused + 900 && resumed < paused + 2_500 && on["deck"]["level"].as_f64().is_some_and(|x| x > 0.1)),
        on["deck"]["level"]
    );
    call(json!({ "cmd": "deck_pause" }));
    let _ = std::fs::remove_dir_all(&work);
    let _ = std::fs::remove_dir_all("/tmp/desk-keeper");
    let _ = std::fs::remove_dir_all("/tmp/desk-listener");
    Ok(())
}
