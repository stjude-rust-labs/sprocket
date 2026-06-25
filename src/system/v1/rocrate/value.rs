//! Converts runtime WDL values into type-aware RO-Crate entities.

use std::collections::HashMap;
use std::path::Path;

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

/// Pushes a `File`/`Dataset` data entity for a path and returns its crate-
/// relative `@id`.
///
/// This is the non-localizing base; [`super`]'s Task 4A replaces it with
/// copy/download localization, the `inputs/`/`outputs/` and `external/` layout,
/// and directory per-file checksums.
fn data_entity(
    path_str: &str,
    is_dir: bool,
    crate_root: &Path,
    opts: &RoCrateOptions,
    graph: &mut Vec<GraphVector>,
) -> String {
    let path = Path::new(path_str);
    let id = path
        .strip_prefix(crate_root)
        .ok()
        .and_then(|p| p.to_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| path_str.to_string());
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&id)
        .to_string();

    let mut props = vec![("name", EntityValue::EntityString(name))];
    if !is_dir && let Ok(meta) = std::fs::metadata(path) {
        props.push(("contentSize", EntityValue::Entityi64(meta.len() as i64)));
    }
    if opts.checksums
        && !is_dir
        && let Ok(hex) = sha256_hex(path)
    {
        props.push(("sha256", EntityValue::EntityString(hex)));
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
    id
}

/// Recursively converts a value into an `EntityValue`, lifting any nested
/// `File`/`Directory` into its own data entity and returning an `EntityId`
/// reference to it.
fn value_to_entity_value(
    value: &Value,
    crate_root: &Path,
    opts: &RoCrateOptions,
    graph: &mut Vec<GraphVector>,
) -> EntityValue {
    if value.is_none() {
        return EntityValue::EntityNull(None);
    }
    if let Some(f) = value.as_file() {
        let id = data_entity(f.as_str(), false, crate_root, opts, graph);
        return EntityValue::EntityId(Id::Id(id));
    }
    if let Some(d) = value.as_directory() {
        let id = data_entity(d.as_str(), true, crate_root, opts, graph);
        return EntityValue::EntityId(Id::Id(id));
    }
    if let Some(b) = value.as_boolean() {
        return EntityValue::EntityBool(b);
    }
    if let Some(i) = value.as_integer() {
        return EntityValue::Entityi64(i);
    }
    if let Some(f) = value.as_float() {
        return EntityValue::Entityf64(f);
    }
    if let Some(s) = value.as_string() {
        return EntityValue::EntityString(s.to_string());
    }
    if let Some(arr) = value.as_array() {
        let items = arr
            .as_slice()
            .iter()
            .map(|v| value_to_entity_value(v, crate_root, opts, graph))
            .collect();
        return EntityValue::EntityVec(items);
    }
    if let Some(obj) = value.as_object() {
        let map = obj
            .iter()
            .map(|(k, v)| {
                (
                    k.to_string(),
                    value_to_entity_value(v, crate_root, opts, graph),
                )
            })
            .collect();
        return EntityValue::EntityObject(map);
    }
    if let Some(st) = value.as_struct() {
        let map = st
            .iter()
            .map(|(k, v)| {
                (
                    k.to_string(),
                    value_to_entity_value(v, crate_root, opts, graph),
                )
            })
            .collect();
        return EntityValue::EntityObject(map);
    }
    if let Some(m) = value.as_map() {
        let map = m
            .iter()
            .map(|(k, v)| {
                (
                    format!("{k}"),
                    value_to_entity_value(v, crate_root, opts, graph),
                )
            })
            .collect();
        return EntityValue::EntityObject(map);
    }
    if let Some(p) = value.as_pair() {
        let left = value_to_entity_value(p.left(), crate_root, opts, graph);
        let right = value_to_entity_value(p.right(), crate_root, opts, graph);
        return EntityValue::EntityVec(vec![left, right]);
    }
    // Hidden/Call/TypeNameRef and anything unexpected: fall back to display.
    EntityValue::EntityString(format!("{value}"))
}

/// Converts a named value into RO-Crate graph entities, appending File/Dataset
/// data entities to `graph` and returning the entity `@id` that represents the
/// value (a data-entity id for File/Directory, else a `PropertyValue` id).
pub fn value_to_entities(
    id_prefix: &str,
    name: &str,
    value: &Value,
    crate_root: &Path,
    opts: &RoCrateOptions,
    graph: &mut Vec<GraphVector>,
) -> String {
    // Top-level File/Directory: the value *is* the data entity.
    if let Some(f) = value.as_file() {
        return data_entity(f.as_str(), false, crate_root, opts, graph);
    }
    if let Some(d) = value.as_directory() {
        return data_entity(d.as_str(), true, crate_root, opts, graph);
    }

    // Everything else: a PropertyValue carrying the (possibly structured) value.
    let id = format!("#{id_prefix}-{name}");
    let entity_value = value_to_entity_value(value, crate_root, opts, graph);
    graph.push(GraphVector::ContextualEntity(ContextualEntity {
        id: id.clone(),
        type_: DataType::Term("PropertyValue".to_string()),
        dynamic_entity: bag(vec![
            ("name", EntityValue::EntityString(name.to_string())),
            ("value", entity_value),
        ]),
    }));
    id
}

#[cfg(test)]
mod tests {
    use rocraters::ro_crate::graph_vector::GraphVector;
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
        );
        assert_eq!(id, "#input-greeting");
        assert!(ids(&graph).iter().any(|i| i == "#input-greeting"));
    }

    #[test]
    fn file_becomes_data_entity_with_size_no_checksum_when_disabled() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("out.txt");
        std::fs::write(&p, b"abc").unwrap();
        // no_checksums = true
        let opts = RoCrateOptions::from_flags(true, false, true, false);
        let mut graph = Vec::new();
        let v = file_value(&p);
        let id = value_to_entities("output", "f", &v, dir.path(), &opts, &mut graph);
        assert_eq!(id, "out.txt");
        let json = serde_json::to_string(&graph).unwrap();
        assert!(json.contains("\"File\""));
        assert!(json.contains("contentSize"));
        assert!(!json.contains("sha256"));
    }
}
