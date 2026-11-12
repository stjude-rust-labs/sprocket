//! Integration tests for the `textDocument/documentLink` request.

use async_lsp::lsp_types::DocumentLink;
use async_lsp::lsp_types::DocumentLinkParams;
use async_lsp::lsp_types::Position;
use async_lsp::lsp_types::Range;
use async_lsp::lsp_types::TextDocumentIdentifier;
use async_lsp::lsp_types::request::DocumentLinkRequest;
use url::Url;

use crate::common::TestContext;
use crate::common::TestContextBuilder;

fn verify_links(mut expected: Vec<(Range, &str)>, received: &[DocumentLink]) {
    for item in received {
        let matched = expected.iter().position(|(range, expected_uri)| {
            item.range == *range
                && item
                    .target
                    .as_ref()
                    .is_some_and(|uri| uri.as_str() == *expected_uri)
        });

        if let Some(index) = matched {
            expected.remove(index);
        } else {
            panic!("unexpected document link item returned: {item:?}");
        }
    }

    assert!(
        expected.is_empty(),
        "some expected items were not returned: {expected:?}"
    );
}

async fn setup() -> TestContext {
    let mut ctx = TestContextBuilder::new("document_link")
        .setup_modules()
        .build();
    ctx.initialize().await;
    ctx
}

#[tokio::test]
async fn should_collect_document_links() {
    let ctx = setup().await;

    let results = ctx
        .request::<DocumentLinkRequest>(DocumentLinkParams {
            text_document: TextDocumentIdentifier {
                uri: ctx.doc_uri("source.wdl"),
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
        .await
        .unwrap();

    let Some(results) = results else {
        panic!("should produce results");
    };

    verify_links(vec![
        (Range {
            start: Position { line: 2, character: 8 },
            end: Position { line: 2, character: 124 },
        }, "https://raw.githubusercontent.com/stjudecloud/workflows/d45a4c2a6d954b404db818f023b5a0453ac86fd0/tools/librarian.wdl"),
        (Range {
            start: Position { line: 3, character: 8 },
            end: Position { line: 3, character: 15 },
        }, ctx.doc_uri("foo.wdl").as_str()),
        (Range {
            start: Position { line: 4, character: 8 },
            end: Position { line: 4, character: 17 },
        }, ctx.doc_uri("foo.wdl").as_str()),
        (Range {
            start: Position { line: 5, character: 27 },
            end: Position { line: 5, character: 34 },
        }, ctx.doc_uri("foo.wdl").as_str()),
        (Range {
            start: Position { line: 6, character: 7 },
            end: Position { line: 6, character: 13 },
        },
         // Canonicalized path is needed for macOS and Windows
         Url::from_file_path(ctx.doc_path("module/index.wdl").canonicalize().unwrap()).unwrap().as_str())
    ],
    &results);
}
