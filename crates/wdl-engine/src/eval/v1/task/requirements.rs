//! Implementation of utility functions for reading task requirements.

use std::borrow::Cow;
use std::collections::HashMap;
use std::path::PathBuf;
use std::str::FromStr;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use tracing::warn;
use wdl_analysis::types::PrimitiveType;
use wdl_ast::v1::TASK_HINT_DISKS;
use wdl_ast::v1::TASK_HINT_GPU;
use wdl_ast::v1::TASK_REQUIREMENT_CONTAINER;
use wdl_ast::v1::TASK_REQUIREMENT_CONTAINER_ALIAS;
use wdl_ast::v1::TASK_REQUIREMENT_CPU;
use wdl_ast::v1::TASK_REQUIREMENT_DISKS;
use wdl_ast::v1::TASK_REQUIREMENT_GPU;
use wdl_ast::v1::TASK_REQUIREMENT_MAX_RETRIES;
use wdl_ast::v1::TASK_REQUIREMENT_MAX_RETRIES_ALIAS;
use wdl_ast::v1::TASK_REQUIREMENT_MEMORY;

use crate::Coercible;
use crate::ONE_GIBIBYTE;
use crate::TaskInputs;
use crate::Value;
use crate::config::Config;
use crate::units::StorageUnit;
use crate::v1::DEFAULT_DISK_MOUNT_POINT;
use crate::v1::task::DEFAULT_GPU_COUNT;
use crate::v1::task::DEFAULT_TASK_REQUIREMENT_CONTAINER;
use crate::v1::task::DEFAULT_TASK_REQUIREMENT_CPU;
use crate::v1::task::DEFAULT_TASK_REQUIREMENT_MAX_RETRIES;
use crate::v1::task::DEFAULT_TASK_REQUIREMENT_MEMORY;
use crate::v1::task::lookup_entry;
use crate::v1::task::parse_storage_value;
use crate::v1::validators::SettingSource;
use crate::v1::validators::ensure_non_negative_i64;
use crate::v1::validators::invalid_numeric_value_message;

/// The Docker registry protocol prefix.
const DOCKER_PROTOCOL: &str = "docker://";

/// The Sylabs library protocol prefix.
const LIBRARY_PROTOCOL: &str = "library://";

/// The OCI Registry as Storage protocol prefix.
const ORAS_PROTOCOL: &str = "oras://";

/// The file protocol prefix for local container files.
const FILE_PROTOCOL: &str = "file://";

/// The expected extension for local SIF files.
const SIF_EXTENSION: &str = "sif";

/// Represents the source of a container image.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ContainerSource {
    /// A Docker registry image (e.g., `ubuntu:22.04` or
    /// `docker://ubuntu:22.04`).
    Docker(String),
    /// A Sylabs library image (e.g., `library://sylabs/default/alpine`).
    Library(String),
    /// An OCI Registry as Storage image (e.g., `oras://ghcr.io/org/image`).
    Oras(String),
    /// A local SIF file (e.g., `file:///path/to/image.sif`).
    SifFile(PathBuf),
    /// An unknown container source that could not be parsed.
    Unknown(String),
}

impl FromStr for ContainerSource {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
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
            return Ok(Self::Library(image.to_string()));
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

impl std::fmt::Display for ContainerSource {
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
                Self::Docker(s) | Self::Library(s) | Self::Oras(s) | Self::Unknown(s) => {
                    write!(f, "{s}")
                }
                Self::SifFile(p) => write!(f, "{}", p.display()),
            }
        }
    }
}

/// Gets the `container` requirement from a requirements map.
///
/// Returns a [`ContainerSource`] indicating whether the container is a
/// registry-based image or a local SIF file.
pub(crate) fn container(
    inputs: &TaskInputs,
    requirements: &HashMap<String, Value>,
    default: Option<&str>,
) -> ContainerSource {
    let value: Cow<'_, str> = lookup_entry(
        &[TASK_REQUIREMENT_CONTAINER, TASK_REQUIREMENT_CONTAINER_ALIAS],
        |key| inputs.requirement(key).or_else(|| requirements.get(key)),
    )
    .and_then(|(_, v)| -> Option<Cow<'_, str>> {
        // If the value is an array, use the first element or the default.
        // NOTE: in the future we should be resolving which element in the array is
        // usable; this will require some work in Crankshaft to enable.
        if let Some(array) = v.as_array() {
            return array.as_slice().first().map(|v| {
                v.as_string()
                    .expect("type should be string")
                    .as_ref()
                    .into()
            });
        }

        Some(
            v.coerce(None, &PrimitiveType::String.into())
                .expect("type should coerce")
                .unwrap_string()
                .as_ref()
                .clone()
                .into(),
        )
    })
    .and_then(|v| {
        // Treat `*` as the default.
        if v == "*" { None } else { Some(v) }
    })
    .unwrap_or_else(|| {
        default
            .map(Into::into)
            .unwrap_or(DEFAULT_TASK_REQUIREMENT_CONTAINER.into())
    });

    // SAFETY: `FromStr` for `ContainerSource` is infallible.
    value.parse().unwrap()
}

/// Gets the `cpu` requirement from a requirements map.
pub(crate) fn cpu(inputs: &TaskInputs, requirements: &HashMap<String, Value>) -> f64 {
    lookup_entry(&[TASK_REQUIREMENT_CPU], |key| {
        inputs.requirement(key).or_else(|| requirements.get(key))
    })
    .map(|(_, v)| {
        v.coerce(None, &PrimitiveType::Float.into())
            .expect("type should coerce")
            .unwrap_float()
    })
    .unwrap_or(DEFAULT_TASK_REQUIREMENT_CPU)
}

/// Gets the `memory` requirement from a requirements map.
pub(crate) fn memory(inputs: &TaskInputs, requirements: &HashMap<String, Value>) -> Result<i64> {
    if let Some((key, value)) = lookup_entry(&[TASK_REQUIREMENT_MEMORY], |key| {
        inputs.requirement(key).or_else(|| requirements.get(key))
    }) {
        let bytes = parse_storage_value(value, |raw| {
            invalid_numeric_value_message(SettingSource::Requirement, key, raw)
        })?;

        return ensure_non_negative_i64(SettingSource::Requirement, key, bytes);
    }

    Ok(DEFAULT_TASK_REQUIREMENT_MEMORY)
}

/// Gets the number of required GPUs from requirements and hints.
pub(crate) fn gpu(
    inputs: &TaskInputs,
    requirements: &HashMap<String, Value>,
    hints: &HashMap<String, Value>,
) -> Option<u64> {
    // If `requirements { gpu: false }` or there is no `gpu` requirement, return
    // `None`.
    let Some(true) = lookup_entry(&[TASK_REQUIREMENT_GPU], |key| {
        inputs.requirement(key).or_else(|| requirements.get(key))
    })
    .and_then(|(_, v)| v.as_boolean()) else {
        return None;
    };

    // If there is no `gpu` hint giving us more detail on the request, use the
    // default count.
    let Some((_, hint)) = lookup_entry(&[TASK_HINT_GPU], |key| {
        inputs.hint(key).or_else(|| hints.get(key))
    }) else {
        return Some(DEFAULT_GPU_COUNT);
    };

    // A string `gpu` hint is allowed by the spec, but we do not support them yet.
    //
    // TODO(clay): support string hints for GPU specifications.
    if let Some(hint) = hint.as_string() {
        warn!(
            %hint,
            "hint `{TASK_HINT_GPU}` cannot be a string: falling back to {DEFAULT_GPU_COUNT} GPU(s)"
        );
        return Some(DEFAULT_GPU_COUNT);
    }

    match hint.as_integer() {
        Some(count) if count >= 1 => Some(count as u64),
        // If the hint is zero or negative, it's not clear what the user intends. Maybe they have
        // tried to disable GPUs by setting the count to zero, or have made a logic error. Emit a
        // warning, and continue with no GPU request.
        Some(count) => {
            warn!(
                %count,
                "`{TASK_HINT_GPU}` hint specified {count} GPU(s); no GPUs will be requested for execution"
            );
            None
        }
        None => {
            // Typechecking should have already validated that the hint is an integer or
            // a string.
            unreachable!("`{TASK_HINT_GPU}` hint must be an integer or string")
        }
    }
}

/// Represents the type of a disk.
///
/// Disk types are specified via hints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[allow(clippy::upper_case_acronyms)]
pub(crate) enum DiskType {
    /// The disk type is a solid state drive.
    SSD,
    /// The disk type is a hard disk drive.
    HDD,
}

impl FromStr for DiskType {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "SSD" => Ok(Self::SSD),
            "HDD" => Ok(Self::HDD),
            _ => Err(()),
        }
    }
}

/// Represents a task disk requirement.
pub(crate) struct DiskRequirement {
    /// The size of the disk, in GiB.
    pub size: i64,

    /// The disk type as specified by a corresponding task hint.
    pub ty: Option<DiskType>,
}

/// Gets the `disks` requirement.
///
/// Upon success, returns a mapping of mount point to disk requirement.
pub(crate) fn disks<'a>(
    inputs: &'a TaskInputs,
    requirements: &'a HashMap<String, Value>,
    hints: &HashMap<String, Value>,
) -> Result<HashMap<&'a str, DiskRequirement>> {
    /// Helper for looking up a disk type from the hints.
    ///
    /// If we don't recognize the specification, we ignore it.
    fn lookup_type(
        mount_point: Option<&str>,
        hints: &HashMap<String, Value>,
        inputs: &TaskInputs,
    ) -> Option<DiskType> {
        lookup_entry(&[TASK_HINT_DISKS], |key| {
            inputs.hint(key).or_else(|| hints.get(key))
        })
        .and_then(|(_, v)| {
            if let Some(ty) = v.as_string() {
                return ty.parse().ok();
            }

            if let Some(map) = v.as_map() {
                // Find the corresponding key; we have to scan the keys because the map is
                // storing primitive values
                if let Some((_, v)) = map.iter().find(|(k, _)| match (k, mount_point) {
                    (None, None) => true,
                    (None, Some(_)) | (Some(_), None) => false,
                    (Some(k), Some(mount_point)) => k
                        .as_string()
                        .map(|k| k.as_str() == mount_point)
                        .unwrap_or(false),
                }) {
                    return v.as_string().and_then(|ty| ty.parse().ok());
                }
            }

            None
        })
    }

    /// Parses a disk specification into a size (in GiB) and optional mount
    /// point.
    fn parse_disk_spec(spec: &str) -> Option<(i64, Option<&str>)> {
        let iter = spec.split_whitespace();
        let mut first = None;
        let mut second = None;
        let mut third = None;

        for part in iter {
            if first.is_none() {
                first = Some(part);
                continue;
            }

            if second.is_none() {
                second = Some(part);
                continue;
            }

            if third.is_none() {
                third = Some(part);
                continue;
            }

            return None;
        }

        match (first, second, third) {
            (None, None, None) => None,
            (Some(size), None, None) => {
                // Specification is `<size>` (in GiB)
                Some((size.parse().ok()?, None))
            }
            (Some(first), Some(second), None) => {
                // Check for `<size> <unit>`; convert from the specified unit to GiB
                if let Ok(size) = first.parse() {
                    let unit: StorageUnit = second.parse().ok()?;
                    let size = unit.bytes(size)? / (ONE_GIBIBYTE as u64);
                    return Some((size.try_into().ok()?, None));
                }

                // Specification is `<mount-point> <size>` (where size is already in GiB)
                // The mount point must be absolute, i.e. start with `/`
                if !first.starts_with('/') {
                    return None;
                }

                Some((second.parse().ok()?, Some(first)))
            }
            (Some(mount_point), Some(size), Some(unit)) => {
                // Specification is `<mount-point> <size> <units>`
                let unit: StorageUnit = unit.parse().ok()?;
                let size = unit.bytes(size.parse().ok()?)? / (ONE_GIBIBYTE as u64);

                // Mount point must be absolute
                if !mount_point.starts_with('/') {
                    return None;
                }

                Some((size.try_into().ok()?, Some(mount_point)))
            }
            _ => unreachable!("should have one, two, or three values"),
        }
    }

    /// Inserts a disk into the disks map.
    fn insert_disk<'a>(
        spec: &'a str,
        hints: &HashMap<String, Value>,
        inputs: &TaskInputs,
        disks: &mut HashMap<&'a str, DiskRequirement>,
    ) -> Result<()> {
        let (size, mount_point) =
            parse_disk_spec(spec).with_context(|| format!("invalid disk specification `{spec}"))?;

        let prev = disks.insert(
            mount_point.unwrap_or(DEFAULT_DISK_MOUNT_POINT),
            DiskRequirement {
                size,
                ty: lookup_type(mount_point, hints, inputs),
            },
        );

        if prev.is_some() {
            bail!(
                "duplicate mount point `{mp}` specified in `disks` requirement",
                mp = mount_point.unwrap_or(DEFAULT_DISK_MOUNT_POINT)
            );
        }

        Ok(())
    }

    let mut disks = HashMap::new();
    if let Some((key, v)) = lookup_entry(&[TASK_REQUIREMENT_DISKS], |key| {
        inputs.requirement(key).or_else(|| requirements.get(key))
    }) {
        if let Some(size) = v.as_integer() {
            // Disk spec is just the size (in GiB)
            if size < 0 {
                bail!("task requirement `{key}` cannot be less than zero");
            }

            disks.insert(
                "/",
                DiskRequirement {
                    size,
                    ty: lookup_type(None, hints, inputs),
                },
            );
        } else if let Some(spec) = v.as_string() {
            insert_disk(spec, hints, inputs, &mut disks)?;
        } else if let Some(v) = v.as_array() {
            for spec in v.as_slice() {
                insert_disk(
                    spec.as_string().expect("spec should be a string"),
                    hints,
                    inputs,
                    &mut disks,
                )?;
            }
        } else {
            unreachable!("value should be an integer, string, or array");
        }
    }

    Ok(disks)
}

/// Gets the `max_retries` requirement from a requirements map with config
/// fallback.
pub(crate) fn max_retries(
    inputs: &TaskInputs,
    requirements: &HashMap<String, Value>,
    config: &Config,
) -> Result<u64> {
    if let Some((key, value)) = lookup_entry(
        &[
            TASK_REQUIREMENT_MAX_RETRIES,
            TASK_REQUIREMENT_MAX_RETRIES_ALIAS,
        ],
        |key| inputs.requirement(key).or_else(|| requirements.get(key)),
    ) {
        let retries = value
            .as_integer()
            .expect("`max_retries` requirement should be an integer");
        return ensure_non_negative_i64(SettingSource::Requirement, key, retries)
            .map(|value| value as u64);
    }

    Ok(config
        .task
        .retries
        .unwrap_or(DEFAULT_TASK_REQUIREMENT_MAX_RETRIES))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::ContainerSource;
    use super::*;

    fn map_with_value(key: &str, value: Value) -> HashMap<String, Value> {
        let mut map = HashMap::new();
        map.insert(key.to_string(), value);
        map
    }

    #[test]
    fn memory_disallows_negative_values() {
        let requirements = map_with_value(TASK_REQUIREMENT_MEMORY, Value::from(-1));
        let err = memory(&TaskInputs::default(), &requirements)
            .expect_err("`memory` should reject negatives");
        assert!(
            err.to_string()
                .contains("task requirement `memory` cannot be less than zero")
        );
    }

    #[test]
    fn max_retries_disallows_negative_values() {
        let requirements = map_with_value(TASK_REQUIREMENT_MAX_RETRIES, Value::from(-2));
        let err = max_retries(&TaskInputs::default(), &requirements, &Config::default())
            .expect_err("`max_retries` should reject negatives");
        assert!(
            err.to_string()
                .contains("task requirement `max_retries` cannot be less than zero")
        );
    }

    #[test]
    fn parses_bare_docker_image() {
        let source: ContainerSource = "ubuntu:22.04".parse().unwrap();
        assert_eq!(source, ContainerSource::Docker("ubuntu:22.04".to_string()));
        assert_eq!(source.to_string(), "ubuntu:22.04");
        assert_eq!(format!("{source:#}"), "docker://ubuntu:22.04");
    }

    #[test]
    fn parses_docker_protocol() {
        let source: ContainerSource = "docker://ubuntu:latest".parse().unwrap();
        assert_eq!(source, ContainerSource::Docker("ubuntu:latest".to_string()));
        assert_eq!(source.to_string(), "ubuntu:latest");
        assert_eq!(format!("{source:#}"), "docker://ubuntu:latest");
    }

    #[test]
    fn parses_library_protocol() {
        let source: ContainerSource = "library://sylabs/default/alpine:3.18".parse().unwrap();
        assert_eq!(
            source,
            ContainerSource::Library("sylabs/default/alpine:3.18".to_string())
        );
        assert_eq!(source.to_string(), "sylabs/default/alpine:3.18");
        assert_eq!(
            format!("{source:#}"),
            "library://sylabs/default/alpine:3.18"
        );
    }

    #[test]
    fn parses_oras_protocol() {
        let source: ContainerSource = "oras://ghcr.io/org/image:tag".parse().unwrap();
        assert_eq!(
            source,
            ContainerSource::Oras("ghcr.io/org/image:tag".to_string())
        );
        assert_eq!(source.to_string(), "ghcr.io/org/image:tag");
        assert_eq!(format!("{source:#}"), "oras://ghcr.io/org/image:tag");
    }

    #[test]
    fn parses_file_protocol_sif() {
        let source: ContainerSource = "file:///path/to/image.sif".parse().unwrap();
        assert_eq!(
            source,
            ContainerSource::SifFile(PathBuf::from("/path/to/image.sif"))
        );
        assert_eq!(source.to_string(), "/path/to/image.sif");
        assert_eq!(format!("{source:#}"), "file:///path/to/image.sif");
    }

    #[test]
    fn parses_file_protocol_unknown_extension() {
        let source: ContainerSource = "file:///path/to/image.tar".parse().unwrap();
        assert_eq!(
            source,
            ContainerSource::Unknown("file:///path/to/image.tar".to_string())
        );
        assert_eq!(source.to_string(), "file:///path/to/image.tar");
        assert_eq!(format!("{source:#}"), "file:///path/to/image.tar");
    }

    #[test]
    fn parses_unknown_protocol() {
        let source: ContainerSource = "ftp://example.com/image".parse().unwrap();
        assert_eq!(
            source,
            ContainerSource::Unknown("ftp://example.com/image".to_string())
        );
        assert_eq!(source.to_string(), "ftp://example.com/image");
        assert_eq!(format!("{source:#}"), "ftp://example.com/image");
    }

    #[test]
    fn parses_complex_docker_image() {
        let source: ContainerSource = "ghcr.io/stjude/sprocket:v1.0.0".parse().unwrap();
        assert_eq!(
            source,
            ContainerSource::Docker("ghcr.io/stjude/sprocket:v1.0.0".to_string())
        );
    }

    #[test]
    fn parses_docker_image_with_digest() {
        let source: ContainerSource = "ubuntu@sha256:abcdef1234567890".parse().unwrap();
        assert_eq!(
            source,
            ContainerSource::Docker("ubuntu@sha256:abcdef1234567890".to_string())
        );
    }
}
