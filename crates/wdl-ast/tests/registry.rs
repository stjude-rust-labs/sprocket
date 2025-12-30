//! The AST node registry.
//!
//! The AST node registry was introduced only to ensure that all nodes in the
//! concrete syntax tree have one and _only_ one analogous AST entity.
//!
//! The reason this is important to ensure statically is because this assumption
//! of one-to-one mapping between elements within the two types of tree is
//! relied upon in downstream crates. For example, formatting works by
//! traversing the CST of a WDL document and attempting to cast a node to any
//! AST type that can then be recursively formatted.
//!
//! Furthermore, this is just a good invariant to uphold to ensure in general in
//! that the code remains straightforward to reason about (a CST element that
//! can map to multiple AST elements in different contexts is inherently
//! confusing).

use std::any::type_name;
use std::collections::HashMap;
use std::sync::LazyLock;

use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::Comment;
use wdl_ast::Ident;
use wdl_ast::SyntaxKind;
use wdl_ast::SyntaxNode;
use wdl_ast::SyntaxToken;
use wdl_ast::Version;
use wdl_ast::VersionStatement;
use wdl_ast::Whitespace;
use wdl_ast::v1;
use wdl_grammar::ALL_SYNTAX_KIND;

/// A private module for sealed traits.
///
/// The traits are sealed because we want to ensure that we reserve the right to
/// implement them in the future unhindered without introducing breaking
/// changes.
mod private {
    /// The sealed trait for [`AstNodeRegistrant`](super::AstNodeRegistrant).
    pub trait SealedNode {}

    /// The sealed trait for [`AstTokenRegistrant`](super::AstTokenRegistrant).
    pub trait SealedToken {}
}

/// A registry of all known mappings between AST elements (individual Rust types
/// that implement the [`AstNode`] trait or [`AstToken`] trait) and the CST
/// elements they can be cast from (via [`SyntaxKind`]\(s)).
///
/// This is useful for ensuring that AST elements have a one-to-one mapping with
/// CST element kinds.
static REGISTRY: LazyLock<HashMap<&'static str, Box<[SyntaxKind]>>> = LazyLock::new(|| {
    let types = vec![
        Comment::register(),
        Ident::register(),
        v1::AccessExpr::register(),
        v1::AdditionExpr::register(),
        v1::AfterKeyword::register(),
        v1::AliasKeyword::register(),
        v1::ArrayType::register(),
        v1::ArrayTypeKeyword::register(),
        v1::AsKeyword::register(),
        v1::Assignment::register(),
        v1::Ast::register(),
        v1::Asterisk::register(),
        v1::BooleanTypeKeyword::register(),
        v1::BoundDecl::register(),
        v1::CallAfter::register(),
        v1::CallAlias::register(),
        v1::CallExpr::register(),
        v1::CallInputItem::register(),
        v1::CallKeyword::register(),
        v1::CallStatement::register(),
        v1::CallTarget::register(),
        v1::CloseBrace::register(),
        v1::CloseBracket::register(),
        v1::CloseHeredoc::register(),
        v1::CloseParen::register(),
        v1::Colon::register(),
        v1::Comma::register(),
        v1::CommandKeyword::register(),
        v1::CommandSection::register(),
        v1::CommandText::register(),
        v1::ConditionalStatement::register(),
        v1::ConditionalStatementClause::register(),
        v1::DefaultOption::register(),
        v1::DirectoryTypeKeyword::register(),
        v1::DivisionExpr::register(),
        v1::Dot::register(),
        v1::DoubleQuote::register(),
        v1::ElseKeyword::register(),
        v1::EnvKeyword::register(),
        v1::Equal::register(),
        v1::EqualityExpr::register(),
        v1::Exclamation::register(),
        v1::Exponentiation::register(),
        v1::ExponentiationExpr::register(),
        v1::FalseKeyword::register(),
        v1::FileTypeKeyword::register(),
        v1::Float::register(),
        v1::FloatTypeKeyword::register(),
        v1::Greater::register(),
        v1::GreaterEqual::register(),
        v1::GreaterEqualExpr::register(),
        v1::GreaterExpr::register(),
        v1::HintsKeyword::register(),
        v1::IfExpr::register(),
        v1::IfKeyword::register(),
        v1::ImportAlias::register(),
        v1::ImportKeyword::register(),
        v1::ImportStatement::register(),
        v1::IndexExpr::register(),
        v1::InequalityExpr::register(),
        v1::InKeyword::register(),
        v1::InputKeyword::register(),
        v1::InputSection::register(),
        v1::Integer::register(),
        v1::IntTypeKeyword::register(),
        v1::Less::register(),
        v1::LessEqual::register(),
        v1::LessEqualExpr::register(),
        v1::LessExpr::register(),
        v1::LiteralArray::register(),
        v1::LiteralBoolean::register(),
        v1::LiteralFloat::register(),
        v1::LiteralHints::register(),
        v1::LiteralHintsItem::register(),
        v1::LiteralInput::register(),
        v1::LiteralInputItem::register(),
        v1::LiteralInteger::register(),
        v1::LiteralMap::register(),
        v1::LiteralMapItem::register(),
        v1::LiteralNone::register(),
        v1::LiteralNull::register(),
        v1::LiteralObject::register(),
        v1::LiteralObjectItem::register(),
        v1::LiteralOutput::register(),
        v1::LiteralOutputItem::register(),
        v1::LiteralPair::register(),
        v1::LiteralString::register(),
        v1::LiteralStruct::register(),
        v1::LiteralStructItem::register(),
        v1::LogicalAnd::register(),
        v1::LogicalAndExpr::register(),
        v1::LogicalNotExpr::register(),
        v1::LogicalOr::register(),
        v1::LogicalOrExpr::register(),
        v1::MapType::register(),
        v1::MapTypeKeyword::register(),
        v1::MetadataArray::register(),
        v1::MetadataObject::register(),
        v1::MetadataObjectItem::register(),
        v1::MetadataSection::register(),
        v1::MetaKeyword::register(),
        v1::Minus::register(),
        v1::ModuloExpr::register(),
        v1::MultiplicationExpr::register(),
        v1::NameRefExpr::register(),
        v1::NegationExpr::register(),
        v1::NoneKeyword::register(),
        v1::NotEqual::register(),
        v1::NullKeyword::register(),
        v1::ObjectKeyword::register(),
        v1::ObjectType::register(),
        v1::ObjectTypeKeyword::register(),
        v1::OpenBrace::register(),
        v1::OpenBracket::register(),
        v1::OpenHeredoc::register(),
        v1::OpenParen::register(),
        v1::OutputKeyword::register(),
        v1::OutputSection::register(),
        v1::PairType::register(),
        v1::PairTypeKeyword::register(),
        v1::ParameterMetadataSection::register(),
        v1::ParameterMetaKeyword::register(),
        v1::ParenthesizedExpr::register(),
        v1::Percent::register(),
        v1::Placeholder::register(),
        v1::PlaceholderOpen::register(),
        v1::Plus::register(),
        v1::PrimitiveType::register(),
        v1::QuestionMark::register(),
        v1::RequirementsItem::register(),
        v1::RequirementsKeyword::register(),
        v1::RequirementsSection::register(),
        v1::RuntimeItem::register(),
        v1::RuntimeKeyword::register(),
        v1::RuntimeSection::register(),
        v1::ScatterKeyword::register(),
        v1::ScatterStatement::register(),
        v1::SepOption::register(),
        v1::SingleQuote::register(),
        v1::Slash::register(),
        v1::StringText::register(),
        v1::StringTypeKeyword::register(),
        v1::StructDefinition::register(),
        v1::StructKeyword::register(),
        v1::EnumDefinition::register(),
        v1::EnumKeyword::register(),
        v1::EnumTypeParameter::register(),
        v1::EnumVariant::register(),
        v1::SubtractionExpr::register(),
        v1::TaskDefinition::register(),
        v1::TaskHintsItem::register(),
        v1::TaskHintsSection::register(),
        v1::TaskKeyword::register(),
        v1::ThenKeyword::register(),
        v1::TrueFalseOption::register(),
        v1::TrueKeyword::register(),
        v1::TypeRef::register(),
        v1::UnboundDecl::register(),
        v1::Unknown::register(),
        v1::VersionKeyword::register(),
        v1::WorkflowDefinition::register(),
        v1::WorkflowHintsItem::register(),
        v1::WorkflowHintsSection::register(),
        v1::WorkflowHintsArray::register(),
        v1::WorkflowHintsObject::register(),
        v1::WorkflowHintsObjectItem::register(),
        v1::WorkflowKeyword::register(),
        Version::register(),
        VersionStatement::register(),
        Whitespace::register(),
    ];

    let mut result = HashMap::new();

    // NOTE: this is done this way instead of simply collecting into a
    // [`HashMap`] to ensure on the fly that no keys are duplicated.
    for (r#type, kinds) in types {
        if result.contains_key(&r#type) {
            panic!("the `{type:?}` key is duplicated");
        }

        result.insert(r#type, kinds);
    }

    result
});

/// Computes the inverse of the registry.
///
/// In other words, maps CST elements—dynamically typed as [`SyntaxKind`]s—to
/// the corresponding AST element(s) that can cast from them.
///
/// This is useful for ensuring that AST elements have a one-to-one mapping with
/// CST element kinds.
fn inverse() -> HashMap<SyntaxKind, Box<[&'static str]>> {
    let mut result = HashMap::<SyntaxKind, Vec<&'static str>>::new();

    for (key, values) in REGISTRY.iter() {
        for value in values.into_iter() {
            result.entry(value.to_owned()).or_default().push(*key);
        }
    }

    result
        .into_iter()
        .map(|(key, values)| (key, values.into_boxed_slice()))
        .collect()
}

trait AstNodeRegistrant: private::SealedNode {
    /// Returns the [`SyntaxKind`]\(s) that can be cast into this AST node type.
    fn register() -> (&'static str, Box<[SyntaxKind]>);
}

impl<T: AstNode<SyntaxNode> + 'static> private::SealedNode for T {}

impl<T: AstNode<SyntaxNode> + 'static> AstNodeRegistrant for T {
    fn register() -> (&'static str, Box<[SyntaxKind]>) {
        (
            type_name::<T>(),
            ALL_SYNTAX_KIND
                .iter()
                .filter(|kind| T::can_cast(**kind))
                .cloned()
                .collect::<Vec<_>>()
                .into_boxed_slice(),
        )
    }
}

trait AstTokenRegistrant: private::SealedToken {
    /// Returns the [`SyntaxKind`]\(s) that can be cast into this AST token
    /// type.
    fn register() -> (&'static str, Box<[SyntaxKind]>);
}

impl<T: AstToken<SyntaxToken> + 'static> private::SealedToken for T {}

impl<T: AstToken<SyntaxToken> + 'static> AstTokenRegistrant for T {
    fn register() -> (&'static str, Box<[SyntaxKind]>) {
        (
            type_name::<T>(),
            ALL_SYNTAX_KIND
                .iter()
                .filter(|kind| T::can_cast(**kind))
                .cloned()
                .collect::<Vec<_>>()
                .into_boxed_slice(),
        )
    }
}

/// This test ensures there is a one-to-one mapping between CST elements
/// ([`SyntaxKind`]\(s)) and AST elements (Rust types that implement
/// the [`AstNode`] trait or the [`AstToken`] trait).
///
/// The importance of this is described at the top of the module.
#[test]
fn ensures_one_to_one() {
    let mut missing = Vec::new();
    let mut multiple = Vec::new();

    let inverse_registry = inverse();

    for kind in ALL_SYNTAX_KIND {
        // NOTE: these are symbolic elements and should not be included in
        // the analysis here.
        if kind.is_symbolic() {
            continue;
        }

        match inverse_registry.get(kind) {
            // SAFETY: because this is an inverse registry, only
            // [`SyntaxKind`]s with at least one registered implementing
            // type would be registered here. Thus, by design of the
            // `inverse()` method, this will never occur.
            Some(values) if values.is_empty() => {
                unreachable!("the inverse registry should never contain an empty array")
            }
            Some(values) if values.len() > 1 => multiple.push((kind, values)),
            None => missing.push(kind),
            // NOTE: this is essentially only if the values exist and the
            // length is 1—in that case, there is a one to one mapping,
            // which is what we would like the case to be.
            _ => {}
        }
    }

    if !missing.is_empty() {
        let mut missing = missing
            .into_iter()
            .map(|kind| format!("{kind:?}"))
            .collect::<Vec<_>>();
        missing.sort();

        panic!(
            "detected `SyntaxKind`s without an associated `AstNode`/`AstToken` (n={}): {}",
            missing.len(),
            missing.join(", ")
        )
    }

    if !multiple.is_empty() {
        multiple.sort();
        let mut multiple = multiple
            .into_iter()
            .map(|(kind, types)| {
                let mut types = types.clone();
                types.sort();

                let mut result = format!("== {kind:?} ==");
                for r#type in types {
                    result.push_str("\n* ");
                    result.push_str(r#type);
                }

                result
            })
            .collect::<Vec<_>>();
        multiple.sort();

        panic!(
            "detected `SyntaxKind`s associated with multiple `AstNode`s/`AstToken`s (n={}):\n\n{}",
            multiple.len(),
            multiple.join("\n\n")
        )
    }
}
