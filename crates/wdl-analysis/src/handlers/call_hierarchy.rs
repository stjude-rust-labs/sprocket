//! Handlers for `call hierarchy` requests.
//!
//! This module implements the LSP "textDocument/prepareCallHierarchy"
//! functionality for WDL files.
//!
//! See: [LSP Specification](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#textDocument_prepareCallHierarchy)

use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::sync::Arc;

use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use line_index::LineIndex;
use lsp_types::CallHierarchyIncomingCall;
use lsp_types::CallHierarchyItem;
use lsp_types::CallHierarchyOutgoingCall;
use lsp_types::Range;
use lsp_types::SymbolKind;
use url::Url;
use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::v1::TaskDefinition;
use wdl_ast::v1::WorkflowDefinition;
use wdl_grammar::Span;
use wdl_grammar::SyntaxNode;

use crate::Document;
use crate::SourcePosition;
use crate::SourcePositionEncoding;
use crate::document::Task;
use crate::document::Workflow;
use crate::graph::DocumentGraph;
use crate::graph::ParseState;
use crate::handlers::EnclosingScopeKind;
use crate::handlers::common::find_identifier_token_at_offset;
use crate::handlers::common::location_from_span;
use crate::handlers::common::position_to_offset;
use crate::handlers::find_all_references;
use crate::types::CallKind;

/// A callable item.
enum Callable<'a> {
    /// A workflow.
    Workflow(&'a Workflow),
    /// A task.
    Task(&'a Task),
}

impl Callable<'_> {
    /// Get the name of this callable.
    fn name(&self) -> &str {
        match self {
            Callable::Workflow(w) => w.name(),
            Callable::Task(t) => t.name(),
        }
    }

    /// Get the [`Span`] of the callable's name.
    fn name_span(&self) -> Span {
        match self {
            Callable::Workflow(w) => w.name_span(),
            Callable::Task(t) => t.name_span(),
        }
    }

    /// Get the [`SymbolKind`] for this callable.
    fn symbol_kind(&self) -> SymbolKind {
        match self {
            Callable::Workflow(_) => SymbolKind::FUNCTION,
            Callable::Task(_) => SymbolKind::METHOD,
        }
    }

    /// Attempt to convert this callable to a [`CallHierarchyItem`].
    fn as_call_hierarchy_item(
        &self,
        analysis_doc: &Document,
        lines: &LineIndex,
    ) -> Result<CallHierarchyItem> {
        let name_range = location_from_span(analysis_doc.uri(), self.name_span(), lines)?.range;

        Ok(CallHierarchyItem {
            name: self.name().to_string(),
            kind: self.symbol_kind(),
            tags: None,
            detail: None,
            uri: (**analysis_doc.uri()).clone(),
            range: name_range,
            selection_range: name_range,
            data: None,
        })
    }
}

/// Attempt to get a [`Callable`] from the specified position.
fn find_callable_at_position<'a>(
    graph: &'a DocumentGraph,
    document_uri: &Url,
    position: SourcePosition,
    encoding: SourcePositionEncoding,
) -> Result<Option<(Document, Arc<LineIndex>, Callable<'a>)>> {
    let index = graph
        .get_index(document_uri)
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
        return Ok(None);
    };

    if let Some(workflow) = analysis_doc.workflow()
        && workflow.name() == token.text()
    {
        return Ok(Some((
            analysis_doc.clone(),
            lines,
            Callable::Workflow(workflow),
        )));
    }

    if let Some(task) = analysis_doc.task_by_name(token.text()) {
        return Ok(Some((analysis_doc.clone(), lines, Callable::Task(task))));
    }

    Ok(None)
}

/// Creates a [`CallHierarchyItem`] for the given symbol, if applicable.
///
/// Implementation of [`textDocument/prepareCallHierarchy`]
///
/// [`textDocument/prepareCallHierarchy`]: https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#textDocument_prepareCallHierarchy
pub fn call_hierarchy(
    graph: &DocumentGraph,
    document_uri: Url,
    position: SourcePosition,
    encoding: SourcePositionEncoding,
) -> Result<Option<Vec<CallHierarchyItem>>> {
    let Some((analysis_doc, lines, callable)) =
        find_callable_at_position(graph, &document_uri, position, encoding)?
    else {
        return Ok(None);
    };

    Ok(Some(vec![
        callable.as_call_hierarchy_item(&analysis_doc, &lines)?,
    ]))
}

/// Determines all incoming calls for the given symbol.
///
/// Implementation of [`callHierarchy/incomingCalls`]
///
/// [`callHierarchy/incomingCalls`]: https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#callHierarchy_incomingCalls
pub fn incoming_calls(
    graph: &DocumentGraph,
    document_uri: Url,
    position: SourcePosition,
    encoding: SourcePositionEncoding,
) -> Result<Option<Vec<CallHierarchyIncomingCall>>> {
    let Some((analysis_doc, _, callable)) =
        find_callable_at_position(graph, &document_uri, position, encoding)?
    else {
        return Ok(None);
    };

    let target_name = match &callable {
        Callable::Workflow(workflow) => workflow.name().to_string(),
        Callable::Task(task) => task.name().to_string(),
    };

    let references = find_all_references(graph, document_uri, position, encoding, false)?;
    if references.references.is_empty() {
        return Ok(Some(Vec::new()));
    }

    let mut calls: HashMap<(Url, String), (CallHierarchyItem, Vec<Range>)> = HashMap::new();

    for reference in references.references {
        let Some(scope) = reference.enclosing_scope else {
            continue;
        };

        if scope.name == target_name && scope.location.uri == **analysis_doc.uri() {
            continue;
        }

        let kind = match scope.kind {
            EnclosingScopeKind::Task => SymbolKind::METHOD,
            EnclosingScopeKind::Workflow => SymbolKind::FUNCTION,
        };

        match calls.entry((scope.location.uri.clone(), scope.name.clone())) {
            Entry::Occupied(mut entry) => {
                entry.get_mut().1.push(reference.location.range);
            }
            Entry::Vacant(entry) => {
                entry.insert((
                    CallHierarchyItem {
                        name: scope.name,
                        kind,
                        tags: None,
                        detail: None,
                        uri: scope.location.uri,
                        range: scope.location.range,
                        selection_range: scope.location.range,
                        data: None,
                    },
                    vec![reference.location.range],
                ));
            }
        }
    }

    Ok(Some(
        calls
            .into_values()
            .map(|(from, from_ranges)| CallHierarchyIncomingCall { from, from_ranges })
            .collect(),
    ))
}

/// Determines all outgoing calls for the given symbol.
///
/// Implementation of [`callHierarchy/outgoingCalls`]
///
/// [`callHierarchy/outgoingCalls`]: https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#callHierarchy_outgoingCalls
pub fn outgoing_calls(
    graph: &DocumentGraph,
    document_uri: Url,
    position: SourcePosition,
    encoding: SourcePositionEncoding,
) -> Result<Option<Vec<CallHierarchyOutgoingCall>>> {
    let Some((analysis_doc, lines, callable)) =
        find_callable_at_position(graph, &document_uri, position, encoding)?
    else {
        return Ok(None);
    };

    let Callable::Workflow(workflow) = callable else {
        return Ok(Some(Vec::new()));
    };

    let mut calls: HashMap<(Url, String), (CallHierarchyItem, Vec<Range>)> = HashMap::new();
    let scope = workflow.scope();

    for (ident, call) in workflow.calls() {
        let source_doc = call
            .namespace()
            .map(|ns| {
                analysis_doc
                    .namespace(ns)
                    .expect("namespace should be present")
                    .document()
            })
            .unwrap_or(&analysis_doc);

        let source_index = graph.get_index(source_doc.uri()).ok_or_else(|| {
            anyhow!(
                "document `{uri}` not found in graph",
                uri = source_doc.uri()
            )
        })?;

        let source_node = graph.get(source_index);
        let source_lines = match source_node.parse_state() {
            ParseState::Parsed { lines, .. } => lines.clone(),
            _ => bail!(
                "document `{uri}` has not been parsed",
                uri = source_doc.uri()
            ),
        };

        let from_span = scope.lookup(ident).expect("should be in scope").span();
        let from_range = location_from_span(analysis_doc.uri(), from_span, &lines)?.range;

        let (kind, def_name_span) = match call.kind() {
            CallKind::Task => {
                let task = source_doc
                    .root()
                    .children::<TaskDefinition>()
                    .find(|task| task.name().text() == call.name())
                    .expect("should exist");
                (SymbolKind::METHOD, task.name().span())
            }
            CallKind::Workflow => {
                let workflow = source_doc
                    .root()
                    .children::<WorkflowDefinition>()
                    .find(|workflow| workflow.name().text() == call.name())
                    .expect("should exist");
                (SymbolKind::FUNCTION, workflow.name().span())
            }
        };

        let to_selection_range =
            location_from_span(source_doc.uri(), def_name_span, &source_lines)?.range;

        match calls.entry(((**source_doc.uri()).clone(), call.name().to_string())) {
            Entry::Occupied(mut entry) => {
                entry.get_mut().1.push(from_range);
            }
            Entry::Vacant(entry) => {
                entry.insert((
                    CallHierarchyItem {
                        name: call.name().to_string(),
                        kind,
                        tags: None,
                        detail: None,
                        uri: (**source_doc.uri()).clone(),
                        range: to_selection_range,
                        selection_range: to_selection_range,
                        data: None,
                    },
                    vec![from_range],
                ));
            }
        }
    }

    Ok(Some(
        calls
            .into_values()
            .map(|(to, from_ranges)| CallHierarchyOutgoingCall { to, from_ranges })
            .collect(),
    ))
}
