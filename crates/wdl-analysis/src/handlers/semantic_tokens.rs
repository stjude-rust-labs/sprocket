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
use line_index::LineCol;
use line_index::WideEncoding;
use lsp_types::SemanticToken;
use lsp_types::SemanticTokenModifier;
use lsp_types::SemanticTokenType;
use lsp_types::SemanticTokens;
use rowan::WalkEvent;
use url::Url;
use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::Comment;
use wdl_ast::CommentKind;
use wdl_ast::SupportedVersion;
use wdl_ast::SyntaxKind;
use wdl_ast::SyntaxNode;
use wdl_ast::SyntaxToken;
use wdl_ast::TreeToken;
use wdl_ast::v1::AccessExpr;
use wdl_ast::v1::CallExpr;
use wdl_ast::v1::CallTarget;
use wdl_ast::v1::EnumDefinition;
use wdl_ast::v1::EnumVariant;
use wdl_ast::v1::Expr;
use wdl_ast::v1::ImportStatement;
use wdl_ast::v1::LiteralStruct;
use wdl_ast::v1::LiteralStructItem;
use wdl_ast::v1::MetadataObjectItem;
use wdl_ast::v1::ParameterMetadataSection;
use wdl_ast::v1::RequirementsItem;
use wdl_ast::v1::RuntimeItem;
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
    SemanticTokenType::ENUM_MEMBER,
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
    SemanticTokenModifier::DEFAULT_LIBRARY,
    SemanticTokenModifier::DEFINITION,
    SemanticTokenModifier::DOCUMENTATION,
    SemanticTokenModifier::READONLY,
    SemanticTokenModifier::STATIC,
];

/// Handles a semantic token request for a full document.
///
/// It traverses the entire CST of the document, classifies each token
/// into a semantic type, and constructs the [`SemanticTokens`].
pub fn semantic_tokens(graph: &DocumentGraph, uri: &Url) -> Result<Option<SemanticTokens>> {
    let Some(index) = graph.get_index(uri) else {
        bail!("document `{uri}` not found in graph.");
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
        let Some((token_ty, token_modifiers_bitset)) = token_ty(&token, document) else {
            continue;
        };

        let start_pos = position(&lines, token.text_range().start())?;
        let end_pos = position(&lines, token.text_range().end())?;

        let token_type = WDL_SEMANTIC_TOKEN_TYPES
            .iter()
            .position(|tt| tt == &token_ty)
            .unwrap_or_else(|| {
                panic!("token type `{token_ty:?}` not found in `WDL_SEMANTIC_TOKEN_TYPES`")
            }) as u32;

        if start_pos.line == end_pos.line {
            let delta_line = start_pos.line - last_line;
            let delta_start = if delta_line == 0 {
                start_pos.character - last_start
            } else {
                start_pos.character
            };

            tokens.push(SemanticToken {
                delta_line,
                delta_start,
                length: end_pos.character - start_pos.character,
                token_type,
                token_modifiers_bitset,
            });

            last_line = start_pos.line;
            last_start = start_pos.character;
            continue;
        }

        // Tokens can't be multiline, need to split them up
        for line in start_pos.line..=end_pos.line {
            let Some(current_line_range) = lines.line(line) else {
                continue;
            };

            let Some(utf16_line_col) = lines.to_wide(
                WideEncoding::Utf16,
                LineCol {
                    line,
                    col: u32::from(current_line_range.len()),
                },
            ) else {
                continue;
            };

            let current_line_len_utf16 = utf16_line_col.col;

            let (char_start, length) = if line == start_pos.line {
                (
                    start_pos.character,
                    current_line_len_utf16 - start_pos.character,
                )
            } else if line == end_pos.line {
                (0, end_pos.character)
            } else {
                (0, current_line_len_utf16)
            };

            let delta_line = line - last_line;
            let delta_start = if delta_line == 0 {
                char_start - last_start
            } else {
                char_start
            };

            tokens.push(SemanticToken {
                delta_line,
                delta_start,
                length,
                token_type,
                token_modifiers_bitset,
            });

            last_line = line;
            last_start = char_start;
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
        && version >= SupportedVersion::V1(V1::Two)
        && kind == SyntaxKind::RuntimeKeyword
        && parent.kind() == SyntaxKind::RuntimeSectionNode
    {
        add_modifier(&mut modifiers, SemanticTokenModifier::DEPRECATED)
    }

    let ty = match kind {
        SyntaxKind::Comment => {
            let comment = Comment::cast(token.clone()).expect("should cast");
            if comment.kind() == CommentKind::Documentation {
                add_modifier(&mut modifiers, SemanticTokenModifier::DOCUMENTATION);
            }

            Some(SemanticTokenType::COMMENT)
        },
        SyntaxKind::LiteralStringText
        | SyntaxKind::SingleQuote
        | SyntaxKind::DoubleQuote => Some(SemanticTokenType::STRING),
        SyntaxKind::OpenHeredoc
        | SyntaxKind::CloseHeredoc if parent.kind() == SyntaxKind::LiteralStringNode => Some(SemanticTokenType::STRING),
        SyntaxKind::Integer
        | SyntaxKind::Float
        // The version may not be an actual number (e.g., 1.3), but it still
        // logically maps to a version number.
        | SyntaxKind::Version => Some(SemanticTokenType::NUMBER),
        k if k.is_keyword() => Some(SemanticTokenType::KEYWORD),
        k if k.is_operator() => Some(SemanticTokenType::OPERATOR),
        k if k.is_type() => Some(SemanticTokenType::TYPE),
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
    match parent.kind() {
        SyntaxKind::UnboundDeclNode | SyntaxKind::BoundDeclNode => {
            for ancestor in parent.ancestors() {
                match ancestor.kind() {
                    SyntaxKind::InputSectionNode => {
                        add_modifier(modifiers, SemanticTokenModifier::READONLY);
                        return Some(SemanticTokenType::PARAMETER);
                    }
                    SyntaxKind::StructDefinitionNode => return Some(SemanticTokenType::PROPERTY),
                    _ => {}
                }
            }

            return Some(SemanticTokenType::VARIABLE);
        }
        SyntaxKind::EnumVariantNode => {
            let variant = EnumVariant::cast(parent.clone()).expect("should cast");
            if variant.name().inner() == token {
                add_modifier(modifiers, SemanticTokenModifier::DEFINITION);
                return Some(SemanticTokenType::ENUM_MEMBER);
            }
        }
        SyntaxKind::TaskDefinitionNode => {
            let task = TaskDefinition::cast(parent.clone()).expect("should cast");
            if task.name().inner() == token {
                add_modifier(modifiers, SemanticTokenModifier::DEFINITION);
                return Some(SemanticTokenType::FUNCTION);
            }
        }
        SyntaxKind::WorkflowDefinitionNode => {
            let workflow = WorkflowDefinition::cast(parent.clone()).expect("should cast");
            if workflow.name().inner() == token {
                add_modifier(modifiers, SemanticTokenModifier::DEFINITION);
                return Some(SemanticTokenType::FUNCTION);
            }
        }
        SyntaxKind::StructDefinitionNode => {
            let struct_def = StructDefinition::cast(parent.clone()).expect("should cast");
            if struct_def.name().inner() == token {
                add_modifier(modifiers, SemanticTokenModifier::DEFINITION);
                return Some(SemanticTokenType::STRUCT);
            }
        }
        SyntaxKind::LiteralStructNode => {
            let struct_def = LiteralStruct::cast(parent.clone()).expect("should cast");
            if struct_def.name().inner() == token {
                return Some(SemanticTokenType::STRUCT);
            }
        }
        SyntaxKind::LiteralStructItemNode => {
            let (name, _) = LiteralStructItem::cast(parent.clone())
                .expect("should cast")
                .name_value();
            if name.inner() == token {
                return Some(SemanticTokenType::PROPERTY);
            }
        }
        SyntaxKind::EnumDefinitionNode => {
            let enum_def = EnumDefinition::cast(parent.clone()).expect("should cast");
            if enum_def.name().inner() == token {
                add_modifier(modifiers, SemanticTokenModifier::DEFINITION);
                return Some(SemanticTokenType::ENUM);
            }
        }
        SyntaxKind::MetadataObjectItemNode => {
            let metadata_item = MetadataObjectItem::cast(parent.clone()).expect("should cast");
            if metadata_item.name().inner() == token {
                add_modifier(modifiers, SemanticTokenModifier::STATIC);
                add_modifier(modifiers, SemanticTokenModifier::READONLY);
                return if metadata_item.parent::<ParameterMetadataSection>().is_some() {
                    Some(SemanticTokenType::PARAMETER)
                } else {
                    Some(SemanticTokenType::PROPERTY)
                };
            }
        }
        SyntaxKind::RequirementsItemNode => {
            let item = RequirementsItem::cast(parent.clone()).expect("should cast");
            if item.name().inner() == token {
                if token.text() == "docker" {
                    add_modifier(modifiers, SemanticTokenModifier::DEPRECATED)
                }

                add_modifier(modifiers, SemanticTokenModifier::READONLY);
                return Some(SemanticTokenType::PROPERTY);
            }
        }
        SyntaxKind::RuntimeItemNode => {
            let item = RuntimeItem::cast(parent.clone()).expect("should cast");
            if item.name().inner() == token {
                add_modifier(modifiers, SemanticTokenModifier::READONLY);
                return Some(SemanticTokenType::PROPERTY);
            }
        }
        SyntaxKind::TypeRefNode => {
            let ty_ref = TypeRef::cast(parent.clone()).expect("should cast");
            if ty_ref.name().inner() == token {
                if document.struct_by_name(token.text()).is_some() {
                    return Some(SemanticTokenType::STRUCT);
                }
                if document.enum_by_name(token.text()).is_some() {
                    return Some(SemanticTokenType::ENUM);
                }
                return Some(SemanticTokenType::TYPE);
            }
        }
        SyntaxKind::CallExprNode => {
            let c = CallExpr::cast(parent.clone()).expect("should cast");
            if c.target().inner() == token {
                add_modifier(modifiers, SemanticTokenModifier::DEFAULT_LIBRARY);
                return Some(SemanticTokenType::FUNCTION);
            }
        }
        SyntaxKind::CallTargetNode => {
            let ct = CallTarget::cast(parent.clone()).expect("should cast");
            let names: Vec<_> = ct.names().collect();
            if names.last().is_some_and(|n| n.inner() == token) {
                return Some(SemanticTokenType::FUNCTION);
            }

            return Some(SemanticTokenType::NAMESPACE);
        }
        _ => {
            if let Some(a) = parent.ancestors().find_map(AccessExpr::cast) {
                let (target, member) = a.operands();

                let ident_targets_rhs = member.inner() == token;
                if let Expr::NameRef(name_expr) = target.strip_parenthesized() {
                    let ident_targets_lhs = token == name_expr.name().inner();
                    if document.struct_by_name(name_expr.name().text()).is_some()
                        && ident_targets_lhs
                    {
                        return Some(SemanticTokenType::STRUCT);
                    } else if let Some(e) = document.enum_by_name(name_expr.name().text()) {
                        if ident_targets_lhs {
                            return Some(SemanticTokenType::ENUM);
                        } else if ident_targets_rhs
                            && e.definition()
                                .variants()
                                .any(|v| v.name().text() == token.text())
                        {
                            return Some(SemanticTokenType::ENUM_MEMBER);
                        }
                    }
                }

                if a.is_task_access() {
                    add_modifier(modifiers, SemanticTokenModifier::DEFAULT_LIBRARY);
                    add_modifier(modifiers, SemanticTokenModifier::READONLY);
                }

                if ident_targets_rhs {
                    return Some(SemanticTokenType::PROPERTY);
                }
            }

            if let Some(i) = parent.ancestors().find_map(ImportStatement::cast)
                && i.explicit_namespace().is_some_and(|ns| ns.inner() == token)
            {
                add_modifier(modifiers, SemanticTokenModifier::DECLARATION);
                return Some(SemanticTokenType::NAMESPACE);
            }
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
