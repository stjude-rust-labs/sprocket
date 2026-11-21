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
    analyze_all(files, &["source.wdl"]).await.remove(0)
}

/// Analyzes `source.wdl` and returns the documents with the given file names.
async fn analyze_all(files: &[(&str, &str)], names: &[&str]) -> Vec<Document> {
    let dir = TempDir::new().expect("temporary directory should be created");
    for (name, contents) in files {
        fs::write(dir.path().join(name), contents).expect("test document should be written");
    }

    let source = path_to_uri(dir.path().join("source.wdl")).expect("source URI should be valid");
    let config = Config::default().with_feature_flags(FeatureFlags::default().with_wdl_1_4());
    let analyzer = Analyzer::new(config, |(), _, _, _| async {});
    analyzer
        .add_document(source)
        .await
        .expect("source document should be added");

    let results = analyzer.analyze(()).await.expect("analysis should succeed");
    names
        .iter()
        .map(|name| {
            let uri = path_to_uri(dir.path().join(name)).expect("URI should be valid");
            results
                .iter()
                .find(|result| *result.document().uri() == uri)
                .expect("result should exist")
                .document()
                .clone()
        })
        .collect()
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

#[tokio::test]
async fn wildcard_reexport_preserves_aliases_and_propagates_types() {
    let document = analyze(&[
        (
            "base.wdl",
            "version 1.4\n\nstruct Record {\n    Int value\n}\n\nenum State {\n    Ready,\n    \
             Done\n}\n\ntask greet {\n    command <<<>>>\n    output { Int out = 1 \
             }\n}\n\nworkflow sub {\n    output { Int out = 2 }\n}\n",
        ),
        (
            "mid.wdl",
            "version 1.4\n\nimport { Record as Item, State as Code, greet as say_hi, sub as \
             run_sub } from \"base.wdl\"\n",
        ),
        (
            "source.wdl",
            "version 1.4\n\nimport * from \"mid.wdl\"\n\nworkflow main {\n    input {\n        \
             Item item\n        Code code = Code.Ready\n    }\n\n    call say_hi\n    call \
             run_sub\n\n    output {\n        Int value = item.value\n        Code result = \
             code\n    }\n}\n",
        ),
    ])
    .await;

    assert!(errors(&document).is_empty(), "{:?}", errors(&document));

    let task = document
        .imported_task_by_name("say_hi")
        .expect("aliased task should be re-exported");
    assert_eq!(task.name(), "greet");
    assert!(task.document().uri().as_str().ends_with("base.wdl"));

    let workflow = document
        .imported_workflow_by_name("run_sub")
        .expect("aliased workflow should be re-exported");
    assert_eq!(workflow.name(), "sub");
    assert!(workflow.document().uri().as_str().ends_with("base.wdl"));

    assert!(document.struct_by_name("Item").is_some());
    assert!(document.enum_by_name("Code").is_some());
    assert!(document.get_choice_cache_key("Code", "Ready").is_some());

    assert!(document.imported_task_by_name("greet").is_none());
    assert!(document.imported_workflow_by_name("sub").is_none());
    assert!(document.struct_by_name("Record").is_none());
    assert!(document.enum_by_name("State").is_none());
}

#[tokio::test]
async fn selected_import_reexports_namespace_imported_types() {
    let document =
        analyze(
            &[
                (
                    "base.wdl",
                    "version 1.4\n\nstruct Record {\n    Int value\n}\n\nenum State {\n    \
                     Ready,\n    Done\n}\n",
                ),
                (
                    "mid.wdl",
                    "version 1.4\n\nimport \"base.wdl\" as b\n\nstruct Marker { Int value }\n",
                ),
                (
                    "source.wdl",
                    "version 1.4\n\nimport { Record, State } from \"mid.wdl\"\n\nworkflow main \
                     {\n    input {\n        Record rec\n        State state = State.Done\n    \
                     }\n\n    output {\n        Int value = rec.value\n        State result = \
                     state\n    }\n}\n",
                ),
            ],
        )
        .await;

    assert!(errors(&document).is_empty(), "{:?}", errors(&document));

    let record = document
        .struct_by_name("Record")
        .expect("namespace-imported struct should be re-exported");
    assert!(record.ty().is_some());

    let state = document
        .enum_by_name("State")
        .expect("namespace-imported enum should be re-exported");
    assert!(state.ty().is_some());
    assert!(document.get_choice_cache_key("State", "Done").is_some());
}

#[tokio::test]
async fn has_errors_propagates_through_scope_merging_imports() {
    let broken = "version 1.4\n\ntask broken {\n    command <<<>>>\n    output { Int out = \"not \
                  an int\" }\n}\n";

    for (mid, source) in [
        (
            None,
            "version 1.4\n\nimport * from \"lib.wdl\"\n\nstruct Anchor { Int value }\n",
        ),
        (
            None,
            "version 1.4\n\nimport { broken } from \"lib.wdl\"\n\nstruct Anchor { Int value }\n",
        ),
        (
            Some(
                "version 1.4\n\nimport { broken as relay } from \"lib.wdl\"\n\nstruct MidAnchor { \
                 Int value }\n",
            ),
            "version 1.4\n\nimport * from \"mid.wdl\"\n\nstruct Anchor { Int value }\n",
        ),
        (
            Some("version 1.4\n\nimport * from \"lib.wdl\"\n\nstruct MidAnchor { Int value }\n"),
            "version 1.4\n\nimport { broken } from \"mid.wdl\"\n\nstruct Anchor { Int value }\n",
        ),
    ] {
        let mut files = vec![("lib.wdl", broken), ("source.wdl", source)];
        if let Some(mid) = mid {
            files.push(("mid.wdl", mid));
        }

        let names: &[&str] = if mid.is_some() {
            &["source.wdl", "mid.wdl"]
        } else {
            &["source.wdl"]
        };
        let documents = analyze_all(&files, names).await;
        for document in &documents {
            assert!(errors(document).is_empty(), "{:?}", errors(document));
        }
        assert!(documents[0].has_errors(), "source: {source}");
    }
}

#[tokio::test]
async fn namespace_providing_reexported_types_is_marked_used() {
    let document =
        analyze(
            &[
                (
                    "base.wdl",
                    "version 1.4\n\nstruct Record {\n    Int value\n}\n\nenum State {\n    \
                     Ready,\n    Done\n}\n",
                ),
                (
                    "mid.wdl",
                    "version 1.4\n\nimport { Record, State as Code } from \"base.wdl\"\n",
                ),
                (
                    "source.wdl",
                    "version 1.4\n\nimport \"mid.wdl\" as m\nimport \"mid.wdl\" as n\n\nworkflow \
                     main {\n    input {\n        Record rec\n    }\n\n    output {\n        Code \
                     code = Code.Ready\n    }\n}\n",
                ),
            ],
        )
        .await;

    assert!(errors(&document).is_empty(), "{:?}", errors(&document));
    let unused = document
        .diagnostics()
        .filter(|d| d.message().starts_with("unused import namespace"))
        .map(|d| d.message().to_string())
        .collect::<Vec<_>>();
    assert_eq!(unused, ["unused import namespace `n`"]);

    // A namespace whose only use is an enum choice expression is still used
    let document = analyze(&[
        (
            "base.wdl",
            "version 1.4\n\nenum State {\n    Ready,\n    Done\n}\n",
        ),
        (
            "mid.wdl",
            "version 1.4\n\nimport { State as Code } from \"base.wdl\"\n\nstruct MidAnchor { Int \
             value }\n",
        ),
        (
            "source.wdl",
            "version 1.4\n\nimport \"mid.wdl\" as m\n\nworkflow main {\n    output {\n        \
             String state = \"~{Code.Ready}\"\n    }\n}\n",
        ),
    ])
    .await;

    assert!(errors(&document).is_empty(), "{:?}", errors(&document));
    assert!(
        !document
            .diagnostics()
            .any(|d| d.message().starts_with("unused import namespace")),
        "{:?}",
        document
            .diagnostics()
            .map(|d| d.message().to_string())
            .collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn literal_of_untyped_reexported_struct_does_not_panic() {
    let document = analyze(&[
        (
            "base.wdl",
            "version 1.4\n\nstruct Bad {\n    Map[Int?, Int] bad\n}\n",
        ),
        (
            "mid.wdl",
            "version 1.4\n\nimport { Bad as Alias } from \"base.wdl\"\n\nstruct MidAnchor { Int \
             value }\n",
        ),
        (
            "source.wdl",
            "version 1.4\n\nimport * from \"mid.wdl\"\n\nworkflow main {\n    output {\n        \
             Alias value = Alias { bad: {} }\n    }\n}\n",
        ),
    ])
    .await;

    assert!(document.has_errors());
}

#[tokio::test]
async fn body_only_dependency_edits_refresh_cached_imports() {
    let valid = "version 1.4\n\ntask run {\n    command <<<\n        echo hi\n    >>>\n}\n";
    let invalid =
        "version 1.4\n\n\ntask run {\n    command <<<\n        echo ~{undefined}\n    >>>\n}\n";

    let dir = TempDir::new().expect("temporary directory should be created");
    let lib = dir.path().join("lib.wdl");
    fs::write(&lib, valid).expect("library should be written");
    fs::write(
        dir.path().join("source.wdl"),
        "version 1.4\n\nimport * from \"lib.wdl\"\n\nworkflow main {\n    call run\n}\n",
    )
    .expect("source should be written");

    let lib_uri = path_to_uri(&lib).expect("library URI should be valid");
    let source = path_to_uri(dir.path().join("source.wdl")).expect("source URI should be valid");
    let config = Config::default().with_feature_flags(FeatureFlags::default().with_wdl_1_4());
    let analyzer = Analyzer::new(config, |(), _, _, _| async {});
    analyzer
        .add_document(source.clone())
        .await
        .expect("source document should be added");

    let analyze_source = async || {
        analyzer
            .analyze(())
            .await
            .expect("analysis should succeed")
            .into_iter()
            .find(|result| *result.document().uri() == source)
            .expect("source result should exist")
            .document()
            .clone()
    };

    let document = analyze_source().await;
    assert!(!document.has_errors(), "{:?}", errors(&document));

    for (contents, expected) in [(invalid, true), (valid, false)] {
        fs::write(&lib, contents).expect("library should be written");
        analyzer
            .notify_change(lib_uri.clone(), false)
            .expect("change should be accepted");

        let document = analyze_source().await;
        assert!(errors(&document).is_empty(), "{:?}", errors(&document));
        assert_eq!(document.has_errors(), expected, "library: {contents}");
    }
}
