//! Integration tests for the `textDocument/semanticTokens` request.

use async_lsp::ErrorCode;
use async_lsp::lsp_types::SemanticToken;
use async_lsp::lsp_types::SemanticTokenModifier;
use async_lsp::lsp_types::SemanticTokenType;
use async_lsp::lsp_types::SemanticTokensDeltaParams;
use async_lsp::lsp_types::SemanticTokensFullDeltaResult;
use async_lsp::lsp_types::SemanticTokensParams;
use async_lsp::lsp_types::SemanticTokensRangeParams;
use async_lsp::lsp_types::SemanticTokensRangeResult;
use async_lsp::lsp_types::SemanticTokensResult;
use async_lsp::lsp_types::TextDocumentIdentifier;
use async_lsp::lsp_types::request::SemanticTokensFullDeltaRequest;
use async_lsp::lsp_types::request::SemanticTokensFullRequest;
use async_lsp::lsp_types::request::SemanticTokensRangeRequest;
use pretty_assertions::assert_eq;
use wdl_analysis::handlers::WDL_SEMANTIC_TOKEN_MODIFIERS;
use wdl_analysis::handlers::WDL_SEMANTIC_TOKEN_TYPES;

use crate::common::TestContext;

pub mod common;

async fn semantic_tokens_full_request(
    ctx: &mut TestContext,
    path: &str,
) -> async_lsp::Result<Option<SemanticTokensResult>> {
    ctx.request::<SemanticTokensFullRequest>(SemanticTokensParams {
        text_document: TextDocumentIdentifier {
            uri: ctx.doc_uri(path),
        },
        work_done_progress_params: Default::default(),
        partial_result_params: Default::default(),
    })
    .await
}

async fn semantic_tokens_full_delta_request(
    ctx: &mut TestContext,
    path: &str,
) -> async_lsp::Result<Option<SemanticTokensFullDeltaResult>> {
    ctx.request::<SemanticTokensFullDeltaRequest>(SemanticTokensDeltaParams {
        text_document: TextDocumentIdentifier {
            uri: ctx.doc_uri(path),
        },
        work_done_progress_params: Default::default(),
        partial_result_params: Default::default(),
        previous_result_id: String::new(),
    })
    .await
}

async fn semantic_tokens_range_request(
    ctx: &mut TestContext,
    path: &str,
) -> async_lsp::Result<Option<SemanticTokensRangeResult>> {
    ctx.request::<SemanticTokensRangeRequest>(SemanticTokensRangeParams {
        text_document: TextDocumentIdentifier {
            uri: ctx.doc_uri(path),
        },
        work_done_progress_params: Default::default(),
        partial_result_params: Default::default(),
        range: Default::default(),
    })
    .await
}

async fn setup() -> TestContext {
    let mut ctx = TestContext::new("semantic_tokens");
    ctx.initialize().await;
    ctx
}

fn token_type_index(ty: SemanticTokenType) -> u32 {
    WDL_SEMANTIC_TOKEN_TYPES
        .iter()
        .position(|tt| tt == &ty)
        .unwrap_or_else(|| panic!("token type `{ty:?}` not found in `WDL_SEMANTIC_TOKEN_TYPES`"))
        as u32
}

fn modifiers(modifiers: &[SemanticTokenModifier]) -> u32 {
    modifiers.iter().fold(0, |mut acc, modifier| {
        let pos = WDL_SEMANTIC_TOKEN_MODIFIERS
            .iter()
            .position(|m| m == modifier)
            .unwrap_or_else(|| {
                panic!("token modifier `{modifier:?}` not found in `WDL_SEMANTIC_TOKEN_MODIFIERS`")
            });
        acc |= 1 << pos;

        acc
    })
}

#[tokio::test]
async fn should_provide_semantic_tokens() {
    let mut ctx = setup().await;
    let result = semantic_tokens_full_request(&mut ctx, "source.wdl")
        .await
        .expect("request should succeed")
        .unwrap();

    let SemanticTokensResult::Tokens(tokens) = result else {
        panic!("unexpected partial result");
    };

    let expected_tokens = vec![
        // Comment
        SemanticToken {
            delta_line: 0,
            delta_start: 0,
            length: 40,
            token_type: token_type_index(SemanticTokenType::COMMENT),
            token_modifiers_bitset: modifiers(&[]),
        },
        // == VERSION ==

        // Version keyword
        SemanticToken {
            delta_line: 2,
            delta_start: 0,
            length: 7,
            token_type: token_type_index(SemanticTokenType::KEYWORD),
            token_modifiers_bitset: modifiers(&[]),
        },
        // Version number
        SemanticToken {
            delta_line: 0,
            delta_start: 8,
            length: 3,
            token_type: token_type_index(SemanticTokenType::NUMBER),
            token_modifiers_bitset: modifiers(&[]),
        },
        // == IMPORT ==

        // Import keyword
        SemanticToken {
            delta_line: 2,
            delta_start: 0,
            length: 6,
            token_type: token_type_index(SemanticTokenType::KEYWORD),
            token_modifiers_bitset: modifiers(&[]),
        },
        // Import path
        SemanticToken {
            delta_line: 0,
            delta_start: 7,
            length: 1,
            token_type: token_type_index(SemanticTokenType::STRING),
            token_modifiers_bitset: modifiers(&[]),
        },
        SemanticToken {
            delta_line: 0,
            delta_start: 1,
            length: 7,
            token_type: token_type_index(SemanticTokenType::STRING),
            token_modifiers_bitset: modifiers(&[]),
        },
        SemanticToken {
            delta_line: 0,
            delta_start: 7,
            length: 1,
            token_type: token_type_index(SemanticTokenType::STRING),
            token_modifiers_bitset: modifiers(&[]),
        },
        // Import alias

        // `as` keyword
        SemanticToken {
            delta_line: 0,
            delta_start: 2,
            length: 2,
            token_type: token_type_index(SemanticTokenType::KEYWORD),
            token_modifiers_bitset: modifiers(&[]),
        },
        // alias name
        SemanticToken {
            delta_line: 0,
            delta_start: 3,
            length: 3,
            token_type: token_type_index(SemanticTokenType::NAMESPACE),
            token_modifiers_bitset: modifiers(&[SemanticTokenModifier::DECLARATION]),
        },
        // == STRUCT ==

        // Struct keyword
        SemanticToken {
            delta_line: 2,
            delta_start: 0,
            length: 6,
            token_type: token_type_index(SemanticTokenType::KEYWORD),
            token_modifiers_bitset: modifiers(&[]),
        },
        // Struct name
        SemanticToken {
            delta_line: 0,
            delta_start: 7,
            length: 3,
            token_type: token_type_index(SemanticTokenType::STRUCT),
            token_modifiers_bitset: modifiers(&[SemanticTokenModifier::DEFINITION]),
        },
        // Member type
        SemanticToken {
            delta_line: 1,
            delta_start: 4,
            length: 3,
            token_type: token_type_index(SemanticTokenType::TYPE),
            token_modifiers_bitset: modifiers(&[]),
        },
        // Member name
        SemanticToken {
            delta_line: 0,
            delta_start: 4,
            length: 3,
            token_type: token_type_index(SemanticTokenType::PROPERTY),
            token_modifiers_bitset: modifiers(&[]),
        },
        // == ENUM ==

        // Enum keyword
        SemanticToken {
            delta_line: 3,
            delta_start: 0,
            length: 4,
            token_type: token_type_index(SemanticTokenType::KEYWORD),
            token_modifiers_bitset: modifiers(&[]),
        },
        // Enum name
        SemanticToken {
            delta_line: 0,
            delta_start: 5,
            length: 5,
            token_type: token_type_index(SemanticTokenType::ENUM),
            token_modifiers_bitset: modifiers(&[SemanticTokenModifier::DEFINITION]),
        },
        // Enum choice
        SemanticToken {
            delta_line: 1,
            delta_start: 4,
            length: 5,
            token_type: token_type_index(SemanticTokenType::ENUM_MEMBER),
            token_modifiers_bitset: modifiers(&[SemanticTokenModifier::DEFINITION]),
        },
        // == TASK ==

        // Task keyword
        SemanticToken {
            delta_line: 3,
            delta_start: 0,
            length: 4,
            token_type: token_type_index(SemanticTokenType::KEYWORD),
            token_modifiers_bitset: modifiers(&[]),
        },
        // Task name
        SemanticToken {
            delta_line: 0,
            delta_start: 5,
            length: 7,
            token_type: token_type_index(SemanticTokenType::FUNCTION),
            token_modifiers_bitset: modifiers(&[SemanticTokenModifier::DEFINITION]),
        },
        // == COMMAND ==

        // Command keyword
        SemanticToken {
            delta_line: 1,
            delta_start: 4,
            length: 7,
            token_type: token_type_index(SemanticTokenType::KEYWORD),
            token_modifiers_bitset: modifiers(&[]),
        },
        // == OUTPUT ==

        // Output keyword
        SemanticToken {
            delta_line: 2,
            delta_start: 4,
            length: 6,
            token_type: token_type_index(SemanticTokenType::KEYWORD),
            token_modifiers_bitset: modifiers(&[]),
        },
        // Array type
        SemanticToken {
            delta_line: 1,
            delta_start: 8,
            length: 5,
            token_type: token_type_index(SemanticTokenType::TYPE),
            token_modifiers_bitset: modifiers(&[]),
        },
        // File type
        SemanticToken {
            delta_line: 0,
            delta_start: 6,
            length: 4,
            token_type: token_type_index(SemanticTokenType::TYPE),
            token_modifiers_bitset: modifiers(&[]),
        },
        // `files` variable name
        SemanticToken {
            delta_line: 0,
            delta_start: 6,
            length: 5,
            token_type: token_type_index(SemanticTokenType::VARIABLE),
            token_modifiers_bitset: modifiers(&[]),
        },
        // =
        SemanticToken {
            delta_line: 0,
            delta_start: 6,
            length: 1,
            token_type: token_type_index(SemanticTokenType::OPERATOR),
            token_modifiers_bitset: modifiers(&[]),
        },
        // glob
        SemanticToken {
            delta_line: 0,
            delta_start: 2,
            length: 4,
            token_type: token_type_index(SemanticTokenType::FUNCTION),
            token_modifiers_bitset: modifiers(&[SemanticTokenModifier::DEFAULT_LIBRARY]),
        },
        // "
        SemanticToken {
            delta_line: 0,
            delta_start: 5,
            length: 1,
            token_type: token_type_index(SemanticTokenType::STRING),
            token_modifiers_bitset: modifiers(&[]),
        },
        // *
        SemanticToken {
            delta_line: 0,
            delta_start: 1,
            length: 1,
            token_type: token_type_index(SemanticTokenType::STRING),
            token_modifiers_bitset: modifiers(&[]),
        },
        // "
        SemanticToken {
            delta_line: 0,
            delta_start: 1,
            length: 1,
            token_type: token_type_index(SemanticTokenType::STRING),
            token_modifiers_bitset: modifiers(&[]),
        },
        // Doc comment
        SemanticToken {
            delta_line: 4,
            delta_start: 0,
            length: 16,
            token_type: token_type_index(SemanticTokenType::COMMENT),
            token_modifiers_bitset: modifiers(&[SemanticTokenModifier::DOCUMENTATION]),
        },
        // == WORKFLOW ==

        // Workflow keyword
        SemanticToken {
            delta_line: 1,
            delta_start: 0,
            length: 8,
            token_type: token_type_index(SemanticTokenType::KEYWORD),
            token_modifiers_bitset: modifiers(&[]),
        },
        // Workflow name
        SemanticToken {
            delta_line: 0,
            delta_start: 9,
            length: 2,
            token_type: token_type_index(SemanticTokenType::FUNCTION),
            token_modifiers_bitset: modifiers(&[SemanticTokenModifier::DEFINITION]),
        },
        // == CALL STATEMENT ==

        // Call keyword
        SemanticToken {
            delta_line: 1,
            delta_start: 4,
            length: 4,
            token_type: token_type_index(SemanticTokenType::KEYWORD),
            token_modifiers_bitset: modifiers(&[]),
        },
        // `do_work` task name
        SemanticToken {
            delta_line: 0,
            delta_start: 5,
            length: 7,
            token_type: token_type_index(SemanticTokenType::FUNCTION),
            token_modifiers_bitset: modifiers(&[]),
        },
        // == NAMESPACED CALL ==

        // Call keyword
        SemanticToken {
            delta_line: 2,
            delta_start: 4,
            length: 4,
            token_type: token_type_index(SemanticTokenType::KEYWORD),
            token_modifiers_bitset: modifiers(&[]),
        },
        // `foo` namespace
        SemanticToken {
            delta_line: 0,
            delta_start: 5,
            length: 3,
            token_type: token_type_index(SemanticTokenType::NAMESPACE),
            token_modifiers_bitset: modifiers(&[]),
        },
        // .
        SemanticToken {
            delta_line: 0,
            delta_start: 3,
            length: 1,
            token_type: token_type_index(SemanticTokenType::OPERATOR),
            token_modifiers_bitset: modifiers(&[]),
        },
        // `bar` task name
        SemanticToken {
            delta_line: 0,
            delta_start: 1,
            length: 3,
            token_type: token_type_index(SemanticTokenType::FUNCTION),
            token_modifiers_bitset: modifiers(&[]),
        },
        // == VARIABLE DECLARATION ==

        // `Hello` enum type name
        SemanticToken {
            delta_line: 2,
            delta_start: 4,
            length: 5,
            token_type: token_type_index(SemanticTokenType::ENUM),
            token_modifiers_bitset: modifiers(&[]),
        },
        // Variable name
        SemanticToken {
            delta_line: 0,
            delta_start: 6,
            length: 1,
            token_type: token_type_index(SemanticTokenType::VARIABLE),
            token_modifiers_bitset: modifiers(&[]),
        },
        // =
        SemanticToken {
            delta_line: 0,
            delta_start: 2,
            length: 1,
            token_type: token_type_index(SemanticTokenType::OPERATOR),
            token_modifiers_bitset: modifiers(&[]),
        },
        // `Hello` enum name
        SemanticToken {
            delta_line: 0,
            delta_start: 2,
            length: 5,
            token_type: token_type_index(SemanticTokenType::ENUM),
            token_modifiers_bitset: modifiers(&[]),
        },
        // .
        SemanticToken {
            delta_line: 0,
            delta_start: 5,
            length: 1,
            token_type: token_type_index(SemanticTokenType::OPERATOR),
            token_modifiers_bitset: modifiers(&[]),
        },
        // `World` enum member name
        SemanticToken {
            delta_line: 0,
            delta_start: 1,
            length: 5,
            token_type: token_type_index(SemanticTokenType::ENUM_MEMBER),
            token_modifiers_bitset: modifiers(&[]),
        },
        // == VARIABLE DECLARATION ==

        // `Foo` struct type name
        SemanticToken {
            delta_line: 1,
            delta_start: 4,
            length: 3,
            token_type: token_type_index(SemanticTokenType::STRUCT),
            token_modifiers_bitset: modifiers(&[]),
        },
        // Variable name
        SemanticToken {
            delta_line: 0,
            delta_start: 4,
            length: 1,
            token_type: token_type_index(SemanticTokenType::VARIABLE),
            token_modifiers_bitset: modifiers(&[]),
        },
        // =
        SemanticToken {
            delta_line: 0,
            delta_start: 2,
            length: 1,
            token_type: token_type_index(SemanticTokenType::OPERATOR),
            token_modifiers_bitset: modifiers(&[]),
        },
        // `Foo` struct type name
        SemanticToken {
            delta_line: 0,
            delta_start: 2,
            length: 3,
            token_type: token_type_index(SemanticTokenType::STRUCT),
            token_modifiers_bitset: modifiers(&[]),
        },
        // `bar` member name
        SemanticToken {
            delta_line: 1,
            delta_start: 8,
            length: 3,
            token_type: token_type_index(SemanticTokenType::PROPERTY),
            token_modifiers_bitset: modifiers(&[]),
        },
        // 1
        SemanticToken {
            delta_line: 0,
            delta_start: 5,
            length: 1,
            token_type: token_type_index(SemanticTokenType::NUMBER),
            token_modifiers_bitset: modifiers(&[]),
        },
        // == SCATTER STATEMENT ==

        // `scatter` keyword
        SemanticToken {
            delta_line: 3,
            delta_start: 4,
            length: 7,
            token_type: token_type_index(SemanticTokenType::KEYWORD),
            token_modifiers_bitset: modifiers(&[SemanticTokenModifier::ASYNC]),
        },
        // `in` keyword
        SemanticToken {
            delta_line: 0,
            delta_start: 17,
            length: 2,
            token_type: token_type_index(SemanticTokenType::KEYWORD),
            token_modifiers_bitset: modifiers(&[]),
        },
        // == TASK DEFINITION ==

        // `task` keyword
        SemanticToken {
            delta_line: 3,
            delta_start: 0,
            length: 4,
            token_type: token_type_index(SemanticTokenType::KEYWORD),
            token_modifiers_bitset: modifiers(&[]),
        },
        // task name
        SemanticToken {
            delta_line: 0,
            delta_start: 5,
            length: 9,
            token_type: token_type_index(SemanticTokenType::FUNCTION),
            token_modifiers_bitset: modifiers(&[SemanticTokenModifier::DEFINITION]),
        },
        // == META SECTION ==

        // `meta` keyword
        SemanticToken {
            delta_line: 1,
            delta_start: 4,
            length: 4,
            token_type: token_type_index(SemanticTokenType::KEYWORD),
            token_modifiers_bitset: modifiers(&[]),
        },
        // `description` property
        SemanticToken {
            delta_line: 1,
            delta_start: 8,
            length: 11,
            token_type: token_type_index(SemanticTokenType::PROPERTY),
            token_modifiers_bitset: modifiers(&[
                SemanticTokenModifier::READONLY,
                SemanticTokenModifier::STATIC,
            ]),
        },
        // `description` property value
        SemanticToken {
            delta_line: 0,
            delta_start: 13,
            length: 1,
            token_type: token_type_index(SemanticTokenType::STRING),
            token_modifiers_bitset: modifiers(&[]),
        },
        SemanticToken {
            delta_line: 0,
            delta_start: 1,
            length: 22,
            token_type: token_type_index(SemanticTokenType::STRING),
            token_modifiers_bitset: modifiers(&[]),
        },
        SemanticToken {
            delta_line: 0,
            delta_start: 22,
            length: 1,
            token_type: token_type_index(SemanticTokenType::STRING),
            token_modifiers_bitset: modifiers(&[]),
        },
        // == PARAMETER META SECTION ==

        // `parameter_meta` keyword
        SemanticToken {
            delta_line: 3,
            delta_start: 4,
            length: 14,
            token_type: token_type_index(SemanticTokenType::KEYWORD),
            token_modifiers_bitset: modifiers(&[]),
        },
        // `name` entry
        SemanticToken {
            delta_line: 1,
            delta_start: 8,
            length: 4,
            token_type: token_type_index(SemanticTokenType::PARAMETER),
            token_modifiers_bitset: modifiers(&[
                SemanticTokenModifier::READONLY,
                SemanticTokenModifier::STATIC,
            ]),
        },
        // `name` entry value
        SemanticToken {
            delta_line: 0,
            delta_start: 6,
            length: 1,
            token_type: token_type_index(SemanticTokenType::STRING),
            token_modifiers_bitset: modifiers(&[]),
        },
        SemanticToken {
            delta_line: 0,
            delta_start: 1,
            length: 17,
            token_type: token_type_index(SemanticTokenType::STRING),
            token_modifiers_bitset: modifiers(&[]),
        },
        SemanticToken {
            delta_line: 0,
            delta_start: 17,
            length: 1,
            token_type: token_type_index(SemanticTokenType::STRING),
            token_modifiers_bitset: modifiers(&[]),
        },
        // == INPUT SECTION ==

        // `input` keyword
        SemanticToken {
            delta_line: 3,
            delta_start: 4,
            length: 5,
            token_type: token_type_index(SemanticTokenType::KEYWORD),
            token_modifiers_bitset: modifiers(&[]),
        },
        // `String` type
        SemanticToken {
            delta_line: 1,
            delta_start: 8,
            length: 6,
            token_type: token_type_index(SemanticTokenType::TYPE),
            token_modifiers_bitset: modifiers(&[]),
        },
        // `name` parameter
        SemanticToken {
            delta_line: 0,
            delta_start: 7,
            length: 4,
            token_type: token_type_index(SemanticTokenType::PARAMETER),
            token_modifiers_bitset: modifiers(&[SemanticTokenModifier::READONLY]),
        },
        // == COMMAND SECTION ==

        // `command` keyword
        SemanticToken {
            delta_line: 3,
            delta_start: 4,
            length: 7,
            token_type: token_type_index(SemanticTokenType::KEYWORD),
            token_modifiers_bitset: modifiers(&[]),
        },
        // `name` variable in placeholder
        SemanticToken {
            delta_line: 1,
            delta_start: 23,
            length: 4,
            token_type: token_type_index(SemanticTokenType::PARAMETER),
            token_modifiers_bitset: modifiers(&[SemanticTokenModifier::READONLY]),
        },
        // == OUTPUT SECTION ==

        // `output` keyword
        SemanticToken {
            delta_line: 3,
            delta_start: 4,
            length: 6,
            token_type: token_type_index(SemanticTokenType::KEYWORD),
            token_modifiers_bitset: modifiers(&[]),
        },
        // `Int` type
        SemanticToken {
            delta_line: 1,
            delta_start: 8,
            length: 3,
            token_type: token_type_index(SemanticTokenType::TYPE),
            token_modifiers_bitset: modifiers(&[]),
        },
        // ?
        SemanticToken {
            delta_line: 0,
            delta_start: 3,
            length: 1,
            token_type: token_type_index(SemanticTokenType::OPERATOR),
            token_modifiers_bitset: modifiers(&[]),
        },
        // `return_code` output variable
        SemanticToken {
            delta_line: 0,
            delta_start: 2,
            length: 11,
            token_type: token_type_index(SemanticTokenType::VARIABLE),
            token_modifiers_bitset: modifiers(&[]),
        },
        // =
        SemanticToken {
            delta_line: 0,
            delta_start: 12,
            length: 1,
            token_type: token_type_index(SemanticTokenType::OPERATOR),
            token_modifiers_bitset: modifiers(&[]),
        },
        // `task` variable
        SemanticToken {
            delta_line: 0,
            delta_start: 2,
            length: 4,
            token_type: token_type_index(SemanticTokenType::VARIABLE),
            token_modifiers_bitset: modifiers(&[
                SemanticTokenModifier::DEFAULT_LIBRARY,
                SemanticTokenModifier::READONLY,
            ]),
        },
        // .
        SemanticToken {
            delta_line: 0,
            delta_start: 4,
            length: 1,
            token_type: token_type_index(SemanticTokenType::OPERATOR),
            token_modifiers_bitset: modifiers(&[]),
        },
        // `return_code` property
        SemanticToken {
            delta_line: 0,
            delta_start: 1,
            length: 11,
            token_type: token_type_index(SemanticTokenType::PROPERTY),
            token_modifiers_bitset: modifiers(&[
                SemanticTokenModifier::READONLY,
                SemanticTokenModifier::DEFAULT_LIBRARY,
            ]),
        },
    ];

    assert_eq!(tokens.data, expected_tokens);
}

#[tokio::test]
async fn should_mark_deprecated_items() {
    let mut ctx = setup().await;
    let result = semantic_tokens_full_request(&mut ctx, "deprecated.wdl")
        .await
        .expect("request should succeed")
        .unwrap();

    let SemanticTokensResult::Tokens(tokens) = result else {
        panic!("unexpected partial result");
    };

    let mut expected: Vec<SemanticToken> = vec![
        // The `runtime` keyword itself is deprecated
        SemanticToken {
            delta_line: 1,
            delta_start: 4,
            length: 7,
            token_type: token_type_index(SemanticTokenType::KEYWORD),
            token_modifiers_bitset: modifiers(&[SemanticTokenModifier::DEPRECATED]),
        },
        // The `docker` attribute is deprecated in `requirements` sections
        SemanticToken {
            delta_line: 1,
            delta_start: 8,
            length: 6,
            token_type: token_type_index(SemanticTokenType::PROPERTY),
            token_modifiers_bitset: modifiers(&[
                SemanticTokenModifier::DEPRECATED,
                SemanticTokenModifier::READONLY,
            ]),
        },
    ];

    for token in tokens.data {
        let matched = expected.iter().position(|expected| *expected == token);

        if let Some(index) = matched {
            expected.remove(index);
        }
    }

    assert!(
        expected.is_empty(),
        "some expected items were not returned: {expected:?}"
    );
}

#[tokio::test]
async fn should_not_support_full_delta_requests() {
    let mut ctx = setup().await;
    match semantic_tokens_full_delta_request(&mut ctx, "source.wdl")
        .await
        .expect_err("request should fail")
    {
        async_lsp::Error::Response(err) if err.code == ErrorCode::METHOD_NOT_FOUND => {}
        e => panic!("unexpected error type: {e}"),
    }
}

#[tokio::test]
async fn should_not_support_range_requests() {
    let mut ctx = setup().await;
    match semantic_tokens_range_request(&mut ctx, "source.wdl")
        .await
        .expect_err("request should fail")
    {
        async_lsp::Error::Response(err) if err.code == ErrorCode::METHOD_NOT_FOUND => {}
        e => panic!("unexpected error type: {e}"),
    }
}
