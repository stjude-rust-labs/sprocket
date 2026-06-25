//! Maps WDL interface declarations to RO-Crate `FormalParameter` entities.

use std::collections::HashMap;

use rocraters::ro_crate::constraints::DataType;
use rocraters::ro_crate::constraints::EntityValue;
use rocraters::ro_crate::contextual_entity::ContextualEntity;
use rocraters::ro_crate::graph_vector::GraphVector;
use wdl::analysis::types::Optional;
use wdl::analysis::types::PrimitiveType;
use wdl::analysis::types::Type;

/// Returns a coarse, human-readable type term for a WDL type.
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
    if ty.as_array().is_some() {
        return "Array".to_string();
    }
    if ty.as_map().is_some() || ty.as_struct().is_some() {
        return "Object".to_string();
    }
    if ty.as_pair().is_some() {
        return "Array".to_string();
    }
    "Text".to_string()
}

/// Builds the `dynamic_entity` map from `(key, value)` pairs.
fn bag(pairs: Vec<(&str, EntityValue)>) -> Option<HashMap<String, EntityValue>> {
    Some(pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

/// Builds a `FormalParameter` contextual entity for a named declaration.
pub fn formal_parameter(id: &str, name: &str, ty: &Type) -> GraphVector {
    GraphVector::ContextualEntity(ContextualEntity {
        id: id.to_string(),
        type_: DataType::Term("FormalParameter".to_string()),
        dynamic_entity: bag(vec![
            ("name", EntityValue::EntityString(name.to_string())),
            (
                "additionalType",
                EntityValue::EntityString(additional_type(ty)),
            ),
            ("valueRequired", EntityValue::EntityBool(!ty.is_optional())),
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
        let gv = formal_parameter("#param-x", "x", &ty);
        assert_eq!(gv.get_id().as_str(), "#param-x");
        let json = serde_json::to_string(&gv).unwrap();
        assert!(json.contains("FormalParameter"));
        assert!(json.contains("\"name\":\"x\""));
    }
}
