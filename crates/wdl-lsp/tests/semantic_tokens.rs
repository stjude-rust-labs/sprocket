//! Integration tests for the `textDocument/semanticTokens` request.

use pretty_assertions::assert_eq;
use tower_lsp::lsp_types::SemanticToken;
use tower_lsp::lsp_types::SemanticTokenModifier;
use tower_lsp::lsp_types::SemanticTokenType;
use tower_lsp::lsp_types::SemanticTokensDeltaParams;
use tower_lsp::lsp_types::SemanticTokensFullDeltaResult;
use tower_lsp::lsp_types::SemanticTokensParams;
use tower_lsp::lsp_types::SemanticTokensRangeParams;
use tower_lsp::lsp_types::SemanticTokensRangeResult;
use tower_lsp::lsp_types::SemanticTokensResult;
use tower_lsp::lsp_types::TextDocumentIdentifier;
use tower_lsp::lsp_types::request::SemanticTokensFullDeltaRequest;
use tower_lsp::lsp_types::request::SemanticTokensFullRequest;
use tower_lsp::lsp_types::request::SemanticTokensRangeRequest;
use wdl_analysis::handlers::WDL_SEMANTIC_TOKEN_MODIFIERS;
use wdl_analysis::handlers::WDL_SEMANTIC_TOKEN_TYPES;

use crate::common::TestContext;

mod common;

async fn semantic_tokens_full_request(
    ctx: &mut TestContext,
    path: &str,
) -> Option<SemanticTokensResult> {
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
) -> Option<SemanticTokensFullDeltaResult> {
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
) -> Option<SemanticTokensRangeResult> {
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
        if let Some(pos) = WDL_SEMANTIC_TOKEN_MODIFIERS
            .iter()
            .position(|m| m == modifier)
        {
            acc |= 1 << pos;
        }

        acc
    })
}

#[tokio::test]
async fn should_provide_semantic_tokens() {
    let mut ctx = setup().await;
    let result = semantic_tokens_full_request(&mut ctx, "source.wdl")
        .await
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
            token_modifiers_bitset: 0,
        },
        // == VERSION ==

        // Version keyword
        SemanticToken {
            delta_line: 2,
            delta_start: 0,
            length: 7,
            token_type: token_type_index(SemanticTokenType::KEYWORD),
            token_modifiers_bitset: 0,
        },
        // Version number
        SemanticToken {
            delta_line: 0,
            delta_start: 8,
            length: 3,
            token_type: token_type_index(SemanticTokenType::NUMBER),
            token_modifiers_bitset: 0,
        },
        // == IMPORT ==

        // Import keyword
        SemanticToken {
            delta_line: 2,
            delta_start: 0,
            length: 6,
            token_type: token_type_index(SemanticTokenType::KEYWORD),
            token_modifiers_bitset: 0,
        },
        // Import path
        SemanticToken {
            delta_line: 0,
            delta_start: 7,
            length: 1,
            token_type: token_type_index(SemanticTokenType::STRING),
            token_modifiers_bitset: 0,
        },
        SemanticToken {
            delta_line: 0,
            delta_start: 1,
            length: 7,
            token_type: token_type_index(SemanticTokenType::STRING),
            token_modifiers_bitset: 0,
        },
        SemanticToken {
            delta_line: 0,
            delta_start: 7,
            length: 1,
            token_type: token_type_index(SemanticTokenType::STRING),
            token_modifiers_bitset: 0,
        },
        // == STRUCT ==

        // Struct keyword
        SemanticToken {
            delta_line: 2,
            delta_start: 0,
            length: 6,
            token_type: token_type_index(SemanticTokenType::KEYWORD),
            token_modifiers_bitset: 0,
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
            token_modifiers_bitset: 0,
        },
        // Member name
        SemanticToken {
            delta_line: 0,
            delta_start: 4,
            length: 3,
            token_type: 1,
            token_modifiers_bitset: 0,
        },
        // == ENUM ==

        // Enum keyword
        SemanticToken {
            delta_line: 3,
            delta_start: 0,
            length: 4,
            token_type: token_type_index(SemanticTokenType::KEYWORD),
            token_modifiers_bitset: 0,
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
            token_modifiers_bitset: 0,
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
            token_modifiers_bitset: 0,
        },
        // == OUTPUT ==

        // Output keyword
        SemanticToken {
            delta_line: 2,
            delta_start: 4,
            length: 6,
            token_type: token_type_index(SemanticTokenType::KEYWORD),
            token_modifiers_bitset: 0,
        },
        // Array type
        SemanticToken {
            delta_line: 1,
            delta_start: 8,
            length: 5,
            token_type: token_type_index(SemanticTokenType::TYPE),
            token_modifiers_bitset: 0,
        },
        // File type
        SemanticToken {
            delta_line: 0,
            delta_start: 6,
            length: 4,
            token_type: token_type_index(SemanticTokenType::TYPE),
            token_modifiers_bitset: 0,
        },
        // `files` variable name
        SemanticToken {
            delta_line: 0,
            delta_start: 6,
            length: 5,
            token_type: token_type_index(SemanticTokenType::VARIABLE),
            token_modifiers_bitset: 0,
        },
        // =
        SemanticToken {
            delta_line: 0,
            delta_start: 6,
            length: 1,
            token_type: token_type_index(SemanticTokenType::OPERATOR),
            token_modifiers_bitset: 0,
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
            token_modifiers_bitset: 0,
        },
        // *
        SemanticToken {
            delta_line: 0,
            delta_start: 1,
            length: 1,
            token_type: token_type_index(SemanticTokenType::STRING),
            token_modifiers_bitset: 0,
        },
        // "
        SemanticToken {
            delta_line: 0,
            delta_start: 1,
            length: 1,
            token_type: token_type_index(SemanticTokenType::STRING),
            token_modifiers_bitset: 0,
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
            token_modifiers_bitset: 0,
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
            token_modifiers_bitset: 0,
        },
        // `do_work` task name
        SemanticToken {
            delta_line: 0,
            delta_start: 5,
            length: 7,
            token_type: token_type_index(SemanticTokenType::FUNCTION),
            token_modifiers_bitset: 0,
        },
        // == NAMESPACED CALL ==

        // Call keyword
        SemanticToken {
            delta_line: 2,
            delta_start: 4,
            length: 4,
            token_type: token_type_index(SemanticTokenType::KEYWORD),
            token_modifiers_bitset: 0,
        },
        // `foo` namespace
        SemanticToken {
            delta_line: 0,
            delta_start: 5,
            length: 3,
            token_type: token_type_index(SemanticTokenType::NAMESPACE),
            token_modifiers_bitset: 0,
        },
        // .
        SemanticToken {
            delta_line: 0,
            delta_start: 3,
            length: 1,
            token_type: token_type_index(SemanticTokenType::OPERATOR),
            token_modifiers_bitset: 0,
        },
        // `bar` task name
        SemanticToken {
            delta_line: 0,
            delta_start: 1,
            length: 3,
            token_type: token_type_index(SemanticTokenType::FUNCTION),
            token_modifiers_bitset: 0,
        },
        // == VARIABLE DECLARATION ==

        // `Hello` enum type name
        SemanticToken {
            delta_line: 2,
            delta_start: 4,
            length: 5,
            token_type: token_type_index(SemanticTokenType::ENUM),
            token_modifiers_bitset: 0,
        },
        // Variable name
        SemanticToken {
            delta_line: 0,
            delta_start: 6,
            length: 1,
            token_type: 1,
            token_modifiers_bitset: 0,
        },
        // =
        SemanticToken {
            delta_line: 0,
            delta_start: 2,
            length: 1,
            token_type: token_type_index(SemanticTokenType::OPERATOR),
            token_modifiers_bitset: 0,
        },
        // `Hello` enum name
        SemanticToken {
            delta_line: 0,
            delta_start: 2,
            length: 5,
            token_type: token_type_index(SemanticTokenType::ENUM),
            token_modifiers_bitset: 0,
        },
        // .
        SemanticToken {
            delta_line: 0,
            delta_start: 5,
            length: 1,
            token_type: token_type_index(SemanticTokenType::OPERATOR),
            token_modifiers_bitset: 0,
        },
        // `World` enum member name
        SemanticToken {
            delta_line: 0,
            delta_start: 1,
            length: 5,
            token_type: token_type_index(SemanticTokenType::ENUM_MEMBER),
            token_modifiers_bitset: 0,
        },
        // == VARIABLE DECLARATION ==

        // `Foo` struct type name
        SemanticToken {
            delta_line: 1,
            delta_start: 4,
            length: 3,
            token_type: 5,
            token_modifiers_bitset: 0,
        },
        // Variable name
        SemanticToken {
            delta_line: 0,
            delta_start: 4,
            length: 1,
            token_type: 1,
            token_modifiers_bitset: 0,
        },
        // =
        SemanticToken {
            delta_line: 0,
            delta_start: 2,
            length: 1,
            token_type: token_type_index(SemanticTokenType::OPERATOR),
            token_modifiers_bitset: 0,
        },
        // `Foo` struct type name
        SemanticToken {
            delta_line: 0,
            delta_start: 2,
            length: 3,
            token_type: token_type_index(SemanticTokenType::STRUCT),
            token_modifiers_bitset: 0,
        },
        // `bar` member name
        SemanticToken {
            delta_line: 1,
            delta_start: 8,
            length: 3,
            token_type: token_type_index(SemanticTokenType::PROPERTY),
            token_modifiers_bitset: 0,
        },
        // :
        SemanticToken {
            delta_line: 0,
            delta_start: 3,
            length: 1,
            token_type: token_type_index(SemanticTokenType::OPERATOR),
            token_modifiers_bitset: 0,
        },
        // 1
        SemanticToken {
            delta_line: 0,
            delta_start: 2,
            length: 1,
            token_type: token_type_index(SemanticTokenType::NUMBER),
            token_modifiers_bitset: 0,
        },
    ];

    assert_eq!(tokens.data, expected_tokens);
}

#[tokio::test]
#[should_panic(expected = "MethodNotFound")]
async fn should_not_support_full_delta_requests() {
    let mut ctx = setup().await;
    let _result = semantic_tokens_full_delta_request(&mut ctx, "source.wdl").await;
}

#[tokio::test]
#[should_panic(expected = "MethodNotFound")]
async fn should_not_support_range_requests() {
    let mut ctx = setup().await;
    let _result = semantic_tokens_range_request(&mut ctx, "source.wdl").await;
}
