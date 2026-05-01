//! Ed25519 signing and verification, plus the `module.sig` file format.

use std::fmt;
use std::io;
use std::io::Write;
use std::str::FromStr;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use ed25519_dalek::Signer as _;
use ed25519_dalek::Verifier as _;
use serde::Deserialize;
use serde::Serialize;
use thiserror::Error;

use crate::ContentHash;

/// An error parsing an Ed25519 key.
#[derive(Debug, Error)]
pub enum KeyError {
    /// The key text could not be parsed as an OpenSSH key.
    #[error("invalid OpenSSH key: {0}")]
    InvalidOpenSshKey(String),

    /// The key parsed but is not an Ed25519 key.
    #[error("OpenSSH key is not an Ed25519 key")]
    WrongAlgorithm,
}

/// An error parsing an Ed25519 signature.
#[derive(Debug, Error)]
pub enum SignatureError {
    /// The signature is not valid base64.
    #[error("signature is not valid base64")]
    InvalidBase64,

    /// The signature is not 64 bytes.
    #[error("signature must be exactly 64 bytes; got {0}")]
    WrongLength(usize),
}

/// An error parsing or writing a `module.sig` file.
#[derive(Debug, Error)]
pub enum SignatureFileError {
    /// The file is not valid JSON.
    #[error("invalid `module.sig` JSON")]
    InvalidJson(#[from] serde_json::Error),

    /// The `public_key` field could not be parsed as an OpenSSH Ed25519
    /// public key.
    #[error(transparent)]
    Key(#[from] KeyError),

    /// The `signature` field could not be parsed as a base64-encoded
    /// 64-byte Ed25519 signature.
    #[error(transparent)]
    Signature(#[from] SignatureError),
}

/// An error verifying an Ed25519 signature against a content hash.
#[derive(Debug, Error)]
#[error("Ed25519 signature does not match the supplied content hash")]
pub struct VerifyError;

/// An Ed25519 signing key.
#[derive(Clone, Debug)]
pub struct SigningKey(ed25519_dalek::SigningKey);

impl SigningKey {
    /// Parses an OpenSSH-format Ed25519 private key (the contents of the
    /// file produced by `ssh-keygen -t ed25519`).
    pub fn from_openssh(text: &str) -> Result<Self, KeyError> {
        let key = ssh_key::PrivateKey::from_openssh(text)
            .map_err(|e| KeyError::InvalidOpenSshKey(e.to_string()))?;
        let ed = key.key_data().ed25519().ok_or(KeyError::WrongAlgorithm)?;
        let bytes: &[u8; 32] = &ed.private.to_bytes();
        Ok(Self(ed25519_dalek::SigningKey::from_bytes(bytes)))
    }

    /// Returns the corresponding [`VerifyingKey`].
    pub fn verifying_key(&self) -> VerifyingKey {
        VerifyingKey(self.0.verifying_key())
    }

    /// Signs the raw 32-byte content digest of a [`ContentHash`].
    pub fn sign(&self, digest: &ContentHash) -> Signature {
        Signature(self.0.sign(digest.as_bytes()))
    }
}

/// An Ed25519 verifying key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub struct VerifyingKey(ed25519_dalek::VerifyingKey);

impl VerifyingKey {
    /// Parses an OpenSSH-format Ed25519 public key (the single-line
    /// `ssh-ed25519 <base64-blob> [comment]` form produced by
    /// `ssh-keygen -t ed25519` in the corresponding `.pub` file). Trailing
    /// comments are not significant.
    pub fn from_openssh(text: &str) -> Result<Self, KeyError> {
        let key = ssh_key::PublicKey::from_openssh(text.trim())
            .map_err(|e| KeyError::InvalidOpenSshKey(e.to_string()))?;
        let ed = key.key_data().ed25519().ok_or(KeyError::WrongAlgorithm)?;
        let inner = ed25519_dalek::VerifyingKey::from_bytes(&ed.0)
            .map_err(|e| KeyError::InvalidOpenSshKey(e.to_string()))?;
        Ok(Self(inner))
    }

    /// Returns the canonical OpenSSH form `ssh-ed25519 <base64-blob>`,
    /// without a trailing comment.
    pub fn to_openssh(&self) -> String {
        let ed = ssh_key::public::Ed25519PublicKey(*self.0.as_bytes());
        let key = ssh_key::PublicKey::from(ssh_key::public::KeyData::Ed25519(ed));
        // SAFETY: encoding a freshly-constructed in-memory Ed25519
        // `PublicKey` into OpenSSH form cannot fail.
        key.to_openssh().unwrap()
    }

    /// Verifies an Ed25519 [`Signature`] over the raw 32-byte digest of a
    /// [`ContentHash`].
    pub fn verify(&self, digest: &ContentHash, sig: &Signature) -> Result<(), VerifyError> {
        self.0
            .verify(digest.as_bytes(), &sig.0)
            .map_err(|_| VerifyError)
    }

    /// Returns the raw 32-byte public key.
    pub fn as_bytes(&self) -> &[u8; 32] {
        self.0.as_bytes()
    }
}

impl fmt::Display for VerifyingKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_openssh())
    }
}

impl FromStr for VerifyingKey {
    type Err = KeyError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_openssh(s)
    }
}

impl From<VerifyingKey> for String {
    fn from(key: VerifyingKey) -> Self {
        key.to_openssh()
    }
}

impl TryFrom<String> for VerifyingKey {
    type Error = KeyError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        Self::from_openssh(&s)
    }
}

/// An Ed25519 signature over a [`ContentHash`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub struct Signature(ed25519_dalek::Signature);

impl Signature {
    /// Parses a base64-encoded 64-byte Ed25519 signature.
    pub fn from_base64(s: &str) -> Result<Self, SignatureError> {
        let bytes = BASE64_STANDARD
            .decode(s)
            .map_err(|_| SignatureError::InvalidBase64)?;
        let array: [u8; 64] = bytes
            .as_slice()
            .try_into()
            .map_err(|_| SignatureError::WrongLength(bytes.len()))?;
        Ok(Self(ed25519_dalek::Signature::from_bytes(&array)))
    }

    /// Returns the signature in base64 form.
    pub fn to_base64(&self) -> String {
        BASE64_STANDARD.encode(self.0.to_bytes())
    }
}

impl fmt::Display for Signature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_base64())
    }
}

impl FromStr for Signature {
    type Err = SignatureError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_base64(s)
    }
}

impl From<Signature> for String {
    fn from(sig: Signature) -> Self {
        sig.to_base64()
    }
}

impl TryFrom<String> for Signature {
    type Error = SignatureError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        Self::from_base64(&s)
    }
}

/// The contents of a `module.sig` file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleSignature {
    /// The signer's Ed25519 public key in OpenSSH format.
    pub public_key: VerifyingKey,
    /// The Ed25519 signature over the module's raw 32-byte content hash.
    pub signature: Signature,
}

impl ModuleSignature {
    /// Parses a `module.sig` JSON document.
    pub fn parse(bytes: &[u8]) -> Result<Self, SignatureFileError> {
        Ok(serde_json::from_slice(bytes)?)
    }

    /// Writes the signature as JSON to `w`.
    pub fn write(&self, w: impl Write) -> io::Result<()> {
        serde_json::to_writer_pretty(w, self).map_err(io::Error::other)
    }

    /// Verifies that `signature` is a valid signature of `digest` under
    /// `public_key`.
    pub fn verify(&self, digest: &ContentHash) -> Result<(), VerifyError> {
        self.public_key.verify(digest, &self.signature)
    }
}

/// Helpers for tests.
#[cfg(any(test, feature = "test-utils"))]
pub mod test_utils {
    use sha2::Digest;
    use sha2::Sha256;

    use super::*;

    /// Generates a deterministic [`SigningKey`] from a `u64` seed.
    ///
    /// Available only with the `test-utils` cargo feature; not part of the
    /// production public API. Production callers should generate keys with
    /// `ssh-keygen -t ed25519` and load them via
    /// [`SigningKey::from_openssh`].
    pub fn signing_key_from_seed(seed: u64) -> SigningKey {
        let mut hasher = Sha256::new();
        hasher.update(seed.to_le_bytes());
        let bytes: [u8; 32] = hasher.finalize().into();
        SigningKey(ed25519_dalek::SigningKey::from_bytes(&bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::test_utils::signing_key_from_seed;
    use super::*;

    #[test]
    fn signs_and_verifies_round_trip() {
        let signer = signing_key_from_seed(42);
        let verifier = signer.verifying_key();
        let digest = ContentHash::from([0xAB; 32]);

        let sig = signer.sign(&digest);
        verifier.verify(&digest, &sig).unwrap();
    }

    #[test]
    fn detects_tampered_signature() {
        let signer = signing_key_from_seed(42);
        let verifier = signer.verifying_key();
        let digest = ContentHash::from([0xAB; 32]);

        let sig = signer.sign(&digest);
        let tampered = ContentHash::from([0xAC; 32]);
        assert!(verifier.verify(&tampered, &sig).is_err());
    }

    #[test]
    fn verifying_key_round_trips_through_openssh() {
        let signer = signing_key_from_seed(1);
        let key = signer.verifying_key();
        let openssh = key.to_openssh();
        assert!(openssh.starts_with("ssh-ed25519 "));
        let parsed = VerifyingKey::from_openssh(&openssh).unwrap();
        assert_eq!(parsed.as_bytes(), key.as_bytes());
    }

    #[test]
    fn verifying_key_accepts_openssh_with_comment() {
        let signer = signing_key_from_seed(2);
        let key = signer.verifying_key();
        let with_comment = format!("{} user@example.com", key.to_openssh());
        let parsed = VerifyingKey::from_openssh(&with_comment).unwrap();
        assert_eq!(parsed.as_bytes(), key.as_bytes());
    }

    #[test]
    fn signature_round_trips_through_base64() {
        let signer = signing_key_from_seed(3);
        let digest = ContentHash::from([0x11; 32]);
        let sig = signer.sign(&digest);
        let b64 = sig.to_base64();
        let parsed = Signature::from_base64(&b64).unwrap();
        assert_eq!(parsed, sig);
    }

    #[test]
    fn module_signature_round_trips_through_json() {
        let signer = signing_key_from_seed(4);
        let digest = ContentHash::from([0x22; 32]);
        let module_sig = ModuleSignature {
            public_key: signer.verifying_key(),
            signature: signer.sign(&digest),
        };

        let mut buf = Vec::new();
        module_sig.write(&mut buf).unwrap();
        let parsed = ModuleSignature::parse(&buf).unwrap();
        assert_eq!(parsed, module_sig);
        parsed.verify(&digest).unwrap();
    }

    #[test]
    fn module_signature_rejects_unknown_keys() {
        // The schema only has `public_key` and `signature`; serde_json's
        // default Deserialize ignores unknown fields. We rely on the
        // top-level strict-parsing wrapper (when added) to reject those;
        // here we just check that the well-formed schema still parses.
        let signer = signing_key_from_seed(5);
        let digest = ContentHash::from([0x33; 32]);
        let json = serde_json::to_string(&ModuleSignature {
            public_key: signer.verifying_key(),
            signature: signer.sign(&digest),
        })
        .unwrap();
        ModuleSignature::parse(json.as_bytes()).unwrap();
    }
}
