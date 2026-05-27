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
use lsp_types::Location;
use lsp_types::Range;
use lsp_types::SymbolKind;
use url::Url;
use wdl_grammar::Span;
use wdl_grammar::SyntaxNode;

use crate::Document;
use crate::SourcePosition;
use crate::SourcePositionEncoding;
use crate::document::Task;
use crate::document::Workflow;
use crate::graph::DocumentGraph;
use crate::graph::ParseState;
use crate::handlers::common::find_identifier_token_at_offset;
use crate::handlers::common::location_from_span;
use crate::handlers::common::position_to_offset;
use crate::handlers::find_all_references;
use crate::handlers::goto_definition;
use crate::types::CallKind;

/// A callable item.
#[derive(Debug)]
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

    /// Get the [`Span`] of the callable's full definition.
    fn span(&self) -> Span {
        match self {
            Callable::Workflow(w) => w.span(),
            Callable::Task(t) => t.span(),
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
        let range = location_from_span(analysis_doc.uri(), self.span(), lines)?.range;
        let selection_range =
            location_from_span(analysis_doc.uri(), self.name_span(), lines)?.range;

        Ok(CallHierarchyItem {
            name: self.name().to_string(),
            kind: self.symbol_kind(),
            tags: None,
            detail: None,
            uri: (**analysis_doc.uri()).clone(),
            range,
            selection_range,
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
        bail!("document analysis data not available for {document_uri}");
    };

    let offset = position_to_offset(&lines, position, encoding)?;
    let Some(token) = find_identifier_token_at_offset(&root, offset) else {
        return Ok(None);
    };

    let Some(definition) = goto_definition(graph, analysis_doc.uri(), position, encoding)? else {
        return Ok(None);
    };

    let definition_offset: u32 = position_to_offset(
        &lines,
        SourcePosition::new(
            definition.range.start.line,
            definition.range.start.character,
        ),
        encoding,
    )?
    .into();

    if let Some(workflow) = analysis_doc.workflow()
        && workflow.name() == token.text()
        && workflow.name_span().contains(definition_offset as usize)
    {
        return Ok(Some((
            analysis_doc.clone(),
            lines,
            Callable::Workflow(workflow),
        )));
    }

    if let Some(task) = analysis_doc.task_by_name(token.text())
        && task.name_span().contains(definition_offset as usize)
    {
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

/// The enclosing scope of a reference site.
#[derive(Clone, Debug)]
struct EnclosingScope {
    /// The kind of the enclosing scope.
    pub kind: EnclosingScopeKind,
    /// The name of the enclosing task or workflow.
    pub name: String,
    /// The location of the enclosing scope's name declaration.
    pub location: Location,
    /// The full range of the enclosing scope.
    pub range: Range,
}

/// Represents the kind of an enclosing scope.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum EnclosingScopeKind {
    /// The reference is inside a task.
    Task,
    /// The reference is inside a workflow.
    Workflow,
}

/// Resolves the enclosing task or workflow scope.
fn resolve_enclosing_scope(
    document: &Document,
    offset: usize,
    lines: &LineIndex,
) -> Option<EnclosingScope> {
    if let Some(workflow) = document.workflow()
        && workflow.scope().span().contains(offset)
    {
        let location = location_from_span(document.uri(), workflow.name_span(), lines).ok()?;
        let range = location_from_span(document.uri(), workflow.span(), lines)
            .ok()?
            .range;
        return Some(EnclosingScope {
            kind: EnclosingScopeKind::Workflow,
            name: workflow.name().to_string(),
            location,
            range,
        });
    }

    for task in document.tasks() {
        if task.scope().span().contains(offset) {
            let location = location_from_span(document.uri(), task.name_span(), lines).ok()?;
            let range = location_from_span(document.uri(), task.span(), lines)
                .ok()?
                .range;
            return Some(EnclosingScope {
                kind: EnclosingScopeKind::Task,
                name: task.name().to_string(),
                location,
                range,
            });
        }
    }

    None
}

/// Determines all incoming calls for the given symbol.
///
/// Implementation of [`callHierarchy/incomingCalls`]
///
/// [`callHierarchy/incomingCalls`]: https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#callHierarchy_incomingCalls
pub fn incoming_calls(
    graph: &DocumentGraph,
    document_uri: &Url,
    position: SourcePosition,
    encoding: SourcePositionEncoding,
) -> Result<Option<Vec<CallHierarchyIncomingCall>>> {
    let Some((analysis_doc, _, callable)) =
        find_callable_at_position(graph, document_uri, position, encoding)?
    else {
        return Ok(None);
    };

    let target_name = callable.name().to_string();

    let locations = find_all_references(graph, document_uri, position, encoding, false)?;
    if locations.is_empty() {
        return Ok(Some(Vec::new()));
    }

    let mut locations_by_uri: HashMap<Url, Vec<Range>> = HashMap::new();
    for location in locations {
        locations_by_uri
            .entry(location.uri)
            .or_default()
            .push(location.range);
    }

    let mut calls: HashMap<(Url, String), (CallHierarchyItem, Vec<Range>)> = HashMap::new();

    for (uri, ranges) in locations_by_uri {
        let Some(node) = graph.get_index(&uri).map(|index| graph.get(index)) else {
            continue;
        };

        let lines = match node.parse_state() {
            ParseState::Parsed { lines, .. } => lines.clone(),
            _ => bail!("document `{uri}` has not been parsed"),
        };

        let Some(doc) = node.document() else {
            bail!("document analysis data not available for {uri}");
        };

        for range in ranges {
            let token_offset: u32 = position_to_offset(
                &lines,
                SourcePosition::new(range.start.line, range.start.character),
                encoding,
            )?
            .into();
            let Some(scope) = resolve_enclosing_scope(doc, token_offset as usize, &lines) else {
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
                    entry.get_mut().1.push(range);
                }
                Entry::Vacant(entry) => {
                    entry.insert((
                        CallHierarchyItem {
                            name: scope.name,
                            kind,
                            tags: None,
                            detail: None,
                            uri: scope.location.uri,
                            range: scope.range,
                            selection_range: scope.location.range,
                            data: None,
                        },
                        vec![range],
                    ));
                }
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
    document_uri: &Url,
    position: SourcePosition,
    encoding: SourcePositionEncoding,
) -> Result<Option<Vec<CallHierarchyOutgoingCall>>> {
    let Some((analysis_doc, lines, callable)) =
        find_callable_at_position(graph, document_uri, position, encoding)?
    else {
        return Ok(None);
    };

    let Callable::Workflow(workflow) = callable else {
        return Ok(Some(Vec::new()));
    };

    let mut calls: HashMap<(Url, String), (CallHierarchyItem, Vec<Range>)> = HashMap::new();
    let scope = workflow.scope();

    for (ident, call) in workflow.calls() {
        let source_doc = if let Some(ns) = call.namespace() {
            let Some(ns) = analysis_doc.namespace(ns) else {
                continue;
            };
            ns.document()
        } else {
            &analysis_doc
        };

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

        let (kind, def_span, def_name_span) = match call.kind() {
            CallKind::Task => {
                let task = source_doc.task_by_name(call.name()).expect("should exist");
                (SymbolKind::METHOD, task.span(), task.name_span())
            }
            CallKind::Workflow => {
                let workflow = source_doc.workflow().expect("should exist");
                (SymbolKind::FUNCTION, workflow.span(), workflow.name_span())
            }
        };

        let to_range = location_from_span(source_doc.uri(), def_span, &source_lines)?.range;

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
                        range: to_range,
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
