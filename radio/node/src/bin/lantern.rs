//! Lantern: a headless node that runs all day on a spare machine.
//! It carries every station it hears of, relays for anyone, and can raise
//! stations of its own from folders of audio.
//!
//! lantern --dir <data> [--quota-gb 20] [--bootstrap <id>,<id>] [--public-dns]
//!         [--seed "Name|About|folder|music|Home port"]...   (the home port is optional)
//!         The kind (music, mono or talk) may carry a rating after a slash: /mature or /explicit
//!         (e.g. "...|talk/explicit"), the station's own word that a listener sees before it plays.
//!         [--carry-own]   carry only this Lantern's own stations, and drop anything else it held
//!                         (by default it carries the whole Log;
//!                         maroon.txt, one station id per line, takes any station off this Lantern)
//!         A seed folder's bell.mp3 (or .wav, .ogg) plays on the hour; replace it and the new one takes over.
//!         A seed folder's station.jpg and banner.jpg become the station's art.
//!         A seed folder's programme.json is the station's programme, in the JSON the app writes,
//!         with "utc_offset_min" for the folder's own time and recordings named by title.
//!         A seed folder's notices.txt holds the station's notices, one per paragraph.
//!         [--http 127.0.0.1:8080] the web listener; [--player <folder>] serves the open player
//!         (browser/www/dist-open) on it at /listen/. Never at a HOSTIS name.
//!         The hold key comes from a passphrase (see HELP below and LANTERN.md); stations are
//!         carried blind unless kept open with --open.

use std::{collections::HashSet, path::PathBuf, time::Duration};

use pyrate_core::{Body, Profile};
use pyrate_node::{Config, Node, now_ms};
use zeroize::Zeroizing;

const HELP: &str = "A Pyrate Radio Lantern is a node that runs all day on a spare machine and carries the network.

lantern --dir <folder> [flags]

The hold key
  Everything a Lantern keeps is sealed under its hold key, and the hold key is sealed under a
  passphrase that never lies beside the data. The Lantern takes the passphrase from the first of
    --hold-pass-fd <n>            a file descriptor, such as 3 with 3</media/usb/pass
    PYRATE_HOLD_PASS_FILE=<file>  a file, best on a removable disk or in a secrets store
    PYRATE_HOLD_PASS_CMD=<cmd>    a command that prints it, such as the system keyring's own
                                  (secret-tool lookup service pyrate-hold, or on a Mac
                                  security find-generic-password -w -s pyrate-hold)
  or asks for it at the terminal. Twelve characters or more; four or five random words are good.
  Without it the Lantern opens nothing and starts nothing. Lose it and the hold is lost.
  The first start with a passphrase seals an older Lantern's hold.key and removes the plain file.
  --hold-key-on-disk    keep the key in the clear in hold.key (or PYRATE_HOLD_KEY) instead.
                        Whoever takes the disk opens everything. Warned at every start.
  --scuttle             destroy the hold key, sealed or plain, and everything kept becomes noise.

What it carries
  --carry-own           carry only this Lantern's own stations
  --quota-gb <n>        disk for carried sound (default 20)
  --open <station>      keep a carried station open, named by its id, by a file of ids (such as
                        HOSTIS's harbour list, radio/browser/harbour.json) or by all. Repeatable.
                        Its own stations are always open. Any other is carried blind, with no
                        clear answer, no carrier note and no web page, and its sound passed on
                        sealed.
  --seed \"Name|About|folder|music|Home port\"   raise and keep a station from a folder
  --reseed \"Name\"  --withdraw \"words\"  --words <file>   see LANTERN.md

The network and the web
  --bootstrap <id>,<id>   first doors, each best as id@relay; --relay <addr> run a relay;
                          --public-dns; --release-keys <file> to take seed lists (and releases)
  PYRATE_DARK=1 with PYRATE_PROXY=<Tor's HTTP tunnel>   run dark: relays only, through Tor,
                          no address published and no DHT; radio/lantern-dark/ sets it all up
  PYRATE_DHT=0            leave no note in the DHT (a Lantern not running dark leaves one)
  --http <addr>           the web page and podcast feeds (open stations only); --web-all for
                          every open station, not just its own; --player <folder> the open player
  --door <addr|auto> --door-links --door-kept --lan --listener   see LANTERN.md
  --open-hails <folder>   open the hails this Lantern kept into a folder, then stop

The log
  --id                  print this Lantern's node id and stop (it may be running meanwhile)
  --show-ids            print this Lantern's node id, and the stations, peers and paths it meets.
                        By default its log names none of them.
  PYRATE_QUIET=1        say almost nothing
";

/// The passphrase for the hold, from the first place the keeper put it, or None.
fn passphrase(args: &[String], dir: &std::path::Path) -> anyhow::Result<Option<Zeroizing<String>>> {
    let get = |flag: &str| args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).cloned();
    let mut raw = Zeroizing::new(String::new());
    if let Some(fd) = get("--hold-pass-fd") {
        let fd: i32 = fd.parse().map_err(|_| anyhow::anyhow!("--hold-pass-fd takes a file descriptor number, such as 3"))?;
        #[cfg(unix)]
        {
            use std::io::Read;
            use std::os::fd::FromRawFd;
            // SAFETY: the keeper handed this descriptor to the Lantern for this one read.
            let mut f = unsafe { std::fs::File::from_raw_fd(fd) };
            f.read_to_string(&mut raw).map_err(|e| anyhow::anyhow!("the passphrase could not be read from descriptor {fd} ({e})"))?;
        }
        #[cfg(not(unix))]
        anyhow::bail!("--hold-pass-fd works on Linux and macOS; use PYRATE_HOLD_PASS_FILE here (descriptor {fd})");
    } else if let Some(p) = std::env::var_os("PYRATE_HOLD_PASS_FILE") {
        let b = Zeroizing::new(std::fs::read(&p).map_err(|e| anyhow::anyhow!("PYRATE_HOLD_PASS_FILE names {}, which could not be read ({e}). Is its disk mounted?", std::path::Path::new(&p).display()))?);
        raw.push_str(std::str::from_utf8(&b).map_err(|_| anyhow::anyhow!("the passphrase file is not text"))?);
    } else if let Ok(cmd) = std::env::var("PYRATE_HOLD_PASS_CMD") {
        let out = if cfg!(windows) { std::process::Command::new("cmd").args(["/C", &cmd]).stderr(std::process::Stdio::inherit()).output() } else { std::process::Command::new("sh").args(["-c", &cmd]).stderr(std::process::Stdio::inherit()).output() };
        let out = out.map_err(|e| anyhow::anyhow!("PYRATE_HOLD_PASS_CMD could not run ({e})"))?;
        let b = Zeroizing::new(out.stdout);
        if !out.status.success() {
            anyhow::bail!("PYRATE_HOLD_PASS_CMD ended with {} and gave no passphrase", out.status);
        }
        raw.push_str(std::str::from_utf8(&b).map_err(|_| anyhow::anyhow!("PYRATE_HOLD_PASS_CMD printed something that is not text"))?);
    } else if let Some(p) = typed(dir)? {
        return Ok(Some(p));
    } else {
        return Ok(None);
    }
    let pass = Zeroizing::new(raw.trim_end_matches(['\n', '\r']).to_string());
    if pass.is_empty() {
        anyhow::bail!("the passphrase given for the hold is empty");
    }
    Ok(Some(pass))
}

/// The passphrase typed at the terminal, unseen, and typed twice for a hold sealed for the first
/// time. None when no one is at a terminal.
fn typed(dir: &std::path::Path) -> anyhow::Result<Option<Zeroizing<String>>> {
    #[cfg(unix)]
    {
        if unsafe { libc::isatty(0) } != 1 || unsafe { libc::isatty(2) } != 1 {
            return Ok(None);
        }
        let ask = |prompt: &str| -> anyhow::Result<Zeroizing<String>> {
            eprint!("{prompt}");
            let mut t: libc::termios = unsafe { std::mem::zeroed() };
            let had = unsafe { libc::tcgetattr(0, &mut t) } == 0;
            let quiet = libc::termios { c_lflag: t.c_lflag & !libc::ECHO, ..t };
            if had {
                unsafe { libc::tcsetattr(0, libc::TCSANOW, &quiet) };
            }
            let mut line = Zeroizing::new(String::new());
            let read = std::io::stdin().read_line(&mut line);
            if had {
                unsafe { libc::tcsetattr(0, libc::TCSANOW, &t) };
            }
            eprintln!();
            read?;
            Ok(Zeroizing::new(line.trim_end_matches(['\n', '\r']).to_string()))
        };
        let first = ask("Hold passphrase: ")?;
        if first.is_empty() {
            return Ok(None);
        }
        if !dir.join(pyrate_node::hold::SEALED_KEY).exists() && ask("The same again: ")? != first {
            anyhow::bail!("the two passphrases differ, so nothing was sealed");
        }
        Ok(Some(first))
    }
    #[cfg(not(unix))]
    {
        let _ = dir;
        Ok(None)
    }
}

/// The stations named by --open: ids, files of ids (any 64-character hex in them), or all.
fn open_list(args: &[String]) -> anyhow::Result<(Vec<pyrate_core::StationId>, bool)> {
    let (mut ids, mut all) = (Vec::new(), false);
    for v in args.windows(2).filter(|w| w[0] == "--open").map(|w| w[1].trim().to_string()) {
        if v == "all" {
            all = true;
        } else if let Ok(id) = pyrate_core::StationId::from_hex(&v) {
            ids.push(id);
        } else {
            let text = std::fs::read_to_string(&v).map_err(|e| anyhow::anyhow!("--open {v} is neither a station id nor a file that can be read ({e})"))?;
            let before = ids.len();
            for word in text.split(|c: char| !c.is_ascii_hexdigit()) {
                if word.len() == 64 {
                    if let Ok(id) = pyrate_core::StationId::from_hex(word) {
                        ids.push(id);
                    }
                }
            }
            if ids.len() == before {
                anyhow::bail!("--open {v} names a file with no station ids in it");
            }
        }
    }
    ids.sort_by_key(|i| i.0);
    ids.dedup();
    Ok((ids, all))
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print!("{HELP}");
        return Ok(());
    }
    // An unknown flag stops the Lantern before it starts: a mistyped or test flag must never
    // bring up a stray node on whatever network the shell happens to be on.
    const VALUED: &[&str] = &["--dir", "--quota-gb", "--bootstrap", "--seed", "--http", "--open-hails", "--relay", "--release-keys", "--words", "--withdraw", "--reseed", "--player", "--door", "--open", "--hold-pass-fd"];
    const BARE: &[&str] = &["--carry-own", "--public-dns", "--scuttle", "--web-all", "--listener", "--door-links", "--door-kept", "--lan", "--hold-key-on-disk", "--show-ids", "--id"];
    let mut i = 1;
    while i < args.len() {
        let a = args[i].as_str();
        if VALUED.contains(&a) {
            i += 2;
        } else if BARE.contains(&a) {
            i += 1;
        } else {
            eprintln!("lantern: {a} is not a flag this Lantern knows. Flags: {} {}. See lantern --help and LANTERN.md.", VALUED.join(" "), BARE.join(" "));
            std::process::exit(2);
        }
    }
    let get = |flag: &str| args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).cloned();
    let dir = PathBuf::from(get("--dir").unwrap_or_else(|| "lantern-data".into()));
    let quota = get("--quota-gb").and_then(|q| q.parse::<u64>().ok()).unwrap_or(20) * 1_000_000_000;
    // Each door an id, or `id@relay` to reach it without a lookup.
    let bootstrap = get("--bootstrap").map(|b| pyrate_node::doors_from(&b)).unwrap_or_default();
    let seeds: Vec<String> = args.windows(2).filter(|w| w[0] == "--seed").map(|w| w[1].clone()).collect();
    std::fs::create_dir_all(&dir)?;

    // Scuttle: the hold key goes, sealed or plain, and everything this Lantern kept becomes
    // noise at once. No passphrase is needed for that.
    if args.iter().any(|a| a == "--scuttle") {
        pyrate_node::hold::scuttle(&dir)?;
        println!("scuttled: {} is noise now; delete it at leisure", dir.display());
        return Ok(());
    }
    // Lines naming stations, peers and this node stay out of the log unless asked for.
    let show_ids = args.iter().any(|a| a == "--show-ids");
    pyrate_node::NAMES.store(show_ids, std::sync::atomic::Ordering::Relaxed);
    let (open, open_all) = match open_list(&args) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("lantern: {e}");
            std::process::exit(2);
        }
    };
    // The hold key, from the keeper's passphrase. Without it this Lantern opens nothing.
    let key = match passphrase(&args, &dir).and_then(|p| pyrate_node::hold::lantern_key(&dir, p.as_ref().map(|p| p.as_bytes()), args.iter().any(|a| a == "--hold-key-on-disk"))) {
        Ok(k) => k,
        Err(e) => {
            eprintln!("lantern: {e}");
            std::process::exit(1);
        }
    };
    // --id: this Lantern's node id, for its keeper, read from the hold; nothing is started and
    // nothing is written, so it may be asked while the Lantern runs.
    if args.iter().any(|a| a == "--id") {
        let hold = pyrate_node::hold::Hold::new(&key);
        let seed = hold.read_note(&dir, "node-key", None).filter(|b| b.len() >= 32).ok_or_else(|| anyhow::anyhow!("this Lantern has no id yet: start it once"))?;
        let secret = iroh::SecretKey::from_bytes(&<[u8; 32]>::try_from(&seed[..32]).unwrap());
        println!("node id: {}", secret.public());
        return Ok(());
    }
    // The keeper, back: the hails this Lantern kept, opened into a folder of their choosing.
    if let Some(out) = get("--open-hails") {
        let n = pyrate_node::open_hails(&dir, std::path::Path::new(&out), Some(*key))?;
        println!("{n} {} opened into {out}", if n == 1 { "hail" } else { "hails" });
        return Ok(());
    }
    // --listener runs as a listener's app does (for the test network): no Lantern duties, fetching
    // through routes, carrying for the crew.
    let listener = args.iter().any(|a| a == "--listener");
    pyrate_node::set_hold_key(*key);
    drop(key);
    let node = Node::start(Config { dir: dir.clone(), quota, lantern: !listener, carry_all: !listener && !args.iter().any(|a| a == "--carry-own"), bootstrap, public_dns: args.iter().any(|a| a == "--public-dns"), private: listener, ballast: 0, release_keys: get("--release-keys").map(|p| std::fs::read_to_string(p).unwrap_or_default().lines().filter_map(|l| pyrate_core::StationId::from_hex(l.trim()).ok().map(|i| i.0)).collect()).unwrap_or_default() })?;
    node.keep_open(&open, open_all);
    println!("LANTERN LIT");
    // Counts only, as everywhere in this log.
    if open_all {
        println!("every station it carries is kept open");
    } else {
        println!("{} stations named open with --open, and its own are open too; any other is carried blind", open.len());
    }
    if listener {
        node.call(&serde_json::json!({ "cmd": "ballast_ok", "value": false, "unmetered": true }));
    }
    // --lan: find radios on this network with no relay and no internet (the test network's doors).
    if args.iter().any(|a| a == "--lan") {
        node.call(&serde_json::json!({ "cmd": "lan", "value": true }));
    }
    // --door <ip:port> opens a door for browsers at that outside address; --door auto asks the
    // home router for a port (UPnP-IGD, NAT-PMP, PCP). A Lantern's door is kept and listed. A
    // --listener's is a listener's app door, opened when the network needs one and introduced by
    // middles, never listed, unless --door-kept says its keeper keeps it as a listed door.
    if let Some(d) = get("--door") {
        let app = listener && !args.iter().any(|a| a == "--door-kept");
        let r = node.call(&serde_json::json!({ "cmd": "door", "on": true, "app": app, "links": args.iter().any(|a| a == "--door-links"), "addr": if d == "auto" { serde_json::Value::Null } else { d.into() } }));
        let _ = r;
        std::thread::sleep(std::time::Duration::from_secs(4));
        println!("door {}", node.call(&serde_json::json!({ "cmd": "door" })));
    }
    // Carrying only its own stations: whatever else it held before goes, now.
    if args.iter().any(|a| a == "--carry-own") {
        let r = node.call(&serde_json::json!({ "cmd": "clear_hold" }));
        println!("carrying only this Lantern's own stations {r}");
    }
    // --http 127.0.0.1:8080 serves the web listener; --relay 127.0.0.1:3340 runs a relay. Both
    // bind to loopback by default: a tunnel puts them on the public internet, not this machine.
    if let Some(addr) = get("--http").and_then(|a| a.parse().ok()) {
        let inner = node.inner.clone();
        let web_all = args.iter().any(|a| a == "--web-all");
        if let Some(dir) = get("--player") {
            pyrate_node::web::set_player(PathBuf::from(dir));
        }
        if pyrate_node::web::has_player() {
            println!("open player on http://{addr}/listen/");
        } else if get("--player").is_some() {
            println!("open player: no index.html in that folder, so /listen/ serves nothing");
        }
        node.rt.spawn(async move {
            if let Err(e) = pyrate_node::web::serve(inner, addr, web_all).await {
                println!("web listener: {e}");
            }
        });
    }
    if let Some(addr) = get("--relay").and_then(|a| a.parse::<std::net::SocketAddr>().ok()) {
        node.rt.spawn(async move {
            let mut cfg = iroh_relay::server::ServerConfig::default();
            let mut relay = iroh_relay::server::RelayConfig::new(addr);
            // No one client can take the relay for itself: a megabyte a second each (sixteen
            // times what a stereo station needs), bursts of eight for a stem's cargo, and new
            // connections at fifty a second.
            let mut rx = iroh_relay::server::ClientRateLimit::new(std::num::NonZeroU32::new(1_000_000).unwrap());
            rx.max_burst_bytes = std::num::NonZeroU32::new(8_000_000);
            relay.limits.client_rx = Some(rx);
            relay.limits.accept_conn_limit = Some(50.0);
            relay.limits.accept_conn_burst = Some(200);
            cfg.relay = Some(relay);
            match iroh_relay::server::Server::spawn(cfg).await {
                Ok(server) => {
                    println!("relay on http://{addr}");
                    std::mem::forget(server);
                }
                Err(e) => println!("relay: {e}"),
            }
        });
    }
    if show_ids {
        println!("node id: {}", node.id());
    }

    // --withdraw "words": takes matching recordings off our own stations' clocks.
    let withdraw: Vec<String> = args.windows(2).filter(|w| w[0] == "--withdraw").map(|w| w[1].to_lowercase()).collect();
    if !withdraw.is_empty() {
        let mine: Vec<String> = node.call(&serde_json::json!({ "cmd": "status" }))
            .get("mine").and_then(|m| m.get("id")).and_then(|v| v.as_str()).map(String::from).into_iter().collect();
        let _ = mine;
        for name in seeds.iter().filter_map(|s| s.split('|').next()) {
            let Some(id) = node.inner_store_find(name) else { continue };
            let lib = node.call(&serde_json::json!({ "cmd": "library", "station": id.to_hex() }));
            for t in lib.as_array().into_iter().flatten() {
                let title = format!("{} {}", t["by"].as_str().unwrap_or(""), t["title"].as_str().unwrap_or("")).to_lowercase();
                if withdraw.iter().any(|w| title.contains(w)) {
                    node.call(&serde_json::json!({ "cmd": "withdraw", "station": id.to_hex(), "track": t["hash"] }));
                    println!("withdrew {title}");
                }
            }
        }
    }
    // --words <file>: writes the 24 words of every station this Lantern holds the master key
    // for into <file>, readable by its owner only, and stops. For the keeper to copy onto
    // paper; never print the file.
    if let Some(out) = get("--words") {
        let list = node.call(&serde_json::json!({ "cmd": "own_words" }));
        let mut text = String::from("Pyrate Radio station keys. Copy each set of words onto paper, in order, then delete this file.\n\n");
        let mut n = 0;
        for s in list.as_array().into_iter().flatten() {
            if s["master"] == true {
                text += &format!("{}\n{}\n{}\n\n", s["name"].as_str().unwrap_or(""), s["id"].as_str().unwrap_or(""), s["words"].as_str().unwrap_or(""));
                n += 1;
            }
        }
        pyrate_node::hold::write_secret(std::path::Path::new(&out), text.as_bytes())?;
        println!("the 24 words of {n} stations are in {out}: copy them onto paper, then delete the file");
        return Ok(());
    }
    // --reseed "Station name": for this start only, every recording in that station's folder
    // is set again with the seed's profile (as when a station moves to mono), and the old ones
    // are withdrawn once the new are on the clock, so the station never falls silent.
    let reseed: Vec<String> = args.windows(2).filter(|w| w[0] == "--reseed").map(|w| w[1].clone()).collect();
    seed_all(&node, &seeds, &reseed)?;
    // The seeded stations by name and id, once at start: what the harbour list needs.
    for name in seeds.iter().filter_map(|s| s.split('|').next()) {
        if let Some(id) = node.inner_store_find(name) {
            println!("station {name}: {}", id.to_hex());
        }
    }
    for line in node.call(&serde_json::json!({ "cmd": "status" }))["mine"].as_object().into_iter() {
        let _ = line;
    }
    // For the crowd test: what this Lantern serves and passes on, every few seconds.
    if let Some(every) = std::env::var("PYRATE_STATS_S").ok().and_then(|v| v.parse::<u64>().ok()).filter(|s| *s > 0) {
        let n2 = node.inner.clone();
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_secs(every));
            let (s, r) = (n2.served_bytes.load(std::sync::atomic::Ordering::Relaxed), n2.relayed_bytes.load(std::sync::atomic::Ordering::Relaxed));
            println!("stats {} served {s} relayed {r}", now_ms());
        });
    }
    let mut n = 0u64;
    loop {
        std::thread::sleep(Duration::from_secs(60));
        n += 1;
        // New files in a seed folder go on the clock within ten minutes.
        if n % 10 == 0 {
            if let Err(e) = seed_all(&node, &seeds, &[]) {
                println!("seeding: {e}");
            }
        }
        // Counts only: a Lantern's own output never lists the stations it carries.
        let st = node.call(&serde_json::json!({ "cmd": "stations" }));
        let n = st.as_array().map(|a| a.len()).unwrap_or(0);
        let mine = node.call(&serde_json::json!({ "cmd": "status" }));
        let own = mine["mine"]["name"].as_str().map(|m| format!(" · keeping {m}")).unwrap_or_default();
        println!("{} · {n} stations in the Log{own}", now_ms() / 1000);
        let _ = Body::Strike; // keep the import honest across features
    }
}

fn seed_all(node: &Node, seeds: &[String], reseed: &[String]) -> anyhow::Result<()> {
    for seed in seeds {
        let parts: Vec<&str> = seed.split('|').collect();
        let (name, about, folder, kind, port) = match parts[..] {
            [n, a, f, k] => (n, a, f, k, None),
            [n, a, f, k, p] => (n, a, f, k, Some(p.trim()).filter(|p| !p.is_empty())),
            _ => {
                eprintln!("seed must be Name|About|folder|music, mono or talk, with |Home port after it if you like");
                continue;
            }
        };
        // The kind may carry a rating after a slash, the station's own word on itself:
        // music, talk or mono, with /mature or /explicit after it (Contraband|...|talk/explicit).
        let (base, mature, explicit) = pyrate_node::parse_kind(kind);
        // "mono" is music recorded in one channel (old records): half the bytes, same sound.
        let profile = match base {
            "talk" => Profile::Talk,
            "mono" => Profile::Mono,
            _ => Profile::Music,
        };
        let existing = {
            let s = node.inner_store_find(name);
            s
        };
        let id = match existing {
            Some(id) => id,
            None => node.raise(name, about, port.map(String::from))?,
        };
        // The seed's words are the station's: a changed about or home port goes up.
        if let Some(Body::Colours { about: a0, port: p0, .. }) = node.archive(&id).and_then(|a| a.colours().cloned()) {
            if a0 != about || p0.as_deref() != port {
                let r = node.call(&serde_json::json!({ "cmd": "colours_set", "station": id.to_hex(), "about": about, "port": port.unwrap_or("") }));
                println!("  colours set {r}");
            }
        }

        // A station seeded with no kinds of its own takes the seed's, music or talk, and the
        // seed's rating if it names one, so the Log's filters find it and an explicit station
        // stays off the open dial until a listener asks for it. Kinds set since by other means
        // are left alone, but a rating named in the seed is the keeper's word, set if it is not
        // already so (so an older station can be marked explicit by adding /explicit to its seed).
        if node.archive(&id).is_some_and(|a| a.details().is_none()) {
            let k = if base == "talk" { "talk" } else { "music" };
            let r = node.call(&serde_json::json!({ "cmd": "details_set", "station": id.to_hex(), "kinds": [k], "mature": mature, "explicit": explicit }));
            println!("  kinds set {r}");
        } else if mature || explicit {
            let a = node.archive(&id);
            let (kinds, cur_mature) = a.as_ref().and_then(|a| a.details()).unwrap_or_else(|| (vec![if base == "talk" { "talk".to_string() } else { "music".to_string() }], false));
            let cur_explicit = a.as_ref().is_some_and(|a| a.explicit());
            if cur_mature != mature || cur_explicit != explicit {
                let r = node.call(&serde_json::json!({ "cmd": "details_set", "station": id.to_hex(), "kinds": kinds, "mature": mature, "explicit": explicit }));
                println!("  rating set {r}");
            }
        }

        // station.jpg and banner.jpg in the folder are the station's art.
        let current = node.archive(&id).and_then(|a| a.art());
        for (file, kind) in [("station", "cover"), ("banner", "banner")] {
            let Some(path) = ["jpg", "jpeg", "png", "webp"].iter().map(|e| PathBuf::from(folder).join(format!("{file}.{e}"))).find(|p| p.exists()) else { continue };
            let Ok(raw) = std::fs::read(&path) else { continue };
            let k = if kind == "banner" { pyrate_core::art::Kind::Banner } else { pyrate_core::art::Kind::Cover };
            let Ok(p) = pyrate_core::art::prepare(&raw, k) else { println!("  {} is not a picture we can read", path.display()); continue };
            let new = *blake3::hash(&p.jpeg).as_bytes();
            let old = current.and_then(|c| if kind == "banner" { c.1 } else { c.0 }).map(|r| r.hash);
            if old != Some(new) {
                let r = node.call(&serde_json::json!({ "cmd": "art_set", "station": id.to_hex(), "kind": kind, "path": path }));
                println!("  {kind} raised {r}");
            }
        }
        ring_the_bell(node, id, folder, profile);
        let again = reseed.iter().any(|r| r.trim() == name.trim());
        let old: Vec<(serde_json::Value, String)> = if again {
            node.call(&serde_json::json!({ "cmd": "library", "station": id.to_hex() })).as_array().cloned().unwrap_or_default().into_iter().map(|t| (t["hash"].clone(), t["title"].as_str().unwrap_or("").to_string())).collect()
        } else {
            Vec::new()
        };
        if again {
            println!("  reseeding {name}: {} recordings set again, then the old ones withdrawn", old.len());
        }
        let have: HashSet<String> = if again { HashSet::new() } else { node.titles(&id).into_iter().collect() };
        let mut added = false;
        let mut set: HashSet<String> = HashSet::new();
        let mut files: Vec<PathBuf> = std::fs::read_dir(folder)?
            .filter_map(|f| f.ok().map(|f| f.path()))
            .filter(|p| p.extension().is_some_and(|e| ["mp3", "ogg", "flac", "wav", "m4a", "opus", "oga"].contains(&e.to_string_lossy().to_lowercase().as_str())))
            .filter(|p| p.file_stem().is_none_or(|s| s != "bell"))
            .collect();
        files.sort();
        let in_folder: HashSet<String> = files
            .iter()
            .map(|f| {
                let stem = f.file_stem().unwrap().to_string_lossy().to_string();
                stem.split_once(" - ").map(|(_, t)| t.trim().to_string()).unwrap_or(stem)
            })
            .collect();
        for f in files {
            let stem = f.file_stem().unwrap().to_string_lossy().to_string();
            let (by, title) = match stem.split_once(" - ") {
                Some((b, t)) => (b.trim().to_string(), t.trim().to_string()),
                None => (String::new(), stem.clone()),
            };
            if have.contains(&title) {
                continue;
            }
            let source = std::fs::read_to_string(f.with_extension("source.txt")).unwrap_or_default();
            match node.add_track(id, &f, &title, &by, source.trim(), profile) {
                Ok(_) => {
                    println!("  set {title}");
                    added = true;
                    set.insert(title.clone());
                }
                Err(e) => println!("  could not set {title}: {e}"),
            }
        }
        // The old recordings go only once the new ones are on the clock.
        // The folder becomes the station: an old recording goes if it was set again, or if its
        // file has left the folder. One whose file is there but could not be set stays.
        if again && added {
            let gone: Vec<_> = old.iter().filter(|(_, t)| set.contains(t) || !in_folder.contains(t)).collect();
            for (h, _) in &gone {
                node.call(&serde_json::json!({ "cmd": "withdraw", "station": id.to_hex(), "track": h }));
            }
            println!("  reseeded {name}: {} old recordings withdrawn, {} kept", gone.len(), old.len() - gone.len());
        }
        // Recordings first, so the programme can name the ones just set.
        keep_programme(node, id, folder);
        keep_notices(node, id, folder);
    }
    Ok(())
}

/// programme.json: the programme the station keeps. The same JSON the app writes, with two
/// conveniences for a folder: "utc_offset_min" stands in for the app's table of offsets, and
/// a recording can be named by its title, "Artist - Title", or its file name.
fn keep_programme(node: &Node, id: pyrate_core::StationId, folder: &str) {
    let Ok(text) = std::fs::read_to_string(PathBuf::from(folder).join("programme.json")) else { return };
    let mut v: serde_json::Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(e) => {
            println!("  programme.json does not read: {e}");
            return;
        }
    };
    let Some(a) = node.archive(&id) else { return };
    let lib = node.call(&serde_json::json!({ "cmd": "library", "station": id.to_hex() }));
    let find = |name: &str| -> Option<String> {
        if name.len() == 64 && name.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Some(name.to_lowercase());
        }
        let name = name.trim();
        let stem = name.rsplit_once('.').filter(|(_, e)| e.len() <= 4).map(|(s, _)| s).unwrap_or(name);
        lib.as_array().into_iter().flatten().find(|t| {
            let (title, by) = (t["title"].as_str().unwrap_or(""), t["by"].as_str().unwrap_or(""));
            let full = format!("{by} - {title}");
            title == name || title == stem || full == name || full == stem
        }).and_then(|t| t["hash"].as_str().map(String::from))
    };
    for slot in v["slots"].as_array_mut().into_iter().flatten() {
        if let Some(list) = slot["fill"]["tracks"].as_array_mut() {
            *list = list.iter().filter_map(|t| {
                let found = t.as_str().and_then(find);
                if found.is_none() {
                    println!("  programme.json names a recording not on the clock: {t}");
                }
                found.map(serde_json::Value::from)
            }).collect();
        }
    }
    if v.get("zone").is_none_or(|z| z.as_array().is_none_or(|z| z.is_empty())) {
        let off = v["utc_offset_min"].as_i64().unwrap_or(0);
        v["zone"] = serde_json::json!([[0, off]]);
    }
    // The bell keeps ringing unless the programme names its own.
    let current_ident = a.programme().and_then(|p| p.ident).or(a.rota().ident).map(|h| pyrate_node::hex(&h));
    match v["ident"].as_str().map(String::from) {
        Some(name) => v["ident"] = find(&name).map(serde_json::Value::from).unwrap_or(serde_json::Value::Null),
        None if v.get("ident").is_none() => {
            v["ident"] = current_ident.map(serde_json::Value::from).unwrap_or(serde_json::Value::Null);
            if v.get("ident_every_min").is_none() {
                v["ident_every_min"] = 60.into();
            }
        }
        None => {}
    }
    let want = match pyrate_core::page::programme_from(&v) {
        Ok(p) => p,
        Err(e) => {
            println!("  programme.json: {e}");
            return;
        }
    };
    if a.programme().as_ref() == Some(&want) {
        return;
    }
    let r = node.call(&serde_json::json!({ "cmd": "programme_set", "station": id.to_hex(), "programme": pyrate_core::page::programme_json(&want) }));
    println!("  programme set {r}");
}

/// notices.txt: the station's notices, one per paragraph. A new paragraph goes up; a notice
/// this Lantern put up comes down when its paragraph leaves the file. Notices a phone posted
/// are the phone's own business.
fn keep_notices(node: &Node, id: pyrate_core::StationId, folder: &str) {
    let Ok(text) = std::fs::read_to_string(PathBuf::from(folder).join("notices.txt")) else { return };
    let want: Vec<String> = text.replace("\r\n", "\n").split("\n\n").map(|p| p.split_whitespace().collect::<Vec<_>>().join(" ")).filter(|p| !p.is_empty()).collect();
    let Some(a) = node.archive(&id) else { return };
    let up = a.notices(now_ms());
    let ours: HashSet<[u8; 32]> = a.entries().iter().filter(|e| matches!(e.entry.body, Body::Notice { .. })).filter_map(|e| e.hash().ok()).collect();
    for (h, _, words) in &up {
        if ours.contains(h) && !want.contains(words) {
            node.call(&serde_json::json!({ "cmd": "notice_remove", "station": id.to_hex(), "notice": pyrate_node::hex(h) }));
            println!("  notice taken down: {words}");
        }
    }
    for w in want.iter().rev() {
        if !up.iter().any(|(_, _, t)| t == w) {
            node.call(&serde_json::json!({ "cmd": "notice_post", "station": id.to_hex(), "text": w, "until_ms": 0 }));
            println!("  notice up: {w}");
        }
    }
}

/// bell.* in a seed folder is the station's bell: it plays on the hour. When the file changes,
/// the new one is set, the old one withdrawn, and the clock points the bell at it.
fn ring_the_bell(node: &Node, id: pyrate_core::StationId, folder: &str, profile: Profile) {
    let Some(path) = ["mp3", "wav", "ogg", "m4a", "flac", "opus"].iter().map(|e| PathBuf::from(folder).join(format!("bell.{e}"))).find(|p| p.exists()) else { return };
    let Ok(bytes) = std::fs::read(&path) else { return };
    // The bell's fingerprint rides in its source line, so a read-only folder works too.
    let mark = format!("bell {}", &blake3::hash(&bytes).to_hex()[..16]);
    let set = node.archive(&id).is_some_and(|a| a.entries().iter().any(|e| matches!(e.entry.body(), Body::Track { source, .. } if *source == mark)));
    if set {
        return;
    }
    let title = format!("The Bell, {}", chrono_day());
    let Ok(track) = node.add_track(id, &path, &title, "", &mark, profile) else {
        println!("  the bell would not set");
        return;
    };
    let prog = node.call(&serde_json::json!({ "cmd": "programme", "station": id.to_hex() }));
    if let Some(mut p) = prog.get("programme").filter(|p| p.is_object()).cloned() {
        // A programme is in force: the bell changes inside it, and nothing else does.
        if let Some(old) = p["ident"].as_str() {
            node.call(&serde_json::json!({ "cmd": "withdraw", "station": id.to_hex(), "track": old }));
        }
        p["ident"] = pyrate_node::hex(&track).into();
        let r = node.call(&serde_json::json!({ "cmd": "programme_set", "station": id.to_hex(), "programme": p }));
        println!("  the bell is set: {title} {r}");
        return;
    }
    let rota = node.call(&serde_json::json!({ "cmd": "rota", "station": id.to_hex() }));
    if let Some(old) = rota["ident"].as_str() {
        node.call(&serde_json::json!({ "cmd": "withdraw", "station": id.to_hex(), "track": old }));
    }
    let every = rota["ident_every_min"].as_u64().filter(|m| *m >= 5).unwrap_or(60);
    let r = node.call(&serde_json::json!({
        "cmd": "rota_set", "station": id.to_hex(), "blocks": rota["blocks"], "ident": pyrate_node::hex(&track), "ident_every_min": every,
    }));
    println!("  the bell is set: {title} {r}");
}

/// Today's date in words, from the clock alone.
fn chrono_day() -> String {
    let days = pyrate_node::now_ms() / 86_400_000;
    let (y, m, d) = civil(days as i64);
    const MONTHS: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
    format!("{d} {} {y}", MONTHS[(m - 1) as usize])
}

/// Days since 1970 to a calendar date (Howard Hinnant's algorithm).
fn civil(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}
