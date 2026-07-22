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
use serde_with::DeserializeFromStr;
use serde_with::SerializeDisplay;
use thiserror::Error;
#[cfg(feature = "git-resolver")]
use toml_spanner::Arena;
#[cfg(feature = "git-resolver")]
use toml_spanner::Context;
#[cfg(feature = "git-resolver")]
use toml_spanner::Error as TomlError;
#[cfg(feature = "git-resolver")]
use toml_spanner::Failed;
#[cfg(feature = "git-resolver")]
use toml_spanner::FromToml;
#[cfg(feature = "git-resolver")]
use toml_spanner::Item;
#[cfg(feature = "git-resolver")]
use toml_spanner::ToToml;
#[cfg(feature = "git-resolver")]
use toml_spanner::ToTomlError;

use crate::hash::ContentHash;

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

    /// Signer identity metadata contains an invalid field.
    #[error(
        "invalid signer identity `{field}`; values must be non-empty and at most 256 characters \
         without control characters"
    )]
    InvalidIdentity {
        /// The invalid identity field.
        field: &'static str,
    },
}

/// An error verifying an Ed25519 module signature.
#[derive(Debug, Error)]
#[error("signature does not match the supplied module content or signer identity")]
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

    /// Signs an encoded module signature payload.
    fn sign_message(&self, message: &[u8]) -> Signature {
        Signature(self.0.sign(message))
    }
}

/// An Ed25519 verifying key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, SerializeDisplay, DeserializeFromStr)]
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

    /// Verifies an encoded module signature payload.
    fn verify_message(&self, message: &[u8], sig: &Signature) -> Result<(), VerifyError> {
        self.0.verify(message, &sig.0).map_err(|_| VerifyError)
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

/// Authenticated human-readable metadata associated with a signing key.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub enum SignerIdentity {
    /// A parsed signer name and email address.
    Signer {
        /// The signer's display name.
        name: String,
        /// The signer's email address.
        email: String,
    },
    /// An unstructured OpenSSH public key comment.
    Comment {
        /// The complete public key comment.
        comment: String,
    },
}

impl SignerIdentity {
    /// Returns the parsed signer name when this identity is structured.
    pub fn name(&self) -> Option<&str> {
        match self {
            Self::Signer { name, .. } => Some(name),
            Self::Comment { .. } => None,
        }
    }

    /// Returns the parsed signer email when this identity is structured.
    pub fn email(&self) -> Option<&str> {
        match self {
            Self::Signer { email, .. } => Some(email),
            Self::Comment { .. } => None,
        }
    }

    /// Returns the complete comment when this identity is unstructured.
    pub fn comment(&self) -> Option<&str> {
        match self {
            Self::Signer { .. } => None,
            Self::Comment { comment } => Some(comment),
        }
    }
}

/// Parses identity metadata from an OpenSSH public key comment.
pub fn parse_openssh_public_key_identity(text: &str) -> Option<SignerIdentity> {
    let (kind, rest) = split_openssh_field(text)?;
    let (blob, comment) = split_openssh_field(rest)?;
    let comment = comment.trim().to_string();
    debug_assert!(!kind.is_empty() && !blob.is_empty());
    if comment.is_empty() {
        return None;
    }

    if let Some(without_end) = comment.strip_suffix('>')
        && let Some((name, email)) = without_end.rsplit_once('<')
    {
        let name = name.trim();
        let email = email.trim();
        if !name.is_empty() && !email.is_empty() {
            return Some(SignerIdentity::Signer {
                name: name.to_string(),
                email: email.to_string(),
            });
        }
    }

    Some(SignerIdentity::Comment { comment })
}

/// Splits the first whitespace-delimited OpenSSH field from the remainder.
fn split_openssh_field(text: &str) -> Option<(&str, &str)> {
    let text = text.trim_start();
    if text.is_empty() {
        return None;
    }
    let end = text.find(char::is_whitespace).unwrap_or(text.len());
    Some((&text[..end], text[end..].trim_start()))
}

#[cfg(feature = "git-resolver")]
impl<'de> FromToml<'de> for VerifyingKey {
    fn from_toml(ctx: &mut Context<'de>, item: &Item<'de>) -> Result<Self, Failed> {
        if let Some(s) = item.as_str() {
            return s
                .parse()
                .map_err(|e: KeyError| ctx.push_error(TomlError::custom(e, item.span())));
        }

        Err(ctx.report_expected_but_found(&"an OpenSSH public key string", item))
    }
}

#[cfg(feature = "git-resolver")]
impl ToToml for VerifyingKey {
    fn to_toml<'a>(&'a self, arena: &'a Arena) -> Result<Item<'a>, ToTomlError> {
        Ok(Item::string(arena.alloc_str(&self.to_openssh())))
    }
}

/// An Ed25519 signature over a [`ContentHash`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, SerializeDisplay, DeserializeFromStr)]
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

/// The contents of a `module.sig` file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModuleSignature {
    /// The signer's Ed25519 public key in OpenSSH format.
    public_key: VerifyingKey,
    /// Optional signer identity metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    identity: Option<SignerIdentity>,
    /// The Ed25519 signature over the module content and signer identity.
    signature: Signature,
}

impl ModuleSignature {
    /// Creates a signature over module content and signer identity metadata.
    pub fn new(
        signing_key: &SigningKey,
        digest: &ContentHash,
        identity: Option<SignerIdentity>,
    ) -> Result<Self, SignatureFileError> {
        validate_identity(identity.as_ref())?;
        let signature = signing_key.sign_message(&signature_message(digest, identity.as_ref()));
        Ok(Self {
            public_key: signing_key.verifying_key(),
            identity,
            signature,
        })
    }

    /// Parses a `module.sig` JSON document.
    pub fn parse(bytes: &[u8]) -> Result<Self, SignatureFileError> {
        let signature: Self = crate::strict_json::from_slice(bytes)?;
        validate_identity(signature.identity.as_ref())?;
        Ok(signature)
    }

    /// Writes the signature as JSON to `w`.
    pub fn write(&self, w: impl Write) -> io::Result<()> {
        validate_identity(self.identity.as_ref()).map_err(io::Error::other)?;
        serde_json::to_writer_pretty(w, self).map_err(io::Error::other)
    }

    /// Returns the signer public key.
    pub fn public_key(&self) -> VerifyingKey {
        self.public_key
    }

    /// Returns the authenticated signer identity metadata.
    pub fn identity(&self) -> Option<&SignerIdentity> {
        self.identity.as_ref()
    }

    /// Verifies the module content and signer identity.
    pub fn verify(&self, digest: &ContentHash) -> Result<(), VerifyError> {
        self.public_key.verify_message(
            &signature_message(digest, self.identity.as_ref()),
            &self.signature,
        )
    }
}

/// Validates identity fields before signing, writing, or displaying them.
fn validate_identity(identity: Option<&SignerIdentity>) -> Result<(), SignatureFileError> {
    let Some(identity) = identity else {
        return Ok(());
    };
    match identity {
        SignerIdentity::Signer { name, email } => {
            validate_identity_field("name", name)?;
            validate_identity_field("email", email)?;
        }
        SignerIdentity::Comment { comment } => validate_identity_field("comment", comment)?,
    }
    Ok(())
}

/// Validates one signer identity string.
fn validate_identity_field(field: &'static str, value: &str) -> Result<(), SignatureFileError> {
    if value.is_empty() || value.chars().count() > 256 || value.chars().any(char::is_control) {
        return Err(SignatureFileError::InvalidIdentity { field });
    }
    Ok(())
}

/// Encodes the domain-separated payload covered by a module signature.
fn signature_message(digest: &ContentHash, identity: Option<&SignerIdentity>) -> Vec<u8> {
    const DOMAIN: &[u8] = b"openwdl.module-signature.v2";

    let mut message = Vec::with_capacity(128);
    message.extend_from_slice(DOMAIN);
    message.extend_from_slice(digest.as_bytes());
    match identity {
        None => message.push(0),
        Some(SignerIdentity::Signer { name, email }) => {
            message.push(1);
            append_string(&mut message, name);
            append_string(&mut message, email);
        }
        Some(SignerIdentity::Comment { comment }) => {
            message.push(2);
            append_string(&mut message, comment);
        }
    }
    message
}

/// Appends a length-framed UTF-8 string to a signature payload.
fn append_string(message: &mut Vec<u8>, value: &str) {
    message.extend_from_slice(&(value.len() as u64).to_le_bytes());
    message.extend_from_slice(value.as_bytes());
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
        // SAFETY: the signature was created for this key and digest.
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
        // SAFETY: `to_openssh` emits a valid Ed25519 public key.
        let parsed = VerifyingKey::from_openssh(&openssh).unwrap();
        assert_eq!(parsed.as_bytes(), key.as_bytes());
    }

    #[test]
    fn verifying_key_accepts_openssh_with_comment() {
        let signer = signing_key_from_seed(2);
        let key = signer.verifying_key();
        let with_comment = format!("{} user@example.com", key.to_openssh());
        // SAFETY: appending a comment does not change the valid key fields.
        let parsed = VerifyingKey::from_openssh(&with_comment).unwrap();
        assert_eq!(parsed.as_bytes(), key.as_bytes());
    }

    #[test]
    fn parses_identity_from_openssh_comment() {
        let signer = signing_key_from_seed(7);
        let key = signer.verifying_key();
        // SAFETY: the public key text includes a non-empty comment.
        let identity =
            parse_openssh_public_key_identity(&format!("{} Jane Doe <jane@example.com>", key))
                .unwrap();
        assert_eq!(identity.name(), Some("Jane Doe"));
        assert_eq!(identity.email(), Some("jane@example.com"));
    }

    #[test]
    fn preserves_unstructured_openssh_comment() {
        let signer = signing_key_from_seed(7);
        let key = signer.verifying_key();
        // SAFETY: the public key text includes a non-empty comment.
        let identity =
            parse_openssh_public_key_identity(&format!("{key} release   signer")).unwrap();
        // SAFETY: signer identities contain only serializable strings.
        let value = serde_json::to_value(identity).unwrap();

        assert_eq!(value, serde_json::json!({ "comment": "release   signer" }));
    }

    #[test]
    fn preserves_signer_like_comment_with_trailing_text() {
        let signer = signing_key_from_seed(7);
        let key = signer.verifying_key();
        // SAFETY: the public key text includes a non-empty comment.
        let identity = parse_openssh_public_key_identity(&format!(
            "{key} Jane Doe <jane@example.com> trailing"
        ))
        .unwrap();
        // SAFETY: signer identities contain only serializable strings.
        let value = serde_json::to_value(identity).unwrap();

        assert_eq!(
            value,
            serde_json::json!({ "comment": "Jane Doe <jane@example.com> trailing" })
        );
    }

    #[test]
    fn signature_round_trips_through_base64() {
        let signer = signing_key_from_seed(3);
        let digest = ContentHash::from([0x11; 32]);
        let sig = signer.sign(&digest);
        let b64 = sig.to_base64();
        // SAFETY: `to_base64` emits a valid encoded signature.
        let parsed = Signature::from_base64(&b64).unwrap();
        assert_eq!(parsed, sig);
    }

    #[test]
    fn module_signature_round_trips_through_json() {
        let signer = signing_key_from_seed(4);
        let digest = ContentHash::from([0x22; 32]);
        // SAFETY: absent identity metadata contains no invalid fields.
        let module_sig = ModuleSignature::new(&signer, &digest, None).unwrap();

        let mut buf = Vec::new();
        // SAFETY: writing valid signature data to an in-memory buffer cannot fail.
        module_sig.write(&mut buf).unwrap();
        // SAFETY: `write` emitted a valid module signature document.
        let parsed = ModuleSignature::parse(&buf).unwrap();
        assert_eq!(parsed, module_sig);
        // SAFETY: the parsed signature was created for this digest.
        parsed.verify(&digest).unwrap();
    }

    #[test]
    fn module_signature_error_omits_algorithm_name() {
        let signer = signing_key_from_seed(5);
        let signed_digest = ContentHash::from([0x22; 32]);
        let checked_digest = ContentHash::from([0x33; 32]);
        // SAFETY: absent identity metadata contains no invalid fields.
        let module_sig = ModuleSignature::new(&signer, &signed_digest, None).unwrap();

        let error = module_sig.verify(&checked_digest).unwrap_err();
        assert_eq!(
            error.to_string(),
            "signature does not match the supplied module content or signer identity"
        );
    }

    #[test]
    fn module_signature_authenticates_identity() -> Result<(), Box<dyn std::error::Error>> {
        let signer = signing_key_from_seed(8);
        let digest = ContentHash::from([0x55; 32]);
        let identity = SignerIdentity::Signer {
            name: "Original Signer".to_string(),
            email: "original@example.com".to_string(),
        };
        let signature = ModuleSignature::new(&signer, &digest, Some(identity))?;
        let mut value = serde_json::to_value(&signature)?;
        value["identity"]["name"] = serde_json::Value::String("Impostor".to_string());
        let bytes = serde_json::to_vec(&value)?;
        let tampered = ModuleSignature::parse(&bytes)?;

        assert!(tampered.verify(&digest).is_err());
        Ok(())
    }

    #[test]
    fn module_signature_rejects_mixed_identity_fields() -> Result<(), Box<dyn std::error::Error>> {
        let signer = signing_key_from_seed(8);
        let digest = ContentHash::from([0x55; 32]);
        let identity = SignerIdentity::Signer {
            name: "Original Signer".to_string(),
            email: "original@example.com".to_string(),
        };
        let signature = ModuleSignature::new(&signer, &digest, Some(identity))?;
        let mut value = serde_json::to_value(&signature)?;
        value["identity"]["comment"] = serde_json::Value::String("unexpected comment".to_string());

        assert!(ModuleSignature::parse(&serde_json::to_vec(&value)?).is_err());
        Ok(())
    }

    #[test]
    fn module_signature_rejects_identity_control_characters() {
        let signer = signing_key_from_seed(9);
        let digest = ContentHash::from([0x66; 32]);
        // SAFETY: the public key text includes a non-empty comment.
        let identity = parse_openssh_public_key_identity(&format!(
            "{} trusted\u{1b}[2J",
            signer.verifying_key()
        ))
        .unwrap();

        assert!(matches!(
            ModuleSignature::new(&signer, &digest, Some(identity)),
            Err(SignatureFileError::InvalidIdentity { field: "comment" })
        ));
    }

    #[test]
    fn module_signature_rejects_unknown_keys() {
        let signer = signing_key_from_seed(6);
        let digest = ContentHash::from([0x33; 32]);
        // SAFETY: verifying keys always serialize as JSON strings.
        let public_key = serde_json::to_string(&signer.verifying_key()).unwrap();
        // SAFETY: signatures always serialize as JSON strings.
        let signature = serde_json::to_string(&signer.sign(&digest)).unwrap();
        let json = format!(
            r#"{{
                "public_key": {},
                "signature": {},
                "unexpected": true
            }}"#,
            public_key, signature
        );

        assert!(ModuleSignature::parse(json.as_bytes()).is_err());
    }

    #[test]
    fn module_signature_rejects_duplicate_keys() {
        let signer = signing_key_from_seed(6);
        let digest = ContentHash::from([0x44; 32]);
        // SAFETY: verifying keys always serialize as JSON strings.
        let public_key = serde_json::to_string(&signer.verifying_key()).unwrap();
        // SAFETY: signatures always serialize as JSON strings.
        let signature = serde_json::to_string(&signer.sign(&digest)).unwrap();
        let json = format!(
            r#"{{
                "public_key": {},
                "public_key": {},
                "signature": {}
            }}"#,
            public_key, public_key, signature
        );

        let err = ModuleSignature::parse(json.as_bytes()).unwrap_err();
        assert!(
            err.to_string().contains("invalid `module.sig` JSON"),
            "wrong error: {err}"
        );
    }

    #[test]
    fn signer_signature_message_matches_openwdl_vector() {
        let digest = ContentHash::from([0x42; 32]);
        let identity = SignerIdentity::Signer {
            name: "Jane Doe".to_string(),
            email: "jane@example.com".to_string(),
        };
        let message = signature_message(&digest, Some(&identity));
        assert_eq!(
            hex::encode(&message),
            concat!(
                "6f70656e77646c2e6d6f64756c652d7369676e61747572652e7632",
                "4242424242424242424242424242424242424242424242424242424242424242",
                "0108000000000000004a616e6520446f65",
                "10000000000000006a616e65406578616d706c652e636f6d"
            )
        );
    }

    #[test]
    fn comment_signature_message_matches_openwdl_vector() {
        let signer = signing_key_from_seed(7);
        let digest = ContentHash::from([0x42; 32]);
        // SAFETY: the public key text includes a non-empty comment.
        let identity = parse_openssh_public_key_identity(&format!(
            "{} release signer",
            signer.verifying_key()
        ))
        .unwrap();
        let message = signature_message(&digest, Some(&identity));

        assert_eq!(
            hex::encode(&message),
            concat!(
                "6f70656e77646c2e6d6f64756c652d7369676e61747572652e7632",
                "4242424242424242424242424242424242424242424242424242424242424242",
                "020e0000000000000072656c65617365207369676e6572"
            )
        );
    }

    #[test]
    fn module_signature_rejects_sprocket_domain() {
        let signer = signing_key_from_seed(11);
        let digest = ContentHash::from([0x42; 32]);
        let name = "Jane Doe";
        let email = "jane@example.com";

        // Construct the old-domain payload manually.
        let mut old_payload = Vec::new();
        old_payload.extend_from_slice(b"sprocket.module-signature.v2");
        old_payload.extend_from_slice(digest.as_bytes());
        old_payload.push(1);
        append_string(&mut old_payload, name);
        append_string(&mut old_payload, email);

        // Sign the old-domain payload.
        let old_sig = signer.sign_message(&old_payload);

        // Build a ModuleSignature via JSON carrying the old-domain signature.
        let json = serde_json::json!({
            "public_key": signer.verifying_key().to_string(),
            "identity": {
                "name": "Jane Doe",
                "email": "jane@example.com"
            },
            "signature": old_sig.to_base64()
        });
        // SAFETY: the JSON value contains only serializable strings.
        let bytes = serde_json::to_vec(&json).unwrap();
        // SAFETY: the JSON is well-formed and all identity fields are valid.
        let module_sig = ModuleSignature::parse(&bytes).unwrap();

        assert!(module_sig.verify(&digest).is_err());
    }
}
