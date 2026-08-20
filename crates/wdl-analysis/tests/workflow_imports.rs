//! Integration tests for the single-workflow local-scope import invariant.
//!
//! These tests enforce that at most one workflow may occupy the local scope
//! of a document via scope-merging (wildcard or selected) imports, that the
//! first distinct workflow processed occupies local scope (imports run before
//! local declarations, so an imported workflow beats a later local definition),
//! and that re-importing the same underlying declaration is deduplicated.
//! Namespaced imports are unrestricted and do not consume the workflow slot.

use std::fs;

use tempfile::TempDir;
use wdl_analysis::Analyzer;
use wdl_analysis::Config;
use wdl_analysis::Document;
use wdl_analysis::FeatureFlags;
use wdl_analysis::path_to_uri;
use wdl_ast::Severity;

async fn analyze(files: &[(&str, &str)]) -> Document {
    let dir = TempDir::new().expect("temporary directory should be created");
    for (name, contents) in files {
        fs::write(dir.path().join(name), contents).expect("test document should be written");
    }

    let source = path_to_uri(dir.path().join("source.wdl")).expect("source URI should be valid");
    let config = Config::default().with_feature_flags(FeatureFlags::default().with_wdl_1_4());
    let analyzer = Analyzer::new(config, |(), _, _, _| async {});
    analyzer
        .add_document(source.clone())
        .await
        .expect("source document should be added");

    let mut results = analyzer.analyze(()).await.expect("analysis should succeed");
    let index = results
        .iter()
        .position(|result| **result.document().uri() == source)
        .expect("source result should exist");
    results.swap_remove(index).document().clone()
}

fn errors(document: &Document) -> Vec<String> {
    document
        .diagnostics()
        .filter(|diagnostic| diagnostic.severity() == Severity::Error)
        .map(|diagnostic| diagnostic.message().to_string())
        .collect()
}

#[tokio::test]
async fn wildcard_imported_workflow_precedes_local_workflow() {
    let document = analyze(&[
        ("lib.wdl", "version 1.4\n\nworkflow imported {}\n"),
        (
            "source.wdl",
            "version 1.4\n\nimport * from \"lib.wdl\"\n\nworkflow local {}\n",
        ),
    ])
    .await;

    assert!(document.workflow().is_none());
    assert!(document.imported_workflow_by_name("imported").is_some());
    assert_eq!(
        errors(&document),
        ["cannot add workflow `local` because only one workflow may be in scope"]
    );
}

#[tokio::test]
async fn selected_imported_workflow_precedes_local_workflow() {
    let document = analyze(&[
        ("lib.wdl", "version 1.4\n\nworkflow imported {}\n"),
        (
            "source.wdl",
            "version 1.4\n\nimport { imported } from \"lib.wdl\"\n\nworkflow local {}\n",
        ),
    ])
    .await;

    assert!(document.workflow().is_none());
    assert!(document.imported_workflow_by_name("imported").is_some());
    assert_eq!(
        errors(&document),
        ["cannot add workflow `local` because only one workflow may be in scope"]
    );
}

async fn assert_first_import_wins(source: &str, first: &str, second: &str) {
    let document = analyze(&[
        ("a.wdl", "version 1.4\n\nworkflow alpha {}\n"),
        ("b.wdl", "version 1.4\n\nworkflow beta {}\n"),
        ("source.wdl", source),
    ])
    .await;

    assert!(document.imported_workflow_by_name(first).is_some());
    assert!(document.imported_workflow_by_name(second).is_none());
    assert_eq!(
        errors(&document),
        [format!(
            "cannot add workflow `{second}` because only one workflow may be in scope"
        )]
    );
}

#[tokio::test]
async fn distinct_imported_workflows_conflict_in_every_scope_merging_order() {
    for (source, first, second) in [
        (
            "version 1.4\n\nimport * from \"a.wdl\"\nimport * from \"b.wdl\"\n\nstruct Anchor { \
             Int value }\n",
            "alpha",
            "beta",
        ),
        (
            "version 1.4\n\nimport { alpha } from \"a.wdl\"\nimport { beta } from \
             \"b.wdl\"\n\nstruct Anchor { Int value }\n",
            "alpha",
            "beta",
        ),
        (
            "version 1.4\n\nimport * from \"a.wdl\"\nimport { beta } from \"b.wdl\"\n\nstruct \
             Anchor { Int value }\n",
            "alpha",
            "beta",
        ),
        (
            "version 1.4\n\nimport { alpha } from \"a.wdl\"\nimport * from \"b.wdl\"\n\nstruct \
             Anchor { Int value }\n",
            "alpha",
            "beta",
        ),
    ] {
        assert_first_import_wins(source, first, second).await;
    }
}

#[tokio::test]
async fn same_workflow_reimport_is_deduplicated() {
    let document = analyze(&[
        ("base.wdl", "version 1.4\n\nworkflow shared {}\n"),
        (
            "mid.wdl",
            "version 1.4\n\nimport * from \"base.wdl\"\n\nstruct Relay { Int x }\n",
        ),
        (
            "source.wdl",
            "version 1.4\n\nimport * from \"base.wdl\"\nimport * from \"mid.wdl\"\n\nstruct \
             Anchor { Int value }\n",
        ),
    ])
    .await;

    assert!(errors(&document).is_empty());
    assert!(document.imported_workflow_by_name("shared").is_some());
}

#[tokio::test]
async fn namespaced_workflows_do_not_occupy_the_local_workflow_slot() {
    let document = analyze(&[
        ("a.wdl", "version 1.4\n\nworkflow alpha {}\n"),
        ("b.wdl", "version 1.4\n\nworkflow beta {}\n"),
        (
            "source.wdl",
            "version 1.4\n\nimport \"a.wdl\" as a\nimport \"b.wdl\" as b\n\nworkflow local {}\n",
        ),
    ])
    .await;

    assert!(errors(&document).is_empty());
    assert_eq!(
        document.workflow().map(|workflow| workflow.name()),
        Some("local")
    );
    assert!(document.namespace("a").is_some());
    assert!(document.namespace("b").is_some());
}

#[tokio::test]
async fn selected_workflow_import_is_present_without_local_workflow() {
    let document = analyze(&[
        (
            "lib.wdl",
            "version 1.4\n\nworkflow run {\n    output {\n        Int out = 1\n    }\n}\n",
        ),
        (
            "source.wdl",
            "version 1.4\n\nimport { run } from \"lib.wdl\"\n\nstruct Anchor { Int value }\n",
        ),
    ])
    .await;

    assert!(errors(&document).is_empty());
    assert!(document.imported_workflow_by_name("run").is_some());
}

#[tokio::test]
async fn selected_reexport_exposes_imported_task_and_workflow() {
    let document = analyze(&[
        (
            "base.wdl",
            "version 1.4\n\ntask do_task {\n    command <<<>>>\n    output { Int out = 1 \
             }\n}\n\nworkflow do_flow {\n    output { Int out = 2 }\n}\n",
        ),
        (
            "mid.wdl",
            "version 1.4\n\nimport * from \"base.wdl\"\n\nstruct Marker { Int value }\n",
        ),
        (
            "source.wdl",
            "version 1.4\n\nimport { do_task } from \"mid.wdl\"\nimport { do_flow } from \
             \"mid.wdl\"\n\nstruct Anchor { Int value }\n",
        ),
    ])
    .await;

    assert!(errors(&document).is_empty());
    assert!(document.imported_task_by_name("do_task").is_some());
    assert!(document.imported_workflow_by_name("do_flow").is_some());
}

#[tokio::test]
async fn wildcard_reexport_exposes_imported_workflow() {
    let document = analyze(&[
        (
            "base.wdl",
            "version 1.4\n\nworkflow run {\n    output { Int out = 1 }\n}\n",
        ),
        (
            "mid.wdl",
            "version 1.4\n\nimport * from \"base.wdl\"\n\nstruct Marker { Int value }\n",
        ),
        (
            "source.wdl",
            "version 1.4\n\nimport * from \"mid.wdl\"\n\nstruct Anchor { Int value }\n",
        ),
    ])
    .await;

    assert!(errors(&document).is_empty());
    assert!(document.imported_workflow_by_name("run").is_some());
}

#[tokio::test]
async fn wildcard_all_kinds_exposes_task_workflow_struct_and_enum() {
    let document = analyze(&[
        (
            "lib.wdl",
            "version 1.4\n\nstruct Record {\n    Int value\n}\n\nenum State {\n    Ready,\n    \
             Done\n}\n\ntask run_task {\n    command <<<>>>\n    output { Int out = 1 \
             }\n}\n\nworkflow run_workflow {\n    output { Int out = 2 }\n}\n",
        ),
        (
            "source.wdl",
            "version 1.4\n\nimport * from \"lib.wdl\"\n\ntask use_types {\n    input {\n        \
             Record rec\n        State state\n    }\n\n    command <<<>>>\n\n    output {\n       \
             Int out = rec.value\n        State result = state\n    }\n}\n",
        ),
    ])
    .await;

    assert!(errors(&document).is_empty());
    assert!(document.imported_task_by_name("run_task").is_some());
    assert!(document.imported_workflow_by_name("run_workflow").is_some());
    assert!(document.struct_by_name("Record").is_some());
    assert!(document.enum_by_name("State").is_some());
}

#[tokio::test]
async fn selected_import_workflow_rejection_does_not_block_task_import() {
    // Characterization/regression: verifies selected-member isolation.
    // `alpha` occupies the workflow slot via the first selected import; a
    // second selected import lists both `beta` (workflow) and `run_task`
    // (task) in the same member list.  `beta` must be rejected while
    // `run_task` is still imported.  A GREEN result is expected because the
    // production code already processes each member independently — this test
    // was redesigned to cover the actual isolation path rather than the
    // local-workflow-rejection path that the previous version exercised.
    let document = analyze(&[
        ("a.wdl", "version 1.4\n\nworkflow alpha {}\n"),
        (
            "b.wdl",
            "version 1.4\n\ntask run_task {\n    command <<<>>>\n    output { Int out = 1 \
             }\n}\n\nworkflow beta {\n    output { Int out = 2 }\n}\n",
        ),
        (
            "source.wdl",
            "version 1.4\n\nimport { alpha } from \"a.wdl\"\nimport { beta, run_task } from \
             \"b.wdl\"\n\nstruct Anchor { Int value }\n",
        ),
    ])
    .await;

    assert!(document.imported_workflow_by_name("alpha").is_some());
    assert!(document.imported_workflow_by_name("beta").is_none());
    assert!(document.imported_task_by_name("run_task").is_some());
    assert_eq!(
        errors(&document),
        ["cannot add workflow `beta` because only one workflow may be in scope"]
    );
}

#[tokio::test]
async fn aliased_distinct_workflows_still_conflict_retaining_first_alias() {
    let document = analyze(&[
        ("a.wdl", "version 1.4\n\nworkflow alpha {}\n"),
        ("b.wdl", "version 1.4\n\nworkflow beta {}\n"),
        (
            "source.wdl",
            "version 1.4\n\nimport { alpha as foo } from \"a.wdl\"\nimport { beta as bar } from \
             \"b.wdl\"\n\nstruct Anchor { Int value }\n",
        ),
    ])
    .await;

    assert!(document.imported_workflow_by_name("foo").is_some());
    assert!(document.imported_workflow_by_name("bar").is_none());
    // SAFETY: asserted Some above.
    assert_eq!(
        document.imported_workflow_by_name("foo").unwrap().name(),
        "alpha"
    );
    assert_eq!(
        errors(&document),
        ["cannot add workflow `beta` because only one workflow may be in scope"]
    );
}

#[tokio::test]
async fn same_workflow_under_two_aliases_produces_no_error_and_both_resolve() {
    let document = analyze(&[
        ("a.wdl", "version 1.4\n\nworkflow alpha {}\n"),
        (
            "source.wdl",
            "version 1.4\n\nimport { alpha as foo } from \"a.wdl\"\nimport { alpha as bar } from \
             \"a.wdl\"\n\nstruct Anchor { Int value }\n",
        ),
    ])
    .await;

    assert!(errors(&document).is_empty());
    assert!(document.imported_workflow_by_name("foo").is_some());
    assert!(document.imported_workflow_by_name("bar").is_some());
    // SAFETY: both asserted Some above.
    let foo = document.imported_workflow_by_name("foo").unwrap();
    // SAFETY: both asserted Some above.
    let bar = document.imported_workflow_by_name("bar").unwrap();
    assert_eq!(foo.name(), "alpha");
    assert_eq!(bar.name(), "alpha");
    assert_eq!(foo.document().uri(), bar.document().uri());
}

#[tokio::test]
async fn same_name_selected_import_precedes_local_workflow() {
    let document = analyze(&[
        ("lib.wdl", "version 1.4\n\nworkflow foo {}\n"),
        (
            "source.wdl",
            "version 1.4\n\nimport { foo } from \"lib.wdl\"\n\nworkflow foo {}\n",
        ),
    ])
    .await;

    assert!(document.workflow().is_none());
    assert!(document.imported_workflow_by_name("foo").is_some());
    assert_eq!(
        errors(&document),
        ["cannot add workflow `foo` because only one workflow may be in scope"]
    );
}
