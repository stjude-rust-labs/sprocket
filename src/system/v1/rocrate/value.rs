//! Converts runtime WDL values into type-aware RO-Crate entities.

use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Context as _;
use anyhow::Result;
use rocraters::ro_crate::constraints::DataType;
use rocraters::ro_crate::constraints::EntityValue;
use rocraters::ro_crate::constraints::Id;
use rocraters::ro_crate::contextual_entity::ContextualEntity;
use rocraters::ro_crate::data_entity::DataEntity;
use rocraters::ro_crate::graph_vector::GraphVector;
use sha2::Digest;
use sha2::Sha256;
use wdl::engine::Value;

use super::RoCrateOptions;

/// Builds the `dynamic_entity` map from `(key, value)` pairs.
fn bag(pairs: Vec<(&str, EntityValue)>) -> Option<HashMap<String, EntityValue>> {
    Some(pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

/// Computes the SHA-256 of a file as a lowercase hex string.
fn sha256_hex(path: &Path) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher)?;
    Ok(format!("{:x}", hasher.finalize()))
}

/// Adds one length-delimited string to a stable hash.
fn update_stable_hash(hasher: &mut Sha256, value: &str) {
    hasher.update((value.len() as u64).to_be_bytes());
    hasher.update(value.as_bytes());
}

/// Stable, path-free placeholder `@id` for a non-localized data value. The hash
/// is derived from `role` + `rel` (the WDL traversal position), NOT from the
/// source absolute path, so it is stable and reveals no host layout.
fn external_placeholder_id(role: &str, rel: &str, basename: &str) -> String {
    let mut hasher = Sha256::new();
    update_stable_hash(&mut hasher, role);
    update_stable_hash(&mut hasher, rel);
    let hash = format!("{:x}", hasher.finalize());
    format!(
        "external/{}/{}/{}",
        sanitize_component(role),
        hash,
        sanitize_component(basename)
    )
}

/// Encodes one path component so it cannot affect crate layout.
pub(super) fn sanitize_component(component: &str) -> String {
    if component == "." {
        return "%2e".to_string();
    }
    if component == ".." {
        return "%2e%2e".to_string();
    }

    let mut sanitized = String::new();
    for byte in component.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'.' | b'_' | b'-' => {
                sanitized.push(byte as char);
            }
            _ => {
                sanitized.push_str(&format!("%{byte:02x}"));
            }
        }
    }

    if sanitized.is_empty() {
        "_".to_string()
    } else {
        sanitized
    }
}

/// Encodes a traversal path while preserving its component structure.
fn sanitize_relative_path(path: &str) -> String {
    let components = path
        .split(['/', '\\'])
        .filter(|component| !component.is_empty())
        .map(sanitize_component)
        .collect::<Vec<_>>();

    if components.is_empty() {
        "_".to_string()
    } else {
        components.join("/")
    }
}

/// Recursively copies a directory tree.
fn copy_dir_recursive(src: &Path, dest: &Path) -> Result<()> {
    std::fs::create_dir_all(dest)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        anyhow::ensure!(
            !file_type.is_symlink(),
            "cannot localize symlink `{}` into the crate",
            entry.path().display()
        );
        let to = dest.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_recursive(&entry.path(), &to)?;
        } else {
            std::fs::copy(entry.path(), &to)?;
        }
    }
    Ok(())
}

/// Returns `true` when a host path string refers to a remote URL.
fn is_remote(path_str: &str) -> bool {
    path_str.contains("://")
}

/// Appends a trailing slash to directory `@id`s, per the RO-Crate convention
/// that `Dataset` identifiers end with `/`.
fn finalize_id(id: &str, is_dir: bool) -> String {
    if is_dir && !id.ends_with('/') {
        format!("{id}/")
    } else {
        id.to_string()
    }
}

/// Ensures a path is a regular file (or a directory when `is_dir`), rejecting
/// FIFOs, devices, sockets, and type mismatches that could hang or corrupt a
/// copy/checksum.
fn ensure_regular_or_dir(path: &Path, is_dir: bool) -> Result<()> {
    let meta =
        std::fs::metadata(path).with_context(|| format!("inspecting `{}`", path.display()))?;
    if is_dir {
        anyhow::ensure!(
            meta.is_dir(),
            "expected a directory at `{}`",
            path.display()
        );
    } else {
        anyhow::ensure!(
            meta.is_file(),
            "cannot represent non-regular file `{}` in the crate",
            path.display()
        );
    }
    Ok(())
}

/// Rejects symlinks and non-regular files before localizing an external value.
fn ensure_localizable(src: &Path, is_dir: bool) -> Result<()> {
    let meta = std::fs::symlink_metadata(src).with_context(|| {
        format!(
            "inspecting data value `{}` before localization",
            src.display()
        )
    })?;
    anyhow::ensure!(
        !meta.file_type().is_symlink(),
        "cannot localize symlink `{}` into the crate; rerun with \
         `--no-ro-crate-localize` to record an external reference instead",
        src.display()
    );
    ensure_regular_or_dir(src, is_dir)
}

/// Determines the crate-relative `@id` for a data value, copying local files and
/// directories under `inputs/`/`outputs/` when `opts.localize`, else returning an
/// `external/` placeholder plus a redacted original-location marker. Returns any
/// extra entity properties to record (e.g. the external marker).
fn localize_data_path(
    src: &str,
    is_dir: bool,
    role: &str,
    rel: &str,
    crate_root: &Path,
    opts: &RoCrateOptions,
) -> Result<(String, Vec<(&'static str, EntityValue)>)> {
    let src_path = Path::new(src);
    let basename = src_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("data");

    // Already inside the crate root (e.g. run outputs): reference in place.
    // Canonicalize both sides so a `..` segment or a symlink whose target escapes
    // the crate root is NOT treated as in-crate; such paths fall through to the
    // rejection logic below rather than being silently followed.
    if let Ok(canon_root) = crate_root.canonicalize()
        && let Ok(canon_src) = src_path.canonicalize()
        && let Ok(rel_existing) = canon_src.strip_prefix(&canon_root)
        && let Some(rel_str) = rel_existing.to_str()
    {
        ensure_regular_or_dir(&canon_src, is_dir)?;
        return Ok((finalize_id(rel_str, is_dir), Vec::new()));
    }

    if !opts.localize {
        // Record that an external value exists, without leaking the host path or
        // traversing its contents.
        let id = finalize_id(&external_placeholder_id(role, rel, basename), is_dir);
        let marker = vec![(
            "contentLocation",
            EntityValue::EntityString("[redacted: external, not localized]".to_string()),
        )];
        return Ok((id, marker));
    }

    // Localization is on. Remote download is not supported in this build; per the
    // spec, failing to localize an enabled value fails emission (non-fatal unless
    // `--ro-crate-strict`).
    if is_remote(src) {
        anyhow::bail!(
            "cannot localize remote data value `{src}` into the crate; rerun with \
             `--no-ro-crate-localize` to record an external reference instead"
        );
    }
    ensure_localizable(src_path, is_dir)?;

    let base = format!(
        "{}/{}/{}",
        sanitize_component(role),
        sanitize_relative_path(rel),
        sanitize_component(basename)
    );
    let dest = crate_root.join(&base);
    anyhow::ensure!(
        dest.starts_with(crate_root),
        "localized crate path escaped the crate root"
    );
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating crate directory `{}`", parent.display()))?;
    }
    if is_dir {
        copy_dir_recursive(src_path, &dest)
            .with_context(|| format!("localizing directory `{src}`"))?;
    } else {
        std::fs::copy(src, &dest).with_context(|| format!("localizing file `{src}`"))?;
    }
    Ok((finalize_id(&base, is_dir), Vec::new()))
}

/// Pushes a child `File` entity (with size and optional checksum) for every
/// readable file under a localized directory, returning their `@id`s for the
/// parent `Dataset`'s `hasPart`.
fn directory_part_entities(
    dir_id: &str,
    dir_abs: &Path,
    opts: &RoCrateOptions,
    graph: &mut Vec<GraphVector>,
) -> Result<Vec<String>> {
    let mut parts = Vec::new();
    let mut stack = vec![dir_abs.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = std::fs::read_dir(&dir)
            .with_context(|| format!("reading localized directory `{}`", dir.display()))?;
        for entry in entries {
            let entry = entry
                .with_context(|| format!("reading localized directory `{}`", dir.display()))?;
            let path = entry.path();
            let file_type = entry
                .file_type()
                .with_context(|| format!("reading file type for `{}`", path.display()))?;
            anyhow::ensure!(
                !file_type.is_symlink(),
                "refusing to follow symlink `{}` inside a crate directory",
                path.display()
            );
            if file_type.is_dir() {
                stack.push(path);
                continue;
            }
            anyhow::ensure!(
                file_type.is_file(),
                "refusing to include non-regular file `{}` in the crate",
                path.display()
            );
            let rel = path
                .strip_prefix(dir_abs)
                .with_context(|| format!("relativizing localized file `{}`", path.display()))?;
            let rel_str = rel.to_str().with_context(|| {
                format!("localized file path was not utf-8 `{}`", path.display())
            })?;
            let id = format!("{dir_id}/{rel_str}");
            let mut props = vec![("name", EntityValue::EntityString(rel_str.to_string()))];
            let meta = std::fs::metadata(&path).with_context(|| {
                format!("reading metadata for localized file `{}`", path.display())
            })?;
            props.push(("contentSize", EntityValue::Entityi64(meta.len() as i64)));
            if opts.checksums {
                let hex = sha256_hex(&path)
                    .with_context(|| format!("checksumming localized file `{}`", path.display()))?;
                props.push(("sha256", EntityValue::EntityString(hex)));
            }
            graph.push(GraphVector::DataEntity(DataEntity {
                id: id.clone(),
                type_: DataType::Term("File".to_string()),
                dynamic_entity: bag(props),
            }));
            parts.push(id);
        }
    }
    Ok(parts)
}

/// Pushes a `File`/`Dataset` data entity for a data value and returns its
/// crate-relative `@id`. Localizes per [`localize_data_path`]; directories carry
/// per-file checksums via `hasPart` rather than an aggregate tree hash.
fn data_entity(
    src: &str,
    is_dir: bool,
    role: &str,
    rel: &str,
    crate_root: &Path,
    opts: &RoCrateOptions,
    graph: &mut Vec<GraphVector>,
) -> Result<String> {
    let (id, extra) = localize_data_path(src, is_dir, role, rel, crate_root, opts)?;

    // Deduplicate: the same path used as both an input and an output (or repeated
    // in a structure) must be a single data entity with a unique `@id`, not a
    // duplicate. If it is already in the graph, just reference it.
    if graph.iter().any(|g| g.get_id() == &id) {
        return Ok(id);
    }

    let name = Path::new(&id)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&id)
        .to_string();

    let is_external = id.starts_with("external/");
    // Where the materialized bytes live: under the crate for localized/in-place
    // values, or at the original source for external (non-localized) values. The
    // crate-relative id may carry a trailing slash (directories), so trim it when
    // forming a filesystem path.
    let stat_path: PathBuf = if is_external {
        PathBuf::from(src)
    } else {
        crate_root.join(id.trim_end_matches('/'))
    };

    let mut props = vec![("name", EntityValue::EntityString(name))];
    props.extend(extra);

    if is_dir {
        // Enumerate only directories whose bytes live in the crate (localized or
        // in-place). External, non-localized directories are referenced as a
        // placeholder without traversing or checksumming host contents.
        if !is_external {
            let parts = directory_part_entities(id.trim_end_matches('/'), &stat_path, opts, graph)?;
            if !parts.is_empty() {
                props.push(("hasPart", EntityValue::EntityId(Id::IdArray(parts))));
            }
        }
    } else if let Ok(meta) = std::fs::metadata(&stat_path) {
        props.push(("contentSize", EntityValue::Entityi64(meta.len() as i64)));
        // Only checksum genuine regular files; hashing a FIFO/device would hang.
        if opts.checksums
            && meta.is_file()
            && let Ok(hex) = sha256_hex(&stat_path)
        {
            props.push(("sha256", EntityValue::EntityString(hex)));
        }
    }

    let ty = if is_dir {
        DataType::Term("Dataset".to_string())
    } else {
        DataType::Term("File".to_string())
    };
    graph.push(GraphVector::DataEntity(DataEntity {
        id: id.clone(),
        type_: ty,
        dynamic_entity: bag(props),
    }));
    Ok(id)
}

/// Recursively converts a value into an `EntityValue`, lifting any nested
/// `File`/`Directory` into its own data entity and returning an `EntityId`
/// reference to it. `rel` is the traversal path used for stable data-entity IDs.
fn value_to_entity_value(
    value: &Value,
    role: &str,
    rel: &str,
    crate_root: &Path,
    opts: &RoCrateOptions,
    graph: &mut Vec<GraphVector>,
) -> Result<EntityValue> {
    if value.is_none() {
        return Ok(EntityValue::EntityNull(None));
    }
    if let Some(f) = value.as_file() {
        let id = data_entity(f.as_str(), false, role, rel, crate_root, opts, graph)?;
        return Ok(EntityValue::EntityId(Id::Id(id)));
    }
    if let Some(d) = value.as_directory() {
        let id = data_entity(d.as_str(), true, role, rel, crate_root, opts, graph)?;
        return Ok(EntityValue::EntityId(Id::Id(id)));
    }
    if let Some(b) = value.as_boolean() {
        return Ok(EntityValue::EntityBool(b));
    }
    if let Some(i) = value.as_integer() {
        return Ok(EntityValue::Entityi64(i));
    }
    if let Some(f) = value.as_float() {
        return Ok(EntityValue::Entityf64(f));
    }
    if let Some(s) = value.as_string() {
        return Ok(EntityValue::EntityString(s.to_string()));
    }
    if let Some(arr) = value.as_array() {
        let mut items = Vec::new();
        for (i, v) in arr.as_slice().iter().enumerate() {
            items.push(value_to_entity_value(
                v,
                role,
                &format!("{rel}/{i}"),
                crate_root,
                opts,
                graph,
            )?);
        }
        return Ok(EntityValue::EntityVec(items));
    }
    if let Some(obj) = value.as_object() {
        let mut map = HashMap::new();
        for (k, v) in obj.iter() {
            map.insert(
                k.to_string(),
                value_to_entity_value(v, role, &format!("{rel}/{k}"), crate_root, opts, graph)?,
            );
        }
        return Ok(EntityValue::EntityObject(map));
    }
    if let Some(st) = value.as_struct() {
        let mut map = HashMap::new();
        for (k, v) in st.iter() {
            map.insert(
                k.to_string(),
                value_to_entity_value(v, role, &format!("{rel}/{k}"), crate_root, opts, graph)?,
            );
        }
        return Ok(EntityValue::EntityObject(map));
    }
    if let Some(m) = value.as_map() {
        let mut map = HashMap::new();
        for (k, v) in m.iter() {
            let key = format!("{k}");
            let entry =
                value_to_entity_value(v, role, &format!("{rel}/{key}"), crate_root, opts, graph)?;
            map.insert(key, entry);
        }
        return Ok(EntityValue::EntityObject(map));
    }
    if let Some(p) = value.as_pair() {
        let left = value_to_entity_value(
            p.left(),
            role,
            &format!("{rel}/left"),
            crate_root,
            opts,
            graph,
        )?;
        let right = value_to_entity_value(
            p.right(),
            role,
            &format!("{rel}/right"),
            crate_root,
            opts,
            graph,
        )?;
        return Ok(EntityValue::EntityVec(vec![left, right]));
    }
    // Hidden/Call/TypeNameRef and anything unexpected: fall back to display.
    Ok(EntityValue::EntityString(format!("{value}")))
}

/// Role-threaded form of [`value_to_entities`]. `role` is `inputs`/`outputs` (the
/// crate-relative layout prefix) and `rel` is the value's traversal path.
fn value_to_entities_roled(
    id_prefix: &str,
    role: &str,
    rel: &str,
    value: &Value,
    crate_root: &Path,
    opts: &RoCrateOptions,
    graph: &mut Vec<GraphVector>,
) -> Result<String> {
    // Top-level File/Directory: the value *is* the data entity.
    if let Some(f) = value.as_file() {
        return data_entity(f.as_str(), false, role, rel, crate_root, opts, graph);
    }
    if let Some(d) = value.as_directory() {
        return data_entity(d.as_str(), true, role, rel, crate_root, opts, graph);
    }

    // Everything else: a PropertyValue carrying the (possibly structured) value.
    let id = format!("#{id_prefix}-{rel}");
    let entity_value = value_to_entity_value(value, role, rel, crate_root, opts, graph)?;
    graph.push(GraphVector::ContextualEntity(ContextualEntity {
        id: id.clone(),
        type_: DataType::Term("PropertyValue".to_string()),
        dynamic_entity: bag(vec![
            ("name", EntityValue::EntityString(rel.to_string())),
            ("value", entity_value),
        ]),
    }));
    Ok(id)
}

/// Converts a named value into RO-Crate graph entities, appending File/Dataset
/// data entities to `graph` and returning the entity `@id` that represents the
/// value (a data-entity id for File/Directory, else a `PropertyValue` id).
///
/// `id_prefix` is `input`/`output`; it determines both the `PropertyValue` id and
/// the `inputs/`/`outputs/` localization layout.
pub fn value_to_entities(
    id_prefix: &str,
    name: &str,
    value: &Value,
    crate_root: &Path,
    opts: &RoCrateOptions,
    graph: &mut Vec<GraphVector>,
) -> Result<String> {
    let role = match id_prefix {
        "input" => "inputs",
        "output" => "outputs",
        other => anyhow::bail!("unknown ro-crate value prefix `{other}`"),
    };
    value_to_entities_roled(id_prefix, role, name, value, crate_root, opts, graph)
}

#[cfg(test)]
mod tests {
    use rocraters::ro_crate::graph_vector::GraphVector;
    use wdl::analysis::types::MapType;
    use wdl::analysis::types::PrimitiveType;
    use wdl::engine::Map;
    use wdl::engine::PrimitiveValue;
    use wdl::engine::Value;

    use super::*;

    /// Builds a `File` WDL value pointing at `path`.
    pub(super) fn file_value(path: &Path) -> Value {
        PrimitiveValue::new_file(path.to_str().unwrap()).into()
    }

    fn ids(graph: &[GraphVector]) -> Vec<String> {
        graph.iter().map(|g| g.get_id().clone()).collect()
    }

    #[test]
    fn primitive_becomes_property_value() {
        let opts = RoCrateOptions::from_flags(true, false, true, false);
        let mut graph = Vec::new();
        let v = Value::from("hello".to_string());
        let id = value_to_entities(
            "input",
            "greeting",
            &v,
            Path::new("/tmp"),
            &opts,
            &mut graph,
        )
        .unwrap();
        assert_eq!(id, "#input-greeting");
        assert!(ids(&graph).iter().any(|i| i == "#input-greeting"));
    }

    #[test]
    fn file_in_crate_root_is_referenced_in_place() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("out.txt");
        std::fs::write(&p, b"abc").unwrap();
        // no_checksums = true
        let opts = RoCrateOptions::from_flags(true, false, true, false);
        let mut graph = Vec::new();
        let v = file_value(&p);
        let id = value_to_entities("output", "f", &v, dir.path(), &opts, &mut graph).unwrap();
        assert_eq!(id, "out.txt");
        let json = serde_json::to_string(&graph).unwrap();
        assert!(json.contains("\"File\""));
        assert!(json.contains("contentSize"));
        assert!(!json.contains("sha256"));
    }

    #[test]
    fn localizes_local_input_by_copy() {
        let src_dir = tempfile::tempdir().unwrap();
        let src = src_dir.path().join("reads.bam");
        std::fs::write(&src, b"BAM").unwrap();
        let crate_dir = tempfile::tempdir().unwrap();
        // localize on
        let opts = RoCrateOptions::from_flags(true, false, false, false);
        let mut graph = Vec::new();
        let id = value_to_entities_roled(
            "input",
            "inputs",
            "aligner.reads",
            &file_value(&src),
            crate_dir.path(),
            &opts,
            &mut graph,
        )
        .unwrap();
        assert_eq!(id, "inputs/aligner.reads/reads.bam");
        assert!(
            crate_dir.path().join(&id).exists(),
            "file copied into crate"
        );
    }

    #[test]
    fn no_localize_uses_external_placeholder_not_abs_path() {
        let src_dir = tempfile::tempdir().unwrap();
        let src = src_dir.path().join("reads.bam");
        std::fs::write(&src, b"BAM").unwrap();
        let crate_dir = tempfile::tempdir().unwrap();
        // no_localize = true
        let opts = RoCrateOptions::from_flags(true, false, false, true);
        let mut graph = Vec::new();
        let id = value_to_entities_roled(
            "input",
            "inputs",
            "aligner.reads",
            &file_value(&src),
            crate_dir.path(),
            &opts,
            &mut graph,
        )
        .unwrap();
        assert!(id.starts_with("external/inputs/"), "got `{id}`");
        assert!(
            !id.contains(src_dir.path().to_str().unwrap()),
            "must not embed abs path"
        );
        let json = serde_json::to_string(&graph).unwrap();
        assert!(json.contains("contentLocation"));
    }

    #[test]
    fn external_placeholder_ids_are_stable() {
        let id = external_placeholder_id("inputs", "input.file", "artifact.bin");

        assert_eq!(
            id,
            "external/inputs/0196bfc88e0b2be3c90cabc955115006e0e05db28b4362304d53c06fc6a9193c/artifact.bin"
        );
    }

    #[test]
    fn localization_sanitizes_traversal_components() -> Result<()> {
        let src_dir = tempfile::tempdir()?;
        let src = src_dir.path().join("reads.bam");
        std::fs::write(&src, b"BAM")?;
        let crate_dir = tempfile::tempdir()?;
        let opts = RoCrateOptions::from_flags(true, false, false, false);
        let mut graph = Vec::new();

        let id = value_to_entities_roled(
            "input",
            "inputs",
            "samples/../../escape",
            &file_value(&src),
            crate_dir.path(),
            &opts,
            &mut graph,
        )?;

        assert_eq!(id, "inputs/samples/%2e%2e/%2e%2e/escape/reads.bam");
        assert!(crate_dir.path().join(&id).exists());
        assert!(!crate_dir.path().join("escape/reads.bam").exists());

        Ok(())
    }

    #[test]
    fn localization_sanitizes_map_keys() -> Result<()> {
        let src_dir = tempfile::tempdir()?;
        let src = src_dir.path().join("reads.bam");
        std::fs::write(&src, b"BAM")?;
        let crate_dir = tempfile::tempdir()?;
        let opts = RoCrateOptions::from_flags(true, false, false, false);
        let value = Value::from(Map::new(
            MapType::new(PrimitiveType::String, PrimitiveType::File),
            [("../../../outside".to_string(), file_value(&src))],
        )?);
        let mut graph = Vec::new();

        value_to_entities(
            "input",
            "samples",
            &value,
            crate_dir.path(),
            &opts,
            &mut graph,
        )?;

        let id = ids(&graph)
            .into_iter()
            .find(|id| id.starts_with("inputs/samples/") && id.ends_with("/reads.bam"))
            .ok_or_else(|| anyhow::anyhow!("localized file entity was not emitted"))?;
        assert!(
            !Path::new(&id)
                .components()
                .any(|component| matches!(component, std::path::Component::ParentDir))
        );
        assert!(crate_dir.path().join(id).exists());
        if let Some(parent) = crate_dir.path().parent() {
            assert!(!parent.join("outside/reads.bam").exists());
        }

        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn localization_rejects_symlink_files() -> Result<()> {
        let src_dir = tempfile::tempdir()?;
        let target = src_dir.path().join("target.bam");
        std::fs::write(&target, b"BAM")?;
        let link = src_dir.path().join("link.bam");
        std::os::unix::fs::symlink(&target, &link)?;
        let crate_dir = tempfile::tempdir()?;
        let opts = RoCrateOptions::from_flags(true, false, false, false);
        let mut graph = Vec::new();

        let Err(err) = value_to_entities(
            "input",
            "reads",
            &file_value(&link),
            crate_dir.path(),
            &opts,
            &mut graph,
        ) else {
            anyhow::bail!("symlink localization unexpectedly succeeded");
        };

        assert!(
            err.chain()
                .any(|cause| cause.to_string().contains("cannot localize symlink"))
        );
        assert!(!crate_dir.path().join("inputs/reads/link.bam").exists());

        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn localization_rejects_symlinks_inside_directories() -> Result<()> {
        let src_dir = tempfile::tempdir()?;
        let input_dir = src_dir.path().join("input");
        std::fs::create_dir(&input_dir)?;
        let target = src_dir.path().join("target.txt");
        std::fs::write(&target, b"secret")?;
        std::os::unix::fs::symlink(&target, input_dir.join("link.txt"))?;
        let crate_dir = tempfile::tempdir()?;
        let opts = RoCrateOptions::from_flags(true, false, false, false);
        let mut graph = Vec::new();
        let input_dir = input_dir
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("input directory path was not utf-8"))?;

        let Err(err) = value_to_entities_roled(
            "input",
            "inputs",
            "dataset",
            &PrimitiveValue::new_directory(input_dir).into(),
            crate_dir.path(),
            &opts,
            &mut graph,
        ) else {
            anyhow::bail!("directory symlink localization unexpectedly succeeded");
        };

        assert!(
            err.chain()
                .any(|cause| cause.to_string().contains("cannot localize symlink"))
        );
        assert!(
            !crate_dir
                .path()
                .join("inputs/dataset/input/link.txt")
                .exists()
        );

        Ok(())
    }

    #[test]
    fn directory_part_entities_reports_read_errors() -> Result<()> {
        let missing_dir = tempfile::tempdir()?.path().join("missing");
        let opts = RoCrateOptions::from_flags(true, false, false, false);
        let mut graph = Vec::new();

        let Err(err) =
            directory_part_entities("inputs/dataset/input", &missing_dir, &opts, &mut graph)
        else {
            anyhow::bail!("directory part generation unexpectedly succeeded");
        };

        assert!(err.to_string().contains("reading localized directory"));
        assert!(graph.is_empty());

        Ok(())
    }

    #[test]
    fn value_to_entities_rejects_unknown_prefixes() {
        let opts = RoCrateOptions::from_flags(true, false, false, false);
        let mut graph = Vec::new();
        let Err(err) = value_to_entities(
            "parameter",
            "item",
            &Value::from("value".to_string()),
            Path::new("/tmp"),
            &opts,
            &mut graph,
        ) else {
            panic!("unknown prefix unexpectedly succeeded");
        };

        assert!(err.to_string().contains("unknown ro-crate value prefix"));
        assert!(graph.is_empty());
    }

    #[test]
    fn localized_directory_id_has_trailing_slash() -> Result<()> {
        let src_dir = tempfile::tempdir()?;
        std::fs::write(src_dir.path().join("a.txt"), b"a")?;
        let crate_dir = tempfile::tempdir()?;
        let opts = RoCrateOptions::from_flags(true, false, false, false);
        let mut graph = Vec::new();

        let id = data_entity(
            src_dir.path().to_str().unwrap(),
            true,
            "inputs",
            "d",
            crate_dir.path(),
            &opts,
            &mut graph,
        )?;

        assert!(id.ends_with('/'), "directory id should end with `/`: {id}");
        let json = serde_json::to_string(&graph).unwrap();
        assert!(json.contains("Dataset"));
        assert!(
            json.contains("a.txt"),
            "localized child file should be listed"
        );
        Ok(())
    }

    #[test]
    fn external_directory_is_not_traversed() -> Result<()> {
        let src_dir = tempfile::tempdir()?;
        std::fs::write(src_dir.path().join("secret.txt"), b"s")?;
        let crate_dir = tempfile::tempdir()?;
        // no_localize = true
        let opts = RoCrateOptions::from_flags(true, false, false, true);
        let mut graph = Vec::new();

        let id = data_entity(
            src_dir.path().to_str().unwrap(),
            true,
            "inputs",
            "d",
            crate_dir.path(),
            &opts,
            &mut graph,
        )?;

        assert!(id.starts_with("external/") && id.ends_with('/'));
        let json = serde_json::to_string(&graph).unwrap();
        assert!(
            !json.contains("secret.txt"),
            "external directory contents must not be enumerated"
        );
        assert!(!json.contains("hasPart"));
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn in_crate_symlink_escape_is_rejected() -> Result<()> {
        let outside = tempfile::tempdir()?;
        let secret = outside.path().join("secret.txt");
        std::fs::write(&secret, b"x")?;
        let crate_dir = tempfile::tempdir()?;
        let link = crate_dir.path().join("link.txt");
        std::os::unix::fs::symlink(&secret, &link)?;
        let opts = RoCrateOptions::from_flags(true, false, false, false);
        let mut graph = Vec::new();

        let result = data_entity(
            link.to_str().unwrap(),
            false,
            "outputs",
            "o",
            crate_dir.path(),
            &opts,
            &mut graph,
        );
        assert!(
            result.is_err(),
            "a symlink in the crate root that escapes it must be rejected"
        );
        Ok(())
    }
}
