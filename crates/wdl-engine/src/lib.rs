//! Execution engine for Workflow Description Language (WDL) documents.

use std::sync::LazyLock;

use sysinfo::CpuRefreshKind;
use sysinfo::MemoryRefreshKind;
use sysinfo::System;
use wdl_analysis::Document;
use wdl_analysis::diagnostics::unknown_type;
use wdl_analysis::document::Enum;
use wdl_analysis::document::v1::infer_type_from_literal;
use wdl_analysis::types::CompoundType;
use wdl_analysis::types::CustomType;
use wdl_analysis::types::PrimitiveType;
use wdl_analysis::types::Type;
use wdl_analysis::types::TypeNameResolver;
use wdl_analysis::types::v1::AstTypeConverter;
use wdl_ast::AstToken as _;
use wdl_ast::Diagnostic;
use wdl_ast::Span;
use wdl_ast::TreeNode;
use wdl_ast::v1::Expr;
use wdl_ast::v1::LiteralExpr;

mod backend;
pub mod config;
pub mod diagnostics;
mod eval;
pub(crate) mod hash;
pub(crate) mod http;
mod inputs;
mod outputs;
pub mod path;
mod stdlib;
pub(crate) mod tree;
mod units;
mod value;

pub use backend::*;
pub use eval::*;
pub use inputs::*;
pub use outputs::*;
pub use units::*;
pub use value::*;

use crate::diagnostics::unknown_enum_variant;

/// One gibibyte (GiB) as a float.
///
/// This is defined as a constant as it's a commonly performed conversion.
const ONE_GIBIBYTE: f64 = 1024.0 * 1024.0 * 1024.0;

/// Resolves a type name from a document.
///
/// This function will import the type into the type cache if not already
/// cached.
fn resolve_type_name(document: &Document, name: &str, span: Span) -> Result<Type, Diagnostic> {
    document
        .struct_by_name(name)
        .map(|s| s.ty().expect("struct should have type").clone())
        .or_else(|| {
            document
                .enum_by_name(name)
                .map(|e| e.ty().expect("enum should have type").clone())
        })
        .ok_or_else(|| unknown_type(name, span))
}

/// Converts a V1 AST type to an analysis type.
fn convert_ast_type_v1<N: TreeNode>(
    document: &Document,
    ty: &wdl_ast::v1::Type<N>,
) -> Result<Type, Diagnostic> {
    /// Used to resolve a type name from a document.
    struct Resolver<'a>(&'a Document);

    impl TypeNameResolver for Resolver<'_> {
        fn resolve(&mut self, name: &str, span: Span) -> Result<Type, Diagnostic> {
            resolve_type_name(self.0, name, span)
        }
    }

    AstTypeConverter::new(Resolver(document)).convert_type(ty)
}

/// Checks that a provided type matches the literal expression type.
///
/// # Panics
///
/// Panics if the expression does not match the expected literal type.
macro_rules! expect_ty_match {
    ($expr:expr, $variant:ident($binding:ident), $type_name:expr) => {
        let Expr::Literal(LiteralExpr::$variant($binding)) = $expr else {
            panic!(
                "expected `{}` literal expression for `{}` type",
                $type_name, $type_name
            );
        };
    };
}

/// Extracts a literal value from an AST expression.
///
///
/// This method is used primarily to extract literal values directly from the
/// AST tree during the static evaluation of enum variant values.
///
/// Returns `None` if the value cannot be parsed as a literal value.
///
/// # Panics
///
/// Panics if any of the expressions do not match their expected literal type.
fn extract_literal_value(ty: &Type, expr: &Expr) -> Option<Value> {
    match ty {
        Type::Primitive(PrimitiveType::Boolean, _) => {
            expect_ty_match!(expr, Boolean(b), "Boolean");
            Some(Value::Primitive(crate::PrimitiveValue::Boolean(b.value())))
        }
        Type::Primitive(PrimitiveType::Integer, _) => {
            expect_ty_match!(expr, Integer(i), "Integer");
            Some(Value::Primitive(crate::PrimitiveValue::Integer(i.value()?)))
        }
        Type::Primitive(PrimitiveType::Float, _) => {
            expect_ty_match!(expr, Float(f), "Float");
            Some(Value::Primitive(crate::PrimitiveValue::Float(
                f.value()?.into(),
            )))
        }
        Type::Primitive(PrimitiveType::String, _) => {
            expect_ty_match!(expr, String(s), "String");
            Some(Value::Primitive(crate::PrimitiveValue::new_string(
                s.text()?.text(),
            )))
        }
        Type::Primitive(PrimitiveType::File, _) => {
            expect_ty_match!(expr, String(s), "File");
            Some(Value::Primitive(crate::PrimitiveValue::new_file(
                s.text()?.text(),
            )))
        }
        Type::Primitive(PrimitiveType::Directory, _) => {
            expect_ty_match!(expr, String(s), "Directory");
            Some(Value::Primitive(crate::PrimitiveValue::new_directory(
                s.text()?.text(),
            )))
        }
        Type::Compound(CompoundType::Array(inner), _) => {
            expect_ty_match!(expr, Array(arr), "Array");
            let element_type = inner.element_type();
            let elements: Option<Vec<Value>> = arr
                .elements()
                .map(|e| extract_literal_value(element_type, &e))
                .collect();
            Some(Value::Compound(crate::CompoundValue::Array(
                crate::Array::new(None, inner.clone(), elements?)
                    .expect("array construction should succeed"),
            )))
        }
        Type::Compound(CompoundType::Pair(inner), _) => {
            expect_ty_match!(expr, Pair(pair), "Pair");
            let (left_expr, right_expr) = pair.exprs();
            let left = extract_literal_value(inner.left_type(), &left_expr)?;
            let right = extract_literal_value(inner.right_type(), &right_expr)?;
            Some(Value::Compound(crate::CompoundValue::Pair(
                crate::Pair::new(None, ty.clone(), left, right)
                    .expect("pair construction should succeed"),
            )))
        }
        Type::Compound(CompoundType::Map(inner), _) => {
            expect_ty_match!(expr, Map(map), "Map");
            let key_type = inner.key_type();
            let value_type = inner.value_type();
            let entries: Option<Vec<(Value, Value)>> = map
                .items()
                .map(|item| {
                    let (key_expr, val_expr) = item.key_value();
                    let key = extract_literal_value(key_type, &key_expr)?;
                    let val = extract_literal_value(value_type, &val_expr)?;
                    Some((key, val))
                })
                .collect();
            Some(Value::Compound(crate::CompoundValue::Map(
                crate::Map::new(None, ty.clone(), entries?)
                    .expect("map construction should succeed"),
            )))
        }
        Type::Compound(CompoundType::Custom(CustomType::Struct(inner)), _) => {
            expect_ty_match!(expr, Struct(s), "Struct");
            let members: Option<indexmap::IndexMap<String, Value>> = s
                .items()
                .map(|item| {
                    let (name, val_expr) = item.name_value();
                    let name_str = name.text().to_string();
                    let member_type = inner
                        .members()
                        .get(&name_str)
                        .expect("member should exist in struct type");
                    let val = extract_literal_value(member_type, &val_expr)?;
                    Some((name_str, val))
                })
                .collect();
            Some(Value::Compound(crate::CompoundValue::Object(
                crate::Object::new(members?),
            )))
        }
        Type::Object | Type::OptionalObject => {
            expect_ty_match!(expr, Object(obj), "Object");
            let members: Option<indexmap::IndexMap<String, Value>> = obj
                .items()
                .map(|item| {
                    let (name, val_expr) = item.name_value();
                    let name_str = name.text().to_string();

                    // Infer the type from the literal expression and recursively extract value
                    let inferred_ty = infer_type_from_literal(&val_expr)?;
                    let val = extract_literal_value(&inferred_ty, &val_expr)?;
                    Some((name_str, val))
                })
                .collect();
            Some(Value::Compound(crate::CompoundValue::Object(
                crate::Object::new(members?),
            )))
        }
        _ => None,
    }
}

/// Resolves the value of an enum variant by looking up the variant's expression
/// in the AST and resolving it to its literal value.
///
/// # Panics
///
/// The function panics if the variant value cannot be parsed as a literal or if
/// the variant's value does not coerce to the enum's inner value type.
///
/// All of these should be caught by `wdl-analysis` checks.
pub(crate) fn resolve_enum_variant_value(
    r#enum: &Enum,
    variant_name: &str,
) -> Result<Value, Diagnostic> {
    // SAFETY: we can assume that any type associated with an [`Enum`] entry is
    // an [`EnumType`] at this point in analysis.
    let enum_ty = r#enum.ty().unwrap().as_enum().unwrap();

    let variant = r#enum
        .definition()
        .variants()
        .find(|variant| variant.name().text() == variant_name)
        .ok_or(unknown_enum_variant(enum_ty.name(), variant_name))?;

    // Get the variant's type from the analyzed enum type.
    let variant_ty = enum_ty
        .variants()
        .get(variant_name)
        .expect("variant should exist in type");

    let value = if let Some(value_expr) = variant.value() {
        // SAFETY: see the panic notice for this function.
        extract_literal_value(variant_ty, &value_expr).unwrap()
    } else {
        // NOTE: when no expression is provided, the default is the
        // variant name as a string.
        Value::Primitive(crate::PrimitiveValue::new_string(variant_name))
    };

    // SAFETY: see the panic notice for this function.
    Ok(value.coerce(None, enum_ty.inner_value_type()).unwrap())
}

/// Cached information about the host system.
static SYSTEM: LazyLock<System> = LazyLock::new(|| {
    let mut system = System::new();
    system.refresh_cpu_list(CpuRefreshKind::nothing());
    system.refresh_memory_specifics(MemoryRefreshKind::nothing().with_ram());
    system
});
