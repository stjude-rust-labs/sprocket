//! Utilities for working with `sprocket` manifest files.

use std::collections::BTreeMap;

use anyhow::Context;
use bytes::Bytes;
use serde::Deserialize;
use serde::Serialize;
use url::Url;

use crate::SPROCKET_UPDATE_ROOT;
use crate::dirs::current_profile_dir;
use crate::downloads::Downloader;

/// A `sprocket` component.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Component {
    /// The source URL to download the component from.
    pub(crate) src: Url,
    /// The sha256 hash of the component.
    pub(crate) hash: String,
}

// TODO: Should probably be versioned.
/// A `sprocket` version manifest.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Manifest {
    /// A map of component names to their target-specific variants.
    #[serde(rename = "component")]
    components: BTreeMap<String, TargetedComponents>,
    /// A map of profile names to [`Profile`]s.
    profiles: BTreeMap<String, Profile>,
}

/// A [target triple] string.
///
/// [target triple]: https://doc.rust-lang.org/cargo/appendix/glossary.html#target
type TargetTriple = String;

/// A map of targets to their specific versions of sprocket components.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct TargetedComponents(BTreeMap<TargetTriple, Component>);

/// A profile definition.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(transparent)]
pub(crate) struct Profile(Vec<String>);

impl Profile {
    /// Get all components in this profile.
    pub(crate) fn components(&self) -> &[String] {
        self.0.as_slice()
    }
}

/// The name of the manifest file.
pub(crate) const MANIFEST_FILE_NAME: &str = "manifest.toml";

impl Manifest {
    /// Download and parse the latest `sprocket` version manifest.
    pub(crate) async fn latest() -> anyhow::Result<(Bytes, Self)> {
        let manifest_url = Url::parse(SPROCKET_UPDATE_ROOT)
            .and_then(|base| base.join(MANIFEST_FILE_NAME))
            .context("failed to create manifest URL")?;
        let (_, manifest) = Downloader::new().download(manifest_url).start().await?;

        let parsed =
            toml::from_slice(&manifest).context("failed to deserialize manifest content")?;
        Ok((manifest, parsed))
    }

    /// Load the manifest from the current profile.
    pub(crate) fn load() -> anyhow::Result<Self> {
        let manifest_path = current_profile_dir()?.join(MANIFEST_FILE_NAME);
        let content = std::fs::read_to_string(&manifest_path)?;

        toml::from_str(&content).context("failed to deserialize manifest content")
    }

    /// Search for a component for the host target.
    pub(crate) fn component(&self, name: &str) -> Option<&Component> {
        self.components
            .get(name)
            .and_then(|targets| targets.0.get(env!("TARGET")))
    }

    /// Get a list of all available component names.
    pub(crate) fn components(&self) -> impl Iterator<Item = &str> {
        self.components.keys().map(String::as_str)
    }

    /// Search for a profile.
    pub(crate) fn profile(&self, name: &str) -> Option<&Profile> {
        self.profiles.get(name)
    }
}
