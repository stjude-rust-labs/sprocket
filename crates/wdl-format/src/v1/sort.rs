//! Sorting logic for WDL elements.

use std::cmp::Ordering;

use wdl_ast::AstToken;
use wdl_ast::v1;
use wdl_ast::v1::PrimitiveType;

/// Define an ordering for declarations.
fn decl_index(decl: &v1::Decl) -> usize {
    match decl {
        v1::Decl::Bound(b) => {
            if b.ty().is_optional() {
                2
            } else {
                3
            }
        }
        v1::Decl::Unbound(u) => {
            if u.ty().is_optional() {
                1
            } else {
                0
            }
        }
    }
}

/// Defines an ordering for types.
fn type_index(ty: &v1::Type) -> usize {
    match ty {
        v1::Type::Map(_) => 6,
        v1::Type::Array(a) => {
            if a.is_non_empty() {
                2
            } else {
                3
            }
        }
        v1::Type::Pair(_) => 7,
        v1::Type::Object(_) => 5,
        v1::Type::Ref(_) => 4,
        v1::Type::Primitive(p) => match p.kind() {
            v1::PrimitiveTypeKind::Boolean => 9,
            v1::PrimitiveTypeKind::Integer => 11,
            v1::PrimitiveTypeKind::Float => 10,
            v1::PrimitiveTypeKind::String => 8,
            v1::PrimitiveTypeKind::Directory => 1,
            v1::PrimitiveTypeKind::File => 0,
        },
    }
}

/// Defines an ordering for PrimitiveTypes
fn primitive_type_index(ty: &PrimitiveType) -> usize {
    match ty.kind() {
        v1::PrimitiveTypeKind::Boolean => 3,
        v1::PrimitiveTypeKind::Integer => 5,
        v1::PrimitiveTypeKind::Float => 4,
        v1::PrimitiveTypeKind::String => 2,
        v1::PrimitiveTypeKind::File => 0,
        v1::PrimitiveTypeKind::Directory => 1,
    }
}

/// Compares the ordering of two map types.
fn compare_map_types(a: &v1::MapType, b: &v1::MapType) -> Ordering {
    let (akey, aty) = a.types();
    let (bkey, bty) = b.types();

    let cmp = primitive_type_index(&akey).cmp(&primitive_type_index(&bkey));
    if cmp != Ordering::Equal {
        return cmp;
    }

    let cmp = compare_types(&aty, &bty);
    if cmp != Ordering::Equal {
        return cmp;
    }

    // Optional check is inverted
    b.is_optional().cmp(&a.is_optional())
}

/// Compares the ordering of two array types.
fn compare_array_types(a: &v1::ArrayType, b: &v1::ArrayType) -> Ordering {
    let cmp = compare_types(&a.element_type(), &b.element_type());
    if cmp != Ordering::Equal {
        return cmp;
    }

    // Non-empty is inverted
    let cmp = b.is_non_empty().cmp(&a.is_non_empty());
    if cmp != Ordering::Equal {
        return cmp;
    }

    // Optional check is inverted
    b.is_optional().cmp(&a.is_optional())
}

/// Compares the ordering of two pair types.
fn compare_pair_types(a: &v1::PairType, b: &v1::PairType) -> Ordering {
    let (afirst, asecond) = a.types();
    let (bfirst, bsecond) = b.types();

    let cmp = compare_types(&afirst, &bfirst);
    if cmp != Ordering::Equal {
        return cmp;
    }

    let cmp = compare_types(&asecond, &bsecond);
    if cmp != Ordering::Equal {
        return cmp;
    }

    // Optional check is inverted
    b.is_optional().cmp(&a.is_optional())
}

/// Compares the ordering of two type references.
fn compare_type_refs(a: &v1::TypeRef, b: &v1::TypeRef) -> Ordering {
    let cmp = a.name().text().cmp(b.name().text());
    if cmp != Ordering::Equal {
        return cmp;
    }

    // Optional check is inverted
    b.is_optional().cmp(&a.is_optional())
}

/// Compares the ordering of two types.
fn compare_types(a: &v1::Type, b: &v1::Type) -> Ordering {
    // Check Array, Map, and Pair for sub-types
    match (a, b) {
        (v1::Type::Map(a), v1::Type::Map(b)) => compare_map_types(a, b),
        (v1::Type::Array(a), v1::Type::Array(b)) => compare_array_types(a, b),
        (v1::Type::Pair(a), v1::Type::Pair(b)) => compare_pair_types(a, b),
        (v1::Type::Ref(a), v1::Type::Ref(b)) => compare_type_refs(a, b),
        (v1::Type::Object(a), v1::Type::Object(b)) => {
            // Optional check is inverted
            b.is_optional().cmp(&a.is_optional())
        }
        _ => type_index(a).cmp(&type_index(b)),
    }
}

/// Compares two declarations for sorting.
pub fn compare_decl(a: &v1::Decl, b: &v1::Decl) -> Ordering {
    if (matches!(a, v1::Decl::Bound(_))
        && matches!(b, v1::Decl::Bound(_))
        && a.ty().is_optional() == b.ty().is_optional())
        || (matches!(a, v1::Decl::Unbound(_))
            && matches!(b, v1::Decl::Unbound(_))
            && a.ty().is_optional() == b.ty().is_optional())
    {
        compare_types(&a.ty(), &b.ty())
    } else {
        decl_index(a).cmp(&decl_index(b))
    }
}
