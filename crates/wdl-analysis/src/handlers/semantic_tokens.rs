//! Handles semantic highlighting for WDL files.
//!
//! This module implements the LSP `textDocument/semanticTokens` functionality
//! for WDL files. It traverses the Concrete Syntax Tree (CST) and assigns
//! semantic types to tokens, enabling richer syntax highlighting in compatible
//! editors.
//!
//! See: [LSP Specification](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#textDocument_semanticTokens)

use anyhow::Result;
use anyhow::bail;
use lsp_types::SemanticToken;
use lsp_types::SemanticTokenModifier;
use lsp_types::SemanticTokenType;
use lsp_types::SemanticTokens;
use rowan::WalkEvent;
use url::Url;
use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::SupportedVersion;
use wdl_ast::SyntaxKind;
use wdl_ast::SyntaxNode;
use wdl_ast::SyntaxToken;
use wdl_ast::TreeToken;
use wdl_ast::v1::AccessExpr;
use wdl_ast::v1::CallExpr;
use wdl_ast::v1::CallTarget;
use wdl_ast::v1::Decl;
use wdl_ast::v1::EnumDefinition;
use wdl_ast::v1::ImportStatement;
use wdl_ast::v1::StructDefinition;
use wdl_ast::v1::TaskDefinition;
use wdl_ast::v1::TypeRef;
use wdl_ast::v1::WorkflowDefinition;
use wdl_ast::version::V1;

use crate::Document;
use crate::graph::DocumentGraph;
use crate::graph::ParseState;
use crate::handlers::common::position;
use crate::types::Type;

/// The supported semantic token types for WDL.
pub const WDL_SEMANTIC_TOKEN_TYPES: &[SemanticTokenType] = &[
    SemanticTokenType::KEYWORD,
    SemanticTokenType::VARIABLE,
    SemanticTokenType::PARAMETER,
    SemanticTokenType::FUNCTION,
    SemanticTokenType::PROPERTY, // expression members
    SemanticTokenType::STRUCT,
    SemanticTokenType::ENUM,
    SemanticTokenType::TYPE,
    SemanticTokenType::STRING,
    SemanticTokenType::NUMBER,
    SemanticTokenType::OPERATOR,
    SemanticTokenType::NAMESPACE, // aliases
    SemanticTokenType::COMMENT,
];

/// The supported semantic token modifiers for WDL
pub const WDL_SEMANTIC_TOKEN_MODIFIERS: &[SemanticTokenModifier] = &[
    SemanticTokenModifier::ASYNC,
    SemanticTokenModifier::DEPRECATED,
    SemanticTokenModifier::DECLARATION,
];

/// Handles a semantic token request for a full docuement.
///
/// It traverses the entire CST of the document, classifies each token
/// into a semantic type, and constructs the [`SemanticTokens`].
pub fn semantic_tokens(graph: &DocumentGraph, uri: &Url) -> Result<Option<SemanticTokens>> {
    let Some(index) = graph.get_index(uri) else {
        bail!("docuement `{uri}` not found in graph.");
    };

    let node = graph.get(index);
    let (root, lines) = match node.parse_state() {
        ParseState::Parsed { lines, root, .. } => {
            (SyntaxNode::new_root(root.clone()), lines.clone())
        }
        _ => bail!("document `{uri}` has not been parsed", uri = uri),
    };

    let Some(document) = node.document() else {
        bail!("document analysis data not available for {}", uri);
    };

    let mut tokens = Vec::new();
    let mut last_line = 0;
    let mut last_start = 0;

    for token in root.preorder_with_tokens().filter_map(|e| match e {
        WalkEvent::Enter(elem) => elem.into_token(),
        WalkEvent::Leave(_) => None,
    }) {
        if let Some((token_ty, token_modifiers_bitset)) = token_ty(&token, document) {
            let start_pos = position(&lines, token.text_range().start())?;
            let end_pos = position(&lines, token.text_range().end())?;

            let delta_line = start_pos.line - last_line;
            let delta_start = if delta_line == 0 {
                start_pos.character - last_start
            } else {
                start_pos.character
            };

            let length = end_pos.character - start_pos.character;

            let lsp_token = SemanticToken {
                delta_line,
                delta_start,
                length,
                token_type: WDL_SEMANTIC_TOKEN_TYPES
                    .iter()
                    .position(|tt| tt == &token_ty)
                    .unwrap_or_else(|| {
                        panic!(
                            "token type `{token_ty:?}` not found in `WDL_SEMANTIC_TOKEN_TYPES`"
                        )
                    }) as u32,
                token_modifiers_bitset,
            };

            tokens.push(lsp_token);

            last_line = start_pos.line;
            last_start = start_pos.character;
        }
    }

    Ok(Some(SemanticTokens {
        result_id: Some(document.id().to_string()),
        data: tokens,
    }))
}

/// Determines the [`SemanticTokenType`] for a given [`SyntaxKind`]
///
/// Handles simple classification based on `SyntaxKind` (e.g., comments,
/// strings, keywords) and delegates to `resolve_identifier_ty` for more
/// complex, context-aware classification of identifiers.
fn token_ty(token: &SyntaxToken, document: &Document) -> Option<(SemanticTokenType, u32)> {
    let kind = token.kind();
    let parent = token.parent()?;

    let mut modifiers = 0;
    if kind == SyntaxKind::ScatterKeyword && parent.kind() == SyntaxKind::ScatterStatementNode {
        add_modifier(&mut modifiers, SemanticTokenModifier::ASYNC)
    }

    if let Some(version) = document.version()
        && version == SupportedVersion::V1(V1::Two)
        && kind == SyntaxKind::RuntimeKeyword
        && parent.kind() == SyntaxKind::RuntimeSectionNode
    {
        add_modifier(&mut modifiers, SemanticTokenModifier::DEPRECATED)
    }

    if token.text() == "docker"
        && parent
            .ancestors()
            .any(|n| n.kind() == SyntaxKind::RuntimeSectionNode)
    {
        add_modifier(&mut modifiers, SemanticTokenModifier::DEPRECATED)
    }

    let ty = match kind {
        SyntaxKind::Comment => Some(SemanticTokenType::COMMENT),
        SyntaxKind::LiteralStringText
        | SyntaxKind::SingleQuote
        | SyntaxKind::DoubleQuote
        | SyntaxKind::OpenHeredoc
        | SyntaxKind::CloseHeredoc
        | SyntaxKind::LiteralCommandText => Some(SemanticTokenType::STRING),
        SyntaxKind::Integer | SyntaxKind::Float => Some(SemanticTokenType::NUMBER),
        k if k.is_keyword() => Some(SemanticTokenType::KEYWORD),
        k if k.is_operator() => Some(SemanticTokenType::OPERATOR),
        SyntaxKind::BooleanTypeKeyword
        | SyntaxKind::IntTypeKeyword
        | SyntaxKind::FloatTypeKeyword
        | SyntaxKind::StringTypeKeyword
        | SyntaxKind::FileTypeKeyword
        | SyntaxKind::DirectoryTypeKeyword
        | SyntaxKind::ArrayTypeKeyword
        | SyntaxKind::PairTypeKeyword
        | SyntaxKind::MapTypeKeyword
        | SyntaxKind::ObjectTypeKeyword => Some(SemanticTokenType::TYPE),
        SyntaxKind::Ident => resolve_identifier_ty(token, &parent, document, &mut modifiers),
        _ => None,
    };

    ty.map(|t| (t, modifiers))
}

/// Resolves the semantic type of an identifier token based on its context.
///
/// This inspects the identifier's parent and ancestor nodes in the CST to
/// determine its role, checking for:
/// 1. Definition sites (task, workflow, struct, variable/parameter
///    declarations).
/// 2. Type references.
/// 3. Function or task/workflow calls.
/// 4. Import namespace aliases.
/// 5. Member access expressions.
/// 6. If none of the above apply, it falls back to looking up the identifier in
///    the current scope to determine its type.
fn resolve_identifier_ty(
    token: &SyntaxToken,
    parent: &SyntaxNode,
    document: &Document,
    modifiers: &mut u32,
) -> Option<SemanticTokenType> {
    if let Some(t) = TaskDefinition::cast(parent.clone())
        && t.name().inner() == token
    {
        add_modifier(modifiers, SemanticTokenModifier::DECLARATION);
        return Some(SemanticTokenType::FUNCTION);
    }

    if let Some(w) = WorkflowDefinition::cast(parent.clone())
        && w.name().inner() == token
    {
        add_modifier(modifiers, SemanticTokenModifier::DECLARATION);
        return Some(SemanticTokenType::FUNCTION);
    }

    if let Some(s) = StructDefinition::cast(parent.clone())
        && s.name().inner() == token
    {
        add_modifier(modifiers, SemanticTokenModifier::DECLARATION);
        return Some(SemanticTokenType::STRUCT);
    }

    if let Some(e) = EnumDefinition::cast(parent.clone())
        && e.name().inner() == token
    {
        add_modifier(modifiers, SemanticTokenModifier::DECLARATION);
        return Some(SemanticTokenType::ENUM);
    }

    if let Some(d) = Decl::cast(parent.clone())
        && d.name().inner() == token
    {
        if parent
            .parent()
            .map(|p| p.kind() == SyntaxKind::InputSectionNode)
            .unwrap_or(false)
        {
            add_modifier(modifiers, SemanticTokenModifier::READONLY);
            return Some(SemanticTokenType::PARAMETER);
        } else {
            return Some(SemanticTokenType::VARIABLE);
        }
    }

    if let Some(ty_ref) = TypeRef::cast(parent.clone())
        && ty_ref.name().inner() == token
    {
        if document.struct_by_name(token.text()).is_some() {
            return Some(SemanticTokenType::STRUCT);
        }
        if document.enum_by_name(token.text()).is_some() {
            return Some(SemanticTokenType::ENUM);
        }
        return Some(SemanticTokenType::TYPE);
    }

    if let Some(c) = CallExpr::cast(parent.clone())
        && c.target().inner() == token
    {
        return Some(SemanticTokenType::FUNCTION);
    }

    if let Some(i) = parent.ancestors().find_map(ImportStatement::cast)
        && i.explicit_namespace().is_some_and(|ns| ns.inner() == token)
    {
        return Some(SemanticTokenType::NAMESPACE);
    }

    if let Some(ct) = CallTarget::cast(parent.clone()) {
        let names: Vec<_> = ct.names().collect();
        if names.len() > 1 && names[0].inner() == token {
            return Some(SemanticTokenType::NAMESPACE);
        }
        if names.last().is_some_and(|n| n.inner() == token) {
            return Some(SemanticTokenType::FUNCTION);
        }
    }

    if let Some(a) = parent.ancestors().find_map(AccessExpr::cast) {
        let (_, member) = a.operands();
        if member.inner() == token {
            return Some(SemanticTokenType::PROPERTY);
        }
    }

    // Fallback to scope lookup
    if let Some(scope) = document.find_scope_by_position(token.span().start())
        && let Some(name_info) = scope.lookup(token.text())
    {
        return match name_info.ty() {
            Type::Call(_) => Some(SemanticTokenType::VARIABLE),
            _ => {
                let offset = name_info.span().start().try_into().ok()?;
                let root = document.root();
                let def_token = root
                    .inner()
                    .token_at_offset(offset)
                    .find(|t| t.span() == name_info.span() && t.kind() == SyntaxKind::Ident)?;
                let def_parent = def_token.parent()?;
                if def_parent.kind() == SyntaxKind::UnboundDeclNode
                    && def_parent
                        .ancestors()
                        .any(|n| n.kind() == SyntaxKind::InputSectionNode)
                {
                    add_modifier(modifiers, SemanticTokenModifier::READONLY);
                    Some(SemanticTokenType::PARAMETER)
                } else {
                    Some(SemanticTokenType::VARIABLE)
                }
            }
        };
    }

    None
}

/// Adds a semantic token modifier to the bitset.
fn add_modifier(modifiers: &mut u32, modifier: SemanticTokenModifier) {
    if let Some(pos) = WDL_SEMANTIC_TOKEN_MODIFIERS
        .iter()
        .position(|m| m == &modifier)
    {
        *modifiers |= 1 << pos;
    }
}
