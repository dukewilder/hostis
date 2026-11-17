//! The release keys, kept offline. Run on the keyholder's own machine, never on a server.
//!
//! release init <folder>
//!     Makes the three release keys. Writes each as 24 words to its own file, the public keys to
//!     release_keys.txt (these go into the app), and a lantern-genesis folder holding the
//!     Releases station with a deputy key for the Lantern. The deputy can publish; only
//!     releases signed by two of the three keys are ever installed.
//!
//! release sign <words file> <key number 1-3> <apk> <version code>
//!     Prints one signature line for the .sigs file beside the APK.
//!
//! release genesis <words file of key 1> <folder>
//!     Makes the Lantern's genesis folder again from release key 1, with a fresh deputy.
//!
//! release android-init <folder>
//!     Makes the Android signing key as 24 words (android-key.txt) and its certificate
//!     (android-cert.der, public). Every Pyrate Radio APK is signed with this one key, so
//!     Android accepts each release as an update of the last.
//!
//! release sign-apk <android-key.txt> <android-cert.der> <unsigned.apk> <signed.apk>
//!     Signs an APK, offline.
//!
//! release sign-all <words file, or - to type them> <key number 1-3> <release folder or one file>
//!     Signs everything a release carries in one go: the APK (named <code>-<version>.apk),
//!     player.manifest, players.txt, the computers' builds and the bearings. Each signature goes
//!     into a file beside its artifact, kept with any other holder's, so two holders sign the
//!     same folder in turn.
//!
//! release seeds <seeds.json> <id@relay>... [--mirror <https address>]...
//!     Writes a seed list, unsigned: volunteers' Lanterns a new node may start from when every
//!     door it was handed is gone, and the mirrors the next list may be fetched from. Each of two
//!     holders then signs it with sign-paper (or sign-all on the folder); a list with one
//!     signature is refused by every node. Publish it at the mirrors and in releases/.
//!
//! release check <release folder>
//!     Says in plain words what in the folder is signed, and by how many keys.

use std::path::PathBuf;

mod apksign;
mod sigs;

use pyrate_core::{Archive, Body, StationKey, release_message};
use pyrate_node::store::Store;
use sha2::{Digest, Sha256};

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    match a.get(1).map(String::as_str) {
        Some("init") => {
            let out = PathBuf::from(a.get(2).cloned().unwrap_or_else(|| "release-keys".into()));
            std::fs::create_dir_all(&out)?;
            let keys: Vec<StationKey> = (0..3).map(|_| StationKey::generate()).collect::<Result<_, _>>()?;
            for (i, k) in keys.iter().enumerate() {
                std::fs::write(out.join(format!("release-key-{}.txt", i + 1)), format!("{}\n", k.phrase()))?;
            }
            std::fs::write(out.join("release_keys.txt"), keys.iter().map(|k| k.id().to_hex()).collect::<Vec<_>>().join("\n") + "\n")?;
            genesis(&keys[0], &out.join("lantern-genesis"))?;
            println!("Three release keys made in {}.", out.display());
            println!("Move each release-key-N.txt to its own place offline, then delete them here.");
            println!("release_keys.txt goes into the app. lantern-genesis goes into the Lantern's data folder.");
        }
        Some("sign") => {
            let words = std::fs::read_to_string(a.get(2).ok_or_else(|| anyhow::anyhow!("words file"))?)?;
            let n: u8 = a.get(3).and_then(|s| s.parse().ok()).filter(|n| (1..=3).contains(n)).ok_or_else(|| anyhow::anyhow!("key number 1-3"))?;
            let apk = std::fs::read(a.get(4).ok_or_else(|| anyhow::anyhow!("apk"))?)?;
            let code: u32 = a.get(5).and_then(|s| s.parse().ok()).ok_or_else(|| anyhow::anyhow!("version code"))?;
            let key = StationKey::from_phrase(&words)?;
            let sha: [u8; 32] = Sha256::digest(&apk).into();
            println!("{} {}", n - 1, hex(&key.sign(&release_message(code, &sha))));
        }
        Some("genesis") => {
            let words = std::fs::read_to_string(a.get(2).ok_or_else(|| anyhow::anyhow!("words file of key 1"))?)?;
            let k = StationKey::from_phrase(&words)?;
            let out = PathBuf::from(a.get(3).cloned().unwrap_or_else(|| "lantern-genesis".into()));
            genesis(&k, &out)?;
            println!("Genesis made in {} for the Releases station {}.", out.display(), k.id().call_sign());
        }
        Some("android-init") => {
            let out = PathBuf::from(a.get(2).cloned().unwrap_or_else(|| "release-keys".into()));
            std::fs::create_dir_all(&out)?;
            let words = StationKey::generate()?;
            let key = apksign::android_key(&words.seed());
            std::fs::write(out.join("android-key.txt"), format!("{}\n", words.phrase()))?;
            std::fs::write(out.join("android-cert.der"), apksign::certificate(&key)?)?;
            println!("Android signing key made in {}.", out.display());
            println!("Write android-key.txt on paper, keep it apart from the release keys, then delete the file.");
            println!("android-cert.der is public: keep it with the release folder and send a copy to Claude.");
        }
        Some("sign-paper") => {
            // Bearings and seed lists take two of the three keys, a canary any one: run once per key.
            let words = std::fs::read_to_string(a.get(2).ok_or_else(|| anyhow::anyhow!("words file"))?)?;
            let n: u8 = a.get(3).and_then(|s| s.parse().ok()).filter(|n| (1..=3).contains(n)).ok_or_else(|| anyhow::anyhow!("key number 1-3"))?;
            let path = a.get(4).ok_or_else(|| anyhow::anyhow!("bearings.json, seeds.json or canary.json"))?;
            let mut doc: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path)?)?;
            let key = StationKey::from_phrase(&words)?;
            pyrate_core::papers::sign(&key, n - 1, &mut doc).ok_or_else(|| anyhow::anyhow!("not a paper: it needs \"pyrate\": \"bearings\", \"seeds\" or \"canary\""))?;
            std::fs::write(path, serde_json::to_string_pretty(&doc)? + "\n")?;
            println!("Signed {path} with key {n}: {} signature(s) on it.", doc["sigs"].as_array().map(|s| s.len()).unwrap_or(0));
        }
        Some("seeds") => {
            let path = a.get(2).ok_or_else(|| anyhow::anyhow!("seeds.json"))?;
            let (mut seeds, mut mirrors) = (Vec::new(), Vec::new());
            let mut rest = a.iter().skip(3);
            while let Some(x) = rest.next() {
                if x == "--mirror" {
                    let m = rest.next().ok_or_else(|| anyhow::anyhow!("--mirror needs an address"))?;
                    if !m.starts_with("https://") {
                        anyhow::bail!("a mirror is an https address: {m}");
                    }
                    mirrors.push(m.clone());
                } else {
                    let (id, relay) = x.split_once('@').map(|(i, r)| (i, Some(r))).unwrap_or((x.as_str(), None));
                    if pyrate_core::StationId::from_hex(id).is_err() || relay.is_some_and(|r| !r.starts_with("https://")) {
                        anyhow::bail!("not a seed: {x} (a Lantern's id, then @ and its relay's https address)");
                    }
                    seeds.push(x.clone());
                }
            }
            if seeds.is_empty() {
                anyhow::bail!("a seed list needs at least one Lantern");
            }
            let doc = serde_json::json!({ "pyrate": "seeds", "at": pyrate_node::now_ms(), "seeds": seeds, "mirrors": mirrors });
            std::fs::write(path, serde_json::to_string_pretty(&doc)? + "\n")?;
            let n = doc["seeds"].as_array().map(|s| s.len()).unwrap_or(0);
            println!("Wrote {path} with {n} {}. Two holders sign it: release sign-paper <words> <1-3> {path}", if n == 1 { "seed" } else { "seeds" });
        }
        Some("canary") => {
            // A fresh canary for this month, unsigned: sign it with sign-paper.
            let path = a.get(2).cloned().unwrap_or_else(|| "canary.json".into());
            let text = a.get(3).cloned().unwrap_or_else(|| "HOSTIS has received no order to hide, change or stop anything, and no order to keep quiet about one.".into());
            let doc = serde_json::json!({ "pyrate": "canary", "at": pyrate_node::now_ms(), "text": text });
            std::fs::write(&path, serde_json::to_string_pretty(&doc)? + "\n")?;
            println!("Wrote {path}. Sign it: release sign-paper <words> <1-3> {path}");
        }
        Some("sign-apk") => {
            // "-" reads the words from standard input, so a signing window can pass them
            // without writing them to disk.
            let src = a.get(2).ok_or_else(|| anyhow::anyhow!("android-key.txt, or - to read the words from input"))?;
            let words = if src == "-" {
                let mut w = String::new();
                std::io::Read::read_to_string(&mut std::io::stdin(), &mut w)?;
                w
            } else {
                std::fs::read_to_string(src)?
            };
            let cert = std::fs::read(a.get(3).ok_or_else(|| anyhow::anyhow!("android-cert.der"))?)?;
            let apk = std::fs::read(a.get(4).ok_or_else(|| anyhow::anyhow!("unsigned apk"))?)?;
            let out = a.get(5).ok_or_else(|| anyhow::anyhow!("signed apk"))?;
            let key = apksign::android_key(&StationKey::from_phrase(&words)?.seed());
            // Words that make a different key would sign an app no phone takes as an update.
            let public = key.verifying_key().to_sec1_point(false);
            if !cert.windows(public.as_bytes().len()).any(|w| w == public.as_bytes()) {
                anyhow::bail!("these words do not match android-cert.der: check them against your paper");
            }
            std::fs::write(out, apksign::sign(&apk, &key, &cert)?)?;
            println!("Signed: {out}");
        }
        Some("sign-all") => {
            let src = a.get(2).ok_or_else(|| anyhow::anyhow!("words file, or - to type the words"))?;
            let n: u8 = a.get(3).and_then(|s| s.parse().ok()).filter(|n| (1..=3).contains(n)).ok_or_else(|| anyhow::anyhow!("key number 1-3"))?;
            let target = PathBuf::from(a.get(4).cloned().unwrap_or_else(|| ".".into()));
            let words = if src == "-" {
                println!("Type the 24 words of release key {n}, with a space between each, then press Enter.");
                let mut w = String::new();
                std::io::stdin().read_line(&mut w)?;
                w
            } else {
                std::fs::read_to_string(src)?
            };
            let key = StationKey::from_phrase(&words).map_err(|_| anyhow::anyhow!("those are not 24 good words: check them against your paper and try again"))?;
            let keys = sigs::keys();
            if keys.get(n as usize - 1).is_some_and(|k| *k != key.id().0) {
                anyhow::bail!("those words are not release key {n}: check the number and the words against your paper");
            }
            let (folder, all) = if target.is_file() {
                let folder = target.parent().map(|p| p.to_path_buf()).unwrap_or_default();
                let all: Vec<_> = sigs::artifacts(&folder).into_iter().filter(|x| x.path.file_name() == target.file_name()).collect();
                (folder, all)
            } else {
                (target.clone(), sigs::artifacts(&target))
            };
            if all.is_empty() {
                anyhow::bail!("there is nothing to sign in {}", target.display());
            }
            for x in &all {
                match sigs::sign(&key, n - 1, x) {
                    Ok(()) => println!("Signed {}.", x.name()),
                    Err(why) => println!("Could not sign {}: it {why}.", x.name()),
                }
            }
            println!();
            for l in sigs::report(&keys, &folder).0 {
                println!("{l}");
            }
        }
        Some("check") => {
            let folder = PathBuf::from(a.get(2).cloned().unwrap_or_else(|| ".".into()));
            let (lines, ready) = sigs::report(&sigs::keys(), &folder);
            for l in lines {
                println!("{l}");
            }
            if !ready {
                std::process::exit(1);
            }
        }
        _ => eprintln!("release init <folder> | sign <words> <1-3> <apk> <code> | genesis <words 1> <folder> | android-init <folder> | sign-apk <key> <cert> <in> <out> | canary [file] [text] | seeds <seeds.json> <id@relay>... [--mirror <url>] | sign-paper <words> <1-3> <bearings.json|seeds.json|canary.json> | sign-all <words or -> <1-3> <folder> | check <folder>"),
    }
    Ok(())
}

/// The Releases station's first entries, and a deputy key for the Lantern that publishes.
fn genesis(k: &StationKey, out: &std::path::Path) -> anyhow::Result<()> {
    let mut store = Store::open(out, 1 << 30)?;
    let deputy = StationKey::generate()?;
    let mut log = Archive::new(k.id());
    let now = pyrate_node::now_ms();
    log.write(k, now, Body::Colours { name: "Pyrate Radio Releases".into(), about: "New versions of the app, signed by two of three release keys.".into(), port: None, tip: None })?;
    log.write(k, now + 1, Body::Deputy { key: deputy.id().0, label: "lantern".into() })?;
    store.put_entries(k.id(), log.entries().to_vec())?;
    store.add_key_for(k.id(), deputy)?;
    Ok(())
}
