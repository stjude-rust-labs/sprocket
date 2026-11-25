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
        r#"import * from "lib.wdl""#,
        r#"import { imported } from "lib.wdl""#,
    ] {
        let source = format!(
            r#"version 1.4

{import}

workflow local {{}}
"#
        );
        let document = analyze(&[
            (
                "lib.wdl",
                r#"version 1.4

workflow imported {}
"#,
            ),
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
            (
                "a.wdl",
                r#"version 1.4

workflow alpha {}
"#,
            ),
            (
                "b.wdl",
                r#"version 1.4

workflow beta {}
"#,
            ),
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
        (
            "base.wdl",
            r#"version 1.4

workflow shared {}
"#,
        ),
        (
            "mid.wdl",
            r#"version 1.4

import * from "base.wdl"

struct Relay { Int x }
"#,
        ),
        (
            "source.wdl",
            r#"version 1.4

import * from "base.wdl"
import * from "mid.wdl"

struct Anchor { Int value }
"#,
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
        (
            "a.wdl",
            r#"version 1.4

workflow alpha {}
"#,
        ),
        (
            "b.wdl",
            r#"version 1.4

workflow beta {}
"#,
        ),
        (
            "source.wdl",
            r#"version 1.4

import "a.wdl" as a
import "b.wdl" as b

workflow local {}
"#,
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
            r#"version 1.4

task do_task {
    command <<<>>>
    output { Int out = 1 }
}

workflow do_flow {
    output { Int out = 2 }
}
"#,
        ),
        (
            "mid.wdl",
            r#"version 1.4

import * from "base.wdl"

struct Marker { Int value }
"#,
        ),
        (
            "source.wdl",
            r#"version 1.4

import { do_task } from "mid.wdl"
import { do_flow } from "mid.wdl"

struct Anchor { Int value }
"#,
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
            r#"version 1.4

struct Record {
    Int value
}

enum State {
    Ready,
    Done
}

task run_task {
    command <<<>>>
    output { Int out = 1 }
}

workflow run_workflow {
    output { Int out = 2 }
}
"#,
        ),
        (
            "source.wdl",
            r#"version 1.4

import * from "lib.wdl"

task use_types {
    input {
        Record rec
        State state
    }

    command <<<>>>

    output {
        Int out = rec.value
        State result = state
    }
}
"#,
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
        (
            "a.wdl",
            r#"version 1.4

workflow alpha {}
"#,
        ),
        (
            "b.wdl",
            r#"version 1.4

workflow beta {}
"#,
        ),
        (
            "source.wdl",
            r#"version 1.4

import { alpha as foo } from "a.wdl"
import { beta as bar } from "b.wdl"

struct Anchor { Int value }
"#,
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
        (
            "a.wdl",
            r#"version 1.4

workflow alpha {}
"#,
        ),
        (
            "source.wdl",
            r#"version 1.4

import { alpha as foo } from "a.wdl"
import { alpha as bar } from "a.wdl"

struct Anchor { Int value }
"#,
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
        (
            "lib.wdl",
            r#"version 1.4

workflow foo {}
"#,
        ),
        (
            "source.wdl",
            r#"version 1.4

import { foo } from "lib.wdl"

workflow foo {}
"#,
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
            r#"version 1.4

workflow child {
    output { Int out = 1 }
}
"#,
        ),
        (
            "mid.wdl",
            r#"version 1.4

import * from "base.wdl"

struct Marker { Int value }
"#,
        ),
        (
            "source.wdl",
            r#"version 1.4

import "mid.wdl" as mid

workflow main {
    call mid.child
    output { Int out = child.out }
}
"#,
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
            r#"version 1.4

struct Record {
    Int value
}

enum State {
    Ready,
    Done
}

task greet {
    command <<<>>>
    output { Int out = 1 }
}

workflow sub {
    output { Int out = 2 }
}
"#,
        ),
        (
            "mid.wdl",
            r#"version 1.4

import { Record as Item, State as Code, greet as say_hi, sub as run_sub } from "base.wdl"
"#,
        ),
        (
            "source.wdl",
            r#"version 1.4

import * from "mid.wdl"

workflow main {
    input {
        Item item
        Code code = Code.Ready
    }

    call say_hi
    call run_sub

    output {
        Int value = item.value
        Code result = code
    }
}
"#,
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
    let document = analyze(&[
        (
            "base.wdl",
            r#"version 1.4

struct Record {
    Int value
}

enum State {
    Ready,
    Done
}
"#,
        ),
        (
            "mid.wdl",
            r#"version 1.4

import "base.wdl" as b

struct Marker { Int value }
"#,
        ),
        (
            "source.wdl",
            r#"version 1.4

import { Record, State } from "mid.wdl"

workflow main {
    input {
        Record rec
        State state = State.Done
    }

    output {
        Int value = rec.value
        State result = state
    }
}
"#,
        ),
    ])
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
    let broken = r#"version 1.4

task broken {
    command <<<>>>
    output { Int out = "not an int" }
}
"#;

    for (mid, source) in [
        (
            None,
            r#"version 1.4

import * from "lib.wdl"

struct Anchor { Int value }
"#,
        ),
        (
            None,
            r#"version 1.4

import { broken } from "lib.wdl"

struct Anchor { Int value }
"#,
        ),
        (
            Some(
                r#"version 1.4

import { broken as relay } from "lib.wdl"

struct MidAnchor { Int value }
"#,
            ),
            r#"version 1.4

import * from "mid.wdl"

struct Anchor { Int value }
"#,
        ),
        (
            Some(
                r#"version 1.4

import * from "lib.wdl"

struct MidAnchor { Int value }
"#,
            ),
            r#"version 1.4

import { broken } from "mid.wdl"

struct Anchor { Int value }
"#,
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
    let document = analyze(&[
        (
            "base.wdl",
            r#"version 1.4

struct Record {
    Int value
}

enum State {
    Ready,
    Done
}
"#,
        ),
        (
            "mid.wdl",
            r#"version 1.4

import { Record, State as Code } from "base.wdl"
"#,
        ),
        (
            "source.wdl",
            r#"version 1.4

import "mid.wdl" as m
import "mid.wdl" as n

workflow main {
    input {
        Record rec
    }

    output {
        Code code = Code.Ready
    }
}
"#,
        ),
    ])
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
            r#"version 1.4

enum State {
    Ready,
    Done
}
"#,
        ),
        (
            "mid.wdl",
            r#"version 1.4

import { State as Code } from "base.wdl"

struct MidAnchor { Int value }
"#,
        ),
        (
            "source.wdl",
            r#"version 1.4

import "mid.wdl" as m

workflow main {
    output {
        String state = "~{Code.Ready}"
    }
}
"#,
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
            r#"version 1.4

struct Bad {
    Map[Int?, Int] bad
}
"#,
        ),
        (
            "mid.wdl",
            r#"version 1.4

import { Bad as Alias } from "base.wdl"

struct MidAnchor { Int value }
"#,
        ),
        (
            "source.wdl",
            r#"version 1.4

import * from "mid.wdl"

workflow main {
    output {
        Alias value = Alias { bad: {} }
    }
}
"#,
        ),
    ])
    .await;

    assert!(document.has_errors());
}

#[tokio::test]
async fn body_only_dependency_edits_refresh_cached_imports() {
    let valid = r#"version 1.4

task run {
    command <<<
        echo hi
    >>>
}
"#;
    let invalid = r#"version 1.4


task run {
    command <<<
        echo ~{undefined}
    >>>
}
"#;

    let dir = TempDir::new().expect("temporary directory should be created");
    let lib = dir.path().join("lib.wdl");
    fs::write(&lib, valid).expect("library should be written");
    fs::write(
        dir.path().join("source.wdl"),
        r#"version 1.4

import * from "lib.wdl"

workflow main {
    call run
}
"#,
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

#[tokio::test]
async fn body_only_dependency_edits_refresh_imported_output_spans() {
    // Only the command and `meta` contents change between these, so the
    // signatures (and thus the exports hash) stay the same while the outputs
    // below them move
    let original = r#"version 1.4

task work {
    command <<<>>>

    output {
        Int out = 1
    }
}

workflow flow {
    meta {}

    output {
        Int out = 2
    }
}
"#;
    let shifted = r#"version 1.4

task work {
    command <<<
        echo "this moves the task outputs"
    >>>

    output {
        Int out = 1
    }
}

workflow flow {
    meta {
        description: "this moves the workflow outputs"
    }

    output {
        Int out = 2
    }
}
"#;

    let dir = TempDir::new().expect("temporary directory should be created");
    let base = dir.path().join("base.wdl");
    fs::write(&base, original).expect("base should be written");
    fs::write(
        dir.path().join("mid.wdl"),
        r#"version 1.4

import * from "base.wdl"

struct Anchor { Int value }
"#,
    )
    .expect("mid should be written");
    fs::write(
        dir.path().join("source.wdl"),
        r#"version 1.4

import * from "mid.wdl"

struct SourceAnchor { Int value }
"#,
    )
    .expect("source should be written");

    let uri = |name: &str| path_to_uri(dir.path().join(name)).expect("URI should be valid");
    let uris = [uri("base.wdl"), uri("mid.wdl"), uri("source.wdl")];
    let config = Config::default().with_feature_flags(FeatureFlags::default().with_wdl_1_4());
    let analyzer = Analyzer::new(config, |(), _, _, _| async {});
    analyzer
        .add_document(uris[2].clone())
        .await
        .expect("source document should be added");

    // Returns the task and workflow output spans of `base.wdl` followed by the
    // spans cached by its direct (`mid.wdl`) and transitive (`source.wdl`)
    // importers
    let output_spans = async || {
        let results = analyzer.analyze(()).await.expect("analysis should succeed");
        let documents = uris.each_ref().map(|uri| {
            results
                .iter()
                .find(|result| *result.document().uri() == *uri)
                .expect("result should exist")
                .document()
                .clone()
        });

        for document in &documents {
            assert!(errors(document).is_empty(), "{:?}", errors(document));
        }

        let [base, importers @ ..] = &documents;
        let local = (
            base.local_task_by_name("work")
                .expect("task should exist")
                .outputs()["out"]
                .name_span(),
            base.local_workflow_by_name("flow")
                .expect("workflow should exist")
                .outputs()["out"]
                .name_span(),
        );
        let imported = importers.each_ref().map(|document| {
            (
                document
                    .imported_task_by_name("work")
                    .expect("task should be imported")
                    .outputs()["out"]
                    .name_span(),
                document
                    .imported_workflow_by_name("flow")
                    .expect("workflow should be imported")
                    .outputs()["out"]
                    .name_span(),
            )
        });
        (local, imported)
    };

    let (initial, imported) = output_spans().await;
    assert_eq!(imported, [initial; 2]);

    fs::write(&base, shifted).expect("base should be written");
    analyzer
        .notify_change(uris[0].clone(), false)
        .expect("change should be accepted");

    let (shifted, imported) = output_spans().await;
    assert_ne!(shifted.0, initial.0);
    assert_ne!(shifted.1, initial.1);
    assert_eq!(imported, [shifted; 2]);
}

#[tokio::test]
async fn transitive_export_changes_invalidate_downstream_importers() {
    let base = |suffix: &str| {
        format!(
            r#"version 1.4

struct Record{suffix} {{
    Int value
}}

enum State{suffix} {{
    Ready
}}

task work{task_suffix} {{
    command <<<>>>

    output {{
        Int out = 1
    }}
}}
"#,
            task_suffix = if suffix.is_empty() { "" } else { "_v2" },
        )
    };

    let dir = TempDir::new().expect("temporary directory should be created");
    let base_path = dir.path().join("base.wdl");
    fs::write(&base_path, base("")).expect("base should be written");
    fs::write(
        dir.path().join("mid.wdl"),
        r#"version 1.4

import * from "base.wdl"

struct Anchor {
    Int value
}
"#,
    )
    .expect("mid should be written");
    fs::write(
        dir.path().join("source.wdl"),
        r#"version 1.4

import * from "mid.wdl"

workflow main {
    input {
        Record rec
        State state = State.Ready
    }

    call work
}
"#,
    )
    .expect("source should be written");

    let base_uri = path_to_uri(&base_path).expect("base URI should be valid");
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
    assert!(errors(&document).is_empty(), "{:?}", errors(&document));
    assert!(document.imported_task_by_name("work").is_some());
    assert!(document.struct_by_name("Record").is_some());
    assert!(document.enum_by_name("State").is_some());

    fs::write(&base_path, base("V2")).expect("base should be written");
    analyzer
        .notify_change(base_uri, false)
        .expect("change should be accepted");

    let document = analyze_source().await;
    let errors = errors(&document);
    for name in ["work", "Record", "State"] {
        assert!(
            errors.iter().any(|e| e.contains(&format!("`{name}`"))),
            "expected an error for `{name}`: {errors:?}"
        );
    }
    assert!(document.imported_task_by_name("work").is_none());
    assert!(document.struct_by_name("Record").is_none());
    assert!(document.enum_by_name("State").is_none());
    assert!(document.imported_task_by_name("work_v2").is_some());
    assert!(document.struct_by_name("RecordV2").is_some());
    assert!(document.enum_by_name("StateV2").is_some());
}
