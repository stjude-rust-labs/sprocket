//! Task container image utilities.

use std::collections::BTreeMap;
use std::sync::Arc;

use toml_spanner::Toml;

use crate::v1::requirements::ImageSource;

/// A map of container image overrides.
///
/// `<image> -> <override>`
///
/// This is used in Sprocket lockfiles to map mutable image tags (e.g.
/// `docker://ubuntu:latest`) to an immutable hash (e.g.
/// `docker://ubuntu@sha256:
/// c4a8d5503dfb2a3eb8ab5f807da5bc69a85730fb49b5cfca2330194ebcc41c7b`).
///
/// See also: [`ImageDigests`].
pub type ImageOverrideMap = BTreeMap<ImageSource, ImageDigests>;

/// The digest specification for a container image.
///
/// This bridges the gap between OCI-compliant and non-compliant registries with
/// regards to storing architecture-specific hashes.
///
/// See also: [`ImageOverrideMap`].
#[derive(Toml, Debug, Clone, PartialEq, Eq)]
#[toml(FromToml, ToToml, untagged)]
pub enum ImageDigests {
    /// (Possibly) multi-arch OCI manifest index.
    ///
    /// For OCI-compatible registries, all supported architectures can be
    /// identified under a single umbrella index hash. The runtime (e.g.,
    /// Docker) is responsible for determining the appropriate image for the
    /// host architecture based on that hash.
    OciManifest(ImageSource),
    /// Per-architecture image hashes.
    ///
    /// This is used for non-OCI registries (e.g., Sylabs Cloud Library) where
    /// each image is an entirely separate artifact, and is thus hashed
    /// separately.
    ///
    /// The keys of the map match [`std::env::consts::ARCH`] values.
    PerArch(BTreeMap<String, ImageSource>),
}

/// Controls the behavior when the requested image does not exist in the
/// [`ContainerImageOverrides`].
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub enum ImageOverrideResolutionFailureMode {
    /// Fail the execution.
    Error,
    /// Continue the execution with the image as specified.
    #[default]
    Continue,
}

/// A collection of container image overrides.
///
/// Before task execution, this collection will be checked for overrides.
///
/// This type is cheap to clone.
#[derive(Clone, Debug, Default)]
pub struct ContainerImageOverrides {
    /// How to handle resolution failures.
    failure_mode: ImageOverrideResolutionFailureMode,
    /// See [`ImageOverrideMap`].
    overrides: Arc<ImageOverrideMap>,
}

impl ContainerImageOverrides {
    /// Create a new `ContainerImageOverrides` with the given `overrides`.
    pub fn new(overrides: impl IntoIterator<Item = (ImageSource, ImageDigests)>) -> Self {
        Self {
            failure_mode: ImageOverrideResolutionFailureMode::default(),
            overrides: Arc::new(overrides.into_iter().collect()),
        }
    }

    /// Get the configured [`ImageOverrideResolutionFailureMode`].
    pub fn failure_mode(&self) -> ImageOverrideResolutionFailureMode {
        self.failure_mode
    }

    /// Set the [`ImageOverrideResolutionFailureMode`].
    pub fn set_failure_mode(&mut self, failure_mode: ImageOverrideResolutionFailureMode) {
        self.failure_mode = failure_mode;
    }

    /// Attempt to find `image` in the defined container image overrides.
    pub fn get(&self, image: &ImageSource) -> Option<&ImageDigests> {
        self.overrides.get(image)
    }
}
