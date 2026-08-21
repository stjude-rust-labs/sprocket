//! Tests for the analysis cache's internal state.

use url::Url;
use wdl_grammar::Span;

use crate::AnalysisResult;
use crate::Analyzer;
use crate::Config;
use crate::IncrementalChange;
use crate::SourceEdit;
use crate::SourcePosition;
use crate::SourcePositionEncoding;
use crate::UnusedDeclarationRule;

struct DocumentHandle {
    version: i32,
    uri: Url,
}

impl DocumentHandle {
    async fn edit(&mut self, new_content: &str, analyzer: &Analyzer<()>) {
        self.version += 1;
        let initial_edit = IncrementalChange {
            version: self.version,
            start: Some(new_content.to_string()),
            edits: Vec::new(),
        };

        analyzer
            .notify_incremental_change(self.uri.clone(), initial_edit)
            .unwrap();
    }

    async fn analyze(&self, analyzer: &Analyzer<()>) -> AnalysisResult {
        let mut results = analyzer
            .analyze_document((), self.uri.clone())
            .await
            .unwrap();
        results.pop().unwrap()
    }
}

async fn setup_analyzer(initial_file_content: &str) -> (DocumentHandle, Analyzer<()>) {
    let uri: Url = "file:///main.wdl".parse().unwrap();
    let config = Config::default();
    let analyzer = Analyzer::new(config, |_, _, _, _| async {});
    analyzer.add_document(uri.clone()).await.unwrap();

    let mut doc = DocumentHandle { version: 0, uri };
    doc.edit(initial_file_content, &analyzer).await;

    (doc, analyzer)
}

#[tokio::test]
#[test_log::test]
async fn should_track_local_dependencies() {
    let initial_file = r#"version 1.3

task foo {
    command <<<
        echo "Hello, world!"
    >>>
}

workflow bar {
    call foo
}

task baz {
    command <<<
        echo "Hello, dependency-free world!"
    >>>
}
"#;

    let (mut doc_handle, analyzer) = setup_analyzer(initial_file).await;

    let result = doc_handle.analyze(&analyzer).await;
    let doc = result.document();
    let doc_data = doc.data();
    let cache = doc_data.cache();

    // We should have all 3 items in the cache
    assert_eq!(cache.len(), 3);
    let foo_hash = cache
        .item_by_name("foo")
        .expect("`foo` should exist")
        .signature_hash()
        .expect("`foo` is locally defined");
    let bar_hash = cache
        .item_by_name("bar")
        .expect("`bar` should exist")
        .signature_hash()
        .expect("`bar` is locally defined");
    let baz_hash = cache
        .item_by_name("baz")
        .expect("`baz` should exist")
        .signature_hash()
        .expect("`baz` is locally defined");

    // `bar` should depend on `foo`. There should be no edge to `baz`
    assert!(
        cache.dependencies.contains_edge(bar_hash, foo_hash),
        "bar should depend on foo"
    );
    assert!(
        !cache.dependencies.contains_edge(bar_hash, baz_hash),
        "bar should NOT depend on baz"
    );
    assert!(
        !cache.dependencies.contains_edge(foo_hash, baz_hash),
        "foo should NOT depend on baz"
    );

    // Now `foo` has a required input. Both it and `bar` should be re-analyzed, with
    // `bar` getting a diagnostic for the missing input.
    let edited_contents = r#"version 1.3

task foo {
    input {
        String name
    }

    command <<<
        echo "Hello, ~{name}!"
    >>>
}

workflow bar {
    call foo
}

task baz {
    command <<<
        echo "Hello, dependency-free world!"
    >>>
}
"#;
    doc_handle.edit(edited_contents, &analyzer).await;

    let result = doc_handle.analyze(&analyzer).await;
    let doc_post = result.document();
    let doc_data_post = doc_post.data();
    let cache_post = doc_data_post.cache();

    // We should still have all 3 items in the cache
    let new_foo_hash = cache_post
        .item_by_name("foo")
        .expect("`foo` should exist")
        .signature_hash()
        .expect("`foo` is locally defined");
    let new_bar_hash = cache_post
        .item_by_name("bar")
        .expect("`bar` should exist")
        .signature_hash()
        .expect("`bar` is locally defined");
    let new_baz_hash = cache_post
        .item_by_name("baz")
        .expect("`baz` should exist")
        .signature_hash()
        .expect("`baz` is locally defined");

    // Only `foo` should have a different hash
    assert_ne!(new_foo_hash, foo_hash);
    assert_eq!(new_bar_hash, bar_hash);
    assert_eq!(new_baz_hash, baz_hash);

    assert!(
        cache_post
            .dependencies
            .contains_edge(new_bar_hash, new_foo_hash),
        "bar should depend on foo after edit"
    );

    // Sanity that `foo` actually reflects the expected change
    let (_hash, foo_task) = cache_post
        .task_by_name("foo")
        .expect("foo should be a task");
    assert_eq!(
        foo_task.inputs().len(),
        1,
        "foo should have 1 input after edit"
    );

    // And `bar` should have been re-analyzed and produced a diagnostic
    let missing_input = result
        .document()
        .analysis_diagnostics()
        .iter()
        .find(|d| d.message() == "missing required call input `name` for task `foo`");
    assert!(missing_input.is_some());
}

#[tokio::test]
#[test_log::test]
async fn should_track_external_dependencies() {
    // Signature changes should always invalidate dependents, even across document
    // boundaries.

    let foo_uri: Url = "file:///foo.wdl".parse().unwrap();
    let main_uri: Url = "file:///main.wdl".parse().unwrap();
    let config = Config::default();
    let analyzer = Analyzer::new(config, |_, _, _, _| async {});

    // Create the foo document
    let mut doc_foo = {
        analyzer.add_document(foo_uri.clone()).await.unwrap();

        let initial_foo_content = r#"version 1.3

task foo {
    command <<<
        echo "Hello, world!
    >>>
}
"#;

        let mut doc_foo = DocumentHandle {
            version: 0,
            uri: foo_uri.clone(),
        };
        doc_foo.edit(initial_foo_content, &analyzer).await;
        doc_foo
    };

    // Create the main document
    let doc_main = {
        analyzer.add_document(main_uri.clone()).await.unwrap();
        let initial_main_content = r#"version 1.3

import "foo.wdl"

workflow bar {
    call foo.foo
}
"#;

        let mut doc_main = DocumentHandle {
            version: 0,
            uri: main_uri.clone(),
        };
        doc_main.edit(initial_main_content, &analyzer).await;
        doc_main.analyze(&analyzer).await;
        doc_main
    };

    // Now foo has a required input
    let foo_edit = r#"version 1.3

task foo {
    input {
        String name
    }

    command <<<
        echo "Hello, ~{name}!"
    >>>
}
"#;
    doc_foo.edit(foo_edit, &analyzer).await;

    // Re-analyze main
    let result = doc_main.analyze(&analyzer).await;

    // `bar` should have been re-analyzed and produced a diagnostic
    let missing_input = result
        .document()
        .analysis_diagnostics()
        .iter()
        .find(|d| d.message() == "missing required call input `name` for task `foo`");
    assert!(missing_input.is_some());
}

#[tokio::test]
#[test_log::test]
async fn should_drop_removed_items() {
    let initial_file = r#"version 1.3

struct ToBeRemoved {}
"#;

    let (mut doc_handle, analyzer) = setup_analyzer(initial_file).await;

    let result = doc_handle.analyze(&analyzer).await;
    let doc = result.document();
    let doc_data = doc.data();
    let cache = doc_data.cache();
    assert_eq!(cache.len(), 1);

    // `ToBeRemoved` should be dropped from the cache
    let edited_contents = r#"version 1.3"#;
    doc_handle.edit(edited_contents, &analyzer).await;

    let result = doc_handle.analyze(&analyzer).await;
    let doc_post = result.document();
    let doc_data_post = doc_post.data();
    let cache_post = doc_data_post.cache();
    assert!(cache_post.is_empty());
}

#[tokio::test]
#[test_log::test]
async fn should_isolate_local_body_changes() {
    // When editing the *body* of an item with dependents, we should only ever
    // invalidate the item itself and preserve the dependents.

    let initial_file = r#"version 1.3

task foo {
    command <<<
        echo "Hello, world!"
    >>>
}

workflow bar {
    call foo
}
"#;

    let (mut doc_handle, analyzer) = setup_analyzer(initial_file).await;

    let result = doc_handle.analyze(&analyzer).await;
    let doc = result.document();
    let cache = doc.data().cache();
    let foo_hash = cache
        .item_by_name("foo")
        .expect("`foo` should exist")
        .signature_hash()
        .expect("`foo` is locally defined");
    let bar_hash = cache
        .item_by_name("bar")
        .expect("`bar` should exist")
        .signature_hash()
        .expect("`bar` is locally defined");
    drop(result);

    // Now change only the body of `foo`
    let edited_contents = initial_file.replace("Hello", "Goodbye");
    doc_handle.edit(&edited_contents, &analyzer).await;

    let result = doc_handle.analyze(&analyzer).await;
    let doc_post = result.document();
    let cache_post = doc_post.data().cache();

    let new_foo_hash = cache_post
        .item_by_name("foo")
        .expect("`foo` should exist")
        .signature_hash()
        .expect("`foo` is locally defined");
    let new_bar_hash = cache_post
        .item_by_name("bar")
        .expect("`bar` should exist")
        .signature_hash()
        .expect("`bar` is locally defined");

    // The signature hashes should be identical since the signatures haven't changed
    assert_eq!(foo_hash, new_foo_hash);
    assert_eq!(bar_hash, new_bar_hash);

    // `foo` should have been in the body-invalidated set
    assert!(cache_post.tests.invalidated_bodies.contains(&foo_hash));
    assert!(!cache_post.tests.invalidated_signatures.contains(&foo_hash));

    // `bar` shouldn't be invalidated at all
    assert!(!cache_post.tests.invalidated_bodies.contains(&bar_hash));
    assert!(!cache_post.tests.invalidated_signatures.contains(&bar_hash));

    // The dependency edge from `bar` to `foo` must still exist
    assert!(
        cache_post
            .dependencies
            .contains_edge(new_bar_hash, new_foo_hash)
    );
}

#[tokio::test]
#[test_log::test]
async fn should_invalidate_dependents_of_dropped_items() {
    let initial_file = r#"version 1.3

task foo {
    command <<<
        echo "Hello, world!"
    >>>
}

workflow bar {
    call foo
}
"#;

    let (mut doc_handle, analyzer) = setup_analyzer(initial_file).await;

    let result = doc_handle.analyze(&analyzer).await;
    let doc = result.document();
    let cache = doc.data().cache();
    let foo_hash = cache
        .item_by_name("foo")
        .expect("`foo` should exist")
        .signature_hash()
        .expect("`foo` is locally defined");
    let bar_hash = cache
        .item_by_name("bar")
        .expect("`bar` should exist")
        .signature_hash()
        .expect("`bar` is locally defined");
    drop(result);

    // Now drop `foo`
    let edited_contents = r#"version 1.3

workflow bar {
    call foo
}
"#;
    doc_handle.edit(edited_contents, &analyzer).await;

    let result = doc_handle.analyze(&analyzer).await;
    let doc_post = result.document();
    let cache_post = doc_post.data().cache();

    // `foo` should be signature-invalidated because it was dropped
    assert!(cache_post.tests.invalidated_signatures.contains(&foo_hash));

    // `bar` should also be signature-invalidated because its dependency was dropped
    assert!(cache_post.tests.invalidated_signatures.contains(&bar_hash));
}

#[tokio::test]
#[test_log::test]
async fn should_shift_clean_item_diagnostics() {
    // Cached items store their diagnostics and reuse them between requests.
    // We should be updating the diagnostic offsets regardless if the item is dirty.

    let initial_file = r#"version 1.3

task taking_up_space {
    command <<<
        echo "Hello, world!"
    >>>
}

workflow stays_clean {
    String name = "super unused"
}
"#;

    let (mut doc_handle, analyzer) = setup_analyzer(initial_file).await;

    let result = doc_handle.analyze(&analyzer).await;
    let doc = result.document();
    let cache = doc.data().cache();
    let stays_clean_hash = cache
        .item_by_name("stays_clean")
        .expect("`stays_clean` should exist")
        .signature_hash()
        .expect("`stays_clean` is locally defined");

    let unused_decl_initial = result
        .document()
        .analysis_diagnostics()
        .iter()
        .find(|d| d.rule().is_some_and(|r| r == UnusedDeclarationRule::ID))
        .expect("`stays_clean` should produce an unused declaration diagnostic");
    let highlight_initial_span = unused_decl_initial.labels().next().unwrap().span();
    assert_eq!(highlight_initial_span, Span::new(126, 4));

    drop(result);

    // Now drop `taking_up_space`
    let edited_contents = r#"version 1.3

workflow stays_clean {
    String name = "super unused"
}
"#;
    doc_handle.edit(edited_contents, &analyzer).await;

    let result = doc_handle.analyze(&analyzer).await;
    let doc_post = result.document();
    let cache_post = doc_post.data().cache();

    // `stays_clean_hash` shouldn't be invalidated
    assert!(
        !cache_post
            .tests
            .invalidated_signatures
            .contains(&stays_clean_hash)
    );
    assert!(
        !cache_post
            .tests
            .invalidated_bodies
            .contains(&stays_clean_hash)
    );

    // The diagnostic should shift back with `stays_clean`
    let unused_decl_post = result
        .document()
        .analysis_diagnostics()
        .iter()
        .find(|d| d.rule().is_some_and(|r| r == UnusedDeclarationRule::ID))
        .expect("`stays_clean` should produce an unused declaration diagnostic");
    let highlight_post = unused_decl_post.labels().next().unwrap();
    assert_eq!(highlight_post.span(), Span::new(47, 4));

    // Add back `taking_up_space` for good measure
    doc_handle.edit(initial_file, &analyzer).await;

    let result = doc_handle.analyze(&analyzer).await;
    let unused_decl_post = result
        .document()
        .analysis_diagnostics()
        .iter()
        .find(|d| d.rule().is_some_and(|r| r == UnusedDeclarationRule::ID))
        .expect("`stays_clean` should produce an unused declaration diagnostic");
    let highlight_post = unused_decl_post.labels().next().unwrap();
    assert_eq!(
        highlight_post.span(),
        highlight_initial_span,
        "spans should be at their original positions"
    );
}

#[tokio::test]
#[test_log::test]
async fn should_invalidate_dependents_of_imported_structs() {
    // Imported structs/enums (using both the namespace and select/wildcard import
    // forms) should always invalidate their dependents on change.

    let foo_uri: Url = "file:///foo.wdl".parse().unwrap();
    let main_uri: Url = "file:///main.wdl".parse().unwrap();
    let config =
        Config::default().with_feature_flags(crate::config::FeatureFlags::default().with_wdl_1_4());
    let analyzer = Analyzer::new(config, |_, _, _, _| async {});

    // Create the foo document
    let mut doc_foo = {
        analyzer.add_document(foo_uri.clone()).await.unwrap();

        let initial_foo_content = r#"version 1.4

struct Person {
    String name
}

struct Person2 {
    String name
    Int height_inches
}
"#;

        let mut doc_foo = DocumentHandle {
            version: 0,
            uri: foo_uri.clone(),
        };
        doc_foo.edit(initial_foo_content, &analyzer).await;
        doc_foo
    };

    // Create the main document
    let mut doc_main = {
        analyzer.add_document(main_uri.clone()).await.unwrap();
        let initial_main_content = r#"version 1.4

import "foo.wdl"
import { Person2 } from "foo.wdl"

workflow bar {
    Person p = Person { name: "John" }
    Person2 p2 = Person2 { name: "John", height_inches: 72 }
}
"#;

        let mut doc_main = DocumentHandle {
            version: 0,
            uri: main_uri.clone(),
        };
        doc_main.edit(initial_main_content, &analyzer).await;
        doc_main.analyze(&analyzer).await;
        doc_main
    };

    // Add `age` to `Person`, should invalidate the `bar` workflow
    let foo_edit = r#"version 1.4

struct Person {
    String name
    Int age
}

struct Person2 {
    String name
    Int height_inches
}
"#;
    doc_foo.edit(foo_edit, &analyzer).await;

    // Re-analyze main
    let result = doc_main.analyze(&analyzer).await;

    // `bar` should now produce a diagnostic for the missing `age` field
    let has_diag = result.document().analysis_diagnostics().iter().any(|d| {
        d.message()
            .starts_with("struct `Person` requires a value for member")
    });
    assert!(has_diag);

    // Add the `age` field to the `Person` literal
    let revised_main = r#"version 1.4

import "foo.wdl"
import { Person2 } from "foo.wdl"

workflow bar {
    Person p = Person { name: "John", age: 26 }
    Person2 p2 = Person2 { name: "John", height_inches: 72 }
}
"#;
    doc_main.edit(revised_main, &analyzer).await;
    let result = doc_main.analyze(&analyzer).await;
    assert_eq!(
        result
            .document()
            .analysis_diagnostics()
            .iter()
            .filter(|d| d.severity().is_error())
            .count(),
        0,
        "all error diagnostics should be resolved"
    );

    // Now add an `age` field to `Person2`
    let foo_edit = r#"version 1.4

struct Person {
    String name
    Int age
}

struct Person2 {
    String name
    Int age
    Int height_inches
}
"#;
    doc_foo.edit(foo_edit, &analyzer).await;

    // Re-analyze main
    let result = doc_main.analyze(&analyzer).await;
    // `bar` should produce a diagnostic again for the change in `Person2`
    let has_diag = result.document().analysis_diagnostics().iter().any(|d| {
        d.message()
            .starts_with("struct `Person2` requires a value for member")
    });
    assert!(has_diag);
}

#[tokio::test]
#[test_log::test]
async fn trivia_should_shift_diagnostics() {
    // Trivia (spaces, comments) should shift the diagnostics in an item without
    // invalidating it.
    //
    // See `CachedItemRefMut::shift_existing_diagnostics()` for an explanation.

    let initial_content = r#"version 1.3

task foo {
    input {
        String unused_input
    }

    String unused_decl = "Hello"
}
"#;

    let (doc_handle, analyzer) = setup_analyzer(initial_content).await;
    let result = doc_handle.analyze(&analyzer).await;

    let unused_input = result
        .document()
        .analysis_diagnostics()
        .iter()
        .find(|d| d.rule().is_some_and(|r| r == "UnusedInput"))
        .expect("`foo` should produce an `UnusedInput` diagnostic");
    let initial_unused_input_span = unused_input.labels().next().unwrap().span();
    assert_eq!(initial_unused_input_span, Span::new(51, 12));

    let unused_decl = result
        .document()
        .analysis_diagnostics()
        .iter()
        .find(|d| d.rule().is_some_and(|r| r == "UnusedDeclaration"))
        .expect("`foo` should produce an `UnusedDeclaration` diagnostic");
    let initial_unused_decl_span = unused_decl.labels().next().unwrap().span();
    assert_eq!(initial_unused_decl_span, Span::new(82, 11));

    drop(result);

    analyzer
        .notify_incremental_change(
            doc_handle.uri.clone(),
            IncrementalChange {
                version: 2,
                start: None,
                edits: vec![SourceEdit::new(
                    SourcePosition::new(3, 0)..SourcePosition::new(3, 0),
                    SourcePositionEncoding::UTF8,
                    "    # This comment should shift both diagnostics\n".to_string(),
                )],
            },
        )
        .unwrap();
    let result2 = doc_handle.analyze(&analyzer).await;
    let cache = result2.document().data().cache();
    assert!(cache.tests.invalidated_signatures.is_empty());
    assert!(cache.tests.invalidated_bodies.is_empty());

    // The new comment should shift the diagnostics by 49 bytes
    let comment_shift = 49;
    let unused_input = result2
        .document()
        .analysis_diagnostics()
        .iter()
        .find(|d| d.rule() == Some("UnusedInput"))
        .expect("`foo` should produce an `UnusedInput` diagnostic");

    let unused_input_span2 = unused_input.labels().next().unwrap().span();
    assert_eq!(
        unused_input_span2,
        Span::new(
            initial_unused_input_span.start() + comment_shift,
            initial_unused_input_span.len()
        )
    );

    let unused_decl = result2
        .document()
        .analysis_diagnostics()
        .iter()
        .find(|d| d.rule() == Some("UnusedDeclaration"))
        .expect("`foo` should produce an `UnusedDeclaration` diagnostic");
    let unused_decl_span2 = unused_decl.labels().next().unwrap().span();
    assert_eq!(
        unused_decl_span2,
        Span::new(
            initial_unused_decl_span.start() + comment_shift,
            initial_unused_decl_span.len()
        )
    );

    drop(result2);

    analyzer
        .notify_incremental_change(
            doc_handle.uri.clone(),
            IncrementalChange {
                version: 3,
                start: None,
                edits: vec![SourceEdit::new(
                    SourcePosition::new(7, 0)..SourcePosition::new(7, 0),
                    SourcePositionEncoding::UTF8,
                    "    # This should shift the `UnusedDeclaration` diagnostic\n".to_string(),
                )],
            },
        )
        .unwrap();
    let result3 = doc_handle.analyze(&analyzer).await;
    let cache = result3.document().data().cache();
    assert!(cache.tests.invalidated_signatures.is_empty());
    assert!(cache.tests.invalidated_bodies.is_empty());

    let unused_input = result3
        .document()
        .analysis_diagnostics()
        .iter()
        .find(|d| d.rule().is_some_and(|r| r == "UnusedInput"))
        .expect("`foo` should produce an `UnusedInput` diagnostic");
    let unused_input_span3 = unused_input.labels().next().unwrap().span();
    assert_eq!(unused_input_span2, unused_input_span3);

    // The new comment should shift the diagnostic by 59 bytes
    let comment_shift = 59;
    let unused_decl = result3
        .document()
        .analysis_diagnostics()
        .iter()
        .find(|d| d.rule() == Some("UnusedDeclaration"))
        .expect("`foo` should produce an `UnusedDeclaration` diagnostic");
    let unused_decl_span3 = unused_decl.labels().next().unwrap().span();
    assert_eq!(
        unused_decl_span3,
        Span::new(
            unused_decl_span2.start() + comment_shift,
            unused_decl_span2.len()
        )
    );
}
