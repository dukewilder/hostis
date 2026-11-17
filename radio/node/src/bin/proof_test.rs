//! proof_test <dir> <wav>
//! What a blind carrier holds, proved by looking. A keeper's phone raises a station kept to its
//! link and puts a recording on it; its stem goes through a listener's phone, its first hop,
//! to a Lantern that carries every station blind, which ends it; a second listener plays the
//! station through the same phone and the Lantern. The phone and the Lantern run as processes
//! of their own. Then their data folders and their memory are searched for the station's id
//! (as bytes and as hex), its call sign, its name and blurb, the recording's title and artist,
//! and stretches of the recording's sound as it plays. Any hit in a folder, or in the phone's
//! memory, fails. The Lantern must hold a station it carries to serve it, so its memory is
//! searched and what it holds is counted and said, not failed; the check on it is that no
//! other program may read that memory and that a crash writes no core file.
//! Run under the test network's lock, with the test network's environment.
use std::{
    io::{BufRead, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::Duration,
};

use pyrate_core::{Body, StationId};
use pyrate_node::{Config, Node, net::{Request, Response}};
use serde_json::json;

fn check(c: bool, what: &str) {
    println!("{} {what}", if c { "PASS" } else { "FAIL" });
}

fn cfg(dir: PathBuf, bootstrap: Vec<iroh::EndpointId>) -> Config {
    Config { dir, quota: 1 << 30, lantern: false, bootstrap, public_dns: false, private: true, ballast: 0, carry_all: false, release_keys: vec![] }
}

/// The phone's own process: a listener's device, which prints its id and runs until killed.
fn phone(dir: &str, lantern: &str) -> anyhow::Result<()> {
    let n = Node::start(cfg(PathBuf::from(dir), vec![lantern.parse()?]))?;
    n.call(&json!({ "cmd": "lan", "value": true }));
    // On Wi-Fi and charging: it passes on others' traffic and keeps a ballast share.
    n.call(&json!({ "cmd": "ballast_ok", "value": true, "unmetered": true }));
    println!("phone {}", n.id());
    loop {
        std::thread::sleep(Duration::from_secs(60));
    }
}

/// Waits for a line starting `what` in a child's output, and gives the rest of it.
fn line_of(path: &Path, what: &str, secs: u64) -> Option<String> {
    for _ in 0..secs {
        if let Ok(t) = std::fs::read_to_string(path) {
            if let Some(l) = t.lines().find(|l| l.starts_with(what)) {
                return Some(l[what.len()..].trim().to_string());
            }
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    None
}

/// Every file under a folder, read whole, searched for each needle: (needle's label, path).
fn scan_dir(root: &Path, needles: &[(String, Vec<u8>)]) -> (usize, Vec<(String, String)>) {
    let mut hits = Vec::new();
    let mut files = 0;
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            files += 1;
            let rel = p.strip_prefix(root).unwrap_or(&p).display().to_string();
            let b = std::fs::read(&p).unwrap_or_default();
            for (label, n) in needles {
                if find(&b, n) || rel.to_lowercase().contains(&String::from_utf8_lossy(n).to_lowercase()) && n.iter().all(|c| c.is_ascii_graphic() || *c == b' ') {
                    hits.push((label.clone(), rel.clone()));
                }
            }
        }
    }
    (files, hits)
}

fn find(hay: &[u8], n: &[u8]) -> bool {
    !n.is_empty() && hay.windows(n.len()).any(|w| w == n)
}

/// A process's memory, every readable mapping, searched for each needle: the labels found, and
/// how many bytes were read. None when the memory cannot be read at all.
fn scan_mem(pid: u32, needles: &[(String, Vec<u8>)]) -> Option<(Vec<String>, u64)> {
    let maps = std::fs::File::open(format!("/proc/{pid}/maps")).ok()?;
    let mut mem = std::fs::File::open(format!("/proc/{pid}/mem")).ok()?;
    let mut found = std::collections::BTreeSet::new();
    let mut read = 0u64;
    for l in std::io::BufReader::new(maps).lines().map_while(Result::ok) {
        let mut f = l.split_whitespace();
        let (Some(range), Some(perms)) = (f.next(), f.next()) else { continue };
        if !perms.starts_with('r') || l.contains("[vvar]") || l.contains("[vsyscall]") {
            continue;
        }
        let Some((a, b)) = range.split_once('-') else { continue };
        let (Ok(a), Ok(b)) = (u64::from_str_radix(a, 16), u64::from_str_radix(b, 16)) else { continue };
        let mut at = a;
        while at < b {
            let n = (b - at).min(8 << 20) as usize;
            let mut buf = vec![0u8; n];
            if mem.seek(SeekFrom::Start(at)).is_err() || mem.read_exact(&mut buf).is_err() {
                at += n as u64;
                continue;
            }
            read += n as u64;
            for (label, nd) in needles {
                if !found.contains(label) && find(&buf, nd) {
                    found.insert(label.clone());
                }
            }
            at += n as u64;
        }
    }
    (read > 0).then(|| (found.into_iter().collect(), read))
}

/// Whether a user other than root can read a process's memory: as `nobody`, through setpriv.
fn others_read(pid: u32) -> bool {
    Command::new("setpriv")
        .args(["--reuid=65534", "--regid=65534", "--clear-groups", "dd", &format!("if=/proc/{pid}/mem"), "bs=1", "count=1", "skip=4096", "status=none"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

fn limits_say_no_core(pid: u32) -> bool {
    std::fs::read_to_string(format!("/proc/{pid}/limits")).is_ok_and(|t| t.lines().any(|l| l.starts_with("Max core file size") && l.split_whitespace().nth(4) == Some("0")))
}

struct Kill(Child);
impl Drop for Kill {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    if a.get(1).map(String::as_str) == Some("phone") {
        return phone(&a[2], &a[3]);
    }
    if std::env::var_os("PYRATE_QUARTER_MS").is_none() {
        // SAFETY: set before any node starts, while this process has one thread.
        unsafe { std::env::set_var("PYRATE_QUARTER_MS", "20000") };
    }
    let work = PathBuf::from(&a[1]);
    let wav = &a[2];
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work)?;
    let me = std::env::current_exe()?;
    let lantern_bin = me.with_file_name("lantern");
    std::fs::write(work.join("pass"), "the proof test's passphrase\n")?;

    // The Lantern: carries every station, keeps none open, names nothing in its log but its id.
    let llog = work.join("lantern.log");
    let lantern = Kill(
        Command::new(&lantern_bin)
            .args(["--dir", &work.join("lantern").display().to_string(), "--show-ids", "--lan"])
            .env("PYRATE_HOLD_PASS_FILE", work.join("pass"))
            .stdout(std::fs::File::create(&llog)?)
            .stderr(std::fs::File::create(work.join("lantern.err"))?)
            .spawn()?,
    );
    let lid = line_of(&llog, "node id", 60).and_then(|l| l.split_whitespace().last().map(str::to_string)).ok_or_else(|| anyhow::anyhow!("the Lantern did not start"))?;
    let lid_e: iroh::EndpointId = lid.parse()?;
    // The listener's phone, its own process, homed beside the Lantern.
    let plog = work.join("phone.log");
    let phone = Kill(Command::new(&me).args(["phone", &work.join("phone").display().to_string(), &lid]).stdout(std::fs::File::create(&plog)?).stderr(std::fs::File::create(work.join("phone.err"))?).spawn()?);
    let pid_s = line_of(&plog, "phone", 60).ok_or_else(|| anyhow::anyhow!("the phone did not start"))?;
    let phone_id: iroh::EndpointId = pid_s.parse()?;

    // The keeper's phone knows only the listener's phone, its door and so its first hop; the
    // Lantern it hears of by its note, and the stem's second node is the Lantern.
    let k = Node::start(cfg(work.join("keeper"), vec![phone_id]))?;
    k.call(&json!({ "cmd": "lan", "value": true }));
    // The phone has been the keeper's for a week: its first hop.
    k.age_peers(&[*phone_id.as_bytes()], 8 * 86_400_000);
    // Words drawn now, so the phone's copy of this program (its own process) cannot hold them.
    let word = || -> String { (0..10).map(|_| (b'a' + rand::random::<u8>() % 26) as char).collect() };
    let (name, about, title, by) = (format!("Q{}", word()), format!("Kept {} by its link", word()), format!("V{}", word()), format!("A{}", word()));
    let raised = k.call(&json!({ "cmd": "raise", "name": name, "about": about }));
    let id = raised["id"].as_str().unwrap_or_default().to_string();
    let st = StationId::from_hex(&id)?;
    k.call(&json!({ "cmd": "add_track", "station": id, "path": wav, "title": title, "by": by }));
    let mut chunks = Vec::new();
    for _ in 0..90 {
        chunks = k.archive(&st).map(|a| a.entries().iter().filter_map(|e| match e.entry.body() {
            Body::Track { chunks, .. } => Some(chunks.iter().map(|c| c.hash).collect::<Vec<_>>()),
            _ => None,
        }).flatten().collect()).unwrap_or_default();
        if !chunks.is_empty() {
            break;
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    check(!chunks.is_empty(), "the keeper's phone raises a station kept to its link and puts a recording on it");

    // The Lantern ends the stem: it holds the station's sound, sealed, and passes it by name.
    let relay: iroh::RelayUrl = std::env::var("PYRATE_RELAYS").unwrap_or_default().split(',').next().unwrap_or_default().parse()?;
    let probe = |req: Request| -> Option<Response> {
        let rt = tokio::runtime::Runtime::new().ok()?;
        rt.block_on(async {
            let ep = iroh::Endpoint::builder(iroh::endpoint::presets::Minimal).crypto_provider(std::sync::Arc::new(rustls::crypto::aws_lc_rs::default_provider())).relay_mode(iroh::RelayMode::custom([relay.clone()])).bind().await.ok()?;
            let addr = iroh::EndpointAddr { id: lid_e, addrs: [iroh::TransportAddr::Relay(relay.clone())].into_iter().collect() };
            let c = tokio::time::timeout(Duration::from_secs(15), pyrate_node::dial(&ep, addr)).await.ok()?.ok()?;
            let (mut s, mut r) = c.open_bi().await.ok()?;
            s.write_all(&postcard::to_stdvec(&req).ok()?).await.ok()?;
            s.finish().ok()?;
            let b = tokio::time::timeout(Duration::from_secs(10), r.read_to_end(64 << 20)).await.ok()?.ok()?;
            postcard::from_bytes(&b).ok()
        })
    };
    let first = chunks[0];
    let mut carried = false;
    for _ in 0..240 {
        if matches!(probe(Request::SealedChunk { name: pyrate_core::seal::name(&first) }), Some(Response::Sealed(Some(_)))) {
            carried = true;
            break;
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    check(carried, "the Lantern, keeping nothing open, ends the stem and passes the station's sound sealed by name");

    // A second listener plays it by its link, through the phone and the Lantern.
    let q = Node::start(cfg(work.join("listener"), vec![phone_id]))?;
    q.call(&json!({ "cmd": "lan", "value": true }));
    q.age_peers(&[*phone_id.as_bytes()], 8 * 86_400_000);
    std::thread::sleep(Duration::from_secs(5));
    q.call(&json!({ "cmd": "tune", "station": id }));
    let mut played = false;
    for _ in 0..120 {
        if q.call(&json!({ "cmd": "status" }))["now"]["state"] == "playing" {
            played = true;
            break;
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    check(played, "a listener with the link plays the station through the phone and the Lantern");
    std::thread::sleep(Duration::from_secs(20));

    // What is looked for: the station, its words, and its sound as it plays.
    let mut needles: Vec<(String, Vec<u8>)> = vec![
        ("the station's id".into(), st.0.to_vec()),
        ("the station's id in hex".into(), id.clone().into_bytes()),
        ("its call sign".into(), st.call_sign().into_bytes()),
        ("its name".into(), name.clone().into_bytes()),
        ("its blurb".into(), about.clone().into_bytes()),
        ("the recording's title".into(), title.clone().into_bytes()),
        ("the recording's artist".into(), by.clone().into_bytes()),
    ];
    for (i, h) in chunks.iter().take(3).enumerate() {
        if let Some(b) = k.chunk_plain(h) {
            for (j, at) in [b.len() / 5, b.len() / 2, b.len() * 4 / 5].into_iter().enumerate() {
                if at + 48 <= b.len() {
                    needles.push((format!("sound of chunk {i} (part {j})"), b[at..at + 48].to_vec()));
                }
            }
        }
    }
    check(needles.len() > 7, &format!("{} things to look for, sound among them", needles.len()));

    // Memory first, while both run.
    let (lp, pp) = (lantern.0.id(), phone.0.id());
    match scan_mem(pp, &needles) {
        Some((found, read)) => check(found.is_empty(), &format!("the phone's memory holds nothing of the station it passed on ({} MB read{}{})", read >> 20, if found.is_empty() { "" } else { "; found " }, found.join(", "))),
        None => check(false, "the phone's memory could be read by this test"),
    }
    match scan_mem(lp, &needles) {
        Some((found, read)) => println!("NOTE the Lantern's memory, {} MB read, holds what it serves: {}", read >> 20, if found.is_empty() { "nothing of the station".to_string() } else { found.join(", ") }),
        None => println!("NOTE the Lantern's memory could not be read"),
    }
    for (who, p) in [("the Lantern", lp), ("the phone", pp)] {
        check(!others_read(p), &format!("no other program of another user may read {who}'s memory"));
        check(limits_say_no_core(p), &format!("{who} writes no core file when it crashes"));
    }
    // A crash, made: the Lantern is killed as a fault would kill it, and leaves no core anywhere.
    let before: Vec<PathBuf> = std::fs::read_dir(&work).map(|r| r.flatten().map(|e| e.path()).collect()).unwrap_or_default();
    // SAFETY: a signal to a child this test started.
    unsafe { libc_kill(lp as i32, 11) };
    std::thread::sleep(Duration::from_secs(3));
    drop(lantern);
    drop(phone);
    let cores = std::fs::read_dir(&work).map(|r| r.flatten().map(|e| e.path()).filter(|p| !before.contains(p) && p.file_name().is_some_and(|n| n.to_string_lossy().starts_with("core"))).count()).unwrap_or(0)
        + std::fs::read_dir(work.join("lantern")).map(|r| r.flatten().filter(|e| e.file_name().to_string_lossy().starts_with("core")).count()).unwrap_or(0);
    check(cores == 0, "the Lantern, crashed, leaves no core file");

    // Then the folders, whole.
    for (who, d) in [("the Lantern's", work.join("lantern")), ("the phone's", work.join("phone"))] {
        let (files, hits) = scan_dir(&d, &needles);
        let said: Vec<String> = hits.iter().map(|(l, p)| format!("{l} in {p}")).collect();
        check(files > 0 && hits.is_empty(), &format!("{who} data folder holds nothing of the station ({files} files{}{})", if hits.is_empty() { "" } else { "; " }, said.join("; ")));
    }
    let (_, logs) = scan_dir(&work, &needles.iter().filter(|(l, _)| !l.starts_with("sound")).cloned().collect::<Vec<_>>());
    let in_logs: Vec<String> = logs.iter().filter(|(_, p)| p.ends_with(".log") || p.ends_with(".err")).map(|(l, p)| format!("{l} in {p}")).collect();
    check(in_logs.is_empty(), &format!("nor do their logs name it{}{}", if in_logs.is_empty() { "" } else { ": " }, in_logs.join("; ")));
    drop(q);
    drop(k);
    Ok(())
}

unsafe extern "C" {
    #[link_name = "kill"]
    fn libc_kill(pid: i32, sig: i32) -> i32;
}
