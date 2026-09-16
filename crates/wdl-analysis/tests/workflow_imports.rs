//! Integration tests for workflows imported into WDL 1.4 global scope.

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
        .position(|result| *result.document().uri() == source)
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
async fn local_and_scope_merged_workflows_coexist() {
    for import in [
        "import * from \"lib.wdl\"",
        "import { imported } from \"lib.wdl\"",
    ] {
        let source = format!("version 1.4\n\n{import}\n\nworkflow local {{}}\n");
        let document = analyze(&[
            ("lib.wdl", "version 1.4\n\nworkflow imported {}\n"),
            ("source.wdl", &source),
        ])
        .await;

        assert!(errors(&document).is_empty());
        assert!(document.local_workflow_by_name("local").is_some());
        assert!(document.imported_workflow_by_name("imported").is_some());
        assert_eq!(document.workflows().count(), 2);
    }
}

#[tokio::test]
async fn distinct_scope_merged_workflows_coexist_in_every_import_order() {
    for source in [
        r#"version 1.4

import * from "a.wdl"
import * from "b.wdl"

struct Anchor { Int value }"#,
        r#"version 1.4

import { alpha } from "a.wdl"
import { beta } from "b.wdl"

struct Anchor { Int value }"#,
        r#"version 1.4

import * from "a.wdl"
import { beta } from "b.wdl"

struct Anchor { Int value }"#,
        r#"version 1.4

import { alpha } from "a.wdl"
import * from "b.wdl"

struct Anchor { Int value }"#,
    ] {
        let document = analyze(&[
            ("a.wdl", "version 1.4\n\nworkflow alpha {}\n"),
            ("b.wdl", "version 1.4\n\nworkflow beta {}\n"),
            ("source.wdl", source),
        ])
        .await;

        assert!(errors(&document).is_empty());
        assert!(document.imported_workflow_by_name("alpha").is_some());
        assert!(document.imported_workflow_by_name("beta").is_some());
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
    assert_eq!(document.workflows().count(), 1);
}

#[tokio::test]
async fn namespaced_workflows_do_not_enter_local_scope() {
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
        document
            .local_workflows()
            .map(|workflow| workflow.name())
            .collect::<Vec<_>>(),
        ["local"]
    );
    assert!(document.namespace("a").is_some());
    assert!(document.namespace("b").is_some());
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
async fn wildcard_reexport_exposes_every_item_kind() {
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
async fn distinct_workflows_can_use_distinct_aliases() {
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

    assert!(errors(&document).is_empty());
    assert_eq!(
        document
            .imported_workflow_by_name("foo")
            .expect("aliased workflow should exist")
            .name(),
        "alpha"
    );
    assert_eq!(
        document
            .imported_workflow_by_name("bar")
            .expect("aliased workflow should exist")
            .name(),
        "beta"
    );
}

#[tokio::test]
async fn same_workflow_under_two_aliases_resolves_both() {
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
    let foo = document
        .imported_workflow_by_name("foo")
        .expect("first alias should resolve");
    let bar = document
        .imported_workflow_by_name("bar")
        .expect("second alias should resolve");
    assert_eq!(foo.name(), "alpha");
    assert_eq!(bar.name(), "alpha");
    assert_eq!(foo.document().uri(), bar.document().uri());
}

#[tokio::test]
async fn imported_workflow_name_conflicts_with_local_workflow() {
    let document = analyze(&[
        ("lib.wdl", "version 1.4\n\nworkflow foo {}\n"),
        (
            "source.wdl",
            "version 1.4\n\nimport { foo } from \"lib.wdl\"\n\nworkflow foo {}\n",
        ),
    ])
    .await;

    assert!(document.local_workflow_by_name("foo").is_none());
    assert!(document.imported_workflow_by_name("foo").is_some());
    assert_eq!(
        errors(&document),
        ["import of `foo` conflicts with an existing definition"]
    );
}

#[tokio::test]
async fn namespaced_call_resolves_reexported_workflow() {
    let document = analyze(&[
        (
            "base.wdl",
            "version 1.4\n\nworkflow child {\n    output { Int out = 1 }\n}\n",
        ),
        (
            "mid.wdl",
            "version 1.4\n\nimport * from \"base.wdl\"\n\nstruct Marker { Int value }\n",
        ),
        (
            "source.wdl",
            "version 1.4\n\nimport \"mid.wdl\" as mid\n\nworkflow main {\n    call mid.child\n    \
             output { Int out = child.out }\n}\n",
        ),
    ])
    .await;

    assert!(errors(&document).is_empty());
    let workflow = document
        .local_workflow_by_name("main")
        .expect("local workflow should exist");
    assert_eq!(
        workflow
            .calls()
            .get("child")
            .expect("workflow call should resolve")
            .name(),
        "child"
    );
}
