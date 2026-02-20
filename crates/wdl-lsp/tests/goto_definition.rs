//! Integration tests for the `textDocument/gotoDefinition` request.

mod common;
use core::panic;

use common::TestContext;
use pretty_assertions::assert_eq;
use tower_lsp_server::ls_types::GotoDefinitionParams;
use tower_lsp_server::ls_types::GotoDefinitionResponse;
use tower_lsp_server::ls_types::Position;
use tower_lsp_server::ls_types::Range;
use tower_lsp_server::ls_types::TextDocumentIdentifier;
use tower_lsp_server::ls_types::TextDocumentPositionParams;
use tower_lsp_server::ls_types::request::GotoDefinition;

async fn goto_definition_request(
    ctx: &mut TestContext,
    path: &str,
    position: Position,
) -> Option<GotoDefinitionResponse> {
    ctx.request::<GotoDefinition>(GotoDefinitionParams {
        text_document_position_params: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier {
                uri: ctx.doc_uri(path),
            },
            position,
        },
        work_done_progress_params: Default::default(),
        partial_result_params: Default::default(),
    })
    .await
}

async fn setup() -> TestContext {
    let mut ctx = TestContext::new("goto_definition");
    ctx.initialize().await;
    ctx
}

#[tokio::test]
async fn should_goto_local_variable_definition() {
    let mut ctx = setup().await;

    // Position of RHS `name` in `call greet as t1 { input: name = name }`
    let response = goto_definition_request(&mut ctx, "source.wdl", Position::new(20, 37))
        .await
        .unwrap();

    let GotoDefinitionResponse::Scalar(location) = response else {
        panic!("expected a single location response, got {:?}", response);
    };

    assert_eq!(location.uri, ctx.doc_uri("source.wdl"));
    assert_eq!(
        location.range,
        Range::new(Position::new(16, 15), Position::new(16, 19)) // `String name`
    );
}

#[tokio::test]
async fn should_goto_local_task_definition() {
    let mut ctx = setup().await;

    // Position of `greet` in `call greet`
    let response = goto_definition_request(&mut ctx, "source.wdl", Position::new(20, 9)).await;
    let Some(GotoDefinitionResponse::Scalar(location)) = response else {
        panic!("expected a single location response, got {:?}", response);
    };

    assert_eq!(location.uri, ctx.doc_uri("source.wdl"));
    assert_eq!(
        location.range,
        Range::new(Position::new(4, 5), Position::new(4, 10)) // `task greet`
    );
}

#[tokio::test]
async fn should_goto_imported_task_definition() {
    let mut ctx = setup().await;

    // Position of `add` in `call lib.add as t3`
    let response = goto_definition_request(&mut ctx, "source.wdl", Position::new(24, 13)).await;
    let Some(GotoDefinitionResponse::Scalar(location)) = response else {
        panic!("expected a single location response, got {:?}", response);
    };

    assert_eq!(location.uri, ctx.doc_uri("lib.wdl"));
    assert_eq!(
        location.range,
        Range::new(Position::new(8, 5), Position::new(8, 8)) // `task add` in lib.wdl
    );
}

#[tokio::test]
async fn should_goto_imported_struct_definition() {
    let mut ctx = setup().await;

    // Position of `Person` in `Person p`
    let response = goto_definition_request(&mut ctx, "source.wdl", Position::new(29, 4)).await;
    let Some(GotoDefinitionResponse::Scalar(location)) = response else {
        panic!("expected a single location response, got {:?}", response);
    };

    assert_eq!(location.uri, ctx.doc_uri("lib.wdl"));
    assert_eq!(
        location.range,
        Range::new(Position::new(23, 7), Position::new(23, 13)) // `struct Person` in lib.wdl
    );
}

#[tokio::test]
async fn should_goto_struct_member_definition() {
    let mut ctx = setup().await;

    // Position of `name` in `p.name`
    let response = goto_definition_request(&mut ctx, "source.wdl", Position::new(37, 22)).await;

    let Some(GotoDefinitionResponse::Scalar(location)) = response else {
        panic!("expected a single location response, got {:?}", response);
    };

    assert_eq!(location.uri, ctx.doc_uri("lib.wdl"));
    assert_eq!(
        location.range,
        Range::new(Position::new(24, 11), Position::new(24, 15)) // `String name` in Person struct
    );
}

#[tokio::test]
async fn should_goto_call_output_definition() {
    let mut ctx = setup().await;

    // Position of `result` in `t3.result`
    let response = goto_definition_request(&mut ctx, "source.wdl", Position::new(43, 24)).await;

    let Some(GotoDefinitionResponse::Scalar(location)) = response else {
        panic!("expected a single location response, got {:?}", response);
    };

    assert_eq!(location.uri, ctx.doc_uri("lib.wdl"));
    assert_eq!(
        location.range,
        Range::new(Position::new(19, 12), Position::new(19, 18)) // `Int result` in add's output
    );
}

#[tokio::test]
async fn should_goto_import_namespace_definition() {
    let mut ctx = setup().await;

    // Position of `lib` in `call lib.add as t3`
    let response = goto_definition_request(&mut ctx, "source.wdl", Position::new(24, 9)).await;

    let Some(GotoDefinitionResponse::Scalar(location)) = response else {
        panic!("expected a single location response, got {:?}", response);
    };

    assert_eq!(location.uri, ctx.doc_uri("source.wdl"));
    assert_eq!(
        location.range,
        Range::new(Position::new(2, 20), Position::new(2, 23)) // `as lib` in import statement
    );
}

#[tokio::test]
async fn should_goto_correct_definitions_in_access_expression() {
    let mut ctx = setup().await;

    // Position of `foo` in `Int x = foo.bar.baz.qux.x`
    let response = goto_definition_request(&mut ctx, "structs.wdl", Position::new(27, 16)).await;

    let Some(GotoDefinitionResponse::Scalar(location)) = response else {
        panic!("expected a single location response, got {:?}", response);
    };

    assert_eq!(location.uri, ctx.doc_uri("structs.wdl"));
    assert_eq!(
        location.range,
        Range::new(Position::new(20, 12), Position::new(20, 15)) // `foo` in `Foo foo`
    );

    // Position of `bar` in `Int x = foo.bar.baz.qux.x`
    let response = goto_definition_request(&mut ctx, "structs.wdl", Position::new(27, 20)).await;

    let Some(GotoDefinitionResponse::Scalar(location)) = response else {
        panic!("expected a single location response, got {:?}", response);
    };

    assert_eq!(location.uri, ctx.doc_uri("structs.wdl"));
    assert_eq!(
        location.range,
        Range::new(Position::new(15, 8), Position::new(15, 11)) // `bar` in `Bar bar`
    );

    // Position of `baz` in `Int x = foo.bar.baz.qux.x`
    let response = goto_definition_request(&mut ctx, "structs.wdl", Position::new(27, 24)).await;

    let Some(GotoDefinitionResponse::Scalar(location)) = response else {
        panic!("expected a single location response, got {:?}", response);
    };

    assert_eq!(location.uri, ctx.doc_uri("structs.wdl"));
    assert_eq!(
        location.range,
        Range::new(Position::new(11, 8), Position::new(11, 11)) // `baz` in `Baz baz`
    );

    // Position of `qux` in `Int x = foo.bar.baz.qux.x`
    let response = goto_definition_request(&mut ctx, "structs.wdl", Position::new(27, 28)).await;

    let Some(GotoDefinitionResponse::Scalar(location)) = response else {
        panic!("expected a single location response, got {:?}", response);
    };

    assert_eq!(location.uri, ctx.doc_uri("structs.wdl"));
    assert_eq!(
        location.range,
        Range::new(Position::new(7, 8), Position::new(7, 11)) // `qux` in `Qux qux`
    );

    // Position of RHS `x` in `Int x = foo.bar.baz.qux.x`
    let response = goto_definition_request(&mut ctx, "structs.wdl", Position::new(27, 32)).await;

    let Some(GotoDefinitionResponse::Scalar(location)) = response else {
        panic!("expected a single location response, got {:?}", response);
    };

    assert_eq!(location.uri, ctx.doc_uri("structs.wdl"));
    assert_eq!(
        location.range,
        Range::new(Position::new(3, 8), Position::new(3, 9)) // `x` in `Int x`
    );
}

#[tokio::test]
async fn should_goto_struct_member_definition_for_struct_literal() {
    let mut ctx = setup().await;

    // Position of `name:` in `Person p = Person { name: ... }`
    let response1 = goto_definition_request(&mut ctx, "source.wdl", Position::new(30, 8)).await;
    let Some(GotoDefinitionResponse::Scalar(location1)) = response1 else {
        panic!("expected a single location response, got {:?}", response1);
    };
    // Position of `age:` in `Person p = Person { name: ... , age: ...}`
    let response2 = goto_definition_request(&mut ctx, "source.wdl", Position::new(31, 8)).await;
    let Some(GotoDefinitionResponse::Scalar(location2)) = response2 else {
        panic!("expected a single location response, got {:?}", response2);
    };

    assert_eq!(location1.uri, ctx.doc_uri("lib.wdl"));
    assert_eq!(
        location1.range,
        Range::new(Position::new(24, 11), Position::new(24, 15)) // `name` in `String name`
    );

    assert_eq!(location2.uri, ctx.doc_uri("lib.wdl"));
    assert_eq!(
        location2.range,
        Range::new(Position::new(25, 8), Position::new(25, 11)) // `age` in `Int age`
    );
}

#[tokio::test]
async fn should_goto_call_task_input_definition() {
    let mut ctx = setup().await;

    // Position of `a`  in `a = 1,`
    let response = goto_definition_request(&mut ctx, "source.wdl", Position::new(25, 8)).await;
    let Some(GotoDefinitionResponse::Scalar(location)) = response else {
        panic!("expected a single location response, got {:?}", response);
    };

    assert_eq!(location.uri, ctx.doc_uri("lib.wdl"));
    assert_eq!(
        location.range,
        Range::new(Position::new(10, 12), Position::new(10, 13)) // `a` in `Int a`
    );
}

#[tokio::test]
async fn should_goto_call_workflow_input_definition() {
    let mut ctx = setup().await;

    // Position of `person`  in `call lib.process { input: person = p }`
    let response = goto_definition_request(&mut ctx, "source.wdl", Position::new(34, 30)).await;
    let Some(GotoDefinitionResponse::Scalar(location)) = response else {
        panic!("expected a single location response, got {:?}", response);
    };

    assert_eq!(location.uri, ctx.doc_uri("lib.wdl"));
    assert_eq!(
        location.range,
        Range::new(Position::new(30, 15), Position::new(30, 21)) // `person` in `Person person`
    );
}

#[tokio::test]
async fn should_goto_local_variable_for_abbreviated_call_input_syntax() {
    let mut ctx = setup().await;

    // Position of `name`  in `call greet as t2 { name }`
    let response = goto_definition_request(&mut ctx, "source.wdl", Position::new(22, 23)).await;
    let Some(GotoDefinitionResponse::Scalar(location)) = response else {
        panic!("expected a single location response, got {:?}", response);
    };

    assert_eq!(location.uri, ctx.doc_uri("source.wdl"));
    assert_eq!(
        location.range,
        Range::new(Position::new(16, 15), Position::new(16, 19)) // `name` in `String name`
    );
}

#[tokio::test]
async fn lhs_and_rhs_navigation_in_call_inputs_should_goto_correct_definintions() {
    let mut ctx = setup().await;

    // Position of LHS `name`  in `call greet as t1 { input: name = name }`
    let response1 = goto_definition_request(&mut ctx, "source.wdl", Position::new(20, 30)).await;
    let Some(GotoDefinitionResponse::Scalar(location1)) = response1 else {
        panic!("expected a single location response, got {:?}", response1);
    };

    // Position of RHS `name`  in `call greet as t1 { input: name = name }`
    let response2 = goto_definition_request(&mut ctx, "source.wdl", Position::new(20, 37)).await;
    let Some(GotoDefinitionResponse::Scalar(location2)) = response2 else {
        panic!("expected a single location response, got {:?}", response2);
    };

    assert_eq!(location1.uri, ctx.doc_uri("source.wdl"));
    assert_eq!(
        location1.range,
        Range::new(Position::new(6, 15), Position::new(6, 19)) // `name` in `String name`
    );

    assert_eq!(location2.uri, ctx.doc_uri("source.wdl"));
    assert_eq!(
        location2.range,
        Range::new(Position::new(16, 15), Position::new(16, 19)) // `name` in `String name`
    );
}

#[tokio::test]
async fn should_goto_enum_definition() {
    let mut ctx = setup().await;

    // Position of `Status` in `Status s`
    let response = goto_definition_request(&mut ctx, "enum.wdl", Position::new(9, 4))
        .await
        .unwrap();

    let GotoDefinitionResponse::Scalar(location) = response else {
        panic!("expected a single location response, got {:?}", response);
    };

    assert_eq!(location.uri, ctx.doc_uri("enum.wdl"));
    assert_eq!(
        location.range,
        Range::new(Position::new(2, 5), Position::new(2, 11)) // `enum Status`
    );
}

#[tokio::test]
async fn should_goto_enum_variant_definition() {
    let mut ctx = setup().await;

    // Position of `Active` in `Status.Active`
    let response = goto_definition_request(&mut ctx, "enum.wdl", Position::new(9, 22))
        .await
        .unwrap();

    let GotoDefinitionResponse::Scalar(location) = response else {
        panic!("expected a single location response, got {:?}", response);
    };

    assert_eq!(location.uri, ctx.doc_uri("enum.wdl"));
    assert_eq!(
        location.range,
        Range::new(Position::new(3, 4), Position::new(3, 10)) // `Active,`
    );
}

#[tokio::test]
async fn should_goto_enum_definition_in_member_access() {
    let mut ctx = setup().await;

    // Position of `Status` in `Status.Active`
    let response = goto_definition_request(&mut ctx, "enum.wdl", Position::new(9, 17))
        .await
        .unwrap();

    let GotoDefinitionResponse::Scalar(location) = response else {
        panic!("expected a single location response, got {:?}", response);
    };

    assert_eq!(location.uri, ctx.doc_uri("enum.wdl"));
    assert_eq!(
        location.range,
        Range::new(Position::new(2, 5), Position::new(2, 11)) // `enum Status`
    );
}

#[tokio::test]
async fn should_goto_imported_enum_definition() {
    let mut ctx = setup().await;

    // Position of `Priority` in `lib.Priority priority`
    let response = goto_definition_request(&mut ctx, "source.wdl", Position::new(40, 8))
        .await
        .unwrap();

    let GotoDefinitionResponse::Scalar(location) = response else {
        panic!("expected a single location response, got {:?}", response);
    };

    assert_eq!(location.uri, ctx.doc_uri("lib.wdl"));
    assert_eq!(
        location.range,
        Range::new(Position::new(2, 5), Position::new(2, 13)) // `enum Priority`
    );
}

#[tokio::test]
async fn should_goto_imported_enum_definition_in_member_access() {
    let mut ctx = setup().await;

    // Position of `Priority` in `lib.Priority.High`
    let response = goto_definition_request(&mut ctx, "source.wdl", Position::new(40, 35))
        .await
        .unwrap();

    let GotoDefinitionResponse::Scalar(location) = response else {
        panic!("expected a single location response, got {:?}", response);
    };

    assert_eq!(location.uri, ctx.doc_uri("lib.wdl"));
    assert_eq!(
        location.range,
        Range::new(Position::new(2, 5), Position::new(2, 13)) // `enum Priority`
    );
}
