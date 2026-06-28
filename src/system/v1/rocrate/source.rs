//! Materializes resolved WDL source into the crate under `workflow/`.

use std::collections::HashSet;
use std::path::Path;

use anyhow::Context as _;
use anyhow::Result;
use rocraters::ro_crate::constraints::DataType;
use rocraters::ro_crate::data_entity::DataEntity;
use rocraters::ro_crate::graph_vector::GraphVector;
use url::Url;
use wdl::analysis::Document;
use wdl::ast::AstNode;

use super::WORKFLOW_ID;

/// Derives a crate-relative `workflow/`-rooted file name for an import URL,
/// collision-renaming repeated basenames using `used`.
fn import_path_for(url: &Url, used: &mut HashSet<String>) -> String {
    let raw = url
        .path_segments()
        .and_then(|mut s| s.next_back())
        .filter(|s| !s.is_empty())
        .unwrap_or("import.wdl");
    let base = super::value::sanitize_component(raw);
    let base = base.as_str();
    let mut candidate = format!("workflow/{base}");
    let mut n = 1;
    while used.contains(&candidate) {
        let stem = base.strip_suffix(".wdl").unwrap_or(base);
        candidate = format!("workflow/{stem}-{n}.wdl");
        n += 1;
    }
    used.insert(candidate.clone());
    candidate
}

/// Reads the full resolved source text of a document.
fn document_text(document: &Document) -> String {
    document.root().text().to_string()
}

/// Returns `(crate_relative_path, source_text)` for the main document and every
/// transitively imported document. The main document maps to [`WORKFLOW_ID`];
/// imports are de-duplicated by resolved URL and collision-renamed.
pub fn collect_sources(document: &Document) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = vec![(WORKFLOW_ID.to_string(), document_text(document))];
    let mut used: HashSet<String> = HashSet::from([WORKFLOW_ID.to_string()]);
    let mut seen_urls: HashSet<String> = HashSet::from([document.uri().as_str().to_string()]);

    // Iteratively walk the import graph.
    let mut stack: Vec<Document> = document
        .namespaces()
        .map(|(_, ns)| ns.document().clone())
        .collect();
    while let Some(doc) = stack.pop() {
        let url = doc.uri().as_str().to_string();
        if !seen_urls.insert(url) {
            continue;
        }
        let path = import_path_for(doc.uri(), &mut used);
        out.push((path, document_text(&doc)));
        for (_, ns) in doc.namespaces() {
            stack.push(ns.document().clone());
        }
    }
    out
}

/// Writes every collected source under `crate_root`, pushing a
/// `File`/`SoftwareSourceCode` entity for each **import** (the main workflow
/// entity at [`WORKFLOW_ID`] is owned by the crate builder). Returns the import
/// `@id`s for linking under the workflow entity's `hasPart`.
pub fn materialize_sources(
    document: &Document,
    crate_root: &Path,
    graph: &mut Vec<GraphVector>,
) -> Result<Vec<String>> {
    let mut import_ids = Vec::new();
    for (rel, text) in collect_sources(document) {
        let abs = crate_root.join(&rel);
        if let Some(parent) = abs.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating crate directory `{}`", parent.display()))?;
        }
        std::fs::write(&abs, text.as_bytes())
            .with_context(|| format!("writing crate source `{}`", abs.display()))?;

        if rel != WORKFLOW_ID {
            graph.push(GraphVector::DataEntity(DataEntity {
                id: rel.clone(),
                type_: DataType::TermArray(vec![
                    "File".to_string(),
                    "SoftwareSourceCode".to_string(),
                ]),
                dynamic_entity: Some(
                    [(
                        "name".to_string(),
                        rocraters::ro_crate::constraints::EntityValue::EntityString(rel.clone()),
                    )]
                    .into_iter()
                    .collect(),
                ),
            }));
            import_ids.push(rel);
        }
    }
    Ok(import_ids)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use url::Url;

    use super::import_path_for;

    #[test]
    fn import_paths_are_under_workflow_and_collision_renamed() {
        let mut used = HashSet::new();
        let a = import_path_for(&Url::parse("file:///x/tasks.wdl").unwrap(), &mut used);
        let b = import_path_for(
            &Url::parse("https://example.com/y/tasks.wdl").unwrap(),
            &mut used,
        );
        assert_eq!(a, "workflow/tasks.wdl");
        assert_eq!(b, "workflow/tasks-1.wdl", "repeated basename is renamed");
        assert!(a.starts_with("workflow/") && b.starts_with("workflow/"));
    }
}
