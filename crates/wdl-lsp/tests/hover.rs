//! Integration tests for the `textDocument/completion` request.

mod common;

use core::panic;

use common::TestContext;
use tower_lsp::lsp_types::Hover;
use tower_lsp::lsp_types::HoverContents;
use tower_lsp::lsp_types::HoverParams;
use tower_lsp::lsp_types::MarkupContent;
use tower_lsp::lsp_types::Position;
use tower_lsp::lsp_types::TextDocumentIdentifier;
use tower_lsp::lsp_types::TextDocumentPositionParams;
use tower_lsp::lsp_types::request::HoverRequest;

async fn hover_request(ctx: &mut TestContext, path: &str, position: Position) -> Option<Hover> {
    ctx.request::<HoverRequest>(HoverParams {
        text_document_position_params: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier {
                uri: ctx.doc_uri(path),
            },
            position,
        },
        work_done_progress_params: Default::default(),
    })
    .await
}

fn assert_hover_content(hover: &Option<Hover>, expected: &str) {
    let Some(hover) = hover else {
        panic!("expected a hover response, but got none.");
    };

    let HoverContents::Markup(MarkupContent { value, .. }) = &hover.contents else {
        panic!("expected markup content in hover response.");
    };

    assert!(
        value.contains(expected),
        "hover content did not contain expected string.\nExpected to find: `{expected}`\nActual: \
         `{value}`"
    );
}

async fn setup() -> TestContext {
    let mut ctx = TestContext::new("hover");
    ctx.initialize().await;
    ctx
}

#[tokio::test]
async fn should_hover_local_variable() {
    let mut ctx = setup().await;
    let response = hover_request(&mut ctx, "source.wdl", Position::new(6, 15)).await;
    assert_hover_content(&response, "```wdl\n(variable) name: String\n```");
}

#[tokio::test]
async fn should_hover_struct_definition() {
    let mut ctx = setup().await;
    // Positon of `Person` in `struct Person`
    let response = hover_request(&mut ctx, "lib.wdl", Position::new(16, 7)).await;
    assert_hover_content(&response, "struct Person {");
    assert_hover_content(&response, "**Members**\n- **name**: `String`");
}

#[tokio::test]
async fn should_hover_struct_object() {
    let mut ctx = setup().await;
    // Position of `Person` in `Person p`
    let response = hover_request(&mut ctx, "source.wdl", Position::new(9, 4)).await;
    assert_hover_content(&response, "struct Person");
}

#[tokio::test]
async fn should_hover_task_definition() {
    let mut ctx = setup().await;
    // Position of `greet` in `task greet`
    let response = hover_request(&mut ctx, "lib.wdl", Position::new(2, 7)).await;
    assert_hover_content(&response, "task greet");
    // Inputs
    assert_hover_content(&response, "**name**: `String`");
    // Outputs
    assert_hover_content(&response, "**out**: `String`");
}

#[tokio::test]
async fn should_hover_task_call() {
    let mut ctx = setup().await;
    // Position of `greet` in `call greet`
    let response = hover_request(&mut ctx, "source.wdl", Position::new(14, 9)).await;
    assert_hover_content(&response, "task greet");
}

#[tokio::test]
async fn should_hover_imported_task_call() {
    let mut ctx = setup().await;
    // Position of `greet` in `call lib.greet`
    let response = hover_request(&mut ctx, "source.wdl", Position::new(16, 13)).await;
    assert_hover_content(&response, "```wdl\ntask greet\n```");
    assert_hover_content(&response, "**Outputs**\n- **out**: `String`");
}

#[tokio::test]
async fn should_hover_import_namespace() {
    let mut ctx = setup().await;
    // Position of `lib` in `call lib.greet`
    let response = hover_request(&mut ctx, "source.wdl", Position::new(16, 9)).await;
    assert_hover_content(&response, "(import) lib");
    assert_hover_content(&response, "Imports from `");
    let imported_doc_path = ctx.doc_uri("lib.wdl");
    assert_hover_content(&response, imported_doc_path.as_ref());
}

#[tokio::test]
async fn should_hover_stdlib_function() {
    let mut ctx = setup().await;
    // Position of `read_string`
    let response = hover_request(&mut ctx, "source.wdl", Position::new(20, 24)).await;
    assert_hover_content(&response, "read_string(file: File) -> String");
    assert_hover_content(&response, "Reads an entire file as a `String`");
}

#[tokio::test]
async fn should_hover_struct_member_access() {
    let mut ctx = setup().await;
    // Position of `name` in `p.name`
    let response = hover_request(&mut ctx, "source.wdl", Position::new(11, 24)).await;
    assert_hover_content(&response, "(property) name: String");
}

#[tokio::test]
async fn should_hover_call_output_access() {
    let mut ctx = setup().await;
    // Position of `out` in `t.out`
    let response = hover_request(&mut ctx, "source.wdl", Position::new(20, 38)).await;
    assert_hover_content(&response, "(property) out: String");
}

#[tokio::test]
async fn should_hover_workflow_definition() {
    let mut ctx = setup().await;
    // Position of `out` in `t.out`
    let response = hover_request(&mut ctx, "source.wdl", Position::new(4, 9)).await;
    assert_hover_content(&response, "workflow main");
    // Inputs
    assert_hover_content(&response, "**name**: `String` = *`\"world\"`*");
    // Outputs
    assert_hover_content(&response, "**result**: `String`");
}

#[tokio::test]
async fn should_hover_local_variable_docs() {
    let mut ctx = setup().await;
    let response = hover_request(&mut ctx, "meta.wdl", Position::new(23, 16)).await;
    assert_hover_content(&response, "(variable) message: String");
    assert_hover_content(&response, "Text to be printed");
}

#[tokio::test]
async fn should_hover_local_struct_member_access_docs() {
    let mut ctx = setup().await;
    let response = hover_request(&mut ctx, "meta.wdl", Position::new(20, 22)).await;
    assert_hover_content(&response, "(property) name: String");
    assert_hover_content(&response, "Name of the person");
}

#[tokio::test]
async fn should_hover_local_struct_literal_docs() {
    let mut ctx = setup().await;
    let response = hover_request(&mut ctx, "meta.wdl", Position::new(29, 8)).await;
    assert_hover_content(&response, "(property) name: String");
    assert_hover_content(&response, "Name of the person");
}
