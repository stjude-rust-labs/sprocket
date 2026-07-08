//! Integration tests for the `textDocument/codeLens` request.

pub mod common;

use async_lsp::lsp_types::CodeLens;
use async_lsp::lsp_types::CodeLensParams;
use async_lsp::lsp_types::Command;
use async_lsp::lsp_types::Position;
use async_lsp::lsp_types::Range;
use async_lsp::lsp_types::TextDocumentIdentifier;
use async_lsp::lsp_types::request::CodeLensRequest;
use common::TestContext;

async fn code_lens_request(
    ctx: &mut TestContext,
    path: &str,
) -> async_lsp::Result<Option<Vec<CodeLens>>> {
    ctx.request::<CodeLensRequest>(CodeLensParams {
        text_document: TextDocumentIdentifier {
            uri: ctx.doc_uri(path),
        },
        work_done_progress_params: Default::default(),
        partial_result_params: Default::default(),
    })
    .await
}

async fn setup() -> TestContext {
    let mut ctx = TestContext::new("code_lens");
    ctx.initialize().await;
    ctx
}

fn assert_code_lenses(response: Vec<CodeLens>, mut expected: Vec<CodeLens>) {
    for actual in response {
        let matched = expected.iter().position(|expected| {
            expected.range == actual.range
                && expected.command == actual.command
                && expected.data == actual.data
        });

        if let Some(index) = matched {
            expected.remove(index);
        } else {
            panic!("unexpected code lens returned: {actual:?}");
        }
    }

    assert!(
        expected.is_empty(),
        "some expected items were not returned: {expected:?}"
    );
}

#[tokio::test]
async fn should_generate_code_lenses_for_wdl_targets() {
    let mut ctx = setup().await;
    let Some(response) = code_lens_request(&mut ctx, "example.wdl")
        .await
        .expect("request should succeed")
    else {
        panic!("response should contain entries");
    };

    let expected = vec![
        CodeLens {
            range: Range {
                start: Position {
                    line: 2,
                    character: 5,
                },
                end: Position {
                    line: 2,
                    character: 14,
                },
            },
            command: Some(Command {
                title: "Run 'say_hello'".to_string(),
                command: String::from("sprocket.run"),
                arguments: Some(vec![
                    ctx.doc_uri("example.wdl").to_string().into(),
                    "say_hello".into(),
                ]),
            }),
            data: None,
        },
        CodeLens {
            range: Range {
                start: Position {
                    line: 8,
                    character: 9,
                },
                end: Position {
                    line: 8,
                    character: 23,
                },
            },
            command: Some(Command {
                title: "Run 'wrap_say_hello'".to_string(),
                command: String::from("sprocket.run"),
                arguments: Some(vec![
                    ctx.doc_uri("example.wdl").to_string().into(),
                    "wrap_say_hello".into(),
                ]),
            }),
            data: None,
        },
    ];

    assert_code_lenses(response, expected);
}

#[tokio::test]
async fn should_ignore_wdl_targets_with_inputs() {
    let mut ctx = setup().await;
    let response = code_lens_request(&mut ctx, "example2.wdl")
        .await
        .expect("request should succeed");
    assert!(response.is_none(), "response should be empty: {response:?}");
}
