//! Handlers for "find all references" requests.
//!
//! This module implements the LSP "textDocument/references" functionality for
//! WDL files. It finds all references to a symbol by first resolving the
//! symbol's definition, then searches through all the appropriate documents.
//!
//! See: [LSP Specification](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#textDocument_references)

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use lsp_types::Location;
use petgraph::graph::NodeIndex;
use url::Url;
use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::SyntaxKind;
use wdl_ast::SyntaxToken;
use wdl_ast::TreeToken;
use wdl_ast::v1;

use crate::SourcePosition;
use crate::SourcePositionEncoding;
use crate::document::Document as AnalysisDocument;
use crate::graph::DocumentGraph;
use crate::handlers;
use crate::handlers::common::location_from_span;
use crate::handlers::common::position;
use crate::handlers::common::position_to_offset;

/// Represents a target definition for which references are being searched.
#[derive(Debug)]
struct TargetDefinition {
    /// The identifier text of the target symbol.
    name: String,
    /// The location where the target is defined.
    location: Location,
}

/// Returns document node indices to scan for references to a symbol defined at
/// `token`.
fn reference_search_scope(
    graph: &DocumentGraph,
    definition_doc: NodeIndex,
    document: &AnalysisDocument,
    token: &SyntaxToken,
) -> Vec<NodeIndex> {
    if needs_transitive_importers(document, token) {
        graph.transitive_dependents(definition_doc).collect()
    } else {
        vec![definition_doc]
    }
}

/// Returns `true` when a definition can be referenced from importer documents.
///
/// Local-only symbols, like call aliases and declarations inside task/workflow
/// bodies, should remain within the defining document.
fn needs_transitive_importers(document: &AnalysisDocument, token: &SyntaxToken) -> bool {
    if is_local_by_syntax(token) {
        return false;
    }

    if let Some(scope) = document.find_scope_by_position(token.span().start())
        && let Some(name) = scope.lookup(token.text())
        && name.span() == token.span()
    {
        return !name.is_local();
    }

    true
}

/// Determines whether a definition token is local to its document based on
/// its syntax alone.
///
/// This covers local names that a name scope lookup at the token's position
/// does not find: explicit import namespaces, import alias targets, and
/// scatter variables (the scatter scope begins after the variable).
fn is_local_by_syntax(token: &SyntaxToken) -> bool {
    let Some(parent) = token.parent() else {
        return false;
    };

    match parent.kind() {
        SyntaxKind::ImportStatementNode => v1::ImportStatement::cast(parent)
            .and_then(|import| import.explicit_namespace())
            .is_some_and(|namespace| namespace.span() == token.span()),
        SyntaxKind::ImportAliasNode => v1::ImportAlias::cast(parent)
            .is_some_and(|alias| alias.names().1.span() == token.span()),
        SyntaxKind::ScatterStatementNode => v1::ScatterStatement::cast(parent)
            .is_some_and(|scatter| scatter.variable().span() == token.span()),
        _ => false,
    }
}

/// Finds all references to the identifier at the given position.
///
/// It first resolves the definition of the identifier at the specified
/// position, then searches through the appropriate scope of
/// documents to find all references to that definition.
pub fn find_all_references(
    graph: &DocumentGraph,
    document_uri: &Url,
    position: SourcePosition,
    encoding: SourcePositionEncoding,
    include_declaration: bool,
) -> Result<Vec<Location>> {
    let definition_location = handlers::goto_definition(graph, document_uri, position, encoding)
        .context("failed to resolve symbol definition")?
        .ok_or_else(|| {
            anyhow!(
                "no definition location found for symbol at position: {}:{}",
                position.line,
                position.character
            )
        })?;

    let doc_index = graph
        .get_index(&definition_location.uri)
        .ok_or_else(|| anyhow!("definition document not in graph"))?;

    let node = graph.get(doc_index);
    let document = node
        .document()
        .ok_or_else(|| anyhow!("definition document not analyzed"))?;

    let lines = node
        .parse_state()
        .lines()
        .ok_or_else(|| anyhow!("missing line index for target"))?;

    let offset = position_to_offset(
        lines,
        SourcePosition::new(
            definition_location.range.start.line,
            definition_location.range.start.character,
        ),
        encoding,
    )
    .context("failed to convert position to offset")?;

    let token = document
        .root()
        .inner()
        .token_at_offset(offset)
        .find(|t| t.kind() == SyntaxKind::Ident)
        .ok_or_else(|| anyhow!("could not find target token at definition site"))?;

    let target = TargetDefinition {
        name: token.text().to_string(),
        location: definition_location.clone(),
    };

    let search_scope = reference_search_scope(graph, doc_index, document, &token);

    let mut locations = Vec::new();
    for doc_index in search_scope {
        collect_references_from_document(graph, doc_index, &target, encoding, &mut locations)
            .with_context(|| {
                format!("failed to collect references from document at index {doc_index:?}")
            })?;
    }

    if !include_declaration {
        locations.retain(|loc| *loc != target.location);
    }

    Ok(locations)
}

/// Collects references to the target symbol from a single document.
///
/// 1. Traverse all tokens in the document's CST
/// 2. Filter for identifier tokens matching the target name
/// 3. For each match, resolve its definition using goto definition
/// 4. If the resolved definition matches the target, add the reference location
fn collect_references_from_document(
    graph: &DocumentGraph,
    doc_index: petgraph::graph::NodeIndex,
    target: &TargetDefinition,
    encoding: SourcePositionEncoding,
    locations: &mut Vec<Location>,
) -> Result<()> {
    let node = graph.get(doc_index);
    let document = match node.document() {
        Some(doc) => doc,
        None => return Ok(()),
    };

    let lines = match node.parse_state().lines() {
        Some(lines) => lines,
        None => return Ok(()),
    };

    let root = document.root().inner().clone();

    for token in root
        .descendants_with_tokens()
        .filter_map(|el| el.into_token())
    {
        // In WDL, variable shadowing is not allowed.
        //
        // https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#appendix-b-wdl-namespaces-and-scopes
        //
        // - All members of a namespace must be unique within that namespace.
        // - When the user makes a declaration within a nested scope, they are
        //   essentially reserving that name in all of the higher-level scopes
        //   so that it cannot be reused.
        //
        // This means name matching combined with definition resolution is safe
        // and won't produce false positives from shadowed variables.
        if token.kind() == SyntaxKind::Ident && token.text() == target.name {
            let token_pos = position(lines, token.text_range().start())
                .context("failed to convert token position")?;
            let source_pos = SourcePosition::new(token_pos.line, token_pos.character);

            let resolved_location =
                handlers::goto_definition(graph, &document.uri(), source_pos, encoding)
                    .context("failed to resolve token definition")?;

            if let Some(location) = resolved_location
                && location == target.location
            {
                let reference_location = location_from_span(&document.uri(), token.span(), lines)
                    .context("failed to create reference location")?;

                locations.push(reference_location);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;
    use wdl_ast::AstNode;
    use wdl_ast::SyntaxKind;

    use super::needs_transitive_importers;

    /// Analyzes `source.wdl` along with any additional documents it imports.
    async fn analyzed_document(source: &str, imports: &[(&str, &str)]) -> crate::Document {
        let dir = TempDir::new().expect("failed to create temporary directory");
        for (name, contents) in imports {
            fs::write(dir.path().join(name), contents).expect("failed to write imported document");
        }

        let path = dir.path().join("source.wdl");
        fs::write(&path, source).expect("failed to write source document");
        let uri = crate::path_to_uri(&path).expect("should convert path to URI");

        let analyzer = crate::Analyzer::default();
        analyzer
            .add_document(uri.clone())
            .await
            .expect("should add document");

        let results = analyzer
            .analyze(())
            .await
            .expect("analysis should complete");
        results
            .iter()
            .find(|result| result.document().uri().as_ref() == &uri)
            .expect("should have analyzed source document")
            .document()
            .clone()
    }

    fn ident_token(document: &crate::Document, ident: &str) -> wdl_ast::SyntaxToken {
        document
            .root()
            .inner()
            .descendants_with_tokens()
            .filter_map(|element| element.into_token())
            .find(|token| token.kind() == SyntaxKind::Ident && token.text() == ident)
            .unwrap_or_else(|| panic!("missing identifier token `{ident}`"))
    }

    #[tokio::test]
    async fn classifies_reference_visibility_from_analyzed_name_scope() {
        let document = analyzed_document(
            r#"version 1.3

struct Person {
    String struct_member
}

enum Status {
    Active
}

task greet {
    input {
        String task_input
    }

    String task_local = task_input

    command <<<
        echo "~{task_local}"
    >>>

    output {
        String task_output = task_local
    }
}

workflow example {
    input {
        String workflow_input
    }

    String x = "hi"
    call greet as worker { input: task_input = workflow_input }

    scatter (item in [1, 2]) {
        Int scattered = item
    }

    if (true) {
        String conditional = x
    }

    output {
        String out = x
        String task_result = worker.task_output
    }
}
"#,
            &[],
        )
        .await;

        let cases = [
            ("Person", true),
            ("struct_member", true),
            ("Status", true),
            ("Active", true),
            ("greet", true),
            ("task_input", true),
            ("task_local", false),
            ("task_output", true),
            ("example", true),
            ("workflow_input", true),
            ("x", false),
            ("worker", false),
            ("item", false),
            ("scattered", false),
            ("conditional", false),
            ("out", true),
            ("task_result", true),
        ];

        for (ident, expected) in cases {
            let token = ident_token(&document, ident);
            assert_eq!(
                needs_transitive_importers(&document, &token),
                expected,
                "unexpected classification for `{ident}`"
            );
        }
    }

    #[tokio::test]
    async fn classifies_import_names_as_local() {
        let document = analyzed_document(
            r#"version 1.3

import "lib.wdl" as lib alias Source as Target

workflow main {
    input {
        Target value
    }
}
"#,
            &[(
                "lib.wdl",
                r#"version 1.3

struct Source {
    String name
}
"#,
            )],
        )
        .await;

        assert!(!needs_transitive_importers(
            &document,
            &ident_token(&document, "lib")
        ));
        assert!(!needs_transitive_importers(
            &document,
            &ident_token(&document, "Target")
        ));
    }
}
