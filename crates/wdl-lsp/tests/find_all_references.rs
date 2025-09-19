//! Integration tests for the `textDocument/references` request.

mod common;
use core::panic;

use common::TestContext;
use pretty_assertions::assert_eq;
use tower_lsp::lsp_types::Location;
use tower_lsp::lsp_types::Position;
use tower_lsp::lsp_types::Range;
use tower_lsp::lsp_types::ReferenceContext;
use tower_lsp::lsp_types::ReferenceParams;
use tower_lsp::lsp_types::TextDocumentIdentifier;
use tower_lsp::lsp_types::TextDocumentPositionParams;
use tower_lsp::lsp_types::request::References;

async fn find_all_references(
    ctx: &mut TestContext,
    path: &str,
    position: Position,
    include_declaration: bool,
) -> Option<Vec<Location>> {
    ctx.request::<References>(ReferenceParams {
        context: ReferenceContext {
            include_declaration,
        },
        text_document_position: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier {
                uri: ctx.doc_uri(path),
            },
            position,
        },
        partial_result_params: Default::default(),
        work_done_progress_params: Default::default(),
    })
    .await
}

async fn setup() -> TestContext {
    let mut ctx = TestContext::new("find_references");
    ctx.initialize().await;
    ctx
}

#[tokio::test]
async fn should_have_references_to_struct() {
    let mut ctx = setup().await;

    // Position of `Person` in `struct Person`
    let response = find_all_references(&mut ctx, "structs.wdl", Position::new(2, 7), false)
        .await
        .unwrap();

    assert!(!response.is_empty());
    assert!(
        response.len() == 2,
        "references should not contain declaration"
    );

    // Position of `Person` in `struct Person`
    let response = find_all_references(&mut ctx, "structs.wdl", Position::new(2, 7), true)
        .await
        .unwrap();

    assert!(response.len() == 3, "references should contain declaration");
}

#[tokio::test]
async fn should_have_references_across_files() {
    let mut ctx = setup().await;

    // Position of `Person` in `struct Person`
    let response = find_all_references(&mut ctx, "structs.wdl", Position::new(2, 7), true)
        .await
        .unwrap();

    let Some(location) = response
        .iter()
        .find(|l| l.uri == ctx.doc_uri("structs.wdl"))
    else {
        panic!("reference should exist");
    };
    assert_eq!(
        location.range,
        Range::new(Position::new(2, 7), Position::new(2, 13))
    ); // `Person` in `struct Person`

    let Some(location) = response.iter().find(|l| l.uri == ctx.doc_uri("source.wdl")) else {
        panic!("reference should exist");
    };
    assert_eq!(
        location.range,
        Range::new(Position::new(7, 8), Position::new(7, 14))
    ); // `Person` in `Person person`

    let Some(location) = response.iter().find(|l| l.uri == ctx.doc_uri("foo.wdl")) else {
        panic!("reference should exist");
    };
    assert_eq!(
        location.range,
        Range::new(Position::new(6, 8), Position::new(6, 14))
    ); // `Person` in `Person person`
}

#[tokio::test]
async fn should_have_references_to_struct_members() {
    let mut ctx = setup().await;

    // Position of `name` in `String name`
    let response = find_all_references(&mut ctx, "structs.wdl", Position::new(3, 11), false)
        .await
        .unwrap();

    let mut locations = response.iter();

    let Some(location) = locations.next() else {
        panic!("reference should exist");
    };

    assert_eq!(location.uri, ctx.doc_uri("foo.wdl"),);
    assert_eq!(
        location.range,
        Range::new(Position::new(10, 23), Position::new(10, 27))
    ); // `name` in `echo "~{person.name}"`
}

#[tokio::test]
async fn should_have_references_to_local_variables() {
    let mut ctx = setup().await;

    // Position of `person` in `Person person`
    let response = find_all_references(&mut ctx, "foo.wdl", Position::new(6, 15), false)
        .await
        .unwrap();

    let mut locations = response.iter();

    let Some(location) = locations.next() else {
        panic!("reference should exist");
    };

    assert_eq!(location.uri, ctx.doc_uri("foo.wdl"),);
    assert_eq!(
        location.range,
        Range::new(Position::new(10, 16), Position::new(10, 22))
    ); // `person` in `echo "~{person.name}"`
}

#[tokio::test]
async fn should_have_references_to_tasks() {
    let mut ctx = setup().await;

    // Position of `greet` in `task greet`
    let response = find_all_references(&mut ctx, "foo.wdl", Position::new(4, 5), false)
        .await
        .unwrap();

    let mut locations = response.iter();

    let Some(location) = locations.next() else {
        panic!("reference should exist");
    };

    assert_eq!(location.uri, ctx.doc_uri("source.wdl"),);
    assert_eq!(
        location.range,
        Range::new(Position::new(10, 13), Position::new(10, 18))
    ); // `greet` in `call lib.greet`
}

#[tokio::test]
async fn should_have_references_to_tasks_output() {
    let mut ctx = setup().await;

    // Position of `name` in `String name`
    let response = find_all_references(&mut ctx, "foo.wdl", Position::new(14, 15), false)
        .await
        .unwrap();

    let mut locations = response.iter();

    let Some(location) = locations.next() else {
        panic!("reference should exist");
    };

    assert_eq!(location.uri, ctx.doc_uri("source.wdl"),);
    assert_eq!(
        location.range,
        Range::new(Position::new(13, 26), Position::new(13, 30))
    ); // `name` in `String result = t.name`
}
