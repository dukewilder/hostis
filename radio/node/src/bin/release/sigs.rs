//! Everything a release carries, signed by the release keys in one pass over its folder.
//!
//! Each kind of file has a message of its own, so a signature over one kind never passes as
//! another: the APK and the player's file list keep `pyrate/release/v1` (what phones and the
//! installed player already check), players.txt signs as `pyrate/players/v1` and a computer's
//! build as `pyrate/desktop/v1`. Signatures go one per line, `<key index 0-2> <hex>`, into a
//! file beside each artifact; bearings and canaries carry theirs inside.

use std::path::{Path, PathBuf};

use pyrate_core::{StationId, StationKey, release_message};
use sha2::{Digest, Sha256};

/// The release keys the app ships with; PYRATE_RELEASE_KEYS (comma-separated hex) stands in
/// for a trial run with throwaway keys.
pub fn keys() -> Vec<[u8; 32]> {
    let text = std::env::var("PYRATE_RELEASE_KEYS").map(|s| s.replace(',', "\n")).unwrap_or_else(|_| include_str!("../../../../app/assets/release_keys.txt").to_string());
    text.split_whitespace().filter_map(|h| StationId::from_hex(h).ok()).map(|k| k.0).collect()
}

fn sha(b: &[u8]) -> [u8; 32] {
    Sha256::digest(b).into()
}

fn domain(d: &[u8], code: u32, sha: &[u8; 32]) -> Vec<u8> {
    let mut m = d.to_vec();
    m.extend_from_slice(&code.to_le_bytes());
    m.extend_from_slice(sha);
    m
}

/// players.txt: how many players it lists, so a shorter, older list signed earlier is told
/// apart, and the hash of its bytes.
pub fn players_message(list: &[u8]) -> Vec<u8> {
    let n = String::from_utf8_lossy(list).split_whitespace().filter(|c| c.starts_with("bafy")).count() as u32;
    domain(b"pyrate/players/v1\0", n, &sha(list))
}

/// A computer's build (zip or tar.gz): the hash of its bytes.
pub fn desktop_message(file: &[u8]) -> Vec<u8> {
    domain(b"pyrate/desktop/v1\0", 0, &sha(file))
}

/// player.manifest: the code on its first line and the hash of the whole file, as sw.js checks.
pub fn manifest_message(manifest: &[u8]) -> Option<Vec<u8>> {
    let first = String::from_utf8_lossy(manifest).lines().next()?.to_string();
    let code: u32 = first.strip_prefix("pyrate-player/v1 code ")?.trim().parse().ok()?;
    Some(release_message(code, &sha(manifest)))
}

/// An APK named `<version code>-<version>.apk`, as the Lantern's releases folder takes it.
pub fn apk_code(path: &Path) -> Option<u32> {
    path.file_stem()?.to_str()?.split_once('-')?.0.parse().ok()
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    Apk,
    Manifest,
    Players,
    Desktop,
    Paper,
}

pub struct Artifact {
    pub path: PathBuf,
    pub kind: Kind,
}

impl Artifact {
    pub fn name(&self) -> String {
        self.path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
    }

    /// Where its signatures go (a paper keeps its own).
    pub fn sigs(&self) -> PathBuf {
        match self.kind {
            Kind::Apk => self.path.with_extension("sigs"),
            Kind::Manifest => self.path.with_file_name("player.sigs"),
            Kind::Players => self.path.with_file_name("players.sigs"),
            Kind::Desktop => PathBuf::from(format!("{}.sigs", self.path.display())),
            Kind::Paper => self.path.clone(),
        }
    }

    /// What each key signs, or why it cannot be signed.
    pub fn message(&self) -> Result<Vec<u8>, String> {
        let b = std::fs::read(&self.path).map_err(|e| format!("cannot be read ({e})"))?;
        match self.kind {
            Kind::Apk => apk_code(&self.path).map(|c| release_message(c, &sha(&b))).ok_or_else(|| "has no version code in its name: it should be named like 31-0.9.18.apk".into()),
            Kind::Manifest => manifest_message(&b).ok_or_else(|| "does not start with its version code".into()),
            Kind::Players => Ok(players_message(&b)),
            Kind::Desktop => Ok(desktop_message(&b)),
            Kind::Paper => Err("is a paper".into()),
        }
    }

    /// How many keys a release needs on it.
    pub fn needs(&self) -> usize {
        if self.kind == Kind::Paper && paper(&self.path).is_some_and(|d| d["pyrate"] == "canary") { 1 } else { 2 }
    }
}

fn paper(p: &Path) -> Option<serde_json::Value> {
    let d: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(p).ok()?).ok()?;
    matches!(d["pyrate"].as_str(), Some("bearings" | "canary")).then_some(d)
}

/// Every file in the folder a release key signs, by name.
pub fn artifacts(folder: &Path) -> Vec<Artifact> {
    let mut out: Vec<Artifact> = std::fs::read_dir(folder).into_iter().flatten().flatten().filter_map(|f| {
        let path = f.path();
        if !path.is_file() {
            return None;
        }
        let name = path.file_name()?.to_str()?.to_string();
        let kind = if name.ends_with(".apk") {
            Kind::Apk
        } else if name == "player.manifest" {
            Kind::Manifest
        } else if name == "players.txt" {
            Kind::Players
        } else if (name.ends_with(".zip") || name.ends_with(".tar.gz")) && !name.starts_with("pyrate-radio-src") {
            Kind::Desktop
        } else if name.ends_with(".json") && paper(&path).is_some() {
            Kind::Paper
        } else {
            return None;
        };
        Some(Artifact { path, kind })
    }).collect();
    out.sort_by_key(|a| a.name());
    out
}

fn parse(text: &str) -> Vec<(u8, [u8; 64])> {
    text.lines().filter_map(|l| {
        let (i, h) = l.trim().split_once(' ')?;
        let mut sig = [0u8; 64];
        pyrate_core::unhex_into(h.trim(), &mut sig)?;
        Some((i.parse().ok()?, sig))
    }).collect()
}

/// How many different release keys signed `msg` in this signatures file, and how many lines in
/// it no longer match (signatures over an older copy of the file).
pub fn good(keys: &[[u8; 32]], msg: &[u8], text: &str) -> (usize, usize) {
    let mut ok = std::collections::HashSet::new();
    let mut stale = 0;
    for (i, sig) in parse(text) {
        match keys.get(i as usize) {
            Some(k) if StationId(*k).verify(msg, &sig).is_ok() => {
                ok.insert(i);
            }
            _ => stale += 1,
        }
    }
    (ok.len(), stale)
}

/// Signs one artifact as key index `n` (0-2): its line replaces any earlier one by the same key,
/// and every other key's line stays.
pub fn sign(key: &StationKey, n: u8, a: &Artifact) -> Result<(), String> {
    if a.kind == Kind::Paper {
        let mut d = paper(&a.path).ok_or("cannot be read")?;
        pyrate_core::papers::sign(key, n, &mut d).ok_or("is not a paper")?;
        return std::fs::write(&a.path, serde_json::to_string_pretty(&d).unwrap_or_default() + "\n").map_err(|e| e.to_string());
    }
    let msg = a.message()?;
    let sig: String = key.sign(&msg).iter().map(|b| format!("{b:02x}")).collect();
    let old = std::fs::read_to_string(a.sigs()).unwrap_or_default();
    let mut lines: Vec<String> = old.lines().filter(|l| !l.trim().is_empty() && l.trim().split_once(' ').map(|(i, _)| i) != Some(&n.to_string())).map(str::to_string).collect();
    lines.push(format!("{n} {sig}"));
    std::fs::write(a.sigs(), lines.join("\n") + "\n").map_err(|e| e.to_string())
}

/// How many keys stand behind one artifact, and how many old signatures no longer match.
pub fn count(keys: &[[u8; 32]], a: &Artifact) -> Result<(usize, usize), String> {
    if a.kind == Kind::Paper {
        let d = paper(&a.path).ok_or("cannot be read")?;
        let kind = d["pyrate"].as_str().unwrap_or_default().to_string();
        return Ok((pyrate_core::papers::signers(keys, &kind, &d), 0));
    }
    let msg = a.message()?;
    Ok(good(keys, &msg, &std::fs::read_to_string(a.sigs()).unwrap_or_default()))
}

/// The check, in plain words, one line a file and a last line on the whole folder.
pub fn report(keys: &[[u8; 32]], folder: &Path) -> (Vec<String>, bool) {
    let all = artifacts(folder);
    let mut lines = Vec::new();
    let mut ready = !all.is_empty();
    for a in &all {
        let need = a.needs();
        let line = match count(keys, a) {
            Err(why) => {
                ready = false;
                format!("{} {why}.", a.name())
            }
            Ok((n, stale)) => {
                if n < need {
                    ready = false;
                }
                let word = |n: usize| ["no keys", "one key", "two keys", "all three keys"][n.min(3)];
                let mut s = if n >= need {
                    format!("{} is signed by {}. Ready.", a.name(), word(n))
                } else if n == 0 {
                    format!("{} is not signed yet.", a.name())
                } else {
                    format!("{} is signed by {}. One more holder needs to sign it.", a.name(), word(n))
                };
                if stale > 0 {
                    s.push_str(&format!(" {stale} older signature{} in its file no longer match{} it.", if stale == 1 { "" } else { "s" }, if stale == 1 { "es" } else { "" }));
                }
                s
            }
        };
        lines.push(line);
    }
    lines.push(if all.is_empty() {
        "There is nothing here to sign.".into()
    } else if ready {
        "Everything in this folder is signed. Send it back.".into()
    } else {
        "Some files still need signatures.".into()
    });
    (lines, ready)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pyrate-release-test-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("31-0.9.18.apk"), b"an apk").unwrap();
        std::fs::write(d.join("player.manifest"), b"pyrate-player/v1 code 31\n00  index.html\n").unwrap();
        std::fs::write(d.join("players.txt"), b"bafyone\nbafytwo\n").unwrap();
        std::fs::write(d.join("pyrate-radio-0.9.18-windows.zip"), b"a zip").unwrap();
        std::fs::write(d.join("bearings.json"), br#"{"pyrate": "bearings", "at": 1}"#).unwrap();
        std::fs::write(d.join("notes.txt"), b"not signed").unwrap();
        d
    }

    #[test]
    fn two_holders_in_turn_make_a_folder_ready() {
        let ks: Vec<StationKey> = (0..3).map(|_| StationKey::generate().unwrap()).collect();
        let pubs: Vec<[u8; 32]> = ks.iter().map(|k| k.id().0).collect();
        let d = folder("turn");
        let all = artifacts(&d);
        assert_eq!(all.iter().map(|a| a.kind).collect::<Vec<_>>(), vec![Kind::Apk, Kind::Paper, Kind::Manifest, Kind::Players, Kind::Desktop]);
        let (lines, ready) = report(&pubs, &d);
        assert!(!ready);
        assert!(lines[0].contains("not signed yet"), "{lines:?}");
        for a in &all {
            sign(&ks[0], 0, a).unwrap();
        }
        // The same holder twice still counts once.
        for a in &all {
            sign(&ks[0], 0, a).unwrap();
            assert_eq!(count(&pubs, a).unwrap(), (1, 0), "{}", a.name());
        }
        assert!(report(&pubs, &d).0[0].contains("One more holder"));
        for a in &all {
            sign(&ks[2], 2, a).unwrap();
            assert_eq!(count(&pubs, a).unwrap().0, 2, "{}", a.name());
        }
        let (lines, ready) = report(&pubs, &d);
        assert!(ready, "{lines:?}");
        assert!(lines.last().unwrap().contains("Everything"));
        assert_eq!(std::fs::read_to_string(d.join("31-0.9.18.sigs")).unwrap().lines().count(), 2);
        assert!(d.join("pyrate-radio-0.9.18-windows.zip.sigs").exists() && d.join("player.sigs").exists() && d.join("players.sigs").exists());
        // The APK and player lists verify as phones and the installed player check them.
        let apk = std::fs::read(d.join("31-0.9.18.apk")).unwrap();
        let sigs: Vec<(u8, Vec<u8>)> = parse(&std::fs::read_to_string(d.join("31-0.9.18.sigs")).unwrap()).into_iter().map(|(i, s)| (i, s.to_vec())).collect();
        assert!(pyrate_core::release_trusted(&pubs, 31, &sha(&apk), &sigs));
        assert!(pyrate_core::papers::trusted(&pubs, &serde_json::from_str(&std::fs::read_to_string(d.join("bearings.json")).unwrap()).unwrap()));
        // A changed file loses its signatures.
        std::fs::write(d.join("players.txt"), b"bafyone\nbafytwo\nbafythree\n").unwrap();
        let (lines, ready) = report(&pubs, &d);
        assert!(!ready);
        assert!(lines.iter().any(|l| l.starts_with("players.txt is not signed yet") && l.contains("2 older signatures")), "{lines:?}");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_signature_of_one_kind_never_passes_as_another() {
        let k = StationKey::generate().unwrap();
        let pubs = vec![k.id().0];
        let body = b"pyrate-player/v1 code 31\n";
        let as_release = k.sign(&manifest_message(body).unwrap());
        let line = format!("0 {}", as_release.iter().map(|b| format!("{b:02x}")).collect::<String>());
        assert_eq!(good(&pubs, &manifest_message(body).unwrap(), &line).0, 1);
        assert_eq!(good(&pubs, &players_message(body), &line), (0, 1));
        assert_eq!(good(&pubs, &desktop_message(body), &line), (0, 1));
        assert_ne!(players_message(b"bafya\n"), players_message(b"bafya\nbafyb\n"));
    }

    #[test]
    fn an_apk_without_its_code_is_named_and_not_signed() {
        let k = StationKey::generate().unwrap();
        let d = std::env::temp_dir().join(format!("pyrate-release-test-name-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("pyrate-radio.apk"), b"x").unwrap();
        let a = &artifacts(&d)[0];
        assert!(sign(&k, 0, a).unwrap_err().contains("version code"));
        let (lines, ready) = report(&[k.id().0], &d);
        assert!(!ready && lines[0].contains("31-0.9.18.apk"));
        assert_eq!(report(&[k.id().0], &std::env::temp_dir().join("pyrate-no-such-folder")).0, vec!["There is nothing here to sign.".to_string()]);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn the_built_in_keys_are_the_apps() {
        if std::env::var("PYRATE_RELEASE_KEYS").is_err() {
            assert_eq!(keys().len(), 3);
        }
    }
}
