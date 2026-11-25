//! Integration tests for the `textDocument/typeDefinition` request.

use async_lsp::lsp_types::Location;
use async_lsp::lsp_types::Position;
use async_lsp::lsp_types::Range;
use async_lsp::lsp_types::TextDocumentIdentifier;
use async_lsp::lsp_types::TextDocumentPositionParams;
use async_lsp::lsp_types::request::GotoTypeDefinition;
use async_lsp::lsp_types::request::GotoTypeDefinitionParams;
use async_lsp::lsp_types::request::GotoTypeDefinitionResponse;

use crate::common::TestContext;
use crate::common::TestContextBuilder;

async fn type_definition_request(
    ctx: &mut TestContext,
    path: &str,
    position: Position,
) -> async_lsp::Result<Option<GotoTypeDefinitionResponse>> {
    ctx.request::<GotoTypeDefinition>(GotoTypeDefinitionParams {
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

async fn assert_type_definition(
    ctx: &mut TestContext,
    path: &str,
    position: Position,
    expected_path: &str,
    expected_range: Range,
) {
    let response = type_definition_request(ctx, path, position)
        .await
        .expect("request should succeed")
        .unwrap_or_else(|| {
            panic!(
                "expected type definition at {path}:{}:{}",
                position.line, position.character
            )
        });

    let GotoTypeDefinitionResponse::Scalar(Location { uri, range }) = response else {
        panic!("expected a single location response, got {response:?}");
    };

    assert_eq!(uri, ctx.doc_uri(expected_path));
    assert_eq!(range, expected_range);
}

async fn setup() -> TestContext {
    let mut ctx = TestContextBuilder::new("type_definition").build();
    ctx.initialize().await;
    ctx
}

/// Range of the `LocalPerson` struct ident.
const LOCAL_PERSON_RANGE: Range = Range {
    start: Position {
        line: 4,
        character: 7,
    },
    end: Position {
        line: 4,
        character: 18,
    },
};

/// Range of the `ExternalPerson` struct ident.
const EXTERNAL_PERSON_RANGE: Range = Range {
    start: Position {
        line: 2,
        character: 7,
    },
    end: Position {
        line: 2,
        character: 21,
    },
};

/// Range of the `LocalColor` enum ident.
const LOCAL_COLOR_RANGE: Range = Range {
    start: Position {
        line: 8,
        character: 5,
    },
    end: Position {
        line: 8,
        character: 15,
    },
};

/// Range of the `ExternalColor` enum ident.
const EXTERNAL_COLOR_RANGE: Range = Range {
    start: Position {
        line: 6,
        character: 5,
    },
    end: Position {
        line: 6,
        character: 18,
    },
};

#[tokio::test]
async fn should_goto_type_from_type_name() {
    let mut ctx = setup().await;

    // Position of `LocalPerson` in `LocalPerson local = LocalPerson { ... }`
    assert_type_definition(
        &mut ctx,
        "source.wdl",
        Position::new(18, 5),
        "source.wdl",
        LOCAL_PERSON_RANGE,
    )
    .await;

    // Position of `LocalPerson` in struct literal `LocalPerson { ... }`
    assert_type_definition(
        &mut ctx,
        "source.wdl",
        Position::new(18, 26),
        "source.wdl",
        LOCAL_PERSON_RANGE,
    )
    .await;

    // Position of `ExternalPerson` in `ExternalPerson external = { ... }`
    assert_type_definition(
        &mut ctx,
        "source.wdl",
        Position::new(19, 5),
        "external.wdl",
        EXTERNAL_PERSON_RANGE,
    )
    .await;

    // Position of `LocalColor` in `LocalColor color = ...`
    assert_type_definition(
        &mut ctx,
        "source.wdl",
        Position::new(21, 5),
        "source.wdl",
        LOCAL_COLOR_RANGE,
    )
    .await;

    // Position of `LocalColor` in `LocalColor.Red`
    assert_type_definition(
        &mut ctx,
        "source.wdl",
        Position::new(21, 25),
        "source.wdl",
        LOCAL_COLOR_RANGE,
    )
    .await;
}

#[tokio::test]
async fn should_goto_type_from_decl_name() {
    let mut ctx = setup().await;

    // Position of `local` in `LocalPerson local = ...`
    assert_type_definition(
        &mut ctx,
        "source.wdl",
        Position::new(18, 17),
        "source.wdl",
        LOCAL_PERSON_RANGE,
    )
    .await;

    // Position of `external` in `ExternalPerson external = ...`
    assert_type_definition(
        &mut ctx,
        "source.wdl",
        Position::new(19, 21),
        "external.wdl",
        EXTERNAL_PERSON_RANGE,
    )
    .await;

    // Position of `maybe_local` in `LocalPerson? maybe_local = local`
    assert_type_definition(
        &mut ctx,
        "source.wdl",
        Position::new(20, 18),
        "source.wdl",
        LOCAL_PERSON_RANGE,
    )
    .await;

    // Position of `color` variable declaration
    assert_type_definition(
        &mut ctx,
        "source.wdl",
        Position::new(21, 16),
        "source.wdl",
        LOCAL_COLOR_RANGE,
    )
    .await;
}

#[tokio::test]
async fn should_goto_type_from_access_expr() {
    let mut ctx = setup().await;

    // Position of `Red` choice in `LocalColor.Red`
    assert_type_definition(
        &mut ctx,
        "source.wdl",
        Position::new(21, 35),
        "source.wdl",
        LOCAL_COLOR_RANGE,
    )
    .await;

    // Position of `person` in `wrapper.person`
    assert_type_definition(
        &mut ctx,
        "source.wdl",
        Position::new(23, 36),
        "source.wdl",
        LOCAL_PERSON_RANGE,
    )
    .await;

    // Position of `left` in `pair.left`
    assert_type_definition(
        &mut ctx,
        "source.wdl",
        Position::new(25, 34),
        "source.wdl",
        LOCAL_PERSON_RANGE,
    )
    .await;

    // Position of `out` call output in `greet_external.out`
    assert_type_definition(
        &mut ctx,
        "source.wdl",
        Position::new(30, 46),
        "external.wdl",
        EXTERNAL_PERSON_RANGE,
    )
    .await;
}

#[tokio::test]
async fn should_goto_type_from_name_ref_expr() {
    let mut ctx = setup().await;

    // Position of `local` in `LocalPerson? maybe_local = local`
    assert_type_definition(
        &mut ctx,
        "source.wdl",
        Position::new(20, 32),
        "source.wdl",
        LOCAL_PERSON_RANGE,
    )
    .await;

    // Position of `person` struct literal field key in `LocalPersonWrapper {
    // person: local }`
    assert_type_definition(
        &mut ctx,
        "source.wdl",
        Position::new(22, 55),
        "source.wdl",
        LOCAL_PERSON_RANGE,
    )
    .await;

    // Position of `person` call input key in `call external.greet_external {
    // input: person = external }`
    assert_type_definition(
        &mut ctx,
        "source.wdl",
        Position::new(29, 13),
        "external.wdl",
        EXTERNAL_PERSON_RANGE,
    )
    .await;
}

#[tokio::test]
async fn should_resolve_aliases() {
    let mut ctx = setup().await;

    // Source and target names in `alias ExternalPerson as AliasedPerson`
    assert_type_definition(
        &mut ctx,
        "aliased.wdl",
        Position::new(2, 30),
        "external.wdl",
        EXTERNAL_PERSON_RANGE,
    )
    .await;
    assert_type_definition(
        &mut ctx,
        "aliased.wdl",
        Position::new(2, 48),
        "external.wdl",
        EXTERNAL_PERSON_RANGE,
    )
    .await;

    // Source and target names in `alias ExternalColor as AliasedColor`
    assert_type_definition(
        &mut ctx,
        "aliased.wdl",
        Position::new(2, 68),
        "external.wdl",
        EXTERNAL_COLOR_RANGE,
    )
    .await;
    assert_type_definition(
        &mut ctx,
        "aliased.wdl",
        Position::new(2, 85),
        "external.wdl",
        EXTERNAL_COLOR_RANGE,
    )
    .await;

    // Aliased struct type name, decl name, literal, and usage
    for pos in [
        Position::new(5, 6),
        Position::new(5, 18),
        Position::new(5, 24),
        Position::new(6, 15),
    ] {
        assert_type_definition(
            &mut ctx,
            "aliased.wdl",
            pos,
            "external.wdl",
            EXTERNAL_PERSON_RANGE,
        )
        .await;
    }

    // Aliased enum type name, decl name, and choice access
    // (`AliasedColor.Green`)
    for pos in [
        Position::new(7, 6),
        Position::new(7, 17),
        Position::new(7, 36),
    ] {
        assert_type_definition(
            &mut ctx,
            "aliased.wdl",
            pos,
            "external.wdl",
            EXTERNAL_COLOR_RANGE,
        )
        .await;
    }

    // Selected member import with alias (`import { ExternalPerson as
    // SelectedPerson, ... } from "external.wdl"`)
    for pos in [
        Position::new(2, 11),
        Position::new(2, 29),
        Position::new(5, 6),
        Position::new(5, 19),
        Position::new(9, 40),
    ] {
        assert_type_definition(
            &mut ctx,
            "selected.wdl",
            pos,
            "external.wdl",
            EXTERNAL_PERSON_RANGE,
        )
        .await;
    }
}

#[tokio::test]
async fn should_goto_type_of_members() {
    let mut ctx = setup().await;

    // Position of `Red` choice inside `enum LocalColor { Red, Blue }`
    assert_type_definition(
        &mut ctx,
        "source.wdl",
        Position::new(9, 5),
        "source.wdl",
        LOCAL_COLOR_RANGE,
    )
    .await;

    // Position of `person` member in `LocalPersonWrapper`
    assert_type_definition(
        &mut ctx,
        "source.wdl",
        Position::new(14, 17),
        "source.wdl",
        LOCAL_PERSON_RANGE,
    )
    .await;
}

#[tokio::test]
async fn should_goto_type_for_scatter_variables() {
    let mut ctx = setup().await;

    // Position of `p` in `scatter (p in people)`
    assert_type_definition(
        &mut ctx,
        "source.wdl",
        Position::new(33, 13),
        "source.wdl",
        LOCAL_PERSON_RANGE,
    )
    .await;
}

#[tokio::test]
async fn should_not_goto_type_for_callables() {
    let mut ctx = setup().await;

    // Position of `greet_external` in `call external.greet_external`
    let response = type_definition_request(&mut ctx, "source.wdl", Position::new(27, 19))
        .await
        .expect("request should succeed");
    assert!(response.is_none());

    // Position of `greet_external` in `import { ..., greet_external }`
    let response = type_definition_request(&mut ctx, "selected.wdl", Position::new(2, 45))
        .await
        .expect("request should succeed");
    assert!(response.is_none());
}
