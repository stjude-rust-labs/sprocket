//! Integration tests for the `textDocument/inlayHint` request.

use async_lsp::lsp_types::InlayHint;
use async_lsp::lsp_types::InlayHintLabel;
use async_lsp::lsp_types::InlayHintParams;
use async_lsp::lsp_types::Position;
use async_lsp::lsp_types::Range;
use async_lsp::lsp_types::TextDocumentIdentifier;
use async_lsp::lsp_types::request::InlayHintRequest;

use crate::common::TestContext;
use crate::common::TestContextBuilder;

async fn inlay_hint_request(
    ctx: &mut TestContext,
    path: &str,
    range: Range,
) -> async_lsp::Result<Option<Vec<InlayHint>>> {
    ctx.request::<InlayHintRequest>(InlayHintParams {
        text_document: TextDocumentIdentifier {
            uri: ctx.doc_uri(path),
        },
        range,
        work_done_progress_params: Default::default(),
    })
    .await
}

async fn setup() -> TestContext {
    let mut ctx = TestContextBuilder::new("inlay_hints").build();
    ctx.initialize().await;
    ctx
}

fn full_range() -> Range {
    Range {
        start: Position::new(0, 0),
        end: Position::new(1000, 0),
    }
}

fn label(hint: &InlayHint) -> &str {
    match &hint.label {
        InlayHintLabel::String(label) => label,
        InlayHintLabel::LabelParts(_) => panic!("unexpected label parts in hint: {hint:?}"),
    }
}

#[tokio::test]
async fn should_hint_scatter_variable_types() {
    let mut ctx = setup().await;
    let Some(hints) = inlay_hint_request(&mut ctx, "source.wdl", full_range())
        .await
        .expect("request should succeed")
    else {
        panic!("response should contain hints");
    };

    let mut expected = vec![
        (8, 14, ": Int"),
        (13, 18, ": Int"),
        (18, 16, ": Array[Int]"),
        (19, 21, ": Int"),
        (24, 18, ": String"),
    ];

    for hint in hints {
        let matched = expected.iter().position(|(line, character, expected_label)| {
            hint.position.line == *line
                && hint.position.character == *character
                && label(&hint) == *expected_label
        });

        match matched {
            Some(index) => {
                expected.remove(index);
                // Scatter variables have no type annotation syntax, so the
                // hint must not offer a text edit.
                assert!(
                    hint.text_edits.is_none(),
                    "scatter hint should be display-only: {hint:?}"
                );
                assert_eq!(
                    hint.kind,
                    Some(async_lsp::lsp_types::InlayHintKind::TYPE),
                    "scatter hint should be a type hint: {hint:?}"
                );
            }
            None => panic!("unexpected inlay hint returned: {hint:?}"),
        }
    }

    assert!(
        expected.is_empty(),
        "some expected hints were not returned: {expected:?}"
    );
}

#[tokio::test]
async fn should_only_hint_within_the_requested_range() {
    let mut ctx = setup().await;
    let range = Range {
        start: Position::new(8, 0),
        end: Position::new(9, 0),
    };
    let Some(hints) = inlay_hint_request(&mut ctx, "source.wdl", range)
        .await
        .expect("request should succeed")
    else {
        panic!("response should contain hints");
    };

    assert_eq!(
        hints.len(),
        1,
        "only the scatter on line 8 should be hinted: {hints:?}"
    );
    assert_eq!(hints[0].position, Position::new(8, 14));
    assert_eq!(label(&hints[0]), ": Int");
}
