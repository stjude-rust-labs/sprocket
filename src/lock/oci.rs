//! Utilities for interacting with OCI-compatible registries.

use std::collections::HashMap;
use std::env::VarError;
use std::fmt::Debug;
use std::fmt::Formatter;
use std::io::ErrorKind;
use std::ops::Deref;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::ensure;
use docker_credential::CredentialRetrievalError;
use docker_credential::DockerCredential;
use oci_client::Reference;
use oci_client::secrets::RegistryAuth;
use wdl::engine::v1::requirements::ImageSource;

/// The docker auth config.
#[derive(Debug)]
struct AuthConfig(String);

impl AuthConfig {
    /// Attempt to load the Docker auth config, if one exists.
    ///
    /// This tries to following locations, in order:
    ///
    /// 1. `$DOCKER_AUTH_CONFIG/config.json`
    /// 2. `$DOCKER_CONFIG/config.json`
    /// 3. `$HOME/.docker/config.json`
    fn load() -> anyhow::Result<Option<Self>> {
        let config_dir = match std::env::var("DOCKER_AUTH_CONFIG")
            .or_else(|_| std::env::var("DOCKER_CONFIG"))
        {
            Ok(env_path) => Some(PathBuf::from(env_path)),
            Err(VarError::NotPresent) => dirs::home_dir().map(|home_dir| home_dir.join(".docker")),
            Err(e) => return Err(e.into()),
        };

        let Some(config_dir) = config_dir else {
            tracing::trace!("no Docker auth config found");
            return Ok(None);
        };

        let config_path = config_dir.join("config.json");
        match std::fs::read_to_string(&config_path) {
            Ok(config) => {
                tracing::debug!("using Docker auth config: {}", config_path.display());
                Ok(Some(Self(config)))
            }
            Err(e) if e.kind() == ErrorKind::NotFound => {
                tracing::trace!("no Docker auth config found");
                Ok(None)
            }
            Err(e) => Err(e.into()),
        }
    }
}

/// A client for OCI-compatible registries.
pub struct OciClient {
    /// The actual client.
    client: oci_client::Client,
    /// The Docker auth config.
    auth_config: Option<AuthConfig>,
    /// Per-server credentials
    credentials: HashMap<String, Arc<RegistryAuth>>,
}

impl Debug for OciClient {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OciClient")
            .field("auth_config", &self.auth_config)
            .field("credentials", &self.credentials)
            .finish_non_exhaustive()
    }
}

impl OciClient {
    /// Create a new `OciClient`.
    pub fn new() -> anyhow::Result<Self> {
        let auth_config = AuthConfig::load()?;

        let config = oci_client::client::ClientConfig {
            user_agent: super::USER_AGENT,
            ..Default::default()
        };

        Ok(Self {
            client: oci_client::Client::new(config),
            auth_config,
            credentials: HashMap::new(),
        })
    }

    /// Get the auth config for the given registry.
    fn fetch_auth(&mut self, server: &str) -> anyhow::Result<Arc<RegistryAuth>> {
        let Some(auth_config) = self.auth_config.as_ref() else {
            return Ok(Arc::new(RegistryAuth::Anonymous));
        };

        if let Some(auth) = self.credentials.get(server) {
            return Ok(auth.clone());
        }

        let credentials = match docker_credential::get_credential_from_reader(
            &mut auth_config.0.as_bytes(),
            server,
        ) {
            Ok(DockerCredential::UsernamePassword(username, password)) => {
                Ok(RegistryAuth::Basic(username, password))
            }
            Ok(DockerCredential::IdentityToken(token)) => Ok(RegistryAuth::Bearer(token)),
            Err(CredentialRetrievalError::ConfigNotFound)
            | Err(CredentialRetrievalError::NoCredentialConfigured) => Ok(RegistryAuth::Anonymous),
            Err(error) => Err(anyhow::anyhow!(
                "failed to retrieve Docker credentials for registry `{server}` ({kind})",
                kind = match error {
                    CredentialRetrievalError::HelperCommunicationError =>
                        "helper communication failed",
                    CredentialRetrievalError::MalformedHelperResponse => {
                        "helper response was malformed"
                    }
                    CredentialRetrievalError::HelperFailure { .. } => "credential helper failed",
                    CredentialRetrievalError::CredentialDecodingError =>
                        "credential decoding failed",
                    CredentialRetrievalError::CredentialMismatchError =>
                        "credential fields disagree",
                    CredentialRetrievalError::ConfigReadError => "Docker config could not be read",
                    CredentialRetrievalError::ConfigNotFound
                    | CredentialRetrievalError::NoCredentialConfigured => unreachable!(),
                }
            )),
        }?;

        self.credentials
            .insert(server.to_string(), Arc::new(credentials));
        Ok(self.credentials.get(server).cloned().unwrap())
    }
}

impl Deref for OciClient {
    type Target = oci_client::Client;

    fn deref(&self) -> &Self::Target {
        &self.client
    }
}

/// The source of an [`OciImage`].
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum OciImageSource {
    /// A Docker registry image.
    Docker,
    /// An OCI Registry as Storage image.
    Oras,
}

/// An image from an OCI-compatible source.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct OciImage<'a> {
    /// The source of the image.
    source: OciImageSource,
    /// The image name.
    name: &'a str,
}

impl<'a> TryFrom<&'a ImageSource> for OciImage<'a> {
    type Error = ();

    fn try_from(value: &'a ImageSource) -> Result<Self, Self::Error> {
        match value {
            ImageSource::Docker(image) => Ok(OciImage {
                source: OciImageSource::Docker,
                name: image,
            }),
            ImageSource::Oras(image) => Ok(OciImage {
                source: OciImageSource::Oras,
                name: image,
            }),
            _ => Err(()),
        }
    }
}

/// Resolve an image from an OCI-compatible registry
pub async fn resolve_oci_image(
    client: &mut OciClient,
    oci_image: OciImage<'_>,
) -> anyhow::Result<ImageSource> {
    fn verify_digest(digest: &str) -> bool {
        let Some(digest) = digest.strip_prefix("sha256:") else {
            return false;
        };

        digest.len() == 64 && digest.chars().all(|c| c.is_ascii_alphanumeric())
    }

    let reference: Reference = oci_image.name.parse()?;

    let server = reference.resolve_registry();
    let auth = client.fetch_auth(server)?;

    let digest = client.fetch_manifest_digest(&reference, &auth).await?;
    ensure!(
        verify_digest(&digest),
        "registry returned an invalid digest"
    );

    let locked = format!("{}@{digest}", oci_image.name);
    Ok(match oci_image.source {
        OciImageSource::Docker => ImageSource::Docker(locked),
        OciImageSource::Oras => ImageSource::Oras(locked),
    })
}
