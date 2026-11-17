//! find_test <relay Lantern id> <Gangway's station id> [<work dir>]
//! No box in charge (0.9.20, phase B). Phones with no bootstrap list at all find the test
//! network: one through the DHT (a stand-in DHT of its own on loopback, where a real Lantern
//! leaves its note), one through a seed list fetched from a mirror. A seed list with one release
//! signature is refused. The phone that looked up on the DHT left no note there and no DHT node
//! took its address into its table.
//! Run on the test network (net_up.sh) under its lock, with PYRATE_RELAYS set; it starts a
//! Lantern of its own from target/release/lantern beside this binary.
use std::{
    io::{BufRead, BufReader, Read, Write},
    path::PathBuf,
    process::{Command, Stdio},
    sync::{Arc, Mutex, atomic::AtomicUsize},
    time::{Duration, Instant},
};

use pyrate_core::StationKey;
use pyrate_node::{Config, Node, find};
use serde_json::json;

fn check(c: bool, what: &str) -> bool {
    println!("{} {what}", if c { "PASS" } else { "FAIL" });
    c
}

/// Whether a node's dial lists the station, waiting at most `secs`.
fn hears(n: &Node, station: &str, secs: u64) -> bool {
    let t = Instant::now();
    while t.elapsed() < Duration::from_secs(secs) {
        let s = n.call(&json!({ "cmd": "stations" }));
        if s.as_array().into_iter().flatten().any(|x| x["id"] == station) {
            return true;
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    false
}

fn phone(dir: &str) -> Node {
    let keys: Vec<[u8; 32]> = std::env::var("FIND_TEST_KEYS").unwrap_or_default().split(',').filter_map(|h| pyrate_core::StationId::from_hex(h).ok().map(|k| k.0)).collect();
    let _ = std::fs::remove_dir_all(dir);
    Node::start(Config { dir: dir.into(), quota: 1 << 30, lantern: false, carry_all: false, bootstrap: vec![], public_dns: false, private: true, ballast: 0, release_keys: keys }).expect("a phone starts")
}

/// A child process: one phone, told what to do by its first argument.
fn child(role: &str, station: &str, dir: &str) {
    let n = phone(dir);
    println!("phone {}", n.id());
    std::io::stdout().flush().ok();
    match role {
        "dht" => {
            check(hears(&n, station, 120), "a phone with no bootstrap list finds the test network through the DHT");
        }
        "seeds" => {
            check(!hears(&n, station, 25), "a phone handed a seed list with one release signature stays alone");
            println!("stage two");
            std::io::stdout().flush().ok();
            check(hears(&n, station, 150), "a phone with no bootstrap list finds the test network through a seed list signed by two keys");
        }
        _ => {}
    }
    std::io::stdout().flush().ok();
}

/// A mirror on loopback: serves whatever list is in `body` at any path, and counts the asks.
fn mirror(body: Arc<Mutex<Vec<u8>>>, hits: Arc<AtomicUsize>) -> u16 {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for s in l.incoming().flatten() {
            let mut s = s;
            let mut buf = [0u8; 2048];
            let _ = s.set_read_timeout(Some(Duration::from_secs(5)));
            let _ = s.read(&mut buf);
            hits.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let b = body.lock().unwrap().clone();
            let _ = s.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", b.len()).as_bytes());
            let _ = s.write_all(&b);
        }
    });
    port
}

/// The verdicts the phones printed.
static PASSES: AtomicUsize = AtomicUsize::new(0);
static FAILS: AtomicUsize = AtomicUsize::new(0);

/// Runs this binary again as a phone, with the environment given. Lines it prints come back
/// through the channel; PASS and FAIL lines are printed as they come.
fn spawn(role: &str, station: &str, dir: &str, env: &[(&str, String)]) -> (std::process::Child, std::sync::mpsc::Receiver<String>) {
    let mut c = Command::new(std::env::current_exe().unwrap());
    c.args(["--child", role, station, dir]).stdout(Stdio::piped()).stderr(Stdio::null()).env_remove("PYRATE_LAN_ANY").env_remove("PYRATE_DHT_BOOTSTRAP").env_remove("PYRATE_SEED_MIRRORS");
    for (k, v) in env {
        c.env(k, v);
    }
    let mut child = c.spawn().expect("the phone starts");
    let out = child.stdout.take().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for l in BufReader::new(out).lines().map_while(Result::ok) {
            if l.starts_with("PASS") {
                PASSES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                println!("{l}");
            } else if l.starts_with("FAIL") {
                FAILS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                println!("{l}");
            }
            let _ = tx.send(l);
        }
    });
    (child, rx)
}

fn wait_line(rx: &std::sync::mpsc::Receiver<String>, starts: &str, secs: u64) -> Option<String> {
    let t = Instant::now();
    while t.elapsed() < Duration::from_secs(secs) {
        match rx.recv_timeout(Duration::from_secs(1)) {
            Ok(l) if l.starts_with(starts) => return Some(l),
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return None,
            _ => {}
        }
    }
    None
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.get(1).map(String::as_str) == Some("--child") {
        child(&a[2], &a[3], &a[4]);
        return;
    }
    let (Some(relay), Some(station)) = (a.get(1), a.get(2)) else {
        eprintln!("find_test <relay Lantern id> <Gangway's station id> [<work dir>]");
        std::process::exit(2);
    };
    let work = PathBuf::from(a.get(3).cloned().unwrap_or_else(|| "/tmp/find-test".into()));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).unwrap();
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut ok = true;
    // Notes naming the test network's loopback relay are believed here, as on its nodes.
    pyrate_node::net::LOCAL_RELAYS_OK.store(true, std::sync::atomic::Ordering::Relaxed);

    // ---- The DHT, a stand-in of six nodes on loopback, grown from its first node ----
    let testnet = mainline::Testnet::builder(6).bind_address(std::net::Ipv4Addr::LOCALHOST).seeded(false).build().expect("a stand-in DHT");
    let boot = testnet.bootstrap.join(",");
    #[allow(deprecated)]
    let dht_addrs: Vec<String> = testnet.nodes.iter().map(|n| n.info().local_addr().to_string()).collect();
    let lookup = |rt: &tokio::runtime::Runtime| rt.block_on(find::dht_lookup(&Some(testnet.bootstrap.clone()))).unwrap_or_default();

    // A real Lantern, joined to the test network through its relay Lantern, leaves its note.
    let lantern_bin = std::env::current_exe().unwrap().with_file_name("lantern");
    std::fs::write(work.join("pass"), "the find test passphrase\n").unwrap();
    let log = std::fs::File::create(work.join("lantern.log")).unwrap();
    let mut lantern = Command::new(&lantern_bin)
        .args(["--dir", work.join("lantern").to_str().unwrap(), "--show-ids", "--bootstrap", relay])
        .env("PYRATE_HOLD_PASS_FILE", work.join("pass"))
        .env("PYRATE_DHT_BOOTSTRAP", &boot)
        .env("PYRATE_DHT_EVERY_S", "5")
        .env_remove("PYRATE_LAN_ANY")
        .stdout(log.try_clone().unwrap())
        .stderr(log)
        .spawn()
        .expect("the Lantern starts");
    let mut lantern_id = None;
    let t = Instant::now();
    while t.elapsed() < Duration::from_secs(60) && lantern_id.is_none() {
        std::thread::sleep(Duration::from_secs(1));
        let text = std::fs::read_to_string(work.join("lantern.log")).unwrap_or_default();
        lantern_id = text.lines().find_map(|l| l.strip_prefix("node id: ").map(|s| s.trim().to_string()));
    }
    let lantern_id = lantern_id.unwrap_or_default();
    let mut noted = false;
    let t = Instant::now();
    while t.elapsed() < Duration::from_secs(120) && !noted {
        noted = lookup(&rt).iter().any(|(id, _)| id.to_string() == lantern_id);
        if !noted {
            std::thread::sleep(Duration::from_secs(3));
        }
    }
    ok &= check(!lantern_id.is_empty() && noted, "a Lantern leaves a note in the DHT naming itself and its relay");

    // A phone with nothing at all: no doors, no peers, no keys for a seed list.
    let (mut p, rx) = spawn("dht", station, work.join("phone-dht").to_str().unwrap(), &[("PYRATE_DHT_BOOTSTRAP", boot.clone()), ("PYRATE_FIND_WAIT_S", "3".into()), ("PYRATE_SEED_MIRRORS", "http://127.0.0.1:9/none".into())]);
    let phone_id = wait_line(&rx, "phone ", 60).map(|l| l[6..].trim().to_string()).unwrap_or_default();
    let t = Instant::now();
    while t.elapsed() < Duration::from_secs(150) && p.try_wait().ok().flatten().is_none() {
        std::thread::sleep(Duration::from_secs(1));
    }
    let _ = p.kill();
    let _ = p.wait();
    ok &= PASSES.load(std::sync::atomic::Ordering::Relaxed) == 1;
    // What the DHT holds now names the Lantern, and never the phone.
    let notes = lookup(&rt);
    ok &= check(!phone_id.is_empty() && !notes.iter().any(|(id, _)| id.to_string() == phone_id), "the phone left no note in the DHT");
    ok &= check(notes.iter().all(|(_, r)| net_relay(r)), "a note in the DHT names a Lantern and a relay and nothing more");
    // The stand-in's first node has no bootstrap list of its own, and the crate lets such a node
    // take in anyone who asks it for nodes, read-only or not, so the network can grow from it.
    // Every other node holds to BEP 43: a read-only client's address is never taken in.
    #[allow(deprecated)]
    let tables: Vec<String> = testnet.nodes.iter().skip(1).flat_map(|n| n.to_bootstrap()).collect();
    let strangers: Vec<&String> = tables.iter().filter(|x| !dht_addrs.contains(x)).collect();
    ok &= check(strangers.is_empty() && !tables.is_empty(), &format!("no DHT node that keeps to read-only clients took the phone or the Lantern into its table ({} strangers)", strangers.len()));

    // ---- A seed list ----
    let keys: Vec<StationKey> = (0..3).map(|_| StationKey::generate().unwrap()).collect();
    let hexes: Vec<String> = keys.iter().map(|k| k.id().to_hex()).collect();
    let relay_url = std::env::var("PYRATE_RELAYS").unwrap_or_default().split(',').next().unwrap_or("").trim().trim_end_matches('/').to_string() + "/";
    let mut doc = json!({ "pyrate": "seeds", "at": pyrate_node::now_ms(), "seeds": [format!("{relay}@{relay_url}")], "mirrors": [] });
    pyrate_core::papers::sign(&keys[1], 1, &mut doc).unwrap();
    let body = Arc::new(Mutex::new(doc.to_string().into_bytes()));
    let hits = Arc::new(AtomicUsize::new(0));
    let port = mirror(body.clone(), hits.clone());
    let (mut p, rx) = spawn(
        "seeds",
        station,
        work.join("phone-seeds").to_str().unwrap(),
        &[("PYRATE_DHT", "0".into()), ("PYRATE_FIND_WAIT_S", "3".into()), ("PYRATE_SEED_MIRRORS", format!("http://127.0.0.1:{port}/seeds.json")), ("FIND_TEST_KEYS", hexes.join(","))],
    );
    let stage = wait_line(&rx, "stage two", 90);
    ok &= check(stage.is_some() && hits.load(std::sync::atomic::Ordering::Relaxed) > 0, "the phone fetched the list from its mirror before refusing it");
    pyrate_core::papers::sign(&keys[2], 2, &mut doc).unwrap();
    *body.lock().unwrap() = doc.to_string().into_bytes();
    // The phone prints its second verdict and stops; every verdict is counted at the end.
    let t = Instant::now();
    while t.elapsed() < Duration::from_secs(200) && p.try_wait().ok().flatten().is_none() {
        std::thread::sleep(Duration::from_secs(1));
    }
    let _ = p.kill();
    let _ = p.wait();

    let _ = lantern.kill();
    let _ = lantern.wait();
    drop(testnet);
    // Every phone verdict: one for the DHT, two for the seed list, and no failure.
    ok &= FAILS.load(std::sync::atomic::Ordering::Relaxed) == 0 && PASSES.load(std::sync::atomic::Ordering::Relaxed) == 3;
    let _ = std::fs::remove_dir_all(&work);
    println!("{}", if ok { "find_test: all passed" } else { "find_test: FAILED" });
    std::process::exit(if ok { 0 } else { 1 });
}

fn net_relay(r: &str) -> bool {
    r.starts_with("http://") || r.starts_with("https://")
}
