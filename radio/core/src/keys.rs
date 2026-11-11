//! Station keys. Made on the device, never sent anywhere, exportable as 24 words.

use ed25519_dalek::{Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};

use crate::{Error, Result};

/// The public half. A station's identity, its address and its proof.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct StationId(pub [u8; 32]);

impl StationId {
    pub fn verify(&self, msg: &[u8], sig: &[u8; 64]) -> Result<()> {
        let key = VerifyingKey::from_bytes(&self.0).map_err(|_| Error::Key)?;
        let sig = ed25519_dalek::Signature::from_bytes(sig);
        key.verify_strict(msg, &sig).map_err(|_| Error::Signature)
    }

    pub fn call_sign(&self) -> String {
        crate::callsign::call_sign(self)
    }

    pub fn to_hex(&self) -> String {
        self.0.iter().map(|b| format!("{b:02x}")).collect()
    }

    pub fn from_hex(s: &str) -> Result<Self> {
        let out = crate::unhex32(s).ok_or(Error::Key)?;
        VerifyingKey::from_bytes(&out).map_err(|_| Error::Key)?;
        Ok(Self(out))
    }
}

impl std::fmt::Debug for StationId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "StationId({})", self.call_sign())
    }
}

impl std::fmt::Display for StationId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.to_hex())
    }
}

/// The seed the words make when a passphrase stands on top of them.
fn passed(seed: &[u8; 32], pass: &str) -> Result<[u8; 32]> {
    let params = argon2::Params::new(64 * 1024, 3, 1, Some(32)).map_err(|_| Error::Key)?;
    let mut out = [0u8; 32];
    let salt = blake3::derive_key("pyrate/passphrase/v1", seed);
    argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params)
        .hash_password_into(pass.as_bytes(), &salt, &mut out)
        .map_err(|_| Error::Key)?;
    Ok(blake3::derive_key("pyrate/passphrase/seed/v1", &[seed.as_slice(), &out].concat()))
}

/// The private half. Whoever holds it is the station.
pub struct StationKey(SigningKey);

impl StationKey {
    pub fn generate() -> Result<Self> {
        let mut seed = [0u8; 32];
        getrandom::fill(&mut seed).map_err(|_| Error::Key)?;
        Ok(Self(SigningKey::from_bytes(&seed)))
    }

    pub fn from_seed(seed: [u8; 32]) -> Self {
        Self(SigningKey::from_bytes(&seed))
    }

    pub fn seed(&self) -> [u8; 32] {
        self.0.to_bytes()
    }

    /// The key as 24 BIP-39 words.
    pub fn phrase(&self) -> String {
        bip39::Mnemonic::from_entropy(&self.seed())
            .expect("32 bytes is valid entropy")
            .to_string()
    }

    pub fn from_phrase(phrase: &str) -> Result<Self> {
        let words = phrase.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase();
        let m = bip39::Mnemonic::parse(words).map_err(|e| Error::Phrase(e.to_string()))?;
        let entropy = m.to_entropy();
        let seed: [u8; 32] = entropy
            .try_into()
            .map_err(|_| Error::Phrase("a station key is 24 words".into()))?;
        Ok(Self::from_seed(seed))
    }

    /// The key from 24 words and a passphrase, which may be empty. With a passphrase the words
    /// alone are another station: a photograph of the words gives nothing away, and words given
    /// up under force can open that other, empty one instead. The passphrase is stretched with
    /// Argon2id, so guessing it costs a second or so a try.
    pub fn from_words(phrase: &str, pass: &str) -> Result<Self> {
        let base = Self::from_phrase(phrase)?;
        Ok(if pass.is_empty() { base } else { Self::from_seed(passed(&base.seed(), pass)?) })
    }

    /// A fresh key and the 24 words that, with `pass`, make it.
    pub fn generate_with(pass: &str) -> Result<(Self, String)> {
        let words = Self::generate()?;
        let phrase = words.phrase();
        Ok((if pass.is_empty() { words } else { Self::from_seed(passed(&words.seed(), pass)?) }, phrase))
    }

    pub fn id(&self) -> StationId {
        StationId(self.0.verifying_key().to_bytes())
    }

    pub fn sign(&self, msg: &[u8]) -> [u8; 64] {
        self.0.sign(msg).to_bytes()
    }
}

#[cfg(test)]
mod passphrase_tests {
    use super::*;

    #[test]
    fn a_passphrase_makes_the_words_another_station() {
        let (k, words) = StationKey::generate_with("the tide turns at four").unwrap();
        assert_eq!(StationKey::from_words(&words, "the tide turns at four").unwrap().id(), k.id());
        assert_ne!(StationKey::from_words(&words, "").unwrap().id(), k.id(), "the words alone are another station");
        assert_ne!(StationKey::from_words(&words, "the tide turns at five").unwrap().id(), k.id());
        let (plain, w2) = StationKey::generate_with("").unwrap();
        assert_eq!(StationKey::from_words(&w2, "").unwrap().id(), plain.id(), "no passphrase is the words as before");
    }
}
