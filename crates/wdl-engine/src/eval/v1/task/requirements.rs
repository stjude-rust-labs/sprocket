//! Implementation of utility functions for reading task requirements.

use std::borrow::Cow;
use std::collections::HashMap;
use std::hash::Hash;
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
use crate::ContainerImageOverrides;
use crate::ImageDigests;
use crate::ImageOverrideResolutionFailureMode;
use crate::ImageSource;
use crate::ONE_GIBIBYTE;
use crate::Object;
use crate::TaskInputs;
use crate::config::Config;
use crate::units::StorageUnit;
use crate::v1::DEFAULT_DISK_MOUNT_POINT;
use crate::v1::task::DEFAULT_GPU_COUNT;
use crate::v1::task::DEFAULT_TASK_REQUIREMENT_CPU;
use crate::v1::task::DEFAULT_TASK_REQUIREMENT_MEMORY;
use crate::v1::task::find_key_value;
use crate::v1::task::parse_storage_value;
use crate::v1::validators::SettingSource;
use crate::v1::validators::ensure_non_negative_i64;
use crate::v1::validators::invalid_numeric_value_message;

/// The WDL wildcard container value (`*`), meaning any image source is
/// acceptable.
const WILDCARD_CONTAINER: &str = "*";

/// Returns whether the task has an explicit `container` (or `docker` alias)
/// requirement set in either inputs or requirements.
pub(crate) fn has_container_requirement(inputs: &TaskInputs, requirements: &Object) -> bool {
    find_key_value(
        &[TASK_REQUIREMENT_CONTAINER, TASK_REQUIREMENT_CONTAINER_ALIAS],
        |key| inputs.requirement(key).or_else(|| requirements.get(key)),
    )
    .is_some()
}

/// Resolves an image source string against the provided override map and
/// default container.
fn resolve_image_source(
    s: &str,
    overrides: &ContainerImageOverrides,
    default: &str,
) -> Result<ImageSource> {
    let s = if s == WILDCARD_CONTAINER { default } else { s };

    // SAFETY: `FromStr` for `ImageSource` is infallible.
    let source: ImageSource = s.parse().unwrap();

    if let Some(image_override) = overrides.get(&source) {
        match image_override {
            ImageDigests::OciManifest(image_override) => return Ok(image_override.clone()),
            // See if we can find an override for the target arch...
            ImageDigests::PerArch(arch_overrides) => {
                if let Some(image_override) = arch_overrides.get(std::env::consts::ARCH) {
                    return Ok(image_override.clone());
                }
            }
        }
    }

    if overrides.failure_mode() == ImageOverrideResolutionFailureMode::Error {
        bail!("no override available for container image `{s}`");
    }

    Ok(source)
}

/// Gets the `container` requirement from a requirements map.
///
/// Returns a list of [`ImageSource`] candidates to try in order.
/// Any [`ImageSource::Any`] entries (from the WDL `*` wildcard) are
/// resolved to the configured default container.
pub(crate) fn container(
    inputs: &TaskInputs,
    requirements: &Object,
    overrides: &ContainerImageOverrides,
    default: &str,
) -> Result<Vec<ImageSource>> {
    let entry = find_key_value(
        &[TASK_REQUIREMENT_CONTAINER, TASK_REQUIREMENT_CONTAINER_ALIAS],
        |key| inputs.requirement(key).or_else(|| requirements.get(key)),
    );

    let Some((_, value)) = entry else {
        let default = resolve_image_source(default, overrides, default)?;
        return Ok(vec![default]);
    };

    if let Some(array) = value.as_array() {
        array
            .as_slice()
            .iter()
            .map(|v| {
                // SAFETY: the WDL type checker guarantees that elements of
                // a `container` array are `String`.
                let s = v
                    .as_string()
                    .expect("container array element should be a `String`");
                resolve_image_source(s.as_ref(), overrides, default)
            })
            .collect::<Result<_, _>>()
    } else {
        // SAFETY: the WDL type checker guarantees that `container` is
        // either `String` or `Array[String]`. Since we've ruled out the
        // array case above, the value must be coercible to `String`.
        let s: Cow<'_, str> = value
            .coerce(None, &PrimitiveType::String.into())
            .expect("container value should be coercible to `String`")
            .unwrap_string()
            .as_ref()
            .clone()
            .into();

        resolve_image_source(&s, overrides, default).map(|source| vec![source])
    }
}

/// Gets the `cpu` requirement from a requirements map.
pub(crate) fn cpu(inputs: &TaskInputs, requirements: &Object) -> f64 {
    find_key_value(&[TASK_REQUIREMENT_CPU], |key| {
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
pub(crate) fn memory(inputs: &TaskInputs, requirements: &Object) -> Result<i64> {
    if let Some((key, value)) = find_key_value(&[TASK_REQUIREMENT_MEMORY], |key| {
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
pub(crate) fn gpu(inputs: &TaskInputs, requirements: &Object, hints: &Object) -> Option<u64> {
    // If `requirements { gpu: false }` or there is no `gpu` requirement, return
    // `None`.
    let Some(true) = find_key_value(&[TASK_REQUIREMENT_GPU], |key| {
        inputs.requirement(key).or_else(|| requirements.get(key))
    })
    .and_then(|(_, v)| v.as_boolean()) else {
        return None;
    };

    // If there is no `gpu` hint giving us more detail on the request, use the
    // default count.
    let Some((_, hint)) = find_key_value(&[TASK_HINT_GPU], |key| {
        inputs.hint(key).or_else(|| hints.get(key))
    }) else {
        return Some(DEFAULT_GPU_COUNT);
    };

    // A string `gpu` hint is allowed by the spec, but we do not support them
    // yet.
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
        // If the hint is zero or negative, it's not clear what the user
        // intends. Maybe they have tried to disable GPUs by setting the count
        // to zero, or have made a logic error. Emit a warning, and continue
        // with no GPU request.
        Some(count) => {
            warn!(
                %count,
                "`{TASK_HINT_GPU}` hint specified {count} GPU(s); no GPUs will be requested for execution"
            );
            None
        }
        None => {
            // Typechecking should have already validated that the hint is an
            // integer or a string.
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    requirements: &'a Object,
    hints: &Object,
) -> Result<HashMap<&'a str, DiskRequirement>> {
    /// Helper for looking up a disk type from the hints.
    ///
    /// If we don't recognize the specification, we ignore it.
    fn lookup_type(
        mount_point: Option<&str>,
        hints: &Object,
        inputs: &TaskInputs,
    ) -> Option<DiskType> {
        find_key_value(&[TASK_HINT_DISKS], |key| {
            inputs.hint(key).or_else(|| hints.get(key))
        })
        .and_then(|(_, v)| {
            if let Some(ty) = v.as_string() {
                return ty.parse().ok();
            }

            if let Some(map) = v.as_map() {
                // Find the corresponding key; we have to scan the keys because
                // the map is storing primitive values
                if let Some((_, v)) = map.iter().find(|(k, _)| match (k, mount_point) {
                    (_, None) => false,
                    (k, Some(mount_point)) => k
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
                // Check for `<size> <unit>`; convert from the specified unit to
                // GiB
                if let Ok(size) = first.parse() {
                    let unit: StorageUnit = second.parse().ok()?;
                    let size = unit.bytes(size)? / (ONE_GIBIBYTE as u64);
                    return Some((size.try_into().ok()?, None));
                }

                // Specification is `<mount-point> <size>` (where size is
                // already in GiB) The mount point must be
                // absolute, i.e. start with `/`
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
        hints: &Object,
        inputs: &TaskInputs,
        disks: &mut HashMap<&'a str, DiskRequirement>,
    ) -> Result<()> {
        let (size, mount_point) = parse_disk_spec(spec)
            .with_context(|| format!("invalid disk specification `{spec}`"))?;

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
    if let Some((key, v)) = find_key_value(&[TASK_REQUIREMENT_DISKS], |key| {
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
    requirements: &Object,
    config: &Config,
) -> Result<u64> {
    if config.task.retries.is_disabled() {
        return Ok(0);
    }

    if let Some((key, value)) = find_key_value(
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

    Ok(config.task.retries.into())
}

#[cfg(test)]
mod tests {
    use indexmap::IndexMap;

    use super::*;
    use crate::ImageSource;
    use crate::PrimitiveValue;
    use crate::Value;
    use crate::config::DEFAULT_TASK_CONTAINER;

    fn object_with_value(key: &str, value: Value) -> Object {
        let mut map = IndexMap::new();
        map.insert(key.to_string(), value);
        Object::new(map)
    }

    #[test]
    fn memory_disallows_negative_values() {
        let requirements = object_with_value(TASK_REQUIREMENT_MEMORY, Value::from(-1));
        let err = memory(&TaskInputs::default(), &requirements)
            .expect_err("`memory` should reject negatives");
        assert!(
            err.to_string()
                .contains("task requirement `memory` cannot be less than zero")
        );
    }

    #[test]
    fn max_retries_disallows_negative_values() {
        let requirements = object_with_value(TASK_REQUIREMENT_MAX_RETRIES, Value::from(-2));
        let err = max_retries(&TaskInputs::default(), &requirements, &Config::default())
            .expect_err("`max_retries` should reject negatives");
        assert!(
            err.to_string()
                .contains("task requirement `max_retries` cannot be less than zero")
        );
    }

    #[test]
    fn container_returns_default_when_unset() {
        let result = container(
            &TaskInputs::default(),
            &Object::empty(),
            &ContainerImageOverrides::default(),
            DEFAULT_TASK_CONTAINER,
        )
        .unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0],
            ImageSource::Docker(DEFAULT_TASK_CONTAINER.to_string())
        );
    }

    #[test]
    fn container_returns_custom_default_when_unset() {
        let result = container(
            &TaskInputs::default(),
            &Object::empty(),
            &ContainerImageOverrides::default(),
            "alpine:3.18",
        )
        .unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], ImageSource::Docker("alpine:3.18".to_string()));
    }

    #[test]
    fn container_returns_single_image() {
        let requirements = object_with_value(
            TASK_REQUIREMENT_CONTAINER,
            PrimitiveValue::new_string("foo:bar").into(),
        );
        let result = container(
            &TaskInputs::default(),
            &requirements,
            &ContainerImageOverrides::default(),
            DEFAULT_TASK_CONTAINER,
        )
        .unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], ImageSource::Docker("foo:bar".to_string()));
    }

    #[test]
    fn container_resolves_single_wildcard_to_default() {
        let requirements = object_with_value(
            TASK_REQUIREMENT_CONTAINER,
            PrimitiveValue::new_string("*").into(),
        );
        let result = container(
            &TaskInputs::default(),
            &requirements,
            &ContainerImageOverrides::default(),
            DEFAULT_TASK_CONTAINER,
        )
        .unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0],
            ImageSource::Docker(DEFAULT_TASK_CONTAINER.to_string())
        );
    }

    #[test]
    fn container_resolves_single_wildcard_to_custom_default() {
        let requirements = object_with_value(
            TASK_REQUIREMENT_CONTAINER,
            PrimitiveValue::new_string("*").into(),
        );
        let result = container(
            &TaskInputs::default(),
            &requirements,
            &ContainerImageOverrides::default(),
            "debian:12",
        )
        .unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], ImageSource::Docker("debian:12".to_string()));
    }

    #[test]
    fn container_returns_array_of_images() {
        use wdl_analysis::types::ArrayType;

        let elements = vec![
            PrimitiveValue::new_string("foo:1.0").into(),
            PrimitiveValue::new_string("bar:2.0").into(),
            PrimitiveValue::new_string("baz:3.0").into(),
        ];
        let array = crate::Array::new_unchecked(
            ArrayType::new(wdl_analysis::types::PrimitiveType::String),
            elements,
        );
        let requirements = object_with_value(
            TASK_REQUIREMENT_CONTAINER,
            Value::Compound(crate::CompoundValue::Array(array)),
        );

        let result = container(
            &TaskInputs::default(),
            &requirements,
            &ContainerImageOverrides::default(),
            DEFAULT_TASK_CONTAINER,
        )
        .unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[0], ImageSource::Docker("foo:1.0".to_string()));
        assert_eq!(result[1], ImageSource::Docker("bar:2.0".to_string()));
        assert_eq!(result[2], ImageSource::Docker("baz:3.0".to_string()));
    }

    #[test]
    fn container_resolves_wildcard_in_array() {
        use wdl_analysis::types::ArrayType;

        let elements = vec![
            PrimitiveValue::new_string("foo:1.0").into(),
            PrimitiveValue::new_string("*").into(),
            PrimitiveValue::new_string("bar:2.0").into(),
        ];
        let array = crate::Array::new_unchecked(
            ArrayType::new(wdl_analysis::types::PrimitiveType::String),
            elements,
        );
        let requirements = object_with_value(
            TASK_REQUIREMENT_CONTAINER,
            Value::Compound(crate::CompoundValue::Array(array)),
        );

        let result = container(
            &TaskInputs::default(),
            &requirements,
            &ContainerImageOverrides::default(),
            DEFAULT_TASK_CONTAINER,
        )
        .unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[0], ImageSource::Docker("foo:1.0".to_string()));
        assert_eq!(
            result[1],
            ImageSource::Docker(DEFAULT_TASK_CONTAINER.to_string())
        );
        assert_eq!(result[2], ImageSource::Docker("bar:2.0".to_string()));
    }

    #[test]
    fn container_resolves_wildcard_in_array_with_custom_default() {
        use wdl_analysis::types::ArrayType;

        let elements = vec![
            PrimitiveValue::new_string("foo:1.0").into(),
            PrimitiveValue::new_string("*").into(),
        ];
        let array = crate::Array::new_unchecked(
            ArrayType::new(wdl_analysis::types::PrimitiveType::String),
            elements,
        );
        let requirements = object_with_value(
            TASK_REQUIREMENT_CONTAINER,
            Value::Compound(crate::CompoundValue::Array(array)),
        );

        let result = container(
            &TaskInputs::default(),
            &requirements,
            &ContainerImageOverrides::default(),
            "alpine:3.18",
        )
        .unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], ImageSource::Docker("foo:1.0".to_string()));
        assert_eq!(result[1], ImageSource::Docker("alpine:3.18".to_string()));
    }

    #[test]
    fn container_resolves_overrides_in_array() {
        use wdl_analysis::types::ArrayType;

        let elements = vec![
            PrimitiveValue::new_string("foo:1.0").into(),
            PrimitiveValue::new_string("bar:2.0").into(),
        ];
        let array = crate::Array::new_unchecked(
            ArrayType::new(wdl_analysis::types::PrimitiveType::String),
            elements,
        );
        let requirements = object_with_value(
            TASK_REQUIREMENT_CONTAINER,
            Value::Compound(crate::CompoundValue::Array(array)),
        );

        let overrides: ContainerImageOverrides = [(
            ImageSource::Docker(String::from("foo:1.0")),
            ImageDigests::OciManifest(ImageSource::Docker("foo@sha256:1111".to_string())),
        )]
        .into_iter()
        .collect();

        let result = container(
            &TaskInputs::default(),
            &requirements,
            &overrides,
            DEFAULT_TASK_CONTAINER,
        )
        .unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(
            result[0],
            ImageSource::Docker("foo@sha256:1111".to_string())
        );
        assert_eq!(result[1], ImageSource::Docker("bar:2.0".to_string()));
    }

    #[test]
    fn container_resolves_overrides_without_docker_prefix() {
        let requirements = object_with_value(
            TASK_REQUIREMENT_CONTAINER,
            PrimitiveValue::new_string("foo:1.0").into(),
        );

        let overrides: ContainerImageOverrides = [(
            ImageSource::Docker(String::from("foo:1.0")),
            ImageDigests::OciManifest(ImageSource::Docker("foo@sha256:1111".to_string())),
        )]
        .into_iter()
        .collect();

        let result = container(
            &TaskInputs::default(),
            &requirements,
            &overrides,
            DEFAULT_TASK_CONTAINER,
        )
        .unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0],
            ImageSource::Docker("foo@sha256:1111".to_string())
        );
    }

    #[test]
    fn respect_inputs_over_requirements() {
        let mut inputs = TaskInputs::default();
        inputs.override_requirement("container", PrimitiveValue::new_string("foo:bar"));
        inputs.override_requirement("cpu", 1234);
        inputs.override_requirement("gpu", true);
        inputs.override_requirement("memory", 1234);
        inputs.override_requirement("disks", 1234);
        inputs.override_requirement("max_retries", 1234);
        inputs.override_hint("gpu", 10);

        let mut requirements: IndexMap<String, Value> = Default::default();
        requirements.insert(
            "container".to_string(),
            PrimitiveValue::new_string("baz:qux").into(),
        );
        requirements.insert("cpu".to_string(), PrimitiveValue::from(1.0).into());
        requirements.insert("memory".to_string(), PrimitiveValue::from(1).into());
        requirements.insert(
            "disks".to_string(),
            PrimitiveValue::new_string("1 GiB").into(),
        );
        requirements.insert("max_retries".to_string(), PrimitiveValue::from(1).into());
        let requirements = Object::new(requirements);

        assert_eq!(
            container(
                &inputs,
                &requirements,
                &Default::default(),
                DEFAULT_TASK_CONTAINER
            )
            .unwrap()
            .first()
            .unwrap()
            .to_string(),
            "foo:bar"
        );

        assert_eq!(cpu(&inputs, &requirements), 1234.0);

        assert_eq!(gpu(&inputs, &requirements, &Object::empty()), Some(10));

        assert_eq!(
            disks(&inputs, &requirements, &Object::empty()).unwrap(),
            HashMap::from_iter([(
                "/",
                DiskRequirement {
                    size: 1234,
                    ty: None
                }
            )])
        );

        assert_eq!(
            max_retries(&inputs, &requirements, &Default::default()).unwrap(),
            1234
        );
    }
}
