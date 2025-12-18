//! Handlers for `goto definition` requests.
//!
//! This module implements the LSP "textDocument/definition" functionality for
//! WDL files. It handles various types of symbol resolution including:
//!
//! - Local variables and declarations within scopes
//! - Type references to struct definitions
//! - Call targets (tasks and workflows)
//! - Import namespace identifiers
//! - Access expressions for struct members and call outputs
//! - Global symbols (structs, tasks, workflows) across documents
//!
//! See: [LSP Specification](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#textDocument_definition)

use std::sync::Arc;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use line_index::LineIndex;
use lsp_types::Location;
use url::Url;
use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::Span;
use wdl_ast::SyntaxKind;
use wdl_ast::SyntaxNode;
use wdl_ast::SyntaxToken;
use wdl_ast::TreeNode;
use wdl_ast::TreeToken;
use wdl_ast::v1;

use crate::SourcePosition;
use crate::SourcePositionEncoding;
use crate::document::Document;
use crate::graph::DocumentGraph;
use crate::graph::ParseState;
use crate::handlers::TypeEvalContext;
use crate::handlers::common::find_identifier_token_at_offset;
use crate::handlers::common::get_imported_doc_context;
use crate::handlers::common::location_from_span;
use crate::handlers::common::position_to_offset;
use crate::types::CustomType;
use crate::types::Type;
use crate::types::v1::ExprTypeEvaluator;

/// Finds the definition location for an identifier at the given position.
///
/// Searches the document and its imports for the definition of the
/// identifier at the specified position, returning the location if
/// found.
///
/// * If a definition is found for the identifier then a [`Location`] containing
///   the URI and range is returned wrapped in [`Some`].
///
/// * Else, [`None`] is returned.
pub fn goto_definition(
    graph: &DocumentGraph,
    document_uri: Url,
    position: SourcePosition,
    encoding: SourcePositionEncoding,
) -> Result<Option<Location>> {
    let index = graph
        .get_index(&document_uri)
        .ok_or_else(|| anyhow!("document `{uri}` not found in graph", uri = document_uri))?;

    let node = graph.get(index);
    let (root, lines) = match node.parse_state() {
        ParseState::Parsed { lines, root, .. } => {
            (SyntaxNode::new_root(root.clone()), lines.clone())
        }
        _ => bail!("document `{uri}` has not been parsed", uri = document_uri),
    };

    let Some(analysis_doc) = node.document() else {
        bail!("document analysis data not available for {}", document_uri);
    };

    let offset = position_to_offset(&lines, position, encoding)?;
    let Some(token) = find_identifier_token_at_offset(&root, offset) else {
        bail!("no identifier found at position");
    };

    let ident_text = token.text();
    let parent_node = token.parent().expect("identifier has not parent");

    // Context based resolution
    if let Some(location) = resolve_by_context(
        &parent_node,
        &token,
        analysis_doc,
        &document_uri,
        &lines,
        graph,
    )? {
        return Ok(Some(location));
    }

    // Scope based resolution
    if let Some(scope_ref) = analysis_doc.find_scope_by_position(token.span().start())
        && let Some(name_def) = scope_ref.lookup(ident_text)
    {
        if let Type::Call(_) = name_def.ty() {
            let def_offset = name_def.span().start().try_into()?;
            let def_token = root
                .token_at_offset(def_offset)
                .find(|t| t.span() == name_def.span() && t.kind() == SyntaxKind::Ident);

            if let Some(def_token) = def_token
                && let Some(call_stmt) = def_token
                    .parent_ancestors()
                    .find_map(v1::CallStatement::cast)
                && call_stmt.alias().is_none()
            {
                // NOTE: implicit alias found, resolving call target instead of the
                // alias
                let target = call_stmt.target();
                let callee_name = target.names().last().expect("call target must have a name");
                return resolve_call_target(
                    target.inner(),
                    callee_name.inner(),
                    analysis_doc,
                    &document_uri,
                    &lines,
                    graph,
                );
            }
        }

        return Ok(Some(location_from_span(
            &document_uri,
            name_def.span(),
            &lines,
        )?));
    }

    // Global resolution
    resolve_global_identifier(analysis_doc, ident_text, &document_uri, &lines, graph)
}

/// Resolves identifier definition based on their parent node's syntax kind.
fn resolve_by_context(
    parent_node: &SyntaxNode,
    token: &SyntaxToken,
    analysis_doc: &Document,
    document_uri: &Url,
    lines: &Arc<LineIndex>,
    graph: &DocumentGraph,
) -> Result<Option<Location>> {
    match parent_node.kind() {
        SyntaxKind::TypeRefNode | SyntaxKind::LiteralStructNode => {
            resolve_type_reference(analysis_doc, token, document_uri, lines, graph)
        }

        SyntaxKind::CallTargetNode => {
            resolve_call_target(parent_node, token, analysis_doc, document_uri, lines, graph)
        }
        SyntaxKind::ImportStatementNode => {
            resolve_import_namespace(parent_node, token, document_uri, lines)
        }

        SyntaxKind::AccessExprNode => {
            resolve_access_expression(parent_node, token, analysis_doc, document_uri, lines, graph)
        }

        SyntaxKind::UnboundDeclNode => {
            resolve_decl_definition::<v1::UnboundDecl>(parent_node, token, document_uri, lines)
        }

        SyntaxKind::BoundDeclNode => {
            resolve_decl_definition::<v1::BoundDecl>(parent_node, token, document_uri, lines)
        }

        SyntaxKind::EnumVariantNode => {
            resolve_enum_variant_definition(parent_node, token, document_uri, lines)
        }

        SyntaxKind::LiteralStructItemNode => resolve_struct_literal_item(
            parent_node,
            token,
            analysis_doc,
            document_uri,
            lines,
            graph,
        ),

        SyntaxKind::CallInputItemNode => {
            resolve_call_input_item(parent_node, token, analysis_doc, document_uri, lines, graph)
        }

        // This case is handled by scope resolution.
        SyntaxKind::NameRefExprNode => Ok(None),
        _ => Ok(None),
    }
}

/// Resolves type references to their definition locations.
///
/// Searches for struct definitions in the current document and imported
/// namespaces.
fn resolve_type_reference(
    analysis_doc: &Document,
    token: &SyntaxToken,
    document_uri: &Url,
    lines: &Arc<LineIndex>,
    graph: &DocumentGraph,
) -> Result<Option<Location>> {
    let ident_text = token.text();

    if let Some(enum_info) = analysis_doc.enum_by_name(ident_text) {
        if enum_info.namespace().is_none() {
            return Ok(Some(location_from_span(
                document_uri,
                enum_info.name_span(),
                lines,
            )?));
        }

        // Return the location in the imported file.
        let ns_name = enum_info.namespace().unwrap();

        if let Some(ctx) = get_imported_doc_context(ns_name, analysis_doc, graph)
            && let Some(original_enum) = ctx.doc.enum_by_name(ident_text)
        {
            return Ok(Some(location_from_span(
                ctx.uri,
                original_enum.name_span(),
                ctx.lines,
            )?));
        }
    }

    if let Some(struct_info) = analysis_doc.struct_by_name(ident_text) {
        if struct_info.namespace().is_none() {
            // Handle struct defined in local document.
            return Ok(Some(location_from_span(
                document_uri,
                struct_info.name_span(),
                lines,
            )?));
        }

        let is_aliased_import = struct_info
            .ty()
            .and_then(|t| t.as_struct())
            .map(|st| st.name().as_str() != ident_text)
            .unwrap_or(false);

        if is_aliased_import {
            // Returns the location where alias import was defined.
            return Ok(Some(location_from_span(
                document_uri,
                struct_info.name_span(),
                lines,
            )?));
        } else {
            // Return the location in the imported file.
            let ns_name = struct_info.namespace().unwrap();

            if let Some(ctx) = get_imported_doc_context(ns_name, analysis_doc, graph)
                && let Some(original_struct) = ctx.doc.struct_by_name(ident_text)
            {
                return Ok(Some(location_from_span(
                    ctx.uri,
                    original_struct.name_span(),
                    ctx.lines,
                )?));
            }
        }
    }

    // Fallback search in case the struct is not in the current document's analysis
    // map
    for (_, ns) in analysis_doc.namespaces() {
        let node = graph.get(graph.get_index(ns.source()).unwrap());
        let Some(imported_doc) = node.document() else {
            continue;
        };

        let Some(struct_info) = imported_doc.struct_by_name(ident_text) else {
            continue;
        };

        let imported_lines = node.parse_state().lines().unwrap();
        return Ok(Some(location_from_span(
            ns.source(),
            struct_info.name_span(),
            imported_lines,
        )?));
    }

    Err(anyhow!(
        "could not resolve type reference for `{}`",
        ident_text
    ))
}

/// Resolves call targets to their definition locations.
///
/// Handles both local and namespaced function calls, resolving them to task
/// and workflow definition in the current document or imported
/// namespaces.
fn resolve_call_target(
    parent_node: &SyntaxNode,
    token: &SyntaxToken,
    analysis_doc: &Document,
    document_uri: &Url,
    lines: &Arc<LineIndex>,
    graph: &DocumentGraph,
) -> Result<Option<Location>> {
    let target = wdl_ast::v1::CallTarget::cast(parent_node.clone()).unwrap();
    let target_names: Vec<_> = target.names().collect();
    let is_callee_name_clicked = target_names
        .last()
        .is_some_and(|n| n.span() == token.span());

    if is_callee_name_clicked {
        let callee_name_str = token.text();

        // NOTE: Namespaced (foo.bar)
        if target_names.len() == 2 {
            let namespaced_name_str = target_names.first().unwrap().text();
            let Some(ns_info) = analysis_doc.namespace(namespaced_name_str) else {
                return Ok(None);
            };

            let node = graph.get(graph.get_index(ns_info.source()).unwrap());
            let Some(imported_doc) = node.document() else {
                return Ok(None);
            };
            let imported_lines = node.parse_state().lines().unwrap();

            if let Some(task_def) = imported_doc.task_by_name(callee_name_str) {
                return Ok(Some(location_from_span(
                    ns_info.source(),
                    task_def.name_span(),
                    imported_lines,
                )?));
            }

            if let Some(wf_def) = imported_doc
                .workflow()
                .filter(|w| w.name() == callee_name_str)
            {
                return Ok(Some(location_from_span(
                    ns_info.source(),
                    wf_def.name_span(),
                    imported_lines,
                )?));
            }
        } else if target_names.len() == 1 {
            // NOTE: Local calls
            if let Some(task_def) = analysis_doc.task_by_name(callee_name_str) {
                return Ok(Some(location_from_span(
                    document_uri,
                    task_def.name_span(),
                    lines,
                )?));
            }

            if let Some(wf_def) = analysis_doc
                .workflow()
                .filter(|w| w.name() == callee_name_str)
            {
                return Ok(Some(location_from_span(
                    document_uri,
                    wf_def.name_span(),
                    lines,
                )?));
            }
        } else {
            // More than 2 names (e.g. foo.bar.baz) - invalid expression.
            return Ok(None);
        }
    } else if let Some(ns_info) = analysis_doc.namespace(token.text()) {
        return Ok(Some(location_from_span(
            document_uri,
            ns_info.span(),
            lines,
        )?));
    }

    Ok(None)
}

/// Resolves import namespace identifier to their definition locations.
fn resolve_import_namespace(
    parent_node: &SyntaxNode,
    token: &SyntaxToken,
    document_uri: &Url,
    lines: &Arc<LineIndex>,
) -> Result<Option<Location>> {
    let import_stmt = wdl_ast::v1::ImportStatement::cast(parent_node.clone()).unwrap();
    let ident_text = token.text();

    if import_stmt
        .explicit_namespace()
        .is_some_and(|ns_ident| ns_ident.text() == ident_text)
    {
        return Ok(Some(location_from_span(document_uri, token.span(), lines)?));
    }

    Ok(None)
}

/// Searches for global definitions(structs, tasks, workflows) in the
/// current document and all imported namespaces.
fn resolve_global_identifier(
    analysis_doc: &Document,
    ident_text: &str,
    document_uri: &Url,
    lines: &Arc<LineIndex>,
    graph: &DocumentGraph,
) -> Result<Option<Location>> {
    if let Some(location) =
        find_global_definition_in_doc(analysis_doc, ident_text, document_uri, lines)?
    {
        return Ok(Some(location));
    }

    for (_, ns) in analysis_doc.namespaces() {
        // SAFETY: we know `get_index` will return `Some` as `ns.source` comes from
        // `analysis_doc.namespaces` which only contains namespaces for documents that
        // are guaranteed to be present in the graph.
        let node = graph.get(graph.get_index(ns.source()).unwrap());
        let Some(imported_doc) = node.document() else {
            continue;
        };

        // SAFETY: we know `lines` will return Some as we only reach here when
        // `node.document` is fully parsed.
        let imported_lines = node.parse_state().lines().unwrap();

        if let Some(location) = find_global_definition_in_doc(
            imported_doc,
            ident_text,
            ns.source().as_ref(),
            imported_lines,
        )? {
            return Ok(Some(location));
        }
    }

    Ok(None)
}

/// Resolves access expressions to their member definition locations.
///
/// Evaluates the target expression's type and resolves member access to the
/// appropriate definition location.
///
/// # Supports:
/// - Struct member access (`person.name`)
/// - enum member access (`Person.name`)
/// - Call output access (`call_result.output`)
/// - Arrays (persons[0].name)
/// - Chained Access Expressions (documents.persons[0].address.street)
fn resolve_access_expression(
    parent_node: &SyntaxNode,
    token: &SyntaxToken,
    analysis_doc: &Document,
    document_uri: &Url,
    lines: &Arc<LineIndex>,
    graph: &DocumentGraph,
) -> Result<Option<Location>> {
    // SAFETY: we already checked `parent_node.kind()` is
    // `SyntaxKind::AccessExprNode` in the `resolve_by_context` before
    // calling this function.
    let access_expr = wdl_ast::v1::AccessExpr::cast(parent_node.clone()).unwrap();
    let (target_expr, variant_ident) = access_expr.operands();

    if variant_ident.span() != token.span() {
        return Ok(None);
    }

    let scope = analysis_doc
        .find_scope_by_position(parent_node.span().start())
        .context("could not find scope for access expression")?;

    // Check if target is a namespace reference
    if let v1::Expr::NameRef(name_ref) = &target_expr {
        let name = name_ref.name();
        let name = name.text();
        if let Some(ns) = analysis_doc.namespace(name) {
            let member_name = variant_ident.text();
            if analysis_doc
                .enums()
                .any(|(_, e)| e.namespace() == Some(name) && e.name() == member_name)
            {
                let imported_node = graph.get(graph.get_index(ns.source()).unwrap());
                let imported_lines = imported_node.parse_state().lines().unwrap();
                let imported_doc = imported_node.document().unwrap();

                if let Some(original_enum) = imported_doc.enum_by_name(member_name) {
                    return Ok(Some(location_from_span(
                        ns.source(),
                        original_enum.name_span(),
                        imported_lines,
                    )?));
                }
            }
        }
    }

    let mut ctx = TypeEvalContext {
        scope,
        document: analysis_doc,
    };

    let mut evaluator = ExprTypeEvaluator::new(&mut ctx);
    let target_type = evaluator
        .evaluate_expr(&target_expr)
        .unwrap_or(crate::types::Type::Union);

    if let Some(struct_ty) = target_type.as_struct() {
        let original_struct_name = struct_ty.name().as_str();

        // Check for struct definition in imported namespaces.
        for (_, ns) in analysis_doc.namespaces() {
            // SAFETY: `ns.source` comes from a `analysis_doc.namespaces` which only
            // contains namespaces for documents that guaranteed to be present in
            // the graph.
            let node = graph.get(graph.get_index(ns.source()).unwrap());

            let Some(imported_doc) = node.document() else {
                continue;
            };

            let Some(original_struct) = imported_doc.struct_by_name(original_struct_name) else {
                continue;
            };

            // Only process original structs without namespaces.
            if original_struct.namespace().is_some() {
                continue;
            };

            // SAFETY: we know `lines` will return Some as we only reach here when
            // `node.document` is fully parsed and in `ParsedState::Parse`
            // state.
            let imported_lines = node.parse_state().lines().unwrap();

            let struct_node =
                v1::StructDefinition::cast(SyntaxNode::new_root(original_struct.node().clone()))
                    .expect("should cast to struct definition");

            if let Some(member) = struct_node
                .members()
                .find(|m| m.name().text() == variant_ident.text())
            {
                let member_span = member.name().span();
                let span = Span::new(
                    member_span.start() + original_struct.offset(),
                    member_span.len(),
                );
                return Ok(Some(location_from_span(ns.source(), span, imported_lines)?));
            }
        }

        // Check for struct definition in local document.
        let struct_def = analysis_doc
            .struct_by_name(struct_ty.name())
            .ok_or_else(|| {
                anyhow!(
                    "definition not found for struct `{name}`",
                    name = struct_ty.name()
                )
            })?;

        let (uri, def_lines) = match struct_def.namespace() {
            Some(ns_name) => {
                // SAFETY: `namespace` returns `Some` only when struct was imported from a
                // namespace that exists in the document.
                let ns = analysis_doc.namespace(ns_name).unwrap();

                // SAFETY: `ns.source` comes from a valid namespace entry which guarantees the
                // document exists in the graph.
                let imported_node = graph.get(graph.get_index(ns.source()).unwrap());

                // SAFETY: we successfully got the document above, it's in
                // `ParseState::Parsed` which always has a valid lines field.
                let lines = imported_node.parse_state().lines().unwrap();
                (ns.source().as_ref(), lines)
            }
            None => (document_uri, lines),
        };

        let struct_node =
            v1::StructDefinition::cast(SyntaxNode::new_root(struct_def.node().clone()))
                .expect("should cast to struct definition");

        let Some(member) = struct_node
            .members()
            .find(|m| m.name().text() == variant_ident.text())
        else {
            return Ok(None);
        };

        let member_span = member.name().span();
        let span = Span::new(member_span.start() + struct_def.offset(), member_span.len());
        // Returns found struct member definition location.
        return Ok(Some(location_from_span(uri, span, def_lines)?));
    }

    if let Type::TypeNameRef(CustomType::Enum(enum_ty)) = target_type {
        let original_enum_name = enum_ty.name().as_str();

        // Check for enum definition in imported namespaces.
        for (_, ns) in analysis_doc.namespaces() {
            // SAFETY: `ns.source` comes from a `analysis_doc.namespaces` which only
            // contains namespaces for documents that guaranteed to be present in
            // the graph.
            let node = graph.get(graph.get_index(ns.source()).unwrap());

            let Some(imported_doc) = node.document() else {
                continue;
            };

            let Some(original_enum) = imported_doc.enum_by_name(original_enum_name) else {
                continue;
            };

            // Only process original enums without namespaces.
            if original_enum.namespace().is_some() {
                continue;
            };

            // SAFETY: we know `lines` will return Some as we only reach here when
            // `node.document` is fully parsed and in `ParsedState::Parse`
            // state.
            let imported_lines = node.parse_state().lines().unwrap();

            let enum_node =
                v1::EnumDefinition::cast(SyntaxNode::new_root(original_enum.node().clone()))
                    .expect("should cast to enum definition");

            if let Some(variant) = enum_node
                .variants()
                .find(|v| v.name().text() == variant_ident.text())
            {
                let variant_span = variant.name().span();
                let span = Span::new(
                    variant_span.start() + original_enum.offset(),
                    variant_span.len(),
                );
                return Ok(Some(location_from_span(ns.source(), span, imported_lines)?));
            }
        }

        // Check for enum definition in local document.
        let enum_def = analysis_doc
            .enum_by_name(enum_ty.name())
            .ok_or_else(|| {
                anyhow!(
                    "definition not found for enum `{name}`",
                    name = enum_ty.name()
                )
            })?;

        let (uri, def_lines) = match enum_def.namespace() {
            Some(ns_name) => {
                // SAFETY: `namespace` returns `Some` only when enum was imported from a
                // namespace that exists in the document.
                let ns = analysis_doc.namespace(ns_name).unwrap();

                // SAFETY: `ns.source` comes from a valid namespace entry which guarantees the
                // document exists in the graph.
                let imported_node = graph.get(graph.get_index(ns.source()).unwrap());

                // SAFETY: we successfully got the document above, it's in
                // `ParseState::Parsed` which always has a valid lines field.
                let lines = imported_node.parse_state().lines().unwrap();
                (ns.source().as_ref(), lines)
            }
            None => (document_uri, lines),
        };

        let enum_node = v1::EnumDefinition::cast(SyntaxNode::new_root(enum_def.node().clone()))
            .expect("should cast to enum definition");

        let Some(variant) = enum_node
            .variants()
            .find(|v| v.name().text() == variant_ident.text())
        else {
            return Ok(None);
        };

        let variant_span = variant.name().span();
        let span = Span::new(variant_span.start() + enum_def.offset(), variant_span.len());
        // Returns found enum variant definition location.
        return Ok(Some(location_from_span(uri, span, def_lines)?));
    }

    if let Type::TypeNameRef(CustomType::Struct(_)) = target_type {
        todo!("handle struct member access via `TypeNameRef`")
    }

    if let Some(call_ty) = target_type.as_call() {
        let Some(output) = call_ty.outputs().get(variant_ident.text()) else {
            // Call output not found for the requested member.
            return Ok(None);
        };

        let (uri, callee_lines) = match call_ty.namespace() {
            Some(ns_name) => {
                // SAFETY: `namespace` returns `Some` only when the call type references
                // a namespace that exists in document.
                let ns = analysis_doc.namespace(ns_name).unwrap();

                // SAFETY: `ns.source` comes from a valid namespace entry which guarantees the
                // document exists in the graph.
                let imported_node = graph.get(graph.get_index(ns.source()).unwrap());

                // SAFETY: we successfully got the document above, it's in
                // `ParseState::Parsed` which always has a valid lines field.
                let lines = imported_node.parse_state().lines().unwrap();
                (ns.source().as_ref(), lines)
            }
            None => (document_uri, lines),
        };

        // Returns found call output definition location.
        return Ok(Some(location_from_span(
            uri,
            output.name_span(),
            callee_lines,
        )?));
    }

    if let Some(enum_ty) = target_type.as_enum() {
        // Check for enum definition in local document.
        let enum_def = analysis_doc
            .enum_by_name(enum_ty.name())
            .ok_or_else(|| {
                anyhow!(
                    "definition not found for enum `{name}`",
                    name = enum_ty.name()
                )
            })?;

        let (uri, def_lines) = match enum_def.namespace() {
            Some(ns_name) => {
                // SAFETY: `namespace` returns `Some` only when enum was imported from a
                // namespace that exists in the document.
                let ns = analysis_doc.namespace(ns_name).unwrap();

                // SAFETY: `ns.source` comes from a valid namespace entry which guarantees the
                // document exists in the graph.
                let imported_node = graph.get(graph.get_index(ns.source()).unwrap());

                // SAFETY: we successfully got the document above, it's in
                // `ParseState::Parsed` which always has a valid lines field.
                let lines = imported_node.parse_state().lines().unwrap();
                (ns.source().as_ref(), lines)
            }
            None => (document_uri, lines),
        };

        let enum_node = enum_def.definition();

        let Some(variant) = enum_node
            .variants()
            .find(|v| v.name().text() == variant_ident.text())
        else {
            return Ok(None);
        };

        let variant_span = variant.name().span();
        let span = Span::new(variant_span.start() + enum_def.offset(), variant_span.len());
        // Returns found enum variant definition location.
        return Ok(Some(location_from_span(uri, span, def_lines)?));
    }

    Ok(None)
}

/// Resolve declaration declarations to themselves.
fn resolve_decl_definition<T>(
    parent_node: &SyntaxNode,
    token: &SyntaxToken,
    document_uri: &Url,
    lines: &Arc<LineIndex>,
) -> Result<Option<Location>>
where
    T: AstNode<SyntaxNode> + 'static,
{
    let Some(decl_node) = T::cast(parent_node.clone()) else {
        return Ok(None);
    };

    let ident = v1::Decl::cast(decl_node.inner().clone())
        .expect("casting should succeed")
        .name();
    if ident.span() == token.span() {
        return Ok(Some(location_from_span(document_uri, token.span(), lines)?));
    }

    Ok(None)
}

/// Resolve enum variant declarations to themselves.
fn resolve_enum_variant_definition(
    parent_node: &SyntaxNode,
    token: &SyntaxToken,
    document_uri: &Url,
    lines: &Arc<LineIndex>,
) -> Result<Option<Location>> {
    let Some(variant_node) = v1::EnumVariant::cast(parent_node.clone()) else {
        return Ok(None);
    };

    let ident = variant_node.name();
    if ident.span() == token.span() {
        return Ok(Some(location_from_span(document_uri, token.span(), lines)?));
    }

    Ok(None)
}

/// Resolve struct literal item references to struct member definitions.
///
/// for example: Person p = Person { name: "..."}
///                                  ^^^^
fn resolve_struct_literal_item(
    parent_node: &SyntaxNode,
    token: &SyntaxToken,
    analysis_doc: &Document,
    document_uri: &Url,
    lines: &Arc<LineIndex>,
    graph: &DocumentGraph,
) -> Result<Option<Location>> {
    let Some(struct_item) = wdl_ast::v1::LiteralStructItem::cast(parent_node.clone()) else {
        bail!("cannot cast to `LiteralStructItem`");
    };
    let (name, _expr) = struct_item.name_value();

    // Verify that the user clicked on the specific identifier we're trying to
    // resolve.
    //
    // e.g., in `name: some_variable`, both `name` and `some_variable` are
    // identifiers.
    if name.span() != token.span() {
        return Ok(None);
    }

    let literal_struct = parent_node
        .parent()
        .and_then(wdl_ast::v1::LiteralStruct::cast)
        .ok_or_else(|| anyhow!("struct item not inside struct literal"))?;

    let struct_name = literal_struct.name();

    if let Some(struct_info) = analysis_doc.struct_by_name(struct_name.text()) {
        let (uri, def_lines) = match struct_info.namespace() {
            Some(ns_name) => {
                // SAFETY: we just found a struct_info with this namespace name and the document
                // guarantees that `analysis_doc.namespaces` contains a corresponding entry for
                // `ns_name`.
                let ns = analysis_doc.namespace(ns_name).unwrap();

                // SAFETY: `ns.source` comes from a valid namespace entry which guarantees the
                // document exists in the graph.
                let imported_node = graph.get(graph.get_index(ns.source()).unwrap());

                // SAFETY: we successfully got the document above, it's in
                // `ParseState::Parsed` which always has a valid lines field.
                let lines = imported_node.parse_state().lines().unwrap();
                (ns.source().as_ref(), lines)
            }
            None => (document_uri, lines),
        };

        let node =
            wdl_ast::v1::StructDefinition::cast(SyntaxNode::new_root(struct_info.node().clone()))
                .expect("should cast to struct definition");

        if let Some(member) = node.members().find(|m| m.name().text() == name.text()) {
            let member_span = member.name().span();
            let span = Span::new(
                member_span.start() + struct_info.offset(),
                member_span.len(),
            );
            return Ok(Some(location_from_span(uri, span, def_lines)?));
        }
    }

    Ok(None)
}

/// Resolves call input item identifiers.
///
/// For call input items like `i = 3` or `i = i * 2`:
/// - The left-hand side identifier should resolve to the target task/workflow's
///   input parameter
/// - The right-hand side expressions should be resolved through normal scope
///   resolution
///
/// For shorthand syntax like `{ i }`:
/// - The identifier should be resolved through scope resolution
fn resolve_call_input_item(
    parent_node: &SyntaxNode,
    token: &SyntaxToken,
    analysis_doc: &Document,
    document_uri: &Url,
    lines: &Arc<LineIndex>,
    graph: &DocumentGraph,
) -> Result<Option<Location>> {
    let Some(input_item) = wdl_ast::v1::CallInputItem::cast(parent_node.clone()) else {
        bail!("cannot cast to `CallInputItem`");
    };
    let ident = input_item.name();

    if input_item.is_implicit_bind() {
        return Ok(None);
    }

    if ident.span() != token.span() {
        // For RHS identifier, fall through to scope resolution
        return Ok(None);
    }

    // For LHS identifier, resolve to the target task/workflow's input parameter.
    let mut current = parent_node.parent();

    // Walk up the CST to find the containing `CallStatement`.
    // This traversal is necessary because call input parameters are not part of the
    // local scope - they refer to the called task/workflow's parameter definitions.
    while let Some(node) = current {
        if node.kind() == SyntaxKind::CallStatementNode {
            let Some(call_stmt) = wdl_ast::v1::CallStatement::cast(node) else {
                break;
            };

            let target = call_stmt.target();
            let mut target_names = target.names();
            let (target_name, target_namespace) = match (target_names.next(), target_names.next()) {
                // Namespaced call
                (Some(ns), Some(name)) => (name, Some(ns)),
                // Local call
                (Some(name), None) => (name, None),
                _ => return Ok(None),
            };

            if let Some(ns_str) = target_namespace {
                let Some(ns) = analysis_doc.namespace(ns_str.text()) else {
                    return Ok(None);
                };

                // SAFETY: we know `get_index` will return `Some` as `ns.source` comes from
                // `analysis_doc.namespaces` which only contains namespaces for documents that
                // are guaranteed to be present in the graph.
                let node = graph.get(graph.get_index(ns.source()).unwrap());
                let Some(imported_doc) = node.document() else {
                    return Ok(None);
                };

                // SAFETY: we successfully got the document above, it's in
                // `ParseState::Parsed` which always has a valid lines field.
                let imported_lines = node.parse_state().lines().unwrap();

                // Imported tasks/workflow inputs
                return find_target_input_parameter(
                    imported_doc,
                    target_name.text(),
                    token,
                    ns.source(),
                    imported_lines,
                );
            } else {
                // Local tasks/workflow inputs
                return find_target_input_parameter(
                    analysis_doc,
                    target_name.text(),
                    token,
                    document_uri,
                    lines,
                );
            }
        }
        current = node.parent();
    }
    Ok(None)
}

/// Finds input parameter definitions in tasks and workflows.
fn find_target_input_parameter(
    doc: &Document,
    target_name: &str,
    token: &SyntaxToken,
    uri: &Url,
    lines: &Arc<LineIndex>,
) -> Result<Option<Location>> {
    if let Some(task) = doc.task_by_name(target_name)
        && task.inputs().contains_key(token.text())
    {
        let scope = task.scope();
        if let Some(ident) = scope.lookup(token.text()) {
            return Ok(Some(location_from_span(uri, ident.span(), lines)?));
        }
    }

    if let Some(workflow) = doc.workflow()
        && workflow.name() == target_name
        && workflow.inputs().contains_key(token.text())
    {
        let scope = workflow.scope();
        if let Some(ident) = scope.lookup(token.text()) {
            return Ok(Some(location_from_span(uri, ident.span(), lines)?));
        }
    }

    Ok(None)
}

/// Finds global structs, tasks and workflow definition in a document.
fn find_global_definition_in_doc(
    analysis_doc: &Document,
    ident_text: &str,
    document_uri: &Url,
    lines: &Arc<LineIndex>,
) -> Result<Option<Location>> {
    if let Some(s) = analysis_doc.struct_by_name(ident_text) {
        return Ok(Some(location_from_span(
            document_uri,
            s.name_span(),
            lines,
        )?));
    }
    if let Some(e) = analysis_doc.enum_by_name(ident_text) {
        return Ok(Some(location_from_span(
            document_uri,
            e.name_span(),
            lines,
        )?));
    }
    if let Some(t) = analysis_doc.task_by_name(ident_text) {
        return Ok(Some(location_from_span(
            document_uri,
            t.name_span(),
            lines,
        )?));
    }
    if let Some(w) = analysis_doc
        .workflow()
        .filter(|w_def| w_def.name() == ident_text)
    {
        return Ok(Some(location_from_span(
            document_uri,
            w.name_span(),
            lines,
        )?));
    }

    Ok(None)
}
