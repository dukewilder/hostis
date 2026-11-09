//! APK Signature Scheme v2, for Pyrate Radio's own builds, run on the keyholder's machine.
//! ECDSA P-256 with SHA-256. Android 7 and later read it; Pyrate Radio needs Android 9.

use anyhow::{Result, anyhow, bail};
use p256::ecdsa::{Signature, SigningKey, signature::Signer};
use sha2::{Digest, Sha256};

const V2_ID: u32 = 0x7109_871a;
const ECDSA_SHA256: u32 = 0x0201;
const MAGIC: &[u8; 16] = b"APK Sig Block 42";

fn lp(b: &[u8]) -> Vec<u8> {
    let mut o = (b.len() as u32).to_le_bytes().to_vec();
    o.extend_from_slice(b);
    o
}

fn u32le(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}

/// The P-256 key a set of 24 words stands for.
pub fn android_key(seed: &[u8; 32]) -> SigningKey {
    for n in 0u32.. {
        let mut h = Sha256::new();
        h.update(b"pyrate/android-signing/v1");
        h.update(seed);
        h.update(n.to_le_bytes());
        if let Ok(k) = SigningKey::from_slice(&h.finalize()) {
            return k;
        }
    }
    unreachable!()
}

/// A self-signed certificate for the key. Made once; Android pins every update to it.
pub fn certificate(key: &SigningKey) -> Result<Vec<u8>> {
    use p256::pkcs8::EncodePrivateKey;
    let der = key.to_pkcs8_der().map_err(|e| anyhow!("{e}"))?;
    let kp = rcgen::KeyPair::try_from(der.as_bytes())?;
    let mut p = rcgen::CertificateParams::new(Vec::<String>::new())?;
    p.distinguished_name = rcgen::DistinguishedName::new();
    p.distinguished_name.push(rcgen::DnType::CommonName, "Pyrate Radio");
    p.distinguished_name.push(rcgen::DnType::OrganizationName, "HOSTIS");
    p.not_before = rcgen::date_time_ymd(2026, 1, 1);
    p.not_after = rcgen::date_time_ymd(2126, 1, 1);
    Ok(p.self_signed(&kp)?.der().to_vec())
}

fn digest(sections: &[&[u8]]) -> Vec<u8> {
    let mut chunks = Vec::new();
    for s in sections {
        for c in s.chunks(1 << 20) {
            let mut h = Sha256::new();
            h.update([0xa5]);
            h.update((c.len() as u32).to_le_bytes());
            h.update(c);
            chunks.push(h.finalize());
        }
    }
    let mut top = Sha256::new();
    top.update([0x5a]);
    top.update((chunks.len() as u32).to_le_bytes());
    for c in &chunks {
        top.update(c);
    }
    top.finalize().to_vec()
}

pub fn sign(apk: &[u8], key: &SigningKey, cert: &[u8]) -> Result<Vec<u8>> {
    let eocd = (0..apk.len().saturating_sub(21)).rev().take(65_557).find(|&i| u32le(apk, i) == 0x0605_4b50).ok_or_else(|| anyhow!("not a zip"))?;
    let cd_size = u32le(apk, eocd + 12) as usize;
    let cd_off = u32le(apk, eocd + 16) as usize;
    if cd_off + cd_size != eocd {
        bail!("unexpected zip layout");
    }
    // An existing signing block is replaced.
    let mut start = cd_off;
    if cd_off >= 24 && &apk[cd_off - 16..cd_off] == MAGIC {
        let size = u64::from_le_bytes(apk[cd_off - 24..cd_off - 16].try_into()?) as usize;
        start = cd_off - size - 8;
    }
    let body = &apk[..start];
    let cd = &apk[cd_off..eocd];
    let mut tail = apk[eocd..].to_vec();
    tail[16..20].copy_from_slice(&(start as u32).to_le_bytes());

    let d = digest(&[body, cd, &tail]);
    let mut digests = ECDSA_SHA256.to_le_bytes().to_vec();
    digests.extend(lp(&d));
    let mut signed = lp(&lp(&digests));
    signed.extend(lp(&lp(cert)));
    signed.extend(lp(&[]));

    let sig: Signature = key.sign(&signed);
    let mut sigrec = ECDSA_SHA256.to_le_bytes().to_vec();
    sigrec.extend(lp(sig.to_der().as_bytes()));
    use p256::pkcs8::EncodePublicKey;
    let spki = key.verifying_key().to_public_key_der().map_err(|e| anyhow!("{e}"))?;

    let mut signer = lp(&signed);
    signer.extend(lp(&lp(&sigrec)));
    signer.extend(lp(spki.as_bytes()));
    let value = lp(&lp(&signer));

    let mut pair = V2_ID.to_le_bytes().to_vec();
    pair.extend(value);
    let mut pairs = (pair.len() as u64).to_le_bytes().to_vec();
    pairs.extend(pair);
    let size = (pairs.len() + 8 + 16) as u64;
    let mut block = size.to_le_bytes().to_vec();
    block.extend(pairs);
    block.extend(size.to_le_bytes());
    block.extend(MAGIC);

    let mut out = body.to_vec();
    out.extend(&block);
    out.extend(cd);
    let mut tail = apk[eocd..].to_vec();
    tail[16..20].copy_from_slice(&((start + block.len()) as u32).to_le_bytes());
    out.extend(tail);
    Ok(out)
}
