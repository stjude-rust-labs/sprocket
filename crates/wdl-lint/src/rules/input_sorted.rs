//! A lint rule for sorting of inputs.

use std::cmp::Ordering;

use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::Diagnostic;
use wdl_ast::Diagnostics;
use wdl_ast::Document;
use wdl_ast::Span;
use wdl_ast::SupportedVersion;
use wdl_ast::SyntaxElement;
use wdl_ast::SyntaxKind;
use wdl_ast::VisitReason;
use wdl_ast::Visitor;
use wdl_ast::v1;
use wdl_ast::v1::PrimitiveType;

use crate::Rule;
use crate::Tag;
use crate::TagSet;

/// The identifier for the input not sorted rule.
const ID: &str = "InputSorted";

/// Creates a "input not sorted" diagnostic.
fn input_not_sorted(span: Span, sorted_inputs: String) -> Diagnostic {
    Diagnostic::note("input not sorted")
        .with_rule(ID)
        .with_label("input section must be sorted".to_string(), span)
        .with_fix(format!("sort input statements as: \n{}", sorted_inputs))
}

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
fn compare_decl(a: &v1::Decl, b: &v1::Decl) -> Ordering {
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

/// Detects unsorted input declarations.
#[derive(Default, Debug, Clone, Copy)]
pub struct InputSortedRule;

impl Rule for InputSortedRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures that input declarations are sorted."
    }

    fn explanation(&self) -> &'static str {
        "Each input declaration section should be sorted. This rule enforces an opinionated \
         sorting. First sorts by 1. required inputs, 2. optional inputs without defaults, 3. \
         optional inputs with defaults, and 4. inputs with a default value. Then by the type: 1. \
         File, 2. Array[*]+, 3. Array[*], 4. struct, 5. Object, 6. Map[*, *], 7. Pair[*, *], 8. \
         String, 9. Boolean, 10. Float, 11. Int. For ordering of the same compound type (Array[*], \
         Map[*, *], Pair[*, *]), drop the outermost type (Array, Map, etc.) and recursively apply \
         above sorting on the first inner type *, with ties broken by the second inner type. \
         Continue this pattern as far as possible. Once this ordering is satisfied, it is up to \
         the developer for final order of inputs of the same type."
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Style, Tag::Clarity, Tag::Sorting])
    }

    fn exceptable_nodes(&self) -> Option<&'static [wdl_ast::SyntaxKind]> {
        Some(&[
            SyntaxKind::VersionStatementNode,
            SyntaxKind::InputSectionNode,
        ])
    }

    fn related_rules(&self) -> &[&'static str] {
        &["ParameterMetaMatched"]
    }
}

impl Visitor for InputSortedRule {
    type State = Diagnostics;

    fn document(
        &mut self,
        _: &mut Self::State,
        reason: VisitReason,
        _: &Document,
        _: SupportedVersion,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        // Reset the visitor upon document entry
        *self = Default::default();
    }

    fn input_section(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        input: &v1::InputSection,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        // Get input section declarations
        let decls: Vec<_> = input.declarations().collect();
        let mut sorted_decls = decls.clone();
        sorted_decls.sort_by(compare_decl);

        let input_string: String = sorted_decls
            .clone()
            .into_iter()
            .map(|decl| decl.inner().text().to_string() + "\n")
            .collect::<String>();
        let mut errors = 0;

        decls
            .into_iter()
            .zip(sorted_decls)
            .for_each(|(decl, sorted_decl)| {
                if decl != sorted_decl {
                    errors += 1;
                }
            });
        if errors > 0 {
            let span = input
                .inner()
                .first_token()
                .expect("input section should have tokens")
                .text_range()
                .into();
            state.exceptable_add(
                input_not_sorted(span, input_string),
                SyntaxElement::from(input.inner().clone()),
                &self.exceptable_nodes(),
            );
        }
    }
}
