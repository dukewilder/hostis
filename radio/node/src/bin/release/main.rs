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

use std::path::PathBuf;

mod apksign;

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
            // Bearings take two of the three keys, a canary any one: run once per key.
            let words = std::fs::read_to_string(a.get(2).ok_or_else(|| anyhow::anyhow!("words file"))?)?;
            let n: u8 = a.get(3).and_then(|s| s.parse().ok()).filter(|n| (1..=3).contains(n)).ok_or_else(|| anyhow::anyhow!("key number 1-3"))?;
            let path = a.get(4).ok_or_else(|| anyhow::anyhow!("bearings.json or canary.json"))?;
            let mut doc: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path)?)?;
            let key = StationKey::from_phrase(&words)?;
            pyrate_core::papers::sign(&key, n - 1, &mut doc).ok_or_else(|| anyhow::anyhow!("not a paper: it needs \"pyrate\": \"bearings\" or \"canary\""))?;
            std::fs::write(path, serde_json::to_string_pretty(&doc)? + "\n")?;
            println!("Signed {path} with key {n}: {} signature(s) on it.", doc["sigs"].as_array().map(|s| s.len()).unwrap_or(0));
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
        _ => eprintln!("release init <folder> | sign <words> <1-3> <apk> <code> | genesis <words 1> <folder> | android-init <folder> | sign-apk <key> <cert> <in> <out> | canary [file] [text] | sign-paper <words> <1-3> <bearings.json|canary.json>"),
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
