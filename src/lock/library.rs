//! Utilities for interacting with a [Library API](https://singularityhub.github.io/library-api)-compatible registry.

use std::collections::BTreeMap;

use anyhow::Context;
use anyhow::bail;
use axum::http::StatusCode;
use reqwest::Client;
use serde::Deserialize;
use wdl::engine::v1::requirements::ImageSource;
use wdl::engine::v1::requirements::LibraryApiSpec;

use crate::config::LockConfig;

/// Determine the architecture-specific digests for a [Library API](https://singularityhub.github.io/library-api) image.
pub async fn resolve_library_image(
    client: &Client,
    container_spec: &LibraryApiSpec,
    config: &LockConfig,
) -> anyhow::Result<BTreeMap<String, ImageSource>> {
    /// Mapping of Sylabs architecture variants ->
    /// [`std::env::consts::ARCH`] variants.
    const SUPPORTED_ARCHITECTURES: &[(&str, &str)] = &[("amd64", "x86_64"), ("arm64", "aarch64")];

    #[derive(Deserialize)]
    struct Response {
        data: Data,
    }

    #[derive(Deserialize)]
    struct Data {
        hash: String,
    }

    let Some(registry) = container_spec
        .registry()
        .or(config.library.registry.as_deref())
    else {
        bail!("image `{container_spec}` doesn't specify a registry");
    };

    let mut resolved_digests = BTreeMap::new();

    for (arch, normalized_arch) in SUPPORTED_ARCHITECTURES {
        let url = format!(
            "https://{registry}/v1/images/{entity}/{collection}/{container}?arch={arch}",
            entity = container_spec.entity(),
            collection = container_spec.collection(),
            container = container_spec.container(),
        );

        let request = client.get(&url).build()?;
        let response = client.execute(request).await?;

        match response.status() {
            StatusCode::OK => {
                let response = response
                    .json::<Response>()
                    .await
                    .context("failed to parse response")?;

                let mut container_spec = container_spec.clone();
                container_spec.set_container(format!(
                    "{}:{}",
                    container_spec.container(),
                    response.data.hash
                ));

                resolved_digests.insert(
                    normalized_arch.to_string(),
                    ImageSource::Library(container_spec),
                );
            }
            // Assume this means the specified arch isn't supported
            StatusCode::NOT_FOUND => {}
            _ => {
                bail!("failed to inspect image `{container_spec}` for arch `{arch}`");
            }
        }
    }

    if resolved_digests.is_empty() {
        bail!("`{container_spec}` was not found for any architecture on `{registry}`");
    }

    Ok(resolved_digests)
}
