//! Task container image utilities.

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fmt::Display;
use std::hash::Hash;
use std::hash::Hasher;
use std::ops::Range;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

use serde::Deserialize;
use serde::Serialize;
use toml_spanner::Arena;
use toml_spanner::Context;
use toml_spanner::Failed;
use toml_spanner::FromToml;
use toml_spanner::Item;
use toml_spanner::ToToml;
use toml_spanner::ToTomlError;
use toml_spanner::Toml;

/// The Docker registry protocol prefix.
pub(crate) const DOCKER_PROTOCOL: &str = "docker://";
/// The [Library API](https://singularityhub.github.io/library-api) protocol prefix.
const LIBRARY_PROTOCOL: &str = "library://";
/// The OCI Registry as Storage protocol prefix.
const ORAS_PROTOCOL: &str = "oras://";
/// The file protocol prefix for local image files.
const FILE_PROTOCOL: &str = "file://";

/// The expected extension for local SIF files.
const SIF_EXTENSION: &str = "sif";

/// A parsed [Library API](https://singularityhub.github.io/library-api) URI (`library://`).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct LibraryApiSpec {
    /// The full library image spec.
    spec: String,

    /// The range in the `spec` representing the registry hosting the image.
    registry: Option<Range<usize>>,
    /// The range in the `spec` representing the entity (user) responsible for
    /// the image.
    entity: Option<Range<usize>>,
    /// The range in the `spec` representing the collection name.
    collection: Option<Range<usize>>,
    /// The start of the container image name.
    container_start: usize,
}

impl Eq for LibraryApiSpec {}

impl PartialEq for LibraryApiSpec {
    fn eq(&self, other: &Self) -> bool {
        self.spec == other.spec
    }
}

impl Ord for LibraryApiSpec {
    fn cmp(&self, other: &Self) -> Ordering {
        self.spec.cmp(&other.spec)
    }
}

impl PartialOrd for LibraryApiSpec {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Hash for LibraryApiSpec {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.spec.hash(state);
    }
}

impl LibraryApiSpec {
    /// The registry hosting the image.
    pub fn registry(&self) -> Option<&str> {
        self.registry.clone().map(|range| &self.spec[range])
    }

    /// The entity (user) responsible for the image.
    pub fn entity(&self) -> &str {
        self.entity
            .clone()
            .map(|range| &self.spec[range])
            .unwrap_or("library")
    }

    /// The collection that the image belongs to.
    pub fn collection(&self) -> &str {
        self.collection
            .clone()
            .map(|range| &self.spec[range])
            .unwrap_or("default")
    }

    /// The registry hosting the image.
    pub fn container(&self) -> &str {
        &self.spec[self.container_start..]
    }

    /// Change the container image name.
    pub fn set_container(&mut self, container: impl AsRef<str>) {
        self.spec
            .replace_range(self.container_start.., container.as_ref());
    }
}

impl FromStr for LibraryApiSpec {
    type Err = ();

    fn from_str(s: &str) -> anyhow::Result<Self, Self::Err> {
        const SEPARATOR: char = '/';

        fn segment(offset: &mut usize, s: &str) -> Range<usize> {
            let range = *offset..*offset + s.len();
            *offset += s.len() + SEPARATOR.len_utf8();

            range
        }

        // library://library.sylabs.io/library/default/ubuntu:latest
        //           ----------------- ------- ------- -------------
        //                   |            |       |          |
        //                registry      entity collection   image

        let mut parts = s.split(SEPARATOR).collect::<Vec<_>>();

        let mut offset = 0;
        let registry = if parts.len() > 1 && parts[0].contains('.') {
            Some(segment(&mut offset, parts.remove(0)))
        } else {
            None
        };

        let (entity, collection, container_start) = match parts.len() {
            // library://ubuntu:latest
            1 => (None, None, offset),
            // library://entity/ubuntu:latest
            2 => {
                let entity = segment(&mut offset, parts[0]);

                (Some(entity), None, offset)
            }
            // library://library/default/ubuntu:latest
            3 => {
                let entity = segment(&mut offset, parts[0]);
                let collection = segment(&mut offset, parts[1]);

                (Some(entity), Some(collection), offset)
            }
            _ => return Err(()),
        };

        Ok(Self {
            spec: s.to_string(),

            registry,
            entity,
            collection,
            container_start,
        })
    }
}

impl Display for LibraryApiSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.spec)
    }
}

/// Represents the source of a container image.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageSource {
    /// A Docker registry image (e.g. `docker://ubuntu:22.04`).
    Docker(String),
    /// A [Library API](https://singularityhub.github.io/library-api) image (e.g., `library://sylabs/default/alpine`).
    Library(LibraryApiSpec),
    /// An OCI Registry as Storage image (e.g., `oras://ghcr.io/org/image`).
    Oras(String),
    /// A local SIF file (e.g., `file:///path/to/image.sif`).
    SifFile(PathBuf),
    /// An unknown image source that could not be parsed.
    Unknown(String),
}

impl ToToml for ImageSource {
    fn to_toml<'a>(&'a self, arena: &'a Arena) -> std::result::Result<Item<'a>, ToTomlError> {
        let formatted = arena.alloc_str(&format!("{:#}", self));
        formatted.to_toml(arena)
    }
}

impl<'de> FromToml<'de> for ImageSource {
    fn from_toml(ctx: &mut Context<'de>, item: &Item<'de>) -> std::result::Result<Self, Failed> {
        let s = String::from_toml(ctx, item)?;
        // SAFETY: `FromStr` for `ImageSource` is infallible.
        Ok(s.parse().unwrap())
    }
}

impl ImageSource {
    /// Gets the scheme of the image source.
    ///
    /// Returns `None` for unknown image sources.
    pub fn scheme(&self) -> Option<&'static str> {
        match self {
            Self::Docker(_) => Some("docker"),
            Self::Library(_) => Some("library"),
            Self::Oras(_) => Some("oras"),
            Self::SifFile(_) => Some("file"),
            Self::Unknown(_) => None,
        }
    }

    /// Gets the display name of the image source.
    ///
    /// Returns `None` if the source is a file or an unknown source.
    pub fn name(&self) -> Option<&str> {
        match self {
            Self::Library(spec) => Some(&spec.spec),
            Self::Docker(name) | Self::Oras(name) => Some(name),
            Self::SifFile(_) | Self::Unknown(_) => None,
        }
    }

    /// Gets the full URI ([`Self::scheme()`] + [`Self::name()`]) for display.
    ///
    /// This is used for user-facing events.
    pub fn uri(&self) -> String {
        match self {
            ImageSource::SifFile(path) => format!("file://{}", path.display()),
            ImageSource::Unknown(unknown) => unknown.clone(),
            ImageSource::Docker(_) | ImageSource::Oras(_) | ImageSource::Library(_) => {
                format!("{}://{}", self.scheme().unwrap(), self.name().unwrap())
            }
        }
    }
}

impl FromStr for ImageSource {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> anyhow::Result<Self, Self::Err> {
        // Check for `file://` protocol.
        if let Some(path_str) = s.strip_prefix(FILE_PROTOCOL) {
            let path = PathBuf::from(path_str);
            return match path.extension().and_then(|e| e.to_str()) {
                Some(ext) if ext == SIF_EXTENSION => Ok(Self::SifFile(path)),
                _ => Ok(Self::Unknown(s.to_string())),
            };
        }

        // Check for known registry protocols.
        if let Some(image) = s.strip_prefix(DOCKER_PROTOCOL) {
            return Ok(Self::Docker(image.to_string()));
        }
        if let Some(image) = s.strip_prefix(LIBRARY_PROTOCOL) {
            return LibraryApiSpec::from_str(image)
                .map(Self::Library)
                .or_else(|_| Ok(Self::Unknown(s.to_string())));
        }
        if let Some(image) = s.strip_prefix(ORAS_PROTOCOL) {
            return Ok(Self::Oras(image.to_string()));
        }

        // Check for unknown protocols.
        if s.contains("://") {
            return Ok(Self::Unknown(s.to_string()));
        }

        // No protocol assumes `docker://`.
        Ok(Self::Docker(s.to_string()))
    }
}

impl std::fmt::Display for ImageSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if f.alternate() {
            // Pretty format includes protocol prefix.
            match self {
                Self::Docker(s) => write!(f, "docker://{s}"),
                Self::Library(s) => write!(f, "library://{s}"),
                Self::Oras(s) => write!(f, "oras://{s}"),
                Self::SifFile(p) => write!(f, "file://{}", p.display()),
                Self::Unknown(s) => write!(f, "{s}"),
            }
        } else {
            // Normal format omits protocol prefix.
            match self {
                Self::Library(s) => write!(f, "{s}"),
                Self::Docker(s) | Self::Oras(s) | Self::Unknown(s) => {
                    write!(f, "{s}")
                }
                Self::SifFile(p) => write!(f, "{}", p.display()),
            }
        }
    }
}

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

impl FromIterator<(ImageSource, ImageDigests)> for ContainerImageOverrides {
    fn from_iter<I: IntoIterator<Item = (ImageSource, ImageDigests)>>(iter: I) -> Self {
        Self {
            failure_mode: ImageOverrideResolutionFailureMode::default(),
            overrides: Arc::new(iter.into_iter().collect()),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use toml_spanner::Toml;

    use crate::ImageDigests;
    use crate::ImageSource;

    #[test]
    fn parses_bare_docker_image() {
        let source: ImageSource = "ubuntu:22.04".parse().unwrap();
        assert_eq!(source, ImageSource::Docker("ubuntu:22.04".to_string()));
        assert_eq!(source.to_string(), "ubuntu:22.04");
        assert_eq!(format!("{source:#}"), "docker://ubuntu:22.04");
    }

    #[test]
    fn parses_docker_protocol() {
        let source: ImageSource = "docker://ubuntu:latest".parse().unwrap();
        assert_eq!(source, ImageSource::Docker("ubuntu:latest".to_string()));
        assert_eq!(source.to_string(), "ubuntu:latest");
        assert_eq!(format!("{source:#}"), "docker://ubuntu:latest");
    }

    #[test]
    fn parses_library_protocol() {
        for (spec, (registry, entity, collection, container)) in [
            // Image
            ("alpine:3.18", (None, "library", "default", "alpine:3.18")),
            // Image and registry
            (
                "library.sylabs.io/alpine:3.18",
                (
                    Some("library.sylabs.io"),
                    "library",
                    "default",
                    "alpine:3.18",
                ),
            ),
            // Entity and image
            (
                "sylabs/alpine:3.18",
                (None, "sylabs", "default", "alpine:3.18"),
            ),
            // Entity, image, and registry
            (
                "library.sylabs.io/sylabs/alpine:3.18",
                (
                    Some("library.sylabs.io"),
                    "sylabs",
                    "default",
                    "alpine:3.18",
                ),
            ),
            // Entity, collection, and image
            (
                "sylabs/collection/alpine:3.18",
                (None, "sylabs", "collection", "alpine:3.18"),
            ),
            // All components
            (
                "library.sylabs.io/sylabs/collection/alpine:3.18",
                (
                    Some("library.sylabs.io"),
                    "sylabs",
                    "collection",
                    "alpine:3.18",
                ),
            ),
        ] {
            let source: ImageSource = format!("library://{spec}").parse().unwrap();
            let ImageSource::Library(parsed_spec) = &source else {
                panic!("expected library source, found: {source:?}");
            };

            assert_eq!(parsed_spec.registry(), registry);
            assert_eq!(parsed_spec.entity(), entity);
            assert_eq!(parsed_spec.collection(), collection);
            assert_eq!(parsed_spec.container(), container);
            assert_eq!(parsed_spec.to_string(), spec);
            assert_eq!(format!("{source:#}"), format!("library://{spec}"));
        }
    }

    #[test]
    fn parses_oras_protocol() {
        let source: ImageSource = "oras://ghcr.io/org/image:tag".parse().unwrap();
        assert_eq!(
            source,
            ImageSource::Oras("ghcr.io/org/image:tag".to_string())
        );
        assert_eq!(source.to_string(), "ghcr.io/org/image:tag");
        assert_eq!(format!("{source:#}"), "oras://ghcr.io/org/image:tag");
    }

    #[test]
    fn parses_file_protocol_sif() {
        let source: ImageSource = "file:///path/to/image.sif".parse().unwrap();
        assert_eq!(
            source,
            ImageSource::SifFile(PathBuf::from("/path/to/image.sif"))
        );
        assert_eq!(source.to_string(), "/path/to/image.sif");
        assert_eq!(format!("{source:#}"), "file:///path/to/image.sif");
    }

    #[test]
    fn parses_file_protocol_unknown_extension() {
        let source: ImageSource = "file:///path/to/image.tar".parse().unwrap();
        assert_eq!(
            source,
            ImageSource::Unknown("file:///path/to/image.tar".to_string())
        );
        assert_eq!(source.to_string(), "file:///path/to/image.tar");
        assert_eq!(format!("{source:#}"), "file:///path/to/image.tar");
    }

    #[test]
    fn parses_unknown_protocol() {
        let source: ImageSource = "ftp://example.com/image".parse().unwrap();
        assert_eq!(
            source,
            ImageSource::Unknown("ftp://example.com/image".to_string())
        );
        assert_eq!(source.to_string(), "ftp://example.com/image");
        assert_eq!(format!("{source:#}"), "ftp://example.com/image");
    }

    #[test]
    fn parses_complex_docker_image() {
        let source: ImageSource = "ghcr.io/stjude/sprocket:v1.0.0".parse().unwrap();
        assert_eq!(
            source,
            ImageSource::Docker("ghcr.io/stjude/sprocket:v1.0.0".to_string())
        );
    }

    #[test]
    fn parses_docker_image_with_digest() {
        let source: ImageSource = "ubuntu@sha256:abcdef1234567890".parse().unwrap();
        assert_eq!(
            source,
            ImageSource::Docker("ubuntu@sha256:abcdef1234567890".to_string())
        );
    }

    #[test]
    fn test_image_source_from_toml() {
        #[derive(Toml, PartialEq, Eq, Debug)]
        #[toml(FromToml)]
        struct S {
            img: ImageSource,
        }

        let docker_bare: S = toml_spanner::from_str(r#"img = "ubuntu:22.04""#).unwrap();
        assert_eq!(
            docker_bare.img,
            ImageSource::Docker("ubuntu:22.04".to_string())
        );

        let docker_protocol: S =
            toml_spanner::from_str(r#"img = "docker://ubuntu:latest""#).unwrap();
        assert_eq!(
            docker_protocol.img,
            ImageSource::Docker("ubuntu:latest".to_string())
        );

        let oras: S = toml_spanner::from_str(r#"img = "oras://ghcr.io/org/image:tag""#).unwrap();
        assert_eq!(
            oras.img,
            ImageSource::Oras("ghcr.io/org/image:tag".to_string())
        );

        let library: S =
            toml_spanner::from_str(r#"img = "library://sylabs/default/alpine:3.18""#).unwrap();
        let ImageSource::Library(spec) = &library.img else {
            panic!("expected library source, found: {:?}", library.img);
        };

        assert_eq!(spec.registry(), None);
        assert_eq!(spec.entity(), "sylabs");
        assert_eq!(spec.collection(), "default");
        assert_eq!(spec.container(), "alpine:3.18");
    }

    #[test]
    fn test_image_digests_from_toml() {
        #[derive(Toml, PartialEq, Eq, Debug)]
        #[toml(FromToml)]
        struct S {
            img: ImageDigests,
        }

        let oci: S = toml_spanner::from_str(
            r#"img = "docker://ubuntu@sha256:fd7fe639db24c4e005643921beea92bc449aac4f4d40d60cd9ad9ab6456aec01""#,
        )
            .unwrap();
        assert_eq!(
            oci.img,
            ImageDigests::OciManifest(ImageSource::Docker(
                "ubuntu@sha256:fd7fe639db24c4e005643921beea92bc449aac4f4d40d60cd9ad9ab6456aec01"
                    .to_string()
            ))
        );
    }
}
