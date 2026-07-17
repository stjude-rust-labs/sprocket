use std::collections::BTreeMap;

use anyhow::Context as _;
use anyhow::Result;
use async_trait::async_trait;
use docker_credential::CredentialRetrievalError;
use docker_credential::DockerCredential;
use oci_client::secrets::RegistryAuth;
use wdl::engine::container_lock::RegistryReference;

/// Retrieves the top-level OCI manifest or image-index digest.
#[async_trait]
pub(crate) trait ManifestDigestClient: Send + Sync {
    /// Fetches the digest for a registry reference.
    async fn fetch_digest(
        &self,
        reference: &RegistryReference,
        auth: &RegistryAuth,
    ) -> Result<String>;
}

/// Retrieves registry credentials.
#[async_trait]
pub(crate) trait CredentialProvider: Send + Sync {
    /// Gets authentication for a registry reference.
    async fn auth(&self, reference: &RegistryReference) -> Result<RegistryAuth>;
}

/// OCI Distribution client used to fetch manifest and image-index digests.
pub(crate) struct OciManifestDigestClient {
    client: oci_client::Client,
}

#[async_trait]
impl ManifestDigestClient for OciManifestDigestClient {
    async fn fetch_digest(
        &self,
        reference: &RegistryReference,
        auth: &RegistryAuth,
    ) -> Result<String> {
        self.client
            .fetch_manifest_digest(reference.as_oci_reference(), auth)
            .await
            .with_context(|| format!("failed to resolve manifest for `{reference}`"))
    }
}

/// Docker configuration and credential-helper credential provider.
pub(crate) struct DockerCredentialProvider;

#[async_trait]
impl CredentialProvider for DockerCredentialProvider {
    async fn auth(&self, reference: &RegistryReference) -> Result<RegistryAuth> {
        let server = reference.as_oci_reference().resolve_registry().to_string();
        tokio::task::spawn_blocking(move || docker_auth(&server))
            .await
            .context("Docker credential lookup task failed")?
    }
}

fn docker_auth(server: &str) -> Result<RegistryAuth> {
    docker_auth_from_result(server, docker_credential::get_credential(server))
}

fn docker_auth_from_result(
    server: &str,
    credential: std::result::Result<DockerCredential, CredentialRetrievalError>,
) -> Result<RegistryAuth> {
    match credential {
        Ok(DockerCredential::UsernamePassword(username, password)) => {
            Ok(RegistryAuth::Basic(username, password))
        }
        Ok(DockerCredential::IdentityToken(token)) => Ok(RegistryAuth::Bearer(token)),
        Err(CredentialRetrievalError::ConfigNotFound)
        | Err(CredentialRetrievalError::NoCredentialConfigured) => Ok(RegistryAuth::Anonymous),
        Err(error) => Err(anyhow::anyhow!(
            "failed to retrieve Docker credentials for registry `{server}` ({kind})",
            kind = match error {
                CredentialRetrievalError::HelperCommunicationError => "helper communication failed",
                CredentialRetrievalError::MalformedHelperResponse => {
                    "helper response was malformed"
                }
                CredentialRetrievalError::HelperFailure { .. } => "credential helper failed",
                CredentialRetrievalError::CredentialDecodingError => "credential decoding failed",
                CredentialRetrievalError::CredentialMismatchError => "credential fields disagree",
                CredentialRetrievalError::ConfigReadError => "Docker config could not be read",
                CredentialRetrievalError::ConfigNotFound
                | CredentialRetrievalError::NoCredentialConfigured => unreachable!(),
            }
        )),
    }
}

/// Resolves mutable registry references to immutable digest references.
pub(crate) struct RegistryResolver<C = OciManifestDigestClient, A = DockerCredentialProvider> {
    client: C,
    credentials: A,
}

impl Default for RegistryResolver {
    fn default() -> Self {
        Self {
            client: OciManifestDigestClient {
                client: oci_client::Client::default(),
            },
            credentials: DockerCredentialProvider,
        }
    }
}

impl<C, A> RegistryResolver<C, A>
where
    C: ManifestDigestClient,
    A: CredentialProvider,
{
    /// Resolves one mutable reference to its top-level manifest digest.
    pub(crate) async fn resolve(&self, reference: &RegistryReference) -> Result<RegistryReference> {
        let auth = self.credentials.auth(reference).await?;
        let digest = self.client.fetch_digest(reference, &auth).await?;
        reference.with_digest(&digest)
    }

    /// Resolves canonical references once, with at most eight concurrent requests.
    pub(crate) async fn resolve_all(
        &self,
        references: impl IntoIterator<Item = RegistryReference>,
    ) -> Result<BTreeMap<String, String>> {
        use futures::StreamExt as _;
        use futures::TryStreamExt as _;

        let unique = references
            .into_iter()
            .map(|reference| (reference.canonical(), reference))
            .collect::<BTreeMap<_, _>>();
        futures::stream::iter(unique.into_values())
            .map(|reference| async move {
                let pinned = self.resolve(&reference).await?;
                Ok::<_, anyhow::Error>((reference.canonical(), pinned.canonical()))
            })
            .buffer_unordered(8)
            .try_collect()
            .await
    }
}

/// Resolves mutable registry references for lock generation.
#[async_trait]
pub(crate) trait ResolveRegistryReferences: Send + Sync {
    /// Resolves mutable references to their immutable digest pins.
    async fn resolve_all(
        &self,
        references: Vec<RegistryReference>,
    ) -> Result<BTreeMap<String, String>>;
}

#[async_trait]
impl<C, A> ResolveRegistryReferences for RegistryResolver<C, A>
where
    C: ManifestDigestClient,
    A: CredentialProvider,
{
    async fn resolve_all(
        &self,
        references: Vec<RegistryReference>,
    ) -> Result<BTreeMap<String, String>> {
        Self::resolve_all(self, references).await
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering;

    use anyhow::Result;
    use async_trait::async_trait;
    use docker_credential::CredentialRetrievalError;
    use oci_client::secrets::RegistryAuth;
    use wdl::engine::container_lock::RegistryReference;
    use wdl::engine::v1::requirements::ContainerSource;

    use super::CredentialProvider;
    use super::ManifestDigestClient;
    use super::RegistryResolver;
    use super::docker_auth_from_result;

    #[derive(Clone)]
    struct FakeDigestClient {
        digest: String,
        calls: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl ManifestDigestClient for FakeDigestClient {
        async fn fetch_digest(&self, _: &RegistryReference, _: &RegistryAuth) -> Result<String> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(self.digest.clone())
        }
    }

    struct AnonymousCredentials;

    #[async_trait]
    impl CredentialProvider for AnonymousCredentials {
        async fn auth(&self, _: &RegistryReference) -> Result<RegistryAuth> {
            Ok(RegistryAuth::Anonymous)
        }
    }

    #[tokio::test]
    async fn resolves_top_level_digest_and_preserves_transport() {
        let resolver = RegistryResolver {
            client: FakeDigestClient {
                digest: format!("sha256:{}", "a".repeat(64)),
                calls: Arc::new(AtomicUsize::new(0)),
            },
            credentials: AnonymousCredentials,
        };
        let reference =
            RegistryReference::try_from_source(&ContainerSource::Docker("ubuntu:24.04".into()))
                .unwrap();

        let pinned = resolver.resolve(&reference).await.unwrap();

        assert_eq!(
            pinned.canonical(),
            format!(
                "docker://docker.io/library/ubuntu@sha256:{}",
                "a".repeat(64)
            )
        );
    }

    #[tokio::test]
    async fn deduplicates_canonical_references() {
        let calls = Arc::new(AtomicUsize::new(0));
        let resolver = RegistryResolver {
            client: FakeDigestClient {
                digest: format!("sha256:{}", "a".repeat(64)),
                calls: calls.clone(),
            },
            credentials: AnonymousCredentials,
        };
        let references = ["ubuntu:latest", "docker.io/library/ubuntu:latest"]
            .into_iter()
            .map(|value| {
                RegistryReference::try_from_source(&ContainerSource::Docker(value.into())).unwrap()
            });

        resolver.resolve_all(references).await.unwrap();

        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn missing_docker_config_uses_anonymous_auth() {
        let auth =
            docker_auth_from_result("docker.io", Err(CredentialRetrievalError::ConfigNotFound))
                .unwrap();

        assert_eq!(auth, RegistryAuth::Anonymous);
    }

    #[test]
    fn configured_helper_failure_is_not_silenced() {
        let error = docker_auth_from_result(
            "registry.example",
            Err(CredentialRetrievalError::HelperFailure {
                helper: "secretservice".into(),
                stdout: "secret stdout".into(),
                stderr: "secret stderr".into(),
            }),
        )
        .unwrap_err()
        .to_string();

        assert!(error.contains("credential helper failed"));
        assert!(!error.contains("secret stdout"));
        assert!(!error.contains("secret stderr"));
    }

    #[tokio::test]
    async fn reads_the_top_level_digest_from_a_registry() {
        use axum::Router;
        use axum::http::HeaderValue;
        use axum::routing::head;
        use oci_client::Client;
        use oci_client::Reference;
        use oci_client::client::ClientConfig;
        use oci_client::client::ClientProtocol;

        let digest = format!("sha256:{}", "b".repeat(64));
        let response_digest = digest.clone();
        let app = Router::new().route(
            "/v2/team/tool/manifests/v1",
            head(move || {
                let response_digest = response_digest.clone();
                async move {
                    (
                        [(
                            "docker-content-digest",
                            HeaderValue::from_str(&response_digest).unwrap(),
                        )],
                        "",
                    )
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let client = Client::new(ClientConfig {
            protocol: ClientProtocol::Http,
            ..Default::default()
        });
        let reference: Reference = format!("{address}/team/tool:v1").parse().unwrap();
        let actual = client
            .fetch_manifest_digest(&reference, &RegistryAuth::Anonymous)
            .await
            .unwrap();

        assert_eq!(actual, digest);
        server.abort();
    }
}
