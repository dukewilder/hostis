//! The ship's papers: small documents the release keys sign, carried by anyone.
//!
//! **Bearings** say where the fleet can be found: first doors (`id@relay`), open players, the
//! IPNS name, an onion address, and the fingerprints of the latest app, desktop builds and
//! source. They take two of the three release keys, as a release does, because they steer
//! where every new install goes first.
//!
//! **The canary** is a line HOSTIS signs once a month. Any one release key will do. A canary
//! that stops being renewed says what a gag order forbids saying.
//!
//! Both are JSON, signed over their own text without the `sigs` field, so they can travel by
//! any road at all (the Releases station's log, a web page, IPFS, a QR code on paper) and be
//! checked wherever they land. No rights reserved.

use std::collections::HashSet;

use serde_json::Value;

use crate::{StationId, StationKey, unhex_into};

/// What a signature covers: the kind, then the document without its signatures.
pub fn message(kind: &str, doc: &Value) -> Vec<u8> {
    let mut d = doc.clone();
    if let Some(o) = d.as_object_mut() {
        o.remove("sigs");
    }
    let mut m = format!("pyrate/{kind}/v1\n").into_bytes();
    m.extend(serde_json::to_vec(&d).unwrap_or_default());
    m
}

/// How many distinct release keys signed this document as `kind`.
pub fn signers(keys: &[[u8; 32]], kind: &str, doc: &Value) -> usize {
    if doc["pyrate"].as_str() != Some(kind) {
        return 0;
    }
    let msg = message(kind, doc);
    let mut good = HashSet::new();
    for s in doc["sigs"].as_array().into_iter().flatten() {
        let (Some(i), Some(h)) = (s[0].as_u64(), s[1].as_str()) else { continue };
        let Some(k) = keys.get(i as usize) else { continue };
        let mut sig = [0u8; 64];
        if unhex_into(h, &mut sig).is_some() && StationId(*k).verify(&msg, &sig).is_ok() {
            good.insert(*k);
        }
    }
    good.len()
}

/// Whether the release keys stand behind this document: two for bearings, one for a canary.
pub fn trusted(keys: &[[u8; 32]], doc: &Value) -> bool {
    match doc["pyrate"].as_str() {
        Some("bearings") => signers(keys, "bearings", doc) >= 2,
        Some("canary") => signers(keys, "canary", doc) >= 1,
        _ => false,
    }
}

/// Signs a document as release key `n` (0, 1 or 2), replacing that key's earlier signature.
pub fn sign(key: &StationKey, n: u8, doc: &mut Value) -> Option<()> {
    let kind = doc["pyrate"].as_str()?.to_string();
    let sig: String = key.sign(&message(&kind, doc)).iter().map(|b| format!("{b:02x}")).collect();
    let o = doc.as_object_mut()?;
    let mut sigs: Vec<Value> = o.get("sigs").and_then(|s| s.as_array().cloned()).unwrap_or_default();
    sigs.retain(|s| s[0].as_u64() != Some(n as u64));
    sigs.push(serde_json::json!([n, sig]));
    o.insert("sigs".into(), Value::Array(sigs));
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn papers_need_their_keys() {
        let k: Vec<StationKey> = (0..3).map(|_| StationKey::generate().unwrap()).collect();
        let ids: Vec<[u8; 32]> = k.iter().map(|k| k.id().0).collect();
        let mut b = json!({ "pyrate": "bearings", "at": 1, "doors": ["a@https://r.example/"] });
        sign(&k[0], 0, &mut b).unwrap();
        assert!(!trusted(&ids, &b), "one key is not enough for bearings");
        sign(&k[0], 0, &mut b).unwrap();
        assert_eq!(signers(&ids, "bearings", &b), 1, "the same key twice is one key");
        sign(&k[2], 2, &mut b).unwrap();
        assert!(trusted(&ids, &b), "two keys are");
        let text = serde_json::to_string(&b).unwrap();
        let back: Value = serde_json::from_str(&text).unwrap();
        assert!(trusted(&ids, &back), "and it survives the trip as text");
        let mut forged = back.clone();
        forged["doors"] = json!(["evil@https://x.example/"]);
        assert!(!trusted(&ids, &forged), "a changed door breaks it");
        let mut c = json!({ "pyrate": "canary", "at": 5, "text": "No orders." });
        assert!(!trusted(&ids, &c));
        sign(&k[1], 1, &mut c).unwrap();
        assert!(trusted(&ids, &c), "one key signs a canary");
        let mut wrong = c.clone();
        wrong["pyrate"] = json!("bearings");
        assert!(!trusted(&ids, &wrong), "a canary's signature is not a bearing's");
    }
}
