//! Utilities for interacting with the Sylabs cloud registry.

use std::collections::BTreeMap;

use anyhow::Context;
use anyhow::bail;
use axum::http::StatusCode;
use reqwest::Client;
use serde::Deserialize;
use wdl::engine::v1::requirements::ImageSource;

/// Determine the architecture-specific digests for a Sylabs Cloud image.
pub async fn resolve_sylabs_image(
    client: &Client,
    image: &str,
    tag: &str,
) -> anyhow::Result<BTreeMap<String, ImageSource>> {
    /// Mapping of Sylabs architecture variants ->
    /// [`std::env::consts::ARCH`] variants.
    const SUPPORTED_ARCHITECTURES: &[(&str, &str)] = &[
        ("amd64", "x86_64"),
        ("arm64", "aarch64"),
        ("arm", "arm"),
        ("ppc64le", "ppc64le"),
        ("s390x", "s390x"),
        ("386", "x86"),
    ];

    #[derive(Deserialize)]
    struct Response {
        data: Data,
    }

    #[derive(Deserialize)]
    struct Data {
        hash: String,
    }

    // library://library.sylabs.io/library/default/ubuntu:latest
    //           ----------------- ------- ------- -------------
    //                   |            |       |          |
    //                registry      entity collection   image

    let mut parts = image.split('/').collect::<Vec<_>>();

    let registry = if !parts.is_empty() && (parts[0].contains('.') || parts[0].contains(':')) {
        parts.remove(0)
    } else {
        "library.sylabs.io"
    };

    let (entity, collection, container) = match parts.len() {
        // library://ubuntu:latest
        1 => ("library", "default", String::from(parts[0])),
        // library://entity/ubuntu:latest
        2 => (parts[0], "default", String::from(parts[1])),
        // library://library/default/ubuntu:latest
        3 => (parts[0], parts[1], String::from(parts[2])),
        _ => bail!("invalid image format"),
    };

    let mut resolved_digests = BTreeMap::new();

    for (arch, normalized_arch) in SUPPORTED_ARCHITECTURES {
        let url = format!(
            "https://{registry}/v1/images/{entity}/{collection}/{container}:{tag}?arch={arch}",
        );

        let request = client.get(&url).build()?;
        let response = client.execute(request).await?;

        match response.status() {
            StatusCode::OK => {
                let response = response
                    .json::<Response>()
                    .await
                    .context("failed to parse response")?;
                resolved_digests.insert(
                    normalized_arch.to_string(),
                    ImageSource::Library(format!("{image}:{}", response.data.hash)),
                );
            }
            // Assume this means the specified arch isn't supported
            StatusCode::NOT_FOUND => {}
            _ => {
                bail!("failed to inspect image `{image}:{tag}` for arch `{arch}`");
            }
        }
    }

    if resolved_digests.is_empty() {
        bail!("`{image}:{tag}` was not found for any architecture on `{registry}`");
    }

    Ok(resolved_digests)
}
