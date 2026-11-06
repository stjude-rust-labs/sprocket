//! Handlers for "rename" requests.
//!
//! This module implements the LSP `textDocument/rename` functionality for WDL
//! files.
//!
//! See: [LSP Specification](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#textDocument_rename)

use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::PathBuf;

use anyhow::Result;
use anyhow::bail;
use indexmap::IndexSet;
use lsp_types::TextEdit;
use lsp_types::Url;
use lsp_types::WorkspaceEdit;
use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::lexer::v1::is_ident;

use crate::SourcePosition;
use crate::SourcePositionEncoding;
use crate::graph::DocumentGraph;
use crate::handlers;
use crate::queue::DocumentRename;

/// Renames a symbol at a given position in a document.
///
/// It first finds all references to the symbol at the given position,
/// including the definition itself. Then, it creates a `WorkspaceEdit`
/// to rename all occurrences.
///
/// The rename is rejected if the new name is not a valid WDL identifier.
pub fn rename(
    graph: &DocumentGraph,
    document_uri: &Url,
    position: SourcePosition,
    encoding: SourcePositionEncoding,
    new_name: String,
) -> Result<Option<WorkspaceEdit>> {
    if !is_ident(&new_name) {
        bail!("name `{new_name}` is not a valid WDL identifier");
    }

    let references = handlers::find_all_references(graph, document_uri, position, encoding, true)?;
    if references.is_empty() {
        return Ok(None);
    }

    let mut changes: HashMap<Url, Vec<TextEdit>> = HashMap::new();
    for location in references {
        let text_edit = TextEdit {
            range: location.range,
            new_text: new_name.clone(),
        };
        changes.entry(location.uri).or_default().push(text_edit);
    }

    Ok(Some(WorkspaceEdit {
        changes: Some(changes),
        document_changes: None,
        change_annotations: None,
    }))
}

/// Calculates the relative path from `base_dir` to `target_path` for use in an
/// import statement.
fn relative_import_path(
    target_path: &std::path::Path,
    base_dir: &std::path::Path,
) -> Option<String> {
    pathdiff::diff_paths(target_path, base_dir)
        .map(|diff| diff.to_string_lossy().replace('\\', "/"))
}

/// Renames a set of documents and updates all references.
pub fn rename_documents(
    graph: &DocumentGraph,
    renames: Vec<DocumentRename>,
) -> Result<Option<WorkspaceEdit>> {
    struct RenameInfo {
        new_path: PathBuf,
        old_stem: String,
        new_stem: String,
        is_valid_stem: bool,
    }

    let mut rename_by_uri: HashMap<Url, RenameInfo> = HashMap::new();
    let mut affected_documents: IndexSet<petgraph::graph::NodeIndex> = IndexSet::new();

    for rename in &renames {
        let Ok(old_path) = rename.document.to_file_path() else {
            continue;
        };
        let Ok(new_path) = rename.new_uri.to_file_path() else {
            continue;
        };
        let Some(old_stem) = old_path.file_stem().and_then(OsStr::to_str) else {
            continue;
        };
        let Some(new_stem) = new_path.file_stem().and_then(OsStr::to_str) else {
            continue;
        };

        let old_stem = old_stem.to_string();
        let new_stem = new_stem.to_string();
        let is_valid_stem = is_ident(&new_stem);

        if let Some(idx) = graph.get_index(&rename.document) {
            affected_documents.insert(idx);
            for dep in graph.dependents(idx) {
                affected_documents.insert(dep);
            }
        }

        rename_by_uri.insert(
            rename.document.clone(),
            RenameInfo {
                new_path,
                old_stem,
                new_stem,
                is_valid_stem,
            },
        );
    }

    if rename_by_uri.is_empty() {
        return Ok(None);
    }

    let mut changes: HashMap<Url, Vec<TextEdit>> = HashMap::new();

    for doc_idx in affected_documents {
        let node = graph.get(doc_idx);
        let doc_uri = node.uri();
        let Some(doc) = node.document() else {
            continue;
        };
        let Some(lines) = node.parse_state().lines() else {
            continue;
        };
        let Ok(doc_path) = doc_uri.to_file_path() else {
            continue;
        };

        let Some(new_dir) = (if let Some(info) = rename_by_uri.get(doc_uri) {
            info.new_path.parent()
        } else {
            doc_path.parent()
        }) else {
            continue;
        };

        let importer_dir_changed = rename_by_uri
            .get(doc_uri)
            .map(|info| info.new_path.parent() != doc_path.parent())
            .unwrap_or(false);

        let mut namespace_renames: HashMap<String, String> = HashMap::new();

        for import in doc.root().children::<wdl_ast::v1::ImportStatement>() {
            let wdl_ast::v1::ImportSource::Uri(uri) = import.source() else {
                continue;
            };

            let Some(wdl_ast::v1::LiteralStringText::Token(text)) = uri.text() else {
                continue;
            };

            let Ok(import_uri) = node.uri().join(text.text()) else {
                continue;
            };
            let Ok(import_path) = import_uri.to_file_path() else {
                continue;
            };

            let (target_new_path, rename_info) = if let Some(info) = rename_by_uri.get(&import_uri)
            {
                (&info.new_path, Some(info))
            } else {
                (&import_path, None)
            };

            // Rename the import path
            if (importer_dir_changed || rename_info.is_some())
                && let Some(rel) = relative_import_path(target_new_path, new_dir)
                && rel != text.text()
            {
                let loc = handlers::common::location_from_span(doc_uri, text.span(), lines)?;
                changes
                    .entry((**doc_uri).clone())
                    .or_default()
                    .push(TextEdit {
                        range: loc.range,
                        new_text: rel,
                    });
            }

            // Rename the namespace if necessary

            let Some(info) = rename_info else {
                continue;
            };

            if info.new_stem == info.old_stem {
                continue; // no-op
            }

            if import.form() != wdl_ast::v1::ImportForm::Namespace
                || import.explicit_namespace().is_some()
                || !info.is_valid_stem
            {
                continue; // keep the original namespace in place
            }

            namespace_renames.insert(info.old_stem.clone(), info.new_stem.clone());
        }

        if namespace_renames.is_empty() {
            continue;
        }

        for target in doc.root().descendants::<wdl_ast::v1::CallTarget>() {
            let mut names = target.names();
            let (Some(first), Some(_second)) = (names.next(), names.next()) else {
                continue;
            };

            let Some(new_name) = namespace_renames.get(first.text()) else {
                continue;
            };

            if let Some(ns) = doc.namespace(first.text())
                && let Some(info) = rename_by_uri.get(ns.source().as_ref())
                && &info.new_stem != new_name
            {
                continue;
            }

            let loc = handlers::common::location_from_span(doc_uri, first.span(), lines)?;
            changes
                .entry((**doc_uri).clone())
                .or_default()
                .push(TextEdit {
                    range: loc.range,
                    new_text: new_name.clone(),
                });
        }
    }

    if changes.is_empty() {
        return Ok(None);
    }

    for edits in changes.values_mut() {
        edits.sort_by_key(|e| (e.range.start.line, e.range.start.character));
        edits.dedup();
    }

    Ok(Some(WorkspaceEdit {
        changes: Some(changes),
        document_changes: None,
        change_annotations: None,
    }))
}
