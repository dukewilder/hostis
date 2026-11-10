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

    pub fn id(&self) -> StationId {
        StationId(self.0.verifying_key().to_bytes())
    }

    pub fn sign(&self, msg: &[u8]) -> [u8; 64] {
        self.0.sign(msg).to_bytes()
    }
}
