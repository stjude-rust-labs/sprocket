//! Integration tests for the `workspace/willRenameFiles` request.

use async_lsp::lsp_types::request::WillRenameFiles;
use async_lsp::lsp_types::*;
use pretty_assertions::assert_eq;

use crate::common::TestContext;
use crate::common::TestContextBuilder;

async fn setup() -> TestContext {
    let mut ctx = TestContextBuilder::new("will_rename_files").build();
    ctx.initialize().await;
    ctx
}

fn verify_edits(mut expected: Vec<TextEdit>, received: &[TextEdit]) {
    for item in received {
        let matched = expected.iter().position(|expected| expected == item);

        if let Some(index) = matched {
            expected.remove(index);
        } else {
            panic!("unexpected text edit returned: {item:?}");
        }
    }

    assert!(
        expected.is_empty(),
        "some expected items were not returned: {expected:?}"
    );
}

#[tokio::test]
#[test_log::test]
async fn should_handle_file_renames() {
    let ctx = setup().await;

    // `./dep.wdl` -> `./dep_new.wdl`
    let edit = ctx
        .request::<WillRenameFiles>(RenameFilesParams {
            files: vec![FileRename {
                old_uri: ctx.doc_uri("dep.wdl").to_string(),
                new_uri: ctx.doc_uri("dep_new.wdl").to_string(),
            }],
        })
        .await
        .unwrap()
        .expect("should produce edit");

    let expected_changes = vec![
        TextEdit {
            // First "dep.wdl" import
            range: Range {
                start: Position::new(2, 8),
                end: Position::new(2, 15),
            },
            new_text: "dep_new.wdl".to_string(),
        },
        TextEdit {
            // Second "dep.wdl" import
            range: Range {
                start: Position::new(3, 8),
                end: Position::new(3, 15),
            },
            new_text: "dep_new.wdl".to_string(),
        },
        TextEdit {
            // Implicit "dep" namespace
            range: Range {
                start: Position::new(6, 9),
                end: Position::new(6, 12),
            },
            new_text: "dep_new".to_string(),
        },
    ];

    let changes = edit.changes.expect("should produce changes");
    assert_eq!(changes.len(), 1);

    let changes = changes
        .get(&ctx.doc_uri("source.wdl"))
        .expect("should only produce changes for source.wdl");
    verify_edits(expected_changes, changes);
}

#[tokio::test]
#[test_log::test]
async fn should_handle_dependency_directory_changes() {
    // Moving a file with dependents should update all import statements and
    // implicit namespaces referencing it.

    let ctx = setup().await;

    // `./dep.wdl` -> `./some_dir/dep.wdl`
    let mut new_path = ctx.doc_path("dep.wdl");
    assert!(new_path.pop());
    new_path.push("some_dir");
    new_path.push("dep.wdl");

    let edit = ctx
        .request::<WillRenameFiles>(RenameFilesParams {
            files: vec![FileRename {
                old_uri: ctx.doc_uri("dep.wdl").to_string(),
                new_uri: Url::from_file_path(new_path).unwrap().to_string(),
            }],
        })
        .await
        .unwrap()
        .expect("should produce edit");

    let expected_changes = vec![
        TextEdit {
            // First "dep.wdl" import
            range: Range {
                start: Position::new(2, 8),
                end: Position::new(2, 15),
            },
            new_text: "some_dir/dep.wdl".to_string(),
        },
        TextEdit {
            // Second "dep.wdl" import
            range: Range {
                start: Position::new(3, 8),
                end: Position::new(3, 15),
            },
            new_text: "some_dir/dep.wdl".to_string(),
        },
    ];

    let changes = edit.changes.expect("should produce changes");
    assert_eq!(changes.len(), 1);

    let changes = changes
        .get(&ctx.doc_uri("source.wdl"))
        .expect("should only produce changes for source.wdl");
    verify_edits(expected_changes, changes);
}

#[tokio::test]
#[test_log::test]
async fn should_handle_directory_changes() {
    // Moving a file with dependencies needs to update the paths of dependencies
    // relative to its new location.
    //
    // In this case, `source.wdl` moves into another directory while depending
    // on the sibling `dep.wdl`. All `dep.wdl` imports need to be updated to
    // `../dep.wdl`.

    let ctx = setup().await;

    // `./source.wdl` -> `./some_dir/source.wdl`
    let mut new_path = ctx.doc_path("source.wdl");
    assert!(new_path.pop());
    new_path.push("some_dir");
    new_path.push("source.wdl");

    let edit = ctx
        .request::<WillRenameFiles>(RenameFilesParams {
            files: vec![FileRename {
                old_uri: ctx.doc_uri("source.wdl").to_string(),
                new_uri: Url::from_file_path(new_path).unwrap().to_string(),
            }],
        })
        .await
        .unwrap()
        .expect("should produce edit");

    let expected_changes = vec![
        TextEdit {
            // First "dep.wdl" import
            range: Range {
                start: Position::new(2, 8),
                end: Position::new(2, 15),
            },
            new_text: "../dep.wdl".to_string(),
        },
        TextEdit {
            // Second "dep.wdl" import
            range: Range {
                start: Position::new(3, 8),
                end: Position::new(3, 15),
            },
            new_text: "../dep.wdl".to_string(),
        },
    ];

    let changes = edit.changes.expect("should produce changes");
    assert_eq!(changes.len(), 1);

    let changes = changes
        .get(&ctx.doc_uri("source.wdl"))
        .expect("should only produce changes for source.wdl");
    verify_edits(expected_changes, changes);
}

#[tokio::test]
#[test_log::test]
async fn should_not_emit_noop_edits() {
    let ctx = setup().await;

    // `./source.wdl` -> `./source_renamed.wdl` in same directory
    let edit = ctx
        .request::<WillRenameFiles>(RenameFilesParams {
            files: vec![FileRename {
                old_uri: ctx.doc_uri("source.wdl").to_string(),
                new_uri: ctx.doc_uri("source_renamed.wdl").to_string(),
            }],
        })
        .await
        .unwrap();

    assert!(
        edit.is_none(),
        "should not produce any edits for rename without dependents"
    );

    // `./` shouldn't be dropped from relative paths
    ctx.create_file(
        "dot_sibling.wdl",
        r#"version 1.3

import "./dep.wdl"

workflow do_foo {
    call dep.say_hello
}
"#,
    )
    .await;

    // `./dot_sibling.wdl` -> `./dot_sibling_renamed.wdl` in same directory
    let edit = ctx
        .request::<WillRenameFiles>(RenameFilesParams {
            files: vec![FileRename {
                old_uri: ctx.doc_uri("dot_sibling.wdl").to_string(),
                new_uri: ctx.doc_uri("dot_sibling_renamed.wdl").to_string(),
            }],
        })
        .await
        .unwrap();

    assert!(edit.is_none());
}

#[tokio::test]
#[test_log::test]
async fn should_handle_batch_renames() {
    let ctx = setup().await;

    // Both files move to `some_dir/`
    let mut new_source = ctx.doc_path("source.wdl");
    assert!(new_source.pop());
    new_source.push("some_dir");
    new_source.push("source.wdl");

    let mut new_dep = ctx.doc_path("dep.wdl");
    assert!(new_dep.pop());
    new_dep.push("some_dir");
    new_dep.push("dep.wdl");

    let edit = ctx
        .request::<WillRenameFiles>(RenameFilesParams {
            files: vec![
                FileRename {
                    old_uri: ctx.doc_uri("source.wdl").to_string(),
                    new_uri: Url::from_file_path(new_source).unwrap().to_string(),
                },
                FileRename {
                    old_uri: ctx.doc_uri("dep.wdl").to_string(),
                    new_uri: Url::from_file_path(new_dep).unwrap().to_string(),
                },
            ],
        })
        .await
        .unwrap();

    // Since both files moved into the same directory together, the relative
    // import remains "dep.wdl"
    assert!(
        edit.is_none(),
        "should not produce any edits when both move together"
    );
}

#[tokio::test]
#[test_log::test]
async fn should_handle_invalid_stem() {
    let ctx = setup().await;

    // `./dep.wdl` -> `./dep-new.wdl` (dep-new makes an invalid identifier)
    let edit = ctx
        .request::<WillRenameFiles>(RenameFilesParams {
            files: vec![FileRename {
                old_uri: ctx.doc_uri("dep.wdl").to_string(),
                new_uri: ctx.doc_uri("dep-new.wdl").to_string(),
            }],
        })
        .await
        .unwrap()
        .expect("should produce edit");

    let changes = edit.changes.expect("should produce changes");
    assert_eq!(changes.len(), 1);

    let changes = changes
        .get(&ctx.doc_uri("source.wdl"))
        .expect("should only produce changes for source.wdl");

    let expected_changes = vec![
        TextEdit {
            // First "dep.wdl" import
            range: Range {
                start: Position::new(2, 8),
                end: Position::new(2, 15),
            },
            new_text: "dep-new.wdl".to_string(),
        },
        TextEdit {
            // Second "dep.wdl" import
            range: Range {
                start: Position::new(3, 8),
                end: Position::new(3, 15),
            },
            new_text: "dep-new.wdl".to_string(),
        },
        // Shouldn't update any usage of the namespace since the identifier is invalid
    ];

    verify_edits(expected_changes, changes);
}

#[tokio::test]
#[test_log::test]
async fn should_handle_other_import_forms() {
    let ctx = setup().await;

    ctx.create_file(
        "wildcard_source.wdl",
        r#"version 1.3

import * from "dep.wdl"

workflow do_wildcard {
    call say_hello
}
"#,
    )
    .await;

    // `./dep.wdl` -> `./dep_new.wdl`
    let edit = ctx
        .request::<WillRenameFiles>(RenameFilesParams {
            files: vec![FileRename {
                old_uri: ctx.doc_uri("dep.wdl").to_string(),
                new_uri: ctx.doc_uri("dep_new.wdl").to_string(),
            }],
        })
        .await
        .unwrap()
        .expect("should produce edit");

    let changes = edit.changes.expect("should produce changes");
    let wildcard_changes = changes
        .get(&ctx.doc_uri("wildcard_source.wdl"))
        .expect("should produce changes for wildcard_source.wdl");

    let expected_changes = vec![TextEdit {
        range: Range {
            start: Position::new(2, 15),
            end: Position::new(2, 22),
        },
        new_text: "dep_new.wdl".to_string(),
    }];

    verify_edits(expected_changes, wildcard_changes);
}
