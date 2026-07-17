use std::fmt;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use oci_spec::distribution::Reference;

use crate::v1::requirements::ContainerSource;

/// The transport used by a canonical registry reference.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RegistryTransport {
    /// Docker registry transport.
    Docker,
    /// ORAS registry transport.
    Oras,
}

/// A canonical transport-aware registry reference.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct RegistryReference {
    transport: RegistryTransport,
    reference: Reference,
}

impl RegistryReference {
    /// Creates a canonical registry reference from a WDL container source.
    pub fn try_from_source(source: &ContainerSource) -> Result<Self> {
        let (transport, value) = match source {
            ContainerSource::Docker(value) => (RegistryTransport::Docker, value),
            ContainerSource::Oras(value) => (RegistryTransport::Oras, value),
            ContainerSource::Library(_) | ContainerSource::Unknown(_) => {
                bail!("unsupported mutable container source `{source:#}`")
            }
            ContainerSource::SifFile(_) => {
                bail!("local SIF source `{source:#}` is not a registry reference")
            }
        };

        let mut reference = value
            .parse::<Reference>()
            .with_context(|| format!("invalid registry reference `{source:#}`"))?;
        if let Some(digest) = reference.digest() {
            validate_sha256(digest)?;
            reference = Reference::with_digest(
                reference.registry().to_owned(),
                reference.repository().to_owned(),
                digest.to_owned(),
            );
        }

        Ok(Self {
            transport,
            reference,
        })
    }

    /// Returns the canonical reference string.
    pub fn canonical(&self) -> String {
        let scheme = match self.transport {
            RegistryTransport::Docker => "docker",
            RegistryTransport::Oras => "oras",
        };
        format!("{scheme}://{}", self.reference)
    }

    /// Returns whether the reference is immutable.
    pub fn is_immutable(&self) -> bool {
        self.reference.digest().is_some()
    }

    /// Returns the transport associated with the reference.
    pub fn transport(&self) -> RegistryTransport {
        self.transport
    }

    /// Returns the registry host name.
    pub fn registry(&self) -> &str {
        self.reference.registry()
    }

    /// Returns the repository path.
    pub fn repository(&self) -> &str {
        self.reference.repository()
    }

    /// Returns the underlying OCI reference.
    pub fn as_oci_reference(&self) -> &Reference {
        &self.reference
    }

    /// Returns a copy with the provided digest.
    pub fn with_digest(&self, digest: &str) -> Result<Self> {
        validate_sha256(digest)?;
        Ok(Self {
            transport: self.transport,
            reference: Reference::with_digest(
                self.reference.registry().to_owned(),
                self.reference.repository().to_owned(),
                digest.to_owned(),
            ),
        })
    }

    /// Converts back to a container source, preserving transport.
    pub fn to_container_source(&self) -> ContainerSource {
        match self.transport {
            RegistryTransport::Docker => ContainerSource::Docker(self.reference.to_string()),
            RegistryTransport::Oras => ContainerSource::Oras(self.reference.to_string()),
        }
    }
}

impl fmt::Display for RegistryReference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.canonical())
    }
}

/// Validates a sha256 digest string.
pub(super) fn validate_sha256(digest: &str) -> Result<()> {
    let Some(hex) = digest.strip_prefix("sha256:") else {
        bail!("expected a sha256 digest, found `{digest}`");
    };
    anyhow::ensure!(
        hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit()),
        "invalid sha256 digest `{digest}`"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v1::requirements::ContainerSource;

    #[test]
    fn canonicalizes_docker_hub_shorthand() {
        let reference =
            RegistryReference::try_from_source(&ContainerSource::Docker("ubuntu".into())).unwrap();
        assert_eq!(
            reference.canonical(),
            "docker://docker.io/library/ubuntu:latest"
        );
    }

    #[test]
    fn preserves_registry_ports_and_tag_case() {
        let reference = RegistryReference::try_from_source(&ContainerSource::Docker(
            "localhost:5000/team/tool:RC1".into(),
        ))
        .unwrap();
        assert_eq!(
            reference.canonical(),
            "docker://localhost:5000/team/tool:RC1"
        );
    }

    #[test]
    fn keeps_oras_transport_distinct() {
        let reference = RegistryReference::try_from_source(&ContainerSource::Oras(
            "ghcr.io/org/tool:v1".into(),
        ))
        .unwrap();
        assert_eq!(reference.canonical(), "oras://ghcr.io/org/tool:v1");
    }

    #[test]
    fn replaces_tag_with_sha256_digest() {
        let reference =
            RegistryReference::try_from_source(&ContainerSource::Docker("ubuntu:24.04".into()))
                .unwrap();
        let digest = format!("sha256:{}", "a".repeat(64));
        let pinned = reference.with_digest(&digest).unwrap();
        assert_eq!(
            pinned.canonical(),
            format!("docker://docker.io/library/ubuntu@{digest}")
        );
        assert!(pinned.is_immutable());
    }

    #[test]
    fn rejects_non_sha256_digest() {
        let reference =
            RegistryReference::try_from_source(&ContainerSource::Docker("ubuntu:24.04".into()))
                .unwrap();
        let error = reference
            .with_digest(&format!("sha512:{}", "a".repeat(128)))
            .unwrap_err();
        assert!(error.to_string().contains("expected a sha256 digest"));
    }

    #[test]
    fn rejects_non_registry_sources() {
        let error = RegistryReference::try_from_source(&ContainerSource::Library(
            "library/alpine:latest".into(),
        ))
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("unsupported mutable container source")
        );
    }
}
