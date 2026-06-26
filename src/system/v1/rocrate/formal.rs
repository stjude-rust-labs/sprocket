//! Maps WDL interface declarations to RO-Crate `FormalParameter` entities.

use std::collections::HashMap;

use rocraters::ro_crate::constraints::DataType;
use rocraters::ro_crate::constraints::EntityValue;
use rocraters::ro_crate::constraints::Id;
use rocraters::ro_crate::contextual_entity::ContextualEntity;
use rocraters::ro_crate::graph_vector::GraphVector;
use wdl::analysis::types::PrimitiveType;
use wdl::analysis::types::Type;

/// Bioschemas `FormalParameter` profile that the parameter entities conform to.
const FORMAL_PARAMETER_PROFILE: &str =
    "https://bioschemas.org/profiles/FormalParameter/1.0-RELEASE";

/// Returns the `additionalType` term for a WDL type. Primitives map to
/// schema.org `DataType`s (with `File`/`Dataset` for `File`/`Directory` per
/// RO-Crate convention); compound values map to `PropertyValue`, the schema.org
/// type for structured values.
pub fn additional_type(ty: &Type) -> String {
    if let Some(p) = ty.as_primitive() {
        return match p {
            PrimitiveType::Boolean => "Boolean",
            PrimitiveType::Integer => "Integer",
            PrimitiveType::Float => "Float",
            PrimitiveType::String => "Text",
            PrimitiveType::File => "File",
            PrimitiveType::Directory => "Dataset",
        }
        .to_string();
    }
    // Arrays, pairs, maps, and structs are all structured values.
    "PropertyValue".to_string()
}

/// Builds the `dynamic_entity` map from `(key, value)` pairs.
fn bag(pairs: Vec<(&str, EntityValue)>) -> Option<HashMap<String, EntityValue>> {
    Some(pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

/// Builds a `FormalParameter` contextual entity for a named declaration.
pub fn formal_parameter(id: &str, name: &str, ty: &Type, value_required: bool) -> GraphVector {
    GraphVector::ContextualEntity(ContextualEntity {
        id: id.to_string(),
        type_: DataType::Term("FormalParameter".to_string()),
        dynamic_entity: bag(vec![
            ("name", EntityValue::EntityString(name.to_string())),
            (
                "additionalType",
                EntityValue::EntityString(additional_type(ty)),
            ),
            ("valueRequired", EntityValue::EntityBool(value_required)),
            (
                "conformsTo",
                EntityValue::EntityId(Id::Id(FORMAL_PARAMETER_PROFILE.to_string())),
            ),
        ]),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn additional_type_for_primitive_file() {
        let ty = Type::Primitive(PrimitiveType::File, false);
        assert_eq!(additional_type(&ty), "File");
    }

    #[test]
    fn formal_parameter_has_id_and_type() {
        let ty = Type::Primitive(PrimitiveType::String, false);
        let gv = formal_parameter("#param-x", "x", &ty, true);
        assert_eq!(gv.get_id().as_str(), "#param-x");
        let json = serde_json::to_string(&gv).unwrap();
        assert!(json.contains("FormalParameter"));
        assert!(json.contains("\"name\":\"x\""));
        assert!(json.contains("\"valueRequired\":true"));
    }
}
