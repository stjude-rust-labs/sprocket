//! Handlers for `type definition` requests.
//!
//! This module implements the LSP "textDocument/typeDefinition"
//! functionality for WDL files.
//!
//! See: [LSP Specification](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#textDocument_typeDefinition)

use std::collections::HashSet;
use std::collections::VecDeque;

use line_index::LineIndex;
use lsp_types::Location;
use petgraph::graph::NodeIndex;
use url::Url;
use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::SyntaxNode;
use wdl_ast::SyntaxToken;
use wdl_ast::TreeNode;
use wdl_ast::TreeToken;
use wdl_ast::v1::AccessExpr;
use wdl_ast::v1::Decl;
use wdl_ast::v1::EnumDefinition;
use wdl_ast::v1::Expr;
use wdl_ast::v1::ImportAlias;
use wdl_ast::v1::ImportMember;
use wdl_ast::v1::ScatterStatement;
use wdl_ast::v1::Type as AstType;
use wdl_grammar::SyntaxKind;

use crate::Document;
use crate::SourcePosition;
use crate::SourcePositionEncoding;
use crate::document::EnumRef;
use crate::document::StructRef;
use crate::graph::DocumentGraph;
use crate::handlers::TypeEvalContext;
use crate::handlers::common::ensure_analyzed;
use crate::handlers::common::find_identifier_token_at_offset;
use crate::handlers::common::get_imported_doc_context;
use crate::handlers::common::location_from_span;
use crate::handlers::common::position_to_offset;
use crate::types::CompoundType;
use crate::types::CustomType;
use crate::types::Type;
use crate::types::v1::ExprTypeEvaluator;

/// Result of attempting to resolve a type definition directly from a syntax
/// node.
enum Resolution {
    /// The location was resolved.
    Resolved(Location),
    /// The node refers to a symbol indirectly, and will fallback to
    /// `goto_definition()`.
    Undetermined,
}

/// Finds the type definition location for an identifier at the given position.
pub fn type_definition(
    graph: &DocumentGraph,
    document_uri: &Url,
    position: SourcePosition,
    encoding: SourcePositionEncoding,
) -> anyhow::Result<Option<Location>> {
    let (doc_index, root, lines, document) = ensure_analyzed(graph, document_uri)?;

    let offset = position_to_offset(&lines, position, encoding)?;
    let Some(token) = find_identifier_token_at_offset(&root, offset) else {
        return Ok(None);
    };

    let parent_node = token.parent().expect("identifier has no parent");

    // First pass: Try to determine the type from the current context
    match resolve_type_at_node(graph, doc_index, document, &lines, &parent_node, &token)? {
        Some(Resolution::Resolved(loc)) => {
            return Ok(Some(loc));
        }
        None => {
            return Ok(None);
        }
        // We'll need to find the ident's definition and search again
        Some(Resolution::Undetermined) => {}
    }

    let Some(location) = super::goto_definition(graph, document_uri, position, encoding)? else {
        return Ok(None);
    };

    // Second pass: Try to determine the type from the defining context
    let (target_index, target_root, target_lines, target_doc) =
        ensure_analyzed(graph, &location.uri)?;

    let target_offset = position_to_offset(
        &target_lines,
        SourcePosition::new(location.range.start.line, location.range.start.character),
        SourcePositionEncoding::UTF16,
    )?;
    let Some(target_token) = find_identifier_token_at_offset(&target_root, target_offset) else {
        return Ok(None);
    };

    let target_parent = target_token.parent().expect("identifier has no parent");
    match resolve_type_at_node(
        graph,
        target_index,
        target_doc,
        &target_lines,
        &target_parent,
        &target_token,
    )? {
        Some(Resolution::Resolved(loc)) => Ok(Some(loc)),
        _ => Ok(None),
    }
}

/// Resolves a type definition directly from a syntax node and identifier token.
fn resolve_type_at_node(
    graph: &DocumentGraph,
    doc_index: NodeIndex,
    document: &Document,
    lines: &LineIndex,
    parent_node: &SyntaxNode,
    token: &SyntaxToken,
) -> anyhow::Result<Option<Resolution>> {
    match parent_node.kind() {
        // Already at the type
        SyntaxKind::EnumDefinitionNode | SyntaxKind::StructDefinitionNode => {
            location_from_span(&document.uri(), token.span(), lines)
                .map(|res| Some(Resolution::Resolved(res)))
        }
        SyntaxKind::TypeRefNode | SyntaxKind::LiteralStructNode => {
            resolve_type_location(document, token.text(), lines, graph, true)
                .map(|res| res.map(Resolution::Resolved))
        }
        SyntaxKind::NameRefExprNode => {
            if let Some(scope) = document.find_scope_by_position(token.span().start()) {
                if let Some(name) = scope.lookup(token.text()) {
                    return resolve_type(graph, doc_index, document, name.ty())
                        .map(|res| res.map(Resolution::Resolved));
                }

                if let Some(ty) = document.get_custom_type(token.text()) {
                    return resolve_type(graph, doc_index, document, ty)
                        .map(|res| res.map(Resolution::Resolved));
                }
            }

            resolve_type_location(document, token.text(), lines, graph, true)
                .map(|res| res.map(Resolution::Resolved))
        }
        SyntaxKind::AccessExprNode => {
            let access_expr = AccessExpr::cast(parent_node.clone()).expect("should cast");

            let (_, access_ident) = access_expr.operands();
            if access_ident.span() == token.span()
                && let Some(scope) = document.find_scope_by_position(parent_node.span().start())
            {
                let mut ctx = TypeEvalContext { scope, document };
                let mut evaluator = ExprTypeEvaluator::new(&mut ctx);
                let Some(ty) = evaluator.evaluate_expr(&Expr::Access(access_expr)) else {
                    return Ok(None);
                };

                return resolve_type(graph, doc_index, document, &ty)
                    .map(|res| res.map(Resolution::Resolved));
            }

            Ok(None)
        }
        SyntaxKind::ImportAliasNode => {
            let alias = ImportAlias::cast(parent_node.clone()).expect("should cast");
            let (_, target) = alias.names();
            resolve_type_location(document, target.text(), lines, graph, true)
                .map(|res| res.map(Resolution::Resolved))
        }
        SyntaxKind::ImportMemberNode => {
            let member = ImportMember::cast(parent_node.clone()).expect("should cast");
            let local_name = member.alias().unwrap_or_else(|| member.name());
            resolve_type_location(document, local_name.text(), lines, graph, true)
                .map(|res| res.map(Resolution::Resolved))
        }
        SyntaxKind::EnumChoiceNode => {
            let Some(enum_def) = parent_node.parent().and_then(EnumDefinition::cast) else {
                return Ok(None);
            };

            location_from_span(&document.uri(), enum_def.name().span(), lines)
                .map(|res| Some(Resolution::Resolved(res)))
        }
        SyntaxKind::UnboundDeclNode | SyntaxKind::BoundDeclNode => {
            let decl = Decl::cast(parent_node.clone()).expect("should cast");
            let AstType::Ref(ty) = decl.ty() else {
                return Ok(None);
            };

            resolve_type_location(document, ty.name().text(), lines, graph, true)
                .map(|res| res.map(Resolution::Resolved))
        }
        SyntaxKind::ScatterStatementNode => {
            let scatter = ScatterStatement::cast(parent_node.clone()).expect("should cast");
            if scatter.variable().span() != token.span() {
                return Ok(None);
            }
            let Some(scope_span) = scatter.braced_scope_span(false) else {
                return Ok(None);
            };
            let Some(scope) = document.find_scope_by_position(scope_span.start()) else {
                return Ok(None);
            };
            let Some(name) = scope.lookup(scatter.variable().text()) else {
                return Ok(None);
            };

            resolve_type(graph, doc_index, document, name.ty())
                .map(|res| res.map(Resolution::Resolved))
        }
        _ => Ok(Some(Resolution::Undetermined)),
    }
}

/// Attempts to resolve a defining struct or enum location from a [`Type`].
fn resolve_type(
    graph: &DocumentGraph,
    doc_index: NodeIndex,
    document: &Document,
    ty: &Type,
) -> anyhow::Result<Option<Location>> {
    /// Searches transitive dependencies of `start` to find a type via `lookup`.
    ///
    /// There's a chance the type may not be in the document's scope (e.g.,
    /// importing a task that returns an external struct). In that case, we'll
    /// need to manually walk through the dependencies and find it (as opposed
    /// to using `resolve_defining_{struct, enum}`).
    fn search_deps_for_type(
        graph: &DocumentGraph,
        start: NodeIndex,
        lookup: impl Fn(&Document) -> Option<wdl_ast::Span>,
    ) -> anyhow::Result<Option<Location>> {
        let mut visited = HashSet::from([start]);
        let mut queue: VecDeque<NodeIndex> = graph.dependencies(start).collect();

        while let Some(idx) = queue.pop_front() {
            if !visited.insert(idx) {
                continue;
            }

            let node = graph.get(idx);
            if let Some(dep_doc) = node.document()
                && let Some(span) = lookup(dep_doc)
                && let Some(dep_lines) = node.parse_state().lines()
            {
                return Ok(Some(location_from_span(node.uri(), span, dep_lines)?));
            }

            queue.extend(graph.dependencies(idx));
        }

        Ok(None)
    }

    let custom_ty = match ty {
        Type::Compound(CompoundType::Custom(custom_ty), _) => custom_ty,
        Type::TypeNameRef(ty_ref) => ty_ref.ty(),
        _ => return Ok(None),
    };

    match custom_ty {
        CustomType::Struct(struct_ty) => {
            let name = struct_ty.name().as_str();

            // Unfortunately, `CustomType` doesn't give us much info to work
            // with.
            //
            // This fast path will only work if the `document` both:
            //
            // * Has the expected type in scope (either defined locally or
            //   imported)
            // * And if imported, the type is not aliased
            //
            // In any other case, we'll have to fallback to
            // `search_deps_for_type()`.
            if let Some((def_doc, s)) = resolve_defining_struct(document, name)
                && s.ty().and_then(|t| t.as_struct()) == Some(struct_ty)
                && let Some(ctx) = get_imported_doc_context(&def_doc.uri(), graph)
            {
                return Ok(Some(location_from_span(ctx.uri, s.name_span(), ctx.lines)?));
            }

            // See the docs for `search_deps_for_type`
            search_deps_for_type(graph, doc_index, |dep_doc| {
                dep_doc.local_struct_by_name(name).map(|s| s.name_span())
            })
        }
        CustomType::Enum(enum_ty) => {
            let name = enum_ty.name();

            if let Some((def_doc, e)) = resolve_defining_enum(document, name)
                && e.ty().and_then(|t| t.as_enum()) == Some(enum_ty)
                && let Some(ctx) = get_imported_doc_context(&def_doc.uri(), graph)
            {
                return Ok(Some(location_from_span(ctx.uri, e.name_span(), ctx.lines)?));
            }

            search_deps_for_type(graph, doc_index, |dep_doc| {
                dep_doc.local_enum_by_name(name).map(|e| e.name_span())
            })
        }
    }
}

/// Attempts to resolve the definition location of a type by name.
///
/// This is shared between `textDocument/typeDefinition` and
/// `textDocument/definition` requests.
///
/// The difference between the two is in the case of aliased imports, where
/// `definition` requests will stop searching at the alias definition.
/// `typeDefinition` requests will follow aliases until it reaches the source
/// definition.
///
/// If there are no aliases, the behavior is identical between the two.
pub(super) fn resolve_type_location(
    analysis_doc: &Document,
    ident: &str,
    lines: &LineIndex,
    graph: &DocumentGraph,
    follow_aliases: bool,
) -> anyhow::Result<Option<Location>> {
    if let Some(enum_info) = analysis_doc.enum_by_name(ident) {
        match enum_info {
            EnumRef::Local(e) => {
                return Ok(Some(location_from_span(
                    &analysis_doc.uri(),
                    e.name_span(),
                    lines,
                )?));
            }
            EnumRef::Imported(e) => {
                if !follow_aliases && e.local_name() != e.name() {
                    // Returns the location where alias name was defined
                    return Ok(Some(location_from_span(
                        &analysis_doc.uri(),
                        enum_info.name_span(),
                        lines,
                    )?));
                }

                if let Some((def_doc, original_enum)) =
                    resolve_defining_enum(e.document(), e.name())
                    && let Some(ctx) = get_imported_doc_context(&def_doc.uri(), graph)
                {
                    return Ok(Some(location_from_span(
                        ctx.uri,
                        original_enum.name_span(),
                        ctx.lines,
                    )?));
                }
            }
        }
    }

    if let Some(struct_info) = analysis_doc.struct_by_name(ident) {
        match struct_info {
            StructRef::Local(s) => {
                return Ok(Some(location_from_span(
                    &analysis_doc.uri(),
                    s.name_span(),
                    lines,
                )?));
            }
            StructRef::Imported(s) => {
                if !follow_aliases && s.local_name() != s.name() {
                    // Returns the location where alias name was defined
                    return Ok(Some(location_from_span(
                        &analysis_doc.uri(),
                        struct_info.name_span(),
                        lines,
                    )?));
                }

                if let Some((def_doc, original_struct)) =
                    resolve_defining_struct(s.document(), s.name())
                    && let Some(ctx) = get_imported_doc_context(&def_doc.uri(), graph)
                {
                    return Ok(Some(location_from_span(
                        ctx.uri,
                        original_struct.name_span(),
                        ctx.lines,
                    )?));
                }
            }
        }
    }

    Ok(None)
}

/// Walks transitive struct imports to find the defining document.
pub(super) fn resolve_defining_struct<'a>(
    mut doc: &'a Document,
    mut name: &'a str,
) -> Option<(&'a Document, &'a crate::document::Struct)> {
    while let Some(struct_ref) = doc.struct_by_name(name) {
        match struct_ref {
            StructRef::Local(s) => return Some((doc, s)),
            StructRef::Imported(s) => {
                name = s.name();
                doc = s.document();
            }
        }
    }

    None
}

/// Walks transitive enum imports to find the defining document.
pub(super) fn resolve_defining_enum<'a>(
    mut doc: &'a Document,
    mut name: &'a str,
) -> Option<(&'a Document, &'a crate::document::Enum)> {
    while let Some(enum_ref) = doc.enum_by_name(name) {
        match enum_ref {
            EnumRef::Local(e) => return Some((doc, e)),
            EnumRef::Imported(e) => {
                name = e.name();
                doc = e.document();
            }
        }
    }

    None
}
