//! Module manifest and lockfile persistence.

use std::path::Path;

use anyhow::Context as _;
use wdl_modules::Lockfile;
use wdl_modules::Manifest;
use wdl_modules::dependency::DependencySource;

use super::Project;

/// Aligns a temporary file's permissions with its destination before an
/// atomic rename.
pub(crate) fn align_temp_permissions(
    temp: &tempfile::NamedTempFile,
    path: &Path,
) -> anyhow::Result<()> {
    if let Ok(metadata) = std::fs::metadata(path) {
        temp.as_file()
            .set_permissions(metadata.permissions())
            .with_context(|| format!("setting permissions on `{}`", temp.path().display()))?;
        return Ok(());
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        temp.as_file()
            .set_permissions(std::fs::Permissions::from_mode(0o644))
            .with_context(|| format!("setting permissions on `{}`", temp.path().display()))?;
    }

    Ok(())
}

/// Writes `module-lock.json` atomically.
pub fn write_lockfile(project: &Project, lock: &Lockfile) -> anyhow::Result<()> {
    let dir = project
        .lockfile_path
        .parent()
        .unwrap_or_else(|| Path::new("."));
    let mut temp = tempfile::NamedTempFile::new_in(dir)
        .with_context(|| format!("creating a temporary file in `{}`", dir.display()))?;
    lock.write(&mut temp)
        .with_context(|| format!("writing `{}`", temp.path().display()))?;
    align_temp_permissions(&temp, &project.lockfile_path)?;
    temp.persist(&project.lockfile_path)
        .with_context(|| format!("replacing `{}`", project.lockfile_path.display()))?;
    Ok(())
}

/// Reads `module.json` as json while validating it with strict manifest
/// parsing.
pub fn read_manifest_value(path: &Path) -> anyhow::Result<serde_json::Value> {
    let bytes = std::fs::read(path).with_context(|| format!("reading `{}`", path.display()))?;
    Manifest::parse(&bytes).with_context(|| format!("parsing `{}`", path.display()))?;
    let value = serde_json::from_slice(&bytes)
        .with_context(|| format!("parsing `{}` as json", path.display()))?;
    Ok(value)
}

/// Writes `module.json` atomically after validating parser-accepted shape.
pub fn write_manifest_value(path: &Path, value: &serde_json::Value) -> anyhow::Result<()> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    Manifest::parse(&bytes).with_context(|| format!("parsing `{}`", path.display()))?;

    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let mut temp = tempfile::NamedTempFile::new_in(dir)
        .with_context(|| format!("creating a temporary file in `{}`", dir.display()))?;
    std::io::Write::write_all(&mut temp, &bytes)
        .with_context(|| format!("writing `{}`", temp.path().display()))?;
    align_temp_permissions(&temp, path)?;
    temp.persist(path)
        .with_context(|| format!("replacing `{}`", path.display()))?;
    Ok(())
}

/// Parses an edited manifest json value with strict manifest validation.
pub(crate) fn parse_manifest_value(value: &serde_json::Value) -> anyhow::Result<Manifest> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    Manifest::parse(&bytes).context("parsing edited `module.json`")
}

/// Inserts or replaces a dependency source in the manifest json.
pub fn set_dependency(
    value: &mut serde_json::Value,
    name: &str,
    source: &DependencySource,
) -> anyhow::Result<()> {
    let root = value
        .as_object_mut()
        .with_context(|| "`module.json` root must be an object")?;

    if !root.contains_key("dependencies") {
        root.insert(
            "dependencies".to_string(),
            serde_json::Value::Object(serde_json::Map::new()),
        );
    }

    let dependencies = root
        .get_mut("dependencies")
        .and_then(serde_json::Value::as_object_mut)
        .with_context(|| "`dependencies` in `module.json` must be an object")?;

    dependencies.insert(name.to_string(), serde_json::to_value(source)?);
    dependencies.sort_keys();

    Ok(())
}

/// Removes a dependency from the manifest json.
pub fn remove_dependency(value: &mut serde_json::Value, name: &str) -> anyhow::Result<bool> {
    let root = value
        .as_object_mut()
        .with_context(|| "`module.json` root must be an object")?;

    let Some(dependencies_value) = root.get_mut("dependencies") else {
        return Ok(false);
    };

    let dependencies = dependencies_value
        .as_object_mut()
        .with_context(|| "`dependencies` in `module.json` must be an object")?;
    Ok(dependencies.remove(name).is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_MANIFEST_JSON: &str = r#"{
      "name": "example",
      "license": "MIT",
      "entrypoint": "main.wdl",
      "x-extra": { "enabled": true, "note": "preserve me" },
      "dependencies": {
        "zeta": { "path": "./zeta" },
        "alpha": { "path": "./alpha", "x-source-extra": 7 }
      }
    }"#;

    #[test]
    fn set_dependency_inserts_preserves_extra_and_sorts_dependencies() {
        let mut value: serde_json::Value = serde_json::from_str(VALID_MANIFEST_JSON).unwrap();
        let source: DependencySource = serde_json::from_str(
            r#"{
              "path": "./beta",
              "x-source-extra": "kept"
            }"#,
        )
        .unwrap();

        set_dependency(&mut value, "beta", &source).unwrap();

        assert_eq!(value["name"], "example");
        assert_eq!(value["x-extra"]["note"], "preserve me");
        assert_eq!(value["dependencies"]["alpha"]["x-source-extra"], 7);
        assert_eq!(value["dependencies"]["beta"]["x-source-extra"], "kept");

        let dependency_keys = value["dependencies"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(dependency_keys, vec!["alpha", "beta", "zeta"]);
    }

    #[test]
    fn set_dependency_errors_when_dependencies_is_non_object() {
        let mut value: serde_json::Value = serde_json::from_str(VALID_MANIFEST_JSON).unwrap();
        value["dependencies"] = serde_json::Value::String("not-an-object".to_string());
        let source: DependencySource = serde_json::from_str(r#"{ "path": "./beta" }"#).unwrap();

        let err = set_dependency(&mut value, "beta", &source).unwrap_err();
        assert!(
            err.to_string()
                .contains("`dependencies` in `module.json` must be an object")
        );
    }

    #[test]
    fn remove_dependency_returns_false_when_dependency_absent() {
        let mut value: serde_json::Value = serde_json::from_str(VALID_MANIFEST_JSON).unwrap();
        assert!(!remove_dependency(&mut value, "missing").unwrap());
    }

    #[test]
    fn write_manifest_value_round_trips_through_manifest_parse() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("module.json");
        let value: serde_json::Value = serde_json::from_str(VALID_MANIFEST_JSON).unwrap();

        write_manifest_value(&path, &value).unwrap();

        let bytes = std::fs::read(&path).unwrap();
        assert!(bytes.ends_with(b"\n"));
        Manifest::parse(&bytes).unwrap();

        let written: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(written["x-extra"]["enabled"], true);
        assert_eq!(written["dependencies"]["alpha"]["x-source-extra"], 7);
    }

    /// Reads the permission bits of `path`.
    #[cfg(unix)]
    fn mode_of(path: &Path) -> u32 {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    #[test]
    #[cfg(unix)]
    fn write_manifest_value_gives_new_files_conventional_permissions() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("module.json");
        let value: serde_json::Value = serde_json::from_str(VALID_MANIFEST_JSON).unwrap();

        write_manifest_value(&path, &value).unwrap();

        assert_eq!(mode_of(&path), 0o644);
    }

    #[test]
    #[cfg(unix)]
    fn write_manifest_value_preserves_existing_permissions() {
        use std::os::unix::fs::PermissionsExt as _;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("module.json");
        let value: serde_json::Value = serde_json::from_str(VALID_MANIFEST_JSON).unwrap();
        write_manifest_value(&path, &value).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();

        write_manifest_value(&path, &value).unwrap();

        assert_eq!(mode_of(&path), 0o600);
    }
}
